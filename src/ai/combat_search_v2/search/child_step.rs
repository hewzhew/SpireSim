use std::time::{Duration, Instant};

use crate::sim::combat::CombatStepResult;

use super::super::*;
use super::loop_state::SearchLoopState;

pub(super) enum ChildStepOutcome {
    Stable(CombatStepResult),
    StepLimitReached,
    DeadlineReached,
}

pub(super) fn apply_child_step<S: CombatStepper>(
    loop_state: &mut SearchLoopState,
    position: &CombatPosition,
    input: &ClientInput,
    stepper: &S,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
) -> ChildStepOutcome {
    let step_started = Instant::now();
    let step = stepper.apply_to_stable(
        position,
        input.clone(),
        CombatStepLimits {
            max_engine_steps: config.max_engine_steps_per_action,
            deadline,
        },
    );
    observe_child_step(loop_state, &step, step_started.elapsed());
    if step.timed_out {
        // A timed-out transition is not an atomic child.  It may contain zero
        // engine steps or an unstable partial position, neither of which may
        // acquire an action trace or enter the concrete frontier.
        ChildStepOutcome::DeadlineReached
    } else if step.truncated {
        // A per-action step cap also yields an unstable partial transition.
        // It cannot be retried under the same cap without looping forever, so
        // the caller records the cut but must not materialize a fake child.
        ChildStepOutcome::StepLimitReached
    } else {
        ChildStepOutcome::Stable(step)
    }
}

fn observe_child_step(
    loop_state: &mut SearchLoopState,
    step: &CombatStepResult,
    elapsed: Duration,
) {
    loop_state.performance.engine_step_calls =
        loop_state.performance.engine_step_calls.saturating_add(1);
    loop_state.performance.engine_step_elapsed_us = loop_state
        .performance
        .engine_step_elapsed_us
        .saturating_add(elapsed.as_micros());
    if step.truncated && !step.timed_out {
        loop_state.record_engine_step_limit();
    }
    if step.timed_out {
        loop_state.mark_deadline_hit();
    }
}
