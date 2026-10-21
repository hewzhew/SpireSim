use sts_oracle_runtime::content::monsters::EnemyId;
use sts_oracle_runtime::content::powers::{store::power_amount, PowerId};
use sts_oracle_runtime::runtime::combat::{CombatState, MonsterEntity};

use super::types::ChampPhaseSnapshot;

pub(super) fn crossed_below_champ_half_hp(
    before: &ChampPhaseSnapshot,
    after: &ChampPhaseSnapshot,
) -> bool {
    before.champ_hp * 2 >= before.champ_max_hp && after.champ_hp * 2 < after.champ_max_hp
}

pub(super) fn champ_phase_snapshot(
    step_index: usize,
    combat: &CombatState,
) -> Option<ChampPhaseSnapshot> {
    let champ = champ_entity(combat)?;
    Some(ChampPhaseSnapshot {
        step_index,
        player_hp: combat.entities.player.current_hp,
        player_max_hp: combat.entities.player.max_hp,
        player_block: combat.entities.player.block,
        champ_hp: champ.current_hp,
        champ_max_hp: champ.max_hp,
        champ_block: champ.block,
        champ_strength: power_amount(combat, champ.id, PowerId::Strength),
        champ_weak: power_amount(combat, champ.id, PowerId::Weak),
        champ_vulnerable: power_amount(combat, champ.id, PowerId::Vulnerable),
        champ_threshold_reached: champ.champ.threshold_reached,
        champ_move_id: champ.planned_move_id(),
        total_enemy_hp: audit_total_enemy_hp(combat),
        living_enemy_count: audit_living_enemy_count(combat),
    })
}

fn champ_entity(combat: &CombatState) -> Option<&MonsterEntity> {
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.enemy_id == EnemyId::Champ)
}

fn audit_total_enemy_hp(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster.current_hp.max(0) + monster.block.max(0))
        .sum()
}

fn audit_living_enemy_count(combat: &CombatState) -> usize {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .count()
}
