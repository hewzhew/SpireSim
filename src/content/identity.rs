use serde::{Deserialize, Deserializer, Serializer};

/// Shared contract implemented by gameplay identities that cross a durable or
/// versioned boundary.
///
/// The stable string is the simulator-owned persistence key. The stable
/// ordinal is a separate numeric vocabulary key; neither is derived from the
/// Rust variant spelling or discriminant.
pub trait StableGameplayId: Copy + Sized + 'static {
    const STABLE_IDS: &'static [&'static str];

    fn stable_id(self) -> &'static str;
    fn from_stable_id(stable_id: &str) -> Option<Self>;
    fn stable_ordinal(self) -> u16;
}

/// Explicit non-durable identity surfaces for one gameplay identity.
///
/// None of these values is a persistence key. Missing facts stay `None` or
/// empty rather than falling back to a Rust symbol or the stable wire ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameplayIdentitySurfaceV1 {
    pub stable_id: &'static str,
    pub stable_ordinal: u16,
    pub display_name: Option<&'static str>,
    pub external_id: Option<&'static str>,
    pub external_aliases: Vec<&'static str>,
    pub input_aliases: Vec<&'static str>,
}

/// Presentation, external-protocol, and human-input identity facts.
///
/// This is deliberately separate from [`StableGameplayId`]: serde and model
/// vocabularies must never consult display text or permissive input aliases.
pub trait GameplayIdentitySurface: StableGameplayId {
    fn display_name_v1(self) -> Option<&'static str> {
        None
    }

    fn external_id_v1(self) -> Option<&'static str> {
        None
    }

    fn external_aliases_v1(self) -> Vec<&'static str> {
        Vec::new()
    }

    fn input_aliases_v1(self) -> Vec<&'static str> {
        Vec::new()
    }

    fn identity_surface_v1(self) -> GameplayIdentitySurfaceV1 {
        GameplayIdentitySurfaceV1 {
            stable_id: self.stable_id(),
            stable_ordinal: self.stable_ordinal(),
            display_name: self.display_name_v1(),
            external_id: self.external_id_v1(),
            external_aliases: self.external_aliases_v1(),
            input_aliases: self.input_aliases_v1(),
        }
    }

    /// Permissive matching for a human-authored selector only.
    ///
    /// Exact external protocol ingestion and durable serde must use their
    /// corresponding exact owners instead of this normalized surface.
    fn matches_input_alias_v1(self, raw: &str) -> bool {
        let requested = normalize_gameplay_input_alias(raw);
        if requested.is_empty() {
            return false;
        }
        let surface = self.identity_surface_v1();
        std::iter::once(Some(surface.stable_id))
            .chain(std::iter::once(surface.display_name))
            .chain(std::iter::once(surface.external_id))
            .flatten()
            .chain(surface.external_aliases)
            .chain(surface.input_aliases)
            .any(|candidate| normalize_gameplay_input_alias(candidate) == requested)
    }
}

pub fn normalize_gameplay_input_alias(raw: &str) -> String {
    raw.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Resolves a permissive human-authored identity only when exactly one member
/// of the complete stable vocabulary matches.
///
/// Candidate-scoped selectors may intentionally apply `matches_input_alias_v1`
/// to their visible candidates instead. Whole-vocabulary parsers must use this
/// fail-closed helper so shared display names such as the four starter Strikes
/// never resolve by declaration order.
pub fn gameplay_identity_from_input_alias_v1<T>(raw: &str) -> Option<T>
where
    T: GameplayIdentitySurface,
{
    let mut matches = T::STABLE_IDS.iter().filter_map(|stable_id| {
        let identity = T::from_stable_id(stable_id)?;
        identity.matches_input_alias_v1(raw).then_some(identity)
    });
    let identity = matches.next()?;
    matches.next().is_none().then_some(identity)
}

/// Human-readable fallback chosen explicitly at a presentation boundary.
///
/// This must not be used for persistence, equality, hashing, or model
/// vocabulary keys; those consumers use `stable_id()` or `stable_ordinal()`.
pub fn gameplay_display_label_v1<T: GameplayIdentitySurface>(identity: T) -> &'static str {
    identity
        .display_name_v1()
        .or_else(|| identity.external_id_v1())
        .unwrap_or_else(|| identity.stable_id())
}

