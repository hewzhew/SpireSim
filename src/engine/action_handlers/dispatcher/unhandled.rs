use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

pub(super) fn execute(action: Action, _state: &mut CombatState) {
    match action {
        // === Pass-through / unhandled ===
        // These variants exist but have no handler yet or are handled inline elsewhere.
        Action::PlayCard { .. }
        | Action::UseCard { .. }
        | Action::StartTurnTrigger
        | Action::FleeCombat
        | Action::AbortDeath { .. }
        | Action::ExecuteMonsterTurn(_)
        | Action::SpawnEncounter { .. }
        | Action::Scry(_) => {
            #[cfg(debug_assertions)]
            eprintln!("[action_handlers] Unhandled action: {:?}", action);
        }
        Action::SuspendForHandSelect { .. }
        | Action::SuspendForGridSelect { .. }
        | Action::SuspendForDiscovery { .. }
        | Action::SuspendForForeignInfluence { .. }
        | Action::SuspendForStanceChoice
        | Action::SuspendForChooseOne { .. }
        | Action::SuspendForCardReward { .. } => {
            // These suspend actions are intercepted in engine::core and converted into
            // PendingChoice states. Reaching the thin dispatcher is not actionable noise.
        }
        _other => {
            #[cfg(debug_assertions)]
            eprintln!("[action_handlers] Unrouted action: {:?}", _other);
        }
    }
}
