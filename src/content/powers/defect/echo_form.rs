use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState, QueuedCardPlay, QueuedCardSource};

pub fn on_use_card(state: &mut CombatState, card: &CombatCard, purge: bool, target: Option<usize>) {
    if purge {
        return;
    }

    let Some((amount, doubled_this_turn)) =
        store::with_power_mut(state, 0, PowerId::EchoForm, |power| {
            (power.amount, power.extra_data)
        })
    else {
        return;
    };
    if amount <= 0 {
        return;
    }

    let effective_cards_played_after_current =
        state.turn.counters.cards_played_this_turn as i32 + 1 - doubled_this_turn;
    if effective_cards_played_after_current > amount {
        return;
    }

    let _ = store::with_power_mut(state, 0, PowerId::EchoForm, |power| {
        power.extra_data += 1;
    });

    state.queue_action_back(Action::EnqueueCardPlay {
        item: Box::new(QueuedCardPlay::same_instance_replay_java(
            card,
            target,
            QueuedCardSource::EchoForm,
        )),
        in_front: true,
    });
}
