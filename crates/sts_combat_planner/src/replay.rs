use std::time::Instant;

use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper};

use super::types::{
    exact_hash, supported_boundary, CombatDecisionRoot, CompleteTurnOption,
    CompleteTurnOptionBoundary,
};

#[derive(Clone, Copy, Debug)]
pub struct ReplayLimits {
    pub max_engine_steps: usize,
    pub deadline: Option<Instant>,
}

impl ReplayLimits {
    pub fn deterministic(max_engine_steps: usize) -> Self {
        Self {
            max_engine_steps,
            deadline: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReplayError {
    RootFingerprintMismatch,
    EngineStepBudget,
    Deadline,
    IllegalInput {
        action_index: usize,
    },
    TransitionStepLimit {
        action_index: usize,
    },
    SuccessorMismatch {
        action_index: usize,
    },
    UnsupportedFinalBoundary,
    BoundaryMismatch {
        expected: CompleteTurnOptionBoundary,
        actual: CompleteTurnOptionBoundary,
    },
    FinalSuccessorMismatch,
}

#[derive(Clone, Debug)]
pub struct VerifiedTurnOptionReplay {
    pub position: CombatPosition,
    pub boundary: CompleteTurnOptionBoundary,
    pub engine_steps: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayFailure {
    pub error: ReplayError,
    pub engine_steps: usize,
}

pub fn replay_turn_option(
    root: &CombatDecisionRoot,
    option: &CompleteTurnOption,
    stepper: &dyn CombatStepper,
    limits: ReplayLimits,
) -> Result<VerifiedTurnOptionReplay, ReplayError> {
    replay_turn_option_observed(root, option, stepper, limits).map_err(|failure| failure.error)
}

pub(crate) fn replay_turn_option_observed(
    root: &CombatDecisionRoot,
    option: &CompleteTurnOption,
    stepper: &dyn CombatStepper,
    limits: ReplayLimits,
) -> Result<VerifiedTurnOptionReplay, ReplayFailure> {
    if root.exact_state_hash() != option.root_exact_state_hash() {
        return Err(failure(ReplayError::RootFingerprintMismatch, 0));
    }
    let mut position = root.position().clone();
    let mut engine_steps = 0usize;

    for (action_index, action) in option.actions().iter().enumerate() {
        if limits
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(failure(ReplayError::Deadline, engine_steps));
        }
        if stepper
            .choice_for_legal_input(&position, &action.input)
            .is_none()
        {
            return Err(failure(
                ReplayError::IllegalInput { action_index },
                engine_steps,
            ));
        }
        let remaining = limits.max_engine_steps.saturating_sub(engine_steps);
        let expected_steps = action.engine_steps.max(1);
        if remaining < expected_steps {
            return Err(failure(ReplayError::EngineStepBudget, engine_steps));
        }
        let result = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: expected_steps,
                deadline: limits.deadline,
            },
        );
        engine_steps = engine_steps.saturating_add(result.engine_steps);
        if result.timed_out {
            return Err(failure(ReplayError::Deadline, engine_steps));
        }
        if result.truncated {
            return Err(failure(
                ReplayError::TransitionStepLimit { action_index },
                engine_steps,
            ));
        }
        if exact_hash(&result.position) != action.expected_successor_hash.as_str() {
            return Err(failure(
                ReplayError::SuccessorMismatch { action_index },
                engine_steps,
            ));
        }
        position = result.position;
    }

    let Some(boundary) = supported_boundary(root, &position, stepper.terminal(&position)) else {
        return Err(failure(ReplayError::UnsupportedFinalBoundary, engine_steps));
    };
    if boundary != option.boundary() {
        return Err(failure(
            ReplayError::BoundaryMismatch {
                expected: option.boundary(),
                actual: boundary,
            },
            engine_steps,
        ));
    }
    if position != *option.exact_successor() {
        return Err(failure(ReplayError::FinalSuccessorMismatch, engine_steps));
    }
    Ok(VerifiedTurnOptionReplay {
        position,
        boundary,
        engine_steps,
    })
}

fn failure(error: ReplayError, engine_steps: usize) -> ReplayFailure {
    ReplayFailure {
        error,
        engine_steps,
    }
}
