use std::time::Instant;

use super::super::*;
use super::loop_state::SearchLoopState;

pub(super) enum NodeBudgetGateOutcome {
    Continue(SearchNode),
    Stop,
}

pub(super) fn apply_node_budget_gate(
    loop_state: &mut SearchLoopState,
    node: SearchNode,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
) -> NodeBudgetGateOutcome {
    if loop_state.stats.nodes_expanded as usize >= config.max_nodes {
        loop_state.mark_node_budget_hit();
        loop_state.push_frontier(node);
        return NodeBudgetGateOutcome::Stop;
    }
    if deadline.is_some_and(|limit| Instant::now() >= limit) {
        loop_state.mark_deadline_hit();
        loop_state.push_frontier(node);
        return NodeBudgetGateOutcome::Stop;
    }
    NodeBudgetGateOutcome::Continue(node)
}
