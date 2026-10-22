pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Angry,
        round_duration: false,
    };

use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, PowerId};
use smallvec::{smallvec, SmallVec};

pub fn on_attacked(
    _state: &CombatState,
    owner: crate::EntityId,
    damage: i32,
    source: crate::EntityId,
    damage_type: crate::runtime::action::DamageType,
    power_amount: i32,
) -> SmallVec<[Action; 2]> {
    if damage <= 0
        || source == crate::runtime::action::NO_SOURCE
        || matches!(
            damage_type,
            crate::runtime::action::DamageType::HpLoss | crate::runtime::action::DamageType::Thorns
        )
    {
        return smallvec![];
    }
    smallvec![Action::ApplyPower {
        target: owner,
        source: owner,
        power_id: PowerId::Strength,
        amount: power_amount,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::action::{DamageType, NO_SOURCE};

    #[test]
    fn angry_matches_java_positive_owned_non_thorns_damage_gate() {
        let state = crate::test_support::blank_test_combat();

        assert!(on_attacked(&state, 7, 0, 0, DamageType::Normal, 2).is_empty());
        assert!(on_attacked(&state, 7, 3, NO_SOURCE, DamageType::Normal, 2).is_empty());
        assert!(on_attacked(&state, 7, 3, 0, DamageType::Thorns, 2).is_empty());
        assert!(on_attacked(&state, 7, 3, 0, DamageType::HpLoss, 2).is_empty());
        assert!(matches!(
            on_attacked(&state, 7, 3, 0, DamageType::Normal, 2).as_slice(),
            [Action::ApplyPower {
                source: 7,
                target: 7,
                power_id: PowerId::Strength,
                amount: 2,
            }]
        ));
    }
}
