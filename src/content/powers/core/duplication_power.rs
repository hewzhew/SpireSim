use crate::content::powers::store;
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState, QueuedCardPlay, QueuedCardSource};

/// DuplicationPower: from Duplication Potion.
/// Same as DoubleTap but copies ANY card, not just Attacks.
/// Java: DuplicationPower.onUseCard checks !card.purgeOnUse && amount > 0 (no type check).
pub fn on_use_card(state: &mut CombatState, card: &CombatCard, purge: bool, target: Option<usize>) {
    if !purge {
        // Deduct power amount
        if let Some(amount) = store::with_power_mut(state, 0, PowerId::DuplicationPower, |p| {
            p.amount -= 1;
            p.amount
        }) {
            if !crate::content::powers::should_keep_power_instance(
                PowerId::DuplicationPower,
                amount,
            ) {
                store::remove_power_type(state, 0, PowerId::DuplicationPower);
            }
        }

        state.queue_action_back(Action::EnqueueCardPlay {
            item: Box::new(QueuedCardPlay::same_instance_replay_java(
                card,
                target,
                QueuedCardSource::Duplication,
            )),
            in_front: true,
        });
    }
}
