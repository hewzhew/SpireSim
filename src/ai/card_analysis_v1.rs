use crate::ai::card_semantics_v1::{
    card_reward_facts_v1, card_reward_semantic_profile_v1, CardRewardSemanticRoleV1,
};
use crate::content::cards::{get_card_definition, CardId, CardTag, CardType};
use crate::state::rewards::RewardCard;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAnalysisDeckSourceV1 {
    #[default]
    NonStarter = 0,
    StarterStrike = 1,
    StarterDefend = 2,
    StarterUnique = 3,
    Curse = 4,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAnalysisAttackChunkV1 {
    #[default]
    None = 0,
    Weak = 1,
    Solid = 2,
    Burst = 3,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAnalysisBlockChunkV1 {
    #[default]
    None = 0,
    Low = 1,
    Solid = 2,
    Burst = 3,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAnalysisAoeSupportV1 {
    #[default]
    None = 0,
    Present = 1,
    Strong = 2,
}

impl CardAnalysisAoeSupportV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Present => "present",
            Self::Strong => "strong",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAnalysisVulnerableSupportV1 {
    #[default]
    None = 0,
    Limited = 1,
    StarterBash = 2,
    Reliable = 3,
    Premium = 4,
}

impl CardAnalysisVulnerableSupportV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Limited => "limited",
            Self::StarterBash => "bash",
            Self::Reliable => "reliable",
            Self::Premium => "premium",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAnalysisStartupKeyV1 {
    Corruption,
    Havoc,
    Clash,
    FeelNoPain,
    DualWield,
    Anger,
    Rupture,
    Armaments,
    Apparition,
    Offering,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAnalysisUpgradeRedundancyGroupV1 {
    WeakApplication,
    VulnerableApplication,
    ControlledExhaust,
    MassExhaust,
    DrawCantrip,
    FrontloadBigAttack,
    PersistentStrengthScaling,
    ExhaustPayoffPower,
    NonStackingPower,
    BurstBlock,
    #[default]
    Generic,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAnalysisUpgradeStackBehaviorV1 {
    NonStackingOnce,
    StackableIntensity,
    DurationCoverage,
    RedundantAfterFirst,
    DensityPositive,
    DensityNegative,
    ComboThreshold,
    #[default]
    Generic,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardAnalysisBossTransitionBurstV1 {
    #[default]
    None,
    Unconditional,
    StrengthPayoff,
    StrengthConverter,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardAnalysisProfileV1 {
    pub card: CardId,
    pub upgrades: u8,
    pub source: CardAnalysisDeckSourceV1,
    pub card_type: CardType,
    pub cost: i8,
    /// Reviewed unconditional base hit count; zero also includes unresolved
    /// delivery. Consult card_reward_facts_v1 for the typed condition.
    pub attack_hit_count: i32,
    /// Unconditional base contribution before combat modifiers, including
    /// aggregate random hits. Zero is not proof that a card cannot deal damage.
    pub damage: i32,
    /// Unconditional base Block request; variable/conditional Block is omitted.
    pub block: i32,
    pub attack_chunk: CardAnalysisAttackChunkV1,
    pub block_chunk: CardAnalysisBlockChunkV1,
    pub aoe_support: CardAnalysisAoeSupportV1,
    pub vulnerable_support: CardAnalysisVulnerableSupportV1,
    pub startup_key: Option<CardAnalysisStartupKeyV1>,
    pub is_skill: bool,
    pub is_curse: bool,
    pub has_frontload_damage: bool,
    pub has_block: bool,
    pub has_draw_access: bool,
    pub has_energy_access: bool,
    pub has_damage_scaling: bool,
    pub has_temporary_strength_burst: bool,
    pub has_combat_sustain: bool,
    pub has_block_scaling_hook: bool,
    pub has_exhaust_enabler: bool,
    pub has_exhaust_payoff: bool,
    pub has_status_enabler: bool,
    pub has_status_payoff: bool,
    pub is_non_attack: bool,
    pub is_block_plan_plain_coverage: bool,
    pub is_block_plan_medium_chunk: bool,
    pub is_block_plan_high_quality_chunk: bool,
    pub is_block_retention_source: bool,
    pub is_block_multiplier: bool,
    pub is_block_payoff: bool,
    pub is_feel_no_pain_source: bool,
    pub is_second_wind_source: bool,
    pub is_block_plan_controlled_exhaust_source: bool,
    pub is_block_plan_broad_exhaust_source: bool,
    pub is_block_plan_access_support: bool,
    pub is_stasis_sensitive_key_card: bool,
    pub is_startup_setup_debt: bool,
    pub is_startup_setup_payment: bool,
    pub is_startup_immediate_survival: bool,
    pub is_startup_base_combat_shape_risk: bool,
    pub is_startup_unupgraded_apparition: bool,
    pub is_startup_exhaust_engine: bool,
    pub is_startup_strong_draw: bool,
    pub is_startup_self_damage_source: bool,
    pub is_startup_dual_wield_target: bool,
    pub is_startup_strength_payoff_liability_candidate: bool,
    pub is_startup_strong_setup_support_candidate: bool,
    pub is_startup_fnp_exhaust_support_candidate: bool,
    pub is_startup_stable_strength_support_candidate: bool,
    pub is_startup_self_damage_support_candidate: bool,
    pub is_startup_snecko_energy_candidate: bool,
    pub upgrade_redundancy_group: CardAnalysisUpgradeRedundancyGroupV1,
    pub upgrade_stack_behavior: CardAnalysisUpgradeStackBehaviorV1,
    pub is_upgrade_exhaust_control_delta: bool,
    pub is_upgrade_exhaust_removed_delta: bool,
    pub is_upgrade_ethereal_removed_delta: bool,
    pub is_upgrade_innate_delta: bool,
    pub is_upgrade_core_mechanic: bool,
    pub is_upgrade_engine_enabler: bool,
    pub is_upgrade_consistency: bool,
    pub is_upgrade_defensive_survival: bool,
    pub is_upgrade_scaling: bool,
    pub is_upgrade_phase_burst: bool,
    pub is_upgrade_debuff_coverage_candidate: bool,
    pub is_upgrade_stasis_recovery_candidate: bool,
    pub is_upgrade_hyperbeam_block_candidate: bool,
    pub is_boss_minor_power: bool,
    pub is_boss_artifact_strip: bool,
    pub is_boss_exhaust_access: bool,
    pub is_boss_low_value_spam: bool,
    pub boss_transition_burst: CardAnalysisBossTransitionBurstV1,
}

pub fn card_analysis_profile_v1(card: CardId, upgrades: u8) -> CardAnalysisProfileV1 {
    let definition = get_card_definition(card);
    let reward = RewardCard::new(card, upgrades);
    let semantic = card_reward_semantic_profile_v1(&reward);
    let facts = card_reward_facts_v1(&reward);
    let attack_hit_count = facts.damage.hit_count;
    let damage = facts.damage.unconditional_base_total().unwrap_or(0);
    let block = facts.unconditional_base_block_request_v1().unwrap_or(0);
    let attack_chunk = attack_chunk_v1(damage, facts.cost);
    let block_chunk = block_chunk_v1(block);
    let source = deck_source_v1(card);
    let has_aoe = has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::AoeDamage);
    let has_vulnerable = has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::Vulnerable);

    CardAnalysisProfileV1 {
        card,
        upgrades,
        source,
        card_type: definition.card_type,
        cost: facts.cost,
        attack_hit_count,
        damage,
        block,
        attack_chunk,
        block_chunk,
        aoe_support: if has_aoe {
            aoe_support_v1(card, damage)
        } else {
            CardAnalysisAoeSupportV1::None
        },
        vulnerable_support: if has_vulnerable {
            vulnerable_support_v1(card, facts.cost, facts.vulnerable)
        } else {
            CardAnalysisVulnerableSupportV1::None
        },
        startup_key: startup_key_v1(card),
        is_skill: definition.card_type == CardType::Skill,
        is_curse: definition.card_type == CardType::Curse,
        has_frontload_damage: has_role_v1(
            &semantic.roles,
            CardRewardSemanticRoleV1::FrontloadDamage,
        ),
        has_block: has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::Block),
        has_draw_access: has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::CardDraw)
            || has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::CycleAccess),
        has_energy_access: has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::EnergySource),
        has_damage_scaling: has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::ScalingSource)
            || has_role_v1(
                &semantic.roles,
                CardRewardSemanticRoleV1::CombatExternalPayoff,
            ),
        has_temporary_strength_burst: has_role_v1(
            &semantic.roles,
            CardRewardSemanticRoleV1::TemporaryStrengthBurst,
        ),
        has_combat_sustain: has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::CombatSustain),
        has_block_scaling_hook: has_role_v1(
            &semantic.roles,
            CardRewardSemanticRoleV1::BlockRetention,
        ) || has_role_v1(
            &semantic.roles,
            CardRewardSemanticRoleV1::BlockMultiplier,
        ),
        has_exhaust_enabler: has_role_v1(
            &semantic.roles,
            CardRewardSemanticRoleV1::ExhaustGenerator,
        ),
        has_exhaust_payoff: has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::ExhaustPayoff),
        has_status_enabler: has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::StatusGenerator),
        has_status_payoff: has_role_v1(&semantic.roles, CardRewardSemanticRoleV1::StatusPayoff),
        is_non_attack: definition.card_type != CardType::Attack,
        is_block_plan_plain_coverage: is_block_plan_plain_coverage_v1(card),
        is_block_plan_medium_chunk: is_block_plan_medium_chunk_v1(card),
        is_block_plan_high_quality_chunk: is_block_plan_high_quality_chunk_v1(card),
        is_block_retention_source: matches!(card, CardId::Barricade),
        is_block_multiplier: matches!(card, CardId::Entrench),
        is_block_payoff: matches!(card, CardId::BodySlam | CardId::Juggernaut),
        is_feel_no_pain_source: matches!(card, CardId::FeelNoPain),
        is_second_wind_source: matches!(card, CardId::SecondWind),
        is_block_plan_controlled_exhaust_source: is_block_plan_controlled_exhaust_source_v1(card),
        is_block_plan_broad_exhaust_source: is_block_plan_broad_exhaust_source_v1(card),
        is_block_plan_access_support: is_block_plan_access_support_v1(card),
        is_stasis_sensitive_key_card: is_stasis_sensitive_key_card_v1(card),
        is_startup_setup_debt: is_startup_setup_debt_v1(card, upgrades),
        is_startup_setup_payment: is_startup_setup_payment_v1(card),
        is_startup_immediate_survival: is_startup_immediate_survival_v1(card, upgrades),
        is_startup_base_combat_shape_risk: is_startup_base_combat_shape_risk_v1(card),
        is_startup_unupgraded_apparition: matches!(card, CardId::Apparition) && upgrades == 0,
        is_startup_exhaust_engine: is_startup_exhaust_engine_v1(card),
        is_startup_strong_draw: is_startup_strong_draw_v1(card),
        is_startup_self_damage_source: is_startup_self_damage_source_v1(card),
        is_startup_dual_wield_target: is_startup_dual_wield_target_v1(card),
        is_startup_strength_payoff_liability_candidate:
            is_startup_strength_payoff_liability_candidate_v1(card),
        is_startup_strong_setup_support_candidate: is_startup_strong_setup_support_candidate_v1(
            card,
        ),
        is_startup_fnp_exhaust_support_candidate: is_startup_fnp_exhaust_support_candidate_v1(card),
        is_startup_stable_strength_support_candidate:
            is_startup_stable_strength_support_candidate_v1(card),
        is_startup_self_damage_support_candidate: is_startup_self_damage_support_candidate_v1(card),
        is_startup_snecko_energy_candidate: is_startup_snecko_energy_candidate_v1(card),
        upgrade_redundancy_group: upgrade_redundancy_group_v1(card),
        upgrade_stack_behavior: upgrade_stack_behavior_v1(card),
        is_upgrade_exhaust_control_delta: is_upgrade_exhaust_control_delta_v1(card),
        is_upgrade_exhaust_removed_delta: is_upgrade_exhaust_removed_delta_v1(card),
        is_upgrade_ethereal_removed_delta: matches!(card, CardId::Apparition),
        is_upgrade_innate_delta: matches!(card, CardId::BootSequence),
        is_upgrade_core_mechanic: is_upgrade_core_mechanic_v1(card),
        is_upgrade_engine_enabler: is_upgrade_engine_enabler_v1(card),
        is_upgrade_consistency: is_upgrade_consistency_v1(card),
        is_upgrade_defensive_survival: is_upgrade_defensive_survival_v1(card),
        is_upgrade_scaling: is_upgrade_scaling_v1(card),
        is_upgrade_phase_burst: is_upgrade_phase_burst_v1(card),
        is_upgrade_debuff_coverage_candidate: is_upgrade_debuff_coverage_candidate_v1(card),
        is_upgrade_stasis_recovery_candidate: matches!(card, CardId::Apparition),
        is_upgrade_hyperbeam_block_candidate: is_upgrade_hyperbeam_block_candidate_v1(card),
        is_boss_minor_power: is_boss_minor_power_v1(card),
        is_boss_artifact_strip: is_boss_artifact_strip_v1(card),
        is_boss_exhaust_access: is_boss_exhaust_access_v1(card),
        is_boss_low_value_spam: has_role_v1(
            &semantic.roles,
            CardRewardSemanticRoleV1::TemporaryStrengthBurst,
        ) || is_boss_low_value_spam_base_v1(card),
        boss_transition_burst: boss_transition_burst_v1(card),
    }
}

