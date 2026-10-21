use super::super::*;

pub(super) fn initialize_node_child_observers(
    node: &SearchNode,
    child_count: usize,
) -> (
    TurnBranchingStateObservation,
    TurnLocalDominanceStateObservation,
) {
    (
        TurnBranchingStateObservation::new(&node.combat, child_count),
        TurnLocalDominanceStateObservation::new(&node.engine, &node.combat, child_count),
    )
}
