use crate::runtime::action::ActionInfo;
use smallvec::SmallVec;

pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: crate::runtime::action::Action::EnterStance("Calm".to_string()),
        insertion_mode: crate::runtime::action::AddTo::Top,
    });
    actions
}
