use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;

pub fn at_battle_start(
    _state: &CombatState,
    _relic: &mut RelicState,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();
    actions.push(ActionInfo {
        action: Action::ApplyPower {
            target: 0,
            source: 0,
            power_id: crate::content::powers::PowerId::Focus,
            amount: 1,
        },
        insertion_mode: AddTo::Top,
    });
    actions
}
