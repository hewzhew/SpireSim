use super::{CardLineageId, CombatCard};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;

/// Immutable-by-default deck image captured at combat start.
///
/// Exact search clones a combat state for every atomic successor. Sharing this
/// normally stable slice avoids cloning the whole deck at every edge. The rare
/// in-combat mutation uses `with_cards_mut`, which preserves branch isolation
/// and refreshes the cached structural hash before returning.
#[derive(Clone)]
pub struct MasterDeckSnapshot {
    cards: Arc<[CombatCard]>,
    structural_hash: u64,
}

impl MasterDeckSnapshot {
    pub fn with_cards_mut<R>(&mut self, mutate: impl FnOnce(&mut [CombatCard]) -> R) -> R {
        let result = mutate(Arc::make_mut(&mut self.cards));
        self.structural_hash = master_deck_structural_hash(&self.cards);
        result
    }

    /// Remove one exact card while preserving the snapshot's copy-on-write
    /// storage and structural hash contract.
    pub fn remove_card_by_lineage(&mut self, lineage: CardLineageId) -> Option<CombatCard> {
        let mut cards = self.cards.to_vec();
        let index = cards.iter().position(|card| card.lineage == lineage)?;
        let removed = cards.remove(index);
        *self = cards.into();
        Some(removed)
    }

    pub(crate) fn structural_hash(&self) -> u64 {
        self.structural_hash
    }

    #[cfg(test)]
    pub(crate) fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cards, &other.cards)
    }

    #[cfg(test)]
    pub(crate) fn force_structural_hash_for_test(&mut self, structural_hash: u64) {
        self.structural_hash = structural_hash;
    }
}

impl From<Vec<CombatCard>> for MasterDeckSnapshot {
    fn from(cards: Vec<CombatCard>) -> Self {
        let cards: Arc<[CombatCard]> = cards.into();
        let structural_hash = master_deck_structural_hash(&cards);
        Self {
            cards,
            structural_hash,
        }
    }
}

impl Default for MasterDeckSnapshot {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl Deref for MasterDeckSnapshot {
    type Target = [CombatCard];

    fn deref(&self) -> &Self::Target {
        &self.cards
    }
}

impl AsRef<[CombatCard]> for MasterDeckSnapshot {
    fn as_ref(&self) -> &[CombatCard] {
        &self.cards
    }
}

impl std::fmt::Debug for MasterDeckSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cards.fmt(formatter)
    }
}

impl PartialEq for MasterDeckSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.cards == other.cards
    }
}

impl Eq for MasterDeckSnapshot {}

impl Hash for MasterDeckSnapshot {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.structural_hash);
    }
}

impl Serialize for MasterDeckSnapshot {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(self.cards.as_ref(), serializer)
    }
}

impl<'de> Deserialize<'de> for MasterDeckSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<CombatCard>::deserialize(deserializer).map(Into::into)
    }
}

fn master_deck_structural_hash(cards: &[CombatCard]) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    cards.len().hash(&mut hasher);
    for card in cards {
        card.id.hash(&mut hasher);
        card.lineage.hash(&mut hasher);
        card.upgrades.hash(&mut hasher);
        card.misc_value.hash(&mut hasher);
        card.base_damage_override.hash(&mut hasher);
        card.base_block_override.hash(&mut hasher);
        card.cost_modifier.hash(&mut hasher);
        card.cost_for_turn.hash(&mut hasher);
        card.base_damage_mut.hash(&mut hasher);
        card.base_block_mut.hash(&mut hasher);
        card.base_magic_num_mut.hash(&mut hasher);
        card.multi_damage.hash(&mut hasher);
        card.exhaust_override.hash(&mut hasher);
        card.retain_override.hash(&mut hasher);
        card.free_to_play_once.hash(&mut hasher);
        card.energy_on_use.hash(&mut hasher);
    }
    hasher.finish()
}
