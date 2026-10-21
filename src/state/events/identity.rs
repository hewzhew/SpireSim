use crate::content::identity::GameplayIdentitySurface;

use super::EventId;

/// Exact spellings accepted only when reading pre-protocol save/status evidence.
///
/// This is not a display-name table and does not select a canonical external
/// ID. Retired serialized IDs never appear as values in this table.
pub const LEGACY_EVENT_NAME_ALIASES: &[(&str, EventId)] = &[
    ("Big Fish", EventId::BigFish),
    ("Cleric", EventId::Cleric),
    ("The Cleric", EventId::Cleric),
    ("Dead Adventurer", EventId::DeadAdventurer),
    ("Golden Idol", EventId::GoldenIdol),
    ("Living Wall", EventId::LivingWall),
    ("Hypnotizing Colored Mushrooms", EventId::Mushrooms),
    ("Mushrooms", EventId::Mushrooms),
    ("Scrap Ooze", EventId::ScrapOoze),
    ("Shining Light", EventId::ShiningLight),
    ("Ssssserpent", EventId::Ssssserpent),
    ("The Ssssserpent", EventId::Ssssserpent),
    ("Liars Game", EventId::Ssssserpent),
    ("World of Goop", EventId::WorldOfGoop),
    ("Golden Wing", EventId::GoldenWing),
    ("Wing Statue", EventId::GoldenWing),
    ("Match and Keep", EventId::MatchAndKeep),
    ("Match and Keep!", EventId::MatchAndKeep),
    ("Golden Shrine", EventId::GoldenShrine),
    ("Addict", EventId::Addict),
    ("Ancient Writing", EventId::BackTotheBasics),
    ("Back to Basics", EventId::BackTotheBasics),
    ("Old Beggar", EventId::Beggar),
    ("Beggar", EventId::Beggar),
    ("The Colosseum", EventId::Colosseum),
    ("Colosseum", EventId::Colosseum),
    ("Ghosts", EventId::Ghosts),
    ("Drug Dealer", EventId::DrugDealer),
    ("Knowing Skull", EventId::KnowingSkull),
    ("Masked Bandits", EventId::MaskedBandits),
    ("Mausoleum", EventId::Mausoleum),
    ("The Mausoleum", EventId::Mausoleum),
    ("Nest", EventId::Nest),
    ("Nloth", EventId::Nloth),
    ("N'loth", EventId::Nloth),
    ("The Joust", EventId::TheJoust),
    ("The Library", EventId::TheLibrary),
    ("Vampires", EventId::Vampires),
    ("Cursed Tome", EventId::CursedTome),
    ("Winding Halls", EventId::WindingHalls),
    ("Forgotten Altar", EventId::ForgottenAltar),
    ("Mind Bloom", EventId::MindBloom),
    ("MindBloom", EventId::MindBloom),
    ("Moai Head", EventId::MoaiHead),
    ("The Moai Head", EventId::MoaiHead),
    ("Mysterious Sphere", EventId::MysteriousSphere),
    ("Sensory Stone", EventId::SensoryStone),
    ("SensoryStone", EventId::SensoryStone),
    ("Tomb Red Mask", EventId::TombRedMask),
    ("Tomb of Lord Red Mask", EventId::TombRedMask),
    ("Accursed Blacksmith", EventId::AccursedBlacksmith),
    ("Bonfire Elementals", EventId::BonfireElementals),
    ("Bonfire Spirits", EventId::BonfireElementals),
    ("Designer", EventId::Designer),
    ("Duplicator", EventId::Duplicator),
    ("Face Trader", EventId::FaceTrader),
    ("FaceTrader", EventId::FaceTrader),
    (
        "Fountain of Curse Cleansing",
        EventId::FountainOfCurseCleansing,
    ),
    ("The Divine Fountain", EventId::FountainOfCurseCleansing),
    ("Fountain of Cleansing", EventId::FountainOfCurseCleansing),
    ("Gremlin Wheel Game", EventId::GremlinWheelGame),
    ("Wheel of Change", EventId::GremlinWheelGame),
    ("Lab", EventId::Lab),
    ("A Note For Yourself", EventId::NoteForYourself),
    ("Note For Yourself", EventId::NoteForYourself),
    ("NoteForYourself", EventId::NoteForYourself),
    ("Purification Shrine", EventId::Purifier),
    ("Purifier", EventId::Purifier),
    ("SecretPortal", EventId::SecretPortal),
    ("Secret Portal", EventId::SecretPortal),
    ("Transmogrifier", EventId::Transmorgrifier),
    ("Transmorgrifier", EventId::Transmorgrifier),
    ("Upgrade Shrine", EventId::UpgradeShrine),
    ("We Meet Again!", EventId::WeMeetAgain),
    ("We Meet Again", EventId::WeMeetAgain),
    ("WeMeetAgain", EventId::WeMeetAgain),
    ("Falling", EventId::Falling),
    ("Woman in Blue", EventId::WomanInBlue),
    ("The Woman in Blue", EventId::WomanInBlue),
    ("Neow", EventId::Neow),
    ("Neow Event", EventId::Neow),
    ("Spire Heart", EventId::SpireHeart),
];

