//! Versioned sparse semantic tensors for the standalone Python bridge.
//!
//! The simulator continues to own typed observations and actions. This module
//! is the only owner of their numeric learning vocabulary. Runtime identities
//! such as card UUIDs are used to resolve graph edges and are never emitted as
//! categorical or scalar features.

mod combat;
mod complete_turn;
mod mechanics;

use std::collections::BTreeMap;
use std::fmt;

use sts_oracle_eval::ai::planner_core::{
    public_map_preparation_starts_after_current, summarize_public_map_preparation,
    summarize_public_map_route, PlannerDecisionContext, PlannerDecisionSite, PlannerMapRoom,
    PlannerPlayerClass, PlannerPublicMap, PlannerQuestionMarkOutcome, PlannerRewardDescriptor,
    PlannerRunGoal, PlannerRunTransitionV1, PlannerShopPurchaseBlockReasonV1,
    PlannerShopPurchaseStatusV1, PublicMapPreparationSummary,
};
use sts_oracle_eval::content::cards::CardId;
use sts_oracle_eval::content::events::face_trader::FaceTraderResult;
use sts_oracle_eval::content::identity::StableGameplayId;
use sts_oracle_eval::content::monsters::{factory::EncounterId, EnemyId};
use sts_oracle_eval::content::potions::PotionId;
use sts_oracle_eval::content::powers::PowerId;
use sts_oracle_eval::content::relics::RelicId;
use sts_oracle_eval::sim::combat_action_surface::CombatSelectionDistinctByV2;
use sts_oracle_eval::state::events::{EventActionKind, EventId};
use sts_oracle_eval::state::map::node::RoomType;
use sts_oracle_eval::state::selection::{SelectionReason, SelectionScope};
use sts_oracle_learning::eval::run_control::{
    LearningModelCandidateSemanticsV1, LearningModelDecisionV1, LearningModelInputError,
    LearningModelObservationV1, LearningRunSelectionFamilyV1,
    LearningSelectionCandidateSemanticsV1, LearningSelectionDecisionV1, LearningSelectionDraftV1,
    LearningStrategicActionSemanticsV2, LearningStrategicContextKindV1,
    LearningStrategicModelObservationV1,
};

pub const SEMANTIC_SCHEMA_VERSION: u32 = 32;
pub const NO_CANDIDATE_TOKEN: u64 = u64::MAX;
pub const CARD_ID_VOCABULARY_SIZE: u64 = 371;
pub const RELIC_ID_VOCABULARY_SIZE: u64 = 182;
pub const POTION_ID_VOCABULARY_SIZE: u64 = 42;
pub const ENCOUNTER_ID_VOCABULARY_SIZE: u64 = 65;
pub const EVENT_ID_VOCABULARY_SIZE: u64 = 54;
pub const ENEMY_ID_VOCABULARY_SIZE: u64 = 65;
pub const POWER_ID_VOCABULARY_SIZE: u64 = 136;

const RUN_SELECTION_DECK_CARD_ORDINAL_INPUT_ENCODING: i64 = 3;

pub(super) use mechanics::{
    CARD_ATTACK_AMOUNT_SCHEMA, CARD_ATTACK_HITS_SCHEMA, CARD_ATTACK_PLAY_CONDITION_SCHEMA,
    CARD_ATTACK_TARGETS_SCHEMA, CARD_COMBAT_UPGRADE_SCOPE_SCHEMA, CARD_DAMAGE_DELIVERY_SCHEMA,
    CARD_ENERGY_GAIN_DELIVERY_SCHEMA, CARD_MECHANIC_COVERAGE_SCHEMA, CARD_MECHANIC_ROLE_SCHEMA,
    CARD_RARITY_SCHEMA, CARD_STATUS_CREATION_DESTINATION_SCHEMA,
    CARD_STATUS_CREATION_TIMING_SCHEMA, CARD_STRENGTH_CONVERSION_SCHEMA,
    CARD_STRENGTH_GAIN_DELIVERY_SCHEMA, CARD_TAG_SCHEMA, CARD_TARGET_SCHEMA, CARD_TYPE_SCHEMA,
    IDENTITY_RESIDUAL_CATEGORICAL_FIELDS, POTION_CLASS_SCHEMA, POTION_MECHANIC_ROLE_SCHEMA,
    POTION_RARITY_SCHEMA, POWER_AMOUNT_KIND_SCHEMA, POWER_EFFECT_SCHEMA,
    POWER_EFFECT_TARGET_SCHEMA, POWER_LIFETIME_SCHEMA, POWER_TRIGGER_SCHEMA,
};

// Domain identities use their explicit stable vocabulary ordinals. These
// compile-time sentinels catch vocabulary extension; changing a stable ordinal
// or vocabulary size requires an explicit schema review and bump.
const _: () = {
    assert!(CardId::Strike.stable_ordinal() == 0);
    assert!(<CardId as StableGameplayId>::STABLE_IDS.len() as u64 == CARD_ID_VOCABULARY_SIZE);
    assert!(RelicId::Abacus.stable_ordinal() == 0);
    assert!(<RelicId as StableGameplayId>::STABLE_IDS.len() as u64 == RELIC_ID_VOCABULARY_SIZE);
    assert!(PotionId::FirePotion.stable_ordinal() == 0);
    assert!(<PotionId as StableGameplayId>::STABLE_IDS.len() as u64 == POTION_ID_VOCABULARY_SIZE);
    assert!(EncounterId::BlueSlaver.stable_ordinal() == 0);
    assert!(
        <EncounterId as StableGameplayId>::STABLE_IDS.len() as u64 == ENCOUNTER_ID_VOCABULARY_SIZE
    );
    assert!(EventId::BigFish.stable_ordinal() == 0);
    assert!(<EventId as StableGameplayId>::STABLE_IDS.len() as u64 == EVENT_ID_VOCABULARY_SIZE);
    assert!(EnemyId::JawWorm.stable_ordinal() == 0);
    assert!(<EnemyId as StableGameplayId>::STABLE_IDS.len() as u64 == ENEMY_ID_VOCABULARY_SIZE);
    assert!(PowerId::Strength.stable_ordinal() == 0);
    assert!(<PowerId as StableGameplayId>::STABLE_IDS.len() as u64 == POWER_ID_VOCABULARY_SIZE);
    assert!(EventActionKind::Unknown as i64 == 0);
    assert!(EventActionKind::Special as i64 == 9);
    assert!(RoomType::EventRoom as i64 == 0);
    assert!(RoomType::TrueVictoryRoom as i64 == 7);
    assert!(SelectionScope::Hand as i64 == 0);
    assert!(SelectionScope::Grid as i64 == 2);
    assert!(PlannerRunGoal::ActThreeVictory as i64 == 0);
    assert!(PlannerRunGoal::HeartVictory as i64 == 1);
    assert!(PlannerDecisionSite::Map as i64 == 0);
    assert!(PlannerDecisionSite::Treasure as i64 == 9);
    assert!(PlannerPlayerClass::Ironclad as i64 == 0);
    assert!(PlannerPlayerClass::Watcher as i64 == 3);
};

