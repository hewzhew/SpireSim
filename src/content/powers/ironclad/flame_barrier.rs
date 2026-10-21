use crate::runtime::action::{Action, DamageInfo, DamageType};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "retaliation damage per incoming hit",
    effect: "When another entity attacks the owner with normal damage, deal amount THORNS damage to the attacker, even when Block absorbs the hit. Multi-hit attacks trigger once per hit. Self damage, THORNS and HP-loss damage do not trigger it. Strength, Weak and Vulnerable do not modify retaliation damage.",
    timing: "On-attacked trigger; removed at the start of the owner's next turn. Multiple applications add their amounts until that removal.",
    round_duration: false,
};

pub fn on_attacked(
    owner: crate::EntityId,
    source: crate::EntityId,
    damage_type: DamageType,
    amount: i32,
) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    if source == crate::runtime::action::NO_SOURCE
        || source == owner
        || matches!(damage_type, DamageType::Thorns | DamageType::HpLoss)
    {
        return actions;
    }
    actions.push(Action::Damage(DamageInfo {
        source: owner,
        target: source,
        base: amount,
        output: amount,
        damage_type: DamageType::Thorns,
        is_modified: false,
    }));
    actions
}
