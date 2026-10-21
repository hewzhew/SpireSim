pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "retaliation damage per incoming hit",
    effect: "When attacked by another entity with normal damage, deal this much THORNS damage to the attacker, even when Block absorbs the hit. Multi-hit attacks trigger once per hit. THORNS and HP-loss damage do not trigger retaliation; Strength, Weak and Vulnerable do not modify retaliation damage.",
    timing: "On-attacked trigger; persists until removed.",
    round_duration: false,
};

use crate::runtime::action::{Action, DamageInfo, DamageType, NO_SOURCE};
use crate::runtime::combat::CombatState;
use crate::EntityId;

pub fn on_attacked(
    _state: &CombatState,
    _owner: EntityId,
    _damage: i32,
    source: EntityId,
    damage_type: DamageType,
    power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    // Java ThornsPower.onAttacked requires non-thorns/non-HP-loss damage,
    // a real owner, and a source that is not the thorns owner.
    if matches!(damage_type, DamageType::Thorns | DamageType::HpLoss)
        || source == NO_SOURCE
        || source == _owner
    {
        return actions;
    }

    if source != 0 {
        actions.push(Action::Damage(DamageInfo {
            source: _owner,
            target: source,
            base: power_amount,
            output: power_amount,
            damage_type: DamageType::Thorns,
            is_modified: false,
        }));
    } else if _owner != 0 {
        actions.push(Action::Damage(DamageInfo {
            source: _owner,
            target: 0,
            base: power_amount,
            output: power_amount,
            damage_type: DamageType::Thorns,
            is_modified: false,
        }));
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thorns_power_matches_java_on_attacked_filters() {
        let state = crate::test_support::blank_test_combat();

        assert!(on_attacked(&state, 7, 3, 0, DamageType::Thorns, 2).is_empty());
        assert!(on_attacked(&state, 7, 3, 0, DamageType::HpLoss, 2).is_empty());
        assert!(on_attacked(&state, 7, 3, NO_SOURCE, DamageType::Normal, 2).is_empty());
        assert!(on_attacked(&state, 7, 3, 7, DamageType::Normal, 2).is_empty());

        assert_eq!(
            on_attacked(&state, 7, 3, 0, DamageType::Normal, 2).as_slice(),
            &[Action::Damage(DamageInfo {
                source: 7,
                target: 0,
                base: 2,
                output: 2,
                damage_type: DamageType::Thorns,
                is_modified: false,
            })],
            "Java ThornsPower reflects only real non-thorns/non-HP-loss damage from another owner"
        );
    }
}
