use crate::runtime::combat::{CombatCard, CombatState};

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "presence; not a numeric stack count",
    effect: "While active, Skills cost 0 and Exhaust when played, including Skills that normally do not Exhaust. It does not draw cards or give Energy. Attacks and Powers keep their own costs and destinations. Owning the Corruption card is not the same as having this active power.",
    timing: "Takes effect when deployed and persists for the rest of combat. Additional Corruption applications do not multiply this effect.",
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
