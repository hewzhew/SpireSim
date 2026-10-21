use super::super::*;
use super::types::{TurnBranchActionKind, TurnBranchTransition, TurnBranchTransitionKind};

pub(in crate::ai::combat_search_v2) fn classify_turn_branch_transition(
    parent_engine: &EngineState,
    parent_combat: &CombatState,
    input: &ClientInput,
    child_engine: &EngineState,
    child_combat: &CombatState,
) -> TurnBranchTransition {
    let action_kind = TurnBranchActionKind::from_input(input);
    let kind = if terminal_label(child_engine, child_combat) != SearchTerminalLabel::Unresolved {
        TurnBranchTransitionKind::Terminal
    } else if matches!(child_engine, EngineState::PendingChoice(_)) {
        TurnBranchTransitionKind::PendingChoice
    } else if is_same_turn_continuation(parent_engine, parent_combat, child_engine, child_combat) {
        TurnBranchTransitionKind::SameTurn
    } else if is_next_turn_transition(parent_combat, child_combat) {
        TurnBranchTransitionKind::NextTurn
    } else {
        TurnBranchTransitionKind::Other
    };

    TurnBranchTransition { action_kind, kind }
}

fn is_same_turn_continuation(
    parent_engine: &EngineState,
    parent_combat: &CombatState,
    child_engine: &EngineState,
    child_combat: &CombatState,
) -> bool {
    matches!(parent_engine, EngineState::CombatPlayerTurn)
        && matches!(child_engine, EngineState::CombatPlayerTurn)
        && child_combat.turn.turn_count == parent_combat.turn.turn_count
}

fn is_next_turn_transition(parent_combat: &CombatState, child_combat: &CombatState) -> bool {
    child_combat.turn.turn_count > parent_combat.turn.turn_count
}
