use std::collections::HashSet;
use std::time::Instant;

use super::super::*;
use super::loop_state::SearchLoopState;
use super::pending_choice_expansion::pending_choice_prefix_owned;
use super::rollout_timing::{timed_rollout_estimate, RolloutEstimateSource};

#[derive(Default)]
pub(super) struct TurnPlanSeedTracker {
    seeded_sources: HashSet<CombatExactStateKey>,
}

impl TurnPlanSeedTracker {
    fn claim_source(&mut self, source: &SearchNode) -> bool {
        self.seeded_sources
            .insert(combat_exact_state_key(&source.engine, &source.combat))
    }
}

pub(super) fn seed_turn_plan_frontier(
    loop_state: &mut SearchLoopState,
    source: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
) {
    if !loop_state.turn_plan_seed_tracker.claim_source(source) {
        return;
    }

    let seed_started = Instant::now();
    let mut seeded_plans =
        turn_plan_frontier_seed(source, stepper, config, &loop_state.plugins, deadline);
    loop_state.performance.turn_plan_frontier_seed_calls = loop_state
        .performance
        .turn_plan_frontier_seed_calls
        .saturating_add(1);
    loop_state.performance.turn_plan_frontier_seed_elapsed_us = loop_state
        .performance
        .turn_plan_frontier_seed_elapsed_us
        .saturating_add(seed_started.elapsed().as_micros());
    loop_state
        .performance
        .turn_plan_frontier_seed_inner_nodes_expanded = loop_state
        .performance
        .turn_plan_frontier_seed_inner_nodes_expanded
        .saturating_add(seeded_plans.inner_nodes_expanded as u64);
    loop_state
        .performance
        .turn_plan_frontier_seed_inner_nodes_generated = loop_state
        .performance
        .turn_plan_frontier_seed_inner_nodes_generated
        .saturating_add(seeded_plans.inner_nodes_generated as u64);
    loop_state
        .performance
        .turn_plan_frontier_seed_exact_state_skips = loop_state
        .performance
        .turn_plan_frontier_seed_exact_state_skips
        .saturating_add(seeded_plans.exact_state_skips as u64);
    loop_state
        .diagnostics
        .observe_turn_plan_frontier_seeded_plans(&seeded_plans.plans);
    loop_state
        .diagnostics
        .observe_turn_plan_prior_scored_plans(seeded_plans.turn_plan_prior_scored_plans);
    for plan in seeded_plans.plans.drain(..) {
        let mut seed = plan.end_node;
        loop_state.materialize_root_lineage(&mut seed);
        let nodes_generated_at_discovery = loop_state.stats.nodes_generated.saturating_add(1);
        seed.rollout_estimate = turn_plan_seed_rollout_estimate(
            loop_state,
            &seed,
            stepper,
            config,
            deadline,
            nodes_generated_at_discovery,
        );
        loop_state.record_node_generated(&seed);
        loop_state.observe_exact_root_terminal(&seed);
        loop_state.record_first_generated_win_if_needed(&seed);
        loop_state.push_frontier(seed);
    }
}

fn turn_plan_seed_rollout_estimate(
    loop_state: &mut SearchLoopState,
    seed: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    nodes_generated_at_discovery: u64,
) -> RolloutNodeEstimate {
    if pending_choice_prefix_owned(loop_state, &seed.engine) {
        loop_state.performance.pending_choice_rollout_skips = loop_state
            .performance
            .pending_choice_rollout_skips
            .saturating_add(1);
        RolloutNodeEstimate::unevaluated()
    } else if terminal_label(&seed.engine, &seed.combat) == SearchTerminalLabel::Unresolved {
        timed_rollout_estimate(
            &mut loop_state.rollout_cache,
            seed,
            stepper,
            config,
            deadline,
            &mut loop_state.performance,
            RolloutEstimateSource::TurnPlanSeed,
            nodes_generated_at_discovery,
        )
    } else {
        loop_state.performance.terminal_turn_plan_seed_rollout_skips = loop_state
            .performance
            .terminal_turn_plan_seed_rollout_skips
            .saturating_add(1);
        RolloutNodeEstimate::from_node(
            seed,
            0,
            RolloutStopReason::TerminalState,
            Some("terminal_turn_plan_seed_no_rollout"),
            super::super::rollout_pending_choice::RolloutPendingChoiceProgress::default(),
        )
    }
}
