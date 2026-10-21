use super::super::frontier::is_resource_covered;
use super::super::*;
use super::loop_state::SearchLoopState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NodePruneOutcome {
    Continue,
    Pruned,
}

pub(super) fn apply_node_prune_gates(
    loop_state: &mut SearchLoopState,
    node: &SearchNode,
    config: &CombatSearchV2Config,
) -> NodePruneOutcome {
    if node.actions.len() >= config.max_actions_per_line {
        loop_state.record_max_actions_cut();
        return NodePruneOutcome::Pruned;
    }

    let resource = node.resource_vector();
    let exact_key = combat_exact_state_key(&node.engine, &node.combat);
    if is_resource_covered(&mut loop_state.exact_transpositions, exact_key, resource) {
        loop_state.record_transposition_prune();
        return NodePruneOutcome::Pruned;
    }

    let dominance_key = combat_dominance_key(&node.engine, &node.combat);
    if is_resource_covered(&mut loop_state.dominance, dominance_key, resource) {
        loop_state.record_dominance_prune();
        return NodePruneOutcome::Pruned;
    }

    NodePruneOutcome::Continue
}
