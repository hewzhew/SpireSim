use std::collections::HashSet;
use std::time::Instant;

use crate::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper};

use super::super::super::*;
use super::legal_action_choices;
use super::partial_frontier::{partial_frontier_width, select_partial_frontier};
use super::plan::{plan_from_node, root_eval, stop_reason_for_transition};
use super::ranking::count_turn_plan_prior_scored_plans;
use super::selection::{bucket_counts, first_action_summaries, select_bucketed_plans};
use super::work::TurnPlanWorkNode;
use crate::ai::combat_search_v2::turn_plan_probe::{
    CombatSearchV2TurnPlanPartialRetentionAuditV1, CombatSearchV2TurnPlanProbeGuideRankV1,
};
use crate::ai::combat_search_v2::turn_planner::types::{
    TurnPlanEnumeration, TurnPlanStepStateV1, TurnPlanStopReason, TurnPlanV1, TurnPlannerConfigV1,
};
use crate::ai::combat_search_v2::value::CombatEvalV2;

/// Exact same-turn enumeration split at action-depth boundaries.
///
/// The ordinary one-shot planner drives this session with its maintained
/// heuristic selector. A downstream interactive owner may later pause after
/// `expand_next_layer`, project the pending states, and choose the retained
/// ordinals without reimplementing exact transitions or deduplication.
pub(in crate::ai::combat_search_v2) struct TurnPlanPartialCandidateV1 {
    pub(in crate::ai::combat_search_v2) position: CombatPosition,
    pub(in crate::ai::combat_search_v2) inputs: Vec<ClientInput>,
}

pub(in crate::ai::combat_search_v2) struct TurnPlanPartialLayerV1 {
    pub(in crate::ai::combat_search_v2) atomic_depth: usize,
    pub(in crate::ai::combat_search_v2) max_retained_states: usize,
    pub(in crate::ai::combat_search_v2) candidates: Vec<TurnPlanPartialCandidateV1>,
}

#[derive(Clone)]
pub(in crate::ai::combat_search_v2) struct TurnPlanLayerSessionV1 {
    root: SearchNode,
    config: TurnPlannerConfigV1,
    continue_pending_choices: bool,
    root_action_len: usize,
    root_eval: CombatEvalV2,
    seen: HashSet<CombatExactStateKey>,
    frontier: Vec<TurnPlanWorkNode>,
    pending_next: Option<Vec<TurnPlanWorkNode>>,
    candidates: Vec<TurnPlanV1>,
    enumeration: TurnPlanEnumeration,
}

impl TurnPlanLayerSessionV1 {
    pub(in crate::ai::combat_search_v2) fn new(
        root: &SearchNode,
        config: &TurnPlannerConfigV1,
        continue_pending_choices: bool,
    ) -> Self {
        let mut seen = HashSet::new();
        seen.insert(combat_exact_state_key(&root.engine, &root.combat));
        let root_is_supported_turn_boundary = matches!(root.engine, EngineState::CombatPlayerTurn)
            || (continue_pending_choices && matches!(root.engine, EngineState::PendingChoice(_)));
        let frontier = if root_is_supported_turn_boundary {
            {
                vec![TurnPlanWorkNode {
                    node: root.clone(),
                    action_facts: Vec::new(),
                    step_states: Vec::new(),
                    cached_eligible_actions: None,
                }]
            }
        } else {
            Default::default()
        };
        Self {
            root: root.clone(),
            config: config.clone(),
            continue_pending_choices,
            root_action_len: root.actions.len(),
            root_eval: root_eval(root),
            seen,
            frontier,
            pending_next: None,
            candidates: Vec::new(),
            enumeration: TurnPlanEnumeration::default(),
        }
    }

