use std::time::Instant;

use super::super::*;
use super::loop_state::SearchLoopState;

pub(super) fn enqueue_child_or_remember_leaf(
    loop_state: &mut SearchLoopState,
    child: SearchNode,
    truncated: bool,
) {
    let started = Instant::now();
    loop_state.record_first_generated_win_if_needed(&child);

    if !truncated {
        loop_state.push_frontier(child);
    } else {
        loop_state.record_unresolved_leaf(&child);
    }
    loop_state.performance.child_bookkeeping_elapsed_us = loop_state
        .performance
        .child_bookkeeping_elapsed_us
        .saturating_add(started.elapsed().as_micros());
}