fn has_role_v1(roles: &[CardRewardSemanticRoleV1], role: CardRewardSemanticRoleV1) -> bool {
    roles.contains(&role)
}

fn deck_source_v1(card: CardId) -> CardAnalysisDeckSourceV1 {
    let definition = get_card_definition(card);
    if definition.card_type == CardType::Curse {
        return CardAnalysisDeckSourceV1::Curse;
    }
    if definition.tags.contains(&CardTag::StarterStrike) {
        return CardAnalysisDeckSourceV1::StarterStrike;
    }
    if definition.tags.contains(&CardTag::StarterDefend) {
        return CardAnalysisDeckSourceV1::StarterDefend;
    }
    if matches!(
        card,
        CardId::Bash
            | CardId::Neutralize
            | CardId::Survivor
            | CardId::Zap
            | CardId::Dualcast
            | CardId::Eruption
            | CardId::Vigilance
    ) {
        return CardAnalysisDeckSourceV1::StarterUnique;
    }
    CardAnalysisDeckSourceV1::NonStarter
}

fn attack_chunk_v1(damage: i32, cost: i8) -> CardAnalysisAttackChunkV1 {
    if damage <= 0 {
        CardAnalysisAttackChunkV1::None
    } else if damage >= 20 {
        CardAnalysisAttackChunkV1::Burst
    } else if damage >= 10 || cost == -1 {
        CardAnalysisAttackChunkV1::Solid
    } else {
        CardAnalysisAttackChunkV1::Weak
    }
}

