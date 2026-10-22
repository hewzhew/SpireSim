use crate::runtime::combat::{CombatCard, CombatState};

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Corruption,
        round_duration: false,
    };

pub fn on_apply(state: &mut CombatState) {
    crate::content::cards::ironclad::corruption::corruption_on_apply(state)
}

pub fn on_card_draw(state: &CombatState, card: &mut CombatCard) {
    crate::content::cards::ironclad::corruption::corruption_on_card_draw(state, card)
}

pub fn on_use_card(state: &mut CombatState, card: &CombatCard, exhaust_override: &mut bool) {
    crate::content::cards::ironclad::corruption::corruption_on_use_card(
        state,
        card,
        exhaust_override,
    )
}