#[doc(hidden)]
pub fn serialize_stable_gameplay_id<T, S>(value: T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: StableGameplayId,
    S: Serializer,
{
    serializer.serialize_str(value.stable_id())
}

#[doc(hidden)]
pub fn deserialize_stable_gameplay_id<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: StableGameplayId,
    D: Deserializer<'de>,
{
    let stable_id = String::deserialize(deserializer)?;
    T::from_stable_id(&stable_id)
        .ok_or_else(|| serde::de::Error::unknown_variant(&stable_id, T::STABLE_IDS))
}

/// Declares one fieldless gameplay identity without coupling its persisted or
/// numeric vocabulary identity to a Rust enum representation.
///
/// Every entry deliberately repeats all three layers at the owning declaration:
/// `RuntimeSymbol => ("StableWireId", stable_ordinal)`.
#[macro_export]
macro_rules! stable_gameplay_id {
    (
        $(#[$enum_meta:meta])*
        $visibility:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => ($stable_id:literal, $stable_ordinal:literal)
            ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        $visibility enum $name {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
        }

        impl $name {
            pub const STABLE_IDS: &'static [&'static str] = &[
                $($stable_id,)+
            ];

            pub const fn stable_id(self) -> &'static str {
                match self {
                    $(Self::$variant => $stable_id,)+
                }
            }

            pub fn from_stable_id(stable_id: &str) -> Option<Self> {
                match stable_id {
                    $($stable_id => Some(Self::$variant),)+
                    _ => None,
                }
            }

            pub const fn stable_ordinal(self) -> u16 {
                match self {
                    $(Self::$variant => $stable_ordinal,)+
                }
            }
        }

        impl $crate::content::identity::StableGameplayId for $name {
            const STABLE_IDS: &'static [&'static str] = Self::STABLE_IDS;

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

        impl ::serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                $crate::content::identity::serialize_stable_gameplay_id(*self, serializer)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                $crate::content::identity::deserialize_stable_gameplay_id(deserializer)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fmt::Debug;

    use serde::de::DeserializeOwned;

    use super::{gameplay_identity_from_input_alias_v1, GameplayIdentitySurface, StableGameplayId};

    stable_gameplay_id! {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum ExampleId {
            RuntimeName => ("durable-name", 7),
            Other => ("other", 2),
        }
    }

    impl GameplayIdentitySurface for ExampleId {
        fn display_name_v1(self) -> Option<&'static str> {
            Some("shared display")
        }
    }

    #[test]
    fn stable_identity_is_independent_from_runtime_symbol_and_order() {
        assert_eq!(
            serde_json::to_string(&ExampleId::RuntimeName).unwrap(),
            "\"durable-name\""
        );
        assert_eq!(
            serde_json::from_str::<ExampleId>("\"durable-name\"").unwrap(),
            ExampleId::RuntimeName
        );
        assert!(serde_json::from_str::<ExampleId>("\"RuntimeName\"").is_err());
        assert_eq!(ExampleId::RuntimeName.stable_ordinal(), 7);
        assert_eq!(ExampleId::Other.stable_ordinal(), 2);
    }

    #[test]
    fn whole_vocabulary_input_matching_fails_closed_on_ambiguous_display_text() {
        assert_eq!(
            gameplay_identity_from_input_alias_v1::<ExampleId>("durable-name"),
            Some(ExampleId::RuntimeName)
        );
        assert_eq!(
            gameplay_identity_from_input_alias_v1::<ExampleId>("shared display"),
            None
        );
    }

    fn assert_complete_contract<T>(expected_count: usize)
    where
        T: StableGameplayId + Debug + Eq + DeserializeOwned + serde::Serialize,
    {
        assert_eq!(T::STABLE_IDS.len(), expected_count);
        let mut ordinals = BTreeSet::new();
        for &stable_id in T::STABLE_IDS {
            let id = T::from_stable_id(stable_id).expect("declared stable ID should resolve");
            assert!(ordinals.insert(id.stable_ordinal()));
            let payload = serde_json::to_string(&id).expect("stable identity should serialize");
            assert_eq!(payload, format!("\"{stable_id}\""));
            assert_eq!(
                serde_json::from_str::<T>(&payload).expect("stable identity should deserialize"),
                id
            );
        }
        assert_eq!(
            ordinals,
            (0..expected_count as u16).collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn every_gameplay_identity_has_a_complete_stable_contract() {
        assert_complete_contract::<crate::content::cards::CardId>(371);
        assert_complete_contract::<crate::content::potions::PotionId>(42);
        assert_complete_contract::<crate::content::monsters::EnemyId>(65);
        assert_complete_contract::<crate::content::monsters::factory::EncounterId>(65);
        assert_complete_contract::<crate::state::events::EventId>(54);
        assert_complete_contract::<crate::content::powers::PowerId>(136);
        assert_complete_contract::<crate::runtime::combat::OrbId>(5);
        assert_complete_contract::<crate::runtime::combat::StanceId>(4);
        assert_complete_contract::<crate::content::relics::RelicId>(182);
    }

    #[test]
    fn non_durable_surfaces_do_not_fall_back_to_runtime_or_stable_names() {
        use crate::content::cards::CardId;
        use crate::content::monsters::factory::EncounterId;
        use crate::content::monsters::EnemyId;
        use crate::content::potions::PotionId;
        use crate::content::powers::PowerId;
        use crate::content::relics::RelicId;
        use crate::runtime::combat::{OrbId, StanceId};
        use crate::state::events::EventId;

        let card = CardId::StrikeG.identity_surface_v1();
        assert_eq!(card.stable_id, "StrikeG");
        assert_eq!(card.display_name, Some("Strike"));
        assert_eq!(card.external_id, Some("Strike_G"));
        assert!(CardId::StrikeG.matches_input_alias_v1("Strike_G"));
        assert!(CardId::StrikeG.matches_input_alias_v1("Strike"));

        let potion = PotionId::WeakenPotion.identity_surface_v1();
        assert_eq!(potion.stable_id, "WeakenPotion");
        assert_eq!(potion.display_name, Some("Weak Potion"));
        assert!(PotionId::WeakenPotion.matches_input_alias_v1("Weak Potion"));

        let enemy = EnemyId::Healer.identity_surface_v1();
        assert_eq!(enemy.stable_id, "Healer");
        assert_eq!(enemy.display_name, Some("Mystic"));
        assert!(EnemyId::Healer.matches_input_alias_v1("Mystic"));

        let event = EventId::BackTotheBasics.identity_surface_v1();
        assert_eq!(event.stable_id, "BackTotheBasics");
        assert_eq!(event.display_name, Some("Ancient Writing"));
        assert!(EventId::BackTotheBasics.matches_input_alias_v1("Back to Basics"));

        let power = PowerId::FlameBarrier.identity_surface_v1();
        assert_eq!(power.stable_id, "FlameBarrier");
        assert_eq!(power.display_name, Some("Flame Barrier"));

        let relic = RelicId::PhilosopherStone.identity_surface_v1();
        assert_eq!(relic.stable_id, "PhilosopherStone");
        assert_eq!(relic.display_name, None);
        assert_eq!(relic.external_id, Some("Philosopher's Stone"));
        assert!(RelicId::PhilosopherStone.matches_input_alias_v1("PhilosophersStone"));

        assert_eq!(
            OrbId::Lightning.identity_surface_v1().display_name,
            Some("Lightning")
        );
        assert_eq!(
            StanceId::Wrath.identity_surface_v1().display_name,
            Some("Wrath")
        );

        let encounter = EncounterId::TwoFungiBeasts.identity_surface_v1();
        assert_eq!(encounter.stable_id, "TwoFungiBeasts");
        assert_eq!(encounter.display_name, None);
        assert_eq!(encounter.external_id, None);
    }
}