fn block_chunk_v1(block: i32) -> CardAnalysisBlockChunkV1 {
    if block <= 0 {
        CardAnalysisBlockChunkV1::None
    } else if block >= 20 {
        CardAnalysisBlockChunkV1::Burst
    } else if block >= 8 {
        CardAnalysisBlockChunkV1::Solid
    } else {
        CardAnalysisBlockChunkV1::Low
    }
}

fn aoe_support_v1(card: CardId, damage: i32) -> CardAnalysisAoeSupportV1 {
    if damage >= 18 || matches!(card, CardId::Whirlwind | CardId::Immolate) {
        CardAnalysisAoeSupportV1::Strong
    } else {
        CardAnalysisAoeSupportV1::Present
    }
}

fn vulnerable_support_v1(
    card: CardId,
    cost: i8,
    vulnerable: i32,
) -> CardAnalysisVulnerableSupportV1 {
    if vulnerable <= 0 {
        CardAnalysisVulnerableSupportV1::None
    } else if card == CardId::Bash {
        CardAnalysisVulnerableSupportV1::StarterBash
    } else if vulnerable >= 3 {
        CardAnalysisVulnerableSupportV1::Premium
    } else if cost <= 1 || vulnerable >= 2 {
        CardAnalysisVulnerableSupportV1::Reliable
    } else {
        CardAnalysisVulnerableSupportV1::Limited
    }
}

