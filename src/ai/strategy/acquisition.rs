use crate::ai::analysis::card_semantics::{CardBurden, InstalledRule, Mechanic, RunRewardKind};
use crate::ai::card_component_signal_v1::{
    is_concrete_package_support_signal_v1, is_unresolved_package_payoff_debt_signal_v1,
    CardComponentSignalReportV1,
};
use crate::ai::card_semantics_v1::{card_mechanics_profile_v1, card_reward_facts_v1};
use crate::ai::card_tactical_delivery_v1::{
    card_tactical_delivery_profile_v1, CardTacticalOutputAxisV1,
};
use crate::ai::strategy::boss_scaling_evidence::{
    admission_is_strength_payoff, assess_boss_scaling_evidence,
};
use crate::ai::strategy::boss_survival_evidence::assess_boss_survival_evidence;
use crate::ai::strategy::deck_construction_pressure::{
    candidate_improves_card_flow, FrontloadPressure, PressureLevel,
};
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::deck_role_inventory::{
    card_is_stable_strength_source, card_is_strong_aoe,
};
use crate::ai::strategy::deck_strategic_deficit::StrategicDeficitLevel;
use crate::ai::strategy::reward_admission::{
    RewardAdmission, RewardAdmissionClass, RewardAdmissionReason,
};
use crate::content::cards::CardId;
use crate::state::rewards::RewardCard;
use serde::Serialize;

const CHEAP_SHOP_CARD_PRICE: i32 = 35;
const SHOP_PURGE_RESERVE: i32 = 75;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcquisitionContext {
    deck_plan: DeckPlanSnapshot,
    source: AcquisitionContextSource,
}

impl AcquisitionContext {
    pub fn reward(deck_plan: DeckPlanSnapshot) -> Self {
        Self {
            deck_plan,
            source: AcquisitionContextSource::Reward,
        }
    }

    pub fn shop(deck_plan: DeckPlanSnapshot, gold: i32, price: i32) -> Self {
        Self::shop_with_purge_reserve(deck_plan, gold, price, Some(SHOP_PURGE_RESERVE))
    }

