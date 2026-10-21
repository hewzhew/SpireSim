use std::collections::{BTreeMap, HashSet, VecDeque};
use std::time::Instant;

use rustc_hash::FxBuildHasher;
use sts_agent::agent::information::state::public_combat_state_v1;
use sts_agent::agent::task::{
    advance_public_combat_plan_v1, public_combat_agenda_for_plan_v1, PublicCombatPlanStateV1,
};
use sts_combat_option_grounder::{
    ground_public_combat_agenda_v1, public_combat_agenda_partial_commitment_rank_v1,
    GroundableCompleteTurnV1,
};
use sts_complete_turn_proposal::{
    exact_complete_turn_proposals_with_partial_guides_and_deadline_v1,
    ExactCompleteTurnProposalActionV1, ExactCompleteTurnProposalConfigV1,
    ExactCompleteTurnProposalGuideRankV1,
};
use sts_core::ai::combat_state_key::{combat_exact_state_key, CombatExactStateKey};
use sts_core::sim::combat::{CombatPosition, CombatStepper, CombatTerminal, EngineCombatStepper};

use crate::{CombatGuideLaneId, CombatStateGuide, SharedCombatActionPolicy};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteTurnProposalBeamConfigV1 {
    pub proposal: ExactCompleteTurnProposalConfigV1,
    /// Finite potion-use/discard allowance across the entire combat line.
    /// The nested proposal limit applies to one generated complete turn.
    pub max_total_potion_expenditures: Option<u32>,
    pub beam_width: usize,
    pub max_turn_depth: usize,
    pub max_expanded_boundaries: usize,
    pub deadline: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteTurnProposalBeamSessionConfigV1 {
    pub proposal: ExactCompleteTurnProposalConfigV1,
    pub max_total_potion_expenditures: Option<u32>,
    pub beam_width: usize,
    pub max_turn_depth: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteTurnProposalBeamQuantumV1 {
    pub additional_expanded_boundaries: usize,
    pub deadline: Option<Instant>,
}

impl Default for CompleteTurnProposalBeamConfigV1 {
    fn default() -> Self {
        Self {
            proposal: ExactCompleteTurnProposalConfigV1::default(),
            max_total_potion_expenditures: None,
            beam_width: 48,
            max_turn_depth: 32,
            max_expanded_boundaries: 2_048,
            deadline: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompleteTurnProposalBeamInterruptionV1 {
    TurnDepth,
    ExpandedBoundaries,
    Deadline,
    FrontierExhausted,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompleteTurnProposalBeamCountersV1 {
    pub expanded_boundaries: usize,
    pub proposal_nodes_expanded: usize,
    pub proposal_nodes_generated: usize,
    pub proposal_engine_steps: usize,
    pub proposal_candidates: usize,
    pub grounded_candidates: usize,
    pub agenda_frontier_candidates: usize,
    pub unique_successors: usize,
    pub duplicate_successors: usize,
    pub retained_successors: usize,
    pub terminal_losses: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteTurnProposalBeamLayerV1 {
    pub turn_depth: usize,
    pub parents: usize,
    pub proposal_candidates: usize,
    pub grounded_candidates: usize,
    pub agenda_frontier_candidates: usize,
    pub unique_successors: usize,
    pub duplicate_successors: usize,
    pub terminal_losses: usize,
    pub retained_successors: usize,
    pub retained_root_lineages: usize,
    pub largest_root_lineage: usize,
    pub retained_exact_state_hashes: Vec<String>,
    pub retained_guide_envelopes: Vec<CompleteTurnProposalBeamGuideEnvelopeV1>,
    pub retained_guide_representatives: Vec<CompleteTurnProposalBeamGuideRepresentativeV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteTurnProposalBeamGuideEnvelopeV1 {
    pub lane: CombatGuideLaneId,
    pub members: usize,
    pub best_components: Vec<i32>,
    pub worst_components: Vec<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompleteTurnProposalBeamGuideRepresentativeV1 {
    pub lane: CombatGuideLaneId,
    pub exact_state_hash: String,
    pub player_hp: i32,
    pub player_block: i32,
    pub living_enemies: usize,
    pub total_living_enemy_hp: i32,
    pub rank_components: Vec<i32>,
    pub plan: PublicCombatPlanStateV1,
    pub actions: Vec<sts_core::state::core::ClientInput>,
}

#[derive(Clone)]
pub struct CompleteTurnProposalBeamWinV1 {
    pub actions: Vec<ExactCompleteTurnProposalActionV1>,
    pub terminal: CombatPosition,
}

#[derive(Clone)]
pub struct CompleteTurnProposalBeamReportV1 {
    pub win: Option<CompleteTurnProposalBeamWinV1>,
    pub interruption: Option<CompleteTurnProposalBeamInterruptionV1>,
    pub counters: CompleteTurnProposalBeamCountersV1,
    pub layers: Vec<CompleteTurnProposalBeamLayerV1>,
}

#[derive(Clone)]
struct BeamStateV1 {
    position: CombatPosition,
    plan: PublicCombatPlanStateV1,
    actions: Vec<ExactCompleteTurnProposalActionV1>,
    potion_expenditures: u32,
    guides: Vec<CombatStateGuide>,
    stable_order: usize,
    root_lineage: Option<usize>,
}

pub struct CompleteTurnProposalBeamSessionV1 {
    config: CompleteTurnProposalBeamSessionConfigV1,
    policy: SharedCombatActionPolicy,
    seen: HashSet<(CombatExactStateKey, PublicCombatPlanStateV1, Option<u32>), FxBuildHasher>,
    frontier: VecDeque<BeamStateV1>,
    next: Vec<BeamStateV1>,
    layer_parent_count: usize,
    layer_before: CompleteTurnProposalBeamCountersV1,
    completed_turn_depth: usize,
    next_stable_order: usize,
    report: CompleteTurnProposalBeamReportV1,
    terminal: bool,
}

pub fn search_complete_turn_proposal_beam_v1(
    root: CombatPosition,
    root_plan: PublicCombatPlanStateV1,
    config: CompleteTurnProposalBeamConfigV1,
    policy: SharedCombatActionPolicy,
) -> CompleteTurnProposalBeamReportV1 {
    search_complete_turn_proposal_beam_with_win_acceptance_v1(
        root,
        root_plan,
        config,
        policy,
        &|_| true,
    )
}

/// Searches until the caller accepts one exact terminal win or the ordinary
/// bounded search allowance ends.
///
/// The planner owns exact reachability but does not interpret run resources.
/// A caller may therefore reject an early win against its own typed quality
/// contract without teaching the planner an HP/potion exchange rate.  The
/// first verified win remains in the report as a fallback if no later win is
/// accepted within the bound.
pub fn search_complete_turn_proposal_beam_with_win_acceptance_v1(
    root: CombatPosition,
    root_plan: PublicCombatPlanStateV1,
    config: CompleteTurnProposalBeamConfigV1,
    policy: SharedCombatActionPolicy,
    accept_win: &dyn Fn(&CompleteTurnProposalBeamWinV1) -> bool,
) -> CompleteTurnProposalBeamReportV1 {
    let mut session = CompleteTurnProposalBeamSessionV1::new(
        root,
        root_plan,
        CompleteTurnProposalBeamSessionConfigV1 {
            proposal: config.proposal,
            max_total_potion_expenditures: config.max_total_potion_expenditures,
            beam_width: config.beam_width,
            max_turn_depth: config.max_turn_depth,
        },
        policy,
    );
    session.advance_until_win_accepted(
        CompleteTurnProposalBeamQuantumV1 {
            additional_expanded_boundaries: config.max_expanded_boundaries,
            deadline: config.deadline,
        },
        accept_win,
    )
}

impl CompleteTurnProposalBeamSessionV1 {
    pub fn new(
        root: CombatPosition,
        root_plan: PublicCombatPlanStateV1,
        config: CompleteTurnProposalBeamSessionConfigV1,
        policy: SharedCombatActionPolicy,
    ) -> Self {
        let guides = policy.state_guides(&root);
        let identity = (
            combat_exact_state_key(&root.engine, &root.combat),
            root_plan.clone(),
            config.max_total_potion_expenditures.map(|_| 0),
        );
        let mut seen = HashSet::default();
        seen.insert(identity);
        let mut frontier = VecDeque::new();
        frontier.push_back(BeamStateV1 {
            position: root,
            plan: root_plan,
            actions: Vec::new(),
            potion_expenditures: 0,
            guides,
            stable_order: 0,
            root_lineage: None,
        });
        let terminal = config.beam_width == 0 || config.max_turn_depth == 0;
        Self {
            config,
            policy,
            seen,
            frontier,
            next: Vec::new(),
            layer_parent_count: 1,
            layer_before: CompleteTurnProposalBeamCountersV1::default(),
            completed_turn_depth: 0,
            next_stable_order: 1,
            report: CompleteTurnProposalBeamReportV1 {
                win: None,
                interruption: terminal.then_some(CompleteTurnProposalBeamInterruptionV1::TurnDepth),
                counters: CompleteTurnProposalBeamCountersV1::default(),
                layers: Vec::new(),
            },
            terminal,
        }
    }

    pub fn advance(
        &mut self,
        quantum: CompleteTurnProposalBeamQuantumV1,
    ) -> CompleteTurnProposalBeamReportV1 {
        self.advance_until_win_accepted(quantum, &|_| true)
    }

    pub fn advance_until_win_accepted(
        &mut self,
        quantum: CompleteTurnProposalBeamQuantumV1,
        accept_win: &dyn Fn(&CompleteTurnProposalBeamWinV1) -> bool,
    ) -> CompleteTurnProposalBeamReportV1 {
        if self.terminal {
            return self.report.clone();
        }
        if self.report.win.as_ref().is_some_and(accept_win) {
            self.terminal = true;
            return self.report.clone();
        }
        let expanded_at_start = self.report.counters.expanded_boundaries;
        loop {
            if self.frontier.is_empty() {
                self.finish_layer();
                if self.terminal {
                    return self.report.clone();
                }
                continue;
            }
            if deadline_reached(quantum.deadline) {
                return self.interrupted_report(CompleteTurnProposalBeamInterruptionV1::Deadline);
            }
            if self
                .report
                .counters
                .expanded_boundaries
                .saturating_sub(expanded_at_start)
                >= quantum.additional_expanded_boundaries
            {
                return self.interrupted_report(
                    CompleteTurnProposalBeamInterruptionV1::ExpandedBoundaries,
                );
            }
            let parent = self
                .frontier
                .pop_front()
                .expect("empty frontier is completed before budget checks");
            self.expand_parent(parent, quantum.deadline, accept_win);
            if self.terminal {
                return self.report.clone();
            }
        }
    }

    fn expand_parent(
        &mut self,
        parent: BeamStateV1,
        deadline: Option<Instant>,
        accept_win: &dyn Fn(&CompleteTurnProposalBeamWinV1) -> bool,
    ) {
        self.report.counters.expanded_boundaries =
            self.report.counters.expanded_boundaries.saturating_add(1);
        let root_public = public_combat_state_v1(&parent.position.combat);
        let agenda = public_combat_agenda_for_plan_v1(&root_public, &parent.plan);
        let proposal_config = proposal_config_with_remaining_potion_budget(
            self.config.proposal,
            self.config.max_total_potion_expenditures,
            parent.potion_expenditures,
        );
        let batch = exact_complete_turn_proposals_with_partial_guides_and_deadline_v1(
            &parent.position,
            proposal_config,
            deadline,
            &|partial| {
                public_combat_agenda_partial_commitment_rank_v1(&agenda, partial)
                    .into_iter()
                    .flat_map(|rank| rank.guide_ranks())
                    .map(|(lane, components)| ExactCompleteTurnProposalGuideRankV1 {
                        lane,
                        components,
                    })
                    .collect()
            },
            &|successor| proposal_guide_ranks(self.policy.as_ref(), successor),
        );
        self.report.counters.proposal_nodes_expanded = self
            .report
            .counters
            .proposal_nodes_expanded
            .saturating_add(batch.nodes_expanded);
        self.report.counters.proposal_nodes_generated = self
            .report
            .counters
            .proposal_nodes_generated
            .saturating_add(batch.nodes_generated);
        self.report.counters.proposal_engine_steps = self
            .report
            .counters
            .proposal_engine_steps
            .saturating_add(batch.engine_steps);
        self.report.counters.proposal_candidates = self
            .report
            .counters
            .proposal_candidates
            .saturating_add(batch.candidates.len());
        let candidate_actions = batch
            .candidates
            .iter()
            .map(|candidate| candidate.actions.clone())
            .collect::<Vec<_>>();
        let groundable = batch
            .candidates
            .into_iter()
            .enumerate()
            .map(|(plan_index, candidate)| GroundableCompleteTurnV1 {
                plan_index,
                actions: candidate
                    .actions
                    .iter()
                    .map(|action| action.input.clone())
                    .collect(),
                successor: candidate.successor,
            })
            .collect();
        let Ok(grounded) = ground_public_combat_agenda_v1(
            &parent.position,
            &parent.plan,
            &agenda,
            groundable,
            self.config.proposal.max_engine_steps_per_action,
        ) else {
            return;
        };
        self.report.counters.grounded_candidates = self
            .report
            .counters
            .grounded_candidates
            .saturating_add(grounded.all_candidates().len());
        self.report.counters.agenda_frontier_candidates = self
            .report
            .counters
            .agenda_frontier_candidates
            .saturating_add(grounded.frontier_candidates().count());
        self.report.counters.terminal_losses = self.report.counters.terminal_losses.saturating_add(
            grounded
                .all_candidates()
                .iter()
                .filter(|candidate| {
                    EngineCombatStepper.terminal(&candidate.candidate.successor)
                        == CombatTerminal::Loss
                })
                .count(),
        );
        for candidate in grounded.into_frontier_candidates() {
            let terminal = EngineCombatStepper.terminal(&candidate.candidate.successor);
            if terminal == CombatTerminal::Loss {
                continue;
            }
            let Some(turn_actions) = candidate_actions.get(candidate.candidate.plan_index) else {
                continue;
            };
            let potion_expenditures = parent
                .potion_expenditures
                .saturating_add(count_potion_expenditures(turn_actions));
            if self
                .config
                .max_total_potion_expenditures
                .is_some_and(|limit| potion_expenditures > limit)
            {
                continue;
            }
            let mut actions = parent.actions.clone();
            actions.extend(turn_actions.iter().cloned());
            if EngineCombatStepper.is_exact_victory(&candidate.candidate.successor) {
                let candidate = CompleteTurnProposalBeamWinV1 {
                    actions,
                    terminal: candidate.candidate.successor,
                };
                if self.report.win.is_none() {
                    self.report.win = Some(candidate.clone());
                }
                if accept_win(&candidate) {
                    self.report.win = Some(candidate);
                    self.terminal = true;
                    return;
                }
                continue;
            }
            if terminal != CombatTerminal::Unresolved {
                continue;
            }
            let successor_public = public_combat_state_v1(&candidate.candidate.successor.combat);
            let next_plan = advance_public_combat_plan_v1(&parent.plan, &successor_public);
            let identity = (
                combat_exact_state_key(
                    &candidate.candidate.successor.engine,
                    &candidate.candidate.successor.combat,
                ),
                next_plan.clone(),
                self.config
                    .max_total_potion_expenditures
                    .map(|_| potion_expenditures),
            );
            if !self.seen.insert(identity) {
                self.report.counters.duplicate_successors =
                    self.report.counters.duplicate_successors.saturating_add(1);
                continue;
            }
            let guides = self.policy.state_guides(&candidate.candidate.successor);
            self.next.push(BeamStateV1 {
                position: candidate.candidate.successor,
                plan: next_plan,
                actions,
                potion_expenditures,
                guides,
                stable_order: self.next_stable_order,
                root_lineage: parent.root_lineage.or(Some(self.next_stable_order)),
            });
            self.next_stable_order = self.next_stable_order.saturating_add(1);
            self.report.counters.unique_successors =
                self.report.counters.unique_successors.saturating_add(1);
        }
    }

    fn finish_layer(&mut self) {
        let selected =
            select_independent_guide_beam(std::mem::take(&mut self.next), self.config.beam_width);
        let retained_guide_envelopes = guide_envelopes(&selected);
        let retained_guide_representatives = guide_representatives(&selected);
        let root_lineage_counts = selected.iter().fold(
            BTreeMap::<Option<usize>, usize>::new(),
            |mut counts, state| {
                *counts.entry(state.root_lineage).or_default() += 1;
                counts
            },
        );
        self.report.counters.retained_successors = self
            .report
            .counters
            .retained_successors
            .saturating_add(selected.len());
        self.report.layers.push(CompleteTurnProposalBeamLayerV1 {
            turn_depth: self.completed_turn_depth.saturating_add(1),
            parents: self.layer_parent_count,
            proposal_candidates: self
                .report
                .counters
                .proposal_candidates
                .saturating_sub(self.layer_before.proposal_candidates),
            grounded_candidates: self
                .report
                .counters
                .grounded_candidates
                .saturating_sub(self.layer_before.grounded_candidates),
            agenda_frontier_candidates: self
                .report
                .counters
                .agenda_frontier_candidates
                .saturating_sub(self.layer_before.agenda_frontier_candidates),
            unique_successors: self
                .report
                .counters
                .unique_successors
                .saturating_sub(self.layer_before.unique_successors),
            duplicate_successors: self
                .report
                .counters
                .duplicate_successors
                .saturating_sub(self.layer_before.duplicate_successors),
            terminal_losses: self
                .report
                .counters
                .terminal_losses
                .saturating_sub(self.layer_before.terminal_losses),
            retained_successors: selected.len(),
            retained_root_lineages: root_lineage_counts.len(),
            largest_root_lineage: root_lineage_counts
                .values()
                .copied()
                .max()
                .unwrap_or_default(),
            retained_exact_state_hashes: selected
                .iter()
                .map(|state| {
                    sts_core::ai::combat_state_key::combat_exact_state_hash_v2(
                        &state.position.engine,
                        &state.position.combat,
                    )
                })
                .collect(),
            retained_guide_envelopes,
            retained_guide_representatives,
        });
        self.completed_turn_depth = self.completed_turn_depth.saturating_add(1);
        if selected.is_empty() {
            self.report.interruption =
                Some(CompleteTurnProposalBeamInterruptionV1::FrontierExhausted);
            self.terminal = true;
            return;
        }
        if self.completed_turn_depth >= self.config.max_turn_depth {
            self.report.interruption = Some(CompleteTurnProposalBeamInterruptionV1::TurnDepth);
            self.terminal = true;
            return;
        }
        self.frontier = selected.into();
        self.layer_parent_count = self.frontier.len();
        self.layer_before = self.report.counters.clone();
    }

    fn interrupted_report(
        &self,
        interruption: CompleteTurnProposalBeamInterruptionV1,
    ) -> CompleteTurnProposalBeamReportV1 {
        let mut report = self.report.clone();
        report.interruption = Some(interruption);
        report
    }
}

fn guide_representatives(
    states: &[BeamStateV1],
) -> Vec<CompleteTurnProposalBeamGuideRepresentativeV1> {
    let lanes = states
        .iter()
        .flat_map(|state| state.guides.iter().map(|guide| guide.lane))
        .collect::<std::collections::BTreeSet<_>>();
    lanes
        .into_iter()
        .filter_map(|lane| {
            let state = states.iter().max_by(|left, right| {
                guide_rank(left, lane)
                    .cmp(&guide_rank(right, lane))
                    .then_with(|| right.stable_order.cmp(&left.stable_order))
            })?;
            let rank = guide_rank(state, lane)?;
            let living = state
                .position
                .combat
                .entities
                .monsters
                .iter()
                .filter(|monster| monster.is_alive_for_action())
                .collect::<Vec<_>>();
            Some(CompleteTurnProposalBeamGuideRepresentativeV1 {
                lane,
                exact_state_hash: sts_core::ai::combat_state_key::combat_exact_state_hash_v2(
                    &state.position.engine,
                    &state.position.combat,
                ),
                player_hp: state.position.combat.entities.player.current_hp,
                player_block: state.position.combat.entities.player.block,
                living_enemies: living.len(),
                total_living_enemy_hp: living.iter().map(|monster| monster.current_hp.max(0)).sum(),
                rank_components: rank.components().to_vec(),
                plan: state.plan.clone(),
                actions: state
                    .actions
                    .iter()
                    .map(|action| action.input.clone())
                    .collect(),
            })
        })
        .collect()
}

fn guide_envelopes(states: &[BeamStateV1]) -> Vec<CompleteTurnProposalBeamGuideEnvelopeV1> {
    guide_envelopes_from_sets(states.iter().map(|state| state.guides.as_slice()))
}

fn guide_envelopes_from_sets<'a>(
    guide_sets: impl IntoIterator<Item = &'a [CombatStateGuide]>,
) -> Vec<CompleteTurnProposalBeamGuideEnvelopeV1> {
    let mut ranks = BTreeMap::<CombatGuideLaneId, Vec<&CombatStateGuide>>::new();
    for guides in guide_sets {
        for guide in guides {
            ranks.entry(guide.lane).or_default().push(guide);
        }
    }
    ranks
        .into_iter()
        .filter_map(|(lane, guides)| {
            let best = guides.iter().max_by_key(|guide| &guide.rank)?;
            let worst = guides.iter().min_by_key(|guide| &guide.rank)?;
            Some(CompleteTurnProposalBeamGuideEnvelopeV1 {
                lane,
                members: guides.len(),
                best_components: best.rank.components().to_vec(),
                worst_components: worst.rank.components().to_vec(),
            })
        })
        .collect()
}

fn select_independent_guide_beam(
    candidates: Vec<BeamStateV1>,
    beam_width: usize,
) -> Vec<BeamStateV1> {
    if candidates.len() <= beam_width {
        return candidates;
    }
    let mut by_lane = BTreeMap::<CombatGuideLaneId, Vec<usize>>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        for guide in &candidate.guides {
            by_lane.entry(guide.lane).or_default().push(index);
        }
    }
    for (lane, indexes) in &mut by_lane {
        indexes.sort_by(|left, right| {
            guide_rank(&candidates[*right], *lane)
                .cmp(&guide_rank(&candidates[*left], *lane))
                .then_with(|| {
                    candidates[*left]
                        .stable_order
                        .cmp(&candidates[*right].stable_order)
                })
        });
    }
    let mut selected = Vec::<usize>::with_capacity(beam_width);
    let mut members = HashSet::<usize, FxBuildHasher>::default();
    let mut lineage_members = BTreeMap::<Option<usize>, usize>::new();
    let maximum_lane_depth = by_lane.values().map(Vec::len).max().unwrap_or_default();
    for lineage_cap in 1..=beam_width {
        'rounds: for rank in 0..maximum_lane_depth {
            for indexes in by_lane.values() {
                let Some(index) = indexes.get(rank).copied() else {
                    continue;
                };
                if members.contains(&index)
                    || lineage_members
                        .get(&candidates[index].root_lineage)
                        .copied()
                        .unwrap_or_default()
                        >= lineage_cap
                {
                    continue;
                }
                members.insert(index);
                *lineage_members
                    .entry(candidates[index].root_lineage)
                    .or_default() += 1;
                selected.push(index);
                if selected.len() == beam_width {
                    break 'rounds;
                }
            }
        }
        if selected.len() == beam_width {
            break;
        }
    }
    if selected.len() < beam_width {
        let mut stable = (0..candidates.len()).collect::<Vec<_>>();
        stable.sort_by_key(|index| candidates[*index].stable_order);
        for index in stable {
            if members.insert(index) {
                selected.push(index);
                if selected.len() == beam_width {
                    break;
                }
            }
        }
    }
    let mut slots = candidates.into_iter().map(Some).collect::<Vec<_>>();
    selected
        .into_iter()
        .filter_map(|index| slots[index].take())
        .collect()
}

fn guide_rank(
    state: &BeamStateV1,
    lane: CombatGuideLaneId,
) -> Option<&crate::CombatStateGuideRank> {
    state
        .guides
        .iter()
        .find(|guide| guide.lane == lane)
        .map(|guide| &guide.rank)
}

fn proposal_guide_ranks(
    policy: &dyn crate::CombatActionPolicy,
    successor: &CombatPosition,
) -> Vec<ExactCompleteTurnProposalGuideRankV1> {
    policy
        .state_guides(successor)
        .into_iter()
        .map(|guide| ExactCompleteTurnProposalGuideRankV1 {
            lane: guide.lane.value(),
            components: guide.rank.components().to_vec(),
        })
        .collect()
}

fn proposal_config_with_remaining_potion_budget(
    mut proposal: ExactCompleteTurnProposalConfigV1,
    max_total_potion_expenditures: Option<u32>,
    potion_expenditures: u32,
) -> ExactCompleteTurnProposalConfigV1 {
    let remaining =
        max_total_potion_expenditures.map(|limit| limit.saturating_sub(potion_expenditures));
    proposal.max_potion_expenditures = match (proposal.max_potion_expenditures, remaining) {
        (Some(per_turn), Some(remaining)) => Some(per_turn.min(remaining)),
        (Some(per_turn), None) => Some(per_turn),
        (None, Some(remaining)) => Some(remaining),
        (None, None) => None,
    };
    if proposal.max_potion_expenditures == Some(0) {
        proposal.allow_potion_expenditure = false;
    }
    proposal
}

fn count_potion_expenditures(actions: &[ExactCompleteTurnProposalActionV1]) -> u32 {
    actions
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
        .unwrap_or(u32::MAX)
}

fn deadline_reached(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|limit| Instant::now() >= limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::Arc;

    use sts_agent::agent::task::public_combat_plan_v1;
    use sts_core::content::cards::CardId;
    use sts_core::content::monsters::EnemyId;
    use sts_core::runtime::combat::CombatCard;
    use sts_core::state::core::EngineState;

    #[test]
    fn total_potion_budget_becomes_the_remaining_next_turn_allowance() {
        let proposal = ExactCompleteTurnProposalConfigV1 {
            allow_potion_expenditure: true,
            max_potion_expenditures: None,
            ..ExactCompleteTurnProposalConfigV1::default()
        };

        let first_turn = proposal_config_with_remaining_potion_budget(proposal, Some(1), 0);
        let later_turn = proposal_config_with_remaining_potion_budget(proposal, Some(1), 1);

        assert!(first_turn.allow_potion_expenditure);
        assert_eq!(first_turn.max_potion_expenditures, Some(1));
        assert!(!later_turn.allow_potion_expenditure);
        assert_eq!(later_turn.max_potion_expenditures, Some(0));
    }

    #[test]
    fn caller_can_reject_an_early_win_and_accept_a_later_exact_win() {
        let mut combat = sts_core::test_support::blank_test_combat();
        let mut jaw_worm = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        jaw_worm.current_hp = 3;
        jaw_worm.max_hp = 3;
        combat.entities.monsters = vec![jaw_worm];
        combat.turn.energy = 3;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Bash, 22),
        ];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let plan = public_combat_plan_v1(&public_combat_state_v1(&root.combat));
        let mut session = CompleteTurnProposalBeamSessionV1::new(
            root,
            plan,
            CompleteTurnProposalBeamSessionConfigV1 {
                proposal: ExactCompleteTurnProposalConfigV1 {
                    max_inner_nodes: 64,
                    max_partial_frontier_states: 32,
                    max_end_states: 24,
                    per_bucket_limit: 24,
                    ..ExactCompleteTurnProposalConfigV1::default()
                },
                max_total_potion_expenditures: None,
                beam_width: 8,
                max_turn_depth: 2,
            },
            Arc::new(crate::UniformCombatActionPolicy),
        );
        let wins_seen = Cell::new(0_usize);

        let report = session.advance_until_win_accepted(
            CompleteTurnProposalBeamQuantumV1 {
                additional_expanded_boundaries: 8,
                deadline: None,
            },
            &|_| {
                let seen = wins_seen.get().saturating_add(1);
                wins_seen.set(seen);
                seen >= 2
            },
        );

        assert!(wins_seen.get() >= 2);
        assert!(report.win.is_some());
        assert_eq!(report.interruption, None);
    }

    #[test]
    fn guide_envelopes_keep_independent_lane_orderings() {
        let progress = CombatGuideLaneId::new(1);
        let survival = CombatGuideLaneId::new(2);
        let left = vec![
            CombatStateGuide::new(progress, vec![9, 1]),
            CombatStateGuide::new(survival, vec![2]),
        ];
        let right = vec![
            CombatStateGuide::new(progress, vec![4, 8]),
            CombatStateGuide::new(survival, vec![7]),
        ];

        let envelopes = guide_envelopes_from_sets([left.as_slice(), right.as_slice()]);

        assert_eq!(envelopes.len(), 2);
        assert_eq!(envelopes[0].best_components, vec![9, 1]);
        assert_eq!(envelopes[0].worst_components, vec![4, 8]);
        assert_eq!(envelopes[1].best_components, vec![7]);
        assert_eq!(envelopes[1].worst_components, vec![2]);
    }

    #[test]
    fn beam_admits_only_agenda_frontier_candidates_and_reports_both_counts() {
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.turn.energy = 1;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::FeelNoPain, 11),
            CombatCard::for_scenario(CardId::Defend, 22),
        ];
        combat.meta.master_deck_snapshot = combat.zones.hand.clone().into();
        let mut jaw_worm = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        jaw_worm.current_hp = 80;
        jaw_worm.max_hp = 80;
        combat.entities.monsters = vec![jaw_worm];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let plan = public_combat_plan_v1(&public_combat_state_v1(&root.combat));
        let mut session = CompleteTurnProposalBeamSessionV1::new(
            root,
            plan,
            CompleteTurnProposalBeamSessionConfigV1 {
                proposal: ExactCompleteTurnProposalConfigV1 {
                    max_inner_nodes: 128,
                    max_partial_frontier_states: 32,
                    max_end_states: 24,
                    per_bucket_limit: 24,
                    ..ExactCompleteTurnProposalConfigV1::default()
                },
                max_total_potion_expenditures: None,
                beam_width: 8,
                max_turn_depth: 2,
            },
            Arc::new(crate::UniformCombatActionPolicy),
        );

        let report = session.advance(CompleteTurnProposalBeamQuantumV1 {
            additional_expanded_boundaries: 1,
            deadline: None,
        });
        let layer = report.layers.first().expect("root layer completed");

        assert!(layer.grounded_candidates > layer.agenda_frontier_candidates);
        assert_eq!(
            layer.agenda_frontier_candidates,
            report.counters.agenda_frontier_candidates
        );
        assert!(layer.unique_successors <= layer.agenda_frontier_candidates);
        assert!(layer.retained_successors <= layer.agenda_frontier_candidates);
    }

    #[test]
    fn split_quantums_resume_the_same_partial_layer_without_replay() {
        let mut combat = sts_core::test_support::blank_test_combat();
        let mut jaw_worm = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        jaw_worm.current_hp = 80;
        jaw_worm.max_hp = 80;
        combat.entities.monsters = vec![jaw_worm];
        combat.turn.turn_count = 1;
        combat.turn.energy = 2;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 11),
            CombatCard::for_scenario(CardId::Defend, 22),
        ];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let plan = public_combat_plan_v1(&public_combat_state_v1(&root.combat));
        let config = CompleteTurnProposalBeamSessionConfigV1 {
            proposal: ExactCompleteTurnProposalConfigV1 {
                max_inner_nodes: 32,
                max_end_states: 8,
                per_bucket_limit: 8,
                ..ExactCompleteTurnProposalConfigV1::default()
            },
            max_total_potion_expenditures: None,
            beam_width: 4,
            max_turn_depth: 8,
        };
        let policy: SharedCombatActionPolicy = Arc::new(crate::UniformCombatActionPolicy);
        let mut one_shot = CompleteTurnProposalBeamSessionV1::new(
            root.clone(),
            plan.clone(),
            config,
            policy.clone(),
        );
        let mut resumed = CompleteTurnProposalBeamSessionV1::new(root, plan, config, policy);

        let whole = one_shot.advance(CompleteTurnProposalBeamQuantumV1 {
            additional_expanded_boundaries: 6,
            deadline: None,
        });
        let partial = resumed.advance(CompleteTurnProposalBeamQuantumV1 {
            additional_expanded_boundaries: 2,
            deadline: None,
        });
        assert_eq!(
            partial.interruption,
            Some(CompleteTurnProposalBeamInterruptionV1::ExpandedBoundaries)
        );
        assert!(!resumed.frontier.is_empty());
        assert!(!resumed.next.is_empty());
        let continued = resumed.advance(CompleteTurnProposalBeamQuantumV1 {
            additional_expanded_boundaries: 4,
            deadline: None,
        });

        assert_eq!(continued.interruption, whole.interruption);
        assert_eq!(continued.counters, whole.counters);
        assert_eq!(continued.layers, whole.layers);
        assert_eq!(resumed.seen.len(), one_shot.seen.len());
        assert_eq!(
            pending_identities(&resumed.frontier),
            pending_identities(&one_shot.frontier)
        );
        assert_eq!(
            pending_identities(&resumed.next),
            pending_identities(&one_shot.next)
        );
    }

    #[test]
    fn beam_gives_each_live_root_lineage_one_seat_before_duplicate_seats() {
        let lane = CombatGuideLaneId::new(1);
        let states = vec![
            selection_state(1, 0, &[(lane, 100)]),
            selection_state(1, 1, &[(lane, 99)]),
            selection_state(1, 2, &[(lane, 98)]),
            selection_state(2, 3, &[(lane, 50)]),
            selection_state(3, 4, &[(lane, 40)]),
        ];

        let selected = select_independent_guide_beam(states, 3);
        let lineages = selected
            .iter()
            .map(|state| state.root_lineage)
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(lineages, [Some(1), Some(2), Some(3)].into_iter().collect());
    }

    #[test]
    fn independent_guides_keep_opposite_best_states_without_scalarization() {
        let progress = CombatGuideLaneId::new(1);
        let survival = CombatGuideLaneId::new(2);
        let states = vec![
            selection_state(1, 0, &[(progress, 10), (survival, 0)]),
            selection_state(2, 1, &[(progress, 0), (survival, 10)]),
            selection_state(3, 2, &[(progress, 6), (survival, 6)]),
        ];

        let selected = select_independent_guide_beam(states, 2);
        let lineages = selected
            .iter()
            .map(|state| state.root_lineage)
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(lineages, [Some(1), Some(2)].into_iter().collect());
    }

    fn selection_state(
        root_lineage: usize,
        stable_order: usize,
        guides: &[(CombatGuideLaneId, i32)],
    ) -> BeamStateV1 {
        let position = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            sts_core::test_support::blank_test_combat(),
        );
        let plan = public_combat_plan_v1(&public_combat_state_v1(&position.combat));
        BeamStateV1 {
            position,
            plan,
            actions: Vec::new(),
            potion_expenditures: 0,
            guides: guides
                .iter()
                .map(|(lane, rank)| CombatStateGuide::new(*lane, vec![*rank]))
                .collect(),
            stable_order,
            root_lineage: Some(root_lineage),
        }
    }

    fn pending_identities(
        states: impl IntoIterator<Item = impl std::borrow::Borrow<BeamStateV1>>,
    ) -> Vec<(CombatExactStateKey, PublicCombatPlanStateV1, u32)> {
        states
            .into_iter()
            .map(|state| {
                let state = state.borrow();
                (
                    combat_exact_state_key(&state.position.engine, &state.position.combat),
                    state.plan.clone(),
                    state.potion_expenditures,
                )
            })
            .collect()
    }
}
