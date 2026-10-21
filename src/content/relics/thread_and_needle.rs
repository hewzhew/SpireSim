use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// ThreadAndNeedle: At the start of combat, gain 4 Plated Armor.
pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::PlatedArmor,
            amount: 4,
        },
        insertion_mode: AddTo::Top,
    });
    actions
}
