use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper};
use sts_core::state::core::ClientInput;

use crate::types::{exact_hash, TurnOptionAction};

#[derive(Clone, Debug)]
pub struct ExactAtomicWitness {
    pub actions: Vec<TurnOptionAction>,
    pub final_position: CombatPosition,
    pub negative_log_policy: f64,
    pub replay_engine_steps: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AtomicWitnessReplayError {
    IllegalInput { action_index: usize },
    TransitionStepLimit { action_index: usize },
    SuccessorMismatch { action_index: usize },
}

/// Materializes a caller-selected public input line as replay-exact actions.
///
/// This is a diagnostic composition boundary, not search or policy authority.
/// Every input is checked against the ordinary legal surface and every
/// successor receives its exact identity before the line is returned.
pub fn materialize_exact_action_line(
    stepper: &dyn CombatStepper,
    root: &CombatPosition,
    inputs: &[ClientInput],
    max_engine_steps_per_transition: usize,
) -> Result<ExactAtomicWitness, AtomicWitnessReplayError> {
    let mut position = root.clone();
    let mut actions = Vec::with_capacity(inputs.len());
    let mut replay_engine_steps = 0usize;
    for (action_index, input) in inputs.iter().enumerate() {
        if !stepper.is_legal_action(&position, input) {
            return Err(AtomicWitnessReplayError::IllegalInput { action_index });
        }
        let result = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition.max(1),
                deadline: None,
            },
        );
        replay_engine_steps = replay_engine_steps.saturating_add(result.engine_steps);
        if result.truncated || result.timed_out {
            return Err(AtomicWitnessReplayError::TransitionStepLimit { action_index });
        }
        actions.push(TurnOptionAction {
            input: input.clone(),
            expected_successor_hash: exact_hash(&result.position).into(),
            engine_steps: result.engine_steps,
        });
        position = result.position;
    }
    Ok(ExactAtomicWitness {
        actions,
        final_position: position,
        negative_log_policy: 0.0,
        replay_engine_steps,
    })
}

/// Replay an exact action prefix, including a prefix that stops within a turn.
/// Legality and every recorded successor are checked; this grants no terminal
/// or whole-combat witness status to the returned position.
pub fn replay_atomic_actions(
    stepper: &dyn CombatStepper,
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    max_engine_steps_per_transition: usize,
) -> Result<(CombatPosition, usize), AtomicWitnessReplayError> {
    let mut position = root.clone();
    let mut engine_steps = 0usize;
    for (action_index, action) in actions.iter().enumerate() {
        if !stepper.is_legal_action(&position, &action.input) {
            return Err(AtomicWitnessReplayError::IllegalInput { action_index });
        }
        let result = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        engine_steps = engine_steps.saturating_add(result.engine_steps);
        if result.truncated {
            return Err(AtomicWitnessReplayError::TransitionStepLimit { action_index });
        }
        if exact_hash(&result.position) != action.expected_successor_hash.as_str() {
            return Err(AtomicWitnessReplayError::SuccessorMismatch { action_index });
        }
        position = result.position;
    }
    Ok((position, engine_steps))
}
