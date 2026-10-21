use crate::runtime::action::{Action, DamageType};
use smallvec::SmallVec;

pub fn on_block_gained(amount: i32) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    actions.push(Action::DamageRandomEnemy {
        source: 0,
        base_damage: amount,
        damage_type: DamageType::Thorns,
    });
    actions
}
