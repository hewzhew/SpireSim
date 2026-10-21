use std::collections::BTreeMap;

use crate::content::monsters::EnemyId;
use crate::content::powers::{store, PowerId};
use crate::runtime::combat::{CombatState, MonsterEntity};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ProjectedMonsterDamage {
    pub(super) entity_id: usize,
    pub(super) current_hp: i32,
    pub(super) max_hp: i32,
    pub(super) projected_hp: i32,
    pub(super) projected_block: i32,
    pub(super) hp_loss: i32,
    pub(super) split_power: bool,
    pub(super) large_slime_split_already_triggered: bool,
    pub(super) planned_move_id: u8,
    pub(super) guardian_open: bool,
    pub(super) guardian_close_up_triggered: bool,
    pub(super) guardian_mode_shift_remaining: Option<i32>,
    pub(super) lagavulin_sleeping: bool,
    pub(super) champ_threshold_pending: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PhaseProjection {
    pub(super) monsters: BTreeMap<usize, ProjectedMonsterDamage>,
    slot_entity_ids: Vec<usize>,
}

impl PhaseProjection {
    pub(super) fn from_combat(combat: &CombatState) -> Self {
        let mut monsters = BTreeMap::new();
        let mut slot_entity_ids = Vec::new();
        for monster in &combat.entities.monsters {
            let Some(projected) = projected_monster_from(combat, monster) else {
                continue;
            };
            slot_entity_ids.push(projected.entity_id);
            monsters.insert(projected.entity_id, projected);
        }
        Self {
            monsters,
            slot_entity_ids,
        }
    }

    pub(super) fn apply_damage_to_entity(&mut self, entity_id: usize, damage: i32) {
        let Some(projected) = self.monsters.get_mut(&entity_id) else {
            return;
        };
        projected.apply_damage(damage);
    }

    pub(super) fn apply_damage_to_slot(&mut self, slot: usize, damage: i32) {
        let Some(entity_id) = self.slot_entity_ids.get(slot).copied() else {
            return;
        };
        self.apply_damage_to_entity(entity_id, damage);
    }
}

impl ProjectedMonsterDamage {
    fn apply_damage(&mut self, damage: i32) {
        if damage <= 0 || self.projected_hp <= 0 {
            return;
        }
        let unblocked = if self.projected_block > 0 {
            if damage >= self.projected_block {
                let remaining = damage - self.projected_block;
                self.projected_block = 0;
                remaining
            } else {
                self.projected_block -= damage;
                0
            }
        } else {
            damage
        };
        let hp_loss = unblocked.min(self.projected_hp).max(0);
        self.projected_hp -= hp_loss;
        self.hp_loss = self.hp_loss.saturating_add(hp_loss);
    }
}

fn projected_monster_from(
    combat: &CombatState,
    monster: &MonsterEntity,
) -> Option<ProjectedMonsterDamage> {
    if !monster.is_alive_for_action() {
        return None;
    }
    let enemy_id = monster.enemy_id;
    Some(ProjectedMonsterDamage {
        entity_id: monster.id,
        current_hp: monster.current_hp,
        max_hp: monster.max_hp,
        projected_hp: monster.current_hp,
        projected_block: monster.block,
        hp_loss: 0,
        split_power: store::has_power(combat, monster.id, PowerId::Split),
        large_slime_split_already_triggered: matches!(
            enemy_id,
            EnemyId::AcidSlimeL | EnemyId::SpikeSlimeL
        ) && monster.large_slime.split_triggered,
        planned_move_id: monster.planned_move_id(),
        guardian_open: enemy_id == EnemyId::TheGuardian && monster.guardian.is_open,
        guardian_close_up_triggered: enemy_id == EnemyId::TheGuardian
            && monster.guardian.close_up_triggered,
        guardian_mode_shift_remaining: if enemy_id == EnemyId::TheGuardian
            && store::has_power(combat, monster.id, PowerId::ModeShift)
        {
            Some(store::power_amount(combat, monster.id, PowerId::ModeShift))
        } else {
            None
        },
        lagavulin_sleeping: enemy_id == EnemyId::Lagavulin && !monster.lagavulin.is_out,
        champ_threshold_pending: enemy_id == EnemyId::Champ && !monster.champ.threshold_reached,
    })
}
