use serde::{Deserialize, Serialize};

use crate::content::cards::CardId;
use crate::content::events::face_trader::FaceTraderResult;
use crate::content::monsters::factory::EncounterId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::runtime::combat::CardLineageId;
use crate::state::events::{EventActionKind, EventId};
use crate::state::map::node::RoomType;
use crate::state::selection::{DomainEventSource, SelectionReason, SelectionScope};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerDecisionSite {
    Map,
    CardReward,
    Neow,
    Event,
    Shop,
    Campfire,
    BossRelic,
    Reward,
    RunChoice,
    Treasure,
    RunTransition,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerRunTransitionV1 {
    SecondBoss,
    FinalAct,
    VictoryScreen,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerRunGoal {
    ActThreeVictory,
    HeartVictory,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerPlayerClass {
    Ironclad,
    Silent,
    Defect,
    Watcher,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerMechanicsManifest {
    pub mechanics_id: String,
    pub mechanics_version: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerObservation {
    pub schema_name: String,
    pub schema_version: u32,
    pub observation_id: String,
    pub mechanics: PlannerMechanicsManifest,
    pub run_goal: PlannerRunGoal,
    pub decision_site: PlannerDecisionSite,
    pub run: PlannerRunScalars,
    pub cards: Vec<PlannerCardObservation>,
    pub relics: Vec<PlannerRelicObservation>,
    pub potions: Vec<PlannerPotionSlotObservation>,
    pub public_map: PlannerPublicMap,
    pub context: PlannerDecisionContext,
    pub public_history: PlannerPublicHistory,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerRunScalars {
    pub player_class: PlannerPlayerClass,
    pub ascension_level: u8,
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub keys: [bool; 3],
    pub potion_capacity: usize,
    /// Public, completed-Boss preparation; supplied by the controller's pending
    /// transition, including relic effects that interrupt Act entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_act_entry: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerCardObservation {
    pub card_lineage: CardLineageId,
    pub card: CardId,
    pub upgrades: u8,
    pub misc_value: i32,
    pub base_damage_override: Option<i32>,
    pub base_block_override: Option<i32>,
    pub cost_modifier: i8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerRelicObservation {
    pub relic: RelicId,
    pub counter: i32,
    pub used_up: bool,
    pub amount: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerPotionSlotObservation {
    pub slot: usize,
    pub potion: Option<PlannerPotionObservation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerPotionObservation {
    pub potion: PotionId,
    pub potion_uuid: u32,
    pub can_use: bool,
    pub can_discard: bool,
    pub requires_target: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerShopInventoryV1 {
    pub cards: Vec<PlannerShopCardOfferV1>,
    pub relics: Vec<PlannerShopRelicOfferV1>,
    pub potions: Vec<PlannerShopPotionOfferV1>,
    pub purge: Option<PlannerShopPurgeOfferV1>,
}

impl PlannerShopInventoryV1 {
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
            && self.relics.is_empty()
            && self.potions.is_empty()
            && self.purge.is_none()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerShopCardOfferV1 {
    /// Current engine slot used by `BuyCard`; a purchase can remove or replace it.
    pub slot: usize,
    pub card: CardId,
    pub upgrades: u8,
    pub price: i32,
    pub affordable: bool,
    pub purchase_status: PlannerShopPurchaseStatusV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerShopRelicOfferV1 {
    /// Current engine slot used by `BuyRelic`; a purchase can remove or replace it.
    pub slot: usize,
    pub relic: RelicId,
    pub price: i32,
    pub affordable: bool,
    pub purchase_status: PlannerShopPurchaseStatusV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerShopPotionOfferV1 {
    /// Current engine slot used by `BuyPotion`; a purchase can remove or replace it.
    pub slot: usize,
    pub potion: PotionId,
    pub price: i32,
    pub affordable: bool,
    pub purchase_status: PlannerShopPurchaseStatusV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerShopPurgeOfferV1 {
    pub price: i32,
    pub affordable: bool,
    pub purchase_status: PlannerShopPurchaseStatusV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PlannerShopPurchaseStatusV1 {
    Purchasable,
    Blocked {
        reason: PlannerShopPurchaseBlockReasonV1,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlannerShopPurchaseBlockReasonV1 {
    MerchantUnavailable { detail: Option<String> },
    NotEnoughGold { gold: i32, price: i32 },
    BlockedBySozu,
    NoEmptyPotionSlot,
    PurgeAlreadyUsed,
    NoEligiblePurgeTarget,
}

/// Actor navigation graph. Ordinary-act boss links use the route action's
/// control point; invisible matrix cells are omitted. This projection is not
/// the generator's source topology and must not be used for map parity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerPublicMap {
    pub current_x: i32,
    pub current_y: i32,
    pub boss: Option<EncounterId>,
    pub nodes: Vec<PlannerPublicMapNode>,
}

/// Actor-visible meaning of a map node.
///
/// The exact engine keeps the original game's `RoomType::EventRoom` name for
/// both generated `?` nodes and an event room after entry. Public map topology
/// must not collapse those two moments: before entry the player can only see a
/// question mark, which may resolve to several room kinds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerMapRoom {
    QuestionMark,
    Monster,
    Elite,
    Boss,
    Rest,
    Shop,
    Treasure,
    Victory,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerQuestionMarkOutcome {
    Event,
    Monster,
    Shop,
    Treasure,
    Elite,
}

impl From<crate::state::events::generator::RoomRoll> for PlannerQuestionMarkOutcome {
    fn from(outcome: crate::state::events::generator::RoomRoll) -> Self {
        match outcome {
            crate::state::events::generator::RoomRoll::Event => Self::Event,
            crate::state::events::generator::RoomRoll::Monster => Self::Monster,
            crate::state::events::generator::RoomRoll::Shop => Self::Shop,
            crate::state::events::generator::RoomRoll::Treasure => Self::Treasure,
            crate::state::events::generator::RoomRoll::Elite => Self::Elite,
        }
    }
}

impl From<RoomType> for PlannerMapRoom {
    fn from(room: RoomType) -> Self {
        match room {
            RoomType::EventRoom => Self::QuestionMark,
            RoomType::MonsterRoom => Self::Monster,
            RoomType::MonsterRoomElite => Self::Elite,
            RoomType::MonsterRoomBoss => Self::Boss,
            RoomType::RestRoom => Self::Rest,
            RoomType::ShopRoom => Self::Shop,
            RoomType::TreasureRoom => Self::Treasure,
            RoomType::TrueVictoryRoom => Self::Victory,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerPublicMapNode {
    pub x: i32,
    pub y: i32,
    pub room: PlannerMapRoom,
    pub has_emerald_key: bool,
    pub edges: Vec<PlannerPublicMapEdge>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerPublicMapEdge {
    pub destination_x: i32,
    pub destination_y: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlannerDecisionContext {
    Map {
        overlay: bool,
    },
    CardReward {
        reward_item_index: Option<usize>,
        /// The same public collection visible before opening this choice.
        /// Indices are local to the current collection, not persistent ids.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pending_rewards: Vec<PlannerRewardDescriptor>,
    },
    Event {
        event: EventId,
        screen: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        face_trader_result: Option<FaceTraderResult>,
    },
    Shop {
        purge_cost: i32,
        purge_available: bool,
        #[serde(default, skip_serializing_if = "PlannerShopInventoryV1::is_empty")]
        inventory: PlannerShopInventoryV1,
    },
    Reward {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pending_rewards: Vec<PlannerRewardDescriptor>,
    },
    Campfire,
    BossRelic,
    RunChoice {
        scope: SelectionScope,
        reason: SelectionReason,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<DomainEventSource>,
        min_choices: usize,
        max_choices: usize,
    },
    Treasure,
    RunTransition {
        stage: PlannerRunTransitionV1,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerPublicHistory {
    pub shop_purge_count: i32,
    /// Public summary of the most recently completed combat, when the run
    /// controller observed that boundary.  This is history, not hidden RNG.
    pub recent_combat_hp_loss: Option<i32>,
    pub recent_combat_turns: Option<u32>,
    pub recent_combat_potions_used: Option<u32>,
}

/// Actor-visible card domain attached to one event effect. This preserves the
/// public acquisition pool instead of collapsing every card outcome to a
/// count. It is evidence only and is never parsed back into an executable
/// event choice.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlannerEventCardPoolV1 {
    Unknown,
    Specific { card: CardId, upgrades: u8 },
    RandomClassCard,
    RandomClassCommonOrUncommon,
    RandomClassRare,
    RandomColorless,
    RandomColorlessUncommon,
    RandomColorlessRare,
    RandomCurse,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlannerEventRelicPoolV1 {
    Unknown,
    Specific { relic: RelicId },
    RandomCommonRelic,
    RandomUncommonRelic,
    RandomRareRelic,
    RandomShopRelic,
    RandomBossRelic,
    RandomRelic,
    RandomBook,
    RandomFace,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerEventRandomOutcomeV1 {
    GremlinWheel,
    JoustOwnerWin,
    DeadAdventurerSearch,
    MatchAndKeepFlip,
    ScrapOozeRelicRoll,
    MausoleumCurseRoll,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum PlannerEventPublicEffectV1 {
    GainGold {
        amount: i32,
    },
    GainGoldRange {
        min: i32,
        max: i32,
    },
    LoseGold {
        amount: i32,
    },
    LoseHp {
        amount: i32,
    },
    LoseMaxHp {
        amount: i32,
    },
    Heal {
        amount: i32,
    },
    GainMaxHp {
        amount: i32,
    },
    ObtainRelic {
        count: usize,
        pool: PlannerEventRelicPoolV1,
    },
    ObtainRandomPotions {
        count: usize,
    },
    ObtainCard {
        count: usize,
        pool: PlannerEventCardPoolV1,
    },
    ObtainColorlessCard {
        count: usize,
        pool: PlannerEventCardPoolV1,
    },
    OfferCards {
        count: usize,
        pool: PlannerEventCardPoolV1,
    },
    ObtainCurse {
        count: usize,
        pool: PlannerEventCardPoolV1,
    },
    RemoveCard {
        count: usize,
        target: Option<PlannerCardObservation>,
        pool: PlannerEventCardPoolV1,
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
        outcome: PlannerEventRandomOutcomeV1,
        chance_percent: Option<u8>,
    },
    StartCombat,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerEventSelectionV1 {
    None,
    RemoveCard,
    UpgradeCard,
    TransformCard,
    DuplicateCard,
    OfferCard,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlannerEventTransitionV1 {
    None,
    AdvanceScreen,
    Complete,
    OpenSelection { selection: PlannerEventSelectionV1 },
    OpenReward,
    StartCombat,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerEventOptionStructuredSemanticsV1 {
    pub effects: Vec<PlannerEventPublicEffectV1>,
    pub transition: PlannerEventTransitionV1,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerEventOptionPublicSemanticsV1 {
    /// Typed facts visible in the button, including already settled results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_detail: Option<crate::state::events::EventOptionPublicDetail>,
    /// Exact public button text observed when this candidate was captured.
    /// It is display evidence only: execution and candidate resolution must
    /// continue to use typed event/screen/option identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_label: Option<String>,
    /// Complete typed public mechanics captured from the event owner. Legacy
    /// archives that predate this field remain explicitly incomplete rather
    /// than reconstructing mechanics from prose or observed outcomes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_semantics: Option<PlannerEventOptionStructuredSemanticsV1>,
    pub gold_delta_min: i32,
    pub gold_delta_max: i32,
    pub hp_delta: i32,
    pub max_hp_delta: i32,
    pub relics_gained: usize,
    pub relics_lost: usize,
    pub potions_gained: usize,
    pub cards_gained: usize,
    /// Publicly identified cards gained by this exact option. Events such as
    /// The Library reveal the concrete card before the player chooses; the
    /// candidate surface must not collapse those choices to a count.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub specific_cards_gained: Vec<PlannerOfferedCard>,
    pub cards_offered: usize,
    pub curses_gained: usize,
    pub cards_removed: usize,
    /// Publicly identified card instances removed by this option.  Event
    /// buttons such as Falling reveal the exact card before the player
    /// chooses; preserving that identity prevents distinct legal choices
    /// from collapsing into the same count-only semantics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub specific_cards_removed: Vec<PlannerCardObservation>,
    pub cards_upgraded: usize,
    pub upgrades_all_cards: bool,
    pub cards_transformed: usize,
    pub cards_duplicated: usize,
    pub random_outcome: bool,
    /// Public success probability for a single named random outcome.  This is
    /// absent for deterministic choices and multi-outcome distributions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub random_outcome_chance_percent: Option<u8>,
    pub starts_combat: bool,
    pub repeatable: bool,
    pub terminal: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LegalCandidateSet {
    pub schema_name: String,
    pub schema_version: u32,
    pub candidate_set_id: String,
    pub decision_id: String,
    pub observation_id: String,
    pub site: PlannerDecisionSite,
    pub candidates: Vec<LegalCandidate>,
    pub completeness: CandidateSetCompleteness,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LegalCandidate {
    pub candidate_id: String,
    pub action: PlannerAction,
    pub mechanics: PlannerMechanicsManifest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CandidateSetCompleteness {
    Complete {
        basis: CandidateCompletenessBasis,
    },
    Incomplete {
        basis: CandidateCompletenessBasis,
        gaps: Vec<CandidateRepresentationGap>,
    },
}

impl CandidateSetCompleteness {
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete { .. })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateCompletenessBasis {
    RunControlBoundaryEnumerator,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CandidateRepresentationGap {
    ParameterizedActionFamily,
    UnsupportedBoundaryAction,
    DuplicateTypedIdentity,
    NoRepresentedLegalCandidate,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlannerAction {
    ChooseRouteNode {
        x: i32,
        y: i32,
        flight: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        question_mark_outcomes: Vec<PlannerQuestionMarkOutcome>,
    },
    ChooseEventOption {
        event: EventId,
        screen: usize,
        option_index: usize,
        action: EventActionKind,
        public_semantics: PlannerEventOptionPublicSemanticsV1,
    },
    TakeCard {
        reward_item_index: Option<usize>,
        option_index: usize,
        card: CardId,
        upgrades: u8,
    },
    OpenCardReward {
        reward_item_index: usize,
    },
    SingingBowl {
        reward_item_index: Option<usize>,
    },
    SkipCardReward {
        reward_item_index: usize,
    },
    ClaimReward {
        reward_item_index: usize,
        reward: PlannerRewardDescriptor,
    },
    Rest,
    Smith {
        card_lineage: CardLineageId,
        card: CardId,
        upgrades: u8,
    },
    Dig,
    Lift,
    Toke {
        card_lineage: CardLineageId,
        card: CardId,
        upgrades: u8,
    },
    Recall,
    BuyCard {
        shop_slot: usize,
        card: CardId,
        upgrades: u8,
        price: i32,
    },
    BuyRelic {
        shop_slot: usize,
        relic: RelicId,
        price: i32,
    },
    BuyPotion {
        shop_slot: usize,
        potion: PotionId,
        price: i32,
    },
    UseRunPotion {
        slot: usize,
        potion: PotionId,
        potion_uuid: u32,
    },
    DiscardRunPotion {
        slot: usize,
        potion: PotionId,
        potion_uuid: u32,
    },
    RemoveCard {
        card_lineage: CardLineageId,
        card: CardId,
        upgrades: u8,
        price: i32,
    },
    OpenPendingRewards,
    LeaveShop,
    TakeBossRelic {
        option_index: usize,
        relic: RelicId,
    },
    SkipBossRelic,
    BeginRunCardSelection {
        scope: SelectionScope,
        reason: SelectionReason,
        min_choices: usize,
        max_choices: usize,
        selectable_card_lineages: Vec<CardLineageId>,
    },
    SubmitRunSelection {
        scope: SelectionScope,
        selected_card_lineages: Vec<CardLineageId>,
    },
    OpenChest,
    Proceed {
        site: PlannerDecisionSite,
    },
    Cancel {
        site: PlannerDecisionSite,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlannerRewardDescriptor {
    Gold { amount: i32 },
    StolenGold { amount: i32 },
    CardReward { cards: Vec<PlannerOfferedCard> },
    Relic { relic: RelicId },
    Potion { potion: PotionId },
    EmeraldKey,
    SapphireKey,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerOfferedCard {
    pub card: CardId,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerBehaviorEvent {
    pub schema_name: String,
    pub schema_version: u32,
    pub behavior: PlannerBehaviorDecisionRecord,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerBehaviorDecisionRecord {
    pub behavior_id: String,
    pub trajectory_id: String,
    pub run_id: String,
    pub seed_group_id: String,
    pub decision_id: String,
    pub observation_id: String,
    pub legal_candidate_set_id: String,
    pub selected_candidate_id: String,
    pub behavior_policy: BehaviorPolicyManifest,
    pub selection_probability: SelectionProbability,
    pub continuation_policy: ContinuationPolicyManifest,
    pub mechanics: PlannerMechanicsManifest,
    pub provenance: BehaviorLabelProvenance,
    pub sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorPolicyManifest {
    pub source: BehaviorPolicySource,
    pub schema_version: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorPolicySource {
    RunControlCommandStream,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectionProbability {
    KnownDeterministic,
    KnownStochastic { numerator: u64, denominator: u64 },
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationPolicyManifest {
    pub source: ContinuationPolicySource,
    pub config_fingerprint: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationPolicySource {
    ObservedRunControl,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorLabelProvenance {
    pub role: PlannerLabelRole,
    pub source: BehaviorObservationSource,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerLabelRole {
    BehaviorPolicyNotTeacher,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorObservationSource {
    LiveCommand,
    ReplayVerified,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerOutcomeAttachment {
    pub attachment_id: String,
    pub behavior_id: String,
    pub decision_id: String,
    pub label_kind: PlannerOutcomeLabelKind,
    pub horizon: PlannerOutcomeHorizon,
    pub before: PlannerOutcomeSnapshot,
    pub after: PlannerOutcomeSnapshot,
    pub continuation_policy: ContinuationPolicyManifest,
    pub mechanics: PlannerMechanicsManifest,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerOutcomeLabelKind {
    RealizedBehaviorRun,
    ExactScenarioReplay,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerOutcomeHorizon {
    AfterOneFloor,
    RunTerminal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannerOutcomeSnapshot {
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub deck_size: usize,
    pub relic_count: usize,
    pub potion_count: usize,
    pub terminal: Option<PlannerTerminalKind>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannerTerminalKind {
    Victory,
    Defeat,
}
