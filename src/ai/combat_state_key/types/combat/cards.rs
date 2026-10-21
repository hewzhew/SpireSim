use crate::content::cards::CardId;
use crate::runtime::combat::{CardLineageId, QueuedCardSource};
use serde::ser::SerializeStruct;
use smallvec::SmallVec;

/// Exact card-zone identity stored in one allocation.
///
/// The runtime already exposes each zone as an ordered card sequence. Keeping
/// five independent `Vec`s here paid five allocations for every transposition
/// key, even though search never mutates the projection. The boundary offsets
/// retain those five exact sequences in one backing allocation. Ordinary
/// equality and hashing follow this compact representation. Durable identity
/// uses the explicit semantic-zone serialization below, so storage packing can
/// evolve independently.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CombatZonesKey {
    pub(crate) next_combat_generated_ordinal: u32,
    cards: Vec<CombatCardKey>,
    hand_end: usize,
    draw_end: usize,
    discard_end: usize,
    exhaust_end: usize,
    pub(crate) queued: Vec<CombatQueuedCardKey>,
}

impl CombatZonesKey {
    pub(crate) fn new(
        next_combat_generated_ordinal: u32,
        hand: impl ExactSizeIterator<Item = CombatCardKey>,
        draw: impl ExactSizeIterator<Item = CombatCardKey>,
        discard: impl ExactSizeIterator<Item = CombatCardKey>,
        exhaust: impl ExactSizeIterator<Item = CombatCardKey>,
        limbo: impl ExactSizeIterator<Item = CombatCardKey>,
        queued: Vec<CombatQueuedCardKey>,
    ) -> Self {
        let hand_len = hand.len();
        let draw_len = draw.len();
        let discard_len = discard.len();
        let exhaust_len = exhaust.len();
        let limbo_len = limbo.len();
        let total_len = hand_len + draw_len + discard_len + exhaust_len + limbo_len;
        let mut cards = Vec::with_capacity(total_len);
        cards.extend(hand);
        let hand_end = cards.len();
        cards.extend(draw);
        let draw_end = cards.len();
        cards.extend(discard);
        let discard_end = cards.len();
        cards.extend(exhaust);
        let exhaust_end = cards.len();
        cards.extend(limbo);
        debug_assert_eq!(cards.len(), total_len);

        Self {
            next_combat_generated_ordinal,
            cards,
            hand_end,
            draw_end,
            discard_end,
            exhaust_end,
            queued,
        }
    }

    fn hand(&self) -> &[CombatCardKey] {
        &self.cards[..self.hand_end]
    }

    fn draw(&self) -> &[CombatCardKey] {
        &self.cards[self.hand_end..self.draw_end]
    }

    fn discard(&self) -> &[CombatCardKey] {
        &self.cards[self.draw_end..self.discard_end]
    }

    fn exhaust(&self) -> &[CombatCardKey] {
        &self.cards[self.discard_end..self.exhaust_end]
    }

    fn limbo(&self) -> &[CombatCardKey] {
        &self.cards[self.exhaust_end..]
    }
}

impl serde::Serialize for CombatZonesKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut zones = serializer.serialize_struct("CombatZonesSemanticV2", 7)?;
        zones.serialize_field(
            "next_combat_generated_ordinal",
            &self.next_combat_generated_ordinal,
        )?;
        zones.serialize_field("hand", self.hand())?;
        zones.serialize_field("draw", self.draw())?;
        zones.serialize_field("discard", self.discard())?;
        zones.serialize_field("exhaust", self.exhaust())?;
        zones.serialize_field("limbo", self.limbo())?;
        zones.serialize_field("queued", &self.queued)?;
        zones.end()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatCardKey {
    pub(crate) id: CardId,
    pub(crate) lineage: CardLineageId,
    pub(crate) upgrades: u8,
    pub(crate) misc_value: i32,
    pub(crate) base_damage_override: Option<i32>,
    pub(crate) base_block_override: Option<i32>,
    pub(crate) cost_modifier: i8,
    pub(crate) cost_for_turn: Option<u8>,
    pub(crate) base_damage_mut: i32,
    pub(crate) base_block_mut: i32,
    pub(crate) base_magic_num_mut: i32,
    pub(crate) multi_damage: SmallVec<[i32; 5]>,
    pub(crate) exhaust_override: Option<bool>,
    pub(crate) retain_override: Option<bool>,
    pub(crate) free_to_play_once: bool,
    pub(crate) energy_on_use: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatQueuedCardKey {
    pub(crate) card: CombatCardKey,
    pub(crate) target: CombatTargetKey,
    pub(crate) energy_on_use: i32,
    pub(crate) ignore_energy_total: bool,
    pub(crate) autoplay: bool,
    pub(crate) random_target: bool,
    pub(crate) is_end_turn_autoplay: bool,
    pub(crate) purge_on_use: bool,
    pub(crate) source: QueuedCardSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatTargetKey {
    None,
    MonsterSlot(usize),
    Entity(usize),
}
