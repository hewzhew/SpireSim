use crate::runtime::action::{Action, DamageType, NO_SOURCE};
use crate::runtime::combat::OrbId;
use smallvec::SmallVec;

pub fn on_attacked(
    owner: crate::EntityId,
    source: crate::EntityId,
    damage: i32,
    damage_type: DamageType,
    amount: i32,
) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    if damage <= 0
        || source == NO_SOURCE
        || source == owner
        || matches!(damage_type, DamageType::Thorns | DamageType::HpLoss)
    {
        return actions;
    }
    for _ in 0..amount.max(0) {
        actions.push(Action::ChannelOrb(OrbId::Lightning));
    }
    actions
}
