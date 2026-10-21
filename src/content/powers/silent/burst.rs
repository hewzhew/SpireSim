use crate::content::powers::store;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState, QueuedCardPlay, QueuedCardSource};

pub fn on_use_card(state: &mut CombatState, card: &CombatCard, purge: bool, target: Option<usize>) {
    if purge
        || crate::content::cards::get_card_definition(card.id).card_type
            != crate::content::cards::CardType::Skill
    {
        return;
    }

    if let Some(amount) = store::with_power_mut(state, 0, PowerId::Burst, |p| {
        p.amount -= 1;
        p.amount
    }) {
        if !crate::content::powers::should_keep_power_instance(PowerId::Burst, amount) {
            store::remove_power_type(state, 0, PowerId::Burst);
        }
    }

    state.queue_action_back(Action::EnqueueCardPlay {
        item: Box::new(QueuedCardPlay::same_instance_replay_java(
            card,
            target,
            QueuedCardSource::Burst,
        )),
        in_front: true,
    });
}
