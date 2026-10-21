use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;

pub fn at_battle_start(
    _state: &CombatState,
    _relic: &mut RelicState,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();
    actions.push(ActionInfo {
        action: Action::ChannelOrb(crate::runtime::combat::OrbId::Lightning),
        insertion_mode: AddTo::Bottom,
    });
    actions
}
