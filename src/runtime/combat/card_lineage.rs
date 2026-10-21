use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

const ORIGIN_SHIFT: u32 = 30;
const ORDINAL_MASK: u32 = (1 << ORIGIN_SHIFT) - 1;
const MASTER_DECK_TAG: u32 = 0;
const COMBAT_GENERATED_TAG: u32 = 1;
const CONSTRUCTED_SCENARIO_TAG: u32 = 2;

/// The semantic source of one Rust-local card lineage.
///
/// This is deliberately distinct from Java's random UUID value.  The compact
/// encoding is private and must not be used for ordering or cross-runtime
/// matching.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CardLineageOrigin {
    MasterDeck,
    CombatGenerated,
    ConstructedScenario,
}

/// Stable identity for one logical card lineage inside a Rust exact state.
///
/// Ordinals are meaningful only together with their typed origin. They may be
/// interpreted as an acquisition, generation, or recipe order only by the
/// owner that issued that origin; they are never Java identity and must not be
/// compared across origins or runtimes.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CardLineageId(u32);

impl CardLineageId {
    pub fn master_deck(acquisition_ordinal: u32) -> Self {
        Self::encode(CardLineageOrigin::MasterDeck, acquisition_ordinal)
    }

    pub fn combat_generated(creation_ordinal: u32) -> Self {
        Self::encode(CardLineageOrigin::CombatGenerated, creation_ordinal)
    }

    pub fn constructed_scenario(recipe_slot: u32) -> Self {
        Self::encode(CardLineageOrigin::ConstructedScenario, recipe_slot)
    }

    pub fn origin(self) -> CardLineageOrigin {
        match self.0 >> ORIGIN_SHIFT {
            MASTER_DECK_TAG => CardLineageOrigin::MasterDeck,
            COMBAT_GENERATED_TAG => CardLineageOrigin::CombatGenerated,
            CONSTRUCTED_SCENARIO_TAG => CardLineageOrigin::ConstructedScenario,
            _ => unreachable!("CardLineageId can only be built by typed constructors"),
        }
    }

    pub fn ordinal(self) -> u32 {
        self.0 & ORDINAL_MASK
    }

    pub(crate) fn compact_runtime_value(self) -> u32 {
        self.0
    }

    /// Narrow bridge for legacy simulator payload fields that still store an
    /// opaque signed integer. Domain code must decode it immediately and must
    /// never compare the integer itself with Java UUID text.
    pub(crate) fn to_runtime_i32_payload(self) -> i32 {
        self.0 as i32
    }

    pub(crate) fn from_runtime_i32_payload(value: i32) -> Option<Self> {
        Self::from_compact_runtime_value(value as u32)
    }

    pub(crate) fn from_compact_runtime_value(value: u32) -> Option<Self> {
        match value >> ORIGIN_SHIFT {
            MASTER_DECK_TAG | COMBAT_GENERATED_TAG | CONSTRUCTED_SCENARIO_TAG => Some(Self(value)),
            _ => None,
        }
    }

    fn encode(origin: CardLineageOrigin, ordinal: u32) -> Self {
        assert!(
            ordinal <= ORDINAL_MASK,
            "card lineage ordinal {ordinal} exceeds the supported range"
        );
        let tag = match origin {
            CardLineageOrigin::MasterDeck => MASTER_DECK_TAG,
            CardLineageOrigin::CombatGenerated => COMBAT_GENERATED_TAG,
            CardLineageOrigin::ConstructedScenario => CONSTRUCTED_SCENARIO_TAG,
        };
        Self((tag << ORIGIN_SHIFT) | ordinal)
    }

