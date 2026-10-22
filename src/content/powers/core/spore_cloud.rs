pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::SporeCloud,
        round_duration: false,
    };

use crate::runtime::action::Action;
use crate::runtime::combat::{CombatState, PowerId};
use crate::EntityId;

pub fn on_death(
    state: &CombatState,
    owner: EntityId,
    amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    if state.are_monsters_basically_dead_java() {
        return actions;
    }

    // Spore Cloud applies Vulnerable to player on death
    actions.push(Action::ApplyPower {
        source: owner, // Technically dead, but source still traceable
        target: 0,     // Player
        power_id: PowerId::Vulnerable,
        amount,
    });

    actions
}

#[cfg(test)]
mod tests {
    use super::on_death;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;

    #[test]
    fn spore_cloud_skips_when_java_room_is_battle_ending() {
        let mut state = crate::testing::support::blank_test_combat();
        state.entities.monsters = vec![crate::testing::support::test_monster(EnemyId::FungiBeast)];
        state.entities.monsters[0].id = 1;
        state.entities.monsters[0].is_dying = true;

        let actions = on_death(&state, 1, 2);

        assert!(
            actions.is_empty(),
            "Java SporeCloudPower.onDeath returns when AbstractRoom.isBattleEnding() is true"
        );
    }

    #[test]
    fn spore_cloud_applies_while_another_monster_is_not_basically_dead() {
        let mut state = crate::testing::support::blank_test_combat();
        let mut dying = crate::testing::support::test_monster(EnemyId::FungiBeast);
        dying.id = 1;
        dying.is_dying = true;
        let mut alive = crate::testing::support::test_monster(EnemyId::FungiBeast);
        alive.id = 2;
        state.entities.monsters = vec![dying, alive];

        let actions = on_death(&state, 1, 2);

        assert!(matches!(
            actions.as_slice(),
            [Action::ApplyPower {
                source: 1,
                target: 0,
                power_id: PowerId::Vulnerable,
                amount: 2,
            }]
        ));
    }
}
