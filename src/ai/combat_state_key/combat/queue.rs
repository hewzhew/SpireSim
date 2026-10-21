use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

use super::super::types::CombatQueuedActionKey;

pub(super) fn queue_key(combat: &CombatState) -> Vec<CombatQueuedActionKey> {
    combat.engine.action_queue.iter().map(action_key).collect()
}

fn action_key(action: &Action) -> CombatQueuedActionKey {
    CombatQueuedActionKey {
        canonical_payload: serde_json::to_string(action)
            .expect("queued combat action should serialize deterministically"),
    }
}
