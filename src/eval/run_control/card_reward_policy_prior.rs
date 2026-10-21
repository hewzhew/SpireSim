use std::cmp::Ordering;
use std::collections::BTreeSet;

use serde::Serialize;

use crate::ai::action_supply_v1::additional_play_target_support_v1;
use crate::ai::analysis::card_semantics::{card_definition_with_upgrades, DuplicateBehavior};
use crate::ai::block_plan_profile_v1::{block_plan_profile_v1, BlockPlanProfileV1};
use crate::ai::boss_mechanics_v1::{
    boss_mechanic_pressure_profile_v1, BossEncounterTargetTopologyV1, BossMechanicPressurePointV1,
};
use crate::ai::card_analysis_v1::{
    card_analysis_profile_v1, CardAnalysisAoeSupportV1, CardAnalysisAttackChunkV1,
};
use crate::ai::card_component_signal_v1::{
    evaluate_card_component_signals_v1, is_concrete_package_support_signal_v1,
    is_unresolved_package_payoff_debt_signal_v1, CardComponentSignalContextV1,
    CardComponentSignalKindV1, CardComponentSignalReportV1,
};
use crate::ai::card_semantics_v1::{
    card_access_evidence_v1, card_mechanics_profile_v1, card_reward_facts_v1,
    card_reward_semantic_profile_v1, CardAccessEvidenceV1, CardAccessLeverageV1,
    CardRewardPickDependencyV1, CardRewardSemanticProfileV1, CardRewardSemanticRoleV1,
};
use crate::ai::combat_upgrade_coverage_v1::CombatUpgradeScopeV1;
use crate::ai::deck_shape_v1::{
    deck_shape_candidate_delta_v1, deck_shape_profile_v1,
    persistent_draw_pile_status_assessment_v1, DeckShapeProfileV1, DeckShapeRiskV1,
    PersistentDrawPileStatusAssessmentV1, PersistentDrawPileStatusHandlingV1,
};
use crate::ai::deck_startup_profile_v1::{deck_startup_profile_v1, DeckStartupProfileV1};
use crate::ai::noncombat_strategy_v1::{
    build_run_strategy_snapshot_from_run_state_v2, threat_relevant_capability_improvements_v1,
    StrategyCapabilityCoverageV1, StrategyCapabilityKindV1, StrategyPackageIdV2,
    StrategyPlanSupportV1, StrategyThreatSourceV1,
};
use crate::ai::strategy::acquisition::{
    assess_card_acquisition_from_construction_v1,
    assess_card_construction_with_component_signals_v1, evaluate_deck_construction_contract,
    AcquisitionConstructionRole, AcquisitionContext, AcquisitionPolicyDecision,
    CardConstructionAssessmentV1,
};
use crate::ai::strategy::boss_damage_plan::{
    assess_boss_damage_plan_v1, BossDamagePlanEngineReliabilityV1, BossDamagePlanReadinessV1,
};
use crate::ai::strategy::boss_survival_evidence::assess_boss_survival_evidence;
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::power_tempo::{mummified_hand_power_tempo_v1, MummifiedHandPowerTempoV1};
use crate::ai::strategy::reward_admission::assess_reward_admission_from_master_deck;
use crate::ai::strength_profile_v1::card_unlocks_convertible_strength_payoff_v1;
use crate::ai::upgrade_planner_v1::plan_upgrades_v1;
use crate::content::cards::{get_card_definition, CardId};
use crate::runtime::combat::CombatCard;
use crate::state::rewards::RewardCard;

use super::{
    exact_run_policy_decision_v1, owned_run_policy_actions_v1,
    positive_grouped_run_policy_prior_v1, run_policy_state_delta_v1, DecisionCandidateKey,
    ExactRunPolicyActionSuccessorV1, ExactRunPolicyDecisionV1, RunControlSession,
    RunPolicyCandidateV1, RunPolicyPriorV1, RunPolicyStateDeltaV1,
};

