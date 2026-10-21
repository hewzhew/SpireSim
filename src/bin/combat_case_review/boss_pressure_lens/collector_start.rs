use sts_oracle_runtime::content::monsters::EnemyId;
use sts_oracle_runtime::runtime::combat::{CombatState, MonsterEntity};
use sts_oracle_runtime::sim::combat_projection::monster_preview_total_damage_in_combat;

use super::types::CollectorStartSignals;

pub(super) fn collector_start_signals(
    combat: &CombatState,
    collector: &MonsterEntity,
) -> CollectorStartSignals {
    let player = &combat.entities.player;
    CollectorStartSignals {
        turn: combat.turn.turn_count,
        player_hp: player.current_hp,
        player_max_hp: player.max_hp,
        player_hp_percent: percent(player.current_hp, player.max_hp),
        collector_hp: collector.current_hp,
        collector_max_hp: collector.max_hp,
        collector_hp_percent: percent(collector.current_hp, collector.max_hp),
        torch_heads_alive: combat
            .entities
            .monsters
            .iter()
            .filter(|monster| enemy_id(monster) == EnemyId::TorchHead)
            .filter(|monster| monster.is_alive_for_action())
            .count(),
        visible_incoming_damage: visible_incoming_damage(combat),
    }
}

pub(super) fn find_enemy(combat: &CombatState, id: EnemyId) -> Option<&MonsterEntity> {
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| enemy_id(monster) == id && monster.is_alive_for_action())
}

fn visible_incoming_damage(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster_preview_total_damage_in_combat(combat, monster))
        .sum()
}

fn enemy_id(monster: &MonsterEntity) -> EnemyId {
    monster.enemy_id
}

fn percent(value: i32, max: i32) -> i32 {
    if max <= 0 {
        0
    } else {
        value.saturating_mul(100) / max
    }
}
