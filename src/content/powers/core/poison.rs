use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{Power, PowerPayload};
use crate::EntityId;

pub fn at_turn_start(owner: EntityId, power: &Power) -> smallvec::SmallVec<[Action; 2]> {
    let amount = power.amount;
    if amount <= 0 {
        return smallvec::smallvec![Action::RemovePower {
            target: owner,
            power_id: PowerId::Poison,
        }];
    }

    smallvec::smallvec![Action::PoisonLoseHp {
        source: match power.payload {
            PowerPayload::SourceEntity(source) => source,
            PowerPayload::None => crate::runtime::action::NO_SOURCE,
            _ => panic!("Poison requires a creature source or an explicit absence"),
        },
        target: owner,
        amount,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::powers::store;
    use crate::engine::action_handlers::powers::handle_apply_power;
    use crate::test_support::blank_test_combat;

    #[test]
    fn constructor_source_survives_stacking_and_is_used_by_the_atomic_tick() {
        let mut state = blank_test_combat();
        handle_apply_power(7, 0, PowerId::Poison, 3, &mut state);
        handle_apply_power(9, 0, PowerId::Poison, 4, &mut state);
        let power = store::powers_snapshot_for(&state, 0).remove(0);
        assert_eq!(
            (power.amount, &power.payload),
            (7, &PowerPayload::SourceEntity(7))
        );
        let restored: Power =
            serde_json::from_str(&serde_json::to_string(&power).unwrap()).unwrap();
        assert_eq!(restored, power);
        assert_eq!(
            at_turn_start(0, &restored).as_slice(),
            &[Action::PoisonLoseHp {
                source: 7,
                target: 0,
                amount: 7,
            }]
        );
        store::remove_power_type(&mut state, 0, PowerId::Poison);
        handle_apply_power(9, 0, PowerId::Poison, 2, &mut state);
        assert_eq!(
            store::powers_snapshot_for(&state, 0)[0].payload,
            PowerPayload::SourceEntity(9)
        );
    }

    #[test]
    fn artifact_blocks_the_source_and_constructor_cap_does_not_cap_stacking() {
        let mut state = blank_test_combat();
        handle_apply_power(0, 0, PowerId::Artifact, 1, &mut state);
        handle_apply_power(7, 0, PowerId::Poison, 20_000, &mut state);
        assert!(!store::has_power(&state, 0, PowerId::Poison));
        handle_apply_power(9, 0, PowerId::Poison, 20_000, &mut state);
        assert_eq!(store::power_amount(&state, 0, PowerId::Poison), 9999);
        handle_apply_power(7, 0, PowerId::Poison, 3, &mut state);
        let power = store::powers_snapshot_for(&state, 0).remove(0);
        assert_eq!(
            (power.amount, power.payload),
            (10_002, PowerPayload::SourceEntity(9))
        );
    }
}