fn is_block_plan_plain_coverage_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Defend
            | CardId::Armaments
            | CardId::ShrugItOff
            | CardId::TrueGrit
            | CardId::IronWave
            | CardId::GhostlyArmor
            | CardId::FlameBarrier
            | CardId::Impervious
            | CardId::PowerThrough
    )
}

fn is_block_plan_medium_chunk_v1(card: CardId) -> bool {
    matches!(card, CardId::FlameBarrier | CardId::GhostlyArmor)
}

fn is_block_plan_high_quality_chunk_v1(card: CardId) -> bool {
    matches!(card, CardId::Impervious | CardId::PowerThrough)
}

fn is_block_plan_controlled_exhaust_source_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::BurningPact | CardId::TrueGrit | CardId::SecondWind | CardId::FiendFire
    )
}

fn is_block_plan_broad_exhaust_source_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::SecondWind | CardId::FiendFire | CardId::SeverSoul | CardId::Corruption
    )
}

fn is_block_plan_access_support_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::BattleTrance
            | CardId::BurningPact
            | CardId::PommelStrike
            | CardId::ShrugItOff
            | CardId::Offering
            | CardId::DeepBreath
            | CardId::Warcry
            | CardId::MasterOfStrategy
    )
}

fn is_stasis_sensitive_key_card_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Barricade
            | CardId::Entrench
            | CardId::Impervious
            | CardId::PowerThrough
            | CardId::FeelNoPain
            | CardId::Corruption
            | CardId::Offering
            | CardId::LimitBreak
            | CardId::DemonForm
            | CardId::Shockwave
    )
}

