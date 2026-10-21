use std::time::Instant;

use super::super::*;
use super::loop_state::SearchLoopState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ChildDominanceOutcome {
    Continue,
    Pruned,
}

pub(super) fn apply_child_dominance_gate(
    loop_state: &mut SearchLoopState,
    turn_local_dominance: &mut TurnLocalDominanceStateObservation,
    child: &SearchNode,
    truncated: bool,
) -> ChildDominanceOutcome {
    let started = Instant::now();
    if !truncated && turn_local_dominance.observe_child(child) {
        loop_state.record_turn_local_dominance_prune();
        loop_state.performance.child_bookkeeping_elapsed_us = loop_state
            .performance
            .child_bookkeeping_elapsed_us
            .saturating_add(started.elapsed().as_micros());
        return ChildDominanceOutcome::Pruned;
    }
    ChildDominanceOutcome::Continue
}
