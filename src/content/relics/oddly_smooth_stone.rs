use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Oddly Smooth Stone: At the start of each combat, gain 1 Dexterity.
pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Dexterity,
            amount: 1,
        },
        insertion_mode: AddTo::Top, // Java: addToTop
    });

    actions
}
