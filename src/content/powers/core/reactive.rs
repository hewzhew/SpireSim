use crate::runtime::action::{Action, DamageType, NO_SOURCE};
use crate::runtime::combat::CombatState;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Reactive,
        round_duration: false,
    };

pub fn on_attacked(
    state: &CombatState,
    owner: EntityId,
    damage: i32,
    source: EntityId,
    damage_type: DamageType,
    _power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    // Monster damage dispatches this hook after applying HP loss. Java checks
    // damage < currentHealth before HP loss; here the equivalent condition is
    // survival, not damage < remaining HP (which drops hits above half HP).
    let remaining_hp = if owner == 0 {
        state.entities.player.current_hp
    } else {
        state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == owner)
            .map_or(0, |monster| monster.current_hp)
    };

    if source != NO_SOURCE
        && !matches!(damage_type, DamageType::HpLoss | DamageType::Thorns)
        && damage > 0
        && remaining_hp > 0
    {
        actions.push(Action::RollMonsterMove { monster_id: owner });
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn reactive_power_rerolls_large_nonlethal_hits_in_the_damage_pipeline() {
        use crate::content::powers::PowerId;
        use crate::engine::action_handlers::execute_action;
        use crate::runtime::action::DamageInfo;
        use crate::runtime::combat::{Power, PowerPayload};

        for (damage, damage_type, source, expected_rolls) in [
            (6, DamageType::Normal, 0, 0),
            (7, DamageType::Normal, 0, 1),
            (58, DamageType::Normal, 0, 1),
            (59, DamageType::Normal, 0, 1),
            (73, DamageType::Normal, 0, 1),
            (110, DamageType::Normal, 0, 1),
            (111, DamageType::Normal, 0, 0),
            (112, DamageType::Normal, 0, 0),
            (73, DamageType::Thorns, 0, 0),
            (73, DamageType::HpLoss, 0, 0),
            (73, DamageType::Normal, NO_SOURCE, 0),
        ] {
            let mut mass = crate::test_support::test_monster(EnemyId::WrithingMass);
            mass.id = 7;
            mass.current_hp = 105;
            mass.max_hp = 175;
            mass.block = 6;
            // Keep another monster alive so terminal queue clearing cannot
            // hide an incorrectly scheduled lethal-hit reroll.
            let mut other = crate::test_support::test_monster(EnemyId::JawWorm);
            other.id = 8;
            let mut state = crate::test_support::combat_with_monsters(vec![mass, other]);
            state.entities.power_db.insert(
                7,
                vec![Power {
                    power_type: PowerId::Reactive,
                    instance_id: None,
                    amount: -1,
                    extra_data: 0,
                    payload: PowerPayload::None,
                    just_applied: false,
                }],
            );
            execute_action(
                Action::Damage(DamageInfo {
                    source,
                    target: 7,
                    base: damage,
                    output: damage,
                    damage_type,
                    is_modified: true,
                }),
                &mut state,
            );
            let mut rolls = 0;
            while let Some(action) = state.pop_next_action() {
                if action == (Action::RollMonsterMove { monster_id: 7 }) {
                    rolls += 1;
                }
            }
            assert_eq!(
                rolls, expected_rolls,
                "damage={damage}, type={damage_type:?}, source={source}"
            );
        }
    }

    #[test]
    fn reactive_power_matches_java_on_attacked_filters() {
        let mut mass = crate::test_support::test_monster(EnemyId::WrithingMass);
        mass.id = 7;
        mass.current_hp = 10;
        let mut state = crate::test_support::combat_with_monsters(vec![mass]);

        assert!(on_attacked(&state, 7, 3, 0, DamageType::Thorns, 1).is_empty());
        assert!(on_attacked(&state, 7, 3, 0, DamageType::HpLoss, 1).is_empty());
        assert!(on_attacked(&state, 7, 3, NO_SOURCE, DamageType::Normal, 1).is_empty());
        assert!(on_attacked(&state, 7, 0, 0, DamageType::Normal, 1).is_empty());

        assert_eq!(
            on_attacked(&state, 7, 3, 0, DamageType::Normal, 1).as_slice(),
            &[Action::RollMonsterMove { monster_id: 7 }],
            "Java ReactivePower only rerolls for real non-lethal non-HP-loss/non-thorns attacks"
        );
        state.entities.monsters[0].current_hp = 0;
        assert!(on_attacked(&state, 7, 10, 0, DamageType::Normal, 1).is_empty());
    }
}
