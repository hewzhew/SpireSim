use crate::runtime::combat::{CombatState, Power, PowerId};
use crate::EntityId;

pub fn powers_for(state: &CombatState, entity: EntityId) -> Option<&[Power]> {
    state.entities.power_db.get(&entity).map(Vec::as_slice)
}

pub fn power_amount(state: &CombatState, entity: EntityId, power_id: PowerId) -> i32 {
    powers_for(state, entity)
        .and_then(|powers| powers.iter().find(|p| p.power_type == power_id))
        .map(|power| power.amount)
        .unwrap_or(0)
}

fn matches_instance(power: &Power, power_id: PowerId, instance_id: u32) -> bool {
    power.power_type == power_id && power.instance_id == Some(instance_id)
}

pub fn has_power(state: &CombatState, entity: EntityId, power_id: PowerId) -> bool {
    powers_for(state, entity).is_some_and(|powers| powers.iter().any(|p| p.power_type == power_id))
}

pub fn powers_snapshot_for(state: &CombatState, entity: EntityId) -> Vec<Power> {
    powers_for(state, entity)
        .map(|powers| powers.to_vec())
        .unwrap_or_default()
}

pub fn powers_snapshot_all(state: &CombatState) -> Vec<(EntityId, Vec<Power>)> {
    state
        .entities
        .power_db
        .iter()
        .map(|(entity, powers)| (*entity, powers.clone()))
        .collect()
}

pub fn powers_for_mut(state: &mut CombatState, entity: EntityId) -> Option<&mut Vec<Power>> {
    state.entities.power_db.get_mut(&entity)
}

pub fn ensure_powers_for_mut(state: &mut CombatState, entity: EntityId) -> &mut Vec<Power> {
    state.entities.power_db.entry(entity).or_default()
}

pub fn sort_powers_for_java(state: &mut CombatState, entity: EntityId) {
    if let Some(powers) = state.entities.power_db.get_mut(&entity) {
        powers.sort_by(|left, right| {
            crate::content::powers::power_priority(left.power_type)
                .cmp(&crate::content::powers::power_priority(right.power_type))
        });
    }
}

pub fn set_powers_for(state: &mut CombatState, entity: EntityId, powers: Vec<Power>) {
    if powers.is_empty() {
        state.entities.power_db.remove(&entity);
    } else {
        state.entities.power_db.insert(entity, powers);
        sort_powers_for_java(state, entity);
    }
    if entity == 0 {
        state.recompute_turn_start_draw_modifier();
    }
}

pub fn remove_entity_powers(state: &mut CombatState, entity: EntityId) {
    state.entities.power_db.remove(&entity);
    if entity == 0 {
        state.recompute_turn_start_draw_modifier();
    }
}

pub fn remove_power_type(state: &mut CombatState, entity: EntityId, power_id: PowerId) -> bool {
    let mut removed = false;
    let mut became_empty = false;
    if let Some(powers) = state.entities.power_db.get_mut(&entity) {
        let before = powers.len();
        powers.retain(|p| p.power_type != power_id);
        removed = powers.len() != before;
        became_empty = powers.is_empty();
    }
    if became_empty {
        state.entities.power_db.remove(&entity);
    }
    if removed && entity == 0 {
        state.recompute_turn_start_draw_modifier();
    }
    removed
}

pub fn remove_power_instance(
    state: &mut CombatState,
    entity: EntityId,
    power_id: PowerId,
    instance_id: u32,
) -> bool {
    let mut removed = false;
    let mut became_empty = false;
    if let Some(powers) = state.entities.power_db.get_mut(&entity) {
        let before = powers.len();
        powers.retain(|p| !matches_instance(p, power_id, instance_id));
        removed = powers.len() != before;
        became_empty = powers.is_empty();
    }
    if became_empty {
        state.entities.power_db.remove(&entity);
    }
    if removed && entity == 0 {
        state.recompute_turn_start_draw_modifier();
    }
    removed
}

pub fn retain_entity_powers<F>(state: &mut CombatState, entity: EntityId, mut keep: F)
where
    F: FnMut(&Power) -> bool,
{
    let mut became_empty = false;
    if let Some(powers) = state.entities.power_db.get_mut(&entity) {
        powers.retain(|p| keep(p));
        became_empty = powers.is_empty();
    }
    if became_empty {
        state.entities.power_db.remove(&entity);
    }
    if entity == 0 {
        state.recompute_turn_start_draw_modifier();
    }
}

pub fn with_power_mut<T, F>(
    state: &mut CombatState,
    entity: EntityId,
    power_id: PowerId,
    f: F,
) -> Option<T>
where
    F: FnOnce(&mut Power) -> T,
{
    let result = state
        .entities
        .power_db
        .get_mut(&entity)
        .and_then(|powers| powers.iter_mut().find(|p| p.power_type == power_id))
        .map(f);
    if result.is_some() && entity == 0 {
        state.recompute_turn_start_draw_modifier();
    }
    result
}

pub fn with_power_instance_mut<T, F>(
    state: &mut CombatState,
    entity: EntityId,
    power_id: PowerId,
    instance_id: u32,
    f: F,
) -> Option<T>
where
    F: FnOnce(&mut Power) -> T,
{
    let result = state
        .entities
        .power_db
        .get_mut(&entity)
        .and_then(|powers| {
            powers
                .iter_mut()
                .find(|p| matches_instance(p, power_id, instance_id))
        })
        .map(f);
    if result.is_some() && entity == 0 {
        state.recompute_turn_start_draw_modifier();
    }
    result
}

pub fn clear_just_applied_flags(state: &mut CombatState) {
    for powers in state.entities.power_db.values_mut() {
        for power in powers.iter_mut() {
            power.just_applied = false;
        }
    }
}
