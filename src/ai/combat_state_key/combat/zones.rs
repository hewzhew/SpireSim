use crate::runtime::combat::{CombatCard, CombatState};

use super::super::types::{CombatCardKey, CombatQueuedCardKey, CombatTargetKey, CombatZonesKey};
use super::cards::card_key;

pub(super) fn zones_key(combat: &CombatState) -> CombatZonesKey {
    CombatZonesKey::new(
        combat
            .zones
            .combat_lineage_allocator
            .next_creation_ordinal(),
        zone_keys(&combat.zones.hand),
        zone_keys(&combat.zones.draw_pile),
        zone_keys(&combat.zones.discard_pile),
        zone_keys(&combat.zones.exhaust_pile),
        zone_keys(&combat.zones.limbo),
        combat
            .zones
            .queued_cards
            .iter()
            .map(|queued| CombatQueuedCardKey {
                card: card_key(&queued.card),
                target: target_key(combat, queued.target),
                energy_on_use: queued.energy_on_use,
                ignore_energy_total: queued.ignore_energy_total,
                autoplay: queued.autoplay,
                random_target: queued.random_target,
                is_end_turn_autoplay: queued.is_end_turn_autoplay,
                purge_on_use: queued.purge_on_use,
                source: queued.source,
            })
            .collect(),
    )
}

fn zone_keys<'a, I>(cards: I) -> impl ExactSizeIterator<Item = CombatCardKey>
where
    I: IntoIterator<Item = &'a CombatCard>,
    I::IntoIter: ExactSizeIterator,
{
    cards.into_iter().map(card_key)
}

pub(super) fn target_key(combat: &CombatState, target: Option<usize>) -> CombatTargetKey {
    match target {
        None => CombatTargetKey::None,
        Some(entity_id) => combat
            .entities
            .monsters
            .iter()
            .position(|monster| monster.id == entity_id)
            .map(CombatTargetKey::MonsterSlot)
            .unwrap_or(CombatTargetKey::Entity(entity_id)),
    }
}