fn startup_key_v1(card: CardId) -> Option<CardAnalysisStartupKeyV1> {
    match card {
        CardId::Corruption => Some(CardAnalysisStartupKeyV1::Corruption),
        CardId::Havoc => Some(CardAnalysisStartupKeyV1::Havoc),
        CardId::Clash => Some(CardAnalysisStartupKeyV1::Clash),
        CardId::FeelNoPain => Some(CardAnalysisStartupKeyV1::FeelNoPain),
        CardId::DualWield => Some(CardAnalysisStartupKeyV1::DualWield),
        CardId::Anger => Some(CardAnalysisStartupKeyV1::Anger),
        CardId::Rupture => Some(CardAnalysisStartupKeyV1::Rupture),
        CardId::Armaments => Some(CardAnalysisStartupKeyV1::Armaments),
        CardId::Apparition => Some(CardAnalysisStartupKeyV1::Apparition),
        CardId::Offering => Some(CardAnalysisStartupKeyV1::Offering),
        _ => None,
    }
}

fn is_startup_setup_debt_v1(card: CardId, upgrades: u8) -> bool {
    matches!(
        card,
        CardId::FeelNoPain
            | CardId::DarkEmbrace
            | CardId::DemonForm
            | CardId::Barricade
            | CardId::Metallicize
            | CardId::FireBreathing
            | CardId::Evolve
            | CardId::Rupture
            | CardId::DualWield
            | CardId::LimitBreak
    ) || (card == CardId::Armaments && upgrades == 0)
}

fn is_startup_setup_payment_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Offering
            | CardId::BattleTrance
            | CardId::BurningPact
            | CardId::Bloodletting
            | CardId::SeeingRed
            | CardId::Sentinel
            | CardId::ShrugItOff
            | CardId::PommelStrike
            | CardId::Warcry
    )
}

fn is_startup_immediate_survival_v1(card: CardId, upgrades: u8) -> bool {
    matches!(
        card,
        CardId::Impervious
            | CardId::FlameBarrier
            | CardId::PowerThrough
            | CardId::ShrugItOff
            | CardId::Disarm
            | CardId::Shockwave
            | CardId::Uppercut
            | CardId::Clothesline
            | CardId::Intimidate
            | CardId::TrueGrit
            | CardId::SecondWind
    ) || (card == CardId::Apparition && upgrades > 0)
}

fn is_startup_base_combat_shape_risk_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Anger
            | CardId::WildStrike
            | CardId::RecklessCharge
            | CardId::DualWield
            | CardId::Havoc
            | CardId::Clash
    )
}

fn is_startup_exhaust_engine_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Corruption
            | CardId::BurningPact
            | CardId::TrueGrit
            | CardId::SecondWind
            | CardId::FiendFire
            | CardId::SeverSoul
            | CardId::Havoc
    )
}

