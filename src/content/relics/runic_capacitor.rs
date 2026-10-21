use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub fn at_pre_battle(relic: &mut RelicState) {
    relic.amount = 1;
}

pub fn at_turn_start(relic: &mut RelicState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    if relic.amount == 1 {
        relic.amount = 0;
        actions.push(ActionInfo {
            action: Action::IncreaseMaxOrb(3),
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
