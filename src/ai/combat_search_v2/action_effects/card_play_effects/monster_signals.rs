use crate::content::powers::PowerId;
use crate::content::relics::{paper_crane, RelicId};
use crate::runtime::combat::CombatState;
use crate::sim::combat_projection::project_monster_move_preview_in_combat;

pub(super) fn visible_strength_down_mitigation_hint(
    combat: &CombatState,
    target: usize,
    strength_down: i32,
) -> i32 {
    let Some(monster) = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target && monster.is_alive_for_action())
    else {
        return 0;
    };
    let preview = project_monster_move_preview_in_combat(combat, monster);
    let Some(damage_per_hit) = preview.damage_per_hit else {
        return 0;
    };
    let per_hit = strength_down.min(damage_per_hit).max(0);
    per_hit.saturating_mul(preview.hits.max(1) as i32)
}

pub(super) fn visible_strength_gain_pressure_hint(
    combat: &CombatState,
    target: usize,
    strength_gain: i32,
) -> i32 {
    let Some(monster) = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target && monster.is_alive_for_action())
    else {
        return 0;
    };
    let preview = project_monster_move_preview_in_combat(combat, monster);
    if preview.damage_per_hit.is_none() {
        return 0;
    }
    strength_gain
        .max(0)
        .saturating_mul(preview.hits.max(1) as i32)
}

pub(super) fn visible_weak_mitigation_hint(
    combat: &CombatState,
    target: usize,
    weak_amount: i32,
) -> i32 {
    if weak_amount <= 0 || combat.get_power(target, PowerId::Weak) > 0 {
        return 0;
    }
    let Some(monster) = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target && monster.is_alive_for_action())
    else {
        return 0;
    };
    let preview = project_monster_move_preview_in_combat(combat, monster);
    let Some(damage_per_hit) = preview.damage_per_hit else {
        return 0;
    };
    if damage_per_hit <= 0 || combat.get_power(0, PowerId::Intangible) > 0 {
        return 0;
    }
    let multiplier = if combat.entities.player.has_relic(RelicId::PaperCrane) {
        paper_crane::WEAK_MULTIPLIER
    } else {
        0.75
    };
    let weakened_per_hit = ((damage_per_hit as f32) * multiplier).floor() as i32;
    damage_per_hit
        .saturating_sub(weakened_per_hit)
        .saturating_mul(preview.hits.max(1) as i32)
}

pub(super) fn is_living_monster_id(combat: &CombatState, target: usize) -> bool {
    combat
        .entities
        .monsters
        .iter()
        .any(|monster| monster.id == target && monster.is_alive_for_action())
}
