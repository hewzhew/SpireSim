use crate::runtime::action::ActionInfo;
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

/// Mummified Hand: Whenever you play a Power card, a random card in your hand costs 0 for the turn.
pub fn on_use_card(card: &CombatCard, state: &mut CombatState) -> SmallVec<[ActionInfo; 4]> {
    let def = crate::content::cards::get_card_definition(card.id);

    if def.card_type == crate::content::cards::CardType::Power {
        apply_effect(state);
    }

    SmallVec::new()
}

pub fn apply_effect(state: &mut CombatState) {
    let reserved = state.reserved_card_lineages_for_queue_sensitive_effects();
    let eligible: Vec<usize> = state
        .zones
        .hand
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            let def = crate::content::cards::get_card_definition(c.id);
            def.cost > 0
                && c.cost_for_turn_java() > 0
                && !c.free_to_play_once
                && !reserved.contains(&c.lineage)
        })
        .map(|(i, _)| i)
        .collect();
    if !eligible.is_empty() {
        let idx = state
            .rng
            .card_random_rng
            .random_range(0, eligible.len() as i32 - 1) as usize;
        let card_idx = eligible[idx];
        state.zones.hand[card_idx].set_cost_for_turn_java(0);
    }
}
