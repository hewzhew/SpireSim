use std::time::Instant;

use super::super::*;
use super::loop_state::SearchLoopState;
use super::pending_choice_expansion::pending_choice_prefix_owned;
use super::rollout_timing::{timed_rollout_estimate, RolloutEstimateSource};
use super::turn_plan_seeding::seed_turn_plan_frontier;

pub(super) fn initialize_root_frontier(
    loop_state: &mut SearchLoopState,
    engine: &EngineState,
    combat: &CombatState,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
) -> SearchNode {
    let root = SearchNode::root(engine.clone(), combat.clone());
    if pending_choice_prefix_owned(loop_state, &root.engine) {
        loop_state.performance.pending_choice_rollout_skips = loop_state
            .performance
            .pending_choice_rollout_skips
            .saturating_add(1);
    }
    if terminal_label(&root.engine, &root.combat) == SearchTerminalLabel::Win {
        loop_state.stats.nodes_to_first_win = Some(0);
    }
    let root_for_turn_plan_diagnostics = root.clone();
    loop_state.push_frontier(root);
    if !loop_state.plugins.expansion.owns_turn_boundaries()
        && loop_state.plugins.turn_plan.seeds_root_frontier()
    {
        seed_turn_plan_frontier(
            loop_state,
            &root_for_turn_plan_diagnostics,
            stepper,
            config,
            deadline,
        );
    }
    root_for_turn_plan_diagnostics
}

pub(super) fn deferred_root_rollout_estimate(
    loop_state: &mut SearchLoopState,
    root: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
) -> RolloutNodeEstimate {
    timed_rollout_estimate(
        &mut loop_state.rollout_cache,
        root,
        stepper,
        config,
        deadline,
        &mut loop_state.performance,
        RolloutEstimateSource::Root,
        0,
    )
}
