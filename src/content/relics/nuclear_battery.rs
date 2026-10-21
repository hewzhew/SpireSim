use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Nuclear Battery: At the start of each combat, Channel 1 Plasma.
pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    actions.push(ActionInfo {
        action: Action::ChannelOrb(crate::runtime::combat::OrbId::Plasma),
        insertion_mode: AddTo::Bottom,
    });

    actions
}