    /// Expands every currently retained state by one exact action depth and
    /// pauses before choosing the next retained frontier.
    pub(in crate::ai::combat_search_v2) fn expand_next_layer(
        &mut self,
        stepper: &impl CombatStepper,
        deadline: Option<Instant>,
    ) -> bool {
        assert!(
            self.pending_next.is_none(),
            "retain the pending turn-plan layer before expanding another"
        );
        if self.frontier.is_empty()
            || self.enumeration.nodes_expanded >= self.config.max_inner_nodes
            || deadline.is_some_and(|limit| Instant::now() >= limit)
        {
            return false;
        }

        let mut next = Vec::new();
        for work in std::mem::take(&mut self.frontier) {
            if self.enumeration.nodes_expanded >= self.config.max_inner_nodes
                || deadline.is_some_and(|limit| Instant::now() >= limit)
            {
                break;
            }
            let TurnPlanWorkNode {
                node,
                action_facts,
                step_states,
                cached_eligible_actions,
            } = work;

            if terminal_label(&node.engine, &node.combat) != SearchTerminalLabel::Unresolved {
                self.candidates.push(plan_from_node(
                    TurnPlanWorkNode {
                        node,
                        action_facts,
                        step_states,
                        cached_eligible_actions,
                    },
                    self.root_action_len,
                    TurnPlanStopReason::Terminal,
                    self.root_eval,
                ));
                continue;
            }

            let position = CombatPosition::new(node.engine.clone(), node.combat.clone());
            let legal = cached_eligible_actions.unwrap_or_else(|| {
                eligible_legal_actions(
                    stepper,
                    &position,
                    self.continue_pending_choices,
                    &self.config,
                    node.potions_used,
                )
            });
            if legal.is_empty() {
                self.candidates.push(plan_from_node(
                    TurnPlanWorkNode {
                        node,
                        action_facts,
                        step_states,
                        cached_eligible_actions: Some(Vec::new()),
                    },
                    self.root_action_len,
                    TurnPlanStopReason::NoLegalActions,
                    self.root_eval,
                ));
                continue;
            }

            self.enumeration.nodes_expanded = self.enumeration.nodes_expanded.saturating_add(1);
            let equivalence = compress_equivalent_actions(&node.engine, &node.combat, legal);
            let ordered =
                order_indexed_action_choices(&node.engine, &node.combat, equivalence.choices);
            let before_step_trace = self.config.capture_step_trace.then(|| {
                (
                    combat_exact_state_hash_v2(&node.engine, &node.combat),
                    summarize_state(&node.engine, &node.combat),
                )
            });
            for ordered_choice in ordered.choices {
                if deadline.is_some_and(|limit| Instant::now() >= limit) {
                    break;
                }

                let step = stepper.apply_to_stable(
                    &position,
                    ordered_choice.choice.input.clone(),
                    CombatStepLimits {
                        max_engine_steps: self.config.max_engine_steps_per_action,
                        deadline,
                    },
                );
                self.enumeration.nodes_generated =
                    self.enumeration.nodes_generated.saturating_add(1);
                self.enumeration.engine_steps = self
                    .enumeration
                    .engine_steps
                    .saturating_add(step.engine_steps);

                let mut child_action_facts = action_facts.clone();
                child_action_facts.push(summarize_action_facts_from_step(
                    &node.engine,
                    &node.combat,
                    &ordered_choice.choice.input,
                    &step,
                ));
                let truncated = step.truncated;
                let CombatPosition {
                    engine: child_engine,
                    combat: child_combat,
                } = step.position;
                let mut child = node.clone_for_child(child_engine, child_combat);
                let mut child_step_states = step_states.clone();
                if let Some((before_exact_state_hash, state_before)) = before_step_trace.as_ref() {
                    child_step_states.push(TurnPlanStepStateV1 {
                        before_exact_state_hash: before_exact_state_hash.clone(),
                        before: state_before.clone(),
                        after_exact_state_hash: combat_exact_state_hash_v2(
                            &child.engine,
                            &child.combat,
                        ),
                        after: summarize_state(&child.engine, &child.combat),
                        engine_steps: step.engine_steps,
                    });
                }
                let transition = classify_turn_branch_transition(
                    &node.engine,
                    &node.combat,
                    &ordered_choice.choice.input,
                    &child.engine,
                    &child.combat,
                );
                child.note_turn_prefix(&node.combat, &ordered_choice.choice.input, transition);
                child.note_input(&ordered_choice.choice.input);
                child.note_turn_branch_priority(transition.frontier_priority_hint());
                child.push_action(CombatSearchV2ActionTrace {
                    step_index: node.actions.len(),
                    action_id: ordered_choice.original_action_id,
                    action_key: ordered_choice.choice.action_key,
                    action_debug: ordered_choice.choice.action_debug,
                    input: ordered_choice.choice.input,
                });

                if truncated {
                    self.enumeration.truncated_children =
                        self.enumeration.truncated_children.saturating_add(1);
                    self.candidates.push(plan_from_node(
                        TurnPlanWorkNode {
                            node: child,
                            action_facts: child_action_facts,
                            step_states: child_step_states,
                            cached_eligible_actions: Some(Vec::new()),
                        },
                        self.root_action_len,
                        TurnPlanStopReason::EngineStepLimit,
                        self.root_eval,
                    ));
                } else if transition.is_same_turn()
                    || (self.continue_pending_choices
                        && child.combat.turn.turn_count == self.root.combat.turn.turn_count
                        && matches!(
                            child.engine,
                            EngineState::CombatPlayerTurn | EngineState::PendingChoice(_)
                        ))
                {
                    let key = combat_exact_state_key(&child.engine, &child.combat);
                    if self.seen.insert(key) {
                        let child_position =
                            CombatPosition::new(child.engine.clone(), child.combat.clone());
                        let cached_eligible_actions = eligible_legal_actions(
                            stepper,
                            &child_position,
                            self.continue_pending_choices,
                            &self.config,
                            child.potions_used,
                        );
                        next.push(TurnPlanWorkNode {
                            node: child,
                            action_facts: child_action_facts,
                            step_states: child_step_states,
                            cached_eligible_actions: Some(cached_eligible_actions),
                        });
                    } else {
                        self.enumeration.exact_state_skips =
                            self.enumeration.exact_state_skips.saturating_add(1);
                    }
                } else {
                    self.candidates.push(plan_from_node(
                        TurnPlanWorkNode {
                            node: child,
                            action_facts: child_action_facts,
                            step_states: child_step_states,
                            cached_eligible_actions: Some(Vec::new()),
                        },
                        self.root_action_len,
                        stop_reason_for_transition(transition),
                        self.root_eval,
                    ));
                }
            }
        }
        self.pending_next = Some(next);
        true
    }

