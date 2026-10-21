use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use sts_core::ai::combat_state_key::combat_exact_state_key;
use sts_core::sim::combat::CombatStepper;

use crate::depth_beam_turn::{
    generate_depth_beam_turn_options, DepthBeamTurnBudget, DepthBeamTurnConfig, DepthBeamTurnStatus,
};
use crate::policy::{CombatGuideLaneId, CombatStateGuideRank};
use crate::types::{
    CombatDecisionRoot, CompleteTurnOption, CompleteTurnOptionBoundary, TurnOptionGeneratorConfig,
};

use super::{
    deadline_reached, DiscrepancyWork, DiveSeed, PolicyDiscrepancyInterruption,
    PolicyDiscrepancySession, TraceNode, TurnMacroProposal,
};

impl PolicyDiscrepancySession {
    pub(super) fn enqueue_turn_macro_if_needed(&mut self, seed: &DiveSeed) {
        if self.config.turn_macro.is_none() || !seed.at_player_turn_boundary {
            return;
        }
        let key = combat_exact_state_key(&seed.position.engine, &seed.position.combat);
        if !self.turn_macro_scheduled_states.insert(key) {
            return;
        }
        let discrepancy = seed.discrepancy + std::f64::consts::LN_2;
        self.push_work(
            discrepancy,
            DiscrepancyWork::TurnMacro(TurnMacroProposal { seed: seed.clone() }),
        );
    }

    pub(super) fn run_turn_macro(
        &mut self,
        stepper: &dyn CombatStepper,
        work: TurnMacroProposal,
        deadline: Option<Instant>,
    ) -> Option<PolicyDiscrepancyInterruption> {
        let config = self.config.turn_macro?;
        if deadline_reached(deadline) {
            self.push_work(
                work.seed.discrepancy + std::f64::consts::LN_2,
                DiscrepancyWork::TurnMacro(work),
            );
            return Some(PolicyDiscrepancyInterruption::Deadline);
        }
        let remaining_transitions = self
            .granted_applied_transitions
            .saturating_sub(self.used.applied_action_transitions);
        let reserved_transitions = config.max_applied_transitions.max(1);
        if remaining_transitions < reserved_transitions {
            self.push_work(
                work.seed.discrepancy + std::f64::consts::LN_2,
                DiscrepancyWork::TurnMacro(work),
            );
            return Some(PolicyDiscrepancyInterruption::AppliedTransitionBudget);
        }
        let remaining_engine_steps = self
            .granted_engine_steps
            .saturating_sub(self.used.engine_steps);
        let reserved_engine_steps =
            reserved_transitions.saturating_mul(self.config.max_engine_steps_per_transition.max(1));
        if remaining_engine_steps < reserved_engine_steps {
            self.push_work(
                work.seed.discrepancy + std::f64::consts::LN_2,
                DiscrepancyWork::TurnMacro(work),
            );
            return Some(PolicyDiscrepancyInterruption::EngineStepBudget);
        }
        let Ok(root) = CombatDecisionRoot::new((*work.seed.position).clone()) else {
            self.used.unsupported_stable_boundaries =
                self.used.unsupported_stable_boundaries.saturating_add(1);
            return None;
        };
        let report = generate_depth_beam_turn_options(
            root,
            DepthBeamTurnConfig {
                generator: TurnOptionGeneratorConfig {
                    max_engine_steps_per_transition: self.config.max_engine_steps_per_transition,
                    allow_potion_expenditure: self.config.max_potions_used != Some(0),
                    allow_potion_discard: self.config.allow_potion_discard,
                    allowed_potion_slots: self.config.allowed_potion_slots,
                    ..TurnOptionGeneratorConfig::default()
                },
                max_potion_expenditures: self.config.max_potions_used.map(|limit| {
                    let already_spent = work
                        .seed
                        .trace
                        .actions()
                        .iter()
                        .filter(|action| {
                            matches!(
                                action.input,
                                sts_core::state::core::ClientInput::UsePotion { .. }
                                    | sts_core::state::core::ClientInput::DiscardPotion(_)
                            )
                        })
                        .count()
                        .try_into()
                        .unwrap_or(u32::MAX);
                    limit.saturating_sub(already_spent)
                }),
                partial_beam_width: config.partial_beam_width,
                retained_per_view: config.retained_per_view,
                max_atomic_depth: config.max_atomic_depth,
                max_structured_members_per_family: config.max_structured_members_per_family,
            },
            DepthBeamTurnBudget {
                max_applied_transitions: reserved_transitions,
                max_engine_steps: reserved_engine_steps,
                deadline,
            },
            self.policy.clone(),
            stepper,
        );
        self.used.turn_macro_generations = self.used.turn_macro_generations.saturating_add(1);
        self.used.turn_macro_applied_transitions = self
            .used
            .turn_macro_applied_transitions
            .saturating_add(report.counters.applied_transitions);
        self.used.applied_action_transitions = self
            .used
            .applied_action_transitions
            .saturating_add(report.counters.applied_transitions);
        self.used.engine_steps = self
            .used
            .engine_steps
            .saturating_add(report.counters.engine_steps);
        self.used.turn_macro_options_generated = self
            .used
            .turn_macro_options_generated
            .saturating_add(report.options.len());
        if !matches!(report.status, DepthBeamTurnStatus::Complete) {
            self.used.turn_macro_partial_generations =
                self.used.turn_macro_partial_generations.saturating_add(1);
        }
        let retry_after_deadline = matches!(
            report.status,
            DepthBeamTurnStatus::Partial(
                crate::depth_beam_turn::DepthBeamTurnInterruption::Deadline
            )
        );

        for (_rank, option) in selected_turn_macro_options(
            &report.options,
            self.policy.as_ref(),
            config.proposals_per_view,
        ) {
            if matches!(
                option.boundary(),
                CompleteTurnOptionBoundary::TerminalLoss | CompleteTurnOptionBoundary::Escape
            ) {
                continue;
            }
            // Guide ranks are ordinal and deliberately uncalibrated. Every
            // member of the bounded per-view proposal set is therefore one
            // macro deviation; rank selects the set but is not invented into
            // a probability-like path cost.
            let discrepancy = work.seed.discrepancy + std::f64::consts::LN_2;
            let key = combat_exact_state_key(
                &option.exact_successor().engine,
                &option.exact_successor().combat,
            );
            self.turn_macro_selected_states.insert(key.clone());
            if self
                .best_state_discrepancy
                .get(&key)
                .is_some_and(|previous| *previous <= discrepancy)
            {
                self.used.duplicate_or_dominated_states =
                    self.used.duplicate_or_dominated_states.saturating_add(1);
                continue;
            }
            let previous = self.best_state_discrepancy.insert(key, discrepancy);
            if previous.is_none() {
                self.used.exact_states = self.used.exact_states.saturating_add(1);
            }
            let trace = option
                .actions()
                .iter()
                .cloned()
                .fold(work.seed.trace.clone(), TraceNode::extend);
            self.push_work(
                discrepancy,
                DiscrepancyWork::Dive(DiveSeed {
                    position: Arc::new(option.exact_successor().clone()),
                    trace,
                    discrepancy,
                    greedy_actions_since_deviation: 0,
                    at_player_turn_boundary: matches!(
                        option.boundary(),
                        CompleteTurnOptionBoundary::NextPlayerTurn
                    ),
                }),
            );
            self.used.turn_macro_options_enqueued =
                self.used.turn_macro_options_enqueued.saturating_add(1);
        }
        if retry_after_deadline {
            // The bounded depth beam is currently a one-shot generator. Its
            // partial options are sound and have already been admitted, but
            // an external wall slice must not make the unvisited remainder
            // disappear. Requeue the same exact boundary; duplicate partial
            // options are filtered by exact-state discrepancy on the retry.
            self.used.turn_macro_deadline_retries =
                self.used.turn_macro_deadline_retries.saturating_add(1);
            self.push_work(
                work.seed.discrepancy + std::f64::consts::LN_2,
                DiscrepancyWork::TurnMacro(work),
            );
            return Some(PolicyDiscrepancyInterruption::Deadline);
        }
        None
    }
}

