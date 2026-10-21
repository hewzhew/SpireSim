use crate::ai::analysis::card_semantics::{card_definition_with_upgrades, CardBurden, Mechanic};
use crate::ai::card_analysis_v1::{card_analysis_profile_v1, CardAnalysisBlockChunkV1};
use crate::ai::strategy::acquisition::{
    assess_card_acquisition, evaluate_deck_construction_contract, AcquisitionContext,
    AcquisitionPolicyVerdict,
};
use crate::ai::strategy::boss_scaling_evidence::{
    admission_is_strength_payoff, assess_boss_scaling_evidence,
};
use crate::ai::strategy::boss_survival_evidence::assess_boss_survival_evidence;
use crate::ai::strategy::deck_admission::DeckAdmission;
use crate::ai::strategy::deck_construction_pressure::{
    ConstructionLaneAdjustment, FrontloadPressure,
};
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::deck_strategic_deficit::StrategicDeficitLevel;
use crate::ai::strategy::reward_admission::{
    RewardAdmission, RewardAdmissionClass, RewardAdmissionReason,
};
use crate::ai::strategy::reward_quality::RewardDuplicateConcern;
use crate::ai::strategy::role_saturation::{
    assess_role_saturation, marginal_reason_label, LaneCap, RoleSaturationCandidate,
};
use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecisionPipelineContext {
    pub deck_plan: DeckPlanSnapshot,
    pub gold: Option<i32>,
    pub purge_reserve: Option<i32>,
}

impl DecisionPipelineContext {
    pub fn reward(deck_plan: DeckPlanSnapshot) -> Self {
        Self {
            deck_plan,
            gold: None,
            purge_reserve: None,
        }
    }

    pub fn shop(deck_plan: DeckPlanSnapshot, gold: i32) -> Self {
        Self::shop_with_purge_reserve(deck_plan, gold, Some(75))
    }