    fn wire_prefix(self) -> &'static str {
        match self.origin() {
            CardLineageOrigin::MasterDeck => "master_deck",
            CardLineageOrigin::CombatGenerated => "combat_generated",
            CardLineageOrigin::ConstructedScenario => "constructed_scenario",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MasterDeckLineageAllocator {
    next_acquisition_ordinal: u32,
}

impl MasterDeckLineageAllocator {
    pub fn new() -> Self {
        Self {
            // Zero remains reserved for legacy payload fields whose original
            // owner uses `0` as "no card selected".
            next_acquisition_ordinal: 1,
        }
    }

    pub fn allocate(&mut self) -> CardLineageId {
        let ordinal = self.next_acquisition_ordinal;
        let lineage = CardLineageId::master_deck(ordinal);
        self.next_acquisition_ordinal = ordinal
            .checked_add(1)
            .expect("master-deck card lineage allocator counter overflowed");
        lineage
    }

    pub fn next_acquisition_ordinal(&self) -> u32 {
        self.next_acquisition_ordinal
    }

    pub fn contains_allocated(&self, lineage: CardLineageId) -> bool {
        lineage.origin() == CardLineageOrigin::MasterDeck
            && lineage.ordinal() > 0
            && lineage.ordinal() < self.next_acquisition_ordinal
    }
}

impl Default for MasterDeckLineageAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatGeneratedLineageAllocator {
    next_creation_ordinal: u32,
}

impl CombatGeneratedLineageAllocator {
    pub fn new() -> Self {
        Self {
            next_creation_ordinal: 1,
        }
    }

    pub fn allocate(&mut self) -> CardLineageId {
        let ordinal = self.next_creation_ordinal;
        let lineage = CardLineageId::combat_generated(ordinal);
        self.next_creation_ordinal = ordinal
            .checked_add(1)
            .expect("combat-generated card lineage allocator counter overflowed");
        lineage
    }

    pub fn next_creation_ordinal(&self) -> u32 {
        self.next_creation_ordinal
    }

    pub fn contains_allocated(&self, lineage: CardLineageId) -> bool {
        lineage.origin() == CardLineageOrigin::CombatGenerated
            && lineage.ordinal() > 0
            && lineage.ordinal() < self.next_creation_ordinal
    }
}

impl Default for CombatGeneratedLineageAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for CardLineageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for CardLineageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.wire_prefix(), self.ordinal())
    }
}

impl FromStr for CardLineageId {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let (origin, ordinal) = raw
            .split_once(':')
            .ok_or_else(|| "card lineage must have `<origin>:<ordinal>` form".to_owned())?;
        let ordinal = ordinal
            .parse::<u32>()
            .map_err(|error| format!("invalid card lineage ordinal: {error}"))?;
        match origin {
            "master_deck" => Ok(Self::master_deck(ordinal)),
            "combat_generated" => Ok(Self::combat_generated(ordinal)),
            "constructed_scenario" => Ok(Self::constructed_scenario(ordinal)),
            _ => Err(format!("unknown card lineage origin `{origin}`")),
        }
    }
}

impl Serialize for CardLineageId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CardLineageId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_do_not_alias_at_the_same_ordinal() {
        let master = CardLineageId::master_deck(7);
        let generated = CardLineageId::combat_generated(7);
        let scenario = CardLineageId::constructed_scenario(7);

        assert_ne!(master, generated);
        assert_ne!(master, scenario);
        assert_ne!(generated, scenario);
    }

    #[test]
    fn wire_form_is_typed_and_rejects_historical_bare_numbers() {
        let lineage = CardLineageId::master_deck(12);
        assert_eq!(
            serde_json::to_string(&lineage).unwrap(),
            r#""master_deck:12""#
        );
        assert_eq!(
            serde_json::from_str::<CardLineageId>(r#""master_deck:12""#).unwrap(),
            lineage
        );
        assert!(serde_json::from_str::<CardLineageId>("12").is_err());
    }

    #[test]
    fn compact_encoding_round_trips_only_inside_runtime_owner() {
        for lineage in [
            CardLineageId::master_deck(1),
            CardLineageId::combat_generated(2),
            CardLineageId::constructed_scenario(3),
        ] {
            assert_eq!(
                CardLineageId::from_compact_runtime_value(lineage.compact_runtime_value()),
                Some(lineage)
            );
        }
        assert_eq!(
            CardLineageId::from_compact_runtime_value(3 << ORIGIN_SHIFT),
            None
        );
    }

    #[test]
    fn allocators_are_monotonic_and_keep_origins_separate() {
        let mut master = MasterDeckLineageAllocator::new();
        let first_master = master.allocate();
        let second_master = master.allocate();
        let mut combat = CombatGeneratedLineageAllocator::new();
        let first_combat = combat.allocate();

        assert_eq!(first_master, CardLineageId::master_deck(1));
        assert_eq!(second_master, CardLineageId::master_deck(2));
        assert_eq!(first_combat, CardLineageId::combat_generated(1));
        assert_ne!(first_master, first_combat);
        assert_eq!(master.next_acquisition_ordinal(), 3);
        assert_eq!(combat.next_creation_ordinal(), 2);
    }
}
