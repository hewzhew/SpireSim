use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatState, PlayerEntity};
use smallvec::SmallVec;

/// Gremlin Mask: At the start of each combat, apply 1 Weak to yourself.
pub fn at_battle_start(_state: &CombatState, player: &PlayerEntity) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: player.id,
            target: player.id,
            power_id: crate::content::powers::PowerId::Weak,
            amount: 1,
        },
        insertion_mode: AddTo::Bottom,
    });
    actions
}
