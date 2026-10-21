use crate::ai::analysis::card_semantics::{
    card_definition, CombatEvent, Mechanic, TriggeredEffect,
};
use crate::ai::combat_upgrade_coverage_v1::CombatUpgradeScopeV1;
use crate::content::cards::{CardId, CardRarity, CardTag, CardType};
use crate::content::potions::{
    get_potion_definition, potion_mechanic_roles, PotionClass, PotionId, PotionMechanicRole,
    PotionRarity,
};
use crate::content::relics::RelicId;
use crate::state::rewards::RewardCard;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdditionalPlayScopeV1 {
    Attack,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ActionSupplyTraitsV1 {
    pub opening_once_options: u8,
    pub delayed_per_turn: bool,
    pub same_turn_burst_min_follow_ups: u8,
    pub triggered_repeatable: bool,
    pub additional_play_scope: Option<AdditionalPlayScopeV1>,
    pub cost_or_resource_compression: bool,
    pub potentially_recursive: bool,
    pub immediate_hand: bool,
    pub zero_cost_this_turn: bool,
    pub optional_supply: bool,
}

impl ActionSupplyTraitsV1 {
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    pub fn same_turn_burst(self) -> bool {
        self.same_turn_burst_min_follow_ups > 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrengthConversionMechanicV1 {
    AmplifyCurrentStrength,
    PreventStrengthDownDebuff,
    ClearStrengthDownDebuff,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatExternalPayoffV1 {
    PersistentOrReward,
    HealingIfDamaged,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CardMechanicsProfileV1 {
    pub action_supply: ActionSupplyTraitsV1,
    pub recurring_card_draw: bool,
    pub persistent_strength_source: bool,
    pub temporary_strength_burst: bool,
    pub applies_strength_down_debuff: bool,
    pub applies_no_draw_debuff: bool,
    pub reshuffle_discard_into_draw: bool,
    pub discard_pile_topdeck_access: bool,
    pub hand_topdeck_selection: bool,
    pub strength_converter: Option<StrengthConversionMechanicV1>,
    pub strength_payoff: bool,
    pub self_damage_source: bool,
    pub combat_external_payoff: Option<CombatExternalPayoffV1>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RelicMechanicsProfileV1 {
    pub action_supply: ActionSupplyTraitsV1,
    pub persistent_strength_source: bool,
    pub temporary_strength_burst: bool,
    pub strength_converter: Option<StrengthConversionMechanicV1>,
    pub removes_all_player_debuffs: bool,
    pub core_defense_or_survival: bool,
    pub core_card_access: bool,
    /// Multiplies potion effects but supplies no combat value until a potion
    /// is actually owned. Keep this separate from direct survival so policy
    /// owners cannot treat empty potion slots as already-realized defense.
    pub potion_effect_multiplier: bool,
    pub reactive_damage_per_attack_hit: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PotionMechanicsProfileV1 {
    pub temporary_strength_burst: bool,
    pub strength_converter: Option<StrengthConversionMechanicV1>,
    pub combat_upgrade_scope: Option<CombatUpgradeScopeV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelicAcquisitionTraitV1 {
    ActionSupply,
    CoreDefenseOrSurvival,
    CoreCardAccess,
    PotionEffectMultiplier,
    EliteFightLeverage,
    ShopEconomyMultiplier,
    StatusDigest,
    DebuffControl,
    XCostPayoff,
    ImmediateRecovery,
    DeckMutation,
    ReactiveDamage,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PotionAcquisitionTraitV1 {
    CombatDamage,
    AoeDamage,
    CombatBlock,
    VulnerableSetup,
    WeakControl,
    EnergyBurst,
    StrengthGain,
    CardAccess,
    CardDiscovery,
    ExhaustControl,
    ActionAmplifier,
    CombatUpgrade,
    DeathInsurance,
    DebuffControl,
    EscapeTool,
    PermanentMaxHp,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionRequirementV1 {
    XCostPayoff,
    DuplicateTarget,
    AttackSkillPowerSameTurn,
    LowHpDeathInsurance,
    RouteEscapeValue,
    UpgradeableCombatCards,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardPlayTargetV1 {
    OneEnemy,
    AllEnemies,
    AllCharacters,
    PlayerAndOneEnemy,
    Player,
    #[default]
    NoExplicitTarget,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CardRewardFactsV1 {
    pub card: CardId,
    #[serde(default)]
    pub upgrades: u8,
    pub name: String,
    pub card_type: CardType,
    /// Native membership used by effects such as Perfected Strike. Older
    /// observations without this field leave membership unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<CardTag>>,
    pub rarity: CardRarity,
    pub cost: i8,
    /// Runtime-owned targeting surface for playing the card. Status facts
    /// below remain explicit about player-only effects so an external model
    /// never has to guess their direction from prose.
    #[serde(default)]
    pub play_target: CardPlayTargetV1,
    pub damage: CardRewardDamageFactsV1,
    /// Base definition field, which may be conditional, repeated, dynamically
    /// replaced, or an installed power's parameter. Not a flat Block grant.
    pub block: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_block_delivery: Option<crate::content::cards::mechanics::CardBaseBlockDeliveryV1>,
    /// Unconditional persistent Block gained at the end of each player turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_of_turn_block_each_turn:
        Option<crate::content::cards::mechanics::CardEndOfTurnBlockEachTurnV1>,
    /// Reviewed unconditional draw request after a successful play, before
    /// No Draw, hand limits, or follow-up discards/top-deck selections.
    /// Zero does not rule out conditional, trigger-based, or unreviewed draw.
    pub draw_cards: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaust_draw_power: Option<i32>,
    /// Block per Exhaust event after this Power is deployed, not on-play Block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaust_block_power: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand_topdeck_after_draw: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_creation_on_play:
        Option<crate::content::cards::mechanics::CardStatusCreationOnPlayV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_growth_on_play: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_change_on_hp_loss: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_vulnerable_follow_up:
        Option<crate::content::cards::mechanics::CardTargetVulnerableFollowUpV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand_copy: Option<crate::content::cards::mechanics::CardHandCopyV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaust_pile_to_hand: Option<crate::content::cards::mechanics::CardExhaustPileToHandV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_deck_play: Option<crate::content::cards::mechanics::CardTopDeckPlayV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enemy_debuffs_in_monster_order:
        Option<Vec<crate::content::cards::mechanics::CardEnemyDebuffRequestV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hp_loss_strength_power: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw_pile_skill_to_hand: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_to_discard_after_damage: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub random_attack_to_hand: Option<crate::content::cards::mechanics::CardRandomAttackToHandV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_of_turn_hp_loss_and_damage:
        Option<crate::content::cards::mechanics::CardEndOfTurnHpLossAndDamageV1>,
    #[serde(default)]
    pub draw_cards_each_turn: i32,
    #[serde(default)]
    pub discard_cards_each_turn: i32,
    #[serde(default)]
    pub hp_loss_each_turn: i32,
    /// Direct HP loss paid on play, including its prevention and trigger
    /// semantics. Absent means the card has no reviewed on-play HP payment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_hp_loss_on_play: Option<crate::content::cards::mechanics::CardSelfHpLossOnPlayV1>,
    /// Unconditional on-play request only; conditional/scheduled gain is below.
    pub energy_gain: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_gain_rule: Option<crate::content::cards::mechanics::CardEnergyGainV1>,
    /// Energy granted at the start of each future turn by this card's power.
    #[serde(default)]
    pub energy_gain_each_turn: i32,
    /// Energy produced only when this card itself is exhausted.
    #[serde(default)]
    pub energy_gain_on_exhaust: i32,
    /// Vulnerable applied to enemies.
    pub vulnerable: i32,
    /// Vulnerable applied to the player rather than an enemy.
    #[serde(default)]
    pub player_vulnerable: i32,
    pub weak: i32,
    pub strength_gain: i32,
    /// The scalar alone does not imply immediate or unconditional Strength.
    /// Missing in old facts or outside reviewed coverage means Unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength_gain_delivery:
        Option<crate::content::cards::mechanics::CardPlayerStrengthDeliveryV1>,
    /// Portion of `strength_gain` that is scheduled to be lost at end of turn.
    #[serde(default)]
    pub temporary_strength_gain: i32,
    #[serde(default)]
    pub strength_loss_at_end_of_turn: i32,
    /// Requested reduction per affected enemy, before Artifact or other modifiers.
    /// This magnitude alone does not imply persistent mitigation.
    pub enemy_strength_down: i32,
    /// Missing in older facts and for unreviewed mechanisms: unknown, never
    /// implicitly permanent. The card-owned schedule is not a target outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enemy_strength_restoration:
        Option<crate::content::cards::mechanics::EnemyStrengthRestorationV1>,
    /// Max-HP and current-HP gains awarded only when this attack makes an
    /// eligible enemy fatal. They are separate because max HP is the durable
    /// reward while the current HP increase is immediate sustain.
    #[serde(default)]
    pub max_hp_gain_on_fatal: i32,
    #[serde(default)]
    pub current_hp_gain_on_fatal: i32,
    #[serde(default)]
    pub intangible_gain: i32,
    #[serde(default)]
    pub intangible_damage_cap: i32,
    #[serde(default)]
    pub intangible_caps_direct_hp_loss_to_one: bool,
    pub exhausts: bool,
    #[serde(default)]
    pub ethereal: bool,
    #[serde(default)]
    pub innate: bool,
    pub exhausts_other_cards: bool,
    #[serde(default)]
    pub selects_from_hand: bool,
    pub adds_status_cards: i32,
    #[serde(default)]
    pub status_injections: Vec<CardRewardStatusInjectionV1>,
    pub upgrades_cards: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_upgrade_scope: Option<crate::ai::combat_upgrade_coverage_v1::CombatUpgradeScopeV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_upgrade_persistence:
        Option<crate::ai::combat_upgrade_coverage_v1::CombatUpgradePersistenceV1>,
    pub is_random_output: bool,
    pub has_conditional_playability: bool,
    pub is_aoe: bool,
    pub pick_dependencies: Vec<CardRewardPickDependencyV1>,
}

impl CardRewardFactsV1 {
    /// Reviewed once-on-play base request for a fresh obtainable card. Combat
    /// instance changes and Block prevention still need runtime evaluation.
    pub fn unconditional_base_block_request_v1(&self) -> Option<i32> {
        self.base_block_delivery
            .filter(|delivery| delivery.has_unconditional_base_request())
            .map(|_| self.block)
    }

    pub fn context_dependent_block_delivery_v1(
        &self,
    ) -> Option<crate::content::cards::mechanics::CardBaseBlockDeliveryV1> {
        self.base_block_delivery
            .filter(|delivery| !delivery.has_unconditional_base_request())
    }

    pub fn has_persistent_enemy_strength_reduction_v1(&self) -> bool {
        self.enemy_strength_down > 0
            && self.enemy_strength_restoration
                == Some(crate::content::cards::mechanics::EnemyStrengthRestorationV1::NoScheduledRestoration)
    }
}

/// Typed, strategy-neutral mechanics captured for one obtainable card and its
/// next legal upgrade state.
///
/// Acquisition surfaces use this instead of asking a policy or model to
/// reconstruct upgrade text. `upgraded` follows the simulator's upgrade
/// eligibility and representable upgrade count, including repeated Searing
/// Blow upgrades; it is absent for ineligible cards.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardAcquisitionMechanicsFactsV1 {
    pub current: CardRewardFactsV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgraded: Option<CardRewardFactsV1>,
}

/// Typed, strategy-neutral mechanics for one obtainable relic.
///
/// This deliberately excludes policy traits, scores, and purchase advice.
/// `requirements` describes mechanical preconditions only, while the boolean
/// effect keeps a negative fact visible for relics that do not clear debuffs.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicAcquisitionMechanicsFactsV1 {
    pub relic: RelicId,
    pub requirements: Vec<AcquisitionRequirementV1>,
    pub removes_all_player_debuffs: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_per_turn_delta: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_play_limit_per_turn: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starter_card_replacement:
        Option<crate::content::relics::pandoras_box::StarterCardReplacementRuleV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gold_gain_rule: Option<crate::content::relics::mechanics::RelicGoldGainRuleV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_cards_drawn_each_turn: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawn_card_cost_randomization: Option<DrawnCardCostRandomizationFactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potion_effect_rule: Option<RelicPotionEffectRuleFactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potion_acquisition_rule: Option<RelicPotionAcquisitionRuleFactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest_site_entry_healing: Option<RelicRestSiteEntryHealingFactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_victory_healing: Option<RelicCombatVictoryHealingFactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_draw_on_hp_loss: Option<RelicCardDrawOnHpLossFactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battle_start_power: Option<crate::content::relics::mechanics::RelicBattleStartPowerV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shuffle_energy: Option<crate::content::relics::mechanics::RelicShuffleEnergyV1>,
}

pub use crate::content::powers::core::confusion::{
    DrawnCardCostEligibilityV1, DrawnCardCostRandomizationFactsV1, DrawnCardTurnCostUpdateV1,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicPotionEffectRuleFactsV1 {
    pub potency_multiplier: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicPotionAcquisitionRuleFactsV1 {
    pub prevents_new_potions: bool,
    pub owned_potions_remain_usable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicRestSiteEntryHealingFactsV1 {
    pub deck_cards_per_trigger: usize,
    pub hp_per_trigger: i32,
    pub prevented_when_player_cannot_heal: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicCombatVictoryHealingFactsV1 {
    pub base_hp: i32,
    pub requires_player_alive: bool,
    pub capped_at_max_hp: bool,
    pub affected_by_healing_modifiers: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicCardDrawOnHpLossFactsV1 {
    pub minimum_hp_lost_per_event: i32,
    pub cards_drawn_per_event: u32,
}

/// Typed, strategy-neutral mechanics for one obtainable or currently held
/// potion.  `base_potency` is the amount consumed by the typed potion effect;
/// candidate relic multipliers remain separate relic facts.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionAcquisitionMechanicsFactsV1 {
    pub potion: PotionId,
    pub name: String,
    pub rarity: PotionRarity,
    pub class: PotionClass,
    pub base_potency: i32,
    pub target_required: bool,
    pub is_thrown: bool,
    pub mechanic_roles: Vec<PotionMechanicRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_discovery: Option<PotionCardDiscoveryFactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_use: Option<crate::content::potions::mechanics::ReviewedPotionUseV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PotionDiscoveryPoolV1 {
    Attack,
    Skill,
    Power,
    Colorless,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionCardDiscoveryFactsV1 {
    pub pool: PotionDiscoveryPoolV1,
    pub offered_choices: u8,
    pub base_selected_card_copies: u8,
    pub selected_card_cost_this_turn: u8,
    pub can_skip: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardRewardStatusDestinationV1 {
    DrawPile,
    Hand,
    DiscardPile,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardRewardStatusPersistenceV1 {
    Persistent,
    Ethereal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardRewardStatusInjectionV1 {
    pub card: CardId,
    pub count: i32,
    pub destination: CardRewardStatusDestinationV1,
    pub persistence: CardRewardStatusPersistenceV1,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CardRewardDamageFactsV1 {
    /// Definition field only; e.g. Wish uses this as a Strength parameter.
    pub damage_per_hit: i32,
    /// Legacy numeric projection of the unconditional base contribution.
    /// Zero includes unknown/variable cases. Consumers must use the reviewed
    /// delivery methods below, not infer no damage from this number.
    pub hit_count: i32,
    /// Base contribution, not complete damage or a sum across all enemies.
    /// For random hits this is an aggregate, never focused target damage.
    pub total_damage: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<crate::content::cards::mechanics::CardBaseDamageDeliveryV1>,
}

impl CardRewardDamageFactsV1 {
    pub fn from_base_value(card: CardId, upgrades: u8, damage_per_hit: i32) -> Self {
        let delivery =
            crate::content::cards::mechanics::card_base_damage_delivery_v1(card, upgrades);
        let hit_count = delivery
            .and_then(|rule| rule.unconditional_base_hits())
            .unwrap_or(0);
        let damage_per_hit = damage_per_hit.max(0);
        Self {
            damage_per_hit,
            hit_count,
            total_damage: damage_per_hit.saturating_mul(hit_count),
            delivery,
        }
    }
    pub fn has_reviewed_attack(&self) -> bool {
        self.delivery.is_some_and(|rule| rule.is_attack())
    }

    /// Before powers, instance mutation, prevention or target death. Excludes
    /// conditional extra hits and state-derived additions to the base value.
    pub fn unconditional_base_total(&self) -> Option<i32> {
        self.delivery
            .and_then(|rule| rule.unconditional_base_hits())
            .map(|hits| self.damage_per_hit.saturating_mul(hits))
    }

    /// Damage available on a selected enemy (including that enemy's share of
    /// an all-enemy attack). Random split damage cannot certify focus fire.
    pub fn unconditional_targeted_base_total(&self) -> Option<i32> {
        use crate::content::cards::mechanics::CardAttackTargetsV1;
        if self.delivery.and_then(|rule| rule.targets())
            == Some(CardAttackTargetsV1::RandomEnemyPerHit)
        {
            None
        } else {
            self.unconditional_base_total()
        }
    }
}

/// Factual access leverage supplied by one card play.
///
/// This deliberately does not claim that an access card is always desirable.
/// Owners still account for copies, liabilities, current gaps, and price.  The
/// distinction exists because a zero-energy multi-card burst exposes several
/// already-owned answers at once, while a one-card cantrip is only incremental
/// access; counting both as one generic `draw_source` erases that difference.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAccessLeverageV1 {
    Incremental,
    EfficientBurst,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CardAccessEvidenceV1 {
    pub leverage: CardAccessLeverageV1,
    pub draw_cards: i32,
    pub energy_cost: i8,
    pub applies_no_draw_debuff: bool,
}

pub fn card_access_evidence_v1(card: &RewardCard) -> Option<CardAccessEvidenceV1> {
    let facts = card_reward_facts_v1(card);
    if facts.draw_cards <= 0 {
        return None;
    }
    let leverage = if facts.draw_cards >= 3 && facts.cost <= 0 {
        CardAccessLeverageV1::EfficientBurst
    } else {
        CardAccessLeverageV1::Incremental
    };
    Some(CardAccessEvidenceV1 {
        leverage,
        draw_cards: facts.draw_cards,
        energy_cost: facts.cost,
        applies_no_draw_debuff: card_mechanics_profile_v1(card.id).applies_no_draw_debuff,
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CardRewardPickDependencyV1 {
    RouteUpgradeDensity,
    StrengthScaling,
    BlockDensity,
    StrikeDensity,
    ExhaustPackage,
    StatusPackage,
    SelfDamagePackage,
    RandomOutputPolicy,
    ConditionalPlayabilityPolicy,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CardRewardSemanticProfileV1 {
    pub card: CardId,
    pub name: String,
    pub roles: Vec<CardRewardSemanticRoleV1>,
    #[serde(default)]
    pub combat_upgrade_scope: Option<CombatUpgradeScopeV1>,
    pub dependencies: Vec<CardRewardPickDependencyV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum CardRewardSemanticRoleV1 {
    FrontloadDamage,
    /// Reviewed attack whose amount, repetitions, or legal play need context.
    ContextDependentDamage,
    AoeDamage,
    Block,
    BlockRetention,
    BlockMultiplier,
    CardDraw,
    CycleAccess,
    DiscardPileTopdeckAccess,
    HandTopdeckSelection,
    CardInstanceDamageGrowth,
    HpLossCostReduction,
    ConditionalCardAccess,
    HandCopy,
    TopDeckPlay,
    EnergySource,
    EnergyOnExhaust,
    Vulnerable,
    Weak,
    EnemyStrengthDown,
    ScalingSource,
    TemporaryStrengthBurst,
    StrengthPayoff,
    BlockPayoff,
    StrikePayoff,
    UpgradePayoff,
    ExhaustGenerator,
    ExhaustReuse,
    ExhaustPayoff,
    StatusGenerator,
    StatusPayoff,
    SelfDamagePayoff,
    CombatExternalPayoff,
    CombatSustain,
    ActionAmplifier,
    PackagePayoff,
    RandomOutput,
    ConditionalPlayability,
}

#[path = "card_semantics_v1/owned_output.rs"]
mod owned_output;
#[path = "card_semantics_v1/owned_reading.rs"]
mod owned_reading;
#[path = "card_semantics_v1/potion_reading.rs"]
mod potion_reading;
#[path = "card_semantics_v1/relic_reading.rs"]
mod relic_reading;
#[path = "card_semantics_v1/reward_facts.rs"]
mod reward_facts;
#[path = "card_semantics_v1/reward_profile.rs"]
mod reward_profile;
#[path = "card_semantics_v1/reward_reading.rs"]
mod reward_reading;

pub use owned_output::{
    card_owned_base_output_v1, card_owned_upgrade_output_v1, CardOwnedBaseOutputV1,
    CardOwnedUpgradeOutputV1,
};
pub use owned_reading::{card_owned_reading_v1, CardOwnedReadingV1, CardOwnedStateReadingV1};
pub use potion_reading::{potion_acquisition_reading_v1, PotionAcquisitionReadingV1};
pub use relic_reading::{relic_acquisition_reading_v1, RelicAcquisitionReadingV1};
pub use reward_facts::{card_acquisition_mechanics_facts_v1, card_reward_facts_v1};
pub use reward_profile::card_reward_semantic_profile_v1;
pub use reward_reading::{card_acquisition_reading_v1, CardAcquisitionReadingV1};

pub fn relic_acquisition_mechanics_facts_v1(relic: RelicId) -> RelicAcquisitionMechanicsFactsV1 {
    let mechanics = relic_mechanics_profile_v1(relic);
    RelicAcquisitionMechanicsFactsV1 {
        relic,
        requirements: relic_acquisition_requirements_v1(relic),
        battle_start_power: crate::content::relics::mechanics::relic_battle_start_power_v1(relic),
        shuffle_energy: crate::content::relics::mechanics::relic_shuffle_energy_v1(relic),
        removes_all_player_debuffs: mechanics.removes_all_player_debuffs,
        energy_per_turn_delta: match crate::content::relics::energy_master_delta(relic) {
            0 => None,
            delta => Some(delta),
        },
        card_play_limit_per_turn: (relic == RelicId::VelvetChoker)
            .then_some(crate::content::relics::velvet_choker::CARD_PLAY_LIMIT),
        starter_card_replacement: (relic == RelicId::PandorasBox)
            .then(crate::content::relics::pandoras_box::replacement_rule_v1),
        gold_gain_rule: crate::content::relics::mechanics::relic_gold_gain_rule_v1(relic),
        additional_cards_drawn_each_turn: (relic == RelicId::SneckoEye)
            .then_some(crate::content::relics::snecko_eye::ADDITIONAL_CARDS_DRAWN_EACH_TURN),
        drawn_card_cost_randomization: (relic == RelicId::SneckoEye)
            .then(crate::content::powers::core::confusion::cost_randomization_rule_v1),
        potion_effect_rule: (relic == RelicId::SacredBark).then_some(
            RelicPotionEffectRuleFactsV1 {
                potency_multiplier: crate::content::relics::sacred_bark::get_potion_multiplier(),
            },
        ),
        potion_acquisition_rule: (relic == RelicId::Sozu).then_some(
            RelicPotionAcquisitionRuleFactsV1 {
                prevents_new_potions: true,
                owned_potions_remain_usable: true,
            },
        ),
        rest_site_entry_healing: (relic == RelicId::EternalFeather).then_some(
            RelicRestSiteEntryHealingFactsV1 {
                deck_cards_per_trigger:
                    crate::content::relics::eternal_feather::DECK_CARDS_PER_HEAL_TRIGGER,
                hp_per_trigger: crate::content::relics::eternal_feather::HP_PER_HEAL_TRIGGER,
                prevented_when_player_cannot_heal: true,
            },
        ),
        combat_victory_healing: match relic {
            RelicId::BurningBlood => Some(RelicCombatVictoryHealingFactsV1 {
                base_hp: crate::content::relics::burning_blood::VICTORY_HEAL_AMOUNT,
                requires_player_alive: true,
                capped_at_max_hp: true,
                affected_by_healing_modifiers: true,
            }),
            RelicId::BlackBlood => Some(RelicCombatVictoryHealingFactsV1 {
                base_hp: crate::content::relics::black_blood::VICTORY_HEAL_AMOUNT,
                requires_player_alive: true,
                capped_at_max_hp: true,
                affected_by_healing_modifiers: true,
            }),
            _ => None,
        },
        card_draw_on_hp_loss: (relic == RelicId::RunicCube).then_some(
            RelicCardDrawOnHpLossFactsV1 {
                minimum_hp_lost_per_event:
                    crate::content::relics::runic_cube::MINIMUM_HP_LOST_PER_EVENT,
                cards_drawn_per_event:
                    crate::content::relics::runic_cube::CARDS_DRAWN_PER_HP_LOSS_EVENT,
            },
        ),
    }
}

pub fn potion_acquisition_mechanics_facts_v1(
    potion: PotionId,
) -> PotionAcquisitionMechanicsFactsV1 {
    let definition = get_potion_definition(potion);
    let discovery_spec = crate::content::potions::potion_effects::card_discovery_spec(potion);
    PotionAcquisitionMechanicsFactsV1 {
        potion,
        name: definition.name.to_string(),
        rarity: definition.rarity,
        class: definition.class,
        base_potency: definition.base_potency,
        target_required: definition.target_required,
        is_thrown: definition.is_thrown,
        mechanic_roles: potion_mechanic_roles(potion).to_vec(),
        reviewed_use: crate::content::potions::mechanics::reviewed_potion_use_v1(
            potion,
            definition.base_potency,
        ),
        card_discovery: discovery_spec.map(|spec| PotionCardDiscoveryFactsV1 {
            pool: if spec.colorless {
                PotionDiscoveryPoolV1::Colorless
            } else {
                match spec.card_type {
                    Some(CardType::Attack) => PotionDiscoveryPoolV1::Attack,
                    Some(CardType::Skill) => PotionDiscoveryPoolV1::Skill,
                    Some(CardType::Power) => PotionDiscoveryPoolV1::Power,
                    _ => unreachable!("typed discovery potion must have a supported card pool"),
                }
            },
            offered_choices: crate::runtime::action::DISCOVERY_CARD_CHOICE_COUNT,
            base_selected_card_copies: definition.base_potency.max(0) as u8,
            selected_card_cost_this_turn: spec
                .cost_for_turn
                .expect("discovery potion must set the selected card turn cost"),
            can_skip: spec.can_skip,
        }),
    }
}

pub fn card_mechanics_profile_v1(card: CardId) -> CardMechanicsProfileV1 {
    let recurring_card_draw = card_definition(card).event_handlers.iter().any(|handler| {
        matches!(handler.on, CombatEvent::TurnStart | CombatEvent::TurnEnd)
            && handler.effect == TriggeredEffect::Provide(Mechanic::CardDraw)
    });
    CardMechanicsProfileV1 {
        action_supply: card_action_supply_traits_v1(card),
        recurring_card_draw,
        persistent_strength_source: matches!(
            card,
            CardId::Inflame | CardId::SpotWeakness | CardId::DemonForm | CardId::JAX
        ),
        temporary_strength_burst: matches!(card, CardId::Flex),
        applies_strength_down_debuff: matches!(card, CardId::Flex),
        applies_no_draw_debuff: matches!(card, CardId::BattleTrance | CardId::BulletTime),
        reshuffle_discard_into_draw: matches!(card, CardId::DeepBreath),
        discard_pile_topdeck_access: matches!(card, CardId::Headbutt),
        hand_topdeck_selection: crate::content::cards::mechanics::card_hand_topdeck_after_draw_v1(
            card,
        )
        .is_some(),
        strength_converter: match card {
            CardId::LimitBreak => Some(StrengthConversionMechanicV1::AmplifyCurrentStrength),
            CardId::Panacea | CardId::CoreSurge => {
                Some(StrengthConversionMechanicV1::PreventStrengthDownDebuff)
            }
            _ => None,
        },
        strength_payoff: matches!(
            card,
            CardId::HeavyBlade
                | CardId::SwordBoomerang
                | CardId::Pummel
                | CardId::LimitBreak
                | CardId::Reaper
                | CardId::Whirlwind
        ),
        self_damage_source: matches!(
            card,
            CardId::Bloodletting
                | CardId::Offering
                | CardId::Hemokinesis
                | CardId::Combust
                | CardId::Brutality
                | CardId::JAX
        ),
        combat_external_payoff: match card {
            CardId::Feed
            | CardId::LessonLearned
            | CardId::HandOfGreed
            | CardId::RitualDagger
            | CardId::Alchemize
            | CardId::GeneticAlgorithm
            | CardId::Wish => Some(CombatExternalPayoffV1::PersistentOrReward),
            CardId::BandageUp | CardId::Bite | CardId::Reaper | CardId::SelfRepair => {
                Some(CombatExternalPayoffV1::HealingIfDamaged)
            }
            _ => None,
        },
    }
}

pub fn relic_mechanics_profile_v1(relic: RelicId) -> RelicMechanicsProfileV1 {
    RelicMechanicsProfileV1 {
        action_supply: relic_action_supply_traits_v1(relic),
        persistent_strength_source: matches!(relic, RelicId::Vajra),
        temporary_strength_burst: matches!(relic, RelicId::MutagenicStrength),
        strength_converter: match relic {
            RelicId::ClockworkSouvenir => {
                Some(StrengthConversionMechanicV1::PreventStrengthDownDebuff)
            }
            RelicId::OrangePellets => Some(StrengthConversionMechanicV1::ClearStrengthDownDebuff),
            _ => None,
        },
        removes_all_player_debuffs: relic == RelicId::OrangePellets,
        core_defense_or_survival: matches!(
            relic,
            RelicId::Anchor
                | RelicId::BurningBlood
                | RelicId::BlackBlood
                | RelicId::BloodVial
                | RelicId::MagicFlower
                | RelicId::MeatOnTheBone
                | RelicId::Pantograph
                | RelicId::FossilizedHelix
                | RelicId::IncenseBurner
                | RelicId::ThreadAndNeedle
                | RelicId::Torii
                | RelicId::TungstenRod
                | RelicId::Calipers
                | RelicId::Orichalcum
                | RelicId::HornCleat
                | RelicId::CaptainsWheel
                | RelicId::LizardTail
        ),
        core_card_access: matches!(
            relic,
            RelicId::RunicPyramid
                | RelicId::SneckoEye
                | RelicId::GamblingChip
                | RelicId::FrozenEye
                | RelicId::BagOfPreparation
                | RelicId::Pocketwatch
                | RelicId::QuestionCard
                | RelicId::InkBottle
                | RelicId::RunicCube
                | RelicId::Sundial
                | RelicId::IceCream
        ),
        potion_effect_multiplier: relic == RelicId::SacredBark,
        reactive_damage_per_attack_hit: match relic {
            RelicId::BronzeScales => crate::content::relics::bronze_scales::THORNS_AMOUNT as u8,
            _ => 0,
        },
    }
}

fn card_action_supply_traits_v1(card: CardId) -> ActionSupplyTraitsV1 {
    match card {
        CardId::BladeDance => ActionSupplyTraitsV1 {
            same_turn_burst_min_follow_ups: 3,
            immediate_hand: true,
            ..Default::default()
        },
        CardId::DoubleTap => ActionSupplyTraitsV1 {
            additional_play_scope: Some(AdditionalPlayScopeV1::Attack),
            ..Default::default()
        },
        CardId::Corruption | CardId::Offering => ActionSupplyTraitsV1 {
            cost_or_resource_compression: true,
            ..Default::default()
        },
        _ => ActionSupplyTraitsV1::default(),
    }
}

fn relic_action_supply_traits_v1(relic: RelicId) -> ActionSupplyTraitsV1 {
    match relic {
        RelicId::Lantern => ActionSupplyTraitsV1 {
            opening_once_options: 1,
            cost_or_resource_compression: true,
            ..Default::default()
        },
        RelicId::Nunchaku => ActionSupplyTraitsV1 {
            triggered_repeatable: true,
            cost_or_resource_compression: true,
            ..Default::default()
        },
        RelicId::Enchiridion => ActionSupplyTraitsV1 {
            opening_once_options: 1,
            immediate_hand: true,
            zero_cost_this_turn: true,
            ..Default::default()
        },
        RelicId::Toolbox => ActionSupplyTraitsV1 {
            opening_once_options: 1,
            immediate_hand: true,
            ..Default::default()
        },
        RelicId::NilrysCodex => ActionSupplyTraitsV1 {
            delayed_per_turn: true,
            optional_supply: true,
            ..Default::default()
        },
        RelicId::DeadBranch => ActionSupplyTraitsV1 {
            triggered_repeatable: true,
            immediate_hand: true,
            potentially_recursive: true,
            ..Default::default()
        },
        _ => ActionSupplyTraitsV1::default(),
    }
}

pub fn potion_mechanics_profile_v1(potion: PotionId) -> PotionMechanicsProfileV1 {
    PotionMechanicsProfileV1 {
        temporary_strength_burst: matches!(potion, PotionId::SteroidPotion),
        strength_converter: match potion {
            PotionId::AncientPotion => {
                Some(StrengthConversionMechanicV1::PreventStrengthDownDebuff)
            }
            _ => None,
        },
        combat_upgrade_scope: match potion {
            PotionId::BlessingOfTheForge => Some(CombatUpgradeScopeV1::WholeHand),
            _ => None,
        },
    }
}

pub fn relic_acquisition_traits_v1(relic: RelicId) -> Vec<RelicAcquisitionTraitV1> {
    let mechanics = relic_mechanics_profile_v1(relic);
    let mut traits = Vec::new();
    if !mechanics.action_supply.is_empty() {
        push_relic_trait(&mut traits, RelicAcquisitionTraitV1::ActionSupply);
    }
    if mechanics.core_defense_or_survival {
        push_relic_trait(&mut traits, RelicAcquisitionTraitV1::CoreDefenseOrSurvival);
    }
    if mechanics.core_card_access {
        push_relic_trait(&mut traits, RelicAcquisitionTraitV1::CoreCardAccess);
    }
    if mechanics.potion_effect_multiplier {
        push_relic_trait(&mut traits, RelicAcquisitionTraitV1::PotionEffectMultiplier);
    }
    if mechanics.reactive_damage_per_attack_hit > 0 {
        push_relic_trait(&mut traits, RelicAcquisitionTraitV1::ReactiveDamage);
    }
    if mechanics.strength_converter.is_some() {
        push_relic_trait(&mut traits, RelicAcquisitionTraitV1::DebuffControl);
    }
    match relic {
        RelicId::PreservedInsect => {
            push_relic_trait(&mut traits, RelicAcquisitionTraitV1::EliteFightLeverage);
        }
        RelicId::MembershipCard | RelicId::Courier => {
            push_relic_trait(&mut traits, RelicAcquisitionTraitV1::ShopEconomyMultiplier);
        }
        RelicId::MedicalKit => {
            push_relic_trait(&mut traits, RelicAcquisitionTraitV1::StatusDigest);
        }
        RelicId::ChemicalX => {
            push_relic_trait(&mut traits, RelicAcquisitionTraitV1::XCostPayoff);
        }
        RelicId::Waffle => {
            push_relic_trait(&mut traits, RelicAcquisitionTraitV1::ImmediateRecovery);
        }
        RelicId::DollysMirror
        | RelicId::Orrery
        | RelicId::Astrolabe
        | RelicId::EmptyCage
        | RelicId::PandorasBox => {
            push_relic_trait(&mut traits, RelicAcquisitionTraitV1::DeckMutation);
        }
        _ => {}
    }
    traits
}

pub fn relic_acquisition_requirements_v1(relic: RelicId) -> Vec<AcquisitionRequirementV1> {
    match relic {
        RelicId::ChemicalX => vec![AcquisitionRequirementV1::XCostPayoff],
        RelicId::DollysMirror => vec![AcquisitionRequirementV1::DuplicateTarget],
        RelicId::OrangePellets => {
            vec![AcquisitionRequirementV1::AttackSkillPowerSameTurn]
        }
        _ => Vec::new(),
    }
}

pub fn potion_acquisition_traits_v1(potion: PotionId) -> Vec<PotionAcquisitionTraitV1> {
    let mut traits = Vec::new();
    for role in potion_mechanic_roles(potion) {
        let trait_ = match role {
            PotionMechanicRole::DirectDamage => Some(PotionAcquisitionTraitV1::CombatDamage),
            PotionMechanicRole::MultiTarget => Some(PotionAcquisitionTraitV1::AoeDamage),
            PotionMechanicRole::ApplyWeak => Some(PotionAcquisitionTraitV1::WeakControl),
            PotionMechanicRole::ApplyVulnerable => Some(PotionAcquisitionTraitV1::VulnerableSetup),
            PotionMechanicRole::GainBlock
            | PotionMechanicRole::GainDexterity
            | PotionMechanicRole::GainPlatedArmor
            | PotionMechanicRole::GainMetallicize
            | PotionMechanicRole::GainIntangible => Some(PotionAcquisitionTraitV1::CombatBlock),
            PotionMechanicRole::GainEnergy => Some(PotionAcquisitionTraitV1::EnergyBurst),
            PotionMechanicRole::GainStrength | PotionMechanicRole::GainRitual => {
                Some(PotionAcquisitionTraitV1::StrengthGain)
            }
            PotionMechanicRole::DrawCards
            | PotionMechanicRole::RetrieveFromDiscard
            | PotionMechanicRole::DiscardAndDraw => Some(PotionAcquisitionTraitV1::CardAccess),
            PotionMechanicRole::DiscoverAttack
            | PotionMechanicRole::DiscoverSkill
            | PotionMechanicRole::DiscoverPower
            | PotionMechanicRole::DiscoverColorless => {
                Some(PotionAcquisitionTraitV1::CardDiscovery)
            }
            PotionMechanicRole::UpgradeHand => Some(PotionAcquisitionTraitV1::CombatUpgrade),
            PotionMechanicRole::GainArtifact => Some(PotionAcquisitionTraitV1::DebuffControl),
            PotionMechanicRole::PlayTopCards | PotionMechanicRole::DuplicateCards => {
                Some(PotionAcquisitionTraitV1::ActionAmplifier)
            }
            PotionMechanicRole::ExhaustCards => Some(PotionAcquisitionTraitV1::ExhaustControl),
            PotionMechanicRole::PreventDeath => Some(PotionAcquisitionTraitV1::DeathInsurance),
            PotionMechanicRole::EscapeCombat => Some(PotionAcquisitionTraitV1::EscapeTool),
            PotionMechanicRole::GainMaxHp => Some(PotionAcquisitionTraitV1::PermanentMaxHp),
            PotionMechanicRole::ApplyPoison
            | PotionMechanicRole::Heal
            | PotionMechanicRole::GainFocus
            | PotionMechanicRole::AddCardToHand
            | PotionMechanicRole::GainRegeneration
            | PotionMechanicRole::GainThorns
            | PotionMechanicRole::GainOrbSlots
            | PotionMechanicRole::ChooseStance
            | PotionMechanicRole::GeneratePotions
            | PotionMechanicRole::RandomizeHandCosts
            | PotionMechanicRole::Temporary
            | PotionMechanicRole::EnterDivinity
            | PotionMechanicRole::ChannelDark => None,
        };
        if let Some(trait_) = trait_ {
            push_potion_trait(&mut traits, trait_);
        }
    }
    traits
}

pub fn potion_acquisition_requirements_v1(potion: PotionId) -> Vec<AcquisitionRequirementV1> {
    match potion {
        PotionId::FairyPotion => vec![AcquisitionRequirementV1::LowHpDeathInsurance],
        PotionId::SmokeBomb => vec![AcquisitionRequirementV1::RouteEscapeValue],
        PotionId::BlessingOfTheForge => {
            vec![AcquisitionRequirementV1::UpgradeableCombatCards]
        }
        _ => Vec::new(),
    }
}

fn push_relic_trait(traits: &mut Vec<RelicAcquisitionTraitV1>, trait_: RelicAcquisitionTraitV1) {
    if !traits.contains(&trait_) {
        traits.push(trait_);
    }
}

fn push_potion_trait(traits: &mut Vec<PotionAcquisitionTraitV1>, trait_: PotionAcquisitionTraitV1) {
    if !traits.contains(&trait_) {
        traits.push(trait_);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_mechanics_exposes_combat_external_payoff_once() {
        assert_eq!(
            card_mechanics_profile_v1(CardId::Feed).combat_external_payoff,
            Some(CombatExternalPayoffV1::PersistentOrReward)
        );
        assert_eq!(
            card_mechanics_profile_v1(CardId::HandOfGreed).combat_external_payoff,
            Some(CombatExternalPayoffV1::PersistentOrReward)
        );
        assert_eq!(
            card_mechanics_profile_v1(CardId::Reaper).combat_external_payoff,
            Some(CombatExternalPayoffV1::HealingIfDamaged)
        );
        assert_eq!(
            card_mechanics_profile_v1(CardId::TwinStrike).combat_external_payoff,
            None
        );
    }

    #[test]
    fn deep_breath_exposes_reshuffle_access() {
        let mechanics = card_mechanics_profile_v1(CardId::DeepBreath);

        assert!(mechanics.reshuffle_discard_into_draw);
    }

    #[test]
    fn recurring_turn_start_draw_is_visible_card_access() {
        let profile = card_reward_semantic_profile_v1(&RewardCard::new(CardId::Brutality, 0));

        assert!(profile.roles.contains(&CardRewardSemanticRoleV1::CardDraw));
    }

    #[test]
    fn strength_potion_is_not_temporary_strength_burst() {
        assert!(!potion_mechanics_profile_v1(PotionId::StrengthPotion).temporary_strength_burst);
        assert!(potion_mechanics_profile_v1(PotionId::SteroidPotion).temporary_strength_burst);
        assert!(potion_acquisition_traits_v1(PotionId::StrengthPotion)
            .contains(&PotionAcquisitionTraitV1::StrengthGain));
    }

    #[test]
    fn potion_traits_keep_damage_scope_and_debuff_kind_distinct() {
        let fire = potion_acquisition_traits_v1(PotionId::FirePotion);
        let explosive = potion_acquisition_traits_v1(PotionId::ExplosivePotion);
        let fear = potion_acquisition_traits_v1(PotionId::FearPotion);
        let weaken = potion_acquisition_traits_v1(PotionId::WeakenPotion);

        assert!(fire.contains(&PotionAcquisitionTraitV1::CombatDamage));
        assert!(!fire.contains(&PotionAcquisitionTraitV1::AoeDamage));
        assert!(explosive.contains(&PotionAcquisitionTraitV1::CombatDamage));
        assert!(explosive.contains(&PotionAcquisitionTraitV1::AoeDamage));
        assert!(fear.contains(&PotionAcquisitionTraitV1::VulnerableSetup));
        assert!(!fear.contains(&PotionAcquisitionTraitV1::WeakControl));
        assert!(weaken.contains(&PotionAcquisitionTraitV1::WeakControl));
        assert!(!weaken.contains(&PotionAcquisitionTraitV1::VulnerableSetup));
    }

    #[test]
    fn potion_traits_project_shared_mechanic_roles() {
        assert!(potion_acquisition_traits_v1(PotionId::HeartOfIron)
            .contains(&PotionAcquisitionTraitV1::CombatBlock));
        assert!(potion_acquisition_traits_v1(PotionId::DexterityPotion)
            .contains(&PotionAcquisitionTraitV1::CombatBlock));
        assert!(potion_acquisition_traits_v1(PotionId::DistilledChaosPotion)
            .contains(&PotionAcquisitionTraitV1::ActionAmplifier));
    }

    #[test]
    fn fruit_juice_exposes_its_permanent_run_resource_trait() {
        assert!(potion_acquisition_traits_v1(PotionId::FruitJuice)
            .contains(&PotionAcquisitionTraitV1::PermanentMaxHp));
    }

    #[test]
    fn random_card_potions_are_discovery_not_existing_deck_access() {
        let attack = potion_acquisition_traits_v1(PotionId::AttackPotion);
        let liquid_memories = potion_acquisition_traits_v1(PotionId::LiquidMemories);

        assert!(attack.contains(&PotionAcquisitionTraitV1::CardDiscovery));
        assert!(!attack.contains(&PotionAcquisitionTraitV1::CardAccess));
        assert!(liquid_memories.contains(&PotionAcquisitionTraitV1::CardAccess));
        assert!(!liquid_memories.contains(&PotionAcquisitionTraitV1::CardDiscovery));
    }

    #[test]
    fn topdeck_control_cards_expose_mechanical_access() {
        assert!(card_mechanics_profile_v1(CardId::Headbutt).discard_pile_topdeck_access);
        assert!(card_mechanics_profile_v1(CardId::Warcry).hand_topdeck_selection);
    }

    #[test]
    fn additional_attack_play_exposes_an_action_amplifier_payoff() {
        let profile = card_reward_semantic_profile_v1(&RewardCard::new(CardId::DoubleTap, 0));

        assert!(profile
            .roles
            .contains(&CardRewardSemanticRoleV1::ActionAmplifier));
        assert!(profile
            .roles
            .contains(&CardRewardSemanticRoleV1::PackagePayoff));
    }

    #[test]
    fn conditional_shop_objects_expose_requirements() {
        assert!(relic_acquisition_requirements_v1(RelicId::ChemicalX)
            .contains(&AcquisitionRequirementV1::XCostPayoff));
        assert!(relic_acquisition_requirements_v1(RelicId::DollysMirror)
            .contains(&AcquisitionRequirementV1::DuplicateTarget));
        assert!(relic_acquisition_requirements_v1(RelicId::OrangePellets)
            .contains(&AcquisitionRequirementV1::AttackSkillPowerSameTurn));
        assert!(potion_acquisition_traits_v1(PotionId::SmokeBomb)
            .contains(&PotionAcquisitionTraitV1::EscapeTool));
        assert!(potion_acquisition_requirements_v1(PotionId::FairyPotion)
            .contains(&AcquisitionRequirementV1::LowHpDeathInsurance));
        assert!(potion_acquisition_requirements_v1(PotionId::SmokeBomb)
            .contains(&AcquisitionRequirementV1::RouteEscapeValue));
        assert!(potion_acquisition_traits_v1(PotionId::BlessingOfTheForge)
            .contains(&PotionAcquisitionTraitV1::CombatUpgrade));
        assert_eq!(
            potion_mechanics_profile_v1(PotionId::BlessingOfTheForge).combat_upgrade_scope,
            Some(CombatUpgradeScopeV1::WholeHand)
        );
        assert!(
            potion_acquisition_requirements_v1(PotionId::BlessingOfTheForge)
                .contains(&AcquisitionRequirementV1::UpgradeableCombatCards)
        );
    }

    #[test]
    fn anchor_exposes_its_repeatable_opening_defense_as_a_durable_relic_trait() {
        let mechanics = relic_mechanics_profile_v1(RelicId::Anchor);
        let traits = relic_acquisition_traits_v1(RelicId::Anchor);

        assert!(mechanics.core_defense_or_survival);
        assert!(traits.contains(&RelicAcquisitionTraitV1::CoreDefenseOrSurvival));
    }

    #[test]
    fn sacred_bark_exposes_conditional_potion_leverage_not_direct_survival() {
        let mechanics = relic_mechanics_profile_v1(RelicId::SacredBark);
        let traits = relic_acquisition_traits_v1(RelicId::SacredBark);
        let facts = relic_acquisition_mechanics_facts_v1(RelicId::SacredBark);

        assert!(mechanics.potion_effect_multiplier);
        assert!(!mechanics.core_defense_or_survival);
        assert!(traits.contains(&RelicAcquisitionTraitV1::PotionEffectMultiplier));
        assert!(!traits.contains(&RelicAcquisitionTraitV1::CoreDefenseOrSurvival));
        assert_eq!(
            facts.potion_effect_rule,
            Some(RelicPotionEffectRuleFactsV1 {
                potency_multiplier: 2
            })
        );
    }

    #[test]
    fn eternal_feather_exposes_deck_scaled_rest_site_healing() {
        let facts = relic_acquisition_mechanics_facts_v1(RelicId::EternalFeather);

        assert_eq!(
            facts.rest_site_entry_healing,
            Some(RelicRestSiteEntryHealingFactsV1 {
                deck_cards_per_trigger: 5,
                hp_per_trigger: 3,
                prevented_when_player_cannot_heal: true,
            })
        );
        assert_eq!(
            relic_acquisition_mechanics_facts_v1(RelicId::BurningBlood).rest_site_entry_healing,
            None
        );
    }

    #[test]
    fn relic_facts_expose_victory_healing_and_hp_loss_draw_triggers() {
        let burning = relic_acquisition_mechanics_facts_v1(RelicId::BurningBlood);
        assert_eq!(
            burning.combat_victory_healing,
            Some(RelicCombatVictoryHealingFactsV1 {
                base_hp: 6,
                requires_player_alive: true,
                capped_at_max_hp: true,
                affected_by_healing_modifiers: true,
            })
        );
        assert_eq!(burning.card_draw_on_hp_loss, None);

        let black = relic_acquisition_mechanics_facts_v1(RelicId::BlackBlood);
        assert_eq!(
            black.combat_victory_healing,
            Some(RelicCombatVictoryHealingFactsV1 {
                base_hp: 12,
                requires_player_alive: true,
                capped_at_max_hp: true,
                affected_by_healing_modifiers: true,
            })
        );

        let cube = relic_acquisition_mechanics_facts_v1(RelicId::RunicCube);
        assert_eq!(cube.combat_victory_healing, None);
        assert_eq!(
            cube.card_draw_on_hp_loss,
            Some(RelicCardDrawOnHpLossFactsV1 {
                minimum_hp_lost_per_event: 1,
                cards_drawn_per_event: 1,
            })
        );
    }

    #[test]
    fn boss_relic_and_owned_potion_facts_expose_exact_composable_mechanics() {
        let choker = relic_acquisition_mechanics_facts_v1(RelicId::VelvetChoker);
        assert_eq!(choker.energy_per_turn_delta, Some(1));
        assert_eq!(choker.card_play_limit_per_turn, Some(6));
        assert!(!crate::content::relics::velvet_choker::can_play_card(
            choker.card_play_limit_per_turn.unwrap() as i32
        ));
        assert_eq!(choker.starter_card_replacement, None);
        let pandora = relic_acquisition_mechanics_facts_v1(RelicId::PandorasBox);
        assert_eq!(pandora.card_play_limit_per_turn, None);
        let replacement = pandora.starter_card_replacement.unwrap();
        assert_eq!(replacement.replacements_per_card, 1);
        assert_eq!(replacement.initial_upgrades, 0);
        assert!(replacement.applies_obtain_effects);
        assert_eq!(replacement.target_cards.len(), 8);
        assert!(replacement
            .target_cards
            .iter()
            .all(|card| crate::content::cards::is_starter_basic(*card)));
        assert!(!replacement.target_cards.contains(&CardId::PerfectedStrike));
        assert_eq!(
            replacement.pool_rarities,
            [CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare]
        );

        let snecko = relic_acquisition_mechanics_facts_v1(RelicId::SneckoEye);
        assert_eq!(snecko.energy_per_turn_delta, None);
        assert_eq!(snecko.additional_cards_drawn_each_turn, Some(2));
        assert_eq!(
            snecko.drawn_card_cost_randomization,
            Some(DrawnCardCostRandomizationFactsV1 {
                minimum_cost: 0,
                maximum_cost: 3,
                eligible_cost: DrawnCardCostEligibilityV1::NonnegativeCombatCost,
                turn_cost_update: DrawnCardTurnCostUpdateV1::OnlyWhenCombatCostChanges,
                clears_free_to_play_once: true,
            })
        );

        let sozu = relic_acquisition_mechanics_facts_v1(RelicId::Sozu);
        assert_eq!(sozu.energy_per_turn_delta, Some(1));
        assert_eq!(
            sozu.potion_acquisition_rule,
            Some(RelicPotionAcquisitionRuleFactsV1 {
                prevents_new_potions: true,
                owned_potions_remain_usable: true,
            })
        );

        let power = potion_acquisition_mechanics_facts_v1(PotionId::PowerPotion);
        assert_eq!(power.base_potency, 1);
        assert_eq!(
            power.mechanic_roles,
            vec![PotionMechanicRole::DiscoverPower]
        );
        assert_eq!(
            power.card_discovery,
            Some(PotionCardDiscoveryFactsV1 {
                pool: PotionDiscoveryPoolV1::Power,
                offered_choices: 3,
                base_selected_card_copies: 1,
                selected_card_cost_this_turn: 0,
                can_skip: true,
            })
        );

        let steel = potion_acquisition_mechanics_facts_v1(PotionId::EssenceOfSteel);
        assert_eq!(steel.base_potency, 4);
        assert_eq!(
            steel.mechanic_roles,
            vec![PotionMechanicRole::GainPlatedArmor]
        );
        assert_eq!(steel.card_discovery, None);

        let colorless = potion_acquisition_mechanics_facts_v1(PotionId::ColorlessPotion);
        assert_eq!(
            colorless.card_discovery,
            Some(PotionCardDiscoveryFactsV1 {
                pool: PotionDiscoveryPoolV1::Colorless,
                offered_choices: 3,
                base_selected_card_copies: 1,
                selected_card_cost_this_turn: 0,
                can_skip: false,
            })
        );
    }

    #[test]
    fn bronze_scales_exposes_attack_hit_reactive_damage_without_calling_it_defense() {
        let mechanics = relic_mechanics_profile_v1(RelicId::BronzeScales);
        let traits = relic_acquisition_traits_v1(RelicId::BronzeScales);

        assert_eq!(mechanics.reactive_damage_per_attack_hit, 3);
        assert!(!mechanics.core_defense_or_survival);
        assert!(traits.contains(&RelicAcquisitionTraitV1::ReactiveDamage));
        assert!(!traits.contains(&RelicAcquisitionTraitV1::CoreDefenseOrSurvival));
    }

    #[test]
    fn action_supply_traits_distinguish_once_burst_repeatable_and_additional() {
        let lantern = relic_mechanics_profile_v1(RelicId::Lantern).action_supply;
        assert_eq!(lantern.opening_once_options, 1);
        assert!(lantern.cost_or_resource_compression);
        assert!(relic_acquisition_traits_v1(RelicId::Lantern)
            .contains(&RelicAcquisitionTraitV1::ActionSupply));

        let nunchaku = relic_mechanics_profile_v1(RelicId::Nunchaku).action_supply;
        assert!(nunchaku.triggered_repeatable);
        assert!(nunchaku.cost_or_resource_compression);
        assert!(relic_acquisition_traits_v1(RelicId::Nunchaku)
            .contains(&RelicAcquisitionTraitV1::ActionSupply));

        let enchiridion = relic_mechanics_profile_v1(RelicId::Enchiridion).action_supply;
        assert_eq!(enchiridion.opening_once_options, 1);
        assert!(enchiridion.immediate_hand);
        assert!(enchiridion.zero_cost_this_turn);
        assert!(!enchiridion.triggered_repeatable);
        assert!(!enchiridion.potentially_recursive);

        let toolbox = relic_mechanics_profile_v1(RelicId::Toolbox).action_supply;
        assert_eq!(toolbox.opening_once_options, 1);
        assert!(toolbox.immediate_hand);
        assert!(!toolbox.zero_cost_this_turn);

        let codex = relic_mechanics_profile_v1(RelicId::NilrysCodex).action_supply;
        assert!(codex.delayed_per_turn);
        assert!(codex.optional_supply);
        assert!(!codex.same_turn_burst());

        let branch = relic_mechanics_profile_v1(RelicId::DeadBranch).action_supply;
        assert!(branch.triggered_repeatable);
        assert!(branch.immediate_hand);
        assert!(branch.potentially_recursive);

        let blade_dance = card_mechanics_profile_v1(CardId::BladeDance).action_supply;
        assert_eq!(blade_dance.same_turn_burst_min_follow_ups, 3);
        assert!(blade_dance.same_turn_burst());

        let double_tap = card_mechanics_profile_v1(CardId::DoubleTap).action_supply;
        assert_eq!(
            double_tap.additional_play_scope,
            Some(AdditionalPlayScopeV1::Attack)
        );
        assert_eq!(double_tap.same_turn_burst_min_follow_ups, 0);

        assert!(
            card_mechanics_profile_v1(CardId::Corruption)
                .action_supply
                .cost_or_resource_compression
        );
        assert!(
            card_mechanics_profile_v1(CardId::Offering)
                .action_supply
                .cost_or_resource_compression
        );
    }
}