    pub(in crate::ai::combat_search_v2) fn pending_layer(&self) -> Option<TurnPlanPartialLayerV1> {
        let pending = self.pending_next.as_ref()?;
        let candidates = pending
            .iter()
            .map(|work| TurnPlanPartialCandidateV1 {
                position: CombatPosition::new(work.node.engine.clone(), work.node.combat.clone()),
                inputs: work.node.actions[self.root_action_len..]
                    .iter()
                    .map(|action| action.input.clone())
                    .collect(),
            })
            .collect::<Vec<_>>();
        let atomic_depth = candidates
            .first()
            .map(|candidate| candidate.inputs.len())
            .unwrap_or_default();
        Some(TurnPlanPartialLayerV1 {
            atomic_depth,
            max_retained_states: partial_frontier_width(&self.config),
            candidates,
        })
    }

    pub(in crate::ai::combat_search_v2) fn retain_pending_ordinals(
        &mut self,
        ordinals: &[usize],
    ) -> Result<(), String> {
        let (reserved, _) = self.take_pending_partition(ordinals)?;
        self.frontier = reserved;
        Ok(())
    }

    /// Reserves caller-selected ordinals, then uses the maintained mechanical
    /// selector only to fill otherwise unused frontier capacity. Reserved
    /// states are never displaced or re-ranked by the heuristic fill.
    pub(in crate::ai::combat_search_v2) fn retain_pending_ordinals_with_heuristic_fill(
        &mut self,
        ordinals: &[usize],
    ) -> Result<(), String> {
        let max_retained_states = partial_frontier_width(&self.config);
        let (mut reserved, remaining) = self.take_pending_partition(ordinals)?;
        let fill_width = max_retained_states.saturating_sub(reserved.len());
        if fill_width > 0 {
            let mut fill_config = self.config.clone();
            fill_config.max_partial_frontier_states = Some(fill_width);
            let (fill, _) =
                select_partial_frontier(remaining, &fill_config, self.root_action_len, None, false);
            reserved.extend(fill);
        }
        self.frontier = reserved;
        Ok(())
    }

    fn take_pending_partition(
        &mut self,
        ordinals: &[usize],
    ) -> Result<(Vec<TurnPlanWorkNode>, Vec<TurnPlanWorkNode>), String> {
        let pending = self
            .pending_next
            .as_ref()
            .ok_or_else(|| "expand a turn-plan layer before retaining ordinals".to_owned())?;
        let max_retained_states = partial_frontier_width(&self.config);
        if ordinals.len() > max_retained_states {
            return Err(format!(
                "partial frontier retains at most {max_retained_states} states, got {}",
                ordinals.len()
            ));
        }
        let mut seen_ordinals = HashSet::new();
        if let Some(ordinal) = ordinals
            .iter()
            .copied()
            .find(|ordinal| *ordinal >= pending.len() || !seen_ordinals.insert(*ordinal))
        {
            return Err(format!(
                "partial frontier ordinal {ordinal} is duplicate or outside {} candidates",
                pending.len()
            ));
        }
        let pending = self
            .pending_next
            .take()
            .expect("pending layer existed after ordinal validation");
        let mut slots = pending.into_iter().map(Some).collect::<Vec<_>>();
        let reserved = ordinals
            .iter()
            .filter_map(|ordinal| slots[*ordinal].take())
            .collect();
        let remaining = slots.into_iter().flatten().collect();
        Ok((reserved, remaining))
    }

