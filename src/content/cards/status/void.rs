use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;

pub fn on_drawn(_state: &CombatState) -> smallvec::SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: Action::GainEnergy { amount: -1 },
        insertion_mode: AddTo::Bottom,
    }]
}
