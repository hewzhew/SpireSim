use crate::content::cards::CardType;
use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState};

/// Java `ReboundPower.onAfterUseCard`.
///
/// `extra_data != 0` is the Rust storage for Java's private `justEvoked`
/// flag. A newly created ReboundPower skips the card that created it; stacked
/// existing ReboundPower instances do not reset that flag.
pub fn on_after_use_card(state: &mut CombatState, card: &CombatCard) -> bool {
    let should_process = store::with_power_mut(state, 0, PowerId::Rebound, |power| {
        if power.extra_data != 0 {
            power.extra_data = 0;
            false
        } else {
            power.amount > 0
        }
    })
    .unwrap_or(false);

    if !should_process {
        return false;
    }

    state.queue_action_back(Action::ReducePower {
        target: 0,
        power_id: PowerId::Rebound,
        amount: 1,
    });

    crate::content::cards::get_card_definition(card.id).card_type != CardType::Power
}