    pub(in crate::ai::combat_search_v2) fn retain_pending_with_guides(
        &mut self,
        partial_state_guides: Option<
            &dyn Fn(&CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
        >,
    ) {
        let _ = self.retain_pending_with_guides_impl(partial_state_guides, false);
    }

    pub(in crate::ai::combat_search_v2) fn retain_pending_with_guides_audited(
        &mut self,
        partial_state_guides: Option<
            &dyn Fn(&CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
        >,
    ) -> CombatSearchV2TurnPlanPartialRetentionAuditV1 {
        self.retain_pending_with_guides_impl(partial_state_guides, true)
            .expect("audited partial retention returns its selector report")
    }

    fn retain_pending_with_guides_impl(
        &mut self,
        partial_state_guides: Option<
            &dyn Fn(&CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
        >,
        capture_audit: bool,
    ) -> Option<CombatSearchV2TurnPlanPartialRetentionAuditV1> {
        let next = self
            .pending_next
            .take()
            .expect("expand a turn-plan layer before retaining it");
        let (frontier, audit) = select_partial_frontier(
            next,
            &self.config,
            self.root_action_len,
            partial_state_guides,
            capture_audit,
        );
        self.frontier = frontier;
        audit
    }

    pub(in crate::ai::combat_search_v2) fn is_ready_to_finish(&self) -> bool {
        self.pending_next.is_none()
            && (self.frontier.is_empty()
                || self.enumeration.nodes_expanded >= self.config.max_inner_nodes)
    }

    pub(in crate::ai::combat_search_v2) fn nodes_expanded(&self) -> usize {
        self.enumeration.nodes_expanded
    }

    pub(in crate::ai::combat_search_v2) fn nodes_generated(&self) -> usize {
        self.enumeration.nodes_generated
    }

    pub(in crate::ai::combat_search_v2) fn finish(mut self) -> TurnPlanEnumeration {
        assert!(
            self.pending_next.is_none(),
            "retain the pending turn-plan layer before finishing"
        );
        self.enumeration.preselection_plan_count = self.candidates.len();
        self.enumeration.preselection_first_action_summaries =
            first_action_summaries(&self.candidates);
        self.enumeration.preselection_first_actions = self
            .enumeration
            .preselection_first_action_summaries
            .iter()
            .map(|summary| summary.action.clone())
            .collect();
        self.enumeration.preselection_bucket_counts = bucket_counts(&self.candidates);
        let prior_state_hash = self
            .config
            .turn_plan_prior
            .as_ref()
            .filter(|prior| !prior.is_empty())
            .map(|_| combat_exact_state_hash_v2(&self.root.engine, &self.root.combat));
        self.enumeration.turn_plan_prior_scored_plans = count_turn_plan_prior_scored_plans(
            &self.candidates,
            prior_state_hash.as_deref(),
            self.config.turn_plan_prior.as_ref(),
        );
        let (selected_plans, selection_audit) =
            select_bucketed_plans(self.candidates, &self.config, prior_state_hash.as_deref());
        self.enumeration.plans = selected_plans;
        self.enumeration.selection_audit = selection_audit;
        self.enumeration
    }
}

fn eligible_legal_actions(
    stepper: &impl CombatStepper,
    position: &CombatPosition,
    continue_pending_choices: bool,
    config: &TurnPlannerConfigV1,
    potions_used: u32,
) -> Vec<crate::sim::combat_action::CombatActionChoice> {
    filtered_legal_actions(
        legal_action_choices(
            stepper,
            position,
            continue_pending_choices,
            config.max_inner_nodes,
        ),
        config.potion_policy,
        config.allowed_potion_slots,
        config.allow_potion_discard,
        &position.combat,
    )
    .into_iter()
    .filter(|choice| {
        !config
            .max_potions_used
            .is_some_and(|max| potions_used >= max && is_use_potion_input(&choice.input))
    })
    .collect()
}
