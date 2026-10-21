use crate::runtime::combat::CombatState;
pub fn handle_gain_gold(amount: i32, state: &mut CombatState) {
    if amount <= 0 {
        return;
    }

    if state
        .entities
        .player
        .relics
        .iter()
        .any(|relic| crate::content::relics::mechanics::relic_prevents_gold_gain(relic.id))
    {
        return;
    }

    state.entities.player.gold += amount;
    state.entities.player.gold_delta_this_combat += amount;

    if let Some(amount) =
        state.entities.player.relics.iter().find_map(|relic| {
            crate::content::relics::mechanics::relic_gold_gain_healing_v1(relic.id)
        })
    {
        // Java onGainGold changes HP and queues any threshold callbacks before
        // returning to the gold-producing action (including Greed's cleanup).
        super::health::handle_heal(0, amount, state);
    }
}

pub fn handle_steal_player_gold(thief_id: usize, amount: i32, state: &mut CombatState) {
    if amount <= 0 {
        if let Some(thief) = state
            .entities
            .monsters
            .iter_mut()
            .find(|m| m.id == thief_id)
        {
            thief.thief.protocol_seeded = true;
            thief.thief.slash_count = thief.thief.slash_count.saturating_add(1);
        }
        return;
    }

    let actual = amount.min(state.entities.player.gold).max(0);
    state.entities.player.gold = (state.entities.player.gold - actual).max(0);
    state.entities.player.gold_delta_this_combat -= actual;

    if let Some(thief) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == thief_id)
    {
        thief.thief.protocol_seeded = true;
        thief.thief.slash_count = thief.thief.slash_count.saturating_add(1);
        thief.thief.stolen_gold += actual;
    }
}
