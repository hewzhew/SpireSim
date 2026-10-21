//! Read-only enemy-attack relevance and realized mitigation facts.
//!
//! These values describe current visible monster actions and already-applied
//! negative Strength. They do not predict future intents or choose an action.

use sts_core::content::powers::PowerId;
use sts_core::runtime::combat::CombatState;
use sts_core::sim::combat_projection::project_monster_move_preview_in_combat;

pub fn combat_tactical_monster_attack_relevance_v1(combat: &CombatState, target: usize) -> i32 {
    let Some(monster) = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target && monster.is_alive_for_action())
    else {
        return 0;
    };
    let preview = project_monster_move_preview_in_combat(combat, monster);
    if preview.hits > 0 {
        preview.hits as i32
    } else {
        1
    }
}

pub fn combat_tactical_sustained_mitigation_score_v1(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| {
            let strength = combat.get_power(monster.id, PowerId::Strength);
            if strength >= 0 {
                return 0;
            }
            (-strength).saturating_mul(combat_tactical_monster_attack_relevance_v1(
                combat, monster.id,
            ))
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::monsters::EnemyId;
    use sts_core::runtime::combat::{Power, PowerPayload};
    use sts_core::test_support::{blank_test_combat, test_monster};

    #[test]
    fn sustained_mitigation_counts_negative_enemy_strength() {
        let mut combat = blank_test_combat();
        let mut monster = test_monster(EnemyId::Cultist);
        monster.id = 1;
        combat.entities.monsters = vec![monster];
        combat.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: -3,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        assert_eq!(combat_tactical_sustained_mitigation_score_v1(&combat), 3);
    }
}
