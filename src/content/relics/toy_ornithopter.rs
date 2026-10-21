use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub fn on_use_potion(
    _state: &CombatState,
    player_id: crate::EntityId,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    // Heal 5 HP when you use a potion
    actions.push(ActionInfo {
        action: Action::Heal {
            target: player_id,
            amount: 5,
        },
        insertion_mode: AddTo::Bottom,
    });
    actions
}