    pub fn shop_with_purge_reserve(
        deck_plan: DeckPlanSnapshot,
        gold: i32,
        purge_reserve: Option<i32>,
    ) -> Self {
        Self {
            deck_plan,
            gold: Some(gold),
            purge_reserve,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecisionCandidateIr {
    pub kind: DecisionCandidateKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionCandidateKind {
    CardRewardPick {
        card: CardId,
        upgrades: u8,
    },
    CardRewardSkip,
    BossRelicPick {
        relic: RelicId,
    },
    BossRelicSkip,
    ShopBuyCard {
        card: CardId,
        upgrades: u8,
        price: i32,
    },
    ShopBuyRelic {
        relic: RelicId,
        price: i32,
    },
    ShopBuyPotion {
        potion: PotionId,
        price: i32,
    },
    ShopPurge {
        target: CleanupTarget,
    },
    ShopOpenRewards,
    ShopLeave,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupTarget {
    Curse,
    Status,
    StarterStrike,
    UpgradedStarterStrike,
    StarterDefend,
    UpgradedStarterDefend,
    OtherStarter,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateLane {
    Mainline,
    Probe,
    Skip,
    Reject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpansionPlan {
    Auto,
    InspectOnly(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterDecision {
    Pass,
    InspectOnly(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreComponent {
    pub by: &'static str,
    pub value: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateLaneCapSource {
    RoleSaturation,
    Strategic,
    Acquisition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateLaneCap {
    pub source: CandidateLaneCapSource,
    pub cap: LaneCap,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateLaneAdjudication {
    pub raw_lane: CandidateLane,
    pub final_lane: CandidateLane,
    pub caps: Vec<CandidateLaneCap>,
}

impl CandidateLaneAdjudication {
    pub fn uncapped(lane: CandidateLane) -> Self {
        Self {
            raw_lane: lane,
            final_lane: lane,
            caps: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CandidateOrderKey {
    pub lane_rank: u8,
    pub score_rank: i32,
    pub tiebreak_rank: u8,
}

impl CandidateOrderKey {
    pub fn fallback() -> Self {
        Self {
            lane_rank: 3,
            score_rank: 0,
            tiebreak_rank: 9,
        }
    }

    pub fn with_auto_rank(self, auto_rank: u8) -> (u8, Self) {
        (auto_rank, self)
    }

    pub fn optional_skip(has_mainline_take: bool) -> Self {
        Self {
            lane_rank: candidate_lane_rank(CandidateLane::Skip, has_mainline_take),
            score_rank: 0,
            tiebreak_rank: candidate_tiebreak_rank(DecisionCandidateKind::CardRewardSkip),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateEvaluation {
    pub candidate: DecisionCandidateIr,
    pub lane: CandidateLane,
    pub adjudication: CandidateLaneAdjudication,
    pub expansion: ExpansionPlan,
    pub scores: Vec<ScoreComponent>,
}

impl CandidateEvaluation {
    pub fn inspect_only(candidate: DecisionCandidateIr, reason: &'static str) -> Self {
        Self {
            candidate,
            lane: CandidateLane::Reject,
            adjudication: CandidateLaneAdjudication::uncapped(CandidateLane::Reject),
            expansion: ExpansionPlan::InspectOnly(reason),
            scores: Vec::new(),
        }
    }

    pub fn total_score(&self) -> i32 {
        self.scores.iter().map(|score| score.value).sum()
    }

    pub fn is_mainline(&self) -> bool {
        self.lane == CandidateLane::Mainline
    }

    pub fn auto_expands(&self) -> bool {
        self.expansion == ExpansionPlan::Auto
    }

    pub fn inspect_only_reason(&self) -> Option<&'static str> {
        match self.expansion {
            ExpansionPlan::Auto => None,
            ExpansionPlan::InspectOnly(reason) => Some(reason),
        }
    }

    pub fn order_key(&self, has_mainline_take: bool) -> CandidateOrderKey {
        CandidateOrderKey {
            lane_rank: candidate_lane_rank(self.lane, has_mainline_take),
            score_rank: -self.total_score(),
            tiebreak_rank: candidate_tiebreak_rank(self.candidate.kind),
        }
    }

    pub fn auto_order_key(&self, has_mainline_take: bool) -> (u8, CandidateOrderKey) {
        self.order_key(has_mainline_take)
            .with_auto_rank(u8::from(!self.auto_expands()))
    }
}

type FilterPass =
    fn(DecisionPipelineContext, DecisionCandidateIr, Option<&RewardAdmission>) -> FilterDecision;
type ScorePass = fn(
    DecisionPipelineContext,
    DecisionCandidateIr,
    Option<&RewardAdmission>,
    &mut Vec<ScoreComponent>,
);

pub fn evaluate_decision_candidate(
    context: DecisionPipelineContext,
    kind: DecisionCandidateKind,
    admission: Option<&RewardAdmission>,
) -> CandidateEvaluation {
    let candidate = DecisionCandidateIr { kind };
    for pass in filter_passes() {
        if let FilterDecision::InspectOnly(reason) = pass(context, candidate, admission) {
            return CandidateEvaluation::inspect_only(candidate, reason);
        }
    }

    let mut scores = Vec::new();
    for pass in score_passes() {
        pass(context, candidate, admission, &mut scores);
    }
    let saturation = assess_role_saturation(
        context.deck_plan,
        role_saturation_candidate(candidate.kind),
        admission,
    );
    for penalty in &saturation.penalties {
        scores.push(score(
            marginal_reason_label(penalty.reason),
            penalty.score_delta,
        ));
    }
    let adjudication = adjudicate_candidate_lane(
        candidate.kind,
        scores.iter().map(|score| score.value).sum(),
        if context
            .deck_plan
            .candidate_establishes_fuel_backed_second_wind_plan(candidate_card(candidate.kind))
            || admission.is_some_and(|admission| {
                assess_boss_survival_evidence(
                    context.deck_plan,
                    candidate_card(candidate.kind),
                    admission,
                )
                .repairs_plan()
            })
        {
            None
        } else {
            saturation.lane_cap
        },
        strategic_lane_cap(context, candidate.kind, admission),
        acquisition_lane_cap(context, candidate.kind, admission),
    );
    let lane = adjudication.final_lane;
    let expansion = expansion_for_candidate(candidate.kind, lane);
    CandidateEvaluation {
        candidate,
        lane,
        adjudication,
        expansion,
        scores,
    }
}

pub fn candidate_lane_label(lane: CandidateLane) -> &'static str {
    match lane {
        CandidateLane::Mainline => "mainline",
        CandidateLane::Probe => "probe",
        CandidateLane::Skip => "skip",
        CandidateLane::Reject => "reject",
    }
}

pub fn candidate_lane_rank(lane: CandidateLane, has_mainline_take: bool) -> u8 {
    match lane {
        CandidateLane::Mainline => 0,
        CandidateLane::Skip => {
            if has_mainline_take {
                1
            } else {
                0
            }
        }
        CandidateLane::Probe => {
            if has_mainline_take {
                2
            } else {
                1
            }
        }
        CandidateLane::Reject => 3,
    }
}

pub fn candidate_tiebreak_rank(kind: DecisionCandidateKind) -> u8 {
    match kind {
        DecisionCandidateKind::ShopPurge { target } => match target {
            CleanupTarget::Curse => 0,
            CleanupTarget::Status => 1,
            CleanupTarget::StarterStrike => 2,
            CleanupTarget::StarterDefend => 3,
            CleanupTarget::UpgradedStarterStrike => 4,
            CleanupTarget::UpgradedStarterDefend => 5,
            CleanupTarget::OtherStarter => 6,
            CleanupTarget::Other => 7,
        },
        DecisionCandidateKind::ShopOpenRewards => 1,
        DecisionCandidateKind::ShopBuyRelic { .. } => 2,
        DecisionCandidateKind::ShopBuyPotion { .. } => 3,
        DecisionCandidateKind::ShopBuyCard { .. } => 4,
        DecisionCandidateKind::ShopLeave => 5,
        DecisionCandidateKind::CardRewardPick { .. } => 6,
        DecisionCandidateKind::CardRewardSkip => 7,
        DecisionCandidateKind::BossRelicPick { .. } => 8,
        DecisionCandidateKind::BossRelicSkip => 9,
        DecisionCandidateKind::Unsupported => 9,
    }
}

fn filter_passes() -> &'static [FilterPass] {
    &[
        unsupported_candidate_filter,
        missing_card_admission_filter,
        shop_affordability_filter,
        shop_followup_required_filter,
        cleanup_target_filter,
        unmodeled_card_filter,
        thin_support_filter,
        duplicate_marginal_filter,
        unsupported_payoff_filter,
        risky_shop_card_filter,
        shop_card_acquisition_filter,
    ]
}

fn score_passes() -> &'static [ScorePass] {
    &[
        static_candidate_score,
        cleanup_score,
        admission_class_score,
        strategic_deficit_score,
        deck_admission_score,
        construction_pressure_score,
        latent_quality_score,
        reward_reason_score,
        payoff_support_quality_score,
        shop_relic_score,
        shop_potion_score,
        survival_pressure_score,
    ]
}

fn unsupported_candidate_filter(
    _context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    _admission: Option<&RewardAdmission>,
) -> FilterDecision {
    if candidate.kind == DecisionCandidateKind::Unsupported {
        FilterDecision::InspectOnly("unsupported decision candidate")
    } else {
        FilterDecision::Pass
    }
}

fn missing_card_admission_filter(
    _context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
) -> FilterDecision {
    if candidate_requires_card_admission(candidate.kind) && admission.is_none() {
        FilterDecision::InspectOnly("card candidate missing admission")
    } else {
        FilterDecision::Pass
    }
}

fn shop_affordability_filter(
    context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    _admission: Option<&RewardAdmission>,
) -> FilterDecision {
    match (candidate.kind, context.gold) {
        (
            DecisionCandidateKind::ShopBuyCard { price, .. }
            | DecisionCandidateKind::ShopBuyRelic { price, .. }
            | DecisionCandidateKind::ShopBuyPotion { price, .. },
            Some(gold),
        ) if price > gold => FilterDecision::InspectOnly("shop item is unaffordable"),
        _ => FilterDecision::Pass,
    }
}

fn shop_followup_required_filter(
    _context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    _admission: Option<&RewardAdmission>,
) -> FilterDecision {
    match candidate.kind {
        DecisionCandidateKind::ShopBuyRelic { relic, .. }
            if shop_relic_purchase_needs_followup(relic) =>
        {
            FilterDecision::InspectOnly("shop relic purchase opens a follow-up choice")
        }
        _ => FilterDecision::Pass,
    }
}

fn cleanup_target_filter(
    _context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    _admission: Option<&RewardAdmission>,
) -> FilterDecision {
    match candidate.kind {
        DecisionCandidateKind::ShopPurge {
            target: CleanupTarget::Curse | CleanupTarget::Status | CleanupTarget::StarterStrike,
        } => FilterDecision::Pass,
        DecisionCandidateKind::ShopPurge { .. } => {
            FilterDecision::InspectOnly("shop purge target is not safe for tiny owner")
        }
        _ => FilterDecision::Pass,
    }
}

fn unmodeled_card_filter(
    _context: DecisionPipelineContext,
    _candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
) -> FilterDecision {
    if admission.is_some_and(|admission| admission.class == RewardAdmissionClass::EmptyOrDeferred) {
        FilterDecision::InspectOnly("unmodeled or deferred card candidate")
    } else {
        FilterDecision::Pass
    }
}

fn thin_support_filter(
    _context: DecisionPipelineContext,
    _candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
) -> FilterDecision {
    let thin = admission.is_some_and(|admission| {
        admission
            .reasons
            .iter()
            .any(|reason| matches!(reason, RewardAdmissionReason::ThinSupport(_)))
    });
    let has_independent_recovery = admission.is_some_and(|admission| {
        admission
            .reasons
            .contains(&RewardAdmissionReason::RecoverCurrentHp)
    });
    if thin && !has_independent_recovery {
        FilterDecision::InspectOnly("payoff support is too thin")
    } else {
        FilterDecision::Pass
    }
}

fn duplicate_marginal_filter(
    _context: DecisionPipelineContext,
    _candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
) -> FilterDecision {
    let low_marginal = admission.is_some_and(|admission| {
        admission.reasons.iter().any(|reason| {
            matches!(reason, RewardAdmissionReason::DuplicateBurden(_))
                || matches!(
                    reason,
                    RewardAdmissionReason::DuplicateConcern(concern)
                        if concern.is_hard_penalty()
                )
        })
    });
    if low_marginal {
        FilterDecision::InspectOnly("duplicate marginal value is too low")
    } else {
        FilterDecision::Pass
    }
}

fn unsupported_payoff_filter(
    _context: DecisionPipelineContext,
    _candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
) -> FilterDecision {
    if admission
        .is_some_and(|admission| admission.class == RewardAdmissionClass::OpensUnsupportedPayoff)
    {
        FilterDecision::InspectOnly("unsupported payoff candidate")
    } else {
        FilterDecision::Pass
    }
}

fn risky_shop_card_filter(
    context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
) -> FilterDecision {
    let DecisionCandidateKind::ShopBuyCard { card, upgrades, .. } = candidate.kind else {
        return FilterDecision::Pass;
    };
    if is_boss_answer_shop_card(context, candidate.kind, admission) {
        return FilterDecision::Pass;
    }
    let definition = card_definition_with_upgrades(card, upgrades);
    let card_risk = definition.burdens.iter().any(|burden| {
        matches!(
            burden,
            CardBurden::RandomExhaust
                | CardBurden::AddsCombatDeckClutter
                | CardBurden::HpCost
                | CardBurden::DrawLockout
                | CardBurden::ExhaustsHand
        )
    });
    let duplicate_risk = admission.is_some_and(|admission| {
        admission.reasons.iter().any(|reason| {
            matches!(
                reason,
                RewardAdmissionReason::DuplicateBurden(_)
                    | RewardAdmissionReason::DuplicateConcern(_)
            )
        })
    });
    if card_risk || duplicate_risk {
        FilterDecision::InspectOnly("shop card buy carries unresolved risk")
    } else {
        FilterDecision::Pass
    }
}

fn is_boss_answer_shop_card(
    context: DecisionPipelineContext,
    kind: DecisionCandidateKind,
    admission: Option<&RewardAdmission>,
) -> bool {
    if !matches!(kind, DecisionCandidateKind::ShopBuyCard { .. }) {
        return false;
    }
    let Some(admission) = admission else {
        return false;
    };
    let scaling = assess_boss_scaling_evidence(context.deck_plan, candidate_card(kind), admission);
    let survival =
        assess_boss_survival_evidence(context.deck_plan, candidate_card(kind), admission);
    (needs(context.deck_plan.strategic_deficit.boss_scaling_plan)
        && scaling.relevant_to_boss_plan
        && !fragile_supported_payoff(context, admission))
        || survival.repairs_plan()
}

fn shop_card_acquisition_filter(
    context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
) -> FilterDecision {
    let DecisionCandidateKind::ShopBuyCard {
        card,
        upgrades,
        price,
    } = candidate.kind
    else {
        return FilterDecision::Pass;
    };
    let (Some(admission), Some(gold)) = (admission, context.gold) else {
        return FilterDecision::Pass;
    };
    if is_boss_answer_shop_card(context, candidate.kind, Some(admission)) {
        return FilterDecision::Pass;
    }
    let report = assess_card_acquisition(
        AcquisitionContext::shop_with_purge_reserve(
            context.deck_plan,
            gold,
            price,
            context.purge_reserve,
        ),
        card,
        upgrades,
        admission,
    );
    let policy = evaluate_deck_construction_contract(&report);
    match policy.inspect_only_reason() {
        None => FilterDecision::Pass,
        Some(reason) => FilterDecision::InspectOnly(reason),
    }
}

fn static_candidate_score(
    _context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    _admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    match candidate.kind {
        DecisionCandidateKind::CardRewardSkip => scores.push(score("skip", 0)),
        DecisionCandidateKind::ShopOpenRewards => scores.push(score("open-rewards", 300)),
        DecisionCandidateKind::ShopLeave => scores.push(score("leave", 0)),
        _ => {}
    }
}

fn cleanup_score(
    context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    _admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    let DecisionCandidateKind::ShopPurge { target } = candidate.kind else {
        return;
    };
    scores.push(score(
        "cleanup-target",
        match target {
            CleanupTarget::Curse => 320,
            CleanupTarget::Status => 260,
            CleanupTarget::StarterStrike => 180,
            CleanupTarget::UpgradedStarterStrike
            | CleanupTarget::StarterDefend
            | CleanupTarget::UpgradedStarterDefend
            | CleanupTarget::OtherStarter
            | CleanupTarget::Other => 0,
        },
    ));
    if target == CleanupTarget::StarterStrike
        && context.deck_plan.context.act == 1
        && context.deck_plan.construction.frontload == FrontloadPressure::NeedMore
    {
        scores.push(score("early-frontload-preservation", -150));
    }
}

fn admission_class_score(
    _context: DecisionPipelineContext,
    _candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    let Some(admission) = admission else {
        return;
    };
    scores.push(score(
        "admission-class",
        match admission.class {
            RewardAdmissionClass::ClosesRequirement => 130,
            RewardAdmissionClass::BuildsSupportedPackage => 105,
            RewardAdmissionClass::EngineSeed => 65,
            RewardAdmissionClass::ImmediateWork => 55,
            RewardAdmissionClass::BurdenedImmediateWork => 25,
            RewardAdmissionClass::OpensUnsupportedPayoff
            | RewardAdmissionClass::EmptyOrDeferred
            | RewardAdmissionClass::Skip => 0,
        },
    ));
}

fn strategic_deficit_score(
    context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    if !matches!(
        candidate.kind,
        DecisionCandidateKind::CardRewardPick { .. } | DecisionCandidateKind::ShopBuyCard { .. }
    ) {
        return;
    }
    let Some(admission) = admission else {
        return;
    };
    let deficit = context.deck_plan.strategic_deficit;

    if needs(deficit.deck_access)
        && (admission_provides(admission, Mechanic::CardDraw)
            || admission
                .reasons
                .contains(&RewardAdmissionReason::CombatUpgrade))
    {
        scores.push(score("strategic-access-gap", 55));
    }
    if needs(deficit.energy_or_playability) && admission_provides(admission, Mechanic::Energy) {
        scores.push(score("strategic-energy-gap", 50));
    }
    if context
        .deck_plan
        .candidate_improves_aoe_gap(candidate_card(candidate.kind), admission)
    {
        scores.push(score("strategic-aoe-gap", 50));
    }
    if context
        .deck_plan
        .candidate_establishes_fuel_backed_second_wind_plan(candidate_card(candidate.kind))
    {
        scores.push(score("fuel-backed-second-wind-block-plan", 90));
    }
    if needs(deficit.block_or_mitigation) && admission_survival_tool(admission) {
        scores.push(score("strategic-survival-gap", 40));
    }
    if needs(deficit.boss_scaling_plan) {
        let evidence = assess_boss_scaling_evidence(
            context.deck_plan,
            candidate_card(candidate.kind),
            admission,
        );
        if evidence.score_delta != 0 {
            scores.push(score(evidence.label, evidence.score_delta));
        }
    }
    let survival_evidence =
        assess_boss_survival_evidence(context.deck_plan, candidate_card(candidate.kind), admission);
    if survival_evidence.score_delta != 0 {
        scores.push(score(
            survival_evidence.label,
            survival_evidence.score_delta,
        ));
    }
    if needs(deficit.frontload_damage) && admission_frontloads(admission) {
        scores.push(score("strategic-frontload-gap", 35));
    }
    if deficit.too_many_low_impact_attacks
        && deficit.frontload_damage == StrategicDeficitLevel::Surplus
        && admission_frontloads(admission)
        && !admission_aoe(admission)
        && !admission_scaling_or_engine(admission)
        && !admission_provides(admission, Mechanic::CardDraw)
    {
        scores.push(score("strategic-frontload-saturated", -65));
    }
}

fn deck_admission_score(
    context: DecisionPipelineContext,
    _candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    let Some(admission) = admission else {
        return;
    };
    scores.push(score(
        "deck-admission",
        match context.deck_plan.deck_admission(admission) {
            DeckAdmission::Welcome => 0,
            DeckAdmission::Conditional => -30,
            DeckAdmission::Discouraged => -90,
        },
    ));
}

fn construction_pressure_score(
    context: DecisionPipelineContext,
    _candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    let Some(admission) = admission else {
        return;
    };
    scores.push(score(
        "construction-pressure",
        match context.deck_plan.reward_lane_adjustment(admission) {
            ConstructionLaneAdjustment::None => 0,
            ConstructionLaneAdjustment::PromoteOneStep => 35,
            ConstructionLaneAdjustment::PromoteToMainline => 70,
            ConstructionLaneAdjustment::SoftDemote => -45,
            ConstructionLaneAdjustment::HardDemote => -130,
        },
    ));
}

fn latent_quality_score(
    context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    if !matches!(
        candidate.kind,
        DecisionCandidateKind::CardRewardPick { .. } | DecisionCandidateKind::ShopBuyCard { .. }
    ) {
        return;
    }
    let Some(admission) = admission else {
        return;
    };
    if has_combat_upgrade(admission) && context.deck_plan.roles.upgrade_access_units == 0 {
        let value = if context.deck_plan.context.act <= 1 {
            55
        } else {
            35
        };
        scores.push(score("latent-upgrade-leverage", value));
    }
    if has_duplicate_access_copy(admission) {
        scores.push(score("marginal-access-copy", -25));
    }
}

fn reward_reason_score(
    context: DecisionPipelineContext,
    _candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    let Some(admission) = admission else {
        return;
    };
    for reason in &admission.reasons {
        match *reason {
            RewardAdmissionReason::Closes(_) => scores.push(score("closes-requirement", 85)),
            RewardAdmissionReason::Supports(_) => scores.push(score("supports-package", 65)),
            RewardAdmissionReason::Provides(Mechanic::CardDraw | Mechanic::Energy) => {
                scores.push(score("access", 45))
            }
            RewardAdmissionReason::Provides(
                Mechanic::Block
                | Mechanic::Weak
                | Mechanic::EnemyStrengthDown
                | Mechanic::TemporaryEnemyStrengthDown,
            ) => scores.push(score("survival-tool", 35)),
            RewardAdmissionReason::Provides(Mechanic::Strength | Mechanic::StrengthMultiplier) => {
                scores.push(score("scaling-tool", 30))
            }
            RewardAdmissionReason::FrontloadDamage => scores.push(score("frontload", 25)),
            RewardAdmissionReason::AreaDamage => scores.push(score("aoe", 45)),
            RewardAdmissionReason::CombatUpgrade => scores.push(score("combat-upgrade", 45)),
            RewardAdmissionReason::RunReward(_) => scores.push(score("run-reward", 40)),
            RewardAdmissionReason::Installs(_) => scores.push(score("installed-rule", 50)),
            RewardAdmissionReason::Burden(CardBurden::AddsCombatDeckClutter)
                if context.deck_plan.roles.status_digest_units > 0 =>
            {
                scores.push(score("status-liability-digested", 0))
            }
            RewardAdmissionReason::Burden(burden) => {
                scores.push(score("burden", burden_score(burden)))
            }
            _ => {}
        }
    }
}

fn payoff_support_quality_score(
    context: DecisionPipelineContext,
    _candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    let Some(admission) = admission else {
        return;
    };
    if fragile_supported_payoff(context, admission) {
        scores.push(score("payoff-support-fragile", -80));
    }
}

fn shop_relic_score(
    context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    _admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    let DecisionCandidateKind::ShopBuyRelic { relic, .. } = candidate.kind else {
        return;
    };
    for component in shop_relic_score_components(context.deck_plan, relic) {
        scores.push(component);
    }
}

fn shop_potion_score(
    context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    _admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    let DecisionCandidateKind::ShopBuyPotion { potion, .. } = candidate.kind else {
        return;
    };
    let mut value = shop_potion_score_value(potion);
    if context.deck_plan.survival_pressure() && value > 0 && potion != PotionId::ExplosivePotion {
        value += 40;
    }
    scores.push(score("shop-potion", value));
}

fn survival_pressure_score(
    context: DecisionPipelineContext,
    candidate: DecisionCandidateIr,
    admission: Option<&RewardAdmission>,
    scores: &mut Vec<ScoreComponent>,
) {
    if !context.deck_plan.survival_pressure() {
        return;
    }
    let Some(admission) = admission else {
        return;
    };
    let provides_block = admission_provides(admission, Mechanic::Block);
    let provides_draw = admission_provides(admission, Mechanic::CardDraw);
    let mitigates = admission_provides(admission, Mechanic::Weak)
        || admission_provides(admission, Mechanic::EnemyStrengthDown)
        || admission_provides(admission, Mechanic::TemporaryEnemyStrengthDown);
    scores.push(score(
        "survival-pressure",
        if provides_block && provides_draw {
            65
        } else if mitigates {
            55
        } else if provides_block {
            40
        } else if provides_draw {
            25
        } else if admission_provides(admission, Mechanic::Energy) {
            15
        } else {
            0
        },
    ));
    let block_density = if provides_block {
        candidate_card(candidate.kind)
            .map(|(card, upgrades)| acute_survival_block_density(card, upgrades))
            .unwrap_or(0)
    } else {
        0
    };
    if block_density > 0 {
        scores.push(score("acute-survival-block-density", block_density));
    }
}

fn acute_survival_block_density(card: CardId, upgrades: u8) -> i32 {
    let analysis = card_analysis_profile_v1(card, upgrades);
    if analysis.is_second_wind_source {
        return 0;
    }
    match analysis.block_chunk {
        CardAnalysisBlockChunkV1::Burst => 25,
        CardAnalysisBlockChunkV1::None
        | CardAnalysisBlockChunkV1::Low
        | CardAnalysisBlockChunkV1::Solid => 0,
    }
}

fn lane_for_candidate(kind: DecisionCandidateKind, score: i32) -> CandidateLane {
    match kind {
        DecisionCandidateKind::CardRewardSkip
        | DecisionCandidateKind::BossRelicSkip
        | DecisionCandidateKind::ShopLeave => CandidateLane::Skip,
        _ if score >= 110 => CandidateLane::Mainline,
        _ if score >= 45 => CandidateLane::Probe,
        _ => CandidateLane::Reject,
    }
}

fn adjudicate_candidate_lane(
    kind: DecisionCandidateKind,
    score: i32,
    role_saturation: Option<LaneCap>,
    strategic: Option<LaneCap>,
    acquisition: Option<LaneCap>,
) -> CandidateLaneAdjudication {
    let raw_lane = lane_for_candidate(kind, score);
    let caps = lane_caps(role_saturation, strategic, acquisition);
    let cap = caps
        .iter()
        .fold(None, |cap, next| stricter_lane_cap(cap, Some(next.cap)));
    CandidateLaneAdjudication {
        raw_lane,
        final_lane: capped_lane(raw_lane, cap),
        caps,
    }
}

fn lane_caps(
    role_saturation: Option<LaneCap>,
    strategic: Option<LaneCap>,
    acquisition: Option<LaneCap>,
) -> Vec<CandidateLaneCap> {
    [
        (CandidateLaneCapSource::RoleSaturation, role_saturation),
        (CandidateLaneCapSource::Strategic, strategic),
        (CandidateLaneCapSource::Acquisition, acquisition),
    ]
    .into_iter()
    .filter_map(|(source, cap)| cap.map(|cap| CandidateLaneCap { source, cap }))
    .collect()
}

fn capped_lane(lane: CandidateLane, cap: Option<LaneCap>) -> CandidateLane {
    match (lane, cap) {
        (CandidateLane::Mainline, Some(LaneCap::ProbeOnly)) => CandidateLane::Probe,
        (CandidateLane::Mainline | CandidateLane::Probe, Some(LaneCap::Reject)) => {
            CandidateLane::Reject
        }
        _ => lane,
    }
}

fn stricter_lane_cap(left: Option<LaneCap>, right: Option<LaneCap>) -> Option<LaneCap> {
    match (left, right) {
        (Some(LaneCap::Reject), _) | (_, Some(LaneCap::Reject)) => Some(LaneCap::Reject),
        (Some(LaneCap::ProbeOnly), _) | (_, Some(LaneCap::ProbeOnly)) => Some(LaneCap::ProbeOnly),
        _ => None,
    }
}

fn strategic_lane_cap(
    context: DecisionPipelineContext,
    kind: DecisionCandidateKind,
    admission: Option<&RewardAdmission>,
) -> Option<LaneCap> {
    if !matches!(
        kind,
        DecisionCandidateKind::CardRewardPick { .. } | DecisionCandidateKind::ShopBuyCard { .. }
    ) {
        return None;
    }
    let admission = admission?;
    if acute_survival_setup_only(context, kind, admission) {
        return Some(LaneCap::ProbeOnly);
    }
    // The acquisition contract owns marginal construction value and deck
    // burden. A second coarse "no strategic gap" veto would suppress useful
    // roles it explicitly accepts, such as first persistent mitigation.
    None
}

fn acquisition_lane_cap(
    context: DecisionPipelineContext,
    kind: DecisionCandidateKind,
    admission: Option<&RewardAdmission>,
) -> Option<LaneCap> {
    if is_boss_answer_shop_card(context, kind, admission) {
        return None;
    }
    if context
        .deck_plan
        .candidate_establishes_fuel_backed_second_wind_plan(candidate_card(kind))
    {
        return None;
    }
    let admission = admission?;
    let (card, upgrades, source) = match kind {
        DecisionCandidateKind::CardRewardPick { card, upgrades } => (
            card,
            upgrades,
            AcquisitionContext::reward(context.deck_plan),
        ),
        DecisionCandidateKind::ShopBuyCard {
            card,
            upgrades,
            price,
        } => {
            let gold = context.gold?;
            (
                card,
                upgrades,
                AcquisitionContext::shop_with_purge_reserve(
                    context.deck_plan,
                    gold,
                    price,
                    context.purge_reserve,
                ),
            )
        }
        _ => return None,
    };
    let report = assess_card_acquisition(source, card, upgrades, admission);
    let policy = evaluate_deck_construction_contract(&report);
    match policy.verdict {
        AcquisitionPolicyVerdict::ContextTake => None,
        AcquisitionPolicyVerdict::Speculative
        | AcquisitionPolicyVerdict::SkipPreferred
        | AcquisitionPolicyVerdict::Unverified => Some(LaneCap::ProbeOnly),
        AcquisitionPolicyVerdict::Reject => Some(LaneCap::Reject),
    }
}

fn role_saturation_candidate(kind: DecisionCandidateKind) -> Option<RoleSaturationCandidate> {
    match kind {
        DecisionCandidateKind::CardRewardPick { upgrades, .. } => Some(RoleSaturationCandidate {
            upgrades,
            is_shop_card: false,
        }),
        DecisionCandidateKind::ShopBuyCard { upgrades, .. } => Some(RoleSaturationCandidate {
            upgrades,
            is_shop_card: true,
        }),
        _ => None,
    }
}

fn expansion_for_candidate(kind: DecisionCandidateKind, lane: CandidateLane) -> ExpansionPlan {
    match (kind, lane) {
        (_, CandidateLane::Reject) => ExpansionPlan::InspectOnly("candidate score rejected"),
        (DecisionCandidateKind::CardRewardPick { .. }, CandidateLane::Probe) => {
            ExpansionPlan::InspectOnly("card reward pick is below mainline")
        }
        (DecisionCandidateKind::ShopBuyCard { .. }, CandidateLane::Probe) => {
            ExpansionPlan::InspectOnly("shop card buy is below mainline")
        }
        (DecisionCandidateKind::ShopBuyRelic { .. }, CandidateLane::Probe) => {
            ExpansionPlan::InspectOnly("shop relic buy is below mainline")
        }
        (DecisionCandidateKind::ShopBuyPotion { .. }, CandidateLane::Probe) => {
            ExpansionPlan::InspectOnly("shop potion buy is below mainline")
        }
        _ => ExpansionPlan::Auto,
    }
}

fn candidate_requires_card_admission(kind: DecisionCandidateKind) -> bool {
    matches!(
        kind,
        DecisionCandidateKind::CardRewardPick { .. } | DecisionCandidateKind::ShopBuyCard { .. }
    )
}

fn burden_score(burden: CardBurden) -> i32 {
    match burden {
        CardBurden::PowerSetup => -10,
        CardBurden::HpCost => -35,
        CardBurden::DrawLockout => -30,
        CardBurden::AddsCombatDeckClutter => -35,
        CardBurden::RandomExhaust => -30,
        CardBurden::ExhaustsHand => -45,
        CardBurden::RequiresEnemyAttackIntent => -15,
        CardBurden::CardBlockLockoutUntilNextTurn => -25,
    }
}

fn needs(level: StrategicDeficitLevel) -> bool {
    matches!(
        level,
        StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
    )
}

fn acute_survival_setup_only(
    context: DecisionPipelineContext,
    kind: DecisionCandidateKind,
    admission: &RewardAdmission,
) -> bool {
    if !context.deck_plan.survival_pressure()
        || admission.class == RewardAdmissionClass::ClosesRequirement
        || context
            .deck_plan
            .repairs_strength_package_reliability(candidate_card(kind))
        || context
            .deck_plan
            .candidate_establishes_fuel_backed_second_wind_plan(candidate_card(kind))
        || assess_boss_survival_evidence(context.deck_plan, candidate_card(kind), admission)
            .repairs_plan()
    {
        return false;
    }

    let immediate_survival_or_access = admission_survival_tool(admission)
        || admission_frontloads(admission)
        || admission_provides(admission, Mechanic::CardDraw)
        || admission_provides(admission, Mechanic::Energy)
        || admission
            .reasons
            .contains(&RewardAdmissionReason::RecoverCurrentHp);
    if immediate_survival_or_access {
        return false;
    }

    let setup_burden = candidate_card(kind).is_some_and(|(card, upgrades)| {
        card_definition_with_upgrades(card, upgrades)
            .burdens
            .contains(&CardBurden::PowerSetup)
    });
    setup_burden || admission_scaling_or_engine(admission)
}

fn admission_provides(admission: &RewardAdmission, mechanic: Mechanic) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::Provides(mechanic))
}

fn candidate_card(kind: DecisionCandidateKind) -> Option<(CardId, u8)> {
    match kind {
        DecisionCandidateKind::CardRewardPick { card, upgrades }
        | DecisionCandidateKind::ShopBuyCard { card, upgrades, .. } => Some((card, upgrades)),
        _ => None,
    }
}

fn admission_frontloads(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::FrontloadDamage)
}

fn admission_aoe(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::AreaDamage)
}

fn admission_damage_uses(admission: &RewardAdmission, mechanic: Mechanic) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::DamageUses(mechanic))
}

fn has_combat_upgrade(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::CombatUpgrade)
}

fn has_duplicate_access_copy(admission: &RewardAdmission) -> bool {
    admission.reasons.iter().any(|reason| {
        matches!(
            reason,
            RewardAdmissionReason::DuplicateConcern(RewardDuplicateConcern::DiminishingAccessCopy)
        )
    })
}

fn admission_survival_tool(admission: &RewardAdmission) -> bool {
    admission_provides(admission, Mechanic::Block)
        || admission_provides(admission, Mechanic::Weak)
        || admission_provides(admission, Mechanic::EnemyStrengthDown)
        || admission_provides(admission, Mechanic::TemporaryEnemyStrengthDown)
}

fn admission_scaling_or_engine(admission: &RewardAdmission) -> bool {
    admission_provides(admission, Mechanic::Strength)
        || admission_provides(admission, Mechanic::StrengthMultiplier)
        || admission.reasons.iter().any(|reason| {
            matches!(
                reason,
                RewardAdmissionReason::Closes(_)
                    | RewardAdmissionReason::Supports(_)
                    | RewardAdmissionReason::Installs(_)
                    | RewardAdmissionReason::DamageScalesWith(_)
                    | RewardAdmissionReason::RunReward(_)
            )
        })
}

fn fragile_supported_payoff(context: DecisionPipelineContext, admission: &RewardAdmission) -> bool {
    if admission
        .reasons
        .contains(&RewardAdmissionReason::RecoverCurrentHp)
    {
        return false;
    }
    if !admission
        .reasons
        .iter()
        .any(|reason| matches!(reason, RewardAdmissionReason::Supports(_)))
    {
        return false;
    }
    if context
        .deck_plan
        .has_supported_hand_exhaust_conversion(admission)
    {
        return false;
    }
    if admission_is_strength_payoff(admission) {
        return !context.deck_plan.has_open_stable_strength_payoff_slot();
    }
    if admission_damage_uses(admission, Mechanic::Block) {
        return context.deck_plan.block_plan_readiness
            < crate::ai::block_plan_profile_v1::BlockPlanReadinessV1::Supported;
    }
    false
}

fn shop_relic_purchase_needs_followup(relic: RelicId) -> bool {
    matches!(
        relic,
        RelicId::BottledFlame
            | RelicId::BottledLightning
            | RelicId::BottledTornado
            | RelicId::Cauldron
            | RelicId::DollysMirror
            | RelicId::Orrery
    )
}

fn shop_relic_score_components(
    deck_plan: DeckPlanSnapshot,
    relic: RelicId,
) -> impl Iterator<Item = ScoreComponent> {
    let mut scores = vec![score("shop-relic", shop_relic_score_value(relic))];
    match relic {
        RelicId::ChemicalX if deck_plan.roles.x_cost_payoff_units > 0 => {
            scores.push(score("shop-relic-x-cost-payoff", 75));
        }
        RelicId::ChemicalX => scores.push(score("shop-relic-x-cost-missing", -40)),
        RelicId::PaperFrog => {
            if deck_plan.roles.vulnerable_units >= 2 {
                scores.push(score("shop-relic-vulnerable-density", 70));
            } else if deck_plan.roles.vulnerable_units == 1 {
                scores.push(score("shop-relic-vulnerable-source", 35));
            }
            if deck_plan.roles.vulnerable_units > 0 && deck_plan.roles.aoe_units > 0 {
                scores.push(score("shop-relic-vulnerable-aoe", 25));
            }
        }
        _ => {}
    }
    scores.into_iter()
}

fn shop_relic_score_value(relic: RelicId) -> i32 {
    match relic {
        RelicId::Waffle => 220,
        RelicId::MedicalKit | RelicId::OrangePellets => 150,
        RelicId::MembershipCard => 0,
        RelicId::ClockworkSouvenir | RelicId::Toolbox => 115,
        RelicId::ChemicalX => 45,
        RelicId::FrozenEye => 45,
        _ => 45,
    }
}

fn shop_potion_score_value(potion: PotionId) -> i32 {
    match potion {
        PotionId::PowerPotion => 100,
        PotionId::FairyPotion | PotionId::FruitJuice | PotionId::EntropicBrew => 90,
        PotionId::FirePotion
        | PotionId::ExplosivePotion
        | PotionId::FearPotion
        | PotionId::WeakenPotion
        | PotionId::EnergyPotion
        | PotionId::StrengthPotion
        | PotionId::SteroidPotion
        | PotionId::SwiftPotion
        | PotionId::BlessingOfTheForge
        | PotionId::LiquidMemories
        | PotionId::GamblersBrew
        | PotionId::DuplicationPotion
        | PotionId::AttackPotion
        | PotionId::SkillPotion
        | PotionId::ColorlessPotion => 70,
        PotionId::BlockPotion
        | PotionId::DexterityPotion
        | PotionId::SpeedPotion
        | PotionId::AncientPotion
        | PotionId::RegenPotion
        | PotionId::EssenceOfSteel
        | PotionId::LiquidBronze
        | PotionId::BloodPotion => 55,
        _ => 35,
    }
}

fn score(by: &'static str, value: i32) -> ScoreComponent {
    ScoreComponent { by, value }
}

#[cfg(test)]
mod tests;