macro_rules! numeric_schema_enum {
    ($(#[$meta:meta])* $visibility:vis enum $name:ident: $repr:ty {
        $($variant:ident = $value:expr),+ $(,)?
    }) => {
        $(#[$meta])*
        #[repr($repr)]
        $visibility enum $name {
            $($variant = $value),+
        }

        impl $name {
            pub const SCHEMA: &'static [(&'static str, i64)] = &[
                $((stringify!($variant), Self::$variant as i64)),+
            ];
        }
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SemanticCompleteness {
    NotEncoded = 0,
    Complete = 1,
}

impl SemanticCompleteness {
    pub const SCHEMA: &'static [(&'static str, i64)] = &[
        ("NotEncoded", Self::NotEncoded as i64),
        ("Complete", Self::Complete as i64),
    ];
}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum TokenKind: u16 {
    Observation = 1,
    Run = 2,
    Context = 3,
    History = 4,
    Card = 5,
    Relic = 6,
    PotionSlot = 7,
    Map = 8,
    MapNode = 9,
    Candidate = 10,
    OfferedCard = 11,
    CombatEncounter = 12,
    CombatTurn = 13,
    CombatTurnCounters = 14,
    CombatPlayer = 15,
    CombatOrb = 16,
    CombatPower = 17,
    CombatMonster = 18,
    CombatIntent = 19,
    CombatMoveHistory = 20,
    CombatPublicCounter = 21,
    CombatCardZone = 22,
    CombatCard = 23,
    CombatHiddenReason = 24,
    CombatCounterItem = 25,
    CombatMove = 26,
    CombatDamageProjection = 27,
    SelectionDomain = 28,
    SelectionState = 29,
    SelectionChosen = 30,
    CombatActionPayload = 31,
    CompleteTurnAction = 32,
    CompleteTurnTransition = 33,
    CompleteTurnMilestone = 34,
    ShopOffer = 35,
    PowerEffect = 36,
    UpgradedCard = 37,
    PendingReward = 38,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum CategoricalField: u16 {
    RunGoal = 1,
    DecisionSite = 2,
    PlayerClass = 3,
    RubyKey = 4,
    EmeraldKey = 5,
    SapphireKey = 6,
    CardId = 7,
    RelicId = 8,
    RelicUsedUp = 9,
    PotionOccupied = 10,
    PotionId = 11,
    PotionCanUse = 12,
    PotionCanDiscard = 13,
    PotionRequiresTarget = 14,
    BossEncounterId = 15,
    MapRoom = 16,
    MapNodeHasEmeraldKey = 17,
    ContextKind = 18,
    ContextOverlay = 19,
    ContextEventId = 20,
    ContextPurgeAvailable = 21,
    ActionKind = 22,
    ActionFlight = 23,
    ActionEventId = 24,
    ActionEventKind = 25,
    ActionCardId = 26,
    ActionRelicId = 27,
    ActionPotionId = 28,
    ActionSelectionScope = 29,
    ActionSite = 30,
    RewardKind = 31,
    CombatPhase = 32,
    CombatIsBoss = 33,
    CombatIsElite = 34,
    HiddenReason = 35,
    PlayerFacingLeft = 36,
    StanceId = 37,
    OrbId = 38,
    PowerId = 39,
    PowerJustApplied = 40,
    EnemyId = 42,
    MonsterAlive = 43,
    MonsterEscaped = 44,
    MonsterDying = 45,
    MonsterHalfDead = 46,
    EvidenceKind = 47,
    IntentKind = 48,
    PublicCounterKind = 49,
    CardZoneKind = 50,
    CardExhaustOverride = 51,
    CardRetainOverride = 52,
    CardFreeToPlay = 53,
    CombatActionKind = 54,
    IndexedChoiceInputEncoding = 55,
    IndexedChoiceReasonKind = 56,
    IndexedChoiceColorless = 57,
    IndexedChoiceCardType = 58,
    IndexedChoiceDestination = 59,
    IndexedChoiceUpgraded = 60,
    IndexedChoiceCandidateKind = 61,
    SelectionInputEncoding = 62,
    SelectionReasonKind = 63,
    SelectionSourcePile = 64,
    SelectionPayloadDistinctBy = 65,
    SelectionCandidateKind = 66,
    SelectionDomainKind = 67,
    SelectionDomainEligible = 68,
    CounterItemKind = 69,
    SelectionReasonFlag = 70,
    CardType = 71,
    CardRarity = 72,
    CardTarget = 73,
    CardIsMultiDamage = 74,
    CardExhaust = 75,
    CardEthereal = 76,
    CardInnate = 77,
    CardTag = 78,
    PotionRarity = 79,
    PotionClass = 80,
    PotionIsThrown = 81,
    PotionMechanicRole = 82,
    CardMechanicCoverage = 83,
    CardMechanicRole = 84,
    CombatEncounterId = 85,
    ActionEventRandomOutcome = 86,
    ActionEventStartsCombat = 87,
    ActionEventRepeatable = 88,
    ActionEventTerminal = 89,
    ActionEventUpgradesAllCards = 90,
    CompleteTurnBoundary = 91,
    CompleteTurnMilestone = 92,
    ShopPurchaseStatus = 93,
    ShopAffordable = 94,
    QuestionMarkOutcome = 95,
    ContextFaceTraderResult = 96,
    ContextRunTransitionStage = 97,
    PowerMechanicKnown = 98,
    PowerTrigger = 99,
    PowerEffect = 100,
    PowerEffectTarget = 101,
    PowerAmountKind = 102,
    PowerLifetime = 103,
    PowerTriggerCardType = 104,
    CardCombatUpgradeScope = 105,
    CardStrengthGainDelivery = 106,
    CardStrengthConversion = 107,
    CardAttackAmountKind = 108,
    CardAttackHitsKind = 109,
    CardAttackTargetsKind = 110,
    CardAttackPlayCondition = 111,
    CardDamageDeliveryKind = 112,
    CardCanUpgrade = 113,
    CardSelfHpLossBypassesBlock = 114,
    CardSelfHpLossBypassesIntangible = 115,
    CardSelfHpLossTriggersRupture = 116,
    CardEnergyGainDelivery = 117,
    CardStatusCreationCardId = 118,
    CardStatusCreationDestination = 119,
    CardStatusCreationTiming = 120,
    CardStatusCreationUpgraded = 121,
    CardStatusCreationEthereal = 122,
    RelicPreventPlayerDebuffBeforeArtifactPowerId = 123,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum ScalarField: u16 {
    AscensionLevel = 1,
    Act = 2,
    Floor = 3,
    CurrentHp = 4,
    MaxHp = 5,
    Gold = 6,
    PotionCapacity = 7,
    CardUpgrades = 8,
    CardMiscValue = 9,
    CardBaseDamageOverride = 10,
    CardBaseBlockOverride = 11,
    CardCostModifier = 12,
    RelicCounter = 13,
    RelicAmount = 14,
    PotionSlot = 15,
    MapCurrentX = 16,
    MapCurrentY = 17,
    MapNodeX = 18,
    MapNodeY = 19,
    ContextRewardItemIndex = 20,
    ContextEventScreen = 21,
    ContextPurgeCost = 22,
    ShopPurgeCount = 23,
    ActionX = 24,
    ActionY = 25,
    ActionScreen = 26,
    ActionOptionIndex = 27,
    ActionRewardItemIndex = 28,
    ActionUpgrades = 29,
    ActionShopSlot = 30,
    ActionPrice = 31,
    RewardAmount = 32,
    CombatTurnCount = 33,
    CombatEnergy = 34,
    TurnStartDrawModifier = 35,
    CardsPlayedThisTurn = 36,
    AttacksPlayedThisTurn = 37,
    CardsDiscardedThisTurn = 38,
    MantraGainedThisCombat = 39,
    TimesDamagedThisCombat = 40,
    DiscoveryCostForTurn = 41,
    CombatPlayerHp = 42,
    CombatPlayerMaxHp = 43,
    CombatPlayerBlock = 44,
    CombatPlayerGold = 45,
    GoldDeltaThisCombat = 46,
    EnergyMaster = 47,
    MaxOrbs = 48,
    CollectionPosition = 49,
    OrbBasePassive = 50,
    OrbBaseEvoke = 51,
    OrbPassive = 52,
    OrbEvoke = 53,
    PowerAmount = 54,
    PowerExtraData = 55,
    MonsterSlot = 56,
    MonsterHp = 57,
    MonsterMaxHp = 58,
    MonsterBlock = 59,
    IntentPreviewDamagePerHit = 61,
    IntentDamage = 62,
    IntentHits = 63,
    MoveId = 64,
    PublicCounterValue = 65,
    CardCostForTurn = 66,
    CardEffectiveCost = 67,
    CardCurrentDamage = 68,
    CardCurrentBlock = 69,
    CardCurrentMagicNumber = 70,
    CardEnergyOnUse = 71,
    DamageProjectionValue = 72,
    ActionIndex = 73,
    ActionSecondaryIndex = 74,
    PayloadPosition = 75,
    IndexedChoiceAmount = 76,
    SelectionRawDomainCount = 77,
    SelectionEligibleDomainCount = 78,
    SelectionMaxDistinctCount = 79,
    SelectionDeclaredMin = 80,
    SelectionDeclaredMax = 81,
    SelectionEffectiveMax = 82,
    SelectionDomainAddress = 83,
    SelectionChosenPosition = 84,
    SelectionReasonAmount = 85,
    CardBaseCost = 86,
    CardDefinitionDamage = 87,
    CardDefinitionBlock = 88,
    CardDefinitionMagic = 89,
    CardUpgradeDamage = 90,
    CardUpgradeBlock = 91,
    CardUpgradeMagic = 92,
    PotionBasePotency = 93,
    RecentCombatHpLoss = 94,
    RecentCombatTurns = 95,
    RecentCombatPotionsUsed = 96,
    ActionEventGoldDeltaMin = 97,
    ActionEventGoldDeltaMax = 98,
    ActionEventHpDelta = 99,
    ActionEventMaxHpDelta = 100,
    ActionEventRelicsGained = 101,
    ActionEventRelicsLost = 102,
    ActionEventPotionsGained = 103,
    ActionEventCardsGained = 104,
    ActionEventCardsOffered = 105,
    ActionEventCursesGained = 106,
    ActionEventCardsRemoved = 107,
    ActionEventCardsUpgraded = 108,
    ActionEventCardsTransformed = 109,
    ActionEventCardsDuplicated = 110,
    CompleteTurnActionPosition = 111,
    CompleteTurnPotionSpendCount = 112,
    IntentHealAmount = 113,
    RouteMinElites = 114,
    RouteMaxElites = 115,
    RouteMinRests = 116,
    RouteMaxRests = 117,
    RouteMinShops = 118,
    RouteMaxShops = 119,
    RouteFirstEliteSteps = 120,
    RouteFirstRestSteps = 121,
    RouteFirstShopSteps = 122,
    RouteMinCombatRooms = 123,
    RouteMaxCombatRooms = 124,
    RouteMinUnknownRooms = 125,
    RouteMaxUnknownRooms = 126,
    RouteFirstShopBeforeEliteSteps = 127,
    ActionRestHpGain = 128,
    CardPersistentEnergyCost = 129,
    CardPersistentDamageParameter = 130,
    CardPersistentBlockParameter = 131,
    CardPersistentMagicParameter = 132,
    CardUnconditionalBaseDamage = 133,
    CardUnconditionalBaseBlock = 134,
    DeckSize = 135,
    DeckAttackCount = 136,
    DeckSkillCount = 137,
    DeckPowerCount = 138,
    DeckStatusCount = 139,
    DeckCurseCount = 140,
    DeckCostZeroCount = 141,
    DeckCostOneCount = 142,
    DeckCostTwoCount = 143,
    DeckCostThreeOrMoreCount = 144,
    DeckCostXCount = 145,
    DeckCostUnplayableCount = 146,
    CardAttackFixedHits = 147,
    CardStrengthGainParameter = 148,
    CardAttackStrengthMultiplier = 149,
    CardAttackAmountPerUnit = 150,
    PendingRewardItemIndex = 151,
    PendingActEntry = 152,
    DeckUpgradeableCount = 153,
    DeckUpgradeableBasicCount = 154,
    CardSelfHpLossOnPlay = 155,
    CardEnergyGainRequest = 156,
    CardDrawOnPlayRequest = 157,
    CardHandTopdeckAfterDrawRequest = 158,
    CardStatusCreationRequest = 159,
    DeckStrikeTagCount = 160,
    CardDeployedExhaustDrawPerEventRequest = 161,
    CardDeployedExhaustBlockPerEventRequest = 162,
    DeckStrengthOnPlayCardCount = 163,
    DeckStrengthConditionalCardCount = 164,
    DeckTemporaryStrengthCardCount = 165,
    DeckStrengthPerTurnCardCount = 166,
    DeckExhaustDrawPowerCardCount = 167,
    DeckExhaustBlockPowerCardCount = 168,
    RelicCombatPotionDropChanceOverridePercent = 169,
    RelicCombatPotionDropBlockedAtRewardEntryCount = 170,
    RelicVictoryHealingRequest = 171,
    PreparationEliteWithoutRestOrShopSteps = 172,
    PreparationEliteWithRestOnlySteps = 173,
    PreparationEliteWithShopOnlySteps = 174,
    PreparationEliteWithRestAndShopSteps = 175,
    PreparationFirstKnownEliteFreeEndSteps = 176,
    PreparationFirstUnresolvedEndSteps = 177,
    MapPreparationStartCount = 178,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum RelationKind: u16 {
    ObservationHasRun = 1,
    ObservationHasContext = 2,
    ObservationHasHistory = 3,
    ObservationHasCard = 4,
    ObservationHasRelic = 5,
    ObservationHasPotionSlot = 6,
    ObservationHasMap = 7,
    ObservationHasCandidate = 8,
    MapHasNode = 9,
    MapPathTo = 10,
    CandidateTargets = 11,
    CandidateHasPayload = 12,
    ObservationHasEncounter = 13,
    ObservationHasTurn = 14,
    ObservationHasPlayer = 15,
    ObservationHasHiddenReason = 16,
    ObservationHasCardZone = 17,
    ObservationHasMonster = 18,
    PlayerHasRelic = 19,
    PlayerHasOrb = 20,
    EntityHasPower = 21,
    PowerHasPayloadCard = 22,
    MonsterHasIntent = 23,
    MonsterHasMoveHistory = 24,
    MonsterHasPublicCounter = 25,
    ZoneHasCard = 26,
    TurnHasCounters = 27,
    CountersHasItem = 28,
    HistoryHasMove = 29,
    CardHasDamageProjection = 30,
    DamageTargetsMonster = 31,
    CandidateHasSelectionDomain = 32,
    ObservationHasSelectionState = 33,
    SelectionHasChosen = 34,
    ChosenTargetsDomain = 35,
    SelectionHasDomain = 36,
    SelectionDomainTargets = 37,
    CandidateHasSuccessor = 38,
    CandidateHasAction = 39,
    CandidateHasTransition = 40,
    TransitionHasMilestone = 41,
    CandidateHasAvailableAction = 42,
    ContextHasShopOffer = 43,
    PowerAppliesPower = 44,
    CardTriggersPower = 45,
    CandidateRemovesCard = 46,
    CandidateUpgradesTo = 47,
    ContextHasPendingReward = 48,
    RewardHasOfferedCard = 49,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum MapRoomKind: i64 {
    QuestionMark = 0, Monster = 1, Elite = 2, Boss = 3,
    Rest = 4, Shop = 5, Treasure = 6, Victory = 7,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum QuestionMarkOutcomeKind: i64 {
    Event = 0, Monster = 1, Shop = 2, Treasure = 3, Elite = 4,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum ShopPurchaseStatusKind: i64 {
    Purchasable = 0, MerchantUnavailable = 1, NotEnoughGold = 2,
    BlockedBySozu = 3, NoEmptyPotionSlot = 4, PurgeAlreadyUsed = 5,
    NoEligiblePurgeTarget = 6,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum ContextKind: i64 {
    Map = LearningStrategicContextKindV1::Map as i64,
    CardReward = LearningStrategicContextKindV1::CardReward as i64,
    Event = LearningStrategicContextKindV1::Event as i64,
    Shop = LearningStrategicContextKindV1::Shop as i64,
    Reward = LearningStrategicContextKindV1::Reward as i64,
    Campfire = LearningStrategicContextKindV1::Campfire as i64,
    BossRelic = LearningStrategicContextKindV1::BossRelic as i64,
    RunChoice = LearningStrategicContextKindV1::RunChoice as i64,
    Treasure = LearningStrategicContextKindV1::Treasure as i64,
    RunTransition = LearningStrategicContextKindV1::RunTransition as i64,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum ActionKind: i64 {
    ChooseRouteNode = 1,
    ChooseEventOption = 2,
    TakeCard = 3,
    OpenCardReward = 4,
    SingingBowl = 5,
    SkipCardReward = 6,
    ClaimReward = 7,
    Rest = 8,
    Smith = 9,
    Dig = 10,
    Lift = 11,
    Toke = 12,
    Recall = 13,
    BuyCard = 14,
    BuyRelic = 15,
    BuyPotion = 16,
    RemoveCard = 17,
    OpenPendingRewards = 18,
    LeaveShop = 19,
    TakeBossRelic = 20,
    SkipBossRelic = 21,
    SubmitRunSelection = 22,
    OpenChest = 23,
    Proceed = 24,
    Cancel = 25,
    UseRunPotion = 26,
    DiscardRunPotion = 27,
    BeginRunCardSelection = 28,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum RewardKind: i64 {
    Gold = 1,
    StolenGold = 2,
    CardReward = 3,
    Relic = 4,
    Potion = 5,
    EmeraldKey = 6,
    SapphireKey = 7,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum CombatActionKind: i64 {
    PlayCard = 1,
    UsePotion = 2,
    DiscardPotion = 3,
    EndTurn = 4,
    SubmitIndexedChoice = 5,
    Proceed = 6,
    Cancel = 7,
    BeginSelection = 8,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum IntentKind: i64 {
    Attack = 1,
    AttackBuff = 2,
    AttackDebuff = 3,
    AttackDefend = 4,
    Buff = 5,
    Debuff = 6,
    StrongDebuff = 7,
    Debug = 8,
    Defend = 9,
    DefendDebuff = 10,
    DefendBuff = 11,
    Escape = 12,
    Magic = 13,
    None = 14,
    Sleep = 15,
    Stun = 16,
    Unknown = 17,
    Heal = 18,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum PublicCounterKind: i64 {
    HexaghostActiveOrbs = 1,
    StolenGold = 2,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum CardZoneKind: i64 {
    MasterDeck = 1,
    Hand = 2,
    Draw = 3,
    Discard = 4,
    Exhaust = 5,
    Limbo = 6,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum IndexedChoiceReasonKind: i64 {
    Discovery = 1,
    CardReward = 2,
    ForeignInfluence = 3,
    ChooseOne = 4,
    Stance = 5,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum IndexedChoiceCandidateKind: i64 {
    Card = 1,
    Stance = 2,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum SelectionReasonKind: i64 {
    HandExhaust = 1,
    HandDiscard = 2,
    HandRetain = 3,
    HandPutOnDrawPile = 4,
    HandPutToBottomOfDraw = 5,
    HandSetup = 6,
    HandCopy = 7,
    HandNightmare = 8,
    HandUpgrade = 9,
    HandGamblingChip = 10,
    HandRecycle = 11,
    GridMoveToDrawPile = 12,
    GridExhume = 13,
    GridDrawPileToHand = 14,
    GridSkillFromDeckToHand = 15,
    GridAttackFromDeckToHand = 16,
    GridDiscardToHand = 17,
    GridDiscardToHandNoCostChange = 18,
    GridDiscardToHandRetain = 19,
    GridOmniscience = 20,
    ScryDiscard = 21,
    RunUpgrade = 22,
    RunPurge = 23,
    RunTransform = 24,
    RunTransformUpgraded = 25,
    RunDuplicate = 26,
    RunBottleFlame = 27,
    RunBottleLightning = 28,
    RunBottleTornado = 29,
    HandPotionExhaust = 30,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum SelectionCandidateKind: i64 {
    Submit = 1,
    Append = 2,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum SelectionDomainKind: i64 {
    Card = 1,
    Scry = 2,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum CounterItemKind: i64 {
    CardPlayedThisTurn = 1,
    CardPlayedThisCombat = 2,
    OrbChanneledThisTurn = 3,
    OrbChanneledThisCombat = 4,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum CompleteTurnBoundaryKind: i64 {
    NextPlayerTurn = 1,
    TerminalWin = 2,
    TerminalLoss = 3,
    Other = 4,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum CompleteTurnMilestoneKind: i64 {
    EscalatingAddsRemoved = 1,
    AttackMitigationTargetExposed = 2,
    WithheldSurvivalResourceRecovered = 3,
    ForcedAttackWindowReached = 4,
    ForcedAttackWindowSurvived = 5,
    ForcedAttackRecoveryWindowClosed = 6,
    UntaxedTransitionWindowReached = 7,
    TransitionWindowClosed = 8,
    SecondPhaseOpeningSurvived = 9,
    ThresholdCommitted = 10,
    DebuffCleanseCompleted = 11,
    ExecuteWindowSurvived = 12,
    TeamGrowthSourceEliminated = 13,
    EncounterDefeated = 14,
}}

numeric_schema_enum! {
#[derive(Clone, Copy, Debug)]
pub enum CompleteTurnCandidateEvidenceKind: i64 {
    Unknown = 0,
    VerifiedPositiveDemonstration = 1,
}}

pub const CATEGORICAL_VOCABULARY_SIZES: &[(u16, u64)] = &[
    (CategoricalField::CardCombatUpgradeScope as u16, 3),
    (CategoricalField::RunGoal as u16, 2),
    (CategoricalField::DecisionSite as u16, 11),
    (CategoricalField::PlayerClass as u16, 4),
    (CategoricalField::RubyKey as u16, 2),
    (CategoricalField::EmeraldKey as u16, 2),
    (CategoricalField::SapphireKey as u16, 2),
    (CategoricalField::CardId as u16, CARD_ID_VOCABULARY_SIZE),
    (CategoricalField::RelicId as u16, RELIC_ID_VOCABULARY_SIZE),
    (CategoricalField::RelicUsedUp as u16, 2),
    (CategoricalField::PotionOccupied as u16, 2),
    (CategoricalField::PotionId as u16, POTION_ID_VOCABULARY_SIZE),
    (CategoricalField::PotionCanUse as u16, 2),
    (CategoricalField::PotionCanDiscard as u16, 2),
    (CategoricalField::PotionRequiresTarget as u16, 2),
    (
        CategoricalField::BossEncounterId as u16,
        ENCOUNTER_ID_VOCABULARY_SIZE,
    ),
    (
        CategoricalField::MapRoom as u16,
        MapRoomKind::SCHEMA.len() as u64,
    ),
    (CategoricalField::MapNodeHasEmeraldKey as u16, 2),
    (CategoricalField::ContextKind as u16, 11),
    (CategoricalField::ContextOverlay as u16, 2),
    (
        CategoricalField::ContextEventId as u16,
        EVENT_ID_VOCABULARY_SIZE,
    ),
    (CategoricalField::ContextPurgeAvailable as u16, 2),
    (CategoricalField::ActionKind as u16, 29),
    (CategoricalField::ActionFlight as u16, 2),
    (
        CategoricalField::ActionEventId as u16,
        EVENT_ID_VOCABULARY_SIZE,
    ),
    (CategoricalField::ActionEventKind as u16, 10),
    (
        CategoricalField::ActionCardId as u16,
        CARD_ID_VOCABULARY_SIZE,
    ),
    (
        CategoricalField::ActionRelicId as u16,
        RELIC_ID_VOCABULARY_SIZE,
    ),
    (
        CategoricalField::ActionPotionId as u16,
        POTION_ID_VOCABULARY_SIZE,
    ),
    (CategoricalField::ActionSelectionScope as u16, 3),
    (CategoricalField::ActionSite as u16, 11),
    (CategoricalField::RewardKind as u16, 8),
    (CategoricalField::CombatPhase as u16, 3),
    (CategoricalField::CombatIsBoss as u16, 2),
    (CategoricalField::CombatIsElite as u16, 2),
    (CategoricalField::HiddenReason as u16, 3),
    (CategoricalField::PlayerFacingLeft as u16, 2),
    (CategoricalField::StanceId as u16, 4),
    (CategoricalField::OrbId as u16, 5),
    (CategoricalField::PowerId as u16, POWER_ID_VOCABULARY_SIZE),
    (CategoricalField::PowerJustApplied as u16, 2),
    (CategoricalField::PowerMechanicKnown as u16, 2),
    (
        CategoricalField::PowerTrigger as u16,
        POWER_TRIGGER_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::PowerEffect as u16,
        POWER_EFFECT_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::PowerEffectTarget as u16,
        POWER_EFFECT_TARGET_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::PowerAmountKind as u16,
        POWER_AMOUNT_KIND_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::PowerLifetime as u16,
        POWER_LIFETIME_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::PowerTriggerCardType as u16,
        CARD_TYPE_SCHEMA.len() as u64,
    ),
    (CategoricalField::EnemyId as u16, ENEMY_ID_VOCABULARY_SIZE),
    (CategoricalField::MonsterAlive as u16, 2),
    (CategoricalField::MonsterEscaped as u16, 2),
    (CategoricalField::MonsterDying as u16, 2),
    (CategoricalField::MonsterHalfDead as u16, 2),
    (CategoricalField::EvidenceKind as u16, 4),
    (CategoricalField::IntentKind as u16, 19),
    (CategoricalField::PublicCounterKind as u16, 3),
    (CategoricalField::CardZoneKind as u16, 7),
    (CategoricalField::CardExhaustOverride as u16, 2),
    (CategoricalField::CardRetainOverride as u16, 2),
    (CategoricalField::CardFreeToPlay as u16, 2),
    (CategoricalField::CombatActionKind as u16, 9),
    (CategoricalField::IndexedChoiceInputEncoding as u16, 1),
    (CategoricalField::IndexedChoiceReasonKind as u16, 6),
    (CategoricalField::IndexedChoiceColorless as u16, 2),
    (CategoricalField::IndexedChoiceCardType as u16, 5),
    (CategoricalField::IndexedChoiceDestination as u16, 2),
    (CategoricalField::IndexedChoiceUpgraded as u16, 2),
    (CategoricalField::IndexedChoiceCandidateKind as u16, 3),
    (CategoricalField::SelectionInputEncoding as u16, 4),
    (CategoricalField::SelectionReasonKind as u16, 31),
    (
        CategoricalField::ShopPurchaseStatus as u16,
        ShopPurchaseStatusKind::SCHEMA.len() as u64,
    ),
    (CategoricalField::ShopAffordable as u16, 2),
    (
        CategoricalField::QuestionMarkOutcome as u16,
        QuestionMarkOutcomeKind::SCHEMA.len() as u64,
    ),
    (CategoricalField::SelectionSourcePile as u16, 6),
    (CategoricalField::SelectionPayloadDistinctBy as u16, 2),
    (CategoricalField::SelectionCandidateKind as u16, 3),
    (CategoricalField::SelectionDomainKind as u16, 3),
    (CategoricalField::SelectionDomainEligible as u16, 2),
    (CategoricalField::CounterItemKind as u16, 5),
    (CategoricalField::SelectionReasonFlag as u16, 2),
    (
        CategoricalField::CardType as u16,
        CARD_TYPE_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardRarity as u16,
        CARD_RARITY_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardTarget as u16,
        CARD_TARGET_SCHEMA.len() as u64,
    ),
    (CategoricalField::CardIsMultiDamage as u16, 2),
    (CategoricalField::CardExhaust as u16, 2),
    (CategoricalField::CardEthereal as u16, 2),
    (CategoricalField::CardInnate as u16, 2),
    (
        CategoricalField::CardTag as u16,
        CARD_TAG_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::PotionRarity as u16,
        POTION_RARITY_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::PotionClass as u16,
        POTION_CLASS_SCHEMA.len() as u64,
    ),
    (CategoricalField::PotionIsThrown as u16, 2),
    (
        CategoricalField::PotionMechanicRole as u16,
        POTION_MECHANIC_ROLE_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardMechanicCoverage as u16,
        CARD_MECHANIC_COVERAGE_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardMechanicRole as u16,
        CARD_MECHANIC_ROLE_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CombatEncounterId as u16,
        ENCOUNTER_ID_VOCABULARY_SIZE,
    ),
    (CategoricalField::ActionEventRandomOutcome as u16, 2),
    (CategoricalField::ActionEventStartsCombat as u16, 2),
    (CategoricalField::ActionEventRepeatable as u16, 2),
    (CategoricalField::ActionEventTerminal as u16, 2),
    (CategoricalField::ActionEventUpgradesAllCards as u16, 2),
    (CategoricalField::CompleteTurnBoundary as u16, 5),
    (CategoricalField::CompleteTurnMilestone as u16, 15),
    (CategoricalField::ContextFaceTraderResult as u16, 3),
    (CategoricalField::ContextRunTransitionStage as u16, 3),
    (
        CategoricalField::CardStrengthGainDelivery as u16,
        CARD_STRENGTH_GAIN_DELIVERY_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardStrengthConversion as u16,
        CARD_STRENGTH_CONVERSION_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardAttackAmountKind as u16,
        CARD_ATTACK_AMOUNT_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardAttackHitsKind as u16,
        CARD_ATTACK_HITS_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardAttackTargetsKind as u16,
        CARD_ATTACK_TARGETS_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardAttackPlayCondition as u16,
        CARD_ATTACK_PLAY_CONDITION_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardDamageDeliveryKind as u16,
        CARD_DAMAGE_DELIVERY_SCHEMA.len() as u64,
    ),
    (CategoricalField::CardCanUpgrade as u16, 2),
    (CategoricalField::CardSelfHpLossBypassesBlock as u16, 2),
    (CategoricalField::CardSelfHpLossBypassesIntangible as u16, 2),
    (CategoricalField::CardSelfHpLossTriggersRupture as u16, 2),
    (
        CategoricalField::CardEnergyGainDelivery as u16,
        CARD_ENERGY_GAIN_DELIVERY_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardStatusCreationCardId as u16,
        CARD_ID_VOCABULARY_SIZE,
    ),
    (
        CategoricalField::CardStatusCreationDestination as u16,
        CARD_STATUS_CREATION_DESTINATION_SCHEMA.len() as u64,
    ),
    (
        CategoricalField::CardStatusCreationTiming as u16,
        CARD_STATUS_CREATION_TIMING_SCHEMA.len() as u64,
    ),
    (CategoricalField::CardStatusCreationUpgraded as u16, 2),
    (CategoricalField::CardStatusCreationEthereal as u16, 2),
    (
        CategoricalField::RelicPreventPlayerDebuffBeforeArtifactPowerId as u16,
        POWER_ID_VOCABULARY_SIZE,
    ),
];

#[derive(Clone, Debug, Default)]
pub struct CategoricalTable {
    pub token_indices: Vec<u64>,
    pub fields: Vec<u16>,
    pub values: Vec<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct ScalarTable {
    pub token_indices: Vec<u64>,
    pub fields: Vec<u16>,
    pub values: Vec<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct RelationTable {
    pub source_token_indices: Vec<u64>,
    pub relations: Vec<u16>,
    pub target_token_indices: Vec<u64>,
}

#[derive(Clone, Debug)]
pub struct SemanticBatch {
    pub completeness: Vec<u8>,
    pub token_row_splits: Vec<u64>,
    pub token_kinds: Vec<u16>,
    /// Zero is the shared current observation; positive IDs identify one
    /// candidate and its payload within this row. Scope IDs are not features.
    pub token_scopes: Vec<u32>,
    pub categorical: CategoricalTable,
    pub scalar: ScalarTable,
    pub relation: RelationTable,
    pub candidate_token_indices: Vec<u64>,
}

struct StrategicTokenIndex {
    root: u64,
    cards: Vec<u64>,
    potions: BTreeMap<usize, (u64, PotionId)>,
    map_nodes: BTreeMap<(i32, i32), u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticEncodingError {
    CandidateScopeMismatch,
    MissingCardTarget(usize),
    InvalidSmithTarget(usize),
    InvalidEventCardTarget(LearningModelInputError),
    MissingPotionRelationTarget(usize),
    PotionTargetMismatch { slot: usize, potion: PotionId },
    DuplicateMapNode { x: i32, y: i32 },
    MissingMapTarget { x: i32, y: i32 },
    InvalidPublicMapRoute(String),
    NonStrategicCandidateInStrategicRow,
    CandidateAlignmentMismatch { expected: usize, actual: usize },
    MissingCombatHandCard(usize),
    MissingCombatPotionSlot(usize),
    MissingCombatMonsterTarget(usize),
    MissingDamageProjectionMonster(usize),
    UnsupportedCombatAtomicInput,
    UnsupportedRunSelectionReason(SelectionReason),
    MissingSelectionDomain(usize),
    SelectionObservationMismatch,
    CompleteTurnObservationContractMismatch,
    UnsupportedCompleteTurnSelectedAction,
    IndexOverflow,
}

impl fmt::Display for SemanticEncodingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SemanticEncodingError {}

#[derive(Default)]
pub struct SemanticBatchBuilder {
    completeness: Vec<u8>,
    token_row_splits: Vec<u64>,
    token_kinds: Vec<u16>,
    token_scopes: Vec<u32>,
    active_candidate_scope: Option<u32>,
    row_candidate_scope: u32,
    categorical: CategoricalTable,
    scalar: ScalarTable,
    relation: RelationTable,
    candidate_token_indices: Vec<u64>,
}

impl SemanticBatchBuilder {
    pub fn new() -> Self {
        Self {
            token_row_splits: vec![0],
            ..Self::default()
        }
    }

    pub fn push_decision(
        &mut self,
        decision: &LearningModelDecisionV1<'_>,
    ) -> Result<(), SemanticEncodingError> {
        match decision.observation {
            LearningModelObservationV1::Strategic(observation) => {
                self.completeness.push(SemanticCompleteness::Complete as u8);
                let before = self.candidate_token_indices.len();
                self.encode_strategic(observation, decision)?;
                let actual = self.candidate_token_indices.len() - before;
                if actual != decision.candidates.len() {
                    return Err(SemanticEncodingError::CandidateAlignmentMismatch {
                        expected: decision.candidates.len(),
                        actual,
                    });
                }
            }
            LearningModelObservationV1::Combat(observation) => {
                self.completeness.push(SemanticCompleteness::Complete as u8);
                let before = self.candidate_token_indices.len();
                self.encode_combat_root(observation, decision)?;
                let actual = self.candidate_token_indices.len() - before;
                if actual != decision.candidates.len() {
                    return Err(SemanticEncodingError::CandidateAlignmentMismatch {
                        expected: decision.candidates.len(),
                        actual,
                    });
                }
            }
        }
        self.finish_row()
    }

    pub fn push_selection(
        &mut self,
        observation: LearningModelObservationV1<'_>,
        draft: &LearningSelectionDraftV1,
    ) -> Result<(), SemanticEncodingError> {
        self.completeness.push(SemanticCompleteness::Complete as u8);
        let decision = draft.decision();
        let before = self.candidate_token_indices.len();
        match (observation, draft.combat_family(), draft.run_family()) {
            (LearningModelObservationV1::Combat(observation), Some(_), None) => {
                self.encode_combat_selection(observation, draft, &decision)?;
            }
            (LearningModelObservationV1::Strategic(observation), None, Some(family)) => {
                self.encode_strategic_selection(observation, draft, family, &decision)?;
            }
            _ => return Err(SemanticEncodingError::SelectionObservationMismatch),
        }
        let actual = self.candidate_token_indices.len() - before;
        if actual != decision.candidates.len() {
            return Err(SemanticEncodingError::CandidateAlignmentMismatch {
                expected: decision.candidates.len(),
                actual,
            });
        }
        self.finish_row()
    }

    pub fn finish(self) -> SemanticBatch {
        SemanticBatch {
            completeness: self.completeness,
            token_row_splits: self.token_row_splits,
            token_kinds: self.token_kinds,
            token_scopes: self.token_scopes,
            categorical: self.categorical,
            scalar: self.scalar,
            relation: self.relation,
            candidate_token_indices: self.candidate_token_indices,
        }
    }

    fn finish_row(&mut self) -> Result<(), SemanticEncodingError> {
        if self.active_candidate_scope.is_some() {
            return Err(SemanticEncodingError::CandidateScopeMismatch);
        }
        self.token_row_splits.push(self.token_count()?);
        self.row_candidate_scope = 0;
        Ok(())
    }

    fn encode_strategic(
        &mut self,
        observation: LearningStrategicModelObservationV1<'_>,
        decision: &LearningModelDecisionV1<'_>,
    ) -> Result<(), SemanticEncodingError> {
        let tokens = self.encode_strategic_observation(observation)?;
        for candidate in &decision.candidates {
            let LearningModelCandidateSemanticsV1::Strategic { action } = candidate.semantics
            else {
                return Err(SemanticEncodingError::NonStrategicCandidateInStrategicRow);
            };
            let token = self.begin_candidate()?;
            self.edge(tokens.root, RelationKind::ObservationHasCandidate, token);
            self.encode_action(
                token,
                &action
                    .semantics()
                    .map_err(|_| SemanticEncodingError::NonStrategicCandidateInStrategicRow)?,
                &tokens.cards,
                &tokens.potions,
                &tokens.map_nodes,
                observation,
            )?;
            self.finish_candidate(token)?;
        }
        Ok(())
    }

    fn encode_strategic_observation(
        &mut self,
        observation: LearningStrategicModelObservationV1<'_>,
    ) -> Result<StrategicTokenIndex, SemanticEncodingError> {
        let root = self.add_token(TokenKind::Observation)?;
        self.category(root, CategoricalField::RunGoal, observation.run_goal as i64);
        self.category(
            root,
            CategoricalField::DecisionSite,
            observation.decision_site as i64,
        );

        let run = self.add_token(TokenKind::Run)?;
        self.edge(root, RelationKind::ObservationHasRun, run);
        self.category(
            run,
            CategoricalField::PlayerClass,
            observation.run.player_class as i64,
        );
        self.category(
            run,
            CategoricalField::RubyKey,
            bool_value(observation.run.keys[0]),
        );
        self.category(
            run,
            CategoricalField::EmeraldKey,
            bool_value(observation.run.keys[1]),
        );
        self.category(
            run,
            CategoricalField::SapphireKey,
            bool_value(observation.run.keys[2]),
        );
        self.scalar(
            run,
            ScalarField::AscensionLevel,
            observation.run.ascension_level,
        );
        self.scalar(run, ScalarField::Act, observation.run.act);
        self.scalar(run, ScalarField::Floor, observation.run.floor);
        self.scalar(run, ScalarField::CurrentHp, observation.run.current_hp);
        self.scalar(run, ScalarField::MaxHp, observation.run.max_hp);
        self.scalar(run, ScalarField::Gold, observation.run.gold);
        if let Some(act) = observation.run.pending_act_entry {
            self.scalar(run, ScalarField::PendingActEntry, act);
        }
        self.scalar(
            run,
            ScalarField::PotionCapacity,
            observation.run.potion_capacity,
        );

        let cards = observation.cards().collect::<Vec<_>>();
        let mut card_tokens = vec![0; cards.len()];
        let mut type_counts = [0_usize; 5];
        let mut cost_counts = [0_usize; 6];
        let mut upgradeable_count = 0_usize;
        let mut upgradeable_basic_count = 0_usize;
        let mut strike_tag_count = 0_usize;
        let mut mechanic_counts = mechanics::OwnedDeckMechanicCounts::default();
        self.scalar(run, ScalarField::DeckSize, cards.len());
        for card in cards {
            mechanic_counts.include(card.card());
            let token = self.add_token(TokenKind::Card)?;
            card_tokens[card.ordinal()] = token;
            self.edge(root, RelationKind::ObservationHasCard, token);
            self.card_identity_with_mechanics(
                token,
                CategoricalField::CardId,
                card.card(),
                Some(card.upgrades()),
            );
            self.scalar(token, ScalarField::CardUpgrades, card.upgrades());
            self.scalar(token, ScalarField::CardMiscValue, card.misc_value());
            if let Some(value) = card.base_damage_override() {
                self.scalar(token, ScalarField::CardBaseDamageOverride, value);
            }
            if let Some(value) = card.base_block_override() {
                self.scalar(token, ScalarField::CardBaseBlockOverride, value);
            }
            self.scalar(token, ScalarField::CardCostModifier, card.cost_modifier());
            let output = self.owned_card_output(token, &mechanics::observed_owned_card(card));
            let definition = sts_oracle_eval::content::cards::get_card_definition(card.card());
            type_counts[definition.card_type as usize] += 1;
            strike_tag_count += usize::from(sts_oracle_eval::content::cards::card_has_tag(
                card.card(),
                sts_oracle_eval::content::cards::CardTag::Strike,
            ));
            if sts_oracle_eval::content::cards::can_upgrade_card_once_for(
                card.card(),
                card.upgrades(),
            ) {
                upgradeable_count += 1;
                upgradeable_basic_count += usize::from(
                    definition.rarity == sts_oracle_eval::content::cards::CardRarity::Basic,
                );
            }
            let cost_bucket = match output.energy_cost {
                0 => 0,
                1 => 1,
                2 => 2,
                3.. => 3,
                -1 => 4,
                _ => 5,
            };
            cost_counts[cost_bucket] += 1;
        }
        for (field, count) in [
            ScalarField::DeckAttackCount,
            ScalarField::DeckSkillCount,
            ScalarField::DeckPowerCount,
            ScalarField::DeckStatusCount,
            ScalarField::DeckCurseCount,
        ]
        .into_iter()
        .zip(type_counts)
        {
            self.scalar(run, field, count);
        }
        for (field, count) in [
            ScalarField::DeckCostZeroCount,
            ScalarField::DeckCostOneCount,
            ScalarField::DeckCostTwoCount,
            ScalarField::DeckCostThreeOrMoreCount,
            ScalarField::DeckCostXCount,
            ScalarField::DeckCostUnplayableCount,
        ]
        .into_iter()
        .zip(cost_counts)
        {
            self.scalar(run, field, count);
        }
        // Current owned deck only. Preview cards do not change these counts;
        // they are eligibility facts, not an estimate of combat upgrade value.
        self.scalar(run, ScalarField::DeckUpgradeableCount, upgradeable_count);
        // Permanent owned membership, not an estimate of combat-zone count or
        // modified Perfected Strike damage. Offers and previews are excluded.
        self.scalar(run, ScalarField::DeckStrikeTagCount, strike_tag_count);
        // Reviewed source-card inventory, not active powers, delivered strength
        // or guaranteed opportunities to play the owned cards.
        mechanic_counts.encode(self, run);
        self.scalar(
            run,
            ScalarField::DeckUpgradeableBasicCount,
            upgradeable_basic_count,
        );

        let mut relics = observation.relics.iter().collect::<Vec<_>>();
        relics.sort_by_key(|relic| {
            (
                relic.relic.stable_ordinal(),
                relic.counter,
                relic.used_up,
                relic.amount,
            )
        });
        for relic in relics {
            let token = self.add_token(TokenKind::Relic)?;
            self.edge(root, RelationKind::ObservationHasRelic, token);
            self.relic_identity_with_mechanics(token, CategoricalField::RelicId, relic.relic);
            self.category(
                token,
                CategoricalField::RelicUsedUp,
                bool_value(relic.used_up),
            );
            self.scalar(token, ScalarField::RelicCounter, relic.counter);
            self.scalar(token, ScalarField::RelicAmount, relic.amount);
        }

        let mut potions = observation.potion_slots().collect::<Vec<_>>();
        potions.sort_by_key(|potion_slot| potion_slot.slot());
        let mut potion_tokens = BTreeMap::new();
        for potion_slot in potions {
            let token = self.add_token(TokenKind::PotionSlot)?;
            self.edge(root, RelationKind::ObservationHasPotionSlot, token);
            self.scalar(token, ScalarField::PotionSlot, potion_slot.slot());
            self.category(
                token,
                CategoricalField::PotionOccupied,
                bool_value(potion_slot.potion().is_some()),
            );
            if let Some(potion) = potion_slot.potion() {
                potion_tokens.insert(potion_slot.slot(), (token, potion.potion()));
                self.potion_identity_with_mechanics(
                    token,
                    CategoricalField::PotionId,
                    potion.potion(),
                );
                self.category(
                    token,
                    CategoricalField::PotionCanUse,
                    bool_value(potion.can_use()),
                );
                self.category(
                    token,
                    CategoricalField::PotionCanDiscard,
                    bool_value(potion.can_discard()),
                );
                self.category(
                    token,
                    CategoricalField::PotionRequiresTarget,
                    bool_value(potion.requires_target()),
                );
            }
        }

        let map_tokens = self.encode_public_map(root, observation.public_map)?;

        let context = self.add_token(TokenKind::Context)?;
        self.edge(root, RelationKind::ObservationHasContext, context);
        self.encode_context(context, observation.context)?;

        let history = self.add_token(TokenKind::History)?;
        self.edge(root, RelationKind::ObservationHasHistory, history);
        self.scalar(
            history,
            ScalarField::ShopPurgeCount,
            observation.public_history.shop_purge_count,
        );
        if let Some(hp_loss) = observation.public_history.recent_combat_hp_loss {
            self.scalar(history, ScalarField::RecentCombatHpLoss, hp_loss);
        }
        if let Some(turns) = observation.public_history.recent_combat_turns {
            self.scalar(history, ScalarField::RecentCombatTurns, turns);
        }
        if let Some(potions_used) = observation.public_history.recent_combat_potions_used {
            self.scalar(history, ScalarField::RecentCombatPotionsUsed, potions_used);
        }

        Ok(StrategicTokenIndex {
            root,
            cards: card_tokens,
            potions: potion_tokens,
            map_nodes: map_tokens,
        })
    }

    pub(super) fn encode_public_map(
        &mut self,
        root: u64,
        public_map: &PlannerPublicMap,
    ) -> Result<BTreeMap<(i32, i32), u64>, SemanticEncodingError> {
        let map = self.add_token(TokenKind::Map)?;
        self.edge(root, RelationKind::ObservationHasMap, map);
        self.scalar(map, ScalarField::MapCurrentX, public_map.current_x);
        self.scalar(map, ScalarField::MapCurrentY, public_map.current_y);
        if let Some(starts) = public_map_preparation_starts_after_current(public_map) {
            self.scalar(map, ScalarField::MapPreparationStartCount, starts.len());
            let preparation = summarize_public_map_preparation(public_map, &starts, 1)
                .map_err(SemanticEncodingError::InvalidPublicMapRoute)?;
            self.encode_map_preparation_summary(map, &preparation);
        }
        if let Some(boss) = public_map.boss {
            self.category(
                map,
                CategoricalField::BossEncounterId,
                boss.stable_ordinal() as i64,
            );
        }
        let mut nodes = public_map.nodes.iter().collect::<Vec<_>>();
        nodes.sort_by_key(|node| (node.y, node.x));
        let mut map_tokens = BTreeMap::new();
        for node in &nodes {
            let token = self.add_token(TokenKind::MapNode)?;
            if map_tokens.insert((node.x, node.y), token).is_some() {
                return Err(SemanticEncodingError::DuplicateMapNode {
                    x: node.x,
                    y: node.y,
                });
            }
            self.edge(map, RelationKind::MapHasNode, token);
            self.scalar(token, ScalarField::MapNodeX, node.x);
            self.scalar(token, ScalarField::MapNodeY, node.y);
            self.category(token, CategoricalField::MapRoom, map_room_value(node.room));
            self.category(
                token,
                CategoricalField::MapNodeHasEmeraldKey,
                bool_value(node.has_emerald_key),
            );
        }
        for node in nodes {
            let source = map_tokens[&(node.x, node.y)];
            for edge in &node.edges {
                let target = map_tokens
                    .get(&(edge.destination_x, edge.destination_y))
                    .copied()
                    .ok_or(SemanticEncodingError::MissingMapTarget {
                        x: edge.destination_x,
                        y: edge.destination_y,
                    })?;
                self.edge(source, RelationKind::MapPathTo, target);
            }
        }
        Ok(map_tokens)
    }

    fn encode_map_preparation_summary(
        &mut self,
        token: u64,
        summary: &PublicMapPreparationSummary,
    ) {
        for (field, steps) in [
            (
                ScalarField::PreparationEliteWithoutRestOrShopSteps,
                summary.elite_steps_by_preparation[0],
            ),
            (
                ScalarField::PreparationEliteWithRestOnlySteps,
                summary.elite_steps_by_preparation[1],
            ),
            (
                ScalarField::PreparationEliteWithShopOnlySteps,
                summary.elite_steps_by_preparation[2],
            ),
            (
                ScalarField::PreparationEliteWithRestAndShopSteps,
                summary.elite_steps_by_preparation[3],
            ),
            (
                ScalarField::PreparationFirstKnownEliteFreeEndSteps,
                summary.first_known_elite_free_end_steps,
            ),
            (
                ScalarField::PreparationFirstUnresolvedEndSteps,
                summary.first_unresolved_end_steps,
            ),
        ] {
            if let Some(steps) = steps {
                self.scalar(token, field, steps);
            }
        }
    }

    fn encode_strategic_selection(
        &mut self,
        observation: LearningStrategicModelObservationV1<'_>,
        draft: &LearningSelectionDraftV1,
        family: LearningRunSelectionFamilyV1<'_>,
        decision: &LearningSelectionDecisionV1,
    ) -> Result<(), SemanticEncodingError> {
        let tokens = self.encode_strategic_observation(observation)?;
        let selection = self.add_token(TokenKind::SelectionState)?;
        self.edge(
            tokens.root,
            RelationKind::ObservationHasSelectionState,
            selection,
        );
        let card_ordinals = (0..family.domain_count())
            .map(|index| {
                family
                    .domain_card_ordinal(index)
                    .ok_or(SemanticEncodingError::MissingSelectionDomain(index))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let domains = self.encode_run_selection_family(
            selection,
            RelationKind::SelectionHasDomain,
            family.scope(),
            family.reason(),
            family.declared_min(),
            family.declared_max(),
            &card_ordinals,
            &tokens.cards,
        )?;
        for (position, domain_index) in draft.selected_domain_indices().iter().copied().enumerate()
        {
            let target = domains
                .get(domain_index)
                .copied()
                .ok_or(SemanticEncodingError::MissingSelectionDomain(domain_index))?;
            let chosen = self.add_token(TokenKind::SelectionChosen)?;
            self.scalar(chosen, ScalarField::SelectionChosenPosition, position);
            self.edge(selection, RelationKind::SelectionHasChosen, chosen);
            self.edge(chosen, RelationKind::ChosenTargetsDomain, target);
        }
        for candidate in &decision.candidates {
            let token = self.begin_candidate()?;
            self.edge(tokens.root, RelationKind::ObservationHasCandidate, token);
            match candidate.semantics {
                LearningSelectionCandidateSemanticsV1::Submit => self.category(
                    token,
                    CategoricalField::SelectionCandidateKind,
                    SelectionCandidateKind::Submit as i64,
                ),
                LearningSelectionCandidateSemanticsV1::Append { domain_index } => {
                    self.category(
                        token,
                        CategoricalField::SelectionCandidateKind,
                        SelectionCandidateKind::Append as i64,
                    );
                    let target = domains
                        .get(domain_index)
                        .copied()
                        .ok_or(SemanticEncodingError::MissingSelectionDomain(domain_index))?;
                    self.edge(token, RelationKind::CandidateTargets, target);
                }
            }
            self.finish_candidate(token)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_run_selection_family(
        &mut self,
        owner: u64,
        domain_relation: RelationKind,
        scope: SelectionScope,
        reason: SelectionReason,
        declared_min: usize,
        declared_max: usize,
        card_ordinals: &[usize],
        card_tokens: &[u64],
    ) -> Result<Vec<u64>, SemanticEncodingError> {
        self.category(
            owner,
            CategoricalField::SelectionInputEncoding,
            RUN_SELECTION_DECK_CARD_ORDINAL_INPUT_ENCODING,
        );
        self.category(owner, CategoricalField::ActionSelectionScope, scope as i64);
        self.encode_run_selection_reason(owner, reason)?;
        self.category(
            owner,
            CategoricalField::SelectionPayloadDistinctBy,
            CombatSelectionDistinctByV2::CardLineage as i64,
        );
        self.scalar(
            owner,
            ScalarField::SelectionRawDomainCount,
            card_ordinals.len(),
        );
        self.scalar(
            owner,
            ScalarField::SelectionEligibleDomainCount,
            card_ordinals.len(),
        );
        self.scalar(
            owner,
            ScalarField::SelectionMaxDistinctCount,
            card_ordinals.len(),
        );
        self.scalar(owner, ScalarField::SelectionDeclaredMin, declared_min);
        self.scalar(owner, ScalarField::SelectionDeclaredMax, declared_max);
        self.scalar(
            owner,
            ScalarField::SelectionEffectiveMax,
            declared_max.min(card_ordinals.len()),
        );

        let mut domains = Vec::with_capacity(card_ordinals.len());
        for (domain_index, card_ordinal) in card_ordinals.iter().copied().enumerate() {
            let card = card_tokens
                .get(card_ordinal)
                .copied()
                .ok_or(SemanticEncodingError::MissingCardTarget(card_ordinal))?;
            let domain = self.add_token(TokenKind::SelectionDomain)?;
            self.edge(owner, domain_relation, domain);
            self.edge(domain, RelationKind::SelectionDomainTargets, card);
            self.category(
                domain,
                CategoricalField::SelectionDomainKind,
                SelectionDomainKind::Card as i64,
            );
            self.category(
                domain,
                CategoricalField::SelectionDomainEligible,
                bool_value(true),
            );
            self.scalar(domain, ScalarField::SelectionDomainAddress, domain_index);
            domains.push(domain);
        }
        Ok(domains)
    }

    fn encode_run_selection_reason(
        &mut self,
        token: u64,
        reason: SelectionReason,
    ) -> Result<(), SemanticEncodingError> {
        let kind = match reason {
            SelectionReason::Upgrade => SelectionReasonKind::RunUpgrade,
            SelectionReason::Purge => SelectionReasonKind::RunPurge,
            SelectionReason::Transform => SelectionReasonKind::RunTransform,
            SelectionReason::TransformUpgraded => SelectionReasonKind::RunTransformUpgraded,
            SelectionReason::Duplicate => SelectionReasonKind::RunDuplicate,
            SelectionReason::BottleFlame => SelectionReasonKind::RunBottleFlame,
            SelectionReason::BottleLightning => SelectionReasonKind::RunBottleLightning,
            SelectionReason::BottleTornado => SelectionReasonKind::RunBottleTornado,
            _ => return Err(SemanticEncodingError::UnsupportedRunSelectionReason(reason)),
        };
        self.category(token, CategoricalField::SelectionReasonKind, kind as i64);
        Ok(())
    }

    fn encode_context(
        &mut self,
        token: u64,
        context: &PlannerDecisionContext,
    ) -> Result<(), SemanticEncodingError> {
        match context {
            PlannerDecisionContext::Map { overlay } => {
                self.context_kind(token, ContextKind::Map);
                self.category(
                    token,
                    CategoricalField::ContextOverlay,
                    bool_value(*overlay),
                );
            }
            PlannerDecisionContext::CardReward {
                reward_item_index,
                pending_rewards,
            } => {
                self.context_kind(token, ContextKind::CardReward);
                if let Some(index) = reward_item_index {
                    self.scalar(token, ScalarField::ContextRewardItemIndex, *index);
                }
                self.encode_pending_rewards(token, pending_rewards)?;
            }
            PlannerDecisionContext::Event {
                event,
                screen,
                face_trader_result,
            } => {
                self.context_kind(token, ContextKind::Event);
                self.category(
                    token,
                    CategoricalField::ContextEventId,
                    event.stable_ordinal() as i64,
                );
                self.scalar(token, ScalarField::ContextEventScreen, *screen);
                if let Some(result) = face_trader_result {
                    self.category(
                        token,
                        CategoricalField::ContextFaceTraderResult,
                        match result {
                            FaceTraderResult::Touch => 0,
                            FaceTraderResult::Trade => 1,
                            FaceTraderResult::Leave => 2,
                        },
                    );
                }
            }
            PlannerDecisionContext::Shop {
                purge_cost,
                purge_available,
                inventory,
            } => {
                self.context_kind(token, ContextKind::Shop);
                self.scalar(token, ScalarField::ContextPurgeCost, *purge_cost);
                self.category(
                    token,
                    CategoricalField::ContextPurgeAvailable,
                    bool_value(*purge_available),
                );
                for offer in &inventory.cards {
                    let item = self.shop_offer(
                        token,
                        offer.slot,
                        offer.price,
                        offer.affordable,
                        &offer.purchase_status,
                    )?;
                    self.action_kind(item, ActionKind::BuyCard);
                    self.action_card(item, offer.card, offer.upgrades);
                }
                for offer in &inventory.relics {
                    let item = self.shop_offer(
                        token,
                        offer.slot,
                        offer.price,
                        offer.affordable,
                        &offer.purchase_status,
                    )?;
                    self.action_kind(item, ActionKind::BuyRelic);
                    self.relic_identity_with_mechanics(
                        item,
                        CategoricalField::ActionRelicId,
                        offer.relic,
                    );
                }
                for offer in &inventory.potions {
                    let item = self.shop_offer(
                        token,
                        offer.slot,
                        offer.price,
                        offer.affordable,
                        &offer.purchase_status,
                    )?;
                    self.action_kind(item, ActionKind::BuyPotion);
                    self.action_potion(item, offer.potion);
                }
                if let Some(offer) = &inventory.purge {
                    let item = self.shop_offer(
                        token,
                        0,
                        offer.price,
                        offer.affordable,
                        &offer.purchase_status,
                    )?;
                    self.action_kind(item, ActionKind::RemoveCard);
                }
            }
            PlannerDecisionContext::Reward { pending_rewards } => {
                self.context_kind(token, ContextKind::Reward);
                self.encode_pending_rewards(token, pending_rewards)?;
            }
            PlannerDecisionContext::Campfire => self.context_kind(token, ContextKind::Campfire),
            PlannerDecisionContext::BossRelic => self.context_kind(token, ContextKind::BossRelic),
            PlannerDecisionContext::RunChoice { .. } => {
                self.context_kind(token, ContextKind::RunChoice)
            }
            PlannerDecisionContext::Treasure => self.context_kind(token, ContextKind::Treasure),
            PlannerDecisionContext::RunTransition { stage } => {
                self.context_kind(token, ContextKind::RunTransition);
                self.category(
                    token,
                    CategoricalField::ContextRunTransitionStage,
                    match stage {
                        PlannerRunTransitionV1::SecondBoss => 0,
                        PlannerRunTransitionV1::FinalAct => 1,
                        PlannerRunTransitionV1::VictoryScreen => 2,
                    },
                );
            }
        }
        Ok(())
    }

    fn encode_pending_rewards(
        &mut self,
        context: u64,
        rewards: &[PlannerRewardDescriptor],
    ) -> Result<(), SemanticEncodingError> {
        for (index, reward) in rewards.iter().enumerate() {
            let token = self.add_token(TokenKind::PendingReward)?;
            self.edge(context, RelationKind::ContextHasPendingReward, token);
            self.scalar(token, ScalarField::PendingRewardItemIndex, index);
            self.encode_reward(token, reward, RelationKind::RewardHasOfferedCard)?;
        }
        Ok(())
    }

    fn shop_offer(
        &mut self,
        context: u64,
        slot: usize,
        price: i32,
        affordable: bool,
        status: &PlannerShopPurchaseStatusV1,
    ) -> Result<u64, SemanticEncodingError> {
        let token = self.add_token(TokenKind::ShopOffer)?;
        self.edge(context, RelationKind::ContextHasShopOffer, token);
        self.scalar(token, ScalarField::ActionShopSlot, slot);
        self.scalar(token, ScalarField::ActionPrice, price);
        self.category(
            token,
            CategoricalField::ShopAffordable,
            bool_value(affordable),
        );
        self.category(
            token,
            CategoricalField::ShopPurchaseStatus,
            shop_status_value(status),
        );
        Ok(token)
    }

    fn encode_action(
        &mut self,
        token: u64,
        action: &LearningStrategicActionSemanticsV2,
        card_tokens: &[u64],
        potion_tokens: &BTreeMap<usize, (u64, PotionId)>,
        map_tokens: &BTreeMap<(i32, i32), u64>,
        observation: LearningStrategicModelObservationV1<'_>,
    ) -> Result<(), SemanticEncodingError> {
        match action {
            LearningStrategicActionSemanticsV2::ChooseRouteNode {
                x,
                y,
                flight,
                question_mark_outcomes,
            } => {
                self.action_kind(token, ActionKind::ChooseRouteNode);
                self.scalar(token, ScalarField::ActionX, *x);
                self.scalar(token, ScalarField::ActionY, *y);
                self.category(token, CategoricalField::ActionFlight, bool_value(*flight));
                for outcome in question_mark_outcomes {
                    self.category(
                        token,
                        CategoricalField::QuestionMarkOutcome,
                        question_mark_outcome_value(*outcome),
                    );
                }
                let target = map_tokens
                    .get(&(*x, *y))
                    .copied()
                    .ok_or(SemanticEncodingError::MissingMapTarget { x: *x, y: *y })?;
                self.edge(token, RelationKind::CandidateTargets, target);
                let route = summarize_public_map_route(observation.public_map, *x, *y)
                    .map_err(SemanticEncodingError::InvalidPublicMapRoute)?;
                let preparation =
                    summarize_public_map_preparation(observation.public_map, &[(*x, *y)], 0)
                        .map_err(SemanticEncodingError::InvalidPublicMapRoute)?;
                self.encode_map_preparation_summary(token, &preparation);
                for (field, value) in [
                    (ScalarField::RouteMinElites, route.min_elites),
                    (ScalarField::RouteMaxElites, route.max_elites),
                    (ScalarField::RouteMinCombatRooms, route.min_combat_rooms),
                    (ScalarField::RouteMaxCombatRooms, route.max_combat_rooms),
                    (ScalarField::RouteMinUnknownRooms, route.min_unknown_rooms),
                    (ScalarField::RouteMaxUnknownRooms, route.max_unknown_rooms),
                    (ScalarField::RouteMinRests, route.min_rests),
                    (ScalarField::RouteMaxRests, route.max_rests),
                    (ScalarField::RouteMinShops, route.min_shops),
                    (ScalarField::RouteMaxShops, route.max_shops),
                ] {
                    self.scalar(token, field, value);
                }
                for (field, value) in [
                    (ScalarField::RouteFirstEliteSteps, route.first_elite_steps),
                    (ScalarField::RouteFirstRestSteps, route.first_rest_steps),
                    (ScalarField::RouteFirstShopSteps, route.first_shop_steps),
                    (
                        ScalarField::RouteFirstShopBeforeEliteSteps,
                        route.first_shop_before_elite_steps,
                    ),
                ] {
                    if let Some(value) = value {
                        self.scalar(token, field, value);
                    }
                }
            }
            LearningStrategicActionSemanticsV2::ChooseEventOption {
                event,
                screen,
                option_index,
                action,
                public_semantics,
            } => {
                self.action_kind(token, ActionKind::ChooseEventOption);
                self.category(
                    token,
                    CategoricalField::ActionEventId,
                    event.stable_ordinal() as i64,
                );
                self.category(token, CategoricalField::ActionEventKind, *action as i64);
                self.scalar(token, ScalarField::ActionScreen, *screen);
                self.scalar(token, ScalarField::ActionOptionIndex, *option_index);
                self.scalar(
                    token,
                    ScalarField::ActionEventGoldDeltaMin,
                    public_semantics.gold_delta_min,
                );
                self.scalar(
                    token,
                    ScalarField::ActionEventGoldDeltaMax,
                    public_semantics.gold_delta_max,
                );
                self.scalar(
                    token,
                    ScalarField::ActionEventHpDelta,
                    public_semantics.hp_delta,
                );
                self.scalar(
                    token,
                    ScalarField::ActionEventMaxHpDelta,
                    public_semantics.max_hp_delta,
                );
                for (field, value) in [
                    (
                        ScalarField::ActionEventRelicsGained,
                        public_semantics.relics_gained,
                    ),
                    (
                        ScalarField::ActionEventRelicsLost,
                        public_semantics.relics_lost,
                    ),
                    (
                        ScalarField::ActionEventPotionsGained,
                        public_semantics.potions_gained,
                    ),
                    (
                        ScalarField::ActionEventCardsGained,
                        public_semantics.cards_gained,
                    ),
                    (
                        ScalarField::ActionEventCardsOffered,
                        public_semantics.cards_offered,
                    ),
                    (
                        ScalarField::ActionEventCursesGained,
                        public_semantics.curses_gained,
                    ),
                    (
                        ScalarField::ActionEventCardsRemoved,
                        public_semantics.cards_removed,
                    ),
                    (
                        ScalarField::ActionEventCardsUpgraded,
                        public_semantics.cards_upgraded,
                    ),
                    (
                        ScalarField::ActionEventCardsTransformed,
                        public_semantics.cards_transformed,
                    ),
                    (
                        ScalarField::ActionEventCardsDuplicated,
                        public_semantics.cards_duplicated,
                    ),
                ] {
                    self.scalar(token, field, value);
                }
                for (field, value) in [
                    (
                        CategoricalField::ActionEventRandomOutcome,
                        public_semantics.random_outcome,
                    ),
                    (
                        CategoricalField::ActionEventStartsCombat,
                        public_semantics.starts_combat,
                    ),
                    (
                        CategoricalField::ActionEventRepeatable,
                        public_semantics.repeatable,
                    ),
                    (
                        CategoricalField::ActionEventTerminal,
                        public_semantics.terminal,
                    ),
                    (
                        CategoricalField::ActionEventUpgradesAllCards,
                        public_semantics.upgrades_all_cards,
                    ),
                ] {
                    self.category(token, field, bool_value(value));
                }
                for card in &public_semantics.specific_cards_removed {
                    let ordinal = observation
                        .card_effect_target_ordinal(card)
                        .map_err(SemanticEncodingError::InvalidEventCardTarget)?;
                    let target = card_tokens
                        .get(ordinal)
                        .copied()
                        .ok_or(SemanticEncodingError::MissingCardTarget(ordinal))?;
                    self.edge(token, RelationKind::CandidateRemovesCard, target);
                }
            }
            LearningStrategicActionSemanticsV2::TakeCard {
                reward_item_index,
                option_index: _,
                card,
                upgrades,
            } => {
                self.action_kind(token, ActionKind::TakeCard);
                self.optional_reward_index(token, *reward_item_index);
                self.action_card(token, *card, *upgrades);
            }
            LearningStrategicActionSemanticsV2::OpenCardReward { reward_item_index } => {
                self.action_kind(token, ActionKind::OpenCardReward);
                self.scalar(
                    token,
                    ScalarField::ActionRewardItemIndex,
                    *reward_item_index,
                );
            }
            LearningStrategicActionSemanticsV2::SingingBowl { reward_item_index } => {
                self.action_kind(token, ActionKind::SingingBowl);
                self.optional_reward_index(token, *reward_item_index);
            }
            LearningStrategicActionSemanticsV2::SkipCardReward { reward_item_index } => {
                self.action_kind(token, ActionKind::SkipCardReward);
                self.scalar(
                    token,
                    ScalarField::ActionRewardItemIndex,
                    *reward_item_index,
                );
            }
            LearningStrategicActionSemanticsV2::ClaimReward {
                reward_item_index,
                reward,
            } => {
                self.action_kind(token, ActionKind::ClaimReward);
                self.scalar(
                    token,
                    ScalarField::ActionRewardItemIndex,
                    *reward_item_index,
                );
                self.encode_reward(token, reward, RelationKind::CandidateHasPayload)?;
            }
            LearningStrategicActionSemanticsV2::Rest => {
                self.action_kind(token, ActionKind::Rest);
                let relics = observation
                    .relics
                    .iter()
                    .map(|relic| sts_oracle_eval::content::relics::RelicState {
                        id: relic.relic,
                        counter: relic.counter,
                        used_up: relic.used_up,
                        amount: relic.amount,
                    })
                    .collect::<Vec<_>>();
                self.scalar(
                    token,
                    ScalarField::ActionRestHpGain,
                    sts_oracle_eval::engine::campfire_handler::campfire_rest_hp_gain(
                        observation.run.current_hp,
                        observation.run.max_hp,
                        &relics,
                    ),
                );
            }
            LearningStrategicActionSemanticsV2::Smith {
                card_ordinal,
                card,
                upgrades,
            } => {
                self.action_kind(token, ActionKind::Smith);
                self.action_card(token, *card, *upgrades);
                self.link_card_target(token, *card_ordinal, card_tokens)?;
                let observed = observation
                    .cards()
                    .nth(*card_ordinal)
                    .ok_or(SemanticEncodingError::MissingCardTarget(*card_ordinal))?;
                if observed.card() != *card || observed.upgrades() != *upgrades {
                    return Err(SemanticEncodingError::InvalidSmithTarget(*card_ordinal));
                }
                let mut upgraded = mechanics::observed_owned_card(observed);
                if !sts_oracle_eval::content::cards::upgrade_card_once_java(&mut upgraded) {
                    return Err(SemanticEncodingError::InvalidSmithTarget(*card_ordinal));
                }
                let after = self.add_token(TokenKind::UpgradedCard)?;
                self.edge(token, RelationKind::CandidateUpgradesTo, after);
                self.card_identity_with_mechanics(
                    after,
                    CategoricalField::CardId,
                    upgraded.id,
                    Some(upgraded.upgrades),
                );
                self.scalar(after, ScalarField::CardUpgrades, upgraded.upgrades);
                self.owned_card_output(after, &upgraded);
            }
            LearningStrategicActionSemanticsV2::Dig => self.action_kind(token, ActionKind::Dig),
            LearningStrategicActionSemanticsV2::Lift => self.action_kind(token, ActionKind::Lift),
            LearningStrategicActionSemanticsV2::Toke {
                card_ordinal,
                card,
                upgrades,
            } => {
                self.action_kind(token, ActionKind::Toke);
                self.action_card(token, *card, *upgrades);
                self.link_card_target(token, *card_ordinal, card_tokens)?;
            }
            LearningStrategicActionSemanticsV2::Recall => {
                self.action_kind(token, ActionKind::Recall)
            }
            LearningStrategicActionSemanticsV2::BuyCard {
                shop_slot,
                card,
                upgrades,
                price,
            } => {
                self.action_kind(token, ActionKind::BuyCard);
                self.scalar(token, ScalarField::ActionShopSlot, *shop_slot);
                self.scalar(token, ScalarField::ActionPrice, *price);
                self.action_card(token, *card, *upgrades);
            }
            LearningStrategicActionSemanticsV2::BuyRelic {
                shop_slot,
                relic,
                price,
            } => {
                self.action_kind(token, ActionKind::BuyRelic);
                self.scalar(token, ScalarField::ActionShopSlot, *shop_slot);
                self.scalar(token, ScalarField::ActionPrice, *price);
                self.relic_identity_with_mechanics(token, CategoricalField::ActionRelicId, *relic);
            }
            LearningStrategicActionSemanticsV2::BuyPotion {
                shop_slot,
                potion,
                price,
            } => {
                self.action_kind(token, ActionKind::BuyPotion);
                self.scalar(token, ScalarField::ActionShopSlot, *shop_slot);
                self.scalar(token, ScalarField::ActionPrice, *price);
                self.action_potion(token, *potion);
            }
            LearningStrategicActionSemanticsV2::UseRunPotion { slot, potion } => {
                self.action_kind(token, ActionKind::UseRunPotion);
                self.action_potion(token, *potion);
                self.link_potion_target(token, *slot, *potion, potion_tokens)?;
            }
            LearningStrategicActionSemanticsV2::DiscardRunPotion { slot, potion } => {
                self.action_kind(token, ActionKind::DiscardRunPotion);
                self.action_potion(token, *potion);
                self.link_potion_target(token, *slot, *potion, potion_tokens)?;
            }
            LearningStrategicActionSemanticsV2::RemoveCard {
                card_ordinal,
                card,
                upgrades,
                price,
            } => {
                self.action_kind(token, ActionKind::RemoveCard);
                self.scalar(token, ScalarField::ActionPrice, *price);
                self.action_card(token, *card, *upgrades);
                self.link_card_target(token, *card_ordinal, card_tokens)?;
            }
            LearningStrategicActionSemanticsV2::OpenPendingRewards => {
                self.action_kind(token, ActionKind::OpenPendingRewards)
            }
            LearningStrategicActionSemanticsV2::LeaveShop => {
                self.action_kind(token, ActionKind::LeaveShop)
            }
            LearningStrategicActionSemanticsV2::TakeBossRelic {
                option_index: _,
                relic,
            } => {
                self.action_kind(token, ActionKind::TakeBossRelic);
                self.relic_identity_with_mechanics(token, CategoricalField::ActionRelicId, *relic);
            }
            LearningStrategicActionSemanticsV2::SkipBossRelic => {
                self.action_kind(token, ActionKind::SkipBossRelic)
            }
            LearningStrategicActionSemanticsV2::BeginRunCardSelection {
                scope,
                reason,
                min_choices,
                max_choices,
                selectable_card_ordinals,
            } => {
                self.action_kind(token, ActionKind::BeginRunCardSelection);
                self.encode_run_selection_family(
                    token,
                    RelationKind::CandidateHasSelectionDomain,
                    *scope,
                    *reason,
                    *min_choices,
                    *max_choices,
                    selectable_card_ordinals,
                    card_tokens,
                )?;
            }
            LearningStrategicActionSemanticsV2::SubmitRunSelection {
                scope,
                selected_card_ordinals,
            } => {
                self.action_kind(token, ActionKind::SubmitRunSelection);
                self.category(token, CategoricalField::ActionSelectionScope, *scope as i64);
                for ordinal in selected_card_ordinals {
                    self.link_card_target(token, *ordinal, card_tokens)?;
                }
            }
            LearningStrategicActionSemanticsV2::OpenChest => {
                self.action_kind(token, ActionKind::OpenChest)
            }
            LearningStrategicActionSemanticsV2::Proceed { site } => {
                self.action_kind(token, ActionKind::Proceed);
                self.category(token, CategoricalField::ActionSite, *site as i64);
            }
            LearningStrategicActionSemanticsV2::Cancel { site } => {
                self.action_kind(token, ActionKind::Cancel);
                self.category(token, CategoricalField::ActionSite, *site as i64);
            }
        }
        Ok(())
    }

    fn encode_reward(
        &mut self,
        candidate: u64,
        reward: &PlannerRewardDescriptor,
        payload_relation: RelationKind,
    ) -> Result<(), SemanticEncodingError> {
        match reward {
            PlannerRewardDescriptor::Gold { amount } => {
                self.reward_kind(candidate, RewardKind::Gold);
                self.scalar(candidate, ScalarField::RewardAmount, *amount);
            }
            PlannerRewardDescriptor::StolenGold { amount } => {
                self.reward_kind(candidate, RewardKind::StolenGold);
                self.scalar(candidate, ScalarField::RewardAmount, *amount);
            }
            PlannerRewardDescriptor::CardReward { cards } => {
                self.reward_kind(candidate, RewardKind::CardReward);
                for card in cards {
                    let token = self.add_token(TokenKind::OfferedCard)?;
                    self.edge(candidate, payload_relation, token);
                    self.card_identity_with_mechanics(
                        token,
                        CategoricalField::CardId,
                        card.card,
                        Some(card.upgrades),
                    );
                    self.scalar(token, ScalarField::CardUpgrades, card.upgrades);
                }
            }
            PlannerRewardDescriptor::Relic { relic } => {
                self.reward_kind(candidate, RewardKind::Relic);
                self.relic_identity_with_mechanics(
                    candidate,
                    CategoricalField::ActionRelicId,
                    *relic,
                );
            }
            PlannerRewardDescriptor::Potion { potion } => {
                self.reward_kind(candidate, RewardKind::Potion);
                self.action_potion(candidate, *potion);
            }
            PlannerRewardDescriptor::EmeraldKey => {
                self.reward_kind(candidate, RewardKind::EmeraldKey)
            }
            PlannerRewardDescriptor::SapphireKey => {
                self.reward_kind(candidate, RewardKind::SapphireKey)
            }
        }
        Ok(())
    }

    fn link_card_target(
        &mut self,
        candidate: u64,
        ordinal: usize,
        card_tokens: &[u64],
    ) -> Result<(), SemanticEncodingError> {
        let target = card_tokens
            .get(ordinal)
            .copied()
            .ok_or(SemanticEncodingError::MissingCardTarget(ordinal))?;
        self.edge(candidate, RelationKind::CandidateTargets, target);
        Ok(())
    }

    fn link_potion_target(
        &mut self,
        candidate: u64,
        slot: usize,
        potion: PotionId,
        potion_tokens: &BTreeMap<usize, (u64, PotionId)>,
    ) -> Result<(), SemanticEncodingError> {
        let (target, observed_potion) = potion_tokens
            .get(&slot)
            .copied()
            .ok_or(SemanticEncodingError::MissingPotionRelationTarget(slot))?;
        if observed_potion != potion {
            return Err(SemanticEncodingError::PotionTargetMismatch { slot, potion });
        }
        self.edge(candidate, RelationKind::CandidateTargets, target);
        Ok(())
    }

    fn optional_reward_index(&mut self, token: u64, index: Option<usize>) {
        if let Some(index) = index {
            self.scalar(token, ScalarField::ActionRewardItemIndex, index);
        }
    }

    fn context_kind(&mut self, token: u64, kind: ContextKind) {
        self.category(token, CategoricalField::ContextKind, kind as i64);
    }

    fn action_kind(&mut self, token: u64, kind: ActionKind) {
        self.category(token, CategoricalField::ActionKind, kind as i64);
    }

    fn reward_kind(&mut self, token: u64, kind: RewardKind) {
        self.category(token, CategoricalField::RewardKind, kind as i64);
    }

    fn add_token(&mut self, kind: TokenKind) -> Result<u64, SemanticEncodingError> {
        let index = self.token_count()?;
        self.token_kinds.push(kind as u16);
        self.token_scopes
            .push(self.active_candidate_scope.unwrap_or(0));
        Ok(index)
    }

    fn begin_candidate(&mut self) -> Result<u64, SemanticEncodingError> {
        if self.active_candidate_scope.is_some() {
            return Err(SemanticEncodingError::CandidateScopeMismatch);
        }
        self.row_candidate_scope = self
            .row_candidate_scope
            .checked_add(1)
            .ok_or(SemanticEncodingError::IndexOverflow)?;
        self.active_candidate_scope = Some(self.row_candidate_scope);
        self.add_token(TokenKind::Candidate)
    }

    fn finish_candidate(&mut self, token: u64) -> Result<(), SemanticEncodingError> {
        if self.active_candidate_scope.is_none()
            || self.token_scopes.get(token as usize).copied() != self.active_candidate_scope
        {
            return Err(SemanticEncodingError::CandidateScopeMismatch);
        }
        self.candidate_token_indices.push(token);
        self.active_candidate_scope = None;
        Ok(())
    }

    fn token_count(&self) -> Result<u64, SemanticEncodingError> {
        u64::try_from(self.token_kinds.len()).map_err(|_| SemanticEncodingError::IndexOverflow)
    }

    fn category(&mut self, token: u64, field: CategoricalField, value: i64) {
        self.categorical.token_indices.push(token);
        self.categorical.fields.push(field as u16);
        self.categorical.values.push(value);
    }

    fn scalar(&mut self, token: u64, field: ScalarField, value: impl IntoF32) {
        self.scalar.token_indices.push(token);
        self.scalar.fields.push(field as u16);
        self.scalar.values.push(value.into_f32());
    }

    fn edge(&mut self, source: u64, relation: RelationKind, target: u64) {
        self.relation.source_token_indices.push(source);
        self.relation.relations.push(relation as u16);
        self.relation.target_token_indices.push(target);
    }
}

trait IntoF32 {
    fn into_f32(self) -> f32;
}

macro_rules! impl_into_f32 {
    ($($type:ty),+ $(,)?) => {
        $(
            impl IntoF32 for $type {
                fn into_f32(self) -> f32 {
                    self as f32
                }
            }
        )+
    };
}

impl_into_f32!(u8, u16, u32, u64, i8, i32, usize);

fn bool_value(value: bool) -> i64 {
    i64::from(value)
}

// Numeric model vocabulary is explicit: source enum ordering is not a schema.
fn map_room_value(room: PlannerMapRoom) -> i64 {
    match room {
        PlannerMapRoom::QuestionMark => MapRoomKind::QuestionMark as i64,
        PlannerMapRoom::Monster => MapRoomKind::Monster as i64,
        PlannerMapRoom::Elite => MapRoomKind::Elite as i64,
        PlannerMapRoom::Boss => MapRoomKind::Boss as i64,
        PlannerMapRoom::Rest => MapRoomKind::Rest as i64,
        PlannerMapRoom::Shop => MapRoomKind::Shop as i64,
        PlannerMapRoom::Treasure => MapRoomKind::Treasure as i64,
        PlannerMapRoom::Victory => MapRoomKind::Victory as i64,
    }
}

fn question_mark_outcome_value(outcome: PlannerQuestionMarkOutcome) -> i64 {
    match outcome {
        PlannerQuestionMarkOutcome::Event => QuestionMarkOutcomeKind::Event as i64,
        PlannerQuestionMarkOutcome::Monster => QuestionMarkOutcomeKind::Monster as i64,
        PlannerQuestionMarkOutcome::Shop => QuestionMarkOutcomeKind::Shop as i64,
        PlannerQuestionMarkOutcome::Treasure => QuestionMarkOutcomeKind::Treasure as i64,
        PlannerQuestionMarkOutcome::Elite => QuestionMarkOutcomeKind::Elite as i64,
    }
}

fn shop_status_value(status: &PlannerShopPurchaseStatusV1) -> i64 {
    match status {
        PlannerShopPurchaseStatusV1::Purchasable => ShopPurchaseStatusKind::Purchasable as i64,
        PlannerShopPurchaseStatusV1::Blocked { reason } => match reason {
            PlannerShopPurchaseBlockReasonV1::MerchantUnavailable { .. } => {
                ShopPurchaseStatusKind::MerchantUnavailable as i64
            }
            PlannerShopPurchaseBlockReasonV1::NotEnoughGold { .. } => {
                ShopPurchaseStatusKind::NotEnoughGold as i64
            }
            PlannerShopPurchaseBlockReasonV1::BlockedBySozu => {
                ShopPurchaseStatusKind::BlockedBySozu as i64
            }
            PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot => {
                ShopPurchaseStatusKind::NoEmptyPotionSlot as i64
            }
            PlannerShopPurchaseBlockReasonV1::PurgeAlreadyUsed => {
                ShopPurchaseStatusKind::PurgeAlreadyUsed as i64
            }
            PlannerShopPurchaseBlockReasonV1::NoEligiblePurgeTarget => {
                ShopPurchaseStatusKind::NoEligiblePurgeTarget as i64
            }
        },
    }
}
