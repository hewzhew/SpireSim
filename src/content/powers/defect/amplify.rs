use crate::content::cards::{get_card_definition, CardType};
use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState, QueuedCardPlay, QueuedCardSource};

pub fn on_use_card(state: &mut CombatState, card: &CombatCard, purge: bool, target: Option<usize>) {
    if purge || get_card_definition(card.id).card_type != CardType::Power {
        return;
    }
    let amount = store::power_amount(state, 0, PowerId::Amplify);
    if amount <= 0 {
        return;
    }

    if let Some(remaining) = store::with_power_mut(state, 0, PowerId::Amplify, |power| {
        power.amount -= 1;
        power.amount
    }) {
        if !crate::content::powers::should_keep_power_instance(PowerId::Amplify, remaining) {
            store::remove_power_type(state, 0, PowerId::Amplify);
        }
    }

    state.queue_action_back(Action::EnqueueCardPlay {
        item: Box::new(QueuedCardPlay::same_instance_replay_java(
            card,
            target,
            QueuedCardSource::Amplify,
        )),
        in_front: true,
    });
}
