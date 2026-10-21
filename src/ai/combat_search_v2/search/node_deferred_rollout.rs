use super::super::rollout_scheduler::deferred_child_rollout_admission;
use super::super::*;
use super::loop_state::SearchLoopState;
use super::rollout_timing::{
    observe_deferred_rollout_admission, timed_rollout_estimate, RolloutEstimateSource,
};

pub(super) enum DeferredRolloutOutcome {
    Continue(SearchNode),
    Requeued,
}

pub(super) fn apply_deferred_child_rollout(
    loop_state: &mut SearchLoopState,
    mut node: SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
) -> DeferredRolloutOutcome {
    let admission = deferred_child_rollout_admission(
        &node,
        &loop_state.plugins,
        &loop_state.stats,
        &loop_state.performance,
    );
    observe_deferred_rollout_admission(admission, &mut loop_state.performance);
    if admission.admitted() {
        let nodes_generated_at_discovery = loop_state.stats.nodes_generated;
        node.rollout_estimate = timed_rollout_estimate(
            &mut loop_state.rollout_cache,
            &node,
            stepper,
            config,
            deadline,
            &mut loop_state.performance,
            RolloutEstimateSource::DeferredChild,
            nodes_generated_at_discovery,
        );
        if node.rollout_estimate.is_evaluated() {
            loop_state.performance.deferred_child_rollout_requeues = loop_state
                .performance
                .deferred_child_rollout_requeues
                .saturating_add(1);
            loop_state.push_frontier(node);
            return DeferredRolloutOutcome::Requeued;
        }
    }
    DeferredRolloutOutcome::Continue(node)
}
