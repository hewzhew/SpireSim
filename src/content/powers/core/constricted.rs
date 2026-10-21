use crate::runtime::action::{Action, DamageInfo, DamageType};
use crate::runtime::combat::{Power, PowerPayload};
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Thorns damage dealt to the owner each turn end",
    effect: "At the owner's turn end, the original creature source deals amount Thorns damage to the owner. Block can absorb it; Strength, Weak and Vulnerable do not change it. Repeated applications add amount while keeping the first source. Artifact can block application and debuff removal can remove it.",
    timing: "Owner turn end, including the turn of application. Persists until removed; does not decrement each round.",
    round_duration: false,
};

pub fn at_end_of_turn(owner: EntityId, power: &Power) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];
    let PowerPayload::SourceEntity(source) = power.payload else {
        panic!("Constricted requires its original creature source");
    };
    actions.push(Action::Damage(DamageInfo {
        source,
        target: owner,
        base: power.amount,
        output: power.amount,
        damage_type: DamageType::Thorns,
        is_modified: false,
    }));

    actions
}

#[cfg(test)]
mod tests {
    use crate::content::monsters::EnemyId;
    use crate::content::powers::{resolve_power_at_end_of_turn, store, PowerId};
    use crate::engine::action_handlers::{execute_action, powers::handle_apply_power};
    use crate::runtime::action::{Action, DamageType};
    use crate::runtime::combat::PowerPayload;
    use crate::test_support::{blank_test_combat, test_monster};

    #[test]
    fn constricted_keeps_first_source_until_removed_and_reapplied() {
        let mut state = blank_test_combat();
        handle_apply_power(7, 0, PowerId::Constricted, 10, &mut state);
        handle_apply_power(9, 0, PowerId::Constricted, 4, &mut state);
        let power = store::powers_snapshot_for(&state, 0).remove(0);
        assert_eq!(power.amount, 14);
        assert_eq!(power.payload, PowerPayload::SourceEntity(7));
        let round_trip: crate::runtime::combat::Power =
            serde_json::from_str(&serde_json::to_string(&power).unwrap()).unwrap();
        assert_eq!(round_trip, power);
        let actions = resolve_power_at_end_of_turn(&power, &mut state, 0);
        let Action::Damage(damage) = &actions[0] else {
            panic!("expected damage")
        };
        assert_eq!((damage.source, damage.target, damage.base), (7, 0, 14));
        assert_eq!(damage.damage_type, DamageType::Thorns);
        store::remove_power_type(&mut state, 0, PowerId::Constricted);
        handle_apply_power(9, 0, PowerId::Constricted, 12, &mut state);
        assert_eq!(
            store::powers_snapshot_for(&state, 0)[0].payload,
            PowerPayload::SourceEntity(9)
        );
    }

    #[test]
    fn constricted_artifact_does_not_capture_a_blocked_source() {
        let mut state = blank_test_combat();
        handle_apply_power(0, 0, PowerId::Artifact, 1, &mut state);
        handle_apply_power(7, 0, PowerId::Constricted, 10, &mut state);
        assert!(!store::has_power(&state, 0, PowerId::Constricted));
        handle_apply_power(9, 0, PowerId::Constricted, 10, &mut state);
        assert_eq!(
            store::powers_snapshot_for(&state, 0)[0].payload,
            PowerPayload::SourceEntity(9)
        );
        handle_apply_power(0, 0, PowerId::Artifact, 1, &mut state);
        handle_apply_power(7, 0, PowerId::Constricted, 4, &mut state);
        let power = store::powers_snapshot_for(&state, 0).remove(0);
        assert_eq!(
            (power.amount, power.payload),
            (10, PowerPayload::SourceEntity(9))
        );
    }

    #[test]
    fn constricted_damage_uses_block_and_ignores_attack_modifiers_and_retaliation() {
        for block in [0, 3, 20] {
            let mut state = blank_test_combat();
            state.entities.monsters = vec![test_monster(EnemyId::SpireGrowth)];
            let monster_hp = state.entities.monsters[0].current_hp;
            let hp = state.entities.player.current_hp;
            state.entities.player.block = block;
            for (source, target, id, amount) in [
                (1, 0, PowerId::Constricted, 10),
                (1, 1, PowerId::Strength, 5),
                (0, 1, PowerId::Weak, 2),
                (1, 0, PowerId::Vulnerable, 2),
                (0, 0, PowerId::Thorns, 3),
            ] {
                handle_apply_power(source, target, id, amount, &mut state);
            }
            let power = store::powers_snapshot_for(&state, 0)
                .into_iter()
                .find(|p| p.power_type == PowerId::Constricted)
                .unwrap();
            for action in resolve_power_at_end_of_turn(&power, &mut state, 0) {
                execute_action(action, &mut state);
            }
            while let Some(action) = state.pop_next_action() {
                execute_action(action, &mut state);
            }
            assert_eq!(state.entities.player.current_hp, hp - (10 - block).max(0));
            assert_eq!(state.entities.player.block, (block - 10).max(0));
            assert_eq!(state.entities.monsters[0].current_hp, monster_hp);
        }
    }
}
