use crate::runtime::combat::CombatState;
pub fn handle_update_relic_counter(
    relic_id: crate::content::relics::RelicId,
    counter: i32,
    state: &mut CombatState,
) {
    if let Some(relic) = state
        .entities
        .player
        .relics
        .iter_mut()
        .find(|r| r.id == relic_id)
    {
        relic.counter = counter;
    }
}

pub fn handle_update_relic_amount(
    relic_id: crate::content::relics::RelicId,
    amount: i32,
    state: &mut CombatState,
) {
    if let Some(relic) = state
        .entities
        .player
        .relics
        .iter_mut()
        .find(|r| r.id == relic_id)
    {
        relic.counter += amount;
    }
}

pub fn handle_update_relic_used_up(
    relic_id: crate::content::relics::RelicId,
    used_up: bool,
    state: &mut CombatState,
) {
    if let Some(relic) = state
        .entities
        .player
        .relics
        .iter_mut()
        .find(|r| r.id == relic_id)
    {
        relic.used_up = used_up;
    }
}