fn is_startup_strong_draw_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Offering | CardId::BattleTrance | CardId::BurningPact | CardId::DarkEmbrace
    )
}

fn is_startup_self_damage_source_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Bloodletting
            | CardId::Offering
            | CardId::Hemokinesis
            | CardId::Combust
            | CardId::Brutality
            | CardId::JAX
    )
}

fn is_startup_dual_wield_target_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Feed
            | CardId::Reaper
            | CardId::DemonForm
            | CardId::Barricade
            | CardId::Corruption
            | CardId::LimitBreak
            | CardId::Inflame
            | CardId::SpotWeakness
    )
}

fn is_startup_strength_payoff_liability_candidate_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::HeavyBlade | CardId::SwordBoomerang | CardId::Pummel
    )
}

fn is_startup_strong_setup_support_candidate_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Offering | CardId::BattleTrance | CardId::BurningPact
    )
}

fn is_startup_fnp_exhaust_support_candidate_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::BurningPact | CardId::TrueGrit | CardId::SecondWind | CardId::FiendFire
    )
}

fn is_startup_stable_strength_support_candidate_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Inflame | CardId::SpotWeakness | CardId::DemonForm
    )
}

fn is_startup_self_damage_support_candidate_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Bloodletting | CardId::Hemokinesis | CardId::Combust | CardId::Brutality
    )
}

fn is_startup_snecko_energy_candidate_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Offering | CardId::SeeingRed | CardId::Bloodletting
    )
}

fn upgrade_redundancy_group_v1(card: CardId) -> CardAnalysisUpgradeRedundancyGroupV1 {
    match card {
        CardId::Clothesline | CardId::Uppercut | CardId::Shockwave | CardId::Blind => {
            CardAnalysisUpgradeRedundancyGroupV1::WeakApplication
        }
        CardId::Bash | CardId::ThunderClap | CardId::Trip => {
            CardAnalysisUpgradeRedundancyGroupV1::VulnerableApplication
        }
        CardId::TrueGrit | CardId::BurningPact => {
            CardAnalysisUpgradeRedundancyGroupV1::ControlledExhaust
        }
        CardId::SecondWind | CardId::FiendFire | CardId::SeverSoul => {
            CardAnalysisUpgradeRedundancyGroupV1::MassExhaust
        }
        CardId::PommelStrike
        | CardId::ShrugItOff
        | CardId::Warcry
        | CardId::BattleTrance
        | CardId::Offering => CardAnalysisUpgradeRedundancyGroupV1::DrawCantrip,
        CardId::Bludgeon | CardId::Carnage | CardId::Immolate => {
            CardAnalysisUpgradeRedundancyGroupV1::FrontloadBigAttack
        }
        CardId::DemonForm | CardId::Inflame | CardId::SpotWeakness => {
            CardAnalysisUpgradeRedundancyGroupV1::PersistentStrengthScaling
        }
        CardId::FeelNoPain | CardId::DarkEmbrace => {
            CardAnalysisUpgradeRedundancyGroupV1::ExhaustPayoffPower
        }
        CardId::Corruption | CardId::Barricade => {
            CardAnalysisUpgradeRedundancyGroupV1::NonStackingPower
        }
        CardId::FlameBarrier | CardId::Impervious | CardId::PowerThrough => {
            CardAnalysisUpgradeRedundancyGroupV1::BurstBlock
        }
        _ => CardAnalysisUpgradeRedundancyGroupV1::Generic,
    }
}

