pub mod context;
pub mod generator;
mod identity;

pub use identity::{
    event_display_name_v1, event_id_from_legacy_live_name, LEGACY_EVENT_NAME_ALIASES,
};

use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use serde::{Deserialize, Serialize};

crate::stable_gameplay_id! {
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventId {
    // Act 1
    BigFish => ("BigFish", 0),
    Cleric => ("Cleric", 1),
    DeadAdventurer => ("DeadAdventurer", 2),
    GoldenIdol => ("GoldenIdol", 3),
    LivingWall => ("LivingWall", 4),
    Mushrooms => ("Mushrooms", 5),
    ScrapOoze => ("ScrapOoze", 6),
    ShiningLight => ("ShiningLight", 7),
    Ssssserpent => ("Ssssserpent", 8),
    WorldOfGoop => ("WorldOfGoop", 9),
    GoldenWing => ("GoldenWing", 10),
    MatchAndKeep => ("MatchAndKeep", 11),
    GoldenShrine => ("GoldenShrine", 12),

    // Act 2
    Addict => ("Addict", 13),
    BackTotheBasics => ("BackTotheBasics", 14),
    Beggar => ("Beggar", 15),
    Colosseum => ("Colosseum", 16),
    CursedTome => ("CursedTome", 17),
    DrugDealer => ("DrugDealer", 18),
    ForgottenAltar => ("ForgottenAltar", 19),
    Ghosts => ("Ghosts", 20),
    KnowingSkull => ("KnowingSkull", 21),
    MaskedBandits => ("MaskedBandits", 22),
    Mausoleum => ("Mausoleum", 23),
    Nest => ("Nest", 24),
    Nloth => ("Nloth", 25),
    TheJoust => ("TheJoust", 26),
    TheLibrary => ("TheLibrary", 27),
    Vampires => ("Vampires", 28),

    // Act 3
    Falling => ("Falling", 29),
    MindBloom => ("MindBloom", 30),
    MoaiHead => ("MoaiHead", 31),
    MysteriousSphere => ("MysteriousSphere", 32),
    SensoryStone => ("SensoryStone", 33),
    TombRedMask => ("TombRedMask", 34),
    WindingHalls => ("WindingHalls", 35),

    // Any Act Base Shrines
    AccursedBlacksmith => ("AccursedBlacksmith", 36),
    BonfireElementals => ("BonfireElementals", 37),
    BonfireSpirits => ("BonfireSpirits", 38),
    Designer => ("Designer", 39),
    Duplicator => ("Duplicator", 40),
    FaceTrader => ("FaceTrader", 41),
    FountainOfCurseCleansing => ("FountainOfCurseCleansing", 42),
    GremlinWheelGame => ("GremlinWheelGame", 43),
    Lab => ("Lab", 44),
    NoteForYourself => ("NoteForYourself", 45),
    Purifier => ("Purifier", 46),
    SecretPortal => ("SecretPortal", 47),
    Transmorgrifier => ("Transmorgrifier", 48),
    UpgradeShrine => ("UpgradeShrine", 49),
    WeMeetAgain => ("WeMeetAgain", 50),
    WomanInBlue => ("WomanInBlue", 51),

    // Special
    Neow => ("Neow", 52),
    SpireHeart => ("SpireHeart", 53),
}
}

