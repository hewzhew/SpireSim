use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// MutagenicStrength: At the start of combat, gain 3 Strength and 3 LoseStrength at end of turn.
/// Java: atBattleStart() calls addToTop(Strength) then addToTop(LoseStrength).
pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Strength,
            amount: 3,
        },
        insertion_mode: AddTo::Top, // Java: addToTop
    });
    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::LoseStrength,
            amount: 3,
        },
        insertion_mode: AddTo::Top, // Java: addToTop
    });
    actions
}