fn upgrade_stack_behavior_v1(card: CardId) -> CardAnalysisUpgradeStackBehaviorV1 {
    if card == CardId::Apparition {
        return CardAnalysisUpgradeStackBehaviorV1::DensityPositive;
    }
    match upgrade_redundancy_group_v1(card) {
        CardAnalysisUpgradeRedundancyGroupV1::WeakApplication
        | CardAnalysisUpgradeRedundancyGroupV1::VulnerableApplication => {
            CardAnalysisUpgradeStackBehaviorV1::DurationCoverage
        }
        CardAnalysisUpgradeRedundancyGroupV1::ControlledExhaust
        | CardAnalysisUpgradeRedundancyGroupV1::DrawCantrip => {
            CardAnalysisUpgradeStackBehaviorV1::DensityPositive
        }
        CardAnalysisUpgradeRedundancyGroupV1::MassExhaust
        | CardAnalysisUpgradeRedundancyGroupV1::FrontloadBigAttack => {
            CardAnalysisUpgradeStackBehaviorV1::DensityNegative
        }
        CardAnalysisUpgradeRedundancyGroupV1::PersistentStrengthScaling
        | CardAnalysisUpgradeRedundancyGroupV1::NonStackingPower => {
            CardAnalysisUpgradeStackBehaviorV1::RedundantAfterFirst
        }
        CardAnalysisUpgradeRedundancyGroupV1::ExhaustPayoffPower => {
            CardAnalysisUpgradeStackBehaviorV1::StackableIntensity
        }
        CardAnalysisUpgradeRedundancyGroupV1::BurstBlock => {
            CardAnalysisUpgradeStackBehaviorV1::ComboThreshold
        }
        CardAnalysisUpgradeRedundancyGroupV1::Generic => {
            if matches!(card, CardId::LimitBreak) {
                CardAnalysisUpgradeStackBehaviorV1::ComboThreshold
            } else {
                CardAnalysisUpgradeStackBehaviorV1::Generic
            }
        }
    }
}

fn is_upgrade_exhaust_control_delta_v1(card: CardId) -> bool {
    matches!(card, CardId::TrueGrit | CardId::Meditate)
}

fn is_upgrade_exhaust_removed_delta_v1(card: CardId) -> bool {
    matches!(card, CardId::Havoc | CardId::Armaments | CardId::LimitBreak)
}

fn is_upgrade_core_mechanic_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::TrueGrit
            | CardId::Armaments
            | CardId::LimitBreak
            | CardId::Apparition
            | CardId::Havoc
    )
}

fn is_upgrade_engine_enabler_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::TrueGrit
            | CardId::SecondWind
            | CardId::FiendFire
            | CardId::BurningPact
            | CardId::Corruption
            | CardId::DarkEmbrace
            | CardId::FeelNoPain
    )
}

fn is_upgrade_consistency_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::PommelStrike
            | CardId::Warcry
            | CardId::BattleTrance
            | CardId::BurningPact
            | CardId::Offering
            | CardId::SecretWeapon
            | CardId::SecretTechnique
    )
}

fn is_upgrade_defensive_survival_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::FlameBarrier
            | CardId::Impervious
            | CardId::PowerThrough
            | CardId::Disarm
            | CardId::Apparition
            | CardId::SecondWind
            | CardId::TrueGrit
            | CardId::ShrugItOff
            | CardId::Entrench
            | CardId::Barricade
    )
}

fn is_upgrade_scaling_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::DemonForm
            | CardId::Inflame
            | CardId::SpotWeakness
            | CardId::LimitBreak
            | CardId::Corruption
            | CardId::Barricade
            | CardId::Entrench
    )
}

fn is_upgrade_phase_burst_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Bludgeon
            | CardId::Carnage
            | CardId::Immolate
            | CardId::Offering
            | CardId::Bash
            | CardId::Uppercut
            | CardId::Whirlwind
            | CardId::FiendFire
    )
}

fn is_upgrade_debuff_coverage_candidate_v1(card: CardId) -> bool {
    matches!(card, CardId::Bash | CardId::Uppercut | CardId::Shockwave)
}

fn is_upgrade_hyperbeam_block_candidate_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Impervious | CardId::PowerThrough | CardId::FlameBarrier
    )
}

fn is_boss_minor_power_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Inflame
            | CardId::Metallicize
            | CardId::Combust
            | CardId::FireBreathing
            | CardId::Rupture
            | CardId::Evolve
    )
}

fn is_boss_artifact_strip_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Bash | CardId::Shockwave | CardId::Uppercut | CardId::ThunderClap
    )
}

fn is_boss_exhaust_access_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::BurningPact
            | CardId::Corruption
            | CardId::FiendFire
            | CardId::SecondWind
            | CardId::SeverSoul
            | CardId::TrueGrit
    )
}

