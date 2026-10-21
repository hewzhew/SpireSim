use super::super::*;
use super::loop_state::SearchLoopState;

pub(super) enum NodeTerminalOutcome {
    Continue(SearchNode),
    Skip,
    StopAcceptedWin,
}

pub(super) fn apply_node_terminal_gate(
    loop_state: &mut SearchLoopState,
    node: SearchNode,
    config: &CombatSearchV2Config,
) -> NodeTerminalOutcome {
    loop_state.remember_best_frontier(&node);
    if terminal_label(&node.engine, &node.combat) != SearchTerminalLabel::Unresolved {
        loop_state.mark_unmaterialized_root_surface_complete(&node);
    }
    match terminal_label(&node.engine, &node.combat) {
        SearchTerminalLabel::Win => {
            if loop_state.remember_win(node, config) {
                NodeTerminalOutcome::StopAcceptedWin
            } else {
                NodeTerminalOutcome::Skip
            }
        }
        SearchTerminalLabel::Loss => {
            loop_state.remember_loss(node);
            NodeTerminalOutcome::Skip
        }
        SearchTerminalLabel::Escape => NodeTerminalOutcome::Skip,
        SearchTerminalLabel::Unresolved => NodeTerminalOutcome::Continue(node),
    }
}