/// Serialized event identities retained only for loading older state.
///
/// New generation and live-name resolution must never produce these IDs.
pub const RETIRED_EVENT_IDS: &[EventId] = &[EventId::BonfireSpirits];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EventCardState {
    pub card_id: CardId,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EventState {
    pub id: EventId,
    pub current_screen: usize,
    pub internal_state: i32,
    pub completed: bool,
    /// When true, the engine will return to EventRoom after combat+rewards
    /// instead of transitioning to MapNavigation.
    pub combat_pending: bool,
    /// Extra data for complex events (e.g. GremlinMatchGame board).
    /// Format is event-specific, but must contain numeric state only. Gameplay
    /// identities belong in typed payload fields instead.
    pub extra_data: Vec<i32>,
    /// Typed card identities owned by the active event. Library offerings and
    /// Match and Keep card types live here instead of integer discriminants.
    pub card_payload: Vec<EventCardState>,
    /// Enemies already constructed by the event, before the player chooses to fight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_encounter: Option<crate::state::core::PreparedEncounter>,
}

impl EventState {
    pub fn new(id: EventId) -> Self {
        EventState {
            id,
            current_screen: 0,
            internal_state: 0,
            completed: false,
            combat_pending: false,
            extra_data: Vec::new(),
            card_payload: Vec::new(),
            prepared_encounter: None,
        }
    }
}

/// Public facts already shown by this button, separate from effects that have
/// yet to execute. In particular, settled rewards are not future gains.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventOptionPublicDetail {
    OfferedCard {
        card: CardId,
        upgrades: u8,
    },
    RequestedPotion {
        potion: PotionId,
    },
    MatchPosition {
        position: usize,
    },
    WheelReward {
        reward: WheelPublicReward,
    },
    JoustResult {
        owner_wins: bool,
        gold_received: i32,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WheelPublicReward {
    GoldReceived { amount: i32 },
    RandomRelic,
    FullHeal,
    Curse { card: CardId },
    RemoveCard,
    LoseHp { amount: i32 },
}

/// One visible event button and its public display facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventChoiceMeta {
    pub text: String,
    pub disabled: bool,
    pub disabled_reason: Option<String>,
    pub public_detail: Option<EventOptionPublicDetail>,
}

impl EventChoiceMeta {
    pub fn new(text: impl Into<String>) -> Self {
        EventChoiceMeta {
            text: text.into(),
            disabled: false,
            disabled_reason: None,
            public_detail: None,
        }
    }

    pub fn disabled(text: impl Into<String>, reason: impl Into<String>) -> Self {
        EventChoiceMeta {
            text: text.into(),
            disabled: true,
            disabled_reason: Some(reason.into()),
            public_detail: None,
        }
    }

    pub fn with_public_detail(mut self, detail: EventOptionPublicDetail) -> Self {
        self.public_detail = Some(detail);
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventOptionSemantics {
    pub action: EventActionKind,
    pub effects: Vec<EventEffect>,
    pub constraints: Vec<EventOptionConstraint>,
    pub transition: EventOptionTransition,
    pub repeatable: bool,
    pub terminal: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventOption {
    pub ui: EventChoiceMeta,
    pub semantics: EventOptionSemantics,
}

impl EventOption {
    pub fn unknown(ui: EventChoiceMeta) -> Self {
        EventOption {
            ui,
            semantics: EventOptionSemantics::default(),
        }
    }

    pub fn new(ui: EventChoiceMeta, semantics: EventOptionSemantics) -> Self {
        EventOption { ui, semantics }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum EventActionKind {
    #[default]
    Unknown,
    Leave,
    Continue,
    Accept,
    Decline,
    Fight,
    Trade,
    DeckOperation,
    Gain,
    Special,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventEffect {
    GainGold(i32),
    GainGoldRange {
        min: i32,
        max: i32,
    },
    LoseGold(i32),
    LoseHp(i32),
    LoseMaxHp(i32),
    Heal(i32),
    GainMaxHp(i32),
    ObtainRelic {
        count: usize,
        kind: EventRelicKind,
    },
    ObtainPotion {
        count: usize,
    },
    ObtainCard {
        count: usize,
        kind: EventCardKind,
    },
    ObtainColorlessCard {
        count: usize,
        kind: EventCardKind,
    },
    OfferCards {
        count: usize,
        kind: EventCardKind,
    },
    ObtainCurse {
        count: usize,
        kind: EventCardKind,
    },
    RemoveCard {
        count: usize,
        target_lineage: Option<crate::runtime::combat::CardLineageId>,
        kind: EventCardKind,
    },
    UpgradeCard {
        count: usize,
    },
    UpgradeAllCards,
    TransformCard {
        count: usize,
    },
    DuplicateCard {
        count: usize,
    },
    LoseRelic {
        specific: Option<RelicId>,
        starter_only: bool,
    },
    LoseStarterRelic {
        specific: Option<RelicId>,
    },
    RandomOutcome {
        kind: EventRandomOutcomeKind,
        /// Public probability for the named outcome when the event exposes a
        /// single success roll.  Multi-outcome events leave this unset.
        chance_percent: Option<u8>,
    },
    StartCombat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventRandomOutcomeKind {
    GremlinWheel,
    JoustOwnerWin,
    DeadAdventurerSearch,
    MatchAndKeepFlip,
    ScrapOozeRelicRoll,
    MausoleumCurseRoll,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventOptionTransition {
    #[default]
    None,
    AdvanceScreen,
    Complete,
    OpenSelection(EventSelectionKind),
    OpenReward,
    StartCombat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventOptionConstraint {
    RequiresGold(i32),
    RequiresRelic(RelicId),
    RequiresRemovableCard,
    RequiresNonBottledPurgeableCard,
    RequiresUpgradeableCard,
    RequiresTransformableCard,
    RequiresTransformableCards(usize),
    RequiresPotion,
    RequiresPotionSlotValue,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventSelectionKind {
    #[default]
    None,
    RemoveCard,
    UpgradeCard,
    TransformCard,
    DuplicateCard,
    OfferCard,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventCardKind {
    #[default]
    Unknown,
    Specific(CardId),
    RandomClassCard,
    RandomClassCommonOrUncommon,
    RandomClassRare,
    RandomColorless,
    RandomColorlessUncommon,
    RandomColorlessRare,
    RandomCurse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventRelicKind {
    #[default]
    Unknown,
    Specific(RelicId),
    RandomCommonRelic,
    RandomUncommonRelic,
    RandomRareRelic,
    RandomShopRelic,
    RandomBossRelic,
    RandomRelic,
    RandomBook,
    RandomFace,
}