fn selected_turn_macro_options<'a>(
    options: &'a [CompleteTurnOption],
    policy: &dyn crate::policy::CombatActionPolicy,
    proposals_per_view: usize,
) -> Vec<(usize, &'a CompleteTurnOption)> {
    let per_view = proposals_per_view.max(1);
    let mut selected = HashMap::<String, (usize, usize)>::new();
    for (index, option) in options.iter().enumerate().take(per_view) {
        selected.insert(option.exact_successor_hash().to_owned(), (index, index));
    }
    let mut lanes = HashMap::<CombatGuideLaneId, Vec<(CombatStateGuideRank, usize)>>::new();
    for (index, option) in options.iter().enumerate() {
        for guide in policy.state_guides(option.exact_successor()) {
            lanes
                .entry(guide.lane)
                .or_default()
                .push((guide.rank, index));
        }
    }
    for candidates in lanes.values_mut() {
        candidates.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| {
                    options[left.1]
                        .negative_log_policy()
                        .total_cmp(&options[right.1].negative_log_policy())
                })
                .then_with(|| left.1.cmp(&right.1))
        });
        for (rank, (_, index)) in candidates.iter().take(per_view).enumerate() {
            let hash = options[*index].exact_successor_hash().to_owned();
            selected
                .entry(hash)
                .and_modify(|current| {
                    if rank < current.0 {
                        *current = (rank, *index);
                    }
                })
                .or_insert((rank, *index));
        }
    }
    let mut selected = selected.into_values().collect::<Vec<_>>();
    selected.sort();
    selected
        .into_iter()
        .map(|(rank, index)| (rank, &options[index]))
        .collect()
}
