use std::time::Instant;

use super::super::*;
use super::loop_state::SearchLoopState;
use super::pending_choice_expansion::pending_choice_prefix_owned;
use super::rollout_timing::{timed_rollout_estimate, RolloutEstimateSource};

pub(super) fn child_rollout_estimate(
    loop_state: &mut SearchLoopState,
    child: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
) -> RolloutNodeEstimate {
    if terminal_label(&child.engine, &child.combat) != SearchTerminalLabel::Unresolved {
        loop_state.performance.terminal_child_rollout_skips = loop_state
            .performance
            .terminal_child_rollout_skips
            .saturating_add(1);
        return RolloutNodeEstimate::from_node(
            child,
            0,
            RolloutStopReason::TerminalState,
            Some("terminal_child_no_rollout"),
            super::super::rollout_pending_choice::RolloutPendingChoiceProgress::default(),
        );
    }
    if pending_choice_prefix_owned(loop_state, &child.engine) {
        loop_state.performance.pending_choice_rollout_skips = loop_state
            .performance
            .pending_choice_rollout_skips
            .saturating_add(1);
        return RolloutNodeEstimate::unevaluated();
    }
    if loop_state.plugins.child_rollout == CombatSearchChildRolloutPluginId::LazyOnPop
        && loop_state.plugins.rollout != CombatSearchRolloutPluginId::Disabled
    {
        loop_state.performance.deferred_child_rollout_nodes = loop_state
            .performance
            .deferred_child_rollout_nodes
            .saturating_add(1);
        return RolloutNodeEstimate::unevaluated();
    }
    let nodes_generated_at_discovery = loop_state.stats.nodes_generated;
    timed_rollout_estimate(
        &mut loop_state.rollout_cache,
        child,
        stepper,
        config,
        deadline,
        &mut loop_state.performance,
        RolloutEstimateSource::Child,
        nodes_generated_at_discovery,
    )
}
