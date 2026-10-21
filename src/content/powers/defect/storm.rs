use crate::content::cards::{get_card_definition, CardType};
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState, OrbId};

pub fn on_use_card(state: &mut CombatState, card: &CombatCard, amount: i32) {
    if get_card_definition(card.id).card_type == CardType::Power && amount > 0 {
        for _ in 0..amount {
            state.queue_action_back(Action::ChannelOrb(OrbId::Lightning));
        }
    }
}
