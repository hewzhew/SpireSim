use std::collections::HashSet;
use std::time::Instant;

use super::super::*;
use super::loop_state::SearchLoopState;

#[derive(Default)]
pub(super) struct TurnBoundaryExpansionTracker {
    expanded_sources: HashSet<CombatExactStateKey>,
}

impl TurnBoundaryExpansionTracker {
    fn claim_source(&mut self, source: &SearchNode) -> bool {
        self.expanded_sources
            .insert(combat_exact_state_key(&source.engine, &source.combat))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TurnBoundaryExpansionOutcome {
    NotApplicable,
    Handled,
    AtomicFallback,
    Stop,
}

pub(super) fn expand_turn_boundary_if_owned(
    loop_state: &mut SearchLoopState,
    source: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
) -> TurnBoundaryExpansionOutcome {
    if !loop_state.plugins.expansion.owns_turn_boundaries()
        || !matches!(source.engine, EngineState::CombatPlayerTurn)
        || source.turn_prefix.prefix_length() != 0
        || terminal_label(&source.engine, &source.combat) != SearchTerminalLabel::Unresolved
    {
        return TurnBoundaryExpansionOutcome::NotApplicable;
    }

    if !loop_state
        .turn_boundary_expansion_tracker
        .claim_source(source)
    {
        return TurnBoundaryExpansionOutcome::Handled;
    }

    let remaining_global_nodes = config
        .max_nodes
        .saturating_sub(loop_state.stats.nodes_expanded as usize);
    if remaining_global_nodes == 0 {
        loop_state.mark_node_budget_hit();
        loop_state.push_frontier(source.clone());
        return TurnBoundaryExpansionOutcome::Stop;
    }

    let started = Instant::now();
    let portfolio = build_turn_boundary_portfolio(
        source,
        stepper,
        config,
        &loop_state.plugins,
        remaining_global_nodes,
        deadline,
    );
    loop_state.performance.turn_boundary_macro_calls = loop_state
        .performance
        .turn_boundary_macro_calls
        .saturating_add(1);
    loop_state
        .performance
        .turn_boundary_macro_inner_nodes_expanded = loop_state
        .performance
        .turn_boundary_macro_inner_nodes_expanded
        .saturating_add(portfolio.inner_nodes_expanded as u64);
    loop_state
        .performance
        .turn_boundary_macro_inner_nodes_generated = loop_state
        .performance
        .turn_boundary_macro_inner_nodes_generated
        .saturating_add(portfolio.inner_nodes_generated as u64);
    loop_state.performance.turn_boundary_macro_exact_state_skips = loop_state
        .performance
        .turn_boundary_macro_exact_state_skips
        .saturating_add(portfolio.exact_state_skips as u64);
    loop_state.performance.turn_boundary_macro_elapsed_us = loop_state
        .performance
        .turn_boundary_macro_elapsed_us
        .saturating_add(started.elapsed().as_micros());
    loop_state.record_turn_boundary_work(
        source,
        portfolio.inner_nodes_expanded,
        portfolio.inner_nodes_generated,
    );

    if portfolio.candidates.is_empty() {
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            loop_state.mark_deadline_hit();
            loop_state.push_frontier(source.clone());
            return TurnBoundaryExpansionOutcome::Stop;
        }
        if loop_state.stats.nodes_expanded as usize >= config.max_nodes {
            loop_state.mark_node_budget_hit();
            loop_state.push_frontier(source.clone());
            return TurnBoundaryExpansionOutcome::Stop;
        }
        loop_state.performance.turn_boundary_macro_atomic_fallbacks = loop_state
            .performance
            .turn_boundary_macro_atomic_fallbacks
            .saturating_add(1);
        return TurnBoundaryExpansionOutcome::AtomicFallback;
    }

    loop_state.performance.turn_boundary_macro_candidates = loop_state
        .performance
        .turn_boundary_macro_candidates
        .saturating_add(portfolio.candidates.len() as u64);
    for candidate in portfolio.candidates {
        let mut node = candidate.plan.end_node;
        loop_state.materialize_root_lineage(&mut node);
        loop_state.observe_exact_root_terminal(&node);
        loop_state.record_first_generated_win_if_needed(&node);
        loop_state.push_frontier(node);
    }
    TurnBoundaryExpansionOutcome::Handled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_claims_each_exact_source_once() {
        let node = SearchNode::root(
            EngineState::CombatPlayerTurn,
            crate::test_support::blank_test_combat(),
        );
        let mut tracker = TurnBoundaryExpansionTracker::default();

        assert!(tracker.claim_source(&node));
        assert!(!tracker.claim_source(&node));
    }
}
