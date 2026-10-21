use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;

pub fn on_end_turn_in_hand(_state: &CombatState) -> smallvec::SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: Action::ApplyPower {
            target: 0,
            source: 0,
            power_id: PowerId::Frail,
            amount: 1,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