fn is_boss_low_value_spam_base_v1(card: CardId) -> bool {
    matches!(
        card,
        CardId::Anger | CardId::Warcry | CardId::Bloodletting | CardId::SeeingRed
    )
}

fn boss_transition_burst_v1(card: CardId) -> CardAnalysisBossTransitionBurstV1 {
    match card {
        CardId::DemonForm
        | CardId::Carnage
        | CardId::Bludgeon
        | CardId::Offering
        | CardId::Whirlwind => CardAnalysisBossTransitionBurstV1::Unconditional,
        CardId::HeavyBlade => CardAnalysisBossTransitionBurstV1::StrengthPayoff,
        CardId::LimitBreak => CardAnalysisBossTransitionBurstV1::StrengthConverter,
        _ => CardAnalysisBossTransitionBurstV1::None,
    }
}

#[cfg(test)]
mod tests {
    use super::card_analysis_profile_v1;
    use crate::content::cards::CardId;

    #[test]
    fn limit_break_upgrade_records_exhaust_removal() {
        let profile = card_analysis_profile_v1(CardId::LimitBreak, 0);

        assert!(profile.is_upgrade_exhaust_removed_delta);
    }

    #[test]
    fn apparition_upgrade_records_ethereal_removal() {
        let profile = card_analysis_profile_v1(CardId::Apparition, 0);

        assert!(profile.is_upgrade_ethereal_removed_delta);
    }

    #[test]
    fn analysis_numbers_use_reviewed_reward_delivery() {
        for (id, upgrades, hits, damage, block) in [
            (CardId::Wish, 1, 0, 0, 0),
            (CardId::Pummel, 1, 5, 10, 0),
            (CardId::DaggerSpray, 1, 2, 12, 0),
            (CardId::SearingBlow, 5, 1, 42, 0),
            (CardId::GrandFinale, 1, 0, 0, 0),
            (CardId::EscapePlan, 1, 0, 0, 0),
            (CardId::SecondWind, 1, 0, 0, 0),
            (CardId::Defend, 1, 0, 0, 8),
        ] {
            let profile = card_analysis_profile_v1(id, upgrades);
            assert_eq!(
                (profile.attack_hit_count, profile.damage, profile.block),
                (hits, damage, block),
                "{id:?}"
            );
        }
        assert_eq!(
            card_analysis_profile_v1(CardId::GrandFinale, 1).attack_chunk,
            super::CardAnalysisAttackChunkV1::None
        );
        assert_eq!(
            card_analysis_profile_v1(CardId::SecondWind, 1).block_chunk,
            super::CardAnalysisBlockChunkV1::None
        );
    }

    #[test]
    fn analysis_cost_uses_the_upgraded_card() {
        assert_eq!(card_analysis_profile_v1(CardId::BodySlam, 1).cost, 0);
        assert_eq!(card_analysis_profile_v1(CardId::BloodForBlood, 1).cost, 3);
    }

    #[test]
    fn raw_chunks_and_vulnerable_support_remain_factual() {
        let clothesline = card_analysis_profile_v1(CardId::Clothesline, 0);
        assert_eq!(
            clothesline.attack_chunk,
            super::CardAnalysisAttackChunkV1::Solid
        );

        let upgraded_iron_wave = card_analysis_profile_v1(CardId::IronWave, 1);
        assert_eq!(
            upgraded_iron_wave.attack_chunk,
            super::CardAnalysisAttackChunkV1::Weak
        );
        assert_eq!(
            upgraded_iron_wave.block_chunk,
            super::CardAnalysisBlockChunkV1::Low
        );

        let upgraded_twin_strike = card_analysis_profile_v1(CardId::TwinStrike, 1);
        assert_eq!(
            upgraded_twin_strike.attack_chunk,
            super::CardAnalysisAttackChunkV1::Solid
        );

        let uppercut = card_analysis_profile_v1(CardId::Uppercut, 0);
        assert_eq!(
            uppercut.vulnerable_support,
            super::CardAnalysisVulnerableSupportV1::Limited
        );
        let upgraded_uppercut = card_analysis_profile_v1(CardId::Uppercut, 1);
        assert_eq!(
            upgraded_uppercut.vulnerable_support,
            super::CardAnalysisVulnerableSupportV1::Reliable
        );
    }
}