pub const EXACT_CARD_REWARD_POLICY_AUDIT_SCHEMA_NAME: &str = "ExactCardRewardPolicyAudit";
pub const EXACT_CARD_REWARD_POLICY_AUDIT_SCHEMA_VERSION: u32 = 13;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardRewardPolicyBandV1 {
    ResolvePendingBoundary,
    CloseThreatGap,
    AmplifyStrategicAccess,
    ImproveRequiredCapability,
    EstablishStrategicAsset,
    PreserveDeckQuality,
    SpeculativeAddition,
    Liability,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CardRewardPolicyAcquisitionV1 {
    Card {
        card: CardId,
        upgrades: u8,
        copies_before: usize,
        semantics: CardRewardSemanticProfileV1,
        access: Option<CardAccessEvidenceV1>,
        component_signals: CardComponentSignalReportV1,
        construction: CardConstructionAssessmentV1,
    },
    SingingBowl,
    Skip,
    OpenReward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardRewardBossDamagePlanImprovementV1 {
    pub before_readiness: BossDamagePlanReadinessV1,
    pub after_readiness: BossDamagePlanReadinessV1,
    pub before_reliability: BossDamagePlanEngineReliabilityV1,
    pub after_reliability: BossDamagePlanEngineReliabilityV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardRewardPolicyActionEvidenceV1 {
    pub candidate_id: String,
    pub candidate_key: DecisionCandidateKey,
    pub acquisition: CardRewardPolicyAcquisitionV1,
    pub band: CardRewardPolicyBandV1,
    pub delta: RunPolicyStateDeltaV1,
    pub introduces_undigested_status_burden: bool,
    pub duplicate_low_marginal: bool,
    /// Shared source-aware verdict layered over the source-independent
    /// construction assessment carried by `acquisition`.
    pub card_acquisition_policy: Option<AcquisitionPolicyDecision>,
    pub access_conflict_or_redundancy: bool,
    pub mummified_hand_power_tempo: Option<MummifiedHandPowerTempoV1>,
    pub mummified_hand_stacking_duplicate_support: bool,
    /// The known act boss punishes playing powers and this candidate is a
    /// shared-analysis minor power without an exact strategic improvement.
    pub boss_power_tax_conflict: bool,
    pub random_target_frontload_reliable: bool,
    /// Exact deck-shape liabilities introduced by this candidate.
    ///
    /// This consumes the shared deck-shape model instead of allowing the
    /// reward owner to rediscover card-specific playability and saturation
    /// rules through coarse threat thresholds.
    pub added_deck_shape_risks: Vec<DeckShapeRiskV1>,
    /// Candidate-local assessment of persistent statuses shuffled directly
    /// into the draw pile and the owned deck's typed ways to absorb them.
    pub persistent_draw_pile_status: Option<PersistentDrawPileStatusAssessmentV1>,
    pub improves_threat_relevant_capability: bool,
    pub amplifies_existing_answers: bool,
    /// A shared boss-damage-plan improvement introduced by this exact card.
    /// This is a same-band tie-break, not a card-specific priority override.
    pub boss_damage_plan_improvement: Option<CardRewardBossDamagePlanImprovementV1>,
    /// A shared boss-survival-plan repair introduced by this exact card.
    ///
    /// The label comes from `boss_survival_evidence`; the reward owner does
    /// not maintain a second boss/card rule table.
    pub boss_survival_plan_repair: Option<String>,
    /// Exact combat-upgrade access before and after this candidate.
    pub combat_upgrade_scope_before: Option<CombatUpgradeScopeV1>,
    pub combat_upgrade_scope_after: Option<CombatUpgradeScopeV1>,
    /// Upgradeable cards already present before adding the candidate. Future
    /// unknown rewards are deliberately not counted as current support.
    pub combat_upgrade_target_count_before: usize,
    /// Support for the route resource required by an upgrade-investment card.
    ///
    /// `None` means the candidate has no such dependency.  Keeping this typed
    /// and visible prevents an immediate-damage delta from silently erasing an
    /// unfunded long-term commitment.
    pub upgrade_investment_support: Option<StrategyPlanSupportV1>,
    surface_index: usize,
}

#[derive(Clone, Debug)]
pub struct ExactCardRewardPolicyDecisionV1 {
    pub exact: ExactRunPolicyDecisionV1,
    pub evidence: Vec<CardRewardPolicyActionEvidenceV1>,
    pub prior: RunPolicyPriorV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardRewardPolicyAuditCandidateV1 {
    /// Zero-based semantic preference group. Equal ranks are intentional;
    /// exact-surface order only supplies a deterministic action within a tie.
    pub owner_rank: usize,
    pub candidate_id: String,
    pub label: String,
    pub candidate_key: DecisionCandidateKey,
    pub acquisition: CardRewardPolicyAcquisitionV1,
    pub band: CardRewardPolicyBandV1,
    pub closed_threat_gaps: Vec<super::RunPolicyThreatGapKeyV1>,
    pub capability_improvements: Vec<super::RunPolicyCapabilityChangeV1>,
    pub capability_rule_changes: Vec<super::RunPolicyCapabilityRuleChangeV1>,
    pub resolved_formation_needs:
        Vec<crate::ai::noncombat_strategy_v1::StrategyDeckFormationNeedV1>,
    pub added_formation_strengths: Vec<StrategyPackageIdV2>,
    pub introduces_undigested_status_burden: bool,
    pub duplicate_low_marginal: bool,
    pub card_acquisition_policy: Option<AcquisitionPolicyDecision>,
    pub access_conflict_or_redundancy: bool,
    pub mummified_hand_power_tempo: Option<MummifiedHandPowerTempoV1>,
    pub mummified_hand_stacking_duplicate_support: bool,
    pub boss_power_tax_conflict: bool,
    pub random_target_frontload_reliable: bool,
    pub added_deck_shape_risks: Vec<DeckShapeRiskV1>,
    pub persistent_draw_pile_status: Option<PersistentDrawPileStatusAssessmentV1>,
    pub improves_threat_relevant_capability: bool,
    pub amplifies_existing_answers: bool,
    pub boss_damage_plan_improvement: Option<CardRewardBossDamagePlanImprovementV1>,
    pub boss_survival_plan_repair: Option<String>,
    pub combat_upgrade_scope_before: Option<CombatUpgradeScopeV1>,
    pub combat_upgrade_scope_after: Option<CombatUpgradeScopeV1>,
    pub combat_upgrade_target_count_before: usize,
    pub upgrade_investment_support: Option<StrategyPlanSupportV1>,
    pub surface_index: usize,
    pub prior_probability: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactCardRewardPolicyAuditV1 {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub candidates: Vec<CardRewardPolicyAuditCandidateV1>,
}

impl ExactCardRewardPolicyDecisionV1 {
    pub fn audit(
        &self,
        legal: &[RunPolicyCandidateV1<'_>],
    ) -> Result<ExactCardRewardPolicyAuditV1, String> {
        let mut previous = None;
        let mut owner_rank = 0;
        let candidates = self
            .evidence
            .iter()
            .map(|evidence| {
                if previous.is_some_and(|previous| {
                    compare_card_reward_semantic_evidence(previous, evidence) != Ordering::Equal
                }) {
                    owner_rank += 1;
                }
                previous = Some(evidence);
                let legal_candidate = legal
                    .iter()
                    .find(|candidate| candidate.candidate_id == evidence.candidate_id)
                    .ok_or_else(|| {
                        format!(
                            "card reward policy audit could not find legal candidate '{}'",
                            evidence.candidate_id
                        )
                    })?;
                let prior_probability = self
                    .prior
                    .entries
                    .iter()
                    .find(|entry| entry.candidate_id == evidence.candidate_id)
                    .map(|entry| entry.probability)
                    .ok_or_else(|| {
                        format!(
                            "card reward policy audit could not find prior for candidate '{}'",
                            evidence.candidate_id
                        )
                    })?;
                Ok(CardRewardPolicyAuditCandidateV1 {
                    owner_rank,
                    candidate_id: evidence.candidate_id.clone(),
                    label: legal_candidate.label.to_string(),
                    candidate_key: evidence.candidate_key.clone(),
                    acquisition: evidence.acquisition.clone(),
                    band: evidence.band,
                    closed_threat_gaps: evidence.delta.closed_threat_gaps.clone(),
                    capability_improvements: evidence.delta.capability_improvements.clone(),
                    capability_rule_changes: evidence.delta.capability_rule_changes.clone(),
                    resolved_formation_needs: evidence.delta.resolved_formation_needs.clone(),
                    added_formation_strengths: evidence.delta.added_formation_strengths.clone(),
                    introduces_undigested_status_burden: evidence
                        .introduces_undigested_status_burden,
                    duplicate_low_marginal: evidence.duplicate_low_marginal,
                    card_acquisition_policy: evidence.card_acquisition_policy,
                    access_conflict_or_redundancy: evidence.access_conflict_or_redundancy,
                    mummified_hand_power_tempo: evidence.mummified_hand_power_tempo,
                    mummified_hand_stacking_duplicate_support: evidence
                        .mummified_hand_stacking_duplicate_support,
                    boss_power_tax_conflict: evidence.boss_power_tax_conflict,
                    random_target_frontload_reliable: evidence.random_target_frontload_reliable,
                    added_deck_shape_risks: evidence.added_deck_shape_risks.clone(),
                    persistent_draw_pile_status: evidence.persistent_draw_pile_status.clone(),
                    improves_threat_relevant_capability: evidence
                        .improves_threat_relevant_capability,
                    amplifies_existing_answers: evidence.amplifies_existing_answers,
                    boss_damage_plan_improvement: evidence.boss_damage_plan_improvement,
                    boss_survival_plan_repair: evidence.boss_survival_plan_repair.clone(),
                    combat_upgrade_scope_before: evidence.combat_upgrade_scope_before,
                    combat_upgrade_scope_after: evidence.combat_upgrade_scope_after,
                    combat_upgrade_target_count_before: evidence.combat_upgrade_target_count_before,
                    upgrade_investment_support: evidence.upgrade_investment_support,
                    surface_index: evidence.surface_index,
                    prior_probability,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(ExactCardRewardPolicyAuditV1 {
            schema_name: EXACT_CARD_REWARD_POLICY_AUDIT_SCHEMA_NAME,
            schema_version: EXACT_CARD_REWARD_POLICY_AUDIT_SCHEMA_VERSION,
            current_hp: self.exact.before.resources.current_hp,
            max_hp: self.exact.before.resources.max_hp,
            gold: self.exact.before.resources.gold,
            candidates,
        })
    }
}

pub fn exact_card_reward_policy_audit_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactCardRewardPolicyAuditV1, String> {
    exact_card_reward_policy_decision_v1(session, legal)?.audit(legal)
}

pub fn exact_direct_card_acquisition_policy_audit_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactCardRewardPolicyAuditV1, String> {
    exact_direct_card_acquisition_policy_decision_v1(session, legal)?.audit(legal)
}

pub fn exact_card_reward_policy_prior_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<RunPolicyPriorV1, String> {
    Ok(exact_card_reward_policy_decision_v1(session, legal)?.prior)
}

/// Ranks visible actions that each add exactly one known card by the same
/// evidence used for ordinary card rewards.
///
/// The caller owns source-specific option discovery (Library, another event,
/// or a future direct-card surface). This owner derives the acquired card from
/// each exact successor, so obtain-time upgrades and relic side effects remain
/// simulator facts rather than adapter guesses.
pub fn exact_direct_card_acquisition_policy_prior_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<RunPolicyPriorV1, String> {
    Ok(exact_direct_card_acquisition_policy_decision_v1(session, legal)?.prior)
}

pub fn exact_direct_card_acquisition_policy_decision_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactCardRewardPolicyDecisionV1, String> {
    if legal.is_empty() {
        return Err("direct card acquisition policy requires visible candidates".to_string());
    }
    let exact = exact_run_policy_decision_v1(session)?;
    let legal_ids = legal
        .iter()
        .map(|candidate| candidate.candidate_id)
        .collect::<BTreeSet<_>>();
    if legal_ids.len() != legal.len() {
        return Err("direct card acquisition policy received duplicate candidate ids".to_string());
    }
    let owned = exact
        .actions
        .iter()
        .enumerate()
        .filter(|(_, action)| legal_ids.contains(action.candidate_id.as_str()))
        .collect::<Vec<_>>();
    if owned.len() != legal.len() {
        return Err(format!(
            "direct card acquisition surface disagrees with exact successors: exact={} policy={}",
            owned.len(),
            legal.len()
        ));
    }

    let strategy = build_run_strategy_snapshot_from_run_state_v2(&session.run_state);
    let upgrade_sink_support = strategy.support(StrategyPackageIdV2::UpgradeSink);
    let combat_upgrade_target_count_before = plan_upgrades_v1(&session.run_state).candidates.len();
    let formation_needs = strategy.formation_summary().needs;
    let startup = deck_startup_profile_v1(&session.run_state);
    let deck_shape = deck_shape_profile_v1(&session.run_state);
    let block_plan = block_plan_profile_v1(&session.run_state);
    let deck_plan = DeckPlanSnapshot::from_run_state(&session.run_state);
    let boss_profile = session
        .run_state
        .boss_key
        .map(|boss| boss_mechanic_pressure_profile_v1(&session.run_state, boss));
    let boss_power_tax_active = boss_profile
        .as_ref()
        .is_some_and(|profile| profile.has_pressure(BossMechanicPressurePointV1::PowerPlayPenalty));
    let boss_target_topology = boss_profile
        .as_ref()
        .map(|profile| profile.target_topology)
        .unwrap_or(BossEncounterTargetTopologyV1::Unknown);
    let mut evidence = owned
        .into_iter()
        .map(|(surface_index, action)| {
            let (card, upgrades) = exact_added_card_v1(session, action)?;
            let acquisition = direct_card_acquisition_v1(
                session,
                card,
                upgrades,
                &formation_needs,
                &startup,
                &block_plan,
            );
            card_reward_action_evidence_v1(
                session,
                &exact,
                action,
                acquisition,
                surface_index,
                upgrade_sink_support,
                combat_upgrade_target_count_before,
                &deck_shape,
                deck_plan,
                boss_power_tax_active,
                boss_target_topology,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    evidence.sort_by(compare_card_reward_evidence);
    let prior =
        positive_grouped_run_policy_prior_v1(legal, card_reward_preference_groups_v1(&evidence))?;

    Ok(ExactCardRewardPolicyDecisionV1 {
        exact,
        evidence,
        prior,
    })
}

pub fn exact_card_reward_policy_decision_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactCardRewardPolicyDecisionV1, String> {
    let exact = exact_run_policy_decision_v1(session)?;
    let owned = owned_run_policy_actions_v1(&exact, legal, "card reward", is_card_reward_key)?;
    let strategy = build_run_strategy_snapshot_from_run_state_v2(&session.run_state);
    let upgrade_sink_support = strategy.support(StrategyPackageIdV2::UpgradeSink);
    let combat_upgrade_target_count_before = plan_upgrades_v1(&session.run_state).candidates.len();
    let formation_needs = strategy.formation_summary().needs;
    let startup = deck_startup_profile_v1(&session.run_state);
    let deck_shape = deck_shape_profile_v1(&session.run_state);
    let block_plan = block_plan_profile_v1(&session.run_state);
    let deck_plan = DeckPlanSnapshot::from_run_state(&session.run_state);
    let boss_profile = session
        .run_state
        .boss_key
        .map(|boss| boss_mechanic_pressure_profile_v1(&session.run_state, boss));
    let boss_power_tax_active = boss_profile
        .as_ref()
        .is_some_and(|profile| profile.has_pressure(BossMechanicPressurePointV1::PowerPlayPenalty));
    let boss_target_topology = boss_profile
        .as_ref()
        .map(|profile| profile.target_topology)
        .unwrap_or(BossEncounterTargetTopologyV1::Unknown);
    let mut evidence = owned
        .into_iter()
        .map(|(surface_index, action)| {
            let candidate_key = action.candidate_key.as_ref().ok_or_else(|| {
                format!(
                    "card reward candidate '{}' has no typed key",
                    action.candidate_id
                )
            })?;
            let acquisition = acquisition_v1(
                session,
                candidate_key,
                &formation_needs,
                &startup,
                &block_plan,
            )?;
            card_reward_action_evidence_v1(
                session,
                &exact,
                action,
                acquisition,
                surface_index,
                upgrade_sink_support,
                combat_upgrade_target_count_before,
                &deck_shape,
                deck_plan,
                boss_power_tax_active,
                boss_target_topology,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    evidence.sort_by(compare_card_reward_evidence);
    let ranked_card_ids = evidence
        .iter()
        .map(|candidate| candidate.candidate_id.clone())
        .collect::<BTreeSet<_>>();
    let mut preference_groups = card_reward_preference_groups_v1(&evidence);
    preference_groups.extend(
        legal
            .iter()
            .filter(|candidate| !ranked_card_ids.contains(candidate.candidate_id))
            .map(|candidate| vec![candidate.candidate_id.to_string()]),
    );
    let prior = positive_grouped_run_policy_prior_v1(legal, preference_groups)?;

    Ok(ExactCardRewardPolicyDecisionV1 {
        exact,
        evidence,
        prior,
    })
}

fn is_card_reward_key(key: &DecisionCandidateKey) -> bool {
    matches!(
        key,
        DecisionCandidateKey::CardRewardPick { .. }
            | DecisionCandidateKey::CardRewardOpen { .. }
            | DecisionCandidateKey::CardRewardSingingBowl { .. }
            | DecisionCandidateKey::CardRewardSkip { .. }
    )
}

fn card_reward_action_evidence_v1(
    parent: &RunControlSession,
    decision: &ExactRunPolicyDecisionV1,
    action: &ExactRunPolicyActionSuccessorV1,
    acquisition: CardRewardPolicyAcquisitionV1,
    surface_index: usize,
    upgrade_sink_support: StrategyPlanSupportV1,
    combat_upgrade_target_count_before: usize,
    deck_shape: &DeckShapeProfileV1,
    deck_plan: DeckPlanSnapshot,
    boss_power_tax_active: bool,
    boss_target_topology: BossEncounterTargetTopologyV1,
) -> Result<CardRewardPolicyActionEvidenceV1, String> {
    let candidate_key = action.candidate_key.clone().ok_or_else(|| {
        format!(
            "card reward candidate '{}' has no typed key",
            action.candidate_id
        )
    })?;
    let delta = run_policy_state_delta_v1(&decision.before, &action.after);
    let introduces_undigested_status_burden = matches!(
        &acquisition,
        CardRewardPolicyAcquisitionV1::Card { semantics, .. }
            if semantics.roles.contains(&CardRewardSemanticRoleV1::StatusGenerator)
                && action.after.deck.status_payoffs == 0
                && parent.run_state.relics.iter().any(|relic| {
                    relic.id == crate::content::relics::RelicId::RunicPyramid
                })
    );
    let mummified_hand_power_tempo = match &acquisition {
        CardRewardPolicyAcquisitionV1::Card { card, upgrades, .. } => {
            mummified_hand_power_tempo_v1(&parent.run_state, *card, *upgrades)
        }
        _ => None,
    };
    let mummified_hand_stacking_duplicate_support = mummified_hand_stacking_duplicate_support_v1(
        &acquisition,
        mummified_hand_power_tempo.as_ref(),
    );
    let duplicate_low_marginal = duplicate_low_marginal_v1(
        &acquisition,
        &delta,
        mummified_hand_stacking_duplicate_support,
    );
    let card_acquisition_policy = match &acquisition {
        CardRewardPolicyAcquisitionV1::Card {
            card,
            upgrades,
            construction,
            ..
        } => {
            let admission = assess_reward_admission_from_master_deck(
                &parent.run_state.master_deck,
                *card,
                *upgrades,
            );
            let report = assess_card_acquisition_from_construction_v1(
                AcquisitionContext::reward(deck_plan),
                *card,
                *upgrades,
                &admission,
                construction.clone(),
            );
            Some(evaluate_deck_construction_contract(&report))
        }
        CardRewardPolicyAcquisitionV1::SingingBowl
        | CardRewardPolicyAcquisitionV1::Skip
        | CardRewardPolicyAcquisitionV1::OpenReward => None,
    };
    let recurring_card_access = matches!(
        &acquisition,
        CardRewardPolicyAcquisitionV1::Card { construction, .. }
            if construction
                .construction_roles
                .contains(&AcquisitionConstructionRole::RecurringCardAccess)
    );
    let access_conflict_or_redundancy = matches!(
        &acquisition,
        CardRewardPolicyAcquisitionV1::Card {
            semantics,
            access,
            component_signals,
            ..
        }
            if !semantics.roles.is_empty()
                && semantics.roles.iter().all(|role| is_access_role(*role))
                && (component_signals
                    .debt_signals
                    .contains(&CardComponentSignalKindV1::DuplicateNoDrawAccessDebt)
                    || (!recurring_card_access
                        && ((access.is_none()
                            && decision.before.deck.draw_sources
                                .saturating_add(decision.before.deck.energy_sources)
                                >= 3
                            && delta.capability_improvements.is_empty()
                            && delta.resolved_formation_needs.is_empty())
                            || (access.is_some_and(|access| {
                                access.leverage == CardAccessLeverageV1::Incremental
                            })
                                && decision.before.deck.draw_sources
                                    .saturating_add(decision.before.deck.energy_sources)
                                    >= 3
                                && delta.capability_improvements.is_empty()
                                && delta.resolved_formation_needs.is_empty()))))
    );
    let added_deck_shape_risks = match &acquisition {
        CardRewardPolicyAcquisitionV1::Card { card, .. } => {
            deck_shape_candidate_delta_v1(deck_shape, *card).risks
        }
        CardRewardPolicyAcquisitionV1::SingingBowl
        | CardRewardPolicyAcquisitionV1::Skip
        | CardRewardPolicyAcquisitionV1::OpenReward => Vec::new(),
    };
    let persistent_draw_pile_status = match &acquisition {
        CardRewardPolicyAcquisitionV1::Card { card, upgrades, .. } => {
            let facts = card_reward_facts_v1(&RewardCard::new(*card, *upgrades));
            persistent_draw_pile_status_assessment_v1(&parent.run_state, &facts.status_injections)
        }
        CardRewardPolicyAcquisitionV1::SingingBowl
        | CardRewardPolicyAcquisitionV1::Skip
        | CardRewardPolicyAcquisitionV1::OpenReward => None,
    };
    let improves_threat_relevant_capability = !threat_relevant_capability_improvements_v1(
        &decision.before.threats,
        &decision.before.threat_coverage,
        &action.after.threat_coverage,
    )
    .is_empty();
    let boss_power_tax_conflict =
        boss_power_tax_conflict_v1(boss_power_tax_active, &acquisition, &delta);
    let random_target_frontload_reliable =
        random_target_frontload_reliable_v1(boss_target_topology, &acquisition);
    let combat_upgrade_scope_before = decision.before.combat_upgrade_coverage.strongest_scope();
    let combat_upgrade_scope_after = action.after.combat_upgrade_coverage.strongest_scope();
    let establishes_combat_upgrade_access = combat_upgrade_scope_after
        > combat_upgrade_scope_before
        && combat_upgrade_target_count_before > 0;
    let upgrade_investment_support = matches!(
        &acquisition,
        CardRewardPolicyAcquisitionV1::Card { semantics, .. }
            if semantics
                .dependencies
                .contains(&CardRewardPickDependencyV1::RouteUpgradeDensity)
    )
    .then_some(upgrade_sink_support);
    let amplifies_existing_answers =
        access_amplifies_existing_answers(&decision.before, &acquisition);
    let boss_damage_plan_improvement = boss_damage_plan_improvement_v1(parent, &acquisition);
    let boss_survival_plan_repair = match &acquisition {
        CardRewardPolicyAcquisitionV1::Card { card, upgrades, .. } => {
            let admission = assess_reward_admission_from_master_deck(
                &parent.run_state.master_deck,
                *card,
                *upgrades,
            );
            let survival =
                assess_boss_survival_evidence(deck_plan, Some((*card, *upgrades)), &admission);
            survival.repairs_plan().then(|| survival.label.to_string())
        }
        CardRewardPolicyAcquisitionV1::SingingBowl
        | CardRewardPolicyAcquisitionV1::Skip
        | CardRewardPolicyAcquisitionV1::OpenReward => None,
    };
    let base_band = card_reward_band_v1(
        &acquisition,
        &delta,
        introduces_undigested_status_burden,
        duplicate_low_marginal,
        card_acquisition_policy,
        mummified_hand_stacking_duplicate_support,
        access_conflict_or_redundancy,
        boss_power_tax_conflict,
        !added_deck_shape_risks.is_empty(),
        improves_threat_relevant_capability,
        amplifies_existing_answers,
        boss_damage_plan_improvement.is_some(),
        boss_survival_plan_repair.is_some(),
        establishes_combat_upgrade_access,
    );
    let band = apply_upgrade_investment_gate_v1(
        apply_persistent_draw_pile_status_gate_v1(base_band, persistent_draw_pile_status.as_ref()),
        upgrade_investment_support,
    );

    Ok(CardRewardPolicyActionEvidenceV1 {
        candidate_id: action.candidate_id.clone(),
        candidate_key,
        acquisition,
        band,
        delta,
        introduces_undigested_status_burden,
        duplicate_low_marginal,
        card_acquisition_policy,
        access_conflict_or_redundancy,
        mummified_hand_power_tempo,
        mummified_hand_stacking_duplicate_support,
        boss_power_tax_conflict,
        random_target_frontload_reliable,
        added_deck_shape_risks,
        persistent_draw_pile_status,
        improves_threat_relevant_capability,
        amplifies_existing_answers,
        boss_damage_plan_improvement,
        boss_survival_plan_repair,
        combat_upgrade_scope_before,
        combat_upgrade_scope_after,
        combat_upgrade_target_count_before,
        upgrade_investment_support,
        surface_index,
    })
}

fn duplicate_low_marginal_v1(
    acquisition: &CardRewardPolicyAcquisitionV1,
    delta: &RunPolicyStateDeltaV1,
    mummified_hand_stacking_duplicate_support: bool,
) -> bool {
    let CardRewardPolicyAcquisitionV1::Card {
        card,
        upgrades,
        copies_before,
        semantics,
        component_signals,
        ..
    } = acquisition
    else {
        return false;
    };
    if *copies_before == 0 || mummified_hand_stacking_duplicate_support {
        return false;
    }

    // A first repeat of a cheap card that replaces access to itself while
    // doing real tactical work is not equivalent to another redundant copy
    // of a payoff or coverage card. Its value is consistency: it reaches the
    // rest of the deck without asking the discrete capability delta to invent
    // a new strategic shape. Keep this exception deliberately narrow; later
    // copies, pure access cards, expensive access, and cards with unresolved
    // component debt still face the normal duplicate test.
    let first_efficient_self_replacing_tactical_repeat = *copies_before == 1
        && get_card_definition(*card).cost <= 1
        && semantics.roles.iter().any(|role| is_access_role(*role))
        && semantics.roles.iter().any(|role| !is_access_role(*role))
        && component_signals.debt_signals.is_empty();
    if first_efficient_self_replacing_tactical_repeat {
        return false;
    }

    let no_new_strategic_shape = delta.closed_threat_gaps.is_empty()
        && delta.resolved_formation_needs.is_empty()
        && delta.added_formation_strengths.is_empty();
    if no_new_strategic_shape && delta.capability_improvements.is_empty() {
        return true;
    }

    let tactical_coverage_only = !semantics.roles.is_empty()
        && semantics.roles.iter().all(|role| {
            matches!(
                role,
                CardRewardSemanticRoleV1::FrontloadDamage
                    | CardRewardSemanticRoleV1::AoeDamage
                    | CardRewardSemanticRoleV1::Vulnerable
                    | CardRewardSemanticRoleV1::Weak
            )
        });
    let only_reinforces_supported_capabilities = !delta.capability_improvements.is_empty()
        && delta.capability_improvements.iter().all(|change| {
            matches!(
                change.before,
                StrategyCapabilityCoverageV1::Supported | StrategyCapabilityCoverageV1::Strong
            ) && change.after == StrategyCapabilityCoverageV1::Strong
        });

    let has_aoe_role = semantics
        .roles
        .contains(&CardRewardSemanticRoleV1::AoeDamage);
    let expensive_tactical_duplicate = get_card_definition(*card).cost >= 2 && !has_aoe_role;
    let light_aoe_duplicate = has_aoe_role
        && card_analysis_profile_v1(*card, *upgrades).aoe_support
            == CardAnalysisAoeSupportV1::Present;

    (expensive_tactical_duplicate || light_aoe_duplicate)
        && tactical_coverage_only
        && no_new_strategic_shape
        && only_reinforces_supported_capabilities
}

fn mummified_hand_stacking_duplicate_support_v1(
    acquisition: &CardRewardPolicyAcquisitionV1,
    tempo: Option<&MummifiedHandPowerTempoV1>,
) -> bool {
    let CardRewardPolicyAcquisitionV1::Card {
        card,
        upgrades,
        copies_before,
        component_signals,
        ..
    } = acquisition
    else {
        return false;
    };
    let Some(tempo) = tempo else {
        return false;
    };

    *copies_before > 0
        && tempo.card == *card
        && tempo.eligible_positive_cost_cards > 0
        && card_definition_with_upgrades(*card, *upgrades)
            .duplicate_behaviors
            .iter()
            .any(|behavior| {
                matches!(
                    behavior,
                    DuplicateBehavior::StackingHandler | DuplicateBehavior::StackingOutput
                )
            })
        && component_signals
            .positive_signals
            .iter()
            .any(|signal| is_concrete_package_support_signal_v1(*signal))
}

fn boss_damage_plan_improvement_v1(
    parent: &RunControlSession,
    acquisition: &CardRewardPolicyAcquisitionV1,
) -> Option<CardRewardBossDamagePlanImprovementV1> {
    let CardRewardPolicyAcquisitionV1::Card { card, upgrades, .. } = acquisition else {
        return None;
    };

    let before = assess_boss_damage_plan_v1(&parent.run_state.master_deck);
    let mut after_run = parent.run_state.clone();
    let lineage = after_run.allocate_master_deck_lineage();
    let mut added = CombatCard::with_lineage(*card, lineage);
    added.upgrades = *upgrades;
    after_run.master_deck.push(added);
    let after = assess_boss_damage_plan_v1(&after_run.master_deck);

    ((after.readiness, after.engine_reliability) > (before.readiness, before.engine_reliability))
        .then_some(CardRewardBossDamagePlanImprovementV1 {
            before_readiness: before.readiness,
            after_readiness: after.readiness,
            before_reliability: before.engine_reliability,
            after_reliability: after.engine_reliability,
        })
}

fn apply_upgrade_investment_gate_v1(
    base_band: CardRewardPolicyBandV1,
    support: Option<StrategyPlanSupportV1>,
) -> CardRewardPolicyBandV1 {
    match support {
        None | Some(StrategyPlanSupportV1::Strong) => base_band,
        Some(StrategyPlanSupportV1::Plausible) => {
            base_band.max(CardRewardPolicyBandV1::SpeculativeAddition)
        }
        Some(StrategyPlanSupportV1::Weak | StrategyPlanSupportV1::Blocked) => {
            CardRewardPolicyBandV1::Liability
        }
    }
}

fn apply_persistent_draw_pile_status_gate_v1(
    base_band: CardRewardPolicyBandV1,
    assessment: Option<&PersistentDrawPileStatusAssessmentV1>,
) -> CardRewardPolicyBandV1 {
    match assessment.map(|assessment| assessment.handling) {
        None | Some(PersistentDrawPileStatusHandlingV1::Covered) => base_band,
        Some(
            PersistentDrawPileStatusHandlingV1::Unsupported
            | PersistentDrawPileStatusHandlingV1::Conditional,
        ) => base_band.max(CardRewardPolicyBandV1::SpeculativeAddition),
    }
}

fn acquisition_v1(
    parent: &RunControlSession,
    key: &DecisionCandidateKey,
    formation_needs: &[crate::ai::noncombat_strategy_v1::StrategyDeckFormationNeedV1],
    startup: &DeckStartupProfileV1,
    block_plan: &BlockPlanProfileV1,
) -> Result<CardRewardPolicyAcquisitionV1, String> {
    Ok(match key {
        DecisionCandidateKey::CardRewardPick { card, upgrades, .. } => direct_card_acquisition_v1(
            parent,
            *card,
            *upgrades,
            formation_needs,
            startup,
            block_plan,
        ),
        DecisionCandidateKey::CardRewardSingingBowl { .. } => {
            CardRewardPolicyAcquisitionV1::SingingBowl
        }
        DecisionCandidateKey::CardRewardSkip { .. } => CardRewardPolicyAcquisitionV1::Skip,
        DecisionCandidateKey::CardRewardOpen { .. } => CardRewardPolicyAcquisitionV1::OpenReward,
        other => {
            return Err(format!(
                "exact card reward policy received non-card-reward candidate key {other:?}"
            ))
        }
    })
}

fn direct_card_acquisition_v1(
    parent: &RunControlSession,
    card: CardId,
    upgrades: u8,
    formation_needs: &[crate::ai::noncombat_strategy_v1::StrategyDeckFormationNeedV1],
    startup: &DeckStartupProfileV1,
    block_plan: &BlockPlanProfileV1,
) -> CardRewardPolicyAcquisitionV1 {
    let reward = RewardCard::new(card, upgrades);
    let semantics = card_reward_semantic_profile_v1(&reward);
    let copies_before = parent
        .run_state
        .master_deck
        .iter()
        .filter(|owned| owned.id == card)
        .count();
    let admission =
        assess_reward_admission_from_master_deck(&parent.run_state.master_deck, card, upgrades);
    let component_signals = evaluate_card_component_signals_v1(
        &CardComponentSignalContextV1 {
            same_card_count: copies_before,
            candidate_upgrades: upgrades,
            formation_needs: formation_needs.to_vec(),
            startup: startup.clone(),
            block_plan: block_plan.clone(),
            candidate_unlocks_convertible_strength_payoff:
                card_unlocks_convertible_strength_payoff_v1(&parent.run_state, card, upgrades),
            additional_play_target_supported: card_mechanics_profile_v1(card)
                .action_supply
                .additional_play_scope
                .is_some_and(|scope| {
                    additional_play_target_support_v1(&parent.run_state, scope)
                        .has_high_leverage_target()
                }),
        },
        &semantics,
    );
    let construction = assess_card_construction_with_component_signals_v1(
        DeckPlanSnapshot::from_run_state(&parent.run_state),
        card,
        upgrades,
        &admission,
        &component_signals,
    );
    CardRewardPolicyAcquisitionV1::Card {
        card,
        upgrades,
        copies_before,
        access: card_access_evidence_v1(&reward),
        component_signals,
        construction,
        semantics,
    }
}

fn exact_added_card_v1(
    parent: &RunControlSession,
    action: &ExactRunPolicyActionSuccessorV1,
) -> Result<(CardId, u8), String> {
    let mut matched_parent = vec![false; parent.run_state.master_deck.len()];
    let mut added = Vec::new();
    for child_card in &action.exact.session.run_state.master_deck {
        let parent_match = parent
            .run_state
            .master_deck
            .iter()
            .enumerate()
            .find(|(index, parent_card)| {
                !matched_parent[*index]
                    && parent_card.id == child_card.id
                    && parent_card.upgrades == child_card.upgrades
            })
            .map(|(index, _)| index);
        if let Some(index) = parent_match {
            matched_parent[index] = true;
        } else {
            added.push(child_card);
        }
    }
    match added.as_slice() {
        [card]
            if action.exact.session.run_state.master_deck.len()
                == parent.run_state.master_deck.len().saturating_add(1) =>
        {
            Ok((card.id, card.upgrades))
        }
        _ => Err(format!(
            "direct card acquisition candidate '{}' added {} distinguishable cards (deck {} -> {})",
            action.candidate_id,
            added.len(),
            parent.run_state.master_deck.len(),
            action.exact.session.run_state.master_deck.len()
        )),
    }
}

fn card_reward_band_v1(
    acquisition: &CardRewardPolicyAcquisitionV1,
    delta: &RunPolicyStateDeltaV1,
    status_burden: bool,
    duplicate_low_marginal: bool,
    card_acquisition_policy: Option<AcquisitionPolicyDecision>,
    exact_contextual_support: bool,
    access_conflict_or_redundancy: bool,
    boss_power_tax_conflict: bool,
    introduces_deck_shape_risk: bool,
    improves_threat_relevant_capability: bool,
    amplifies_existing_answers: bool,
    repairs_boss_damage_plan: bool,
    repairs_boss_survival_plan: bool,
    establishes_combat_upgrade_access: bool,
) -> CardRewardPolicyBandV1 {
    match acquisition {
        CardRewardPolicyAcquisitionV1::OpenReward => CardRewardPolicyBandV1::ResolvePendingBoundary,
        CardRewardPolicyAcquisitionV1::SingingBowl => CardRewardPolicyBandV1::PreserveDeckQuality,
        CardRewardPolicyAcquisitionV1::Skip => CardRewardPolicyBandV1::PreserveDeckQuality,
        CardRewardPolicyAcquisitionV1::Card {
            component_signals,
            construction,
            ..
        } => {
            // Mechanics and formation signals are evidence consumed by the
            // shared construction owner, not independent acquisition grants.
            // The boundary owner trusts the source-aware contract verdict and
            // only keeps exact successor deltas as separate exceptions.
            let shared_construction_support =
                card_acquisition_policy.is_some_and(AcquisitionPolicyDecision::allows_acquisition);
            let has_blocking_package_debt = component_signals
                .debt_signals
                .iter()
                .any(|signal| is_unresolved_package_payoff_debt_signal_v1(*signal))
                && !shared_construction_support;
            let exact_multi_axis_capability_asset = !delta.capability_improvements.is_empty()
                && construction.multi_axis_tactical_delivery
                && !construction.fully_redundant_expensive_short_debuff_package;
            if status_burden
                || duplicate_low_marginal
                || access_conflict_or_redundancy
                || introduces_deck_shape_risk
                || has_blocking_package_debt
            {
                CardRewardPolicyBandV1::Liability
            } else if repairs_boss_survival_plan {
                CardRewardPolicyBandV1::CloseThreatGap
            } else if boss_power_tax_conflict {
                CardRewardPolicyBandV1::SpeculativeAddition
            } else if !delta.closed_threat_gaps.is_empty() {
                CardRewardPolicyBandV1::CloseThreatGap
            } else if construction.fully_redundant_expensive_short_debuff_package {
                // A coarse Supported -> Strong capability count is not an
                // acquisition proof when the construction owner has already
                // established that every short debuff axis is covered.  An
                // exact closed threat gap above may still override this, but
                // merely adding another expensive delivery card may not.
                CardRewardPolicyBandV1::SpeculativeAddition
            } else if amplifies_existing_answers {
                CardRewardPolicyBandV1::AmplifyStrategicAccess
            } else if improves_threat_relevant_capability
                && (shared_construction_support || exact_contextual_support)
            {
                CardRewardPolicyBandV1::ImproveRequiredCapability
            } else if shared_construction_support
                || exact_contextual_support
                || exact_multi_axis_capability_asset
                || repairs_boss_damage_plan
                || establishes_combat_upgrade_access
            {
                CardRewardPolicyBandV1::EstablishStrategicAsset
            } else {
                CardRewardPolicyBandV1::SpeculativeAddition
            }
        }
    }
}

fn boss_power_tax_conflict_v1(
    boss_power_tax_active: bool,
    acquisition: &CardRewardPolicyAcquisitionV1,
    delta: &RunPolicyStateDeltaV1,
) -> bool {
    let CardRewardPolicyAcquisitionV1::Card { card, upgrades, .. } = acquisition else {
        return false;
    };

    boss_power_tax_active
        && card_analysis_profile_v1(*card, *upgrades).is_boss_minor_power
        && delta.closed_threat_gaps.is_empty()
        && delta.capability_improvements.is_empty()
        && delta.resolved_formation_needs.is_empty()
        && delta.added_formation_strengths.is_empty()
}

fn random_target_frontload_reliable_v1(
    boss_target_topology: BossEncounterTargetTopologyV1,
    acquisition: &CardRewardPolicyAcquisitionV1,
) -> bool {
    let CardRewardPolicyAcquisitionV1::Card {
        card,
        upgrades,
        semantics,
        ..
    } = acquisition
    else {
        return false;
    };

    boss_target_topology == BossEncounterTargetTopologyV1::SingleOpponent
        && semantics
            .roles
            .contains(&CardRewardSemanticRoleV1::RandomOutput)
        && semantics
            .roles
            .contains(&CardRewardSemanticRoleV1::FrontloadDamage)
        && card_analysis_profile_v1(*card, *upgrades).attack_chunk
            != CardAnalysisAttackChunkV1::None
}

fn access_amplifies_existing_answers(
    before: &super::RunPolicyStateEvidenceV1,
    acquisition: &CardRewardPolicyAcquisitionV1,
) -> bool {
    let CardRewardPolicyAcquisitionV1::Card {
        copies_before,
        access:
            Some(CardAccessEvidenceV1 {
                leverage: CardAccessLeverageV1::EfficientBurst,
                ..
            }),
        component_signals,
        ..
    } = acquisition
    else {
        return false;
    };
    if *copies_before > 0
        || !component_signals
            .positive_signals
            .contains(&CardComponentSignalKindV1::DrawEnergyAccess)
        || !component_signals.debt_signals.is_empty()
    {
        return false;
    }

    !before.formation.strengths.is_empty()
        || before
            .threat_coverage
            .capabilities
            .iter()
            .any(|capability| {
                capability.capability != StrategyCapabilityKindV1::DrawEnergyConsistency
                    && matches!(
                        capability.coverage,
                        StrategyCapabilityCoverageV1::Supported
                            | StrategyCapabilityCoverageV1::Strong
                    )
            })
}

fn is_access_role(role: CardRewardSemanticRoleV1) -> bool {
    matches!(
        role,
        CardRewardSemanticRoleV1::CardDraw
            | CardRewardSemanticRoleV1::CycleAccess
            | CardRewardSemanticRoleV1::DiscardPileTopdeckAccess
            | CardRewardSemanticRoleV1::HandTopdeckSelection
            | CardRewardSemanticRoleV1::EnergySource
            | CardRewardSemanticRoleV1::EnergyOnExhaust
    )
}

fn compare_card_reward_evidence(
    left: &CardRewardPolicyActionEvidenceV1,
    right: &CardRewardPolicyActionEvidenceV1,
) -> Ordering {
    compare_card_reward_semantic_evidence(left, right)
        .then_with(|| left.surface_index.cmp(&right.surface_index))
}

fn compare_card_reward_semantic_evidence(
    left: &CardRewardPolicyActionEvidenceV1,
    right: &CardRewardPolicyActionEvidenceV1,
) -> Ordering {
    left.band
        .cmp(&right.band)
        .then_with(|| {
            compare_gap_source_counts(
                &left.delta.closed_threat_gaps,
                &right.delta.closed_threat_gaps,
            )
        })
        .then_with(|| {
            right
                .boss_damage_plan_improvement
                .is_some()
                .cmp(&left.boss_damage_plan_improvement.is_some())
        })
        .then_with(|| {
            right
                .card_acquisition_policy
                .is_some_and(AcquisitionPolicyDecision::has_concrete_component_support_v1)
                .cmp(
                    &left
                        .card_acquisition_policy
                        .is_some_and(AcquisitionPolicyDecision::has_concrete_component_support_v1),
                )
        })
        .then_with(|| {
            specific_positive_component_count_v1(right)
                .cmp(&specific_positive_component_count_v1(left))
        })
        .then_with(|| {
            right
                .random_target_frontload_reliable
                .cmp(&left.random_target_frontload_reliable)
        })
        .then_with(|| right.delta.max_hp_gain.cmp(&left.delta.max_hp_gain))
}

fn specific_positive_component_count_v1(evidence: &CardRewardPolicyActionEvidenceV1) -> usize {
    let CardRewardPolicyAcquisitionV1::Card {
        component_signals, ..
    } = &evidence.acquisition
    else {
        return 0;
    };
    component_signals
        .positive_signals
        .iter()
        .filter(|signal| **signal != CardComponentSignalKindV1::FormationNeedProgress)
        .count()
}

fn card_reward_preference_groups_v1(
    evidence: &[CardRewardPolicyActionEvidenceV1],
) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut previous = None;
    for candidate in evidence {
        let starts_new_group = previous.is_none_or(|previous| {
            compare_card_reward_semantic_evidence(previous, candidate) != Ordering::Equal
        });
        if starts_new_group {
            groups.push(Vec::new());
        }
        groups
            .last_mut()
            .expect("preference group must exist")
            .push(candidate.candidate_id.clone());
        previous = Some(candidate);
    }
    groups
}

fn compare_gap_source_counts(
    left: &[super::RunPolicyThreatGapKeyV1],
    right: &[super::RunPolicyThreatGapKeyV1],
) -> Ordering {
    for source in [
        StrategyThreatSourceV1::ActBoss,
        StrategyThreatSourceV1::ActEliteEncounter,
        StrategyThreatSourceV1::ActElitePool,
        StrategyThreatSourceV1::ActHallwayPool,
    ] {
        let left_count = left.iter().filter(|gap| gap.source == source).count();
        let right_count = right.iter().filter(|gap| gap.source == source).count();
        match right_count.cmp(&left_count) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::noncombat_strategy_v1::StrategyDeckFormationNeedV1;
    use crate::content::monsters::factory::EncounterId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::eval::run_control::{build_decision_surface, RunControlConfig};
    use crate::state::core::EngineState;
    use crate::state::rewards::{RewardItem, RewardState};

    fn reward_session(cards: &[(CardId, u8)]) -> RunControlSession {
        let mut session = RunControlSession::new(RunControlConfig::default());
        let reward_cards = cards
            .iter()
            .map(|(card, upgrades)| RewardCard::new(*card, *upgrades))
            .collect::<Vec<_>>();
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Card {
            cards: reward_cards.clone(),
        }];
        reward.pending_card_choice = Some(reward_cards);
        reward.pending_card_reward_index = Some(0);
        session.engine_state = EngineState::RewardScreen(reward);
        session
    }

    #[test]
    fn status_payoff_support_is_owned_evidence_not_a_formation_need() {
        let mut unsupported = reward_session(&[
            (CardId::FireBreathing, 0),
            (CardId::Carnage, 0),
            (CardId::Flex, 0),
        ]);
        unsupported.run_state.act_num = 1;
        unsupported.run_state.floor_num = 4;
        unsupported.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::Disarm, 0),
            (CardId::SwordBoomerang, 0),
        ]);

        let unsupported_decision = decision(&unsupported);
        let unsupported_payoff = card_evidence(&unsupported_decision, CardId::FireBreathing);
        assert!(matches!(
            &unsupported_payoff.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .debt_signals
                .contains(&CardComponentSignalKindV1::StatusPayoffUnsupported)
                && !component_signals
                    .positive_signals
                    .contains(&CardComponentSignalKindV1::StatusPayoffSupported)
        ));
        assert_eq!(unsupported_payoff.band, CardRewardPolicyBandV1::Liability);

        unsupported
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::RecklessCharge, 99));
        let supported_decision = decision(&unsupported);
        let supported_payoff = card_evidence(&supported_decision, CardId::FireBreathing);
        assert!(matches!(
            &supported_payoff.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::StatusPayoffSupported)
                && !component_signals
                    .debt_signals
                    .contains(&CardComponentSignalKindV1::StatusPayoffUnsupported)
        ));
        assert_ne!(supported_payoff.band, CardRewardPolicyBandV1::Liability);
    }

    #[test]
    fn raw_formation_threshold_crossing_needs_candidate_density_evidence() {
        let mut session = reward_session(&[
            (CardId::IronWave, 1),
            (CardId::TwinStrike, 0),
            (CardId::SecondWind, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 3;
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::SingingBowl));
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::Clothesline, 0),
        ]);

        let decision = decision(&session);
        let iron_wave = card_evidence(&decision, CardId::IronWave);
        let bowl_position = position(&decision, |key| {
            matches!(key, DecisionCandidateKey::CardRewardSingingBowl { .. })
        });
        let skip_position = position(&decision, |key| {
            matches!(key, DecisionCandidateKey::CardRewardSkip { .. })
        });
        assert!(iron_wave
            .delta
            .resolved_formation_needs
            .contains(&StrategyDeckFormationNeedV1::Block));
        assert!(matches!(
            &iron_wave.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if !component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::FormationNeedProgress)
                && component_signals
                    .note_signals
                    .contains(&CardComponentSignalKindV1::FormationNeedRoleWithoutTacticalDensity)
        ));
        assert_eq!(iron_wave.band, CardRewardPolicyBandV1::SpeculativeAddition);
        assert_eq!(
            decision.evidence[bowl_position].band,
            CardRewardPolicyBandV1::PreserveDeckQuality
        );
        assert!(bowl_position < skip_position);
        assert!(
            skip_position
                < position(&decision, |key| matches!(
                    key,
                    DecisionCandidateKey::CardRewardPick {
                        card: CardId::IronWave,
                        ..
                    }
                ))
        );
    }

    #[test]
    fn clean_early_deck_can_admit_a_multi_axis_tactical_transition() {
        let mut session = reward_session(&[
            (CardId::Uppercut, 0),
            (CardId::HeavyBlade, 0),
            (CardId::Rupture, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 5;
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::SingingBowl));
        session.run_state.master_deck = owned_deck(&[
            (CardId::Bash, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Disarm, 0),
            (CardId::GhostlyArmor, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
        ]);

        let decision = decision(&session);
        let uppercut = card_evidence(&decision, CardId::Uppercut);
        assert!(matches!(
            &uppercut.acquisition,
            CardRewardPolicyAcquisitionV1::Card { construction, .. }
                if !construction.fully_redundant_expensive_short_debuff_package
        ));
        assert!(matches!(
            &uppercut.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::MultiAxisTacticalDelivery)
                && component_signals
                    .positive_signals
                    .contains(&CardComponentSignalKindV1::FormationNeedProgress)
                && !component_signals
                    .note_signals
                    .contains(&CardComponentSignalKindV1::FormationNeedRoleWithoutTacticalDensity)
        ));
        assert_eq!(
            uppercut.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset,
            "uppercut={uppercut:#?}"
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Uppercut,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "evidence={:#?}",
            decision.evidence
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Uppercut,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSingingBowl { .. }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn early_expensive_short_weak_does_not_multiply_into_unrelated_threat_answers() {
        let mut session = reward_session(&[
            (CardId::Anger, 0),
            (CardId::Clothesline, 0),
            (CardId::Whirlwind, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 1;
        session.run_state.boss_key = Some(EncounterId::SlimeBoss);
        session.run_state.current_hp = 72;
        session.run_state.max_hp = 72;
        session.run_state.gold = 360;
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
        ]);

        let decision = decision(&session);
        let clothesline = card_evidence(&decision, CardId::Clothesline);
        assert!(clothesline.delta.closed_threat_gaps.is_empty());
        assert_eq!(
            clothesline.band,
            CardRewardPolicyBandV1::SpeculativeAddition
        );
        assert!(matches!(
            &clothesline.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if !component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::FormationNeedProgress)
                && component_signals
                    .note_signals
                    .contains(&CardComponentSignalKindV1::FormationNeedRoleWithoutTacticalDensity)
        ));
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::CardRewardPick {
                card: CardId::Whirlwind,
                ..
            }
        ));
    }

    #[test]
    fn typed_discard_topdeck_access_competes_as_a_strategic_asset() {
        let mut session = reward_session(&[
            (CardId::WildStrike, 0),
            (CardId::Headbutt, 0),
            (CardId::Anger, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 4;
        session.run_state.current_hp = 85;
        session.run_state.max_hp = 85;
        session.run_state.master_deck = owned_deck(&[
            (CardId::Bash, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Whirlwind, 0),
        ]);

        let decision = decision(&session);
        let headbutt = card_evidence(&decision, CardId::Headbutt);

        assert!(
            headbutt.band < CardRewardPolicyBandV1::PreserveDeckQuality,
            "typed access should receive a positive evidence band; evidence={headbutt:#?}"
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Headbutt,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "typed discard-pile access should not be demoted by a stale unsupported marker; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn additional_attack_play_is_supported_by_high_leverage_owned_targets() {
        let mut session = reward_session(&[
            (CardId::BodySlam, 0),
            (CardId::Rupture, 0),
            (CardId::DoubleTap, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 6;
        session.run_state.current_hp = 71;
        session.run_state.max_hp = 85;
        session.run_state.master_deck = owned_deck(&[
            (CardId::Bash, 0),
            (CardId::Carnage, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Headbutt, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Whirlwind, 0),
        ]);

        let decision = decision(&session);
        let double_tap = card_evidence(&decision, CardId::DoubleTap);

        assert!(matches!(
            &double_tap.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                semantics,
                component_signals,
                ..
            } if semantics.roles.contains(&CardRewardSemanticRoleV1::ActionAmplifier)
                && component_signals
                    .positive_signals
                    .contains(&CardComponentSignalKindV1::AdditionalPlayTargetSupported)
                && !component_signals
                    .debt_signals
                    .contains(&CardComponentSignalKindV1::AdditionalPlayTargetUnsupported)
        ));
        assert_eq!(
            double_tap.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset,
            "double_tap={double_tap:#?}"
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::DoubleTap,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "a conditional additional play needs visible high-leverage targets; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn additional_attack_play_without_a_high_leverage_target_loses_to_skip() {
        let mut session = reward_session(&[
            (CardId::DoubleTap, 0),
            (CardId::BodySlam, 0),
            (CardId::Rupture, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 2;
        session.run_state.master_deck = owned_deck(&[
            (CardId::Bash, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
        ]);

        let decision = decision(&session);
        let double_tap = card_evidence(&decision, CardId::DoubleTap);

        assert!(matches!(
            &double_tap.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .debt_signals
                .contains(&CardComponentSignalKindV1::AdditionalPlayTargetUnsupported)
        ));
        assert_eq!(double_tap.band, CardRewardPolicyBandV1::Liability);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::DoubleTap,
                    ..
                }
            )),
            "additional-play supply is conditional, not an intrinsic asset; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn exhume_with_owned_fiend_fire_is_supported_reuse_not_forced_skip() {
        let mut session = reward_session(&[
            (CardId::Exhume, 0),
            (CardId::Berserk, 0),
            (CardId::Bludgeon, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 16;
        session.run_state.current_hp = 27;
        session.run_state.max_hp = 76;
        session.run_state.master_deck = owned_deck(&[
            (CardId::Anger, 0),
            (CardId::Bash, 1),
            (CardId::Clothesline, 1),
            (CardId::DoubleTap, 0),
            (CardId::FiendFire, 0),
            (CardId::Hemokinesis, 0),
            (CardId::SecondWind, 0),
            (CardId::SeverSoul, 0),
            (CardId::Whirlwind, 1),
        ]);

        let decision = decision(&session);
        let exhume = card_evidence(&decision, CardId::Exhume);
        assert!(matches!(
            &exhume.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::ExhaustReuseSupported)
                && !component_signals
                    .debt_signals
                    .contains(&CardComponentSignalKindV1::ExhaustReuseUnsupported)
        ));
        assert_eq!(
            exhume.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset,
            "exhume={exhume:#?}"
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Exhume,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "owned Fiend Fire should ground Exhume's reuse value; evidence={:#?}",
            decision.evidence
        );
    }

    fn owned_deck(cards: &[(CardId, u8)]) -> Vec<CombatCard> {
        cards
            .iter()
            .enumerate()
            .map(|(index, (card, upgrades))| {
                let mut owned = CombatCard::for_scenario(*card, index as u32);
                owned.upgrades = *upgrades;
                owned
            })
            .collect()
    }

    #[test]
    fn armaments_plus_with_live_targets_establishes_exact_whole_hand_upgrade_access() {
        let mut session = reward_session(&[(CardId::Armaments, 1)]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 29;
        session.run_state.boss_key = Some(EncounterId::Automaton);
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::FusionHammer));
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::Offering, 0),
            (CardId::FiendFire, 0),
            (CardId::FlameBarrier, 0),
        ]);

        let decision = decision(&session);
        let armaments = card_evidence(&decision, CardId::Armaments);
        let skip = position(&decision, |key| {
            matches!(key, DecisionCandidateKey::CardRewardSkip { .. })
        });

        assert!(matches!(
            &armaments.acquisition,
            CardRewardPolicyAcquisitionV1::Card { semantics, .. }
                if semantics.combat_upgrade_scope == Some(CombatUpgradeScopeV1::WholeHand)
        ));
        assert_eq!(armaments.combat_upgrade_scope_before, None);
        assert_eq!(
            armaments.combat_upgrade_scope_after,
            Some(CombatUpgradeScopeV1::WholeHand)
        );
        assert!(armaments.combat_upgrade_target_count_before >= 6);
        assert_eq!(
            armaments.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Armaments,
                    ..
                }
            )) < skip,
            "a first whole-hand upgrade source with live targets must outrank preserving deck quality; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn duplicate_armaments_plus_does_not_claim_a_second_upgrade_scope_gain() {
        let mut session = reward_session(&[(CardId::Armaments, 1)]);
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::Armaments, 1),
        ]);

        let decision = decision(&session);
        let armaments = card_evidence(&decision, CardId::Armaments);

        assert_eq!(
            armaments.combat_upgrade_scope_before,
            Some(CombatUpgradeScopeV1::WholeHand)
        );
        assert_eq!(
            armaments.combat_upgrade_scope_after,
            armaments.combat_upgrade_scope_before
        );
        assert_ne!(
            armaments.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset,
            "an unchanged coverage scope is not a new strategic asset"
        );
    }

    fn a2_automaton_reward_session(
        offered: &[(CardId, u8)],
        extra_owned: &[(CardId, u8)],
    ) -> RunControlSession {
        let mut session = reward_session(offered);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 28;
        session.run_state.boss_key = Some(EncounterId::Automaton);
        session.run_state.current_hp = 72;
        session.run_state.max_hp = 76;
        session.run_state.master_deck = owned_deck(&[
            (CardId::Anger, 0),
            (CardId::Bash, 1),
            (CardId::Bloodletting, 0),
            (CardId::Clothesline, 1),
            (CardId::DarkEmbrace, 0),
            (CardId::DarkShackles, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::DoubleTap, 0),
            (CardId::Exhume, 0),
            (CardId::FiendFire, 0),
            (CardId::Hemokinesis, 0),
            (CardId::PommelStrike, 0),
            (CardId::SecondWind, 0),
            (CardId::SeverSoul, 0),
            (CardId::ShrugItOff, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Whirlwind, 1),
        ]);
        let next_uuid = session.run_state.master_deck.len() as u32;
        session
            .run_state
            .master_deck
            .extend(
                extra_owned
                    .iter()
                    .enumerate()
                    .map(|(index, (card, upgrades))| {
                        let mut owned = CombatCard::for_scenario(*card, next_uuid + index as u32);
                        owned.upgrades = *upgrades;
                        owned
                    }),
            );
        session
    }

    #[test]
    fn automaton_survival_repair_promotes_first_high_quality_block_over_skip() {
        let mut session = a2_automaton_reward_session(
            &[
                (CardId::Warcry, 0),
                (CardId::Dropkick, 0),
                (CardId::Impervious, 0),
            ],
            &[],
        );
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::SingingBowl));

        let decision = decision(&session);
        let impervious = card_evidence(&decision, CardId::Impervious);
        assert_eq!(
            impervious.boss_survival_plan_repair.as_deref(),
            Some("automaton-first-hyperbeam-block-plan")
        );
        assert_eq!(impervious.band, CardRewardPolicyBandV1::CloseThreatGap);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Impervious,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "evidence={:#?}",
            decision.evidence
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Impervious,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSingingBowl { .. }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn automaton_owned_high_quality_block_prevents_duplicate_survival_promotion() {
        let session = a2_automaton_reward_session(
            &[(CardId::Impervious, 0), (CardId::Warcry, 0)],
            &[(CardId::PowerThrough, 0)],
        );

        let decision = decision(&session);
        let impervious = card_evidence(&decision, CardId::Impervious);
        assert_eq!(impervious.boss_survival_plan_repair, None);
    }

    fn a1f10_mummified_hand_session(cards: &[(CardId, u8)]) -> RunControlSession {
        let mut session = reward_session(cards);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 10;
        session.run_state.boss_key = Some(EncounterId::SlimeBoss);
        session.run_state.current_hp = 45;
        session.run_state.max_hp = 85;
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::MummifiedHand));
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::PowerThrough, 0),
            (CardId::SecondWind, 0),
            (CardId::ThunderClap, 0),
            (CardId::Anger, 0),
            (CardId::ShrugItOff, 0),
            (CardId::DarkEmbrace, 0),
        ]);
        session
    }

    fn a1f11_mummified_hand_session(cards: &[(CardId, u8)]) -> RunControlSession {
        let mut session = a1f10_mummified_hand_session(cards);
        session.run_state.floor_num = 11;
        session.run_state.current_hp = 37;
        session.run_state.gold = 159;
        session
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::RecklessCharge, 10_001));
        session
    }

    fn a2f20_champ_without_damage_engine_session(cards: &[(CardId, u8)]) -> RunControlSession {
        let mut session = reward_session(cards);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 20;
        session.run_state.boss_key = Some(EncounterId::TheChamp);
        session.run_state.current_hp = 60;
        session.run_state.max_hp = 85;
        session.run_state.gold = 105;
        session.run_state.relics = vec![
            RelicState::new(RelicId::BurningBlood),
            RelicState::new(RelicId::OrnamentalFan),
            RelicState::new(RelicId::MummifiedHand),
            RelicState::new(RelicId::PenNib),
            RelicState::new(RelicId::RunicCube),
            RelicState::new(RelicId::OrangePellets),
        ];
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::PowerThrough, 0),
            (CardId::SecondWind, 0),
            (CardId::ThunderClap, 0),
            (CardId::Anger, 0),
            (CardId::ShrugItOff, 0),
            (CardId::DarkEmbrace, 0),
            (CardId::RecklessCharge, 0),
            (CardId::DarkEmbrace, 0),
            (CardId::FiendFire, 0),
            (CardId::Cleave, 0),
        ]);
        session
    }

    fn policy_candidates<'a>(
        surface: &'a super::super::DecisionSurface,
    ) -> Vec<RunPolicyCandidateV1<'a>> {
        surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                if !candidate.key.as_ref().is_some_and(is_card_reward_key) {
                    return None;
                }
                candidate
                    .action
                    .executable_action_ref()
                    .map(|action| RunPolicyCandidateV1 {
                        candidate_id: &candidate.id,
                        label: &candidate.label,
                        action,
                    })
            })
            .collect()
    }

    fn decision(session: &RunControlSession) -> ExactCardRewardPolicyDecisionV1 {
        let surface = build_decision_surface(session);
        let legal = policy_candidates(&surface);
        exact_card_reward_policy_decision_v1(session, &legal).expect("exact card reward policy")
    }

    fn position(
        decision: &ExactCardRewardPolicyDecisionV1,
        predicate: impl Fn(&DecisionCandidateKey) -> bool,
    ) -> usize {
        decision
            .evidence
            .iter()
            .position(|candidate| predicate(&candidate.candidate_key))
            .expect("candidate position")
    }

    fn card_evidence(
        decision: &ExactCardRewardPolicyDecisionV1,
        card: CardId,
    ) -> &CardRewardPolicyActionEvidenceV1 {
        decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::CardRewardPick {
                        card: candidate_card,
                        ..
                    } if candidate_card == card
                )
            })
            .expect("card evidence")
    }

    #[test]
    fn second_expensive_tactical_coverage_loses_to_preserving_deck_quality() {
        // Preserve the exact A1F10 public deck shape: the first Uppercut has
        // already supplied its tactical roles before the duplicate is offered.
        let mut session = reward_session(&[(CardId::Uppercut, 0)]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 10;
        session.run_state.boss_key = Some(EncounterId::SlimeBoss);
        session.run_state.current_hp = 53;
        session.run_state.max_hp = 85;
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::MummifiedHand));
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::PowerThrough, 0),
            (CardId::Uppercut, 0),
            (CardId::SecondWind, 0),
            (CardId::Cleave, 0),
            (CardId::ShrugItOff, 0),
            (CardId::DarkEmbrace, 0),
        ]);

        let decision = decision(&session);
        let uppercut = card_evidence(&decision, CardId::Uppercut);
        assert!(uppercut.duplicate_low_marginal, "uppercut={uppercut:#?}");
        assert_eq!(uppercut.band, CardRewardPolicyBandV1::Liability);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Uppercut,
                    ..
                }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn first_efficient_self_replacing_tactical_repeat_is_not_a_liability() {
        let mut session = reward_session(&[(CardId::ShrugItOff, 0)]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 30;
        session.run_state.boss_key = Some(EncounterId::Automaton);
        session.run_state.current_hp = 76;
        session.run_state.max_hp = 76;
        session.run_state.master_deck = current_floor_30_deck(1);

        let decision = decision(&session);
        let shrug = card_evidence(&decision, CardId::ShrugItOff);
        assert!(!shrug.duplicate_low_marginal, "shrug={shrug:#?}");
        assert_ne!(shrug.band, CardRewardPolicyBandV1::Liability);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::ShrugItOff,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn later_self_replacing_tactical_repeat_still_loses_to_skip() {
        let mut session = reward_session(&[(CardId::ShrugItOff, 0)]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 30;
        session.run_state.boss_key = Some(EncounterId::Automaton);
        session.run_state.current_hp = 76;
        session.run_state.max_hp = 76;
        session.run_state.master_deck = current_floor_30_deck(2);

        let decision = decision(&session);
        let shrug = card_evidence(&decision, CardId::ShrugItOff);
        assert!(shrug.duplicate_low_marginal, "shrug={shrug:#?}");
        assert_eq!(shrug.band, CardRewardPolicyBandV1::Liability);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::ShrugItOff,
                    ..
                }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    fn current_floor_30_deck(shrug_copies: usize) -> Vec<CombatCard> {
        let mut cards = vec![
            (CardId::Anger, 0),
            (CardId::Bash, 1),
            (CardId::Bloodletting, 0),
            (CardId::Clothesline, 1),
            (CardId::DarkEmbrace, 0),
            (CardId::DarkShackles, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::DoubleTap, 0),
            (CardId::Exhume, 0),
            (CardId::FiendFire, 0),
            (CardId::Hemokinesis, 0),
            (CardId::Impervious, 0),
            (CardId::PommelStrike, 0),
            (CardId::SecondWind, 0),
            (CardId::SeverSoul, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Whirlwind, 1),
        ];
        cards.extend(std::iter::repeat_n((CardId::ShrugItOff, 0), shrug_copies));
        owned_deck(&cards)
    }

    #[test]
    fn fully_redundant_expensive_short_debuff_package_loses_to_deck_quality() {
        let mut session = a1f10_mummified_hand_session(&[
            (CardId::Flex, 0),
            (CardId::Uppercut, 0),
            (CardId::RecklessCharge, 0),
        ]);
        session
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Clothesline, 10_002));

        let decision = decision(&session);
        let uppercut = card_evidence(&decision, CardId::Uppercut);
        assert!(matches!(
            &uppercut.acquisition,
            CardRewardPolicyAcquisitionV1::Card { construction, .. }
                if construction.fully_redundant_expensive_short_debuff_package
        ));
        assert_eq!(uppercut.band, CardRewardPolicyBandV1::SpeculativeAddition);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Uppercut,
                    ..
                }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn upgraded_debuff_duration_does_not_trigger_short_overlap_gate() {
        let session = a1f10_mummified_hand_session(&[(CardId::Uppercut, 1)]);

        let decision = decision(&session);
        let uppercut = card_evidence(&decision, CardId::Uppercut);
        assert!(matches!(
            &uppercut.acquisition,
            CardRewardPolicyAcquisitionV1::Card { construction, .. }
                if !construction.fully_redundant_expensive_short_debuff_package
        ));
        assert!(uppercut.delta.closed_threat_gaps.is_empty());
        assert_eq!(
            uppercut.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset,
            "uppercut={uppercut:#?}"
        );
    }

    #[test]
    fn novel_multi_axis_debuff_package_is_a_strategic_asset() {
        let mut session = a1f10_mummified_hand_session(&[(CardId::Uppercut, 0)]);
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
        ]);

        let decision = decision(&session);
        let uppercut = card_evidence(&decision, CardId::Uppercut);
        assert!(matches!(
            &uppercut.acquisition,
            CardRewardPolicyAcquisitionV1::Card { construction, .. }
                if !construction.fully_redundant_expensive_short_debuff_package
        ));
        assert!(uppercut.delta.closed_threat_gaps.is_empty());
        assert_eq!(
            uppercut.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset
        );
    }

    #[test]
    fn supported_second_exhaust_power_keeps_typed_mummified_hand_tempo() {
        let mut session = reward_session(&[(CardId::DarkEmbrace, 0)]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 13;
        session.run_state.boss_key = Some(EncounterId::SlimeBoss);
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::MummifiedHand));
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::PowerThrough, 0),
            (CardId::SecondWind, 0),
            (CardId::DarkEmbrace, 0),
        ]);

        let decision = decision(&session);
        let dark_embrace = card_evidence(&decision, CardId::DarkEmbrace);
        let tempo = dark_embrace
            .mummified_hand_power_tempo
            .expect("Power candidate with Mummified Hand should expose typed tempo");
        assert_eq!(tempo.card, CardId::DarkEmbrace);
        assert_eq!(tempo.paid_cost, 2);
        assert!(tempo.eligible_positive_cost_cards > 0);
        assert!(dark_embrace.mummified_hand_stacking_duplicate_support);
        assert!(!dark_embrace.duplicate_low_marginal);
        assert_ne!(dark_embrace.band, CardRewardPolicyBandV1::Liability);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::DarkEmbrace,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn mummified_hand_does_not_rescue_a_redundant_installed_rule() {
        let mut session = reward_session(&[(CardId::Corruption, 0)]);
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::MummifiedHand));
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Corruption, 0),
            (CardId::DarkEmbrace, 0),
        ]);

        let decision = decision(&session);
        let corruption = card_evidence(&decision, CardId::Corruption);
        assert!(corruption.mummified_hand_power_tempo.is_some());
        assert!(!corruption.mummified_hand_stacking_duplicate_support);
        assert!(corruption.duplicate_low_marginal);
        assert_eq!(corruption.band, CardRewardPolicyBandV1::Liability);
    }

    #[test]
    fn efficient_access_amplifies_existing_answers_before_marginal_frontload() {
        let mut session = reward_session(&[(CardId::RecklessCharge, 0), (CardId::BattleTrance, 0)]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 20;
        session.run_state.boss_key = Some(EncounterId::Automaton);
        session.run_state.current_hp = 76;
        session.run_state.max_hp = 80;
        session.run_state.master_deck = [
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::TrueGrit, 1),
            (CardId::SwordBoomerang, 0),
            (CardId::Clash, 0),
            (CardId::Uppercut, 1),
            (CardId::Corruption, 0),
            (CardId::BurningPact, 0),
            (CardId::SeverSoul, 0),
            (CardId::Offering, 0),
            (CardId::Intimidate, 0),
            (CardId::Headbutt, 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let decision = decision(&session);
        let battle_trance = card_evidence(&decision, CardId::BattleTrance);
        assert!(battle_trance.amplifies_existing_answers);
        assert_eq!(
            battle_trance.band,
            CardRewardPolicyBandV1::AmplifyStrategicAccess
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::BattleTrance,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::RecklessCharge,
                    ..
                }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn covered_urgent_frontload_gap_still_precedes_access_amplification() {
        let mut session = reward_session(&[(CardId::WildStrike, 0), (CardId::BattleTrance, 0)]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 1;
        session.run_state.boss_key = Some(EncounterId::TheGuardian);
        session.run_state.current_hp = 80;
        session.run_state.max_hp = 80;
        session.run_state.master_deck = [
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Evolve,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();

        let decision = decision(&session);
        assert_eq!(
            card_evidence(&decision, CardId::WildStrike)
                .persistent_draw_pile_status
                .as_ref()
                .expect("Wild Strike persistent status assessment")
                .handling,
            PersistentDrawPileStatusHandlingV1::Covered
        );
        assert_eq!(
            card_evidence(&decision, CardId::WildStrike).band,
            CardRewardPolicyBandV1::CloseThreatGap
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::WildStrike,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::BattleTrance,
                    ..
                }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn f11_persistent_wound_stays_speculative_with_only_conditional_hand_exhaust() {
        let session = a1f11_mummified_hand_session(&[
            (CardId::WildStrike, 0),
            (CardId::ThunderClap, 0),
            (CardId::Warcry, 0),
        ]);

        let decision = decision(&session);
        let wild = card_evidence(&decision, CardId::WildStrike);
        let assessment = wild
            .persistent_draw_pile_status
            .as_ref()
            .expect("Wild Strike persistent status assessment");

        assert_eq!(
            assessment.handling,
            PersistentDrawPileStatusHandlingV1::Conditional
        );
        assert_eq!(assessment.conditional_hand_exhaust_count, 1);
        assert!(
            !wild.delta.closed_threat_gaps.is_empty()
                || wild.improves_threat_relevant_capability,
            "the gate should remain meaningful even when coarse capability deltas favor Wild Strike"
        );
        assert_eq!(wild.band, CardRewardPolicyBandV1::SpeculativeAddition);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::WildStrike,
                    ..
                }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn f11_evolve_covers_persistent_wound_without_status_gate_demotion() {
        let mut session = a1f11_mummified_hand_session(&[
            (CardId::WildStrike, 0),
            (CardId::ThunderClap, 0),
            (CardId::Warcry, 0),
        ]);
        session
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Evolve, 10_002));

        let decision = decision(&session);
        let wild = card_evidence(&decision, CardId::WildStrike);
        let assessment = wild
            .persistent_draw_pile_status
            .as_ref()
            .expect("Wild Strike persistent status assessment");

        assert_eq!(
            assessment.handling,
            PersistentDrawPileStatusHandlingV1::Covered
        );
        assert_eq!(assessment.draw_recovery_count, 1);
        assert!(
            wild.band < CardRewardPolicyBandV1::SpeculativeAddition,
            "covered status must preserve the underlying supported band: {wild:#?}"
        );
    }

    #[test]
    fn f11_second_light_aoe_does_not_turn_source_count_into_strategic_strength() {
        let session = a1f11_mummified_hand_session(&[
            (CardId::WildStrike, 0),
            (CardId::ThunderClap, 0),
            (CardId::Warcry, 0),
        ]);

        let decision = decision(&session);
        let thunder_clap = card_evidence(&decision, CardId::ThunderClap);

        assert!(thunder_clap.duplicate_low_marginal);
        assert!(thunder_clap
            .delta
            .capability_improvements
            .iter()
            .all(|change| {
                change.before == StrategyCapabilityCoverageV1::Supported
                    && change.after == StrategyCapabilityCoverageV1::Strong
            }));
        assert_eq!(thunder_clap.band, CardRewardPolicyBandV1::Liability);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::ThunderClap,
                    ..
                }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn duplicate_second_wind_strengthens_status_control_without_long_fight_promotion() {
        let session = a1f10_mummified_hand_session(&[(CardId::SecondWind, 0)]);

        let decision = decision(&session);
        let second_wind = card_evidence(&decision, CardId::SecondWind);
        assert!(second_wind
            .delta
            .capability_improvements
            .iter()
            .any(|change| {
                change.capability == StrategyCapabilityKindV1::StatusControl
                    && change.before == StrategyCapabilityCoverageV1::Supported
                    && change.after == StrategyCapabilityCoverageV1::Strong
            }));
        assert!(second_wind
            .delta
            .capability_improvements
            .iter()
            .all(|change| change.capability == StrategyCapabilityKindV1::StatusControl));
        let change = second_wind
            .delta
            .capability_rule_changes
            .iter()
            .find(|change| change.capability == StrategyCapabilityKindV1::LongFightScaling)
            .expect("duplicate Second Wind should retain its typed long-fight input change");
        let input = |inputs: &[crate::ai::noncombat_strategy_v1::StrategyCapabilityInputV1],
                     kind| {
            inputs
                .iter()
                .find(|input| input.input == kind)
                .map(|input| input.value)
        };

        assert_eq!(change.before, StrategyCapabilityCoverageV1::Strong);
        assert_eq!(change.after, StrategyCapabilityCoverageV1::Strong);
        assert_eq!(
            input(
                &change.before_inputs,
                crate::ai::noncombat_strategy_v1::StrategyCapabilityInputKindV1::ExhaustGenerators,
            ),
            Some(1)
        );
        assert_eq!(
            input(
                &change.after_inputs,
                crate::ai::noncombat_strategy_v1::StrategyCapabilityInputKindV1::ExhaustGenerators,
            ),
            Some(2)
        );
        assert_eq!(
            input(
                &change.before_inputs,
                crate::ai::noncombat_strategy_v1::StrategyCapabilityInputKindV1::RepeatableExhaustConverters,
            ),
            Some(1)
        );
        assert_eq!(
            input(
                &change.after_inputs,
                crate::ai::noncombat_strategy_v1::StrategyCapabilityInputKindV1::RepeatableExhaustConverters,
            ),
            Some(2)
        );
        assert!(!second_wind.duplicate_low_marginal);
        assert_eq!(
            second_wind.band,
            CardRewardPolicyBandV1::ImproveRequiredCapability
        );
    }

    #[test]
    fn first_light_aoe_source_can_still_close_an_act_one_gap() {
        let mut session = reward_session(&[(CardId::ThunderClap, 0)]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 4;
        session.run_state.boss_key = Some(EncounterId::SlimeBoss);
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
        ]);

        let decision = decision(&session);
        let thunder_clap = card_evidence(&decision, CardId::ThunderClap);

        assert!(!thunder_clap.duplicate_low_marginal);
        assert!(thunder_clap.band < CardRewardPolicyBandV1::PreserveDeckQuality);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::ThunderClap,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn low_density_damage_does_not_hide_unfunded_strength_payoff_debt() {
        // Heavy Blade has printed damage but neither tactical delivery density
        // nor a strength generator in this exact early deck.
        let mut session = reward_session(&[
            (CardId::Dropkick, 0),
            (CardId::HeavyBlade, 0),
            (CardId::Clash, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 3;
        session.run_state.boss_key = Some(EncounterId::SlimeBoss);
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::PowerThrough, 0),
        ]);

        let decision = decision(&session);
        let heavy_blade = card_evidence(&decision, CardId::HeavyBlade);
        assert!(heavy_blade.delta.closed_threat_gaps.is_empty());
        assert!(!heavy_blade.improves_threat_relevant_capability);
        assert!(!heavy_blade
            .delta
            .capability_improvements
            .iter()
            .any(|change| change.capability == StrategyCapabilityKindV1::LongFightScaling));
        assert_eq!(heavy_blade.band, CardRewardPolicyBandV1::Liability);
        assert!(matches!(
            &heavy_blade.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .debt_signals
                .contains(&CardComponentSignalKindV1::StrengthPayoffUnsupported)
        ));
        assert_eq!(
            card_evidence(&decision, CardId::Clash).band,
            CardRewardPolicyBandV1::Liability
        );
        assert_eq!(
            card_evidence(&decision, CardId::Dropkick).band,
            CardRewardPolicyBandV1::SpeculativeAddition,
            "conditional access is a known effect, not an unsupported mechanic"
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Dropkick,
                    ..
                }
            ))
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::HeavyBlade,
                    ..
                }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn ethereal_dazed_does_not_enter_persistent_draw_pile_gate() {
        let session = reward_session(&[(CardId::RecklessCharge, 0)]);
        let decision = decision(&session);

        assert_eq!(
            card_evidence(&decision, CardId::RecklessCharge).persistent_draw_pile_status,
            None
        );
    }

    #[test]
    fn duplicate_no_draw_and_snecko_debt_block_clean_access_amplification() {
        let mut duplicate = reward_session(&[(CardId::BattleTrance, 0)]);
        duplicate.run_state.master_deck.extend(
            [CardId::Uppercut, CardId::BattleTrance]
                .into_iter()
                .enumerate()
                .map(|(index, card)| CombatCard::for_scenario(card, index as u32)),
        );
        let duplicate_decision = decision(&duplicate);
        let duplicate_access = card_evidence(&duplicate_decision, CardId::BattleTrance);
        assert!(!duplicate_access.amplifies_existing_answers);
        assert!(matches!(
            &duplicate_access.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .debt_signals
                .contains(&CardComponentSignalKindV1::DuplicateNoDrawAccessDebt)
        ));

        let mut snecko = reward_session(&[(CardId::Offering, 0)]);
        snecko
            .run_state
            .relics
            .push(RelicState::new(RelicId::SneckoEye));
        snecko.run_state.master_deck = [
            CardId::BattleTrance,
            CardId::ShrugItOff,
            CardId::PommelStrike,
            CardId::SpotWeakness,
            CardId::Inflame,
            CardId::FeelNoPain,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();
        let snecko_decision = decision(&snecko);
        let offering = card_evidence(&snecko_decision, CardId::Offering);
        assert!(!offering.amplifies_existing_answers);
        assert!(matches!(
            &offering.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .debt_signals
                .contains(&CardComponentSignalKindV1::SneckoEnergyDiscountDebt)
        ));
    }

    #[test]
    fn first_battle_trance_is_access_not_empty_or_deferred() {
        let session = reward_session(&[(CardId::BattleTrance, 1)]);
        let decision = decision(&session);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::BattleTrance,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            ))
        );
    }

    #[test]
    fn recurring_draw_access_is_not_rejected_by_immediate_source_count() {
        let mut session = reward_session(&[(CardId::Brutality, 0)]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 33;
        session.run_state.boss_key = Some(EncounterId::TheChamp);
        session.run_state.current_hp = 7;
        session.run_state.max_hp = 103;
        session.run_state.master_deck = owned_deck(&[
            (CardId::Armaments, 0),
            (CardId::AscendersBane, 0),
            (CardId::Bash, 1),
            (CardId::Clothesline, 0),
            (CardId::Exhume, 0),
            (CardId::Feed, 0),
            (CardId::Feed, 0),
            (CardId::FiendFire, 0),
            (CardId::Flex, 0),
            (CardId::Headbutt, 0),
            (CardId::Hemokinesis, 0),
            (CardId::Immolate, 0),
            (CardId::Impervious, 0),
            (CardId::Inflame, 0),
            (CardId::RecklessCharge, 0),
            (CardId::Sentinel, 0),
            (CardId::Shockwave, 1),
            (CardId::ShrugItOff, 0),
            (CardId::ShrugItOff, 0),
            (CardId::ShrugItOff, 1),
            (CardId::TrueGrit, 1),
            (CardId::TwinStrike, 0),
            (CardId::Whirlwind, 1),
        ]);

        let decision = decision(&session);
        let brutality = card_evidence(&decision, CardId::Brutality);

        assert!(matches!(
            &brutality.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                semantics,
                construction,
                ..
            } if semantics.roles.contains(&CardRewardSemanticRoleV1::CardDraw)
                && construction
                    .construction_roles
                    .contains(&AcquisitionConstructionRole::RecurringCardAccess)
        ));
        assert!(!brutality.access_conflict_or_redundancy);
        assert_eq!(
            brutality.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset
        );
    }

    #[test]
    fn seed006_f8_clash_debt_does_not_masquerade_as_new_frontload() {
        let mut session = reward_session(&[
            (CardId::Clash, 0),
            (CardId::Feed, 0),
            (CardId::PommelStrike, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 8;
        session.run_state.boss_key = Some(EncounterId::TheGuardian);
        session.run_state.current_hp = 72;
        session.run_state.max_hp = 80;
        session.run_state.master_deck = [
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::Berserk, 0),
            (CardId::WildStrike, 0),
            (CardId::ShrugItOff, 0),
            (CardId::HeavyBlade, 0),
            (CardId::Clothesline, 0),
            (CardId::TwinStrike, 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let decision = decision(&session);
        let clash = card_evidence(&decision, CardId::Clash);
        assert!(clash
            .added_deck_shape_risks
            .iter()
            .any(|risk| matches!(risk, DeckShapeRiskV1::ClashPlayabilityDebt { .. })));
        assert_eq!(clash.band, CardRewardPolicyBandV1::Liability);
        for better in [CardId::Feed, CardId::PommelStrike] {
            assert!(
                position(&decision, |key| matches!(
                    key,
                    DecisionCandidateKey::CardRewardPick { card, .. } if *card == better
                )) < position(&decision, |key| matches!(
                    key,
                    DecisionCandidateKey::CardRewardPick {
                        card: CardId::Clash,
                        ..
                    }
                )),
                "{better:?} should precede the newly unplayable Clash; evidence={:#?}",
                decision.evidence
            );
        }
    }

    #[test]
    fn clash_without_deck_shape_debt_is_not_rejected_by_the_shared_gate() {
        let session = reward_session(&[(CardId::Clash, 0)]);
        let decision = decision(&session);
        let clash = card_evidence(&decision, CardId::Clash);
        assert!(clash.added_deck_shape_risks.is_empty());
        assert_ne!(clash.band, CardRewardPolicyBandV1::Liability);
    }

    #[test]
    fn established_boss_scaling_repair_precedes_external_payoff_in_same_band() {
        let mut session = reward_session(&[
            (CardId::Feed, 0),
            (CardId::DemonForm, 0),
            (CardId::FiendFire, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 16;
        session.run_state.boss_key = Some(EncounterId::Hexaghost);
        session.run_state.current_hp = 33;
        session.run_state.max_hp = 80;
        session.run_state.master_deck = [
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::WildStrike, 0),
            (CardId::GhostlyArmor, 0),
            (CardId::SeeingRed, 1),
            (CardId::ShrugItOff, 0),
            (CardId::PommelStrike, 0),
            (CardId::SwordBoomerang, 0),
            (CardId::SpotWeakness, 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let decision = decision(&session);
        let demon_form = card_evidence(&decision, CardId::DemonForm);
        let feed = card_evidence(&decision, CardId::Feed);

        assert_eq!(
            demon_form.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset,
            "demon_form={demon_form:#?}"
        );
        assert_eq!(
            feed.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset,
            "feed={feed:#?}"
        );
        assert_eq!(
            demon_form.boss_damage_plan_improvement,
            Some(CardRewardBossDamagePlanImprovementV1 {
                before_readiness: BossDamagePlanReadinessV1::Engine,
                after_readiness: BossDamagePlanReadinessV1::Engine,
                before_reliability: BossDamagePlanEngineReliabilityV1::Fragile,
                after_reliability: BossDamagePlanEngineReliabilityV1::Established,
            })
        );
        assert_eq!(feed.boss_damage_plan_improvement, None);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::DemonForm,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Feed,
                    ..
                }
            )),
            "the shared reliability repair must break the same-band surface tie; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn known_champ_engine_repair_precedes_an_unreliable_debuff_fragment() {
        let session = a2f20_champ_without_damage_engine_session(&[
            (CardId::Clothesline, 0),
            (CardId::DemonForm, 0),
            (CardId::Havoc, 0),
        ]);

        let decision = decision(&session);
        let clothesline = card_evidence(&decision, CardId::Clothesline);
        let demon_form = card_evidence(&decision, CardId::DemonForm);

        assert_eq!(
            clothesline.band,
            CardRewardPolicyBandV1::SpeculativeAddition
        );
        assert!(clothesline.delta.closed_threat_gaps.is_empty());
        assert_eq!(
            demon_form.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset
        );
        assert_eq!(
            demon_form.boss_damage_plan_improvement,
            Some(CardRewardBossDamagePlanImprovementV1 {
                before_readiness: BossDamagePlanReadinessV1::Support,
                after_readiness: BossDamagePlanReadinessV1::Engine,
                before_reliability: BossDamagePlanEngineReliabilityV1::None,
                after_reliability: BossDamagePlanEngineReliabilityV1::Established,
            })
        );
        assert_eq!(clothesline.boss_damage_plan_improvement, None);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::DemonForm,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Clothesline,
                    ..
                }
            )),
            "the exact known-Boss engine repair should beat a larger count of generic capability changes; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn supported_exhaust_payoff_precedes_skip_before_awakened_one() {
        let mut session = reward_session(&[
            (CardId::ThunderClap, 0),
            (CardId::FeelNoPain, 1),
            (CardId::Evolve, 1),
        ]);
        session.run_state.act_num = 3;
        session.run_state.floor_num = 42;
        session.run_state.boss_key = Some(EncounterId::AwakenedOne);
        session.run_state.current_hp = 87;
        session.run_state.max_hp = 87;
        session.run_state.master_deck = [
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::WildStrike, 0),
            (CardId::ShrugItOff, 1),
            (CardId::HeavyBlade, 0),
            (CardId::Clothesline, 0),
            (CardId::TwinStrike, 0),
            (CardId::Feed, 0),
            (CardId::Intimidate, 0),
            (CardId::Evolve, 1),
            (CardId::BattleTrance, 0),
            (CardId::Shockwave, 0),
            (CardId::Barricade, 1),
            (CardId::FiendFire, 1),
            (CardId::IronWave, 1),
            (CardId::Disarm, 0),
            (CardId::SwordBoomerang, 1),
            (CardId::PommelStrike, 0),
            (CardId::Immolate, 1),
            (CardId::Cleave, 0),
            (CardId::Inflame, 1),
            (CardId::BodySlam, 0),
            (CardId::SecondWind, 1),
            (CardId::Sentinel, 1),
            (CardId::DarkEmbrace, 1),
            (CardId::Carnage, 0),
            (CardId::Bloodletting, 1),
            (CardId::FireBreathing, 1),
            (CardId::Whirlwind, 1),
            (CardId::Parasite, 0),
            (CardId::ThunderClap, 1),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let decision = decision(&session);
        let fnp = card_evidence(&decision, CardId::FeelNoPain);
        assert!(matches!(
            &fnp.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::ExhaustPayoffSupported)
        ));
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::FeelNoPain,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "supported exhaust payoff should precede skip; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn light_aoe_without_exact_delta_does_not_outrank_skip_in_a_mature_deck() {
        let mut session = reward_session(&[
            (CardId::Anger, 1),
            (CardId::Bloodletting, 0),
            (CardId::ThunderClap, 1),
        ]);
        session.run_state.act_num = 3;
        session.run_state.floor_num = 38;
        session.run_state.boss_key = Some(EncounterId::AwakenedOne);
        session.run_state.current_hp = 72;
        session.run_state.max_hp = 90;
        session.run_state.master_deck = [
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::ShrugItOff, 1),
            (CardId::Clash, 0),
            (CardId::SearingBlow, 0),
            (CardId::IronWave, 0),
            (CardId::SeverSoul, 1),
            (CardId::Whirlwind, 1),
            (CardId::Cleave, 1),
            (CardId::FiendFire, 1),
            (CardId::DarkEmbrace, 0),
            (CardId::Armaments, 1),
            (CardId::Shockwave, 1),
            (CardId::PommelStrike, 1),
            (CardId::Offering, 1),
            (CardId::DemonForm, 0),
            (CardId::TrueGrit, 1),
            (CardId::DarkShackles, 0),
            (CardId::Inflame, 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let decision = decision(&session);
        let thunder_clap = card_evidence(&decision, CardId::ThunderClap);
        assert!(matches!(
            &thunder_clap.acquisition,
            CardRewardPolicyAcquisitionV1::Card { semantics, .. }
                if semantics.roles.contains(&CardRewardSemanticRoleV1::AoeDamage)
        ));
        assert_eq!(
            card_analysis_profile_v1(CardId::ThunderClap, 1).aoe_support,
            CardAnalysisAoeSupportV1::Present
        );
        assert!(thunder_clap.delta.closed_threat_gaps.is_empty());
        assert!(thunder_clap.delta.capability_improvements.is_empty());
        assert!(thunder_clap.delta.resolved_formation_needs.is_empty());
        assert!(thunder_clap.delta.added_formation_strengths.is_empty());
        assert_eq!(
            thunder_clap.band,
            CardRewardPolicyBandV1::SpeculativeAddition
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::ThunderClap,
                    ..
                }
            )),
            "light AoE that adds no exact capability must not outrank preserving deck quality; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn energy_conversion_without_exact_delta_does_not_outrank_skip() {
        let mut session = reward_session(&[
            (CardId::PerfectedStrike, 0),
            (CardId::Bloodletting, 0),
            (CardId::Warcry, 0),
        ]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 18;
        session.run_state.boss_key = Some(EncounterId::Automaton);
        session.run_state.current_hp = 79;
        session.run_state.max_hp = 85;
        session.run_state.relics = vec![RelicState::new(RelicId::FusionHammer)];
        session.run_state.master_deck = owned_deck(&[
            (CardId::Bash, 0),
            (CardId::Carnage, 0),
            (CardId::Clothesline, 0),
            (CardId::CurseOfTheBell, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::DoubleTap, 0),
            (CardId::FiendFire, 0),
            (CardId::Headbutt, 0),
            (CardId::Offering, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Whirlwind, 0),
        ]);

        let decision = decision(&session);
        let bloodletting = card_evidence(&decision, CardId::Bloodletting);
        assert!(bloodletting.delta.closed_threat_gaps.is_empty());
        assert!(bloodletting.delta.capability_improvements.is_empty());
        assert!(bloodletting.delta.resolved_formation_needs.is_empty());
        assert!(bloodletting.delta.added_formation_strengths.is_empty());
        assert_eq!(
            bloodletting.band,
            CardRewardPolicyBandV1::SpeculativeAddition
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Bloodletting,
                    ..
                }
            )),
            "an HP-for-energy conversion with no exact improvement must not outrank preserving deck quality; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn second_light_aoe_does_not_turn_supported_control_into_a_required_upgrade() {
        let mut session = reward_session(&[
            (CardId::WildStrike, 0),
            (CardId::Cleave, 0),
            (CardId::TwinStrike, 0),
        ]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 17;
        session.run_state.boss_key = Some(EncounterId::TheChamp);
        session.run_state.current_hp = 71;
        session.run_state.max_hp = 85;
        session.run_state.gold = 250;
        session.run_state.master_deck = [
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::PowerThrough, 1),
            (CardId::HeavyBlade, 0),
            (CardId::SecondWind, 1),
            (CardId::DarkEmbrace, 0),
            (CardId::ThunderClap, 0),
            (CardId::ShrugItOff, 0),
            (CardId::DemonForm, 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let decision = decision(&session);
        let cleave = card_evidence(&decision, CardId::Cleave);
        assert_eq!(
            card_analysis_profile_v1(CardId::Cleave, 0).aoe_support,
            CardAnalysisAoeSupportV1::Present
        );
        assert!(cleave.delta.closed_threat_gaps.is_empty());
        assert!(!cleave
            .delta
            .capability_improvements
            .iter()
            .any(|change| { change.capability == StrategyCapabilityKindV1::MultiTargetControl }));
        assert!(!cleave.improves_threat_relevant_capability);
        assert_eq!(cleave.band, CardRewardPolicyBandV1::SpeculativeAddition);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Cleave,
                    ..
                }
            )),
            "a second light AoE source must not outrank preserving this established deck; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn supported_exhaust_converter_survives_secondary_axis_debt() {
        let mut session = reward_session(&[
            (CardId::FiendFire, 0),
            (CardId::SwordBoomerang, 1),
            (CardId::Clash, 0),
        ]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 30;
        session.run_state.boss_key = Some(EncounterId::TheChamp);
        session.run_state.current_hp = 64;
        session.run_state.max_hp = 93;
        session.run_state.master_deck = [
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 1),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::Clothesline, 1),
            (CardId::IronWave, 0),
            (CardId::ThunderClap, 0),
            (CardId::BurningPact, 0),
            (CardId::Clash, 0),
            (CardId::ShrugItOff, 1),
            (CardId::DarkShackles, 0),
            (CardId::BattleTrance, 0),
            (CardId::FeelNoPain, 1),
            (CardId::FeelNoPain, 1),
            (CardId::TrueGrit, 1),
            (CardId::Disarm, 0),
            (CardId::BodySlam, 1),
            (CardId::PommelStrike, 0),
            (CardId::Entrench, 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let decision = decision(&session);
        let fiend_fire = card_evidence(&decision, CardId::FiendFire);
        assert!(matches!(
            &fiend_fire.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::ExhaustConversionSupported)
                && component_signals
                    .debt_signals
                    .contains(&CardComponentSignalKindV1::StrengthPayoffUnsupported)
        ));
        assert_eq!(
            fiend_fire.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::FiendFire,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "a visible exhaust conversion supported by the existing engine must not be erased by an optional strength axis; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn typed_fiend_fire_conversion_is_an_asset_without_a_strength_or_exhaust_package() {
        let mut session = reward_session(&[(CardId::FiendFire, 0), (CardId::TwinStrike, 0)]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 8;
        session.run_state.master_deck = [
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::Carnage,
            CardId::DoubleTap,
            CardId::Headbutt,
            CardId::Whirlwind,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();

        let decision = decision(&session);
        let fiend_fire = card_evidence(&decision, CardId::FiendFire);

        assert_eq!(
            fiend_fire.band,
            CardRewardPolicyBandV1::ImproveRequiredCapability
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::FiendFire,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "typed hand conversion must remain independently valuable without an optional strength source; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn intrinsic_asset_axis_survives_an_unfunded_secondary_package_axis() {
        let mut session = reward_session(&[(CardId::Whirlwind, 0), (CardId::HeavyBlade, 0)]);
        session.run_state.act_num = 3;
        session.run_state.floor_num = 46;
        session.run_state.boss_key = Some(EncounterId::DonuAndDeca);

        let decision = decision(&session);
        let whirlwind = card_evidence(&decision, CardId::Whirlwind);
        let heavy_blade = card_evidence(&decision, CardId::HeavyBlade);
        assert!(matches!(
            &whirlwind.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                semantics,
                component_signals,
                ..
            } if semantics
                .roles
                .contains(&CardRewardSemanticRoleV1::AoeDamage)
                && component_signals
                    .debt_signals
                    .contains(&CardComponentSignalKindV1::StrengthPayoffUnsupported)
        ));
        assert_ne!(whirlwind.band, CardRewardPolicyBandV1::Liability);
        assert_eq!(heavy_blade.band, CardRewardPolicyBandV1::Liability);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Whirlwind,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            ))
        );
    }

    #[test]
    fn random_multi_hit_reports_target_topology_without_claiming_control_or_asset_quality() {
        let mut session = reward_session(&[(CardId::SwordBoomerang, 0), (CardId::Entrench, 0)]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 28;
        session.run_state.boss_key = Some(EncounterId::TheChamp);
        session.run_state.master_deck = [
            (CardId::Defend, 1),
            (CardId::Defend, 1),
            (CardId::Defend, 1),
            (CardId::Defend, 1),
            (CardId::FeelNoPain, 1),
            (CardId::TrueGrit, 1),
            (CardId::BodySlam, 1),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let champ_decision = decision(&session);
        let sword_boomerang = card_evidence(&champ_decision, CardId::SwordBoomerang);
        assert!(sword_boomerang.random_target_frontload_reliable);
        assert!(matches!(
            &sword_boomerang.acquisition,
            CardRewardPolicyAcquisitionV1::Card { semantics, .. }
                if !semantics.roles.contains(&CardRewardSemanticRoleV1::AoeDamage)
        ));
        assert!(!sword_boomerang
            .delta
            .capability_improvements
            .iter()
            .any(|change| change.capability == StrategyCapabilityKindV1::MultiTargetControl));
        assert_eq!(sword_boomerang.band, CardRewardPolicyBandV1::Liability);
        assert!(matches!(
            &sword_boomerang.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .debt_signals
                .contains(&CardComponentSignalKindV1::StrengthPayoffUnsupported)
        ));

        session.run_state.act_num = 3;
        session.run_state.floor_num = 46;
        session.run_state.boss_key = Some(EncounterId::DonuAndDeca);
        let multi_target_decision = decision(&session);
        let sword_boomerang = card_evidence(&multi_target_decision, CardId::SwordBoomerang);
        assert!(!sword_boomerang.random_target_frontload_reliable);
        assert!(
            position(&multi_target_decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Entrench,
                    ..
                }
            )) < position(&multi_target_decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::SwordBoomerang,
                    ..
                }
            )),
            "random-target damage must not receive the single-opponent reliability credit in a multi-opponent boss fight; evidence={:#?}",
            multi_target_decision.evidence
        );
    }

    #[test]
    fn candidate_strength_burst_closes_uncontested_converter_and_payoff_package() {
        let mut session =
            reward_session(&[(CardId::Havoc, 1), (CardId::Flex, 1), (CardId::Pummel, 0)]);
        session.run_state.act_num = 3;
        session.run_state.floor_num = 45;
        session.run_state.boss_key = Some(EncounterId::DonuAndDeca);
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::ClockworkSouvenir));
        session.run_state.master_deck = [
            (CardId::FiendFire, 1),
            (CardId::SwordBoomerang, 0),
            (CardId::Reaper, 0),
            (CardId::BurningPact, 1),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let decision = decision(&session);
        let flex = card_evidence(&decision, CardId::Flex);
        assert!(matches!(
            &flex.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::StrengthConvertiblePackageUnlock)
                && component_signals
                    .note_signals
                    .contains(&CardComponentSignalKindV1::ConvertibleStrengthRequiresDrawTiming)
        ));
        assert_eq!(
            flex.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset,
            "flex={flex:#?}"
        );
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Flex,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "the exact candidate delta closes a convertible strength package and must outrank preserving deck quality; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn candidate_strength_burst_does_not_double_book_artifact_needed_by_existing_access() {
        let mut session =
            reward_session(&[(CardId::Havoc, 1), (CardId::Flex, 1), (CardId::Pummel, 0)]);
        session.run_state.act_num = 3;
        session.run_state.floor_num = 45;
        session.run_state.boss_key = Some(EncounterId::DonuAndDeca);
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::ClockworkSouvenir));
        session.run_state.master_deck = [
            (CardId::FiendFire, 1),
            (CardId::SwordBoomerang, 0),
            (CardId::Reaper, 0),
            (CardId::BattleTrance, 1),
            (CardId::BurningPact, 1),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let decision = decision(&session);
        let flex = card_evidence(&decision, CardId::Flex);
        assert!(matches!(
            &flex.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if !component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::StrengthConvertiblePackageUnlock)
        ));
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Flex,
                    ..
                }
            )),
            "one Artifact charge cannot simultaneously fund Battle Trance and permanent Flex; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn unsupported_exhaust_payoff_does_not_receive_shared_asset_status() {
        let session = reward_session(&[(CardId::FeelNoPain, 0)]);
        let decision = decision(&session);
        let fnp = card_evidence(&decision, CardId::FeelNoPain);
        assert!(matches!(
            &fnp.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .debt_signals
                .contains(&CardComponentSignalKindV1::ExhaustPayoffUnsupported)
        ));
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::FeelNoPain,
                    ..
                }
            )),
            "unsupported exhaust payoff must not be promoted above skip; evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn unsupported_strength_payoff_does_not_claim_scaling_capability_or_beat_skip() {
        let mut session = reward_session(&[
            (CardId::SwordBoomerang, 0),
            (CardId::GhostlyArmor, 0),
            (CardId::Anger, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 2;
        session.run_state.boss_key = Some(EncounterId::TheGuardian);
        session.run_state.master_deck = [
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
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();

        let decision = decision(&session);
        let payoff = card_evidence(&decision, CardId::SwordBoomerang);
        assert!(matches!(
            &payoff.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .debt_signals
                .contains(&CardComponentSignalKindV1::StrengthPayoffUnsupported)
        ));
        assert!(!payoff.improves_threat_relevant_capability);
        assert!(!payoff
            .delta
            .capability_improvements
            .iter()
            .any(|change| { change.capability == StrategyCapabilityKindV1::LongFightScaling }));
        assert_eq!(payoff.band, CardRewardPolicyBandV1::Liability);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::SwordBoomerang,
                    ..
                }
            )),
            "unsupported payoff must remain below preserving deck quality; evidence={:#?}",
            decision.evidence
        );

        for card in [CardId::GhostlyArmor, CardId::Anger] {
            let transition = card_evidence(&decision, card);
            assert!(transition
                .card_acquisition_policy
                .is_some_and(AcquisitionPolicyDecision::allows_acquisition));
            assert_eq!(
                transition.band,
                CardRewardPolicyBandV1::EstablishStrategicAsset,
                "transition={transition:#?}"
            );
            assert!(
                position(&decision, |key| matches!(
                    key,
                    DecisionCandidateKey::CardRewardPick {
                        card: candidate,
                        ..
                    } if *candidate == card
                )) < position(&decision, |key| matches!(
                    key,
                    DecisionCandidateKey::CardRewardSkip { .. }
                )),
                "transition={transition:#?}"
            );
        }

        let ghostly_armor = card_evidence(&decision, CardId::GhostlyArmor);
        let anger = card_evidence(&decision, CardId::Anger);
        assert_eq!(
            compare_card_reward_semantic_evidence(ghostly_armor, anger),
            Ordering::Equal,
            "neither transition role proves a strict preference"
        );
        let prior_probability = |candidate_id: &str| {
            decision
                .prior
                .entries
                .iter()
                .find(|entry| entry.candidate_id == candidate_id)
                .expect("transition prior")
                .probability
        };
        assert_eq!(
            prior_probability(&ghostly_armor.candidate_id),
            prior_probability(&anger.candidate_id),
            "surface order must not masquerade as semantic confidence"
        );
    }

    #[test]
    fn formation_progress_without_shared_construction_authorization_cannot_beat_skip() {
        let mut session = reward_session(&[(CardId::GhostlyArmor, 0)]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 2;
        session.run_state.master_deck = owned_deck(&[
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 0),
            (CardId::Disarm, 0),
        ]);

        let decision = decision(&session);
        let observed = card_evidence(&decision, CardId::GhostlyArmor);
        assert!(!observed.delta.resolved_formation_needs.is_empty());
        let mut acquisition = observed.acquisition.clone();
        let CardRewardPolicyAcquisitionV1::Card {
            component_signals,
            construction,
            ..
        } = &mut acquisition
        else {
            unreachable!();
        };
        construction.construction_roles.clear();
        component_signals.positive_signals = vec![CardComponentSignalKindV1::FormationNeedProgress];
        component_signals.debt_signals.clear();

        let band = card_reward_band_v1(
            &acquisition,
            &observed.delta,
            false,
            false,
            Some(AcquisitionPolicyDecision {
                verdict: crate::ai::strategy::acquisition::AcquisitionPolicyVerdict::Speculative,
                reason: crate::ai::strategy::acquisition::AcquisitionPolicyReason::NoOpenConstructionRole,
            }),
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
        );
        assert_eq!(band, CardRewardPolicyBandV1::SpeculativeAddition);
    }

    #[test]
    fn champ_obligation_prefers_persistent_scaling_to_transient_vulnerable() {
        let mut session = reward_session(&[(CardId::ThunderClap, 0), (CardId::Inflame, 1)]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 30;
        session.run_state.boss_key = Some(EncounterId::TheChamp);
        let decision = decision(&session);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Inflame,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::ThunderClap,
                    ..
                }
            )),
            "evidence={:#?}",
            decision.evidence
        );
    }

    #[test]
    fn persistent_strength_down_precedes_an_already_supported_damage_payoff() {
        let mut session = reward_session(&[
            (CardId::SwordBoomerang, 0),
            (CardId::Disarm, 0),
            (CardId::PerfectedStrike, 0),
        ]);
        session.run_state.act_num = 2;
        session.run_state.floor_num = 21;
        session.run_state.boss_key = Some(EncounterId::TheChamp);
        // Keep enough established offense that Sword Boomerang remains a
        // supported payoff; a smaller deck would test two missing capabilities.
        session.run_state.master_deck = [
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::PowerThrough, 0),
            (CardId::Uppercut, 0),
            (CardId::SecondWind, 0),
            (CardId::Cleave, 0),
            (CardId::ShrugItOff, 0),
            (CardId::DarkEmbrace, 0),
            (CardId::Uppercut, 0),
            (CardId::WildStrike, 0),
            (CardId::DarkEmbrace, 0),
            (CardId::FiendFire, 0),
            (CardId::Cleave, 0),
            (CardId::DemonForm, 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let decision = decision(&session);
        let sword_boomerang = card_evidence(&decision, CardId::SwordBoomerang);
        let disarm = card_evidence(&decision, CardId::Disarm);

        assert!(sword_boomerang.random_target_frontload_reliable);
        assert_eq!(
            sword_boomerang.band,
            CardRewardPolicyBandV1::EstablishStrategicAsset,
            "sword_boomerang={sword_boomerang:#?}"
        );
        assert!(disarm.improves_threat_relevant_capability);
        assert!(
            disarm.boss_survival_plan_repair.is_some(),
            "persistent strength reduction should expose its shared boss-survival repair; disarm={disarm:#?}"
        );
        assert_eq!(disarm.band, CardRewardPolicyBandV1::CloseThreatGap);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Disarm,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::SwordBoomerang,
                    ..
                }
            )),
            "persistent mitigation should close the defense quality gap before adding another supported payoff"
        );
    }

    #[test]
    fn pyramid_without_status_digest_exposes_wild_strike_liability() {
        let mut session = reward_session(&[(CardId::WildStrike, 0)]);
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::RunicPyramid));
        let decision = decision(&session);
        let wild = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::CardRewardPick {
                        card: CardId::WildStrike,
                        ..
                    }
                )
            })
            .expect("Wild Strike evidence");
        assert!(wild.introduces_undigested_status_burden);
        assert_eq!(wild.band, CardRewardPolicyBandV1::Liability);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::WildStrike,
                    ..
                }
            ))
        );
    }

    #[test]
    fn every_exact_card_reward_action_keeps_positive_support() {
        let mut session = reward_session(&[(CardId::WildStrike, 0), (CardId::BattleTrance, 1)]);
        session
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::BattleTrance, 10_001));
        let decision = decision(&session);
        assert_eq!(decision.prior.entries.len(), decision.evidence.len());
        assert!(decision
            .prior
            .entries
            .iter()
            .all(|entry| entry.probability.is_finite() && entry.probability > 0.0));
    }

    #[test]
    fn audit_exposes_every_ranked_candidate_without_changing_support() {
        let session = reward_session(&[
            (CardId::WildStrike, 0),
            (CardId::BattleTrance, 1),
            (CardId::DualWield, 0),
        ]);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let audit = exact_card_reward_policy_audit_v1(&session, &legal).expect("card reward audit");

        assert_eq!(audit.candidates.len(), legal.len());
        assert_eq!(audit.candidates[0].owner_rank, 0);
        assert!(audit
            .candidates
            .iter()
            .all(|candidate| !candidate.label.is_empty()
                && candidate.prior_probability.is_finite()
                && candidate.prior_probability > 0.0));
        assert!(audit.candidates.windows(2).all(|pair| {
            pair[0].owner_rank <= pair[1].owner_rank && pair[1].owner_rank <= pair[0].owner_rank + 1
        }));
    }

    #[test]
    fn plain_block_needs_actionable_evidence_but_intrinsic_payoff_remains_an_asset() {
        let mut session = reward_session(&[(CardId::Sentinel, 0), (CardId::Feed, 0)]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 11;
        session.run_state.boss_key = Some(EncounterId::TheGuardian);
        session.run_state.current_hp = 83;
        session.run_state.max_hp = 91;
        session.run_state.master_deck = [
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::Berserk, 0),
            (CardId::WildStrike, 0),
            (CardId::ShrugItOff, 0),
            (CardId::Evolve, 0),
            (CardId::Clothesline, 0),
            (CardId::Intimidate, 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();
        let decision = decision(&session);
        let sentinel = card_evidence(&decision, CardId::Sentinel);
        let feed = card_evidence(&decision, CardId::Feed);
        let skip = position(&decision, |key| {
            matches!(key, DecisionCandidateKey::CardRewardSkip { .. })
        });

        assert!(matches!(
            &sentinel.acquisition,
            CardRewardPolicyAcquisitionV1::Card { semantics, .. }
                if semantics.roles.contains(&CardRewardSemanticRoleV1::Block)
                    && semantics
                        .roles
                        .contains(&CardRewardSemanticRoleV1::EnergyOnExhaust)
                    && semantics
                        .roles
                        .contains(&CardRewardSemanticRoleV1::ExhaustPayoff)
        ));
        assert!(sentinel.delta.closed_threat_gaps.is_empty());
        assert!(sentinel.delta.capability_improvements.is_empty());
        assert!(sentinel.delta.resolved_formation_needs.is_empty());
        assert_eq!(sentinel.band, CardRewardPolicyBandV1::Liability);
        assert!(
            skip < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Sentinel,
                    ..
                }
            )),
            "plain block without actionable evidence must not outrank preserving deck quality"
        );

        assert!(matches!(
            &feed.acquisition,
            CardRewardPolicyAcquisitionV1::Card { semantics, .. }
                if semantics
                    .roles
                    .contains(&CardRewardSemanticRoleV1::CombatExternalPayoff)
        ));
        assert_eq!(feed.band, CardRewardPolicyBandV1::EstablishStrategicAsset);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Feed,
                    ..
                }
            )) < skip,
            "an intrinsic persistent payoff must remain independently admissible"
        );
    }

    #[test]
    fn known_power_tax_demotes_supported_minor_power_without_exact_improvement() {
        let mut session = reward_session(&[(CardId::Rupture, 1)]);
        session.run_state.act_num = 3;
        session.run_state.floor_num = 45;
        session.run_state.boss_key = Some(EncounterId::AwakenedOne);
        session.run_state.current_hp = 87;
        session.run_state.max_hp = 103;
        session.run_state.master_deck = [
            (CardId::Defend, 0),
            (CardId::Defend, 0),
            (CardId::Bash, 1),
            (CardId::Berserk, 0),
            (CardId::WildStrike, 0),
            (CardId::ShrugItOff, 1),
            (CardId::Evolve, 0),
            (CardId::Clothesline, 1),
            (CardId::Feed, 0),
            (CardId::Intimidate, 0),
            (CardId::BattleTrance, 0),
            (CardId::Shockwave, 0),
            (CardId::Barricade, 1),
            (CardId::FiendFire, 1),
            (CardId::IronWave, 1),
            (CardId::Disarm, 0),
            (CardId::PommelStrike, 0),
            (CardId::Immolate, 1),
            (CardId::Cleave, 0),
            (CardId::Inflame, 1),
            (CardId::DarkEmbrace, 1),
            (CardId::SecondWind, 1),
            (CardId::BodySlam, 0),
            (CardId::Bloodletting, 1),
            (CardId::DarkShackles, 1),
            (CardId::SeverSoul, 0),
            (CardId::Parasite, 0),
            (CardId::ThunderClap, 1),
            (CardId::FeelNoPain, 1),
            (CardId::Parasite, 0),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut owned = CombatCard::for_scenario(card, index as u32);
            owned.upgrades = upgrades;
            owned
        })
        .collect();

        let awakened_decision = decision(&session);
        let rupture = card_evidence(&awakened_decision, CardId::Rupture);
        assert!(matches!(
            &rupture.acquisition,
            CardRewardPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::SelfDamagePayoffSupported)
        ));
        assert!(rupture.delta.closed_threat_gaps.is_empty());
        assert!(rupture.delta.capability_improvements.is_empty());
        assert!(rupture.delta.resolved_formation_needs.is_empty());
        assert!(rupture.delta.added_formation_strengths.is_empty());
        assert!(rupture.boss_power_tax_conflict);
        assert_eq!(rupture.band, CardRewardPolicyBandV1::SpeculativeAddition);
        let mut exact_improvement = rupture.delta.clone();
        exact_improvement.capability_improvements.push(
            crate::eval::run_control::RunPolicyCapabilityChangeV1 {
                capability: StrategyCapabilityKindV1::LongFightScaling,
                before: StrategyCapabilityCoverageV1::Thin,
                after: StrategyCapabilityCoverageV1::Supported,
                before_inputs: Vec::new(),
                after_inputs: Vec::new(),
            },
        );
        assert!(
            !boss_power_tax_conflict_v1(true, &rupture.acquisition, &exact_improvement),
            "an exact capability improvement must override the generic minor-power conflict"
        );
        assert!(
            position(&awakened_decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )) < position(&awakened_decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Rupture,
                    ..
                }
            )),
            "a supported but non-improving minor power must not outrank preserving deck quality against the known power-tax boss"
        );

        session.run_state.boss_key = Some(EncounterId::DonuAndDeca);
        assert!(
            !card_evidence(&decision(&session), CardId::Rupture).boss_power_tax_conflict,
            "the conflict must come from the shared boss pressure, not from a Rupture-specific rejection"
        );
    }

    #[test]
    fn repeated_ordinary_frontload_needs_real_marginal_strategic_evidence() {
        let mut session = reward_session(&[
            (CardId::IronWave, 0),
            (CardId::WildStrike, 0),
            (CardId::DualWield, 0),
        ]);
        session.run_state.act_num = 1;
        session.run_state.floor_num = 1;
        session.run_state.current_hp = 80;
        session.run_state.max_hp = 80;
        session.run_state.master_deck = [
            CardId::Strike,
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
            // Isolate the ordinary-frontload contract from the independent
            // persistent draw-pile status gate.
            CardId::Evolve,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();

        let first_decision = decision(&session);
        assert!(
            position(&first_decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::WildStrike,
                    ..
                }
            )) < position(&first_decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "a first frontload card that closes exact current gaps remains admissible"
        );

        let mut second = reward_session(&[
            (CardId::PerfectedStrike, 0),
            (CardId::Clash, 0),
            (CardId::Warcry, 0),
        ]);
        second.run_state.act_num = 1;
        second.run_state.floor_num = 2;
        second.run_state.current_hp = 75;
        second.run_state.max_hp = 80;
        second.run_state.master_deck = session.run_state.master_deck.clone();
        second
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::WildStrike, 11));
        let second_decision = decision(&second);
        let skip = position(&second_decision, |key| {
            matches!(key, DecisionCandidateKey::CardRewardSkip { .. })
        });
        for card in [CardId::Clash, CardId::PerfectedStrike] {
            assert!(
                skip < position(&second_decision, |key| matches!(
                    key,
                    DecisionCandidateKey::CardRewardPick {
                        card: candidate,
                        ..
                    } if *candidate == card
                )),
                "{card:?} must not outrank skip merely by increasing an irrelevant capability or repeating an ordinary answer role; evidence={:#?}",
                second_decision.evidence
            );
        }

        let mut mitigation = reward_session(&[(CardId::Shockwave, 1)]);
        mitigation.run_state.act_num = 2;
        mitigation.run_state.floor_num = 17;
        mitigation.run_state.current_hp = 74;
        mitigation.run_state.max_hp = 80;
        mitigation.run_state.boss_key = Some(EncounterId::TheChamp);
        mitigation.run_state.master_deck = second.run_state.master_deck;
        let mitigation_decision = decision(&mitigation);
        assert!(
            position(&mitigation_decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Shockwave,
                    ..
                }
            )) < position(&mitigation_decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardSkip { .. }
            )),
            "a durable mitigation answer for the known Champ must remain independently admissible"
        );
    }

    #[test]
    fn unfunded_upgrade_investment_does_not_masquerade_as_immediate_work() {
        let session = reward_session(&[(CardId::SearingBlow, 0), (CardId::Armaments, 0)]);
        let decision = decision(&session);
        let searing = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::CardRewardPick {
                        card: CardId::SearingBlow,
                        ..
                    }
                )
            })
            .expect("Searing Blow evidence");

        assert_eq!(
            searing.upgrade_investment_support,
            Some(StrategyPlanSupportV1::Blocked)
        );
        assert_eq!(searing.band, CardRewardPolicyBandV1::Liability);
        assert!(
            position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::Armaments,
                    ..
                }
            )) < position(&decision, |key| matches!(
                key,
                DecisionCandidateKey::CardRewardPick {
                    card: CardId::SearingBlow,
                    ..
                }
            ))
        );
    }

    #[test]
    fn funded_upgrade_investment_keeps_its_underlying_policy_band() {
        assert_eq!(
            apply_upgrade_investment_gate_v1(
                CardRewardPolicyBandV1::CloseThreatGap,
                Some(StrategyPlanSupportV1::Strong),
            ),
            CardRewardPolicyBandV1::CloseThreatGap
        );
        assert_eq!(
            apply_upgrade_investment_gate_v1(
                CardRewardPolicyBandV1::CloseThreatGap,
                Some(StrategyPlanSupportV1::Plausible),
            ),
            CardRewardPolicyBandV1::SpeculativeAddition
        );
    }
}
