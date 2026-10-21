use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Vajra: At the start of each combat, gain 1 Strength.
pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Strength,
            amount: 1,
        },
        insertion_mode: AddTo::Top,
    });

    actions
}
