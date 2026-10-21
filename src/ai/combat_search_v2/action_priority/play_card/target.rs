use crate::content::cards::CardTarget;
use crate::content::monsters::EnemyId;
use crate::content::powers::{store, PowerId};
use crate::runtime::combat::CombatState;
use crate::sim::combat_projection::project_monster_move_preview_in_combat;

pub(super) fn target_progress_hint(
    combat: &CombatState,
    target_kind: CardTarget,
    target: Option<usize>,
    damage: i32,
) -> i32 {
    if damage <= 0 {
        return 0;
    }

    match target_kind {
        CardTarget::AllEnemy => combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .map(|monster| damage.min(monster.current_hp + monster.block).max(0))
            .sum(),
        CardTarget::Enemy | CardTarget::SelfAndEnemy => target
            .and_then(|target| monster_hp_with_block(combat, target))
            .map(|hp| damage.min(hp).max(0))
            .unwrap_or_default(),
        _ => 0,
    }
}

pub(super) fn target_progress_kills(
    combat: &CombatState,
    target_kind: CardTarget,
    target: Option<usize>,
    damage: i32,
) -> bool {
    if damage <= 0 {
        return false;
    }

    match target_kind {
        CardTarget::AllEnemy => combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .any(|monster| damage >= monster.current_hp + monster.block),
        CardTarget::Enemy | CardTarget::SelfAndEnemy => target
            .and_then(|target| monster_hp_with_block(combat, target))
            .is_some_and(|hp| damage >= hp),
        _ => false,
    }
}

fn monster_hp_with_block(combat: &CombatState, entity_id: usize) -> Option<i32> {
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == entity_id && monster.is_alive_for_action())
        .map(|monster| monster.current_hp + monster.block)
}

pub(super) fn target_enemy_id(combat: &CombatState, target: Option<usize>) -> Option<EnemyId> {
    target
        .and_then(|entity_id| {
            combat
                .entities
                .monsters
                .iter()
                .find(|monster| monster.id == entity_id && monster.is_alive_for_action())
        })
        .map(|monster| monster.enemy_id)
}

pub(super) fn target_has_stasis_card(combat: &CombatState, target: Option<usize>) -> bool {
    target.is_some_and(|entity_id| store::has_power(combat, entity_id, PowerId::Stasis))
}

pub(super) fn persistent_mitigation_target_hint(
    combat: &CombatState,
    target: Option<usize>,
    persistent_strength_down: i32,
) -> i32 {
    if persistent_strength_down <= 0 {
        return 0;
    }

    target
        .and_then(|entity_id| {
            if store::has_power(combat, entity_id, PowerId::Artifact) {
                return None;
            }
            combat
                .entities
                .monsters
                .iter()
                .find(|monster| monster.id == entity_id && monster.is_alive_for_action())
        })
        .map(|monster| {
            let preview = project_monster_move_preview_in_combat(combat, monster);
            let visible_damage = preview.total_damage.unwrap_or_default().max(0);
            let hit_payoff = i32::from(preview.hits)
                .max(1)
                .saturating_mul(persistent_strength_down);
            visible_damage.saturating_add(hit_payoff)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::combat::{Power, PowerPayload};
    use crate::test_support::{blank_test_combat, planned_monster, test_monster};

    #[test]
    fn all_enemy_target_reports_exact_damage_as_lethal() {
        let mut combat = blank_test_combat();
        let mut monster = test_monster(EnemyId::JawWorm);
        monster.current_hp = 10;
        monster.block = 2;
        combat.entities.monsters = vec![monster];

        assert!(!target_progress_kills(
            &combat,
            CardTarget::AllEnemy,
            None,
            11
        ));
        assert!(target_progress_kills(
            &combat,
            CardTarget::AllEnemy,
            None,
            12
        ));
    }

    #[test]
    fn artifact_barrier_blocks_persistent_mitigation_target_bonus() {
        let mut combat = blank_test_combat();
        let mut monster = planned_monster(EnemyId::TimeEater, 2);
        monster.id = 1;
        combat.entities.monsters = vec![monster];

        assert!(persistent_mitigation_target_hint(&combat, Some(1), 2) > 0);

        combat.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::Artifact,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        assert_eq!(persistent_mitigation_target_hint(&combat, Some(1), 2), 0);
    }
}
