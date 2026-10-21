use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use smallvec::SmallVec;

pub fn on_hp_lost(owner: crate::EntityId, amount: i32) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    actions.push(Action::ApplyPower {
        source: owner,
        target: owner,
        power_id: PowerId::Strength,
        amount,
    });
    actions
}