    pub fn shop_with_purge_reserve(
        deck_plan: DeckPlanSnapshot,
        gold: i32,
        price: i32,
        purge_reserve: Option<i32>,
    ) -> Self {
        Self {
            deck_plan,
            source: AcquisitionContextSource::Shop {
                gold,
                price,
                purge_reserve,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AcquisitionContextSource {
    Reward,
    Shop {
        gold: i32,
        price: i32,
        purge_reserve: Option<i32>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcquisitionSource {
    Reward,
    Shop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcquisitionCost {
    Free,
    Gold {
        price: i32,
        gold_before: i32,
        gold_after: i32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionOpportunityCost {
    None,
    NoPurgeAvailable,
    Cheap,
    PreservesPurgeReserve,
    SpendsPurgeReserve,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionConstructionRole {
    HardStrategicGap,
    PackageReliability,
    SupportedPackageComponent,
    CombatSustain,
    SoftStrategicGap,
    EngineOrScaling,
    UpgradeAccess,
    /// A turn-bound handler repeatedly increases hand access while the deck's
    /// access axis is still missing or thin. This is not immediate draw and
    /// must not be granted again after the access deficit is covered.
    RecurringCardAccess,
    RunReward,
    SurvivalStabilizer,
    /// One draw slot supplies at least three simultaneous public output axes,
    /// including a substantial axis, without conditional or unsupported
    /// mechanics. Fully redundant short-debuff packages do not qualify.
    MultiAxisTacticalDelivery,
    /// Low-cost block and access share one draw slot while the deck has not
    /// already saturated that tactical compression.
    EfficientTacticalAccess,
    /// One play converts a broad hand into immediate output while emitting
    /// the shared exhaust event. Optional payoff axes remain separate.
    BroadHandConversion,
    /// Deterministic access to a chosen discard-pile card changes when an
    /// already-owned answer can be reused; it is not ordinary damage density.
    TargetedCardReuseAccess,
    /// Removes selected or broad hand material rather than merely exhausting
    /// itself, enabling deck compression and supported exhaust packages.
    ExhaustControl,
    /// Persistent enemy strength reduction remains valuable across multi-hit
    /// and long-fight contexts without relying on a one-turn Weak fragment.
    PersistentMitigation,
    /// The candidate changes the deck's measured card-flow state rather than
    /// merely adding another small cantrip to an already thin access package.
    CardFlowImprovement,
    /// A known Boss owner identified a concrete plan contribution in the
    /// current public run context. This is still independent of whether the
    /// card came from a free reward or a shop transaction.
    KnownBossPlanSupport,
    /// Shared card analysis classifies the candidate as strong area damage and
    /// the deck still has an open AoE/minion-control deficit. Strength alone
    /// is not a marginal acquisition reason once that axis is covered.
    StrongAreaDamage,
    /// During Act 1, the deck still lacks adequate frontload and this card
    /// supplies reliable, tactically dense damage in one non-zero-cost draw
    /// slot.  This is phase-specific transition evidence, not a permanent
    /// premium-card label.
    ActOneDenseFrontloadTransition,
    /// During Act 1, the deck still lacks adequate frontload and this card
    /// supplies reliable damage without consuming energy. The free-reward
    /// owner may use this as transition evidence; it is not by itself proof
    /// that a shop should spend gold or purge reserve on the card.
    ActOneZeroCostFrontloadTransition,
    /// During Act 1, the deck still lacks adequate block or mitigation and
    /// this card supplies a dense block chunk for at most one energy without
    /// adding an unresolved combat-deck burden. This is transition evidence,
    /// not a permanent block-engine label.
    ActOneDenseBlockTransition,
    /// Candidate-local public mechanics prove that an owned enabler or target
    /// makes this component usable now. This is stronger than a broad
    /// formation label, but by itself does not authorize shop spending.
    ContextSupportedComponent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionStrategicDelta {
    pub improves_hard_gap: bool,
    pub improves_any_gap: bool,
    pub repairs_boss_scaling_plan: bool,
    pub repairs_package_reliability: bool,
    pub adds_card_without_gap_improvement: bool,
    pub adds_deployability_debt: bool,
}

/// Acquisition-source-independent marginal construction evidence for one card.
///
/// Reward and shop policy must consume this exact assessment instead of
/// independently rediscovering card roles. Price, purge reserve, and other
/// transaction costs deliberately live in `CardAcquisitionReport`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardConstructionAssessmentV1 {
    pub strategic_delta: AcquisitionStrategicDelta,
    pub construction_roles: Vec<AcquisitionConstructionRole>,
    pub hp_cost_access_debt: bool,
    /// Every one-turn Weak/Vulnerable axis supplied by a costly candidate is
    /// already present in the deck. Partial overlap is not redundancy.
    pub fully_redundant_expensive_short_debuff_package: bool,
    /// The candidate combines several meaningful outputs in one draw slot.
    /// This is mechanics evidence, not a transaction verdict.
    pub multi_axis_tactical_delivery: bool,
    /// Persistent rewards produced by successful in-combat conversion. The
    /// acquisition source still decides whether a future conversion window is
    /// worth paying for.
    pub run_rewards: Vec<RunRewardKind>,
}

impl CardConstructionAssessmentV1 {
    /// A free reward may use any positive marginal role established by the
    /// common construction owner. A soft gap alone is not acquisition proof.
    pub fn has_reward_acquisition_role_v1(&self) -> bool {
        self.construction_roles
            .iter()
            .any(|role| !matches!(role, AcquisitionConstructionRole::SoftStrategicGap))
    }

    /// Concrete marginal construction evidence independent of coarse gap
    /// labels and acquisition source. This can include a phase-specific
    /// transition role; it must not be relabelled as a permanent deck asset.
    /// Reward and shop boundary owners may combine this with exact deltas and
    /// transaction evidence, but may not re-enumerate role meanings.
    pub fn has_source_independent_construction_role_v1(&self) -> bool {
        self.construction_roles.iter().any(|role| {
            !matches!(
                role,
                AcquisitionConstructionRole::HardStrategicGap
                    | AcquisitionConstructionRole::SoftStrategicGap
            )
        })
    }

    /// A shop purchase needs a concrete construction role independent of
    /// coarse gap, acute survival, price-dependent tactical compression, or
    /// unresolved conversion labels.  A phase-specific dense frontload role
    /// is concrete even though its purpose is transition rather than the
    /// terminal deck. Max-HP-on-fatal is a durable exception; gold-on-fatal
    /// still requires a later shop window.
    pub fn has_shop_acquisition_role_v1(&self) -> bool {
        self.construction_roles.iter().any(|role| {
            !matches!(
                role,
                AcquisitionConstructionRole::HardStrategicGap
                    | AcquisitionConstructionRole::SoftStrategicGap
                    | AcquisitionConstructionRole::SurvivalStabilizer
                    | AcquisitionConstructionRole::EfficientTacticalAccess
                    | AcquisitionConstructionRole::RunReward
                    | AcquisitionConstructionRole::ActOneZeroCostFrontloadTransition
                    | AcquisitionConstructionRole::ActOneDenseBlockTransition
                    | AcquisitionConstructionRole::ContextSupportedComponent
            )
        }) || self.run_rewards.contains(&RunRewardKind::MaxHpOnFatal)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionPolicyVerdict {
    ContextTake,
    Speculative,
    SkipPreferred,
    /// The acquisition owner has no positive or negative policy proof.
    Unverified,
    Reject,
}

impl AcquisitionPolicyVerdict {
    pub fn allows_acquisition(self) -> bool {
        matches!(self, Self::ContextTake)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionPolicyReason {
    PackageReliabilityRepair,
    SupportedPackageComponent,
    HardGapAcquisition,
    BossScalingRepair,
    ConstructionRoleAccepted,
    SurvivalPressureStabilizer,
    CheapSurvivalAccessCompression,
    HpCostAccessDebt,
    DeployabilityDebt,
    FullyRedundantShortDebuffPackage,
    NoOpenConstructionRole,
    ContextSupportedComponent,
    NoPolicySupport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionPolicyDecision {
    pub verdict: AcquisitionPolicyVerdict,
    pub reason: AcquisitionPolicyReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardAcquisitionReport {
    pub card: CardId,
    pub upgrades: u8,
    pub source: AcquisitionSource,
    pub cost: AcquisitionCost,
    pub opportunity_cost: AcquisitionOpportunityCost,
    pub construction: CardConstructionAssessmentV1,
    pub cheap_survival_access_compression: bool,
}

pub fn assess_card_construction_v1(
    deck_plan: DeckPlanSnapshot,
    card: CardId,
    upgrades: u8,
    admission: &RewardAdmission,
) -> CardConstructionAssessmentV1 {
    let candidate = Some((card, upgrades));
    let improves_hard_gap = improves_hard_gap(deck_plan, candidate, admission);
    let improves_any_gap = improves_any_gap(deck_plan, candidate, admission);
    let repairs_boss_scaling_plan = needs(deck_plan.strategic_deficit.boss_scaling_plan)
        && assess_boss_scaling_evidence(deck_plan, candidate, admission).relevant_to_boss_plan;
    let repairs_package_reliability = deck_plan.repairs_strength_package_reliability(candidate);
    let adds_deployability_debt =
        adds_deployability_debt(deck_plan, card, upgrades, admission, improves_hard_gap);
    let strategic_delta = AcquisitionStrategicDelta {
        improves_hard_gap,
        improves_any_gap,
        repairs_boss_scaling_plan,
        repairs_package_reliability,
        adds_card_without_gap_improvement: admission.card.is_some()
            && !improves_any_gap
            && !repairs_package_reliability,
        adds_deployability_debt,
    };
    let fully_redundant_expensive_short_debuff_package =
        fully_redundant_expensive_short_debuff_package_v1(deck_plan, card, upgrades);
    let multi_axis_tactical_delivery =
        card_tactical_delivery_profile_v1(card, upgrades).multi_axis_draw_slot_density;
    let candidate_facts = card_reward_facts_v1(&RewardCard::new(card, upgrades));
    let improves_card_flow =
        candidate_improves_card_flow(deck_plan.construction.card_flow, card, upgrades);
    let reliable_boss_scaling_support =
        assess_boss_scaling_evidence(deck_plan, candidate, admission).relevant_to_boss_plan
            && !fragile_supported_payoff(deck_plan, admission)
            && !(admission_is_strength_payoff(admission) && candidate_facts.is_random_output);
    let known_boss_plan_support = deck_plan.boss_key.is_some()
        && (reliable_boss_scaling_support
            || assess_boss_survival_evidence(deck_plan, candidate, admission).repairs_plan());
    let construction_roles = construction_roles(
        deck_plan,
        card,
        upgrades,
        admission,
        &strategic_delta,
        multi_axis_tactical_delivery,
        fully_redundant_expensive_short_debuff_package,
        improves_card_flow,
        known_boss_plan_support,
    );

    CardConstructionAssessmentV1 {
        strategic_delta,
        construction_roles,
        hp_cost_access_debt: hp_cost_access_debt(deck_plan, admission),
        fully_redundant_expensive_short_debuff_package,
        multi_axis_tactical_delivery,
        run_rewards: admission
            .reasons
            .iter()
            .filter_map(|reason| match reason {
                RewardAdmissionReason::RunReward(reward) => Some(*reward),
                _ => None,
            })
            .collect(),
    }
}

/// Enriches the common construction assessment with contextual public
/// mechanics computed by the shared component-signal owner. Broad formation
/// progress is deliberately not concrete support, and an unresolved package
/// debt or deployability debt still blocks acquisition authorization.
pub fn assess_card_construction_with_component_signals_v1(
    deck_plan: DeckPlanSnapshot,
    card: CardId,
    upgrades: u8,
    admission: &RewardAdmission,
    component_signals: &CardComponentSignalReportV1,
) -> CardConstructionAssessmentV1 {
    let mut construction = assess_card_construction_v1(deck_plan, card, upgrades, admission);
    let has_context_support = component_signals
        .positive_signals
        .iter()
        .any(|signal| is_concrete_package_support_signal_v1(*signal));
    let has_unresolved_debt = component_signals
        .debt_signals
        .iter()
        .any(|signal| is_unresolved_package_payoff_debt_signal_v1(*signal));
    if has_context_support
        && !has_unresolved_debt
        && !construction.strategic_delta.adds_deployability_debt
    {
        construction
            .construction_roles
            .push(AcquisitionConstructionRole::ContextSupportedComponent);
    }
    construction
}

pub fn assess_card_acquisition(
    context: AcquisitionContext,
    card: CardId,
    upgrades: u8,
    admission: &RewardAdmission,
) -> CardAcquisitionReport {
    let construction = assess_card_construction_v1(context.deck_plan, card, upgrades, admission);
    assess_card_acquisition_from_construction_v1(context, card, upgrades, admission, construction)
}

pub fn assess_card_acquisition_from_construction_v1(
    context: AcquisitionContext,
    card: CardId,
    upgrades: u8,
    admission: &RewardAdmission,
    construction: CardConstructionAssessmentV1,
) -> CardAcquisitionReport {
    let source = acquisition_source(context.source);
    let cost = acquisition_cost(context.source);
    let opportunity_cost = acquisition_opportunity_cost(context.source);
    let cheap_survival_access_compression =
        cheap_survival_access_compression(context.deck_plan, source, opportunity_cost, admission);
    CardAcquisitionReport {
        card,
        upgrades,
        source,
        cost,
        opportunity_cost,
        construction,
        cheap_survival_access_compression,
    }
}

pub fn evaluate_deck_construction_contract(
    report: &CardAcquisitionReport,
) -> AcquisitionPolicyDecision {
    acquisition_policy_decision(report)
}

fn acquisition_source(source: AcquisitionContextSource) -> AcquisitionSource {
    match source {
        AcquisitionContextSource::Reward => AcquisitionSource::Reward,
        AcquisitionContextSource::Shop { .. } => AcquisitionSource::Shop,
    }
}

fn acquisition_cost(source: AcquisitionContextSource) -> AcquisitionCost {
    match source {
        AcquisitionContextSource::Reward => AcquisitionCost::Free,
        AcquisitionContextSource::Shop { gold, price, .. } => AcquisitionCost::Gold {
            price,
            gold_before: gold,
            gold_after: gold.saturating_sub(price),
        },
    }
}

fn acquisition_opportunity_cost(source: AcquisitionContextSource) -> AcquisitionOpportunityCost {
    match source {
        AcquisitionContextSource::Reward => AcquisitionOpportunityCost::None,
        AcquisitionContextSource::Shop {
            gold,
            price,
            purge_reserve: Some(purge_reserve),
        } if gold.saturating_sub(price) < purge_reserve => {
            AcquisitionOpportunityCost::SpendsPurgeReserve
        }
        AcquisitionContextSource::Shop { price, .. } if price <= CHEAP_SHOP_CARD_PRICE => {
            AcquisitionOpportunityCost::Cheap
        }
        AcquisitionContextSource::Shop {
            purge_reserve: Some(_),
            ..
        } => AcquisitionOpportunityCost::PreservesPurgeReserve,
        AcquisitionContextSource::Shop {
            purge_reserve: None,
            ..
        } => AcquisitionOpportunityCost::NoPurgeAvailable,
    }
}

impl AcquisitionPolicyDecision {
    pub fn allows_acquisition(self) -> bool {
        self.verdict.allows_acquisition()
    }

    /// Returns whether the shared construction owner has proved that this
    /// acquisition supplies a component which the current deck can already
    /// use.  Consumers may use this as a same-verdict tie-break without
    /// reaching back into raw component signals or rebuilding package logic.
    pub fn has_concrete_component_support_v1(self) -> bool {
        self.allows_acquisition()
            && matches!(
                self.reason,
                AcquisitionPolicyReason::SupportedPackageComponent
                    | AcquisitionPolicyReason::ContextSupportedComponent
            )
    }

    pub fn inspect_only_reason(self) -> Option<&'static str> {
        if self.allows_acquisition() {
            None
        } else {
            Some(acquisition_policy_reason_label(self.reason))
        }
    }
}

fn acquisition_policy_decision(report: &CardAcquisitionReport) -> AcquisitionPolicyDecision {
    match report.source {
        AcquisitionSource::Reward if report.construction.hp_cost_access_debt => acquisition_policy(
            AcquisitionPolicyVerdict::Speculative,
            AcquisitionPolicyReason::HpCostAccessDebt,
        ),
        AcquisitionSource::Reward
            if report
                .construction
                .fully_redundant_expensive_short_debuff_package =>
        {
            acquisition_policy(
                AcquisitionPolicyVerdict::Speculative,
                AcquisitionPolicyReason::FullyRedundantShortDebuffPackage,
            )
        }
        AcquisitionSource::Reward
            if report
                .construction
                .construction_roles
                .contains(&AcquisitionConstructionRole::SurvivalStabilizer) =>
        {
            acquisition_policy(
                AcquisitionPolicyVerdict::ContextTake,
                AcquisitionPolicyReason::SurvivalPressureStabilizer,
            )
        }
        AcquisitionSource::Reward
            if report
                .construction
                .construction_roles
                .contains(&AcquisitionConstructionRole::SupportedPackageComponent) =>
        {
            acquisition_policy(
                AcquisitionPolicyVerdict::ContextTake,
                AcquisitionPolicyReason::SupportedPackageComponent,
            )
        }
        AcquisitionSource::Reward
            if report
                .construction
                .construction_roles
                .contains(&AcquisitionConstructionRole::ContextSupportedComponent) =>
        {
            acquisition_policy(
                AcquisitionPolicyVerdict::ContextTake,
                AcquisitionPolicyReason::ContextSupportedComponent,
            )
        }
        AcquisitionSource::Reward
            if report.construction.strategic_delta.adds_deployability_debt
                && !report.construction.construction_roles.iter().any(|role| {
                    matches!(
                        role,
                        AcquisitionConstructionRole::BroadHandConversion
                            | AcquisitionConstructionRole::KnownBossPlanSupport
                    )
                }) =>
        {
            acquisition_policy(
                AcquisitionPolicyVerdict::Speculative,
                AcquisitionPolicyReason::DeployabilityDebt,
            )
        }
        AcquisitionSource::Reward if reward_has_durable_construction_role_v1(report) => {
            acquisition_policy(
                AcquisitionPolicyVerdict::ContextTake,
                AcquisitionPolicyReason::ConstructionRoleAccepted,
            )
        }
        AcquisitionSource::Reward => acquisition_policy(
            AcquisitionPolicyVerdict::Speculative,
            AcquisitionPolicyReason::NoOpenConstructionRole,
        ),
        AcquisitionSource::Shop
            if report
                .construction
                .fully_redundant_expensive_short_debuff_package =>
        {
            acquisition_policy(
                AcquisitionPolicyVerdict::SkipPreferred,
                AcquisitionPolicyReason::FullyRedundantShortDebuffPackage,
            )
        }
        AcquisitionSource::Shop
            if report
                .construction
                .strategic_delta
                .repairs_package_reliability =>
        {
            acquisition_policy(
                AcquisitionPolicyVerdict::ContextTake,
                AcquisitionPolicyReason::PackageReliabilityRepair,
            )
        }
        AcquisitionSource::Shop
            if report
                .construction
                .strategic_delta
                .repairs_boss_scaling_plan =>
        {
            acquisition_policy(
                AcquisitionPolicyVerdict::ContextTake,
                AcquisitionPolicyReason::BossScalingRepair,
            )
        }
        AcquisitionSource::Shop if report.construction.strategic_delta.improves_hard_gap => {
            acquisition_policy(
                AcquisitionPolicyVerdict::ContextTake,
                AcquisitionPolicyReason::HardGapAcquisition,
            )
        }
        AcquisitionSource::Shop if report.cheap_survival_access_compression => acquisition_policy(
            AcquisitionPolicyVerdict::ContextTake,
            AcquisitionPolicyReason::CheapSurvivalAccessCompression,
        ),
        AcquisitionSource::Shop if shop_has_construction_acquisition_role_v1(report) => {
            acquisition_policy(
                AcquisitionPolicyVerdict::ContextTake,
                AcquisitionPolicyReason::ConstructionRoleAccepted,
            )
        }
        AcquisitionSource::Shop => acquisition_policy(
            AcquisitionPolicyVerdict::Unverified,
            AcquisitionPolicyReason::NoPolicySupport,
        ),
    }
}

fn reward_has_durable_construction_role_v1(report: &CardAcquisitionReport) -> bool {
    report.construction.has_reward_acquisition_role_v1()
}

fn shop_has_construction_acquisition_role_v1(report: &CardAcquisitionReport) -> bool {
    report.construction.has_shop_acquisition_role_v1()
}

fn acquisition_policy(
    verdict: AcquisitionPolicyVerdict,
    reason: AcquisitionPolicyReason,
) -> AcquisitionPolicyDecision {
    AcquisitionPolicyDecision { verdict, reason }
}

fn acquisition_policy_reason_label(reason: AcquisitionPolicyReason) -> &'static str {
    match reason {
        AcquisitionPolicyReason::BossScalingRepair => {
            "shop card repairs the open boss scaling plan"
        }
        AcquisitionPolicyReason::NoPolicySupport => "shop card has no acquisition policy support",
        AcquisitionPolicyReason::SupportedPackageComponent => {
            "card fills a missing component axis of a supported package"
        }
        AcquisitionPolicyReason::ContextSupportedComponent => {
            "owned public mechanics make this candidate component usable now"
        }
        AcquisitionPolicyReason::HpCostAccessDebt => {
            "hp-cost access card does not improve an access or energy gap"
        }
        AcquisitionPolicyReason::DeployabilityDebt => {
            "expensive card deepens an unresolved energy or playability gap"
        }
        AcquisitionPolicyReason::FullyRedundantShortDebuffPackage => {
            "costly card's short debuff package is already fully represented"
        }
        AcquisitionPolicyReason::NoOpenConstructionRole => {
            "card does not satisfy deck construction contract"
        }
        AcquisitionPolicyReason::SurvivalPressureStabilizer => {
            "card stabilizes immediate survival pressure"
        }
        AcquisitionPolicyReason::CheapSurvivalAccessCompression => {
            "cheap shop card compresses survival and still-needed access"
        }
        AcquisitionPolicyReason::PackageReliabilityRepair
        | AcquisitionPolicyReason::HardGapAcquisition
        | AcquisitionPolicyReason::ConstructionRoleAccepted => {
            "shop card fails acquisition discipline"
        }
    }
}

fn hp_cost_access_debt(deck_plan: DeckPlanSnapshot, admission: &RewardAdmission) -> bool {
    if !admission
        .reasons
        .contains(&RewardAdmissionReason::Burden(CardBurden::HpCost))
    {
        return false;
    }
    if !(admission_provides(admission, Mechanic::CardDraw)
        || admission_provides(admission, Mechanic::Energy))
    {
        return false;
    }
    if needs(deck_plan.strategic_deficit.deck_access)
        || needs(deck_plan.strategic_deficit.energy_or_playability)
    {
        return false;
    }
    !admission.reasons.iter().any(|reason| {
        matches!(
            reason,
            RewardAdmissionReason::Closes(_)
                | RewardAdmissionReason::Supports(_)
                | RewardAdmissionReason::RunReward(_)
        )
    })
}

fn adds_deployability_debt(
    deck_plan: DeckPlanSnapshot,
    card: CardId,
    upgrades: u8,
    admission: &RewardAdmission,
    improves_hard_gap: bool,
) -> bool {
    !(improves_hard_gap || deck_plan.survival_pressure() && admission_survival_tool(admission))
        && needs(deck_plan.strategic_deficit.energy_or_playability)
        && deck_plan
            .construction
            .card_flow
            .evidence
            .expensive_card_count
            >= 2
        && card_reward_facts_v1(&RewardCard::new(card, upgrades)).cost >= 2
        && !admission_provides(admission, Mechanic::Energy)
        && !admission.reasons.contains(&RewardAdmissionReason::Installs(
            InstalledRule::SkillCardsCostZeroAndExhaust,
        ))
}

fn improves_hard_gap(
    deck_plan: DeckPlanSnapshot,
    candidate: Option<(CardId, u8)>,
    admission: &RewardAdmission,
) -> bool {
    let deficit = deck_plan.strategic_deficit;
    (deficit.deck_access == StrategicDeficitLevel::Missing
        && (candidate.is_some_and(|(card, upgrades)| {
            candidate_improves_card_flow(deck_plan.construction.card_flow, card, upgrades)
        }) || admission
            .reasons
            .contains(&RewardAdmissionReason::CombatUpgrade)))
        || (needs(deficit.energy_or_playability) && admission_provides(admission, Mechanic::Energy))
        || (deficit.aoe_or_minion_control == StrategicDeficitLevel::Missing
            && deck_plan.candidate_improves_aoe_gap(candidate, admission))
        || (deficit.boss_scaling_plan == StrategicDeficitLevel::Missing
            && assess_boss_scaling_evidence(deck_plan, candidate, admission).relevant_to_boss_plan
            && !fragile_supported_payoff(deck_plan, admission))
        || assess_boss_survival_evidence(deck_plan, candidate, admission).repairs_plan()
        || (deficit.frontload_damage == StrategicDeficitLevel::Missing
            && admission_frontloads(admission))
}

fn improves_any_gap(
    deck_plan: DeckPlanSnapshot,
    candidate: Option<(CardId, u8)>,
    admission: &RewardAdmission,
) -> bool {
    let deficit = deck_plan.strategic_deficit;
    (needs(deficit.deck_access)
        && (candidate.is_some_and(|(card, upgrades)| {
            candidate_improves_card_flow(deck_plan.construction.card_flow, card, upgrades)
        }) || admission
            .reasons
            .contains(&RewardAdmissionReason::CombatUpgrade)))
        || (needs(deficit.energy_or_playability) && admission_provides(admission, Mechanic::Energy))
        || deck_plan.candidate_improves_aoe_gap(candidate, admission)
        || (needs(deficit.block_or_mitigation) && admission_survival_tool(admission))
        || (needs(deficit.boss_scaling_plan)
            && assess_boss_scaling_evidence(deck_plan, candidate, admission).relevant_to_boss_plan
            && !fragile_supported_payoff(deck_plan, admission))
        || assess_boss_survival_evidence(deck_plan, candidate, admission).repairs_plan()
        || (needs(deficit.frontload_damage) && admission_frontloads(admission))
}

fn construction_roles(
    deck_plan: DeckPlanSnapshot,
    card: CardId,
    upgrades: u8,
    admission: &RewardAdmission,
    strategic_delta: &AcquisitionStrategicDelta,
    multi_axis_tactical_delivery: bool,
    fully_redundant_expensive_short_debuff_package: bool,
    improves_card_flow: bool,
    known_boss_plan_support: bool,
) -> Vec<AcquisitionConstructionRole> {
    let mut roles = Vec::new();
    let supported_hand_exhaust_conversion =
        deck_plan.has_supported_hand_exhaust_conversion(admission);
    let supported_exhaust_payoff_axis =
        deck_plan.has_open_supported_exhaust_payoff_axis(card, upgrades);
    let supported_block_payoff = admission.class == RewardAdmissionClass::BuildsSupportedPackage
        && admission_damage_uses(admission, Mechanic::Block)
        && deck_plan.block_plan_readiness
            >= crate::ai::block_plan_profile_v1::BlockPlanReadinessV1::Supported;
    let candidate_facts = card_reward_facts_v1(&RewardCard::new(card, upgrades));
    if strategic_delta.improves_hard_gap {
        roles.push(AcquisitionConstructionRole::HardStrategicGap);
    }
    if strategic_delta.repairs_package_reliability {
        roles.push(AcquisitionConstructionRole::PackageReliability);
    }
    if has_combat_sustain(admission) {
        roles.push(AcquisitionConstructionRole::CombatSustain);
    }
    // Acute HP pressure still blocks ordinary scaling picks, but it must not
    // erase a missing exhaust-package axis when the event stream is already
    // live. Deployability debt remains an independent guard below.
    if ((admission.class
        == crate::ai::strategy::reward_admission::RewardAdmissionClass::BuildsSupportedPackage
        && admission_is_strength_payoff(admission)
        && !candidate_facts.is_random_output
        && deck_plan.has_open_stable_strength_payoff_slot())
        || supported_hand_exhaust_conversion
        || supported_exhaust_payoff_axis
        || supported_block_payoff)
        && (!deck_plan.survival_pressure()
            || supported_hand_exhaust_conversion
            || supported_exhaust_payoff_axis
            || supported_block_payoff)
        && (!strategic_delta.adds_deployability_debt || deck_plan.roles.energy_units > 0)
    {
        roles.push(AcquisitionConstructionRole::SupportedPackageComponent);
    }
    if has_run_reward(admission) {
        roles.push(AcquisitionConstructionRole::RunReward);
    }
    if has_combat_upgrade(admission) && deck_plan.roles.upgrade_access_units == 0 {
        roles.push(AcquisitionConstructionRole::UpgradeAccess);
    }
    if card_mechanics_profile_v1(card).recurring_card_draw
        && needs(deck_plan.strategic_deficit.deck_access)
    {
        roles.push(AcquisitionConstructionRole::RecurringCardAccess);
    }
    if admission_scaling_or_engine(deck_plan, card, upgrades, admission)
        && !fragile_supported_payoff(deck_plan, admission)
        && !(admission.class
            == crate::ai::strategy::reward_admission::RewardAdmissionClass::BuildsSupportedPackage
            && admission_is_strength_payoff(admission))
    {
        roles.push(AcquisitionConstructionRole::EngineOrScaling);
    }
    if deck_plan.survival_pressure() && admission_survival_tool(admission) {
        roles.push(AcquisitionConstructionRole::SurvivalStabilizer);
    }
    if strategic_delta.improves_any_gap {
        roles.push(AcquisitionConstructionRole::SoftStrategicGap);
    }
    if multi_axis_tactical_delivery && !fully_redundant_expensive_short_debuff_package {
        roles.push(AcquisitionConstructionRole::MultiAxisTacticalDelivery);
    }
    if improves_card_flow {
        roles.push(AcquisitionConstructionRole::CardFlowImprovement);
    }
    if known_boss_plan_support {
        roles.push(AcquisitionConstructionRole::KnownBossPlanSupport);
    }
    if card_is_strong_aoe(card, upgrades)
        && needs(deck_plan.strategic_deficit.aoe_or_minion_control)
    {
        roles.push(AcquisitionConstructionRole::StrongAreaDamage);
    }
    let facts = candidate_facts;
    let tactical_delivery = card_tactical_delivery_profile_v1(card, upgrades);
    // The coarse deficit grants starter Strikes some credit. Act-one
    // transition readiness instead uses the construction owner, which still
    // asks for damage improvements beyond those starter attacks.
    if deck_plan.context.act == 1
        && deck_plan.construction.frontload == FrontloadPressure::NeedMore
        && facts.cost > 0
        && tactical_delivery.direct_damage_dense
        && !facts.is_random_output
        && !facts.has_conditional_playability
        && !admission.reasons.contains(&RewardAdmissionReason::Burden(
            CardBurden::AddsCombatDeckClutter,
        ))
    {
        roles.push(AcquisitionConstructionRole::ActOneDenseFrontloadTransition);
    }
    if deck_plan.context.act == 1
        && deck_plan.construction.frontload == FrontloadPressure::NeedMore
        && facts.cost == 0
        && tactical_delivery.direct_damage_dense
        && !facts.is_random_output
        && !facts.has_conditional_playability
        && !admission.reasons.contains(&RewardAdmissionReason::Burden(
            CardBurden::AddsCombatDeckClutter,
        ))
    {
        roles.push(AcquisitionConstructionRole::ActOneZeroCostFrontloadTransition);
    }
    if deck_plan.context.act == 1
        && deck_plan.construction.defense.level != PressureLevel::Present
        && facts.cost <= 1
        && tactical_delivery.block_dense
        && !facts.has_conditional_playability
        && !admission.reasons.iter().any(|reason| {
            matches!(
                reason,
                RewardAdmissionReason::Burden(
                    CardBurden::AddsCombatDeckClutter | CardBurden::CardBlockLockoutUntilNextTurn
                )
            )
        })
    {
        roles.push(AcquisitionConstructionRole::ActOneDenseBlockTransition);
    }
    if facts.cost <= 1
        && facts
            .unconditional_base_block_request_v1()
            .is_some_and(|block| block >= 8)
        && facts.draw_cards > 0
        && deck_plan.roles.cycle_block_units < 2
    {
        roles.push(AcquisitionConstructionRole::EfficientTacticalAccess);
    }
    if facts.damage.has_reviewed_attack()
        && admission.reasons.contains(&RewardAdmissionReason::Emits(
            crate::ai::analysis::card_semantics::CombatEvent::CardExhausted,
        ))
        && admission
            .reasons
            .contains(&RewardAdmissionReason::Burden(CardBurden::ExhaustsHand))
    {
        roles.push(AcquisitionConstructionRole::BroadHandConversion);
    }
    if admission_provides(admission, Mechanic::TopdeckControl) {
        roles.push(AcquisitionConstructionRole::TargetedCardReuseAccess);
    }
    if facts.exhausts_other_cards {
        roles.push(AcquisitionConstructionRole::ExhaustControl);
    }
    if facts.has_persistent_enemy_strength_reduction_v1() {
        roles.push(AcquisitionConstructionRole::PersistentMitigation);
    }
    roles
}

fn fully_redundant_expensive_short_debuff_package_v1(
    deck_plan: DeckPlanSnapshot,
    card: CardId,
    upgrades: u8,
) -> bool {
    let candidate = card_reward_facts_v1(&RewardCard::new(card, upgrades));
    let delivery = card_tactical_delivery_profile_v1(card, upgrades);
    let tactical_debuff_only = !delivery.simultaneous_output_axes.is_empty()
        && delivery.simultaneous_output_axes.iter().all(|axis| {
            matches!(
                axis,
                CardTacticalOutputAxisV1::DirectDamage
                    | CardTacticalOutputAxisV1::Weak
                    | CardTacticalOutputAxisV1::Vulnerable
            )
        });
    if candidate.cost < 2 || !tactical_debuff_only {
        return false;
    }

    let has_short_debuff = candidate.vulnerable == 1 || candidate.weak == 1;
    let has_only_short_debuffs = (candidate.vulnerable == 0 || candidate.vulnerable == 1)
        && (candidate.weak == 0 || candidate.weak == 1);
    if !has_short_debuff || !has_only_short_debuffs {
        return false;
    }

    (candidate.vulnerable == 0 || deck_plan.roles.vulnerable_units > 0)
        && (candidate.weak == 0 || deck_plan.roles.weak_units > 0)
}

fn needs(level: StrategicDeficitLevel) -> bool {
    matches!(
        level,
        StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
    )
}

fn has_combat_upgrade(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::CombatUpgrade)
}

fn has_run_reward(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .iter()
        .any(|reason| matches!(reason, RewardAdmissionReason::RunReward(_)))
}

fn has_combat_sustain(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::RecoverCurrentHp)
}

fn fragile_supported_payoff(deck_plan: DeckPlanSnapshot, admission: &RewardAdmission) -> bool {
    if has_combat_sustain(admission) {
        return false;
    }
    if !admission
        .reasons
        .iter()
        .any(|reason| matches!(reason, RewardAdmissionReason::Supports(_)))
    {
        return false;
    }
    if deck_plan.has_supported_hand_exhaust_conversion(admission) {
        return false;
    }
    if admission_is_strength_payoff(admission) {
        return !deck_plan.has_open_stable_strength_payoff_slot();
    }
    if admission_damage_uses(admission, Mechanic::Block) {
        return deck_plan.block_plan_readiness
            < crate::ai::block_plan_profile_v1::BlockPlanReadinessV1::Supported;
    }
    false
}

fn admission_provides(admission: &RewardAdmission, mechanic: Mechanic) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::Provides(mechanic))
}

fn admission_frontloads(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::FrontloadDamage)
}

fn admission_survival_tool(admission: &RewardAdmission) -> bool {
    admission_provides(admission, Mechanic::Block)
        || admission_provides(admission, Mechanic::Weak)
        || admission_provides(admission, Mechanic::EnemyStrengthDown)
        || admission_provides(admission, Mechanic::TemporaryEnemyStrengthDown)
}

fn cheap_survival_access_compression(
    deck_plan: DeckPlanSnapshot,
    source: AcquisitionSource,
    opportunity_cost: AcquisitionOpportunityCost,
    admission: &RewardAdmission,
) -> bool {
    source == AcquisitionSource::Shop
        && opportunity_cost == AcquisitionOpportunityCost::Cheap
        && admission.class == RewardAdmissionClass::ImmediateWork
        && admission_survival_tool(admission)
        && ((admission_provides(admission, Mechanic::CardDraw) && deck_plan.roles.draw_units < 2)
            || (admission_provides(admission, Mechanic::Energy)
                && deck_plan.roles.energy_units == 0))
}

fn admission_scaling_or_engine(
    deck_plan: DeckPlanSnapshot,
    card: CardId,
    upgrades: u8,
    admission: &RewardAdmission,
) -> bool {
    admission.reasons.iter().any(|reason| {
        matches!(
            reason,
            RewardAdmissionReason::Closes(_) | RewardAdmissionReason::Installs(_)
        )
    }) || (card_is_stable_strength_source(
        card,
        upgrades,
        deck_plan.roles.repeatable_self_damage_supply,
    ) && deck_plan.roles.strength_source_units < 2)
        || (admission_provides(admission, Mechanic::StrengthMultiplier)
            && deck_plan.has_open_stable_strength_payoff_slot())
}

fn admission_damage_uses(admission: &RewardAdmission, mechanic: Mechanic) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::DamageUses(mechanic))
}

#[cfg(test)]
mod tests {
    use crate::ai::strategy::deck_admission::DeckAdmissionContext;
    use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
    use crate::ai::strategy::deck_strategic_deficit::StrategicDeficitLevel;
    use crate::ai::strategy::reward_admission::assess_reward_admission_from_master_deck;
    use crate::ai::strategy::run_strategic_facts::RunStrategicFacts;
    use crate::content::cards::CardId;
    use crate::runtime::combat::CombatCard;

    use super::{
        assess_card_acquisition, assess_card_acquisition_from_construction_v1,
        assess_card_construction_v1, evaluate_deck_construction_contract,
        AcquisitionConstructionRole, AcquisitionContext, AcquisitionCost,
        AcquisitionOpportunityCost, AcquisitionPolicyReason, AcquisitionPolicyVerdict,
        AcquisitionSource, CardAcquisitionReport,
    };

    fn deck(cards: &[CardId]) -> Vec<CombatCard> {
        cards
            .iter()
            .enumerate()
            .map(|(index, card)| CombatCard::for_scenario(*card, index as u32 + 1))
            .collect()
    }

    fn act1_shop_plan(cards: &[CardId]) -> DeckPlanSnapshot {
        DeckPlanSnapshot::from_deck(
            &deck(cards),
            DeckAdmissionContext {
                act: 1,
                current_hp: 74,
                max_hp: 85,
            },
            RunStrategicFacts {
                entering_act: 2,
                starter_basic_count: 0,
                curse_count: 0,
                has_energy_relic: false,
                has_runic_pyramid: false,
            },
        )
    }

    #[test]
    fn low_hp_does_not_convert_conditional_block_into_a_stable_survival_repair() {
        let cards = deck(&[CardId::Strike, CardId::Strike, CardId::Defend, CardId::Bash]);
        let plan = DeckPlanSnapshot::from_deck(
            &cards,
            DeckAdmissionContext {
                act: 1,
                current_hp: 10,
                max_hp: 80,
            },
            RunStrategicFacts {
                entering_act: 2,
                starter_basic_count: 3,
                curse_count: 0,
                has_energy_relic: false,
                has_runic_pyramid: false,
            },
        );
        for card in [CardId::EscapePlan, CardId::AutoShields] {
            let admission = assess_reward_admission_from_master_deck(&cards, card, 0);
            let construction = assess_card_construction_v1(plan, card, 0, &admission);
            for role in [
                AcquisitionConstructionRole::ActOneDenseBlockTransition,
                AcquisitionConstructionRole::SurvivalStabilizer,
            ] {
                assert!(
                    !construction.construction_roles.contains(&role),
                    "{card:?} {role:?}"
                );
            }
        }
        let admission = assess_reward_admission_from_master_deck(&cards, CardId::ShrugItOff, 0);
        assert!(
            assess_card_construction_v1(plan, CardId::ShrugItOff, 0, &admission)
                .construction_roles
                .contains(&AcquisitionConstructionRole::SurvivalStabilizer)
        );
    }

    fn act1_missing_access_deck() -> Vec<CardId> {
        vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Armaments,
            CardId::Cleave,
            CardId::Inflame,
            CardId::Uppercut,
            CardId::Whirlwind,
        ]
    }

    fn act1_roles_satisfied_deck() -> Vec<CardId> {
        vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Immolate,
            CardId::Cleave,
            CardId::Whirlwind,
            CardId::ShrugItOff,
            CardId::BattleTrance,
            CardId::Inflame,
        ]
    }

    fn act2_access_saturated_plan(cards: &[CardId]) -> DeckPlanSnapshot {
        DeckPlanSnapshot::from_deck(
            &deck(cards),
            DeckAdmissionContext {
                act: 2,
                current_hp: 42,
                max_hp: 80,
            },
            RunStrategicFacts {
                entering_act: 3,
                starter_basic_count: 0,
                curse_count: 0,
                has_energy_relic: true,
                has_runic_pyramid: false,
            },
        )
    }

    fn act2_energy_thin_plan(cards: &[CardId]) -> DeckPlanSnapshot {
        DeckPlanSnapshot::from_deck(
            &deck(cards),
            DeckAdmissionContext {
                act: 2,
                current_hp: 62,
                max_hp: 80,
            },
            RunStrategicFacts {
                entering_act: 2,
                starter_basic_count: 4,
                curse_count: 0,
                has_energy_relic: false,
                has_runic_pyramid: false,
            },
        )
    }

    fn live_strength_multiplier_package() -> Vec<CardId> {
        vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Cleave,
            CardId::Cleave,
            CardId::Inflame,
            CardId::LimitBreak,
            CardId::HeavyBlade,
            CardId::BurningPact,
            CardId::BattleTrance,
            CardId::Armaments,
            CardId::Disarm,
        ]
    }

    fn seed006_post_purge_shop_report(candidate: CardId, price: i32) -> CardAcquisitionReport {
        let cards = vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Berserk,
        ];
        let master_deck = deck(&cards);
        let plan = DeckPlanSnapshot::from_deck(
            &master_deck,
            DeckAdmissionContext {
                act: 1,
                current_hp: 80,
                max_hp: 80,
            },
            RunStrategicFacts {
                entering_act: 2,
                starter_basic_count: 8,
                curse_count: 0,
                has_energy_relic: false,
                has_runic_pyramid: false,
            },
        );
        let admission = assess_reward_admission_from_master_deck(&master_deck, candidate, 0);

        assess_card_acquisition(
            AcquisitionContext::shop_with_purge_reserve(plan, 43, price, None),
            candidate,
            0,
            &admission,
        )
    }

    #[test]
    fn cheap_survival_access_shop_admission_accepts_seed006_shrug() {
        let report = seed006_post_purge_shop_report(CardId::ShrugItOff, 25);
        let policy = evaluate_deck_construction_contract(&report);

        assert!(report.cheap_survival_access_compression);
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::ContextTake);
        assert_eq!(
            policy.reason,
            AcquisitionPolicyReason::CheapSurvivalAccessCompression
        );
    }

    #[test]
    fn cheap_survival_access_shop_admission_requires_cheap_price() {
        let report = seed006_post_purge_shop_report(CardId::ShrugItOff, 36);
        let policy = evaluate_deck_construction_contract(&report);

        assert!(!report.cheap_survival_access_compression);
        assert!(report
            .construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::EfficientTacticalAccess));
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::Unverified);
        assert_eq!(policy.reason, AcquisitionPolicyReason::NoPolicySupport);
    }

