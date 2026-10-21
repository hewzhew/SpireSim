use crate::runtime::action::ActionInfo;
use smallvec::SmallVec;

pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: crate::runtime::action::Action::ChannelOrb(crate::runtime::combat::OrbId::Dark),
        insertion_mode: crate::runtime::action::AddTo::Bottom,
    });
    actions
}
