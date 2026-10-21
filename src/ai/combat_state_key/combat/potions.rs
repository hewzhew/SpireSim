use crate::runtime::combat::CombatState;

use super::super::types::{CombatPotionKey, CombatPotionSlotKey};

pub(super) fn potions_key(combat: &CombatState) -> Vec<CombatPotionSlotKey> {
    combat
        .entities
        .potions
        .iter()
        .enumerate()
        .map(|(slot, potion)| CombatPotionSlotKey {
            slot,
            potion: potion.as_ref().map(|potion| CombatPotionKey {
                id: potion.id,
                uuid: potion.uuid,
                can_use: potion.can_use,
                can_discard: potion.can_discard,
                requires_target: potion.requires_target,
            }),
        })
        .collect()
}
