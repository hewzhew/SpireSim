use crate::runtime::action::ActionInfo;
use smallvec::SmallVec;

pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: crate::runtime::action::Action::SuspendForCardReward {
            pool: crate::runtime::action::CardRewardPool::Colorless,
            destination: crate::runtime::action::CardDestination::Hand,
            can_skip: false,
            skip_if_monsters_basically_dead: false,
        },
        insertion_mode: crate::runtime::action::AddTo::Bottom,
    });
    actions
}