/// Canonical original-game English presentation text.
///
/// Keeping this match separate prevents alias-table ordering from silently
/// changing UI output. The retired compatibility identity deliberately has no
/// display value.
pub const fn event_display_name_v1(event_id: EventId) -> Option<&'static str> {
    Some(match event_id {
        EventId::BigFish => "Big Fish",
        EventId::Cleric => "The Cleric",
        EventId::DeadAdventurer => "Dead Adventurer",
        EventId::GoldenIdol => "Golden Idol",
        EventId::LivingWall => "Living Wall",
        EventId::Mushrooms => "Hypnotizing Colored Mushrooms",
        EventId::ScrapOoze => "Scrap Ooze",
        EventId::ShiningLight => "Shining Light",
        EventId::Ssssserpent => "The Ssssserpent",
        EventId::WorldOfGoop => "World of Goop",
        EventId::GoldenWing => "Wing Statue",
        EventId::MatchAndKeep => "Match and Keep!",
        EventId::GoldenShrine => "Golden Shrine",
        EventId::Addict => "Addict",
        EventId::BackTotheBasics => "Ancient Writing",
        EventId::Beggar => "Old Beggar",
        EventId::Colosseum => "The Colosseum",
        EventId::CursedTome => "Cursed Tome",
        EventId::DrugDealer => "Drug Dealer",
        EventId::ForgottenAltar => "Forgotten Altar",
        EventId::Ghosts => "Ghosts",
        EventId::KnowingSkull => "Knowing Skull",
        EventId::MaskedBandits => "Masked Bandits",
        EventId::Mausoleum => "The Mausoleum",
        EventId::Nest => "Nest",
        EventId::Nloth => "N'loth",
        EventId::TheJoust => "The Joust",
        EventId::TheLibrary => "The Library",
        EventId::Vampires => "Vampires",
        EventId::Falling => "Falling",
        EventId::MindBloom => "Mind Bloom",
        EventId::MoaiHead => "The Moai Head",
        EventId::MysteriousSphere => "Mysterious Sphere",
        EventId::SensoryStone => "Sensory Stone",
        EventId::TombRedMask => "Tomb of Lord Red Mask",
        EventId::WindingHalls => "Winding Halls",
        EventId::AccursedBlacksmith => "Accursed Blacksmith",
        EventId::BonfireElementals => "Bonfire Spirits",
        EventId::BonfireSpirits => return None,
        EventId::Designer => "Designer",
        EventId::Duplicator => "Duplicator",
        EventId::FaceTrader => "Face Trader",
        EventId::FountainOfCurseCleansing => "The Divine Fountain",
        EventId::GremlinWheelGame => "Wheel of Change",
        EventId::Lab => "Lab",
        EventId::NoteForYourself => "A Note For Yourself",
        EventId::Purifier => "Purification Shrine",
        EventId::SecretPortal => "Secret Portal",
        EventId::Transmorgrifier => "Transmogrifier",
        EventId::UpgradeShrine => "Upgrade Shrine",
        EventId::WeMeetAgain => "We Meet Again!",
        EventId::WomanInBlue => "The Woman in Blue",
        EventId::Neow => "Neow",
        EventId::SpireHeart => "Spire Heart",
    })
}

/// Reads pre-protocol display identities from legacy save/status evidence.
///
/// Current parity frames carry `EventId::stable_id()` and must not pass
/// through this presentation-name compatibility reader.
pub fn event_id_from_legacy_live_name(raw: &str) -> Option<EventId> {
    let trimmed = raw.trim();
    LEGACY_EVENT_NAME_ALIASES
        .iter()
        .find_map(|(name, event_id)| (*name == trimmed).then_some(*event_id))
        .or_else(|| match trimmed.to_ascii_lowercase().as_str() {
            "world of goop" => Some(EventId::WorldOfGoop),
            "big fish" => Some(EventId::BigFish),
            "ghosts" => Some(EventId::Ghosts),
            "designer" => Some(EventId::Designer),
            _ => None,
        })
}

impl GameplayIdentitySurface for EventId {
    fn display_name_v1(self) -> Option<&'static str> {
        event_display_name_v1(self)
    }

    fn external_aliases_v1(self) -> Vec<&'static str> {
        LEGACY_EVENT_NAME_ALIASES
            .iter()
            .filter_map(|(name, event_id)| (*event_id == self).then_some(*name))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::state::events::RETIRED_EVENT_IDS;

    #[test]
    fn original_spire_heart_name_resolves_without_changing_stable_identity() {
        assert_eq!(
            event_id_from_legacy_live_name("Spire Heart"),
            Some(EventId::SpireHeart)
        );
        assert_eq!(EventId::SpireHeart.stable_id(), "SpireHeart");
        assert_eq!(EventId::SpireHeart.stable_ordinal(), 53);
    }

    #[test]
    fn every_live_event_has_display_and_exact_inbound_aliases_and_retired_ids_do_not() {
        let represented = LEGACY_EVENT_NAME_ALIASES
            .iter()
            .map(|(_, event_id)| *event_id)
            .collect::<HashSet<_>>();
        for stable_id in EventId::STABLE_IDS {
            let event_id = EventId::from_stable_id(stable_id).unwrap();
            assert_eq!(
                represented.contains(&event_id),
                !RETIRED_EVENT_IDS.contains(&event_id),
                "event identity coverage for {stable_id}"
            );
            assert_eq!(
                event_display_name_v1(event_id).is_some(),
                !RETIRED_EVENT_IDS.contains(&event_id),
                "event display coverage for {stable_id}"
            );
        }
    }
}
