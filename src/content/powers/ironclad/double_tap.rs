use crate::content::powers::store;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState, QueuedCardPlay, QueuedCardSource};

pub fn on_use_card(state: &mut CombatState, card: &CombatCard, purge: bool, target: Option<usize>) {
    if !purge
        && crate::content::cards::get_card_definition(card.id).card_type
            == crate::content::cards::CardType::Attack
    {
        // Deduct power
        if let Some(amount) = store::with_power_mut(state, 0, PowerId::DoubleTap, |p| {
            p.amount -= 1;
            p.amount
        }) {
            if !crate::content::powers::should_keep_power_instance(PowerId::DoubleTap, amount) {
                store::remove_power_type(state, 0, PowerId::DoubleTap);
            }
        }

        // Java inserts the replay into GameActionManager.cardQueue while the
        // original card's actions are already in GameActionManager.actions.
        // The action queue is drained before cardQueue, so this bridge must sit
        // behind the original actions; `in_front` applies only when the bridge
        // finally transfers the replay into the card queue.
        state.queue_action_back(Action::EnqueueCardPlay {
            item: Box::new(QueuedCardPlay::same_instance_replay_java(
                card,
                target,
                QueuedCardSource::DoubleTap,
            )),
            in_front: true,
        });
    }
}
