use std::sync::Arc;
use std::time::Instant;

use sts_combat_strategy::{
    combat_plan_turn_prefix_proposal_v1, CombatPlanPrefixServiceScopeV1, CombatPlanPrefixStepV1,
};
use sts_core::ai::combat_state_key::combat_exact_state_key;
use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal};
use sts_core::state::core::ClientInput;

use crate::policy::{normalized_probabilities, CombatPolicyChoice};
use crate::types::{
    supported_boundary, CompleteTurnOption, GenerationInterruption, ReplaySuccessorHash,
    TurnOptionAction,
};

use super::{deadline_reached, PlanPrefixAdvance, TurnOptionGeneratorSession};

impl TurnOptionGeneratorSession {
    /// Materializes one encounter-owned current-turn proposal before ordinary
    /// lazy generation at this exact turn boundary.
    ///
    /// The proposal waits until its complete bounded action allowance is
    /// available, so split deterministic quanta remain replay-equivalent. Each
    /// step is resolved against the current hand, checked on the ordinary legal
    /// surface, simulated exactly, and charged as generation work. Failure
    /// falls back to the untouched normal generator.
    pub(super) fn advance_plan_prefix_proposal(
        &mut self,
        stepper: &dyn CombatStepper,
        deadline: Option<Instant>,
        allow_root_eligible_proposal: bool,
        allow_continuation_only_proposal: bool,
    ) -> PlanPrefixAdvance {
        if self.plan_prefix_attempted {
            return PlanPrefixAdvance::NotServiced;
        }
        if !allow_root_eligible_proposal {
            return PlanPrefixAdvance::NotServiced;
        }
        let Some(proposal) = combat_plan_turn_prefix_proposal_v1(self.root.position()) else {
            self.plan_prefix_attempted = true;
            return PlanPrefixAdvance::NotServiced;
        };
        if proposal.service_scope == CombatPlanPrefixServiceScopeV1::ContinuationOnly
            && !allow_continuation_only_proposal
        {
            return PlanPrefixAdvance::NotServiced;
        }
        let required_work = proposal.steps.len();
        let transition_reservation = self.config.max_engine_steps_per_transition.max(1);
        let required_steps = required_work.saturating_mul(transition_reservation);
        if deadline_reached(deadline) {
            return PlanPrefixAdvance::Interrupted(GenerationInterruption::Deadline);
        }
        if self
            .granted
            .generation_work
            .saturating_sub(self.used.generation_work)
            < required_work
        {
            return PlanPrefixAdvance::Interrupted(GenerationInterruption::GenerationWorkBudget);
        }
        if self
            .granted
            .engine_steps
            .saturating_sub(self.used.engine_steps)
            < required_steps
        {
            return PlanPrefixAdvance::Interrupted(GenerationInterruption::EngineStepBudget);
        }

        self.plan_prefix_attempted = true;
        self.plan_prefix_attempts = self.plan_prefix_attempts.saturating_add(1);
        let mut position = self.root.position().clone();
        let mut actions = Vec::with_capacity(required_work);
        let mut negative_log_policy = 0.0f64;
        let mut trajectory_progress = self.initial_trajectory_progress.clone();

        for semantic_step in &proposal.steps {
            if deadline_reached(deadline) {
                self.plan_prefix_attempted = false;
                return PlanPrefixAdvance::Interrupted(GenerationInterruption::Deadline);
            }
            let Some(input) = resolve_plan_prefix_step(&position, semantic_step) else {
                self.plan_prefix_rejections = self.plan_prefix_rejections.saturating_add(1);
                return PlanPrefixAdvance::Serviced;
            };
            let surface =
                crate::automatic_action_surface::automatic_action_surface(stepper, &position);
            let choices = surface
                .atomic_actions
                .iter()
                .map(CombatPolicyChoice::Atomic)
                .chain(
                    surface
                        .selection_families
                        .iter()
                        .map(CombatPolicyChoice::StructuredSelection),
                )
                .collect::<Vec<_>>();
            let weights = self.policy.weights(&position, &choices);
            let weights = if weights.len() == choices.len() {
                weights
            } else {
                vec![1.0; choices.len()]
            };
            let probabilities =
                normalized_probabilities(weights, self.config.uniform_exploration_ppm);
            let Some(input_index) = surface
                .atomic_actions
                .iter()
                .position(|candidate| candidate == &input)
            else {
                self.plan_prefix_rejections = self.plan_prefix_rejections.saturating_add(1);
                return PlanPrefixAdvance::Serviced;
            };
            negative_log_policy -= probabilities[input_index].max(f64::MIN_POSITIVE).ln();

            self.used.generation_work = self.used.generation_work.saturating_add(1);
            let result = stepper.apply_to_stable(
                &position,
                input.clone(),
                CombatStepLimits {
                    max_engine_steps: transition_reservation,
                    deadline,
                },
            );
            self.used.engine_steps = self.used.engine_steps.saturating_add(result.engine_steps);
            if result.timed_out {
                self.plan_prefix_attempted = false;
                return PlanPrefixAdvance::Interrupted(GenerationInterruption::Deadline);
            }
            if result.truncated {
                self.plan_prefix_rejections = self.plan_prefix_rejections.saturating_add(1);
                return PlanPrefixAdvance::Serviced;
            }
            self.applied_action_transitions = self.applied_action_transitions.saturating_add(1);
            trajectory_progress = self.trajectory_query.as_ref().map(|query| {
                query.advance(
                    trajectory_progress
                        .as_ref()
                        .expect("query-enabled proposal retains progress"),
                    &position,
                    &input,
                    &result.position,
                )
            });
            let successor_key = Arc::new(combat_exact_state_key(
                &result.position.engine,
                &result.position.combat,
            ));
            actions.push(TurnOptionAction {
                input,
                expected_successor_hash: ReplaySuccessorHash::from_exact_key(successor_key),
                engine_steps: result.engine_steps,
            });
            position = result.position;
            if stepper.terminal(&position) != CombatTerminal::Unresolved {
                break;
            }
        }

        let Some(boundary) = supported_boundary(&self.root, &position, stepper.terminal(&position))
        else {
            self.plan_prefix_rejections = self.plan_prefix_rejections.saturating_add(1);
            return PlanPrefixAdvance::Serviced;
        };
        self.publish_completed(
            CompleteTurnOption::from_encounter_plan_prefix(
                self.root.exact_state_identity().clone(),
                actions,
                boundary,
                position,
                negative_log_policy,
            )
            .with_trajectory_progress(trajectory_progress),
        );
        self.plan_prefix_completed = self.plan_prefix_completed.saturating_add(1);
        PlanPrefixAdvance::Serviced
    }
}

fn resolve_plan_prefix_step(
    position: &CombatPosition,
    step: &CombatPlanPrefixStepV1,
) -> Option<ClientInput> {
    match step {
        CombatPlanPrefixStepV1::PlayCard {
            card_lineage,
            target,
        } => position
            .combat
            .zones
            .hand
            .iter()
            .position(|card| card.lineage == *card_lineage)
            .map(|card_index| ClientInput::PlayCard {
                card_index,
                target: *target,
            }),
        CombatPlanPrefixStepV1::EndTurn => Some(ClientInput::EndTurn),
    }
}
