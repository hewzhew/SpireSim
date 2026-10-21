use sts_core::ai::combat_state_key::combat_exact_state_key;
use sts_core::sim::combat::CombatPosition;
use sts_core::state::core::ClientInput;

use crate::policy::{CombatGuideLaneId, CombatStateGuideRank};
use crate::types::{
    CombatPlanningCounters, TurnOptionGenerationDiagnostics, TurnOptionGenerationGap,
};

use super::guide_frontier::guide_frontier_length_census;
use super::{GeneratorWork, GeneratorWorkHandle, TurnOptionGeneratorSession};

#[derive(Clone, Debug)]
pub(crate) struct RetainedGuidePromise {
    pub(crate) rank: CombatStateGuideRank,
    pub(crate) atomic_depth: usize,
}

/// Read-only lifecycle information for one exact atomic transition which is
/// still waiting inside a lazy turn generator. This distinguishes a missing
/// action from an action hidden behind a resumable sibling cursor.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveActionTransitionSnapshot {
    pub candidate_ordinal: usize,
    pub remaining_candidate_count: usize,
    pub conditional_probability: f64,
    pub candidate_negative_log_policy: f64,
    pub cursor_negative_log_policy: f64,
    pub anchor_queue_rank: usize,
    pub guide_queue_ranks: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TurnOptionGeneratorTiming {
    pub atomic_expand_elapsed_ns: u64,
    pub transition_simulation_elapsed_ns: u64,
    pub transition_identity_elapsed_ns: u64,
    pub transition_key_build_elapsed_ns: u64,
    pub transition_key_index_elapsed_ns: u64,
    pub transition_admission_elapsed_ns: u64,
    pub transition_trace_elapsed_ns: u64,
    pub transition_seen_elapsed_ns: u64,
    pub transition_publish_elapsed_ns: u64,
    pub transition_publish_trace_node_elapsed_ns: u64,
    pub transition_publish_boundary_elapsed_ns: u64,
    pub transition_publish_complete_elapsed_ns: u64,
    pub transition_publish_push_elapsed_ns: u64,
    pub transition_publish_guide_elapsed_ns: u64,
    pub transition_publish_retain_elapsed_ns: u64,
    pub transition_publish_agenda_elapsed_ns: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TurnOptionGeneratorStorageSnapshot {
    pub(crate) finished: bool,
    pub(crate) live_work_items: usize,
    pub(crate) work_slots: usize,
    pub(crate) work_capacity: usize,
    pub(crate) work_sequence_capacity: usize,
    pub(crate) guide_entry_count_capacity: usize,
    pub(crate) free_work_slots: usize,
    pub(crate) free_work_capacity: usize,
    pub(crate) seen_states: usize,
    pub(crate) seen_capacity: usize,
    pub(crate) anchor_entries: usize,
    pub(crate) live_anchor_entries: usize,
    pub(crate) anchor_capacity: usize,
    pub(crate) guide_frontiers: usize,
    pub(crate) empty_guide_frontiers: usize,
    pub(crate) single_entry_guide_frontiers: usize,
    pub(crate) two_to_four_entry_guide_frontiers: usize,
    pub(crate) five_to_sixteen_entry_guide_frontiers: usize,
    pub(crate) larger_guide_frontiers: usize,
    pub(crate) maximum_guide_frontier_entries: usize,
    pub(crate) guide_entries: usize,
    pub(crate) live_guide_entries: usize,
    pub(crate) guide_capacity: usize,
    pub(crate) scheduled_round_entries: usize,
    pub(crate) live_scheduled_round_entries: usize,
    pub(crate) scheduled_round_capacity: usize,
    pub(crate) completed_options: usize,
    pub(crate) completed_capacity: usize,
    pub(crate) gaps: usize,
    pub(crate) gaps_capacity: usize,
    pub(crate) scheduling_rebuilds: usize,
    pub(crate) reused_work_slots: usize,
    pub(crate) reclaimed_anchor_entries: usize,
    pub(crate) reclaimed_guide_entries: usize,
}

impl TurnOptionGeneratorSession {
    /// Diagnostic membership for one exact partial-turn position. `seen`
    /// records both live and already-expanded states, so this distinguishes a
    /// prefix that was never generated from one that was generated and later
    /// consumed. It does not change retention or scheduling.
    pub fn has_seen_exact_position(&self, position: &CombatPosition) -> bool {
        let key = combat_exact_state_key(&position.engine, &position.combat);
        self.seen.iter().any(|seen| *seen.key == key)
    }

    /// Counts still-live generator work rooted at one exact partial-turn
    /// position as `(expand, pending_atomic_actions, structured_selection)`.
    /// Atomic siblings share one resumable cursor, but the second component
    /// remains a count of concrete transitions so membership reports stay
    /// comparable. This is a diagnostic view only.
    pub fn live_work_counts_at_exact_position(
        &self,
        position: &CombatPosition,
    ) -> (usize, usize, usize) {
        let target = combat_exact_state_key(&position.engine, &position.combat);
        self.work
            .iter()
            .filter_map(Option::as_ref)
            .filter(|work| {
                combat_exact_state_key(&work.position().engine, &work.position().combat) == target
            })
            .fold((0, 0, 0), |mut counts, work| {
                match work {
                    GeneratorWork::Expand(_) => counts.0 += 1,
                    GeneratorWork::AtomicActions(actions) => {
                        counts.1 += actions.remaining_candidate_count()
                    }
                    GeneratorWork::StructuredSelection(_) => counts.2 += 1,
                }
                counts
            })
    }

    /// Locates an exact pending action without changing queue order or work.
    /// `candidate_ordinal` is one-based within the cursor's still-unapplied
    /// members; queue ranks refer to the shared cursor work item.
    pub fn live_action_transition_snapshot(
        &self,
        position: &CombatPosition,
        input: &ClientInput,
    ) -> Option<LiveActionTransitionSnapshot> {
        let target = combat_exact_state_key(&position.engine, &position.combat);
        self.work
            .iter()
            .enumerate()
            .find_map(|(work_id, work)| match work.as_ref()? {
                GeneratorWork::AtomicActions(actions)
                    if combat_exact_state_key(
                        &actions.parent.position.engine,
                        &actions.parent.position.combat,
                    ) == target =>
                {
                    let remaining = &actions.candidates[actions.next_candidate..];
                    let (offset, candidate) = remaining
                        .iter()
                        .enumerate()
                        .find(|(_, candidate)| candidate.input == *input)?;
                    let cursor_priority = actions.priority()?;
                    let (anchor_queue_rank, guide_queue_ranks) =
                        self.live_queue_ranks_for_work_handle(self.live_work_handle(work_id)?)?;
                    Some(LiveActionTransitionSnapshot {
                        candidate_ordinal: offset.saturating_add(1),
                        remaining_candidate_count: remaining.len(),
                        conditional_probability: candidate.conditional_probability,
                        candidate_negative_log_policy: candidate.negative_log_policy,
                        cursor_negative_log_policy: cursor_priority.negative_log_policy,
                        anchor_queue_rank,
                        guide_queue_ranks,
                    })
                }
                GeneratorWork::StructuredSelection(selection)
                    if combat_exact_state_key(
                        &selection.parent.position.engine,
                        &selection.parent.position.combat,
                    ) == target =>
                {
                    // Inspect only the ready member; do not materialize the family
                    // merely to find an arbitrary later selection.
                    let action = selection.current_transition()?;
                    if action.input != *input {
                        return None;
                    }
                    let priority = selection.priority()?;
                    let (anchor_queue_rank, guide_queue_ranks) =
                        self.live_queue_ranks_for_work_handle(self.live_work_handle(work_id)?)?;
                    Some(LiveActionTransitionSnapshot {
                        candidate_ordinal: 1,
                        remaining_candidate_count: selection.cursor.remaining_input_count(),
                        conditional_probability: (selection.parent.negative_log_policy
                            - action.negative_log_policy)
                            .exp(),
                        candidate_negative_log_policy: action.negative_log_policy,
                        cursor_negative_log_policy: priority.negative_log_policy,
                        anchor_queue_rank,
                        guide_queue_ranks,
                    })
                }
                _ => None,
            })
    }

    /// One-based live queue ranks for an exact pending expansion, returned as
    /// `(anchor_rank, guide_ranks)`. Lower is scheduled earlier within that
    /// view. This exposes queue placement without mutating queues.
    pub fn live_expand_queue_ranks_at_exact_position(
        &self,
        position: &CombatPosition,
    ) -> Option<(usize, Vec<usize>)> {
        let target_key = combat_exact_state_key(&position.engine, &position.combat);
        let target_work_id = self
            .work
            .iter()
            .enumerate()
            .find_map(|(work_id, work)| match work.as_ref()? {
                GeneratorWork::Expand(partial)
                    if combat_exact_state_key(
                        &partial.position.engine,
                        &partial.position.combat,
                    ) == target_key =>
                {
                    Some(work_id)
                }
                _ => None,
            })?;
        self.live_queue_ranks_for_work_handle(self.live_work_handle(target_work_id)?)
    }

    fn live_queue_ranks_for_work_handle(
        &self,
        target: GeneratorWorkHandle,
    ) -> Option<(usize, Vec<usize>)> {
        let target_anchor = self.anchor_frontier.iter().find(|entry| {
            entry.work_id == target.work_id && entry.sequence_id == target.sequence_id
        })?;
        let anchor_rank = 1 + self
            .anchor_frontier
            .iter()
            .filter(|entry| {
                self.is_live_work_handle(GeneratorWorkHandle {
                    work_id: entry.work_id,
                    sequence_id: entry.sequence_id,
                })
            })
            .filter(|entry| *entry > target_anchor)
            .count();
        let guide_ranks = self
            .guided_frontiers
            .iter()
            .map(|frontier| {
                let Some(target) = frontier.entries.iter().find(|entry| {
                    entry.work_id == target.work_id && entry.sequence_id == target.sequence_id
                }) else {
                    return 0;
                };
                1 + frontier
                    .entries
                    .iter()
                    .filter(|entry| {
                        self.is_live_work_handle(GeneratorWorkHandle {
                            work_id: entry.work_id,
                            sequence_id: entry.sequence_id,
                        })
                    })
                    .filter(|entry| *entry > target)
                    .count()
            })
            .collect();
        Some((anchor_rank, guide_ranks))
    }

    pub fn gaps(&self) -> &[TurnOptionGenerationGap] {
        &self.gaps
    }

    pub fn counters(&self) -> CombatPlanningCounters {
        self.used
    }

    pub fn atomic_state_expansions(&self) -> usize {
        self.atomic_state_expansions
    }

    pub fn anchor_work_pops(&self) -> usize {
        self.anchor_work_pops
    }

    pub fn guided_work_pops(&self) -> usize {
        self.guided_work_pops
    }

    pub(crate) fn retained_guide_lanes(&self) -> Vec<CombatGuideLaneId> {
        self.guided_frontiers
            .iter()
            .filter(|frontier| {
                frontier.entries.iter().any(|entry| {
                    self.is_live_work_handle(GeneratorWorkHandle {
                        work_id: entry.work_id,
                        sequence_id: entry.sequence_id,
                    })
                })
            })
            .map(|frontier| frontier.lane)
            .collect()
    }

    pub fn granted_budget(&self) -> CombatPlanningCounters {
        self.granted
    }

    pub fn retained_work_items(&self) -> usize {
        self.live_work_items
    }

    pub fn diagnostics(&self) -> TurnOptionGenerationDiagnostics {
        TurnOptionGenerationDiagnostics {
            applied_action_transitions: self.applied_action_transitions,
            unique_successor_states: self.seen.len().saturating_sub(1),
            duplicate_exact_successors: self.duplicate_exact_successors,
            completed_turn_options: self.total_completed_options,
            plan_prefix_attempts: self.plan_prefix_attempts,
            plan_prefix_completed: self.plan_prefix_completed,
            plan_prefix_rejections: self.plan_prefix_rejections,
        }
    }

    pub(crate) fn timing(&self) -> TurnOptionGeneratorTiming {
        TurnOptionGeneratorTiming {
            atomic_expand_elapsed_ns: self.atomic_expand_elapsed_ns,
            transition_simulation_elapsed_ns: self.transition_simulation_elapsed_ns,
            transition_identity_elapsed_ns: self.transition_identity_elapsed_ns,
            transition_key_build_elapsed_ns: self.transition_key_build_elapsed_ns,
            transition_key_index_elapsed_ns: self.transition_key_index_elapsed_ns,
            transition_admission_elapsed_ns: self.transition_admission_elapsed_ns,
            transition_trace_elapsed_ns: self.transition_trace_elapsed_ns,
            transition_seen_elapsed_ns: self.transition_seen_elapsed_ns,
            transition_publish_elapsed_ns: self.transition_publish_elapsed_ns,
            transition_publish_trace_node_elapsed_ns: self.transition_publish_trace_node_elapsed_ns,
            transition_publish_boundary_elapsed_ns: self.transition_publish_boundary_elapsed_ns,
            transition_publish_complete_elapsed_ns: self.transition_publish_complete_elapsed_ns,
            transition_publish_push_elapsed_ns: self.transition_publish_push_elapsed_ns,
            transition_publish_guide_elapsed_ns: self.transition_publish_guide_elapsed_ns,
            transition_publish_retain_elapsed_ns: self.transition_publish_retain_elapsed_ns,
            transition_publish_agenda_elapsed_ns: self.transition_publish_agenda_elapsed_ns,
        }
    }

    pub(crate) fn storage_snapshot(&self) -> TurnOptionGeneratorStorageSnapshot {
        let live_anchor_entries = self
            .anchor_frontier
            .iter()
            .filter(|entry| {
                self.is_live_work_handle(GeneratorWorkHandle {
                    work_id: entry.work_id,
                    sequence_id: entry.sequence_id,
                })
            })
            .count();
        let live_guide_entries = self
            .guided_frontiers
            .iter()
            .flat_map(|frontier| frontier.entries.iter())
            .filter(|entry| {
                self.is_live_work_handle(GeneratorWorkHandle {
                    work_id: entry.work_id,
                    sequence_id: entry.sequence_id,
                })
            })
            .count();
        let (guide_frontier_lengths, maximum_guide_frontier_entries) =
            guide_frontier_length_census(&self.guided_frontiers);
        let live_scheduled_round_entries = self
            .scheduled_round
            .iter()
            .filter(|(_, handle)| self.is_live_work_handle(*handle))
            .count();
        debug_assert_eq!(self.work_sequence_ids.len(), self.work.len());
        debug_assert_eq!(self.guide_entries_per_work.len(), self.work.len());
        debug_assert_eq!(
            self.free_work_ids.len(),
            self.work.len().saturating_sub(self.live_work_items)
        );
        debug_assert!(self
            .free_work_ids
            .iter()
            .all(|work_id| self.work.get(*work_id).is_some_and(Option::is_none)));
        debug_assert_eq!(self.live_guide_entries, live_guide_entries);
        TurnOptionGeneratorStorageSnapshot {
            finished: self.is_finished(),
            live_work_items: self.live_work_items,
            work_slots: self.work.len(),
            work_capacity: self.work.capacity(),
            work_sequence_capacity: self.work_sequence_ids.capacity(),
            guide_entry_count_capacity: self.guide_entries_per_work.capacity(),
            free_work_slots: self.free_work_ids.len(),
            free_work_capacity: self.free_work_ids.capacity(),
            seen_states: self.seen.len(),
            seen_capacity: self.seen.capacity(),
            anchor_entries: self.anchor_frontier.len(),
            live_anchor_entries,
            anchor_capacity: self.anchor_frontier.capacity(),
            guide_frontiers: self.guided_frontiers.len(),
            empty_guide_frontiers: guide_frontier_lengths[0],
            single_entry_guide_frontiers: guide_frontier_lengths[1],
            two_to_four_entry_guide_frontiers: guide_frontier_lengths[2],
            five_to_sixteen_entry_guide_frontiers: guide_frontier_lengths[3],
            larger_guide_frontiers: guide_frontier_lengths[4],
            maximum_guide_frontier_entries,
            guide_entries: self
                .guided_frontiers
                .iter()
                .map(|frontier| frontier.entries.len())
                .sum(),
            live_guide_entries,
            guide_capacity: self
                .guided_frontiers
                .iter()
                .map(|frontier| frontier.entries.capacity())
                .sum(),
            scheduled_round_entries: self.scheduled_round.len(),
            live_scheduled_round_entries,
            scheduled_round_capacity: self.scheduled_round.capacity(),
            completed_options: self.completed.len(),
            completed_capacity: self.completed.capacity(),
            gaps: self.gaps.len(),
            gaps_capacity: self.gaps.capacity(),
            scheduling_rebuilds: self.scheduling_rebuilds,
            reused_work_slots: self.reused_work_slots,
            reclaimed_anchor_entries: self.reclaimed_anchor_entries,
            reclaimed_guide_entries: self.reclaimed_guide_entries,
        }
    }

    pub(crate) fn best_retained_path_bound_snapshot(&self) -> Option<(usize, f64)> {
        self.anchor_frontier
            .iter()
            .filter(|entry| {
                self.is_live_work_handle(GeneratorWorkHandle {
                    work_id: entry.work_id,
                    sequence_id: entry.sequence_id,
                })
            })
            .min_by(|left, right| {
                left.priority
                    .levin_log_priority
                    .total_cmp(&right.priority.levin_log_priority)
                    .then_with(|| {
                        left.priority
                            .negative_log_policy
                            .total_cmp(&right.priority.negative_log_policy)
                    })
                    .then_with(|| left.priority.atomic_depth.cmp(&right.priority.atomic_depth))
            })
            .map(|entry| {
                (
                    entry.priority.atomic_depth,
                    entry.priority.negative_log_policy,
                )
            })
    }

    pub(crate) fn best_retained_guide_promise_snapshot(
        &self,
        lane: CombatGuideLaneId,
    ) -> Option<RetainedGuidePromise> {
        self.guided_frontiers
            .iter()
            .find(|frontier| frontier.lane == lane)?
            .entries
            .iter()
            .filter(|entry| {
                self.is_live_work_handle(GeneratorWorkHandle {
                    work_id: entry.work_id,
                    sequence_id: entry.sequence_id,
                })
            })
            .max()
            .map(|entry| RetainedGuidePromise {
                rank: entry.guide_rank.clone(),
                atomic_depth: entry.anchor_priority.atomic_depth,
            })
    }
}
