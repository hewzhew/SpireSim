use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::RelicId;
use crate::content::identity::StableGameplayId;

include!(concat!(env!("OUT_DIR"), "/generated_relic_identity.rs"));

/// Versioned identity facts for one runtime relic.
///
/// `stable_id` is the only value admitted by durable serde. `external_id` and
/// `external_aliases` are exact original-game protocol spellings and are never
/// emitted as simulator persistence keys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelicIdentityV1 {
    pub stable_id: &'static str,
    pub external_id: &'static str,
    pub external_aliases: &'static [&'static str],
}

impl RelicId {
    pub const fn identity_v1(self) -> RelicIdentityV1 {
        RelicIdentityV1 {
            stable_id: relic_stable_id_v1(self),
            external_id: relic_external_id_v1(self),
            external_aliases: relic_external_aliases_v1(self),
        }
    }

    /// Simulator-owned durable wire identity. This is deliberately independent
    /// from the Rust variant spelling even when both strings currently match.
    pub const fn stable_id(self) -> &'static str {
        relic_stable_id_v1(self)
    }

    pub fn from_stable_id(stable_id: &str) -> Option<Self> {
        relic_id_from_stable_id_v1(stable_id)
    }

    /// Stable numeric vocabulary identity. This is independent from the Rust
    /// enum discriminant even while both currently share the same ordering.
    pub const fn stable_ordinal(self) -> u16 {
        relic_stable_ordinal_v1(self)
    }

    /// Exact original-game protocol ID, distinct from localized display text.
    pub const fn external_id(self) -> &'static str {
        relic_external_id_v1(self)
    }

    /// Additional exact protocol spellings retained by the identity contract.
    pub const fn external_aliases(self) -> &'static [&'static str] {
        relic_external_aliases_v1(self)
    }

    pub fn from_external_id_exact(external_id: &str) -> Option<Self> {
        relic_id_from_external_id_exact_v1(external_id)
    }

    /// Permissive parser for human-authored boundaries such as combat fixtures.
    /// It is not used by durable serde or exact protocol ingestion.
    pub fn from_input_alias_v1(raw: &str) -> Option<Self> {
        crate::content::identity::gameplay_identity_from_input_alias_v1(raw)
    }
}

impl StableGameplayId for RelicId {
    const STABLE_IDS: &'static [&'static str] = RELIC_STABLE_IDS_V1;

    fn stable_id(self) -> &'static str {
        self.stable_id()
    }

    fn from_stable_id(stable_id: &str) -> Option<Self> {
        Self::from_stable_id(stable_id)
    }

    fn stable_ordinal(self) -> u16 {
        self.stable_ordinal()
    }
}

impl crate::content::identity::GameplayIdentitySurface for RelicId {
    fn display_name_v1(self) -> Option<&'static str> {
        match self {
            Self::CaptainsWheel => Some("Captain's Wheel"),
            Self::CeramicFish => Some("Ceramic Fish"),
            Self::FrozenEgg => Some("Frozen Egg"),
            Self::MoltenEgg => Some("Molten Egg"),
            Self::ToxicEgg => Some("Toxic Egg"),
            Self::MawBank => Some("Maw Bank"),
            Self::OrangePellets => Some("Orange Pellets"),
            Self::Sling => Some("Sling of Courage"),
            _ => None,
        }
    }

    fn external_id_v1(self) -> Option<&'static str> {
        Some(self.external_id())
    }

    fn external_aliases_v1(self) -> Vec<&'static str> {
        self.external_aliases().to_vec()
    }
}

impl Serialize for RelicId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.stable_id())
    }
}

impl<'de> Deserialize<'de> for RelicId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let stable_id = String::deserialize(deserializer)?;
        Self::from_stable_id(&stable_id)
            .ok_or_else(|| serde::de::Error::unknown_variant(&stable_id, RELIC_STABLE_IDS_V1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_relic_identity_is_not_an_external_or_legacy_alias() {
        assert_eq!(
            serde_json::to_string(&RelicId::PhilosopherStone).unwrap(),
            "\"PhilosopherStone\""
        );
        assert_eq!(
            serde_json::from_str::<RelicId>("\"PhilosopherStone\"").unwrap(),
            RelicId::PhilosopherStone
        );
        assert!(serde_json::from_str::<RelicId>("\"Philosopher's Stone\"").is_err());
        assert!(serde_json::from_str::<RelicId>("\"PhilosophersStone\"").is_err());
    }

    #[test]
    fn philosopher_stone_keeps_runtime_wire_and_external_id_distinct() {
        let identity = RelicId::PhilosopherStone.identity_v1();
        assert_eq!(identity.stable_id, "PhilosopherStone");
        assert_eq!(identity.external_id, "Philosopher's Stone");
        assert!(identity.external_aliases.is_empty());
        assert_eq!(
            RelicId::from_external_id_exact("Philosopher's Stone"),
            Some(RelicId::PhilosopherStone)
        );
        assert_eq!(RelicId::from_external_id_exact("PhilosophersStone"), None);
        assert_eq!(
            RelicId::from_input_alias_v1("PhilosophersStone"),
            Some(RelicId::PhilosopherStone)
        );
    }

    #[test]
    fn every_stable_relic_id_round_trips_through_durable_serde() {
        for stable_id in RELIC_STABLE_IDS_V1 {
            let relic = RelicId::from_stable_id(stable_id).expect("generated stable ID");
            assert_eq!(relic.stable_id(), *stable_id);
            let payload = serde_json::to_string(&relic).unwrap();
            assert_eq!(serde_json::from_str::<RelicId>(&payload).unwrap(), relic);
        }
    }

    #[test]
    fn display_names_do_not_change_relic_persistence_keys() {
        use crate::content::identity::GameplayIdentitySurface;

        for (relic, display) in [
            (RelicId::CaptainsWheel, "Captain's Wheel"),
            (RelicId::MawBank, "Maw Bank"),
        ] {
            assert_eq!(relic.display_name_v1(), Some(display));
            assert_eq!(RelicId::from_stable_id(relic.stable_id()), Some(relic));
        }
    }
}
