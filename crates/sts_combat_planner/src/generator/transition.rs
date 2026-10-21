use std::sync::Arc;
use std::time::Instant;

use sts_core::ai::combat_state_key::combat_exact_state_key;
use sts_core::sim::combat::{CombatStepLimits, CombatStepper};
use sts_core::state::core::ClientInput;

use crate::types::{supported_boundary, CompleteTurnOption, TurnOptionGenerationGapKind};

use super::scheduling::GeneratorWorkPriority;
use super::{
    elapsed_nanos_u64, ActionTransitionWork, GeneratorWork, IndexedExactStateKey,
    PartialTurnOption, PendingActionTrace, TurnOptionGeneratorSession,
    DETAIL_TIMING_SAMPLE_INTERVAL,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ActionTransitionStatus {
    Consumed,
    TimedOut,
}

impl TurnOptionGeneratorSession {
    pub(super) fn apply_action_transition(
        &mut self,
        stepper: &dyn CombatStepper,
        action: ActionTransitionWork,
        transition_reservation: usize,
        deadline: Option<Instant>,
    ) -> ActionTransitionStatus {
        let simulation_started = Instant::now();
        if stepper
            .choice_for_legal_input(&action.parent.position, &action.input)
            .is_none()
        {
            self.record_gap(
                TurnOptionGenerationGapKind::GeneratedInputRejected,
                &action.parent,
            );
            self.transition_simulation_elapsed_ns = self
                .transition_simulation_elapsed_ns
                .saturating_add(elapsed_nanos_u64(simulation_started));
            return ActionTransitionStatus::Consumed;
        }
        let result = stepper.apply_to_stable(
            &action.parent.position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: transition_reservation,
                deadline,
            },
        );
        self.used.engine_steps = self.used.engine_steps.saturating_add(result.engine_steps);
        self.transition_simulation_elapsed_ns = self
            .transition_simulation_elapsed_ns
            .saturating_add(elapsed_nanos_u64(simulation_started));
        if result.timed_out {
            return ActionTransitionStatus::TimedOut;
        }
        if result.truncated {
            self.record_gap(
                TurnOptionGenerationGapKind::TransitionStepLimit,
                &action.parent,
            );
            return ActionTransitionStatus::Consumed;
        }

        self.applied_action_transitions = self.applied_action_transitions.saturating_add(1);
        let detail_timing_scale = detail_timing_scale(self.applied_action_transitions);
        let identity_started = Instant::now();
        let key_build_started = detail_timing_scale.map(|_| Instant::now());
        let key = combat_exact_state_key(&result.position.engine, &result.position.combat);
        self.transition_key_build_elapsed_ns =
            self.transition_key_build_elapsed_ns
                .saturating_add(sampled_elapsed_nanos_u64(
                    key_build_started,
                    detail_timing_scale,
                ));
        let key_index_started = detail_timing_scale.map(|_| Instant::now());
        let successor_key = Arc::new(key);
        let successor_potion_expenditures = action
            .parent
            .potion_expenditures
            .saturating_add(u32::from(is_potion_expenditure(&action.input)));
        let successor_trajectory_progress = self.trajectory_query.as_ref().map(|query| {
            query.advance(
                action
                    .parent
                    .trajectory_progress
                    .as_ref()
                    .expect("query-enabled partial retains progress"),
                &action.parent.position,
                &action.input,
                &result.position,
            )
        });
        let indexed_key = IndexedExactStateKey::from_arc(
            successor_key.clone(),
            self.max_potion_expenditures
                .map(|_| successor_potion_expenditures),
            successor_trajectory_progress.clone(),
        );
        self.transition_key_index_elapsed_ns =
            self.transition_key_index_elapsed_ns
                .saturating_add(sampled_elapsed_nanos_u64(
                    key_index_started,
                    detail_timing_scale,
                ));
        self.transition_identity_elapsed_ns = self
            .transition_identity_elapsed_ns
            .saturating_add(elapsed_nanos_u64(identity_started));
        let admission_started = Instant::now();
        let seen_started = detail_timing_scale.map(|_| Instant::now());
        let unseen = self.seen.insert(indexed_key);
        self.transition_seen_elapsed_ns = self
            .transition_seen_elapsed_ns
            .saturating_add(sampled_elapsed_nanos_u64(seen_started, detail_timing_scale));
        let publish_started = Instant::now();
        if unseen {
            let trace_node_started = detail_timing_scale.map(|_| Instant::now());
            let partial = PartialTurnOption {
                position: result.position,
                trace: Some(Arc::new(PendingActionTrace {
                    parent: action.parent.trace.clone(),
                    input: action.input,
                    successor_key,
                    engine_steps: result.engine_steps,
                    depth: action.parent.action_depth().saturating_add(1),
                })),
                atomic_depth: action.atomic_depth,
                negative_log_policy: action.negative_log_policy,
                potion_expenditures: successor_potion_expenditures,
                generation_guides: None,
                trajectory_progress: successor_trajectory_progress,
            };
            self.transition_publish_trace_node_elapsed_ns = self
                .transition_publish_trace_node_elapsed_ns
                .saturating_add(sampled_elapsed_nanos_u64(
                    trace_node_started,
                    detail_timing_scale,
                ));
            let boundary_started = detail_timing_scale.map(|_| Instant::now());
            let terminal = stepper.terminal(&partial.position);
            let boundary = supported_boundary(&self.root, &partial.position, terminal);
            self.transition_publish_boundary_elapsed_ns =
                self.transition_publish_boundary_elapsed_ns.saturating_add(
                    sampled_elapsed_nanos_u64(boundary_started, detail_timing_scale),
                );
            if let Some(boundary) = boundary {
                // A stable atomic transition has already paid the simulator
                // cost and reached the requested exact boundary. Publish it
                // now instead of routing it back through the private atomic
                // agenda.
                let trace_started = Instant::now();
                let actions = partial.materialize_actions();
                self.transition_trace_elapsed_ns = self
                    .transition_trace_elapsed_ns
                    .saturating_add(elapsed_nanos_u64(trace_started));
                // These mutually exclusive coarse timers are exhaustive. The
                // branch costs are heavy-tailed enough that sparse estimates
                // are misleading; nested hot-path timers remain sampled.
                let complete_started = Instant::now();
                self.publish_completed(
                    CompleteTurnOption::new(
                        self.root.exact_state_identity().clone(),
                        actions,
                        boundary,
                        partial.position,
                        partial.negative_log_policy,
                    )
                    .with_trajectory_progress(partial.trajectory_progress),
                );
                self.transition_publish_complete_elapsed_ns = self
                    .transition_publish_complete_elapsed_ns
                    .saturating_add(elapsed_nanos_u64(complete_started));
            } else {
                let priority = GeneratorWorkPriority::for_path(
                    action.atomic_depth,
                    action.negative_log_policy,
                );
                let push_started = Instant::now();
                let (_, push_timing) = self.push_work_measured(
                    GeneratorWork::Expand(Arc::new(partial)),
                    priority,
                    detail_timing_scale.is_some(),
                );
                self.transition_publish_push_elapsed_ns = self
                    .transition_publish_push_elapsed_ns
                    .saturating_add(elapsed_nanos_u64(push_started));
                let scale = detail_timing_scale.unwrap_or(0);
                self.transition_publish_guide_elapsed_ns = self
                    .transition_publish_guide_elapsed_ns
                    .saturating_add(push_timing.guide_elapsed_ns.saturating_mul(scale));
                self.transition_publish_retain_elapsed_ns = self
                    .transition_publish_retain_elapsed_ns
                    .saturating_add(push_timing.retain_elapsed_ns.saturating_mul(scale));
                self.transition_publish_agenda_elapsed_ns = self
                    .transition_publish_agenda_elapsed_ns
                    .saturating_add(push_timing.agenda_elapsed_ns.saturating_mul(scale));
            }
        } else {
            self.duplicate_exact_successors = self.duplicate_exact_successors.saturating_add(1);
        }
        self.transition_publish_elapsed_ns = self
            .transition_publish_elapsed_ns
            .saturating_add(elapsed_nanos_u64(publish_started));
        self.transition_admission_elapsed_ns = self
            .transition_admission_elapsed_ns
            .saturating_add(elapsed_nanos_u64(admission_started));
        ActionTransitionStatus::Consumed
    }
}

fn sampled_elapsed_nanos_u64(started: Option<Instant>, scale: Option<u64>) -> u64 {
    started
        .zip(scale)
        .map(|(started, scale)| elapsed_nanos_u64(started).saturating_mul(scale))
        .unwrap_or(0)
}

pub(super) fn detail_timing_scale(transition_ordinal: usize) -> Option<u64> {
    debug_assert!(DETAIL_TIMING_SAMPLE_INTERVAL.is_power_of_two());
    // SplitMix64 finalizer: deterministic and cheap, while avoiding a fixed
    // relationship between the sample and canonical action-family order.
    let mut mixed = transition_ordinal as u64;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^= mixed >> 31;
    ((mixed & (DETAIL_TIMING_SAMPLE_INTERVAL as u64 - 1)) == 0)
        .then_some(DETAIL_TIMING_SAMPLE_INTERVAL as u64)
}

pub(super) fn is_potion_expenditure(input: &ClientInput) -> bool {
    matches!(
        input,
        ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
    )
}