    #[test]
    fn cheap_survival_access_shop_admission_requires_both_roles() {
        let report = seed006_post_purge_shop_report(CardId::IronWave, 25);

        assert!(!report.cheap_survival_access_compression);
    }

    #[test]
    fn shop_accepts_second_stable_strength_source_for_live_multiplier_package() {
        let cards = live_strength_multiplier_package();
        let deck = deck(&cards);
        let plan = act2_access_saturated_plan(&cards);
        assert_eq!(plan.roles.strength_source_units, 1);
        assert_eq!(
            plan.strategic_deficit.boss_scaling_plan,
            crate::ai::strategy::deck_strategic_deficit::StrategicDeficitLevel::Thin
        );
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::Inflame, 0);

        let report = assess_card_acquisition(
            AcquisitionContext::shop(plan, 152, 81),
            CardId::Inflame,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert_eq!(
            report.opportunity_cost,
            AcquisitionOpportunityCost::SpendsPurgeReserve
        );
        assert!(
            report
                .construction
                .strategic_delta
                .repairs_package_reliability
        );
        assert!(report
            .construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::PackageReliability));
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::ContextTake);
        assert_eq!(
            policy.reason,
            AcquisitionPolicyReason::PackageReliabilityRepair
        );
        assert!(policy.allows_acquisition());
    }

    #[test]
    fn strength_package_reliability_does_not_promote_third_conditional_or_payoff_cards() {
        let cards = live_strength_multiplier_package();
        let master_deck = deck(&cards);
        let plan = act2_access_saturated_plan(&cards);

        for candidate in [CardId::SpotWeakness, CardId::HeavyBlade] {
            let admission = assess_reward_admission_from_master_deck(&master_deck, candidate, 0);
            let report = assess_card_acquisition(
                AcquisitionContext::shop(plan, 152, 81),
                candidate,
                0,
                &admission,
            );
            let policy = evaluate_deck_construction_contract(&report);

            assert!(
                !report
                    .construction
                    .strategic_delta
                    .repairs_package_reliability,
                "candidate={candidate:?} report={report:?}"
            );
            assert_ne!(
                policy.reason,
                AcquisitionPolicyReason::PackageReliabilityRepair
            );
            assert!(!policy.allows_acquisition(), "candidate={candidate:?}");
        }

        let mut two_source_cards = cards;
        two_source_cards.push(CardId::Inflame);
        let two_source_deck = deck(&two_source_cards);
        let admission =
            assess_reward_admission_from_master_deck(&two_source_deck, CardId::Inflame, 0);
        let report = assess_card_acquisition(
            AcquisitionContext::shop(act2_access_saturated_plan(&two_source_cards), 152, 81),
            CardId::Inflame,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(
            !report
                .construction
                .strategic_delta
                .repairs_package_reliability
        );
        assert_ne!(
            policy.reason,
            AcquisitionPolicyReason::PackageReliabilityRepair
        );
        assert!(!policy.allows_acquisition());
    }

    #[test]
    fn conditional_strength_label_is_not_a_standalone_shop_asset() {
        let cards = vec![
            CardId::DemonForm,
            CardId::LimitBreak,
            CardId::HeavyBlade,
            CardId::BattleTrance,
        ];
        let master_deck = deck(&cards);
        let plan = act2_access_saturated_plan(&cards);
        let admission =
            assess_reward_admission_from_master_deck(&master_deck, CardId::SpotWeakness, 0);
        let report = assess_card_acquisition(
            AcquisitionContext::shop_with_purge_reserve(plan, 300, 80, Some(75)),
            CardId::SpotWeakness,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(!report
            .construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::EngineOrScaling));
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::Unverified);
        assert_eq!(policy.reason, AcquisitionPolicyReason::NoPolicySupport);
    }

    #[test]
    fn strength_multiplier_requires_an_open_stable_payoff_slot() {
        let cases = [
            (
                vec![CardId::Strike, CardId::Defend, CardId::BattleTrance],
                false,
            ),
            (
                vec![
                    CardId::Strike,
                    CardId::Defend,
                    CardId::BattleTrance,
                    CardId::DemonForm,
                ],
                true,
            ),
            (
                vec![
                    CardId::Strike,
                    CardId::Defend,
                    CardId::BattleTrance,
                    CardId::DemonForm,
                    CardId::HeavyBlade,
                ],
                false,
            ),
        ];

        for (cards, expected_take) in cases {
            let master_deck = deck(&cards);
            let plan = act2_access_saturated_plan(&cards);
            let admission =
                assess_reward_admission_from_master_deck(&master_deck, CardId::LimitBreak, 0);
            let report = assess_card_acquisition(
                AcquisitionContext::reward(plan),
                CardId::LimitBreak,
                0,
                &admission,
            );
            let policy = evaluate_deck_construction_contract(&report);

            assert_eq!(
                policy.allows_acquisition(),
                expected_take,
                "cards={cards:?} plan={plan:?} report={report:?} policy={policy:?}"
            );
        }
    }

    #[test]
    fn recurring_draw_power_only_fills_an_open_access_gap() {
        let sparse_cards = vec![CardId::Strike, CardId::Defend, CardId::Bash];
        let sparse_deck = deck(&sparse_cards);
        let sparse_plan = act2_access_saturated_plan(&sparse_cards);
        let sparse_admission =
            assess_reward_admission_from_master_deck(&sparse_deck, CardId::Brutality, 0);
        let sparse_report = assess_card_acquisition(
            AcquisitionContext::reward(sparse_plan),
            CardId::Brutality,
            0,
            &sparse_admission,
        );

        assert!(matches!(
            sparse_plan.strategic_deficit.deck_access,
            StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
        ));
        assert!(evaluate_deck_construction_contract(&sparse_report).allows_acquisition());

        let mut saturated_cards = act1_roles_satisfied_deck();
        saturated_cards.extend([
            CardId::BurningPact,
            CardId::PommelStrike,
            CardId::Offering,
            CardId::ShrugItOff,
        ]);
        let saturated_deck = deck(&saturated_cards);
        let saturated_plan = act2_access_saturated_plan(&saturated_cards);
        let saturated_admission =
            assess_reward_admission_from_master_deck(&saturated_deck, CardId::Brutality, 0);
        let saturated_report = assess_card_acquisition(
            AcquisitionContext::reward(saturated_plan),
            CardId::Brutality,
            0,
            &saturated_admission,
        );

        assert!(!matches!(
            saturated_plan.strategic_deficit.deck_access,
            StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
        ));
        assert!(!evaluate_deck_construction_contract(&saturated_report).allows_acquisition());
    }

    #[test]
    fn reward_accepts_first_strength_payoff_with_one_stable_source() {
        let cards = vec![
            CardId::Strike,
            CardId::Defend,
            CardId::Bash,
            CardId::DemonForm,
            CardId::BattleTrance,
            CardId::FlameBarrier,
            CardId::Cleave,
        ];
        let master_deck = deck(&cards);
        let plan = DeckPlanSnapshot::from_deck(
            &master_deck,
            DeckAdmissionContext {
                act: 3,
                current_hp: 70,
                max_hp: 80,
            },
            RunStrategicFacts {
                entering_act: 3,
                starter_basic_count: 3,
                curse_count: 0,
                has_energy_relic: true,
                has_runic_pyramid: false,
            },
        );
        let admission =
            assess_reward_admission_from_master_deck(&master_deck, CardId::HeavyBlade, 0);

        let report = assess_card_acquisition(
            AcquisitionContext::reward(plan),
            CardId::HeavyBlade,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(plan.roles.strength_source_units > 0);
        assert_eq!(plan.roles.strength_payoff_units, 0);
        assert!(report
            .construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::SupportedPackageComponent));
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::ContextTake);
        assert!(policy.allows_acquisition());
    }

    #[test]
    fn reward_does_not_repeat_strength_payoff_or_trust_conditional_source() {
        for cards in [
            vec![CardId::DemonForm, CardId::HeavyBlade],
            vec![CardId::SpotWeakness, CardId::SpotWeakness],
        ] {
            let master_deck = deck(&cards);
            let plan = act2_access_saturated_plan(&cards);
            let admission =
                assess_reward_admission_from_master_deck(&master_deck, CardId::HeavyBlade, 0);
            let report = assess_card_acquisition(
                AcquisitionContext::reward(plan),
                CardId::HeavyBlade,
                0,
                &admission,
            );
            let policy = evaluate_deck_construction_contract(&report);

            assert!(
                !policy.allows_acquisition(),
                "unsupported or repeated payoff should remain speculative: cards={cards:?} report={report:?} policy={policy:?}"
            );
        }
    }

    #[test]
    fn supported_payoff_does_not_override_energy_low_margin_or_low_hp_guards() {
        let cases = [
            (
                vec![CardId::DemonForm, CardId::FlameBarrier],
                CardId::HeavyBlade,
                70,
                80,
            ),
            (
                vec![CardId::DemonForm, CardId::SeeingRed],
                CardId::SwordBoomerang,
                70,
                80,
            ),
            (
                vec![CardId::DemonForm, CardId::SeeingRed],
                CardId::HeavyBlade,
                20,
                80,
            ),
        ];

        for (cards, candidate, current_hp, max_hp) in cases {
            let master_deck = deck(&cards);
            let plan = DeckPlanSnapshot::from_deck(
                &master_deck,
                DeckAdmissionContext {
                    act: 3,
                    current_hp,
                    max_hp,
                },
                RunStrategicFacts {
                    entering_act: 3,
                    starter_basic_count: 0,
                    curse_count: 0,
                    has_energy_relic: false,
                    has_runic_pyramid: false,
                },
            );
            let admission = assess_reward_admission_from_master_deck(&master_deck, candidate, 0);
            let report =
                assess_card_acquisition(AcquisitionContext::reward(plan), candidate, 0, &admission);
            let policy = evaluate_deck_construction_contract(&report);

            assert!(
                !policy.allows_acquisition(),
                "guarded payoff must remain speculative: cards={cards:?} candidate={candidate:?} hp={current_hp}/{max_hp} report={report:?} policy={policy:?}"
            );
        }
    }

    #[test]
    fn shop_card_acquisition_retains_opportunity_cost_without_erasing_a_hard_gap() {
        let cards = act1_missing_access_deck();
        let deck = deck(&cards);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::BattleTrance, 0);
        let report = assess_card_acquisition(
            AcquisitionContext::shop(act1_shop_plan(&cards), 72, 51),
            CardId::BattleTrance,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert_eq!(report.source, AcquisitionSource::Shop);
        assert_eq!(
            report.cost,
            AcquisitionCost::Gold {
                price: 51,
                gold_before: 72,
                gold_after: 21
            }
        );
        assert_eq!(
            report.opportunity_cost,
            AcquisitionOpportunityCost::SpendsPurgeReserve
        );
        assert!(report.construction.strategic_delta.improves_hard_gap);
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::ContextTake);
        assert_eq!(policy.reason, AcquisitionPolicyReason::HardGapAcquisition);
        assert!(policy.allows_acquisition());
    }

    #[test]
    fn reward_card_acquisition_has_no_gold_opportunity_cost() {
        let cards = act1_missing_access_deck();
        let deck = deck(&cards);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::BattleTrance, 0);
        let report = assess_card_acquisition(
            AcquisitionContext::reward(act1_shop_plan(&cards)),
            CardId::BattleTrance,
            0,
            &admission,
        );

        assert_eq!(report.source, AcquisitionSource::Reward);
        assert_eq!(report.cost, AcquisitionCost::Free);
        assert_eq!(report.opportunity_cost, AcquisitionOpportunityCost::None);
        assert!(report
            .construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::HardStrategicGap));

        let policy = evaluate_deck_construction_contract(&report);
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::ContextTake);
        assert_eq!(
            policy.reason,
            AcquisitionPolicyReason::ConstructionRoleAccepted
        );
        assert!(policy.allows_acquisition());
    }

    #[test]
    fn act_one_dense_frontload_is_a_real_transition_role_not_a_weak_or_random_attack_label() {
        let cards = vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Disarm,
            CardId::SwordBoomerang,
        ];
        let master_deck = deck(&cards);
        let plan = act1_shop_plan(&cards);
        let assessment = |card| {
            let admission = assess_reward_admission_from_master_deck(&master_deck, card, 0);
            assess_card_construction_v1(plan, card, 0, &admission)
        };

        let carnage = assessment(CardId::Carnage);
        assert!(carnage
            .construction_roles
            .contains(&AcquisitionConstructionRole::ActOneDenseFrontloadTransition));
        let blood_for_blood = assess_card_construction_v1(
            plan,
            CardId::BloodForBlood,
            1,
            &assess_reward_admission_from_master_deck(&master_deck, CardId::BloodForBlood, 1),
        );
        assert!(
            blood_for_blood
                .construction_roles
                .contains(&AcquisitionConstructionRole::ActOneDenseFrontloadTransition),
            "known damage is not suppressed by its separate HP-loss cost effect"
        );
        assert!(!assessment(CardId::IronWave)
            .construction_roles
            .contains(&AcquisitionConstructionRole::ActOneDenseFrontloadTransition));
        assert!(!assessment(CardId::SwordBoomerang)
            .construction_roles
            .contains(&AcquisitionConstructionRole::ActOneDenseFrontloadTransition));

        let mut later_act_plan = plan;
        later_act_plan.context.act = 2;
        let later_act_carnage = assess_card_construction_v1(
            later_act_plan,
            CardId::Carnage,
            0,
            &assess_reward_admission_from_master_deck(&master_deck, CardId::Carnage, 0),
        );
        assert!(!later_act_carnage
            .construction_roles
            .contains(&AcquisitionConstructionRole::ActOneDenseFrontloadTransition));

        let mut covered_plan = plan;
        covered_plan.construction.frontload =
            crate::ai::strategy::deck_construction_pressure::FrontloadPressure::Enough;
        let covered_carnage = assess_card_construction_v1(
            covered_plan,
            CardId::Carnage,
            0,
            &assess_reward_admission_from_master_deck(&master_deck, CardId::Carnage, 0),
        );
        assert!(!covered_carnage
            .construction_roles
            .contains(&AcquisitionConstructionRole::ActOneDenseFrontloadTransition));

        let admission = assess_reward_admission_from_master_deck(&master_deck, CardId::Carnage, 0);
        let report = assess_card_acquisition_from_construction_v1(
            AcquisitionContext::shop(plan, 131, 76),
            CardId::Carnage,
            0,
            &admission,
            carnage,
        );
        assert_eq!(
            report.opportunity_cost,
            AcquisitionOpportunityCost::SpendsPurgeReserve
        );
        assert_eq!(
            evaluate_deck_construction_contract(&report),
            super::AcquisitionPolicyDecision {
                verdict: AcquisitionPolicyVerdict::ContextTake,
                reason: AcquisitionPolicyReason::ConstructionRoleAccepted,
            }
        );
    }

    #[test]
    fn free_act_one_transition_roles_cover_zero_cost_damage_and_dense_block() {
        let cards = vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Disarm,
        ];
        let master_deck = deck(&cards);
        let plan = act1_shop_plan(&cards);
        let assessment = |card| {
            let admission = assess_reward_admission_from_master_deck(&master_deck, card, 0);
            assess_card_construction_v1(plan, card, 0, &admission)
        };

        let anger = assessment(CardId::Anger);
        assert!(anger
            .construction_roles
            .contains(&AcquisitionConstructionRole::ActOneZeroCostFrontloadTransition));
        let ghostly_armor = assessment(CardId::GhostlyArmor);
        assert!(
            ghostly_armor
                .construction_roles
                .contains(&AcquisitionConstructionRole::ActOneDenseBlockTransition),
            "ghostly_armor={ghostly_armor:#?} plan={plan:#?} facts={:#?} admission={:#?}",
            crate::ai::card_semantics_v1::card_reward_facts_v1(
                &crate::state::rewards::RewardCard::new(CardId::GhostlyArmor, 0)
            ),
            assess_reward_admission_from_master_deck(&master_deck, CardId::GhostlyArmor, 0)
        );

        let anger_admission =
            assess_reward_admission_from_master_deck(&master_deck, CardId::Anger, 0);
        assert!(anger_admission.reasons.contains(
            &crate::ai::strategy::reward_admission::RewardAdmissionReason::FrontloadDamage
        ));
        let ghostly_admission =
            assess_reward_admission_from_master_deck(&master_deck, CardId::GhostlyArmor, 0);
        assert!(ghostly_admission.reasons.contains(
            &crate::ai::strategy::reward_admission::RewardAdmissionReason::Provides(
                crate::ai::analysis::card_semantics::Mechanic::Block
            )
        ));

        assert!(!assessment(CardId::SwordBoomerang)
            .construction_roles
            .contains(&AcquisitionConstructionRole::ActOneZeroCostFrontloadTransition));
        assert!(!assessment(CardId::IronWave)
            .construction_roles
            .contains(&AcquisitionConstructionRole::ActOneDenseBlockTransition));

        for (card, construction) in [
            (CardId::Anger, anger),
            (CardId::GhostlyArmor, ghostly_armor),
        ] {
            let admission = assess_reward_admission_from_master_deck(&master_deck, card, 0);
            let reward_report = assess_card_acquisition_from_construction_v1(
                AcquisitionContext::reward(plan),
                card,
                0,
                &admission,
                construction.clone(),
            );
            assert!(evaluate_deck_construction_contract(&reward_report).allows_acquisition());

            let shop_report = assess_card_acquisition_from_construction_v1(
                AcquisitionContext::shop(plan, 131, 76),
                card,
                0,
                &admission,
                construction,
            );
            assert!(
                !evaluate_deck_construction_contract(&shop_report).allows_acquisition(),
                "a free transition role alone must not authorize shop spending for {card:?}"
            );
        }
    }

    #[test]
    fn cantrip_keeps_tactical_compression_without_claiming_an_access_gap() {
        let mut cards = act1_missing_access_deck();
        cards.push(CardId::BurningPact);
        let deck = deck(&cards);
        let plan = act1_shop_plan(&cards);
        assert_eq!(
            plan.strategic_deficit.deck_access,
            crate::ai::strategy::deck_strategic_deficit::StrategicDeficitLevel::Thin
        );
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::ShrugItOff, 0);

        let report = assess_card_acquisition(
            AcquisitionContext::reward(plan),
            CardId::ShrugItOff,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(!report.construction.strategic_delta.improves_any_gap);
        assert!(report
            .construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::EfficientTacticalAccess));
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::ContextTake);
        assert_eq!(
            policy.reason,
            AcquisitionPolicyReason::ConstructionRoleAccepted
        );
    }

    #[test]
    fn real_draw_receives_thin_access_gap_credit() {
        let mut cards = act1_missing_access_deck();
        cards.push(CardId::BurningPact);
        let deck = deck(&cards);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::BattleTrance, 0);

        let report = assess_card_acquisition(
            AcquisitionContext::reward(act1_shop_plan(&cards)),
            CardId::BattleTrance,
            0,
            &admission,
        );

        assert!(report.construction.strategic_delta.improves_any_gap);
        assert!(report
            .construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::SoftStrategicGap));
    }

    #[test]
    fn repeated_weak_aoe_does_not_claim_to_repair_shallow_act2_package() {
        let cards = vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::BattleTrance,
            CardId::Armaments,
            CardId::PommelStrike,
            CardId::Cleave,
            CardId::Cleave,
        ];
        let master_deck = deck(&cards);
        let plan = act1_shop_plan(&cards);
        assert_eq!(
            plan.strategic_deficit.aoe_or_minion_control,
            crate::ai::strategy::deck_strategic_deficit::StrategicDeficitLevel::Thin
        );
        let admission = assess_reward_admission_from_master_deck(&master_deck, CardId::Cleave, 0);

        let report = assess_card_acquisition(
            AcquisitionContext::reward(plan),
            CardId::Cleave,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(!report.construction.strategic_delta.improves_any_gap);
        assert!(
            report
                .construction
                .strategic_delta
                .adds_card_without_gap_improvement
        );
        assert!(report.construction.construction_roles.is_empty());
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::Speculative);
    }

    #[test]
    fn strong_aoe_repairs_shallow_weak_aoe_package() {
        let cards = vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::BattleTrance,
            CardId::Armaments,
            CardId::PommelStrike,
            CardId::Cleave,
            CardId::Cleave,
        ];
        let master_deck = deck(&cards);
        let admission =
            assess_reward_admission_from_master_deck(&master_deck, CardId::Whirlwind, 0);

        let report = assess_card_acquisition(
            AcquisitionContext::reward(act1_shop_plan(&cards)),
            CardId::Whirlwind,
            0,
            &admission,
        );

        assert!(report.construction.strategic_delta.improves_any_gap);
        assert!(
            !report.construction.construction_roles.is_empty(),
            "report={report:#?}"
        );
        assert!(report
            .construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::StrongAreaDamage));
    }

    #[test]
    fn strong_aoe_label_does_not_buy_another_aoe_after_the_axis_is_covered() {
        let cards = vec![
            CardId::Armaments,
            CardId::AscendersBane,
            CardId::Bash,
            CardId::BurningPact,
            CardId::Clothesline,
            CardId::DarkShackles,
            CardId::DoubleTap,
            CardId::Exhume,
            CardId::Feed,
            CardId::Feed,
            CardId::FiendFire,
            CardId::Flex,
            CardId::Headbutt,
            CardId::Hemokinesis,
            CardId::Immolate,
            CardId::Impervious,
            CardId::Inflame,
            CardId::RecklessCharge,
            CardId::SeeingRed,
            CardId::Sentinel,
            CardId::Shockwave,
            CardId::ShrugItOff,
            CardId::ShrugItOff,
            CardId::ShrugItOff,
            CardId::TrueGrit,
            CardId::TwinStrike,
            CardId::Whirlwind,
        ];
        let master_deck = deck(&cards);
        let plan = DeckPlanSnapshot::from_deck(
            &master_deck,
            DeckAdmissionContext {
                act: 3,
                current_hp: 66,
                max_hp: 107,
            },
            RunStrategicFacts {
                entering_act: 3,
                starter_basic_count: 0,
                curse_count: 1,
                has_energy_relic: false,
                has_runic_pyramid: false,
            },
        );
        let admission = assess_reward_admission_from_master_deck(&master_deck, CardId::Combust, 0);
        let report = assess_card_acquisition(
            AcquisitionContext::shop(plan, 73, 40),
            CardId::Combust,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(!report.construction.strategic_delta.improves_any_gap);
        assert!(
            report
                .construction
                .strategic_delta
                .adds_card_without_gap_improvement
        );
        assert!(!report
            .construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::StrongAreaDamage));
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::Unverified);
        assert_eq!(policy.reason, AcquisitionPolicyReason::NoPolicySupport);
    }

    #[test]
    fn ordinary_expensive_soft_gap_card_is_speculative_when_energy_is_thin() {
        let cards = vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Cleave,
            CardId::Whirlwind,
            CardId::Immolate,
            CardId::Inflame,
            CardId::LimitBreak,
            CardId::BurningPact,
            CardId::BattleTrance,
        ];
        let deck = deck(&cards);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::Uppercut, 0);
        let report = assess_card_acquisition(
            AcquisitionContext::reward(act2_energy_thin_plan(&cards)),
            CardId::Uppercut,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(!report.construction.strategic_delta.improves_hard_gap);
        assert!(report.construction.strategic_delta.improves_any_gap);
        assert!(report.construction.strategic_delta.adds_deployability_debt);
        assert!(report
            .construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::SoftStrategicGap));
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::Speculative);
        assert_eq!(policy.reason, AcquisitionPolicyReason::DeployabilityDebt);
    }

    #[test]
    fn expensive_hard_gap_solution_is_not_deployability_debt() {
        let cards = vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Inflame,
            CardId::LimitBreak,
            CardId::BurningPact,
            CardId::BattleTrance,
        ];
        let deck = deck(&cards);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::Immolate, 0);
        let report = assess_card_acquisition(
            AcquisitionContext::reward(act2_energy_thin_plan(&cards)),
            CardId::Immolate,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(report.construction.strategic_delta.improves_hard_gap);
        assert!(!report.construction.strategic_delta.adds_deployability_debt);
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::ContextTake);
    }

    #[test]
    fn reward_without_open_construction_role_is_only_speculative() {
        let cards = act1_roles_satisfied_deck();
        let deck = deck(&cards);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::Cleave, 0);
        let report = assess_card_acquisition(
            AcquisitionContext::reward(act1_shop_plan(&cards)),
            CardId::Cleave,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(report.construction.construction_roles.is_empty());
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::Speculative);
        assert_eq!(
            policy.reason,
            AcquisitionPolicyReason::NoOpenConstructionRole
        );
        assert!(!policy.allows_acquisition());
    }

    #[test]
    fn hard_access_gap_and_shop_opportunity_cost_remain_distinct() {
        let cards = act1_missing_access_deck();
        let deck = deck(&cards);
        let admission =
            assess_reward_admission_from_master_deck(&deck, CardId::MasterOfStrategy, 0);
        let report = assess_card_acquisition(
            AcquisitionContext::shop(act1_shop_plan(&cards), 72, 51),
            CardId::MasterOfStrategy,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(report.construction.strategic_delta.improves_hard_gap);
        assert_eq!(
            report.opportunity_cost,
            AcquisitionOpportunityCost::SpendsPurgeReserve
        );
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::ContextTake);
        assert_eq!(policy.reason, AcquisitionPolicyReason::HardGapAcquisition);
        assert!(policy.allows_acquisition());
    }

    #[test]
    fn hp_cost_access_debt_blocks_reward_when_access_is_saturated() {
        let cards = vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Immolate,
            CardId::Cleave,
            CardId::ShrugItOff,
            CardId::PommelStrike,
            CardId::BattleTrance,
            CardId::Bloodletting,
            CardId::SeeingRed,
            CardId::FiendFire,
        ];
        let deck = deck(&cards);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::Offering, 0);
        let report = assess_card_acquisition(
            AcquisitionContext::reward(act2_access_saturated_plan(&cards)),
            CardId::Offering,
            0,
            &admission,
        );
        let policy = evaluate_deck_construction_contract(&report);

        assert!(report.construction.hp_cost_access_debt);
        assert_eq!(policy.verdict, AcquisitionPolicyVerdict::Speculative);
        assert_eq!(policy.reason, AcquisitionPolicyReason::HpCostAccessDebt);
        assert!(
            !policy.allows_acquisition(),
            "HP-cost access should not be acquired when it no longer solves access or energy"
        );
    }

    #[test]
    fn facts_without_a_restoration_observation_do_not_claim_persistence() {
        let current = crate::ai::card_semantics_v1::card_reward_facts_v1(
            &crate::state::rewards::RewardCard::new(CardId::Disarm, 0),
        );
        let mut serialized = serde_json::to_value(&current).unwrap();
        serialized
            .as_object_mut()
            .unwrap()
            .remove("enemy_strength_restoration");
        let missing: crate::ai::card_semantics_v1::CardRewardFactsV1 =
            serde_json::from_value(serialized).unwrap();
        assert_eq!(missing.enemy_strength_down, 2);
        assert_eq!(missing.enemy_strength_restoration, None);
        assert!(!missing.has_persistent_enemy_strength_reduction_v1());
    }

    #[test]
    fn temporary_strength_reduction_is_not_persistent_construction() {
        let cards = act1_roles_satisfied_deck();
        let master_deck = deck(&cards);
        let plan = act1_shop_plan(&cards);
        for upgrades in [0, 1] {
            for card in [CardId::Disarm, CardId::DarkShackles, CardId::PiercingWail] {
                let admission =
                    assess_reward_admission_from_master_deck(&master_deck, card, upgrades);
                let construction = assess_card_construction_v1(plan, card, upgrades, &admission);
                assert_eq!(
                    construction.construction_roles.contains(&AcquisitionConstructionRole::PersistentMitigation),
                    card == CardId::Disarm,
                    "{card:?}+{upgrades}: the card's restoration schedule cannot be promoted to lasting mitigation"
                );
            }
        }
    }

    #[test]
    fn reward_and_shop_share_construction_but_shop_owns_purge_cost() {
        let cards = vec![
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::FeelNoPain,
            CardId::FeelNoPain,
            CardId::BurningPact,
            CardId::TrueGrit,
            CardId::FireBreathing,
            CardId::WildStrike,
            CardId::Whirlwind,
        ];
        let master_deck = deck(&cards);
        let plan = DeckPlanSnapshot::from_deck(
            &master_deck,
            DeckAdmissionContext {
                act: 2,
                current_hp: 70,
                max_hp: 80,
            },
            RunStrategicFacts {
                entering_act: 2,
                starter_basic_count: 5,
                curse_count: 0,
                has_energy_relic: true,
                has_runic_pyramid: false,
            },
        );
        let admission = assess_reward_admission_from_master_deck(&master_deck, CardId::Disarm, 0);
        let construction = assess_card_construction_v1(plan, CardId::Disarm, 0, &admission);
        assert!(!construction.strategic_delta.improves_hard_gap);
        assert!(construction
            .construction_roles
            .contains(&AcquisitionConstructionRole::PersistentMitigation));

        let reward = assess_card_acquisition_from_construction_v1(
            AcquisitionContext::reward(plan),
            CardId::Disarm,
            0,
            &admission,
            construction.clone(),
        );
        let shop_preserving_purge = assess_card_acquisition_from_construction_v1(
            AcquisitionContext::shop_with_purge_reserve(plan, 250, 79, Some(125)),
            CardId::Disarm,
            0,
            &admission,
            construction.clone(),
        );
        let shop_spending_purge = assess_card_acquisition_from_construction_v1(
            AcquisitionContext::shop_with_purge_reserve(plan, 128, 79, Some(125)),
            CardId::Disarm,
            0,
            &admission,
            construction.clone(),
        );

        assert_eq!(reward.construction, construction);
        assert_eq!(shop_preserving_purge.construction, construction);
        assert_eq!(shop_spending_purge.construction, construction);
        assert!(evaluate_deck_construction_contract(&reward).allows_acquisition());
        assert!(evaluate_deck_construction_contract(&shop_preserving_purge).allows_acquisition());
        assert_eq!(
            shop_spending_purge.opportunity_cost,
            AcquisitionOpportunityCost::SpendsPurgeReserve
        );
        assert_eq!(
            evaluate_deck_construction_contract(&shop_spending_purge),
            super::AcquisitionPolicyDecision {
                verdict: AcquisitionPolicyVerdict::ContextTake,
                reason: AcquisitionPolicyReason::ConstructionRoleAccepted,
            }
        );
    }

    #[test]
    fn exhaust_payoff_support_is_marginal_by_triggered_output_axis() {
        let draw_axis_cards = [
            CardId::BurningPact,
            CardId::TrueGrit,
            CardId::FeelNoPain,
            CardId::Immolate,
        ];
        let draw_axis_deck = deck(&draw_axis_cards);
        let draw_axis_plan = act2_energy_thin_plan(&draw_axis_cards);
        let draw_admission =
            assess_reward_admission_from_master_deck(&draw_axis_deck, CardId::DarkEmbrace, 1);
        let draw =
            assess_card_construction_v1(draw_axis_plan, CardId::DarkEmbrace, 1, &draw_admission);
        assert!(draw
            .construction_roles
            .contains(&AcquisitionConstructionRole::SupportedPackageComponent));

        let block_axis_cards = [
            CardId::BurningPact,
            CardId::TrueGrit,
            CardId::DarkEmbrace,
            CardId::Immolate,
        ];
        let block_axis_deck = deck(&block_axis_cards);
        let block_axis_plan = act2_energy_thin_plan(&block_axis_cards);
        let block_admission =
            assess_reward_admission_from_master_deck(&block_axis_deck, CardId::FeelNoPain, 1);
        let block =
            assess_card_construction_v1(block_axis_plan, CardId::FeelNoPain, 1, &block_admission);
        assert!(block
            .construction_roles
            .contains(&AcquisitionConstructionRole::SupportedPackageComponent));

        let represented_cards = [
            CardId::BurningPact,
            CardId::TrueGrit,
            CardId::FeelNoPain,
            CardId::DarkEmbrace,
            CardId::Immolate,
        ];
        let represented_deck = deck(&represented_cards);
        let represented_plan = act2_energy_thin_plan(&represented_cards);
        let repeated_admission =
            assess_reward_admission_from_master_deck(&represented_deck, CardId::DarkEmbrace, 1);
        let repeated = assess_card_construction_v1(
            represented_plan,
            CardId::DarkEmbrace,
            1,
            &repeated_admission,
        );
        assert!(!repeated
            .construction_roles
            .contains(&AcquisitionConstructionRole::SupportedPackageComponent));
    }

    #[test]
    fn one_limited_exhaust_source_does_not_establish_a_payoff_stream() {
        let cards = [
            CardId::Strike,
            CardId::Defend,
            CardId::Bash,
            CardId::TrueGrit,
        ];
        let master_deck = deck(&cards);
        let plan = act2_energy_thin_plan(&cards);
        let admission =
            assess_reward_admission_from_master_deck(&master_deck, CardId::DarkEmbrace, 1);
        let assessment = assess_card_construction_v1(plan, CardId::DarkEmbrace, 1, &admission);

        assert_eq!(plan.roles.exhaust_stream_units, 1);
        assert!(!assessment
            .construction_roles
            .contains(&AcquisitionConstructionRole::SupportedPackageComponent));
    }

    #[test]
    fn deployability_debt_uses_the_candidate_current_cost() {
        let cards = vec![
            CardId::Strike,
            CardId::Defend,
            CardId::Bash,
            CardId::BurningPact,
            CardId::Immolate,
        ];
        let master_deck = deck(&cards);
        let plan = act2_energy_thin_plan(&cards);
        let base_admission =
            assess_reward_admission_from_master_deck(&master_deck, CardId::DarkEmbrace, 0);
        let upgraded_admission =
            assess_reward_admission_from_master_deck(&master_deck, CardId::DarkEmbrace, 1);

        let base = assess_card_construction_v1(plan, CardId::DarkEmbrace, 0, &base_admission);
        let upgraded =
            assess_card_construction_v1(plan, CardId::DarkEmbrace, 1, &upgraded_admission);

        assert!(base.strategic_delta.adds_deployability_debt);
        assert!(!upgraded.strategic_delta.adds_deployability_debt);
    }
}
