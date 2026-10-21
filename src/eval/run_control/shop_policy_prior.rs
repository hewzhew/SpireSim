use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::ai::action_supply_v1::{
    action_supply_delta_v1, action_supply_profile_v1, additional_play_target_support_v1,
    ActionSupplyDeltaV1,
};
use crate::ai::analysis::card_semantics::RunRewardKind;
use crate::ai::block_plan_profile_v1::{block_plan_profile_v1, BlockPlanProfileV1};
use crate::ai::card_component_signal_v1::{
    evaluate_card_component_signals_v1, is_unresolved_package_payoff_debt_signal_v1,
    CardComponentSignalContextV1, CardComponentSignalKindV1, CardComponentSignalReportV1,
};
use crate::ai::card_semantics_v1::{
    card_access_evidence_v1, card_mechanics_profile_v1, card_reward_facts_v1,
    card_reward_semantic_profile_v1, potion_acquisition_requirements_v1,
    potion_acquisition_traits_v1, potion_mechanics_profile_v1, relic_acquisition_requirements_v1,
    relic_acquisition_traits_v1, AcquisitionRequirementV1, CardAccessEvidenceV1,
    CardAccessLeverageV1, CardRewardSemanticProfileV1, PotionAcquisitionTraitV1,
    RelicAcquisitionTraitV1,
};
use crate::ai::combat_upgrade_coverage_v1::CombatUpgradeScopeV1;
use crate::ai::deck_mutation_compiler_v1::{
    best_duplicate_target_for_shop_v1, deck_removal_target_snapshots_v1,
    DeckMutationCardSnapshotV1, DeckMutationRemovalBenefitTierV1, DeckMutationTargetLossTierV1,
    DuplicateStackBehaviorV1, DuplicateTargetRoleV1,
};
use crate::ai::deck_shape_v1::{
    deck_shape_candidate_delta_v1, deck_shape_profile_v1, DeckShapeProfileV1, DeckShapeRiskV1,
};
use crate::ai::deck_startup_profile_v1::{deck_startup_profile_v1, DeckStartupProfileV1};
use crate::ai::noncombat_strategy_v1::{
    build_run_strategy_snapshot_from_run_state_v2, threat_relevant_capability_improvements_v1,
    StrategyCapabilityCoverageV1, StrategyCapabilityKindV1, StrategyDeckFormationNeedV1,
    StrategyPackageIdV2, StrategyThreatProfileV1, StrategyThreatSourceV1, StrategyThreatTagV1,
};
use crate::ai::route_window_facts::{
    build_route_path_family_from_target, route_window_targets, summarize_route_from_target,
    RouteWindowFactsConfig,
};
use crate::ai::strategy::acquisition::{
    assess_card_acquisition_from_construction_v1,
    assess_card_construction_with_component_signals_v1, evaluate_deck_construction_contract,
    AcquisitionContext, AcquisitionOpportunityCost, AcquisitionPolicyDecision,
    AcquisitionPolicyReason, AcquisitionPolicyVerdict, CardConstructionAssessmentV1,
};
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::reward_admission::assess_reward_admission_from_master_deck;
use crate::ai::strength_profile_v1::card_unlocks_convertible_strength_payoff_v1;
use crate::ai::strength_profile_v1::strength_profile_v1;
use crate::content::cards::{get_card_definition, upgrade_card_once_java, CardId, CardType};
use crate::content::potions::{get_potion_definition, potion_can_use_out_of_combat, PotionId};
use crate::content::relics::{energy_master_delta, RelicId};
use crate::runtime::combat::CardLineageId;
use crate::runtime::combat::CombatCard;
use crate::state::core::EngineState;
use crate::state::rewards::RewardCard;
use crate::state::run::RunState;

use super::{
    exact_run_policy_decision_v1, owned_run_policy_actions_v1, positive_ranked_run_policy_prior_v1,
    run_policy_state_delta_v1, DecisionCandidateKey, ExactRunPolicyActionSuccessorV1,
    ExactRunPolicyDecisionV1, RunControlSession, RunPolicyCandidateV1, RunPolicyCapabilityChangeV1,
    RunPolicyPriorV1, RunPolicyThreatGapKeyV1,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShopPolicyBandV1 {
    ResolvePendingBoundary,
    ImmediateSurvival,
    CloseThreatGap,
    AmplifyStrategicAccess,
    ImproveRequiredCapability,
    AcquireTacticalReserve,
    DeckRepair,
    EstablishStrategicAsset,
    PreserveResources,
    SpeculativePurchase,
    Liability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShopPolicyFollowupV1 {
    Shop,
    Reward,
    Selection,
    Map,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionRequirementSupportV1 {
    Current,
    PremiumDuplicateTarget {
        card: CardId,
        upgrades: u8,
        priority: i32,
        role: DuplicateTargetRoleV1,
        stack_behavior: DuplicateStackBehaviorV1,
    },
    OwnedUpgradePath {
        card: CardId,
        lineage: CardLineageId,
        upgrades_before: u8,
        energy_cost_before: i32,
        energy_cost_after: i32,
        energy_gain_before: i32,
        energy_gain_after: i32,
    },
    Unavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ShopPolicyAcquisitionV1 {
    Card {
        card: CardId,
        upgrades: u8,
        copies_before: usize,
        semantics: CardRewardSemanticProfileV1,
        access: Option<CardAccessEvidenceV1>,
        component_signals: CardComponentSignalReportV1,
        construction: CardConstructionAssessmentV1,
    },
    Relic {
        relic: RelicId,
        traits: Vec<RelicAcquisitionTraitV1>,
        requirements: Vec<AcquisitionRequirementV1>,
        requirement_support: AcquisitionRequirementSupportV1,
    },
    Potion {
        potion: PotionId,
        traits: Vec<PotionAcquisitionTraitV1>,
        combat_upgrade_scope: Option<CombatUpgradeScopeV1>,
        requirements: Vec<AcquisitionRequirementV1>,
        requirement_support: AcquisitionRequirementSupportV1,
    },
    Purge {
        card: CardId,
        upgrades: u8,
    },
    OpenRewards,
    Leave,
}

pub type ShopPolicyCapabilityChangeV1 = RunPolicyCapabilityChangeV1;
pub type ShopPolicyThreatGapKeyV1 = RunPolicyThreatGapKeyV1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelicThreatExposureV1 {
    Boss,
    ForcedEliteRoute,
    OptionalEliteRoute,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelicThreatMatchV1 {
    pub trait_: RelicAcquisitionTraitV1,
    pub threat: StrategyThreatTagV1,
    pub source: StrategyThreatSourceV1,
    pub subject: String,
    pub exposure: RelicThreatExposureV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShopPolicyActionEvidenceV1 {
    pub candidate_id: String,
    pub candidate_key: DecisionCandidateKey,
    pub acquisition: ShopPolicyAcquisitionV1,
    pub band: ShopPolicyBandV1,
    pub followup: ShopPolicyFollowupV1,
    pub gold_spent: i32,
    pub hp_gain: i32,
    pub max_hp_gain: i32,
    pub missing_hp_recovered: i32,
    pub guaranteed_realizable_hp_gain: i32,
    pub guaranteed_realizable_max_hp_gain: i32,
    pub conditional_combat_on_use_hp_capacity: i32,
    pub deck_size_delta: isize,
    pub closed_threat_gaps: Vec<ShopPolicyThreatGapKeyV1>,
    pub capability_improvements: Vec<ShopPolicyCapabilityChangeV1>,
    pub reinforced_threat_capabilities: Vec<StrategyCapabilityKindV1>,
    pub resolved_formation_needs: Vec<StrategyDeckFormationNeedV1>,
    pub added_formation_strengths: Vec<StrategyPackageIdV2>,
    pub matched_consumable_capabilities: Vec<StrategyCapabilityKindV1>,
    pub matched_consumable_packages: Vec<StrategyPackageIdV2>,
    pub matched_relic_threats: Vec<RelicThreatMatchV1>,
    pub upgrade_scope_before: Option<CombatUpgradeScopeV1>,
    pub upgrade_scope_after: Option<CombatUpgradeScopeV1>,
    pub introduces_status_burden: bool,
    pub added_deck_shape_risks: Vec<DeckShapeRiskV1>,
    pub introduces_package_debt: bool,
    pub redundant_upgrade_access: bool,
    pub card_acquisition_policy: Option<AcquisitionPolicyDecision>,
    pub card_acquisition_opportunity_cost: Option<AcquisitionOpportunityCost>,
    pub spent_gold_before_action: bool,
    pub durable_asset_support: bool,
    pub consumable_conversion_support: bool,
    pub action_supply: ActionSupplyDeltaV1,
    pub purge_target: Option<DeckMutationCardSnapshotV1>,
    surface_index: usize,
}

#[derive(Clone, Debug)]
pub struct ExactShopPolicyDecisionV1 {
    pub exact: ExactRunPolicyDecisionV1,
    pub evidence: Vec<ShopPolicyActionEvidenceV1>,
    pub prior: RunPolicyPriorV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShopPolicyAuditCandidateV1 {
    pub owner_rank: usize,
    pub candidate_id: String,
    pub label: String,
    pub candidate_key: DecisionCandidateKey,
    pub acquisition: String,
    pub acquisition_requirements: Vec<AcquisitionRequirementV1>,
    pub acquisition_requirement_support: Option<AcquisitionRequirementSupportV1>,
    pub band: ShopPolicyBandV1,
    pub followup: ShopPolicyFollowupV1,
    pub gold_spent: i32,
    pub hp_gain: i32,
    pub max_hp_gain: i32,
    pub missing_hp_recovered: i32,
    pub guaranteed_realizable_hp_gain: i32,
    pub guaranteed_realizable_max_hp_gain: i32,
    pub conditional_combat_on_use_hp_capacity: i32,
    pub deck_size_delta: isize,
    pub closed_threat_gaps: Vec<String>,
    pub capability_improvements: Vec<String>,
    pub reinforced_threat_capabilities: Vec<String>,
    pub resolved_formation_needs: Vec<String>,
    pub added_formation_strengths: Vec<String>,
    pub matched_consumable_capabilities: Vec<String>,
    pub matched_consumable_packages: Vec<String>,
    pub matched_relic_threats: Vec<String>,
    pub upgrade_scope_before: Option<String>,
    pub upgrade_scope_after: Option<String>,
    pub introduces_status_burden: bool,
    pub added_deck_shape_risks: Vec<String>,
    pub introduces_package_debt: bool,
    pub redundant_upgrade_access: bool,
    pub card_acquisition_verdict: Option<AcquisitionPolicyVerdict>,
    pub card_acquisition_reason: Option<AcquisitionPolicyReason>,
    pub card_acquisition_opportunity_cost: Option<AcquisitionOpportunityCost>,
    pub spent_gold_before_action: bool,
    pub durable_asset_support: bool,
    pub consumable_conversion_support: bool,
    pub action_supply: ActionSupplyDeltaV1,
    pub purge_target_class: Option<String>,
    pub purge_removal_benefit: Option<String>,
    pub purge_removal_signals: Vec<String>,
    pub purge_compensating_signals: Vec<String>,
    pub purge_target_loss: Option<String>,
    pub surface_index: usize,
    pub prior_probability: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactShopPolicyAuditV1 {
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub candidates: Vec<ShopPolicyAuditCandidateV1>,
}

impl ExactShopPolicyDecisionV1 {
    pub fn audit(
        &self,
        legal: &[RunPolicyCandidateV1<'_>],
    ) -> Result<ExactShopPolicyAuditV1, String> {
        let candidates = self
            .evidence
            .iter()
            .enumerate()
            .map(|(owner_rank, evidence)| {
                let legal_candidate = legal
                    .iter()
                    .find(|candidate| candidate.candidate_id == evidence.candidate_id)
                    .ok_or_else(|| {
                        format!(
                            "shop policy audit could not find legal candidate '{}'",
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
                            "shop policy audit could not find prior for candidate '{}'",
                            evidence.candidate_id
                        )
                    })?;
                Ok(ShopPolicyAuditCandidateV1 {
                    owner_rank,
                    candidate_id: evidence.candidate_id.clone(),
                    label: legal_candidate.label.to_string(),
                    candidate_key: evidence.candidate_key.clone(),
                    acquisition: format!("{:?}", evidence.acquisition),
                    acquisition_requirements: acquisition_requirements_v1(&evidence.acquisition)
                        .to_vec(),
                    acquisition_requirement_support: acquisition_requirement_support_for_audit_v1(
                        &evidence.acquisition,
                    ),
                    band: evidence.band,
                    followup: evidence.followup,
                    gold_spent: evidence.gold_spent,
                    hp_gain: evidence.hp_gain,
                    max_hp_gain: evidence.max_hp_gain,
                    missing_hp_recovered: evidence.missing_hp_recovered,
                    guaranteed_realizable_hp_gain: evidence.guaranteed_realizable_hp_gain,
                    guaranteed_realizable_max_hp_gain: evidence.guaranteed_realizable_max_hp_gain,
                    conditional_combat_on_use_hp_capacity: evidence
                        .conditional_combat_on_use_hp_capacity,
                    deck_size_delta: evidence.deck_size_delta,
                    closed_threat_gaps: evidence
                        .closed_threat_gaps
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect(),
                    capability_improvements: evidence
                        .capability_improvements
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect(),
                    reinforced_threat_capabilities: evidence
                        .reinforced_threat_capabilities
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect(),
                    resolved_formation_needs: evidence
                        .resolved_formation_needs
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect(),
                    added_formation_strengths: evidence
                        .added_formation_strengths
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect(),
                    matched_consumable_capabilities: evidence
                        .matched_consumable_capabilities
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect(),
                    matched_consumable_packages: evidence
                        .matched_consumable_packages
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect(),
                    matched_relic_threats: evidence
                        .matched_relic_threats
                        .iter()
                        .map(|value| {
                            format!(
                                "{:?}->{:?}@{:?}:{}:{:?}",
                                value.trait_,
                                value.threat,
                                value.source,
                                value.subject,
                                value.exposure
                            )
                        })
                        .collect(),
                    upgrade_scope_before: evidence
                        .upgrade_scope_before
                        .map(|value| format!("{value:?}")),
                    upgrade_scope_after: evidence
                        .upgrade_scope_after
                        .map(|value| format!("{value:?}")),
                    introduces_status_burden: evidence.introduces_status_burden,
                    added_deck_shape_risks: evidence
                        .added_deck_shape_risks
                        .iter()
                        .map(|value| format!("{value:?}"))
                        .collect(),
                    introduces_package_debt: evidence.introduces_package_debt,
                    redundant_upgrade_access: evidence.redundant_upgrade_access,
                    card_acquisition_verdict: evidence
                        .card_acquisition_policy
                        .map(|policy| policy.verdict),
                    card_acquisition_reason: evidence
                        .card_acquisition_policy
                        .map(|policy| policy.reason),
                    card_acquisition_opportunity_cost: evidence.card_acquisition_opportunity_cost,
                    spent_gold_before_action: evidence.spent_gold_before_action,
                    durable_asset_support: evidence.durable_asset_support,
                    consumable_conversion_support: evidence.consumable_conversion_support,
                    action_supply: evidence.action_supply,
                    purge_target_class: evidence
                        .purge_target
                        .as_ref()
                        .map(|target| format!("{:?}", target.target_class)),
                    purge_removal_benefit: evidence
                        .purge_target
                        .as_ref()
                        .map(|target| format!("{:?}", target.removal_benefit.tier)),
                    purge_removal_signals: evidence
                        .purge_target
                        .as_ref()
                        .map(|target| target.removal_benefit.signals.clone())
                        .unwrap_or_default(),
                    purge_compensating_signals: evidence
                        .purge_target
                        .as_ref()
                        .map(|target| target.removal_benefit.compensating_signals.clone())
                        .unwrap_or_default(),
                    purge_target_loss: evidence
                        .purge_target
                        .as_ref()
                        .map(|target| format!("{:?}", target.target_loss.tier)),
                    surface_index: evidence.surface_index,
                    prior_probability,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(ExactShopPolicyAuditV1 {
            current_hp: self.exact.before.resources.current_hp,
            max_hp: self.exact.before.resources.max_hp,
            gold: self.exact.before.resources.gold,
            candidates,
        })
    }
}

pub fn exact_shop_policy_audit_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactShopPolicyAuditV1, String> {
    exact_shop_policy_decision_v1(session, legal)?.audit(legal)
}

pub fn exact_shop_policy_prior_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<RunPolicyPriorV1, String> {
    Ok(exact_shop_policy_decision_v1(session, legal)?.prior)
}

pub fn exact_shop_policy_decision_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactShopPolicyDecisionV1, String> {
    if !matches!(session.engine_state, EngineState::Shop(_)) {
        return Err("exact shop policy requires a Shop decision boundary".to_string());
    }

    let exact = exact_run_policy_decision_v1(session)?;
    let owned = owned_run_policy_actions_v1(&exact, legal, "shop", is_shop_key)?;
    let purge_targets = deck_removal_target_snapshots_v1(&session.run_state)
        .into_iter()
        .map(|snapshot| (snapshot.deck_index, snapshot))
        .collect::<BTreeMap<_, _>>();
    let strategy = build_run_strategy_snapshot_from_run_state_v2(&session.run_state);
    let formation_needs = strategy.formation_summary().needs;
    let startup = deck_startup_profile_v1(&session.run_state);
    let deck_shape = deck_shape_profile_v1(&session.run_state);
    let block_plan = block_plan_profile_v1(&session.run_state);
    let deck_plan = DeckPlanSnapshot::from_run_state(&session.run_state);
    let action_supply_before = action_supply_profile_v1(&session.run_state);
    let mut evidence = owned
        .into_iter()
        .map(|(surface_index, action)| {
            shop_action_evidence_v1(
                session,
                &exact,
                action,
                surface_index,
                &purge_targets,
                &formation_needs,
                &startup,
                &deck_shape,
                &block_plan,
                deck_plan,
                &action_supply_before,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    evidence.sort_by(compare_shop_evidence);
    let prior = positive_ranked_run_policy_prior_v1(
        legal,
        evidence
            .iter()
            .map(|candidate| candidate.candidate_id.clone()),
    )?;

    Ok(ExactShopPolicyDecisionV1 {
        exact,
        evidence,
        prior,
    })
}

fn is_shop_key(key: &DecisionCandidateKey) -> bool {
    matches!(
        key,
        DecisionCandidateKey::ShopBuyCard { .. }
            | DecisionCandidateKey::ShopBuyRelic { .. }
            | DecisionCandidateKey::ShopBuyPotion { .. }
            | DecisionCandidateKey::ShopPurgeCard { .. }
            | DecisionCandidateKey::ShopOpenRewards
            | DecisionCandidateKey::ShopLeave
    )
}

fn shop_action_evidence_v1(
    parent: &RunControlSession,
    decision: &ExactRunPolicyDecisionV1,
    action: &ExactRunPolicyActionSuccessorV1,
    surface_index: usize,
    purge_targets: &BTreeMap<usize, DeckMutationCardSnapshotV1>,
    formation_needs: &[StrategyDeckFormationNeedV1],
    startup: &DeckStartupProfileV1,
    deck_shape: &DeckShapeProfileV1,
    block_plan: &BlockPlanProfileV1,
    deck_plan: DeckPlanSnapshot,
    action_supply_before: &crate::ai::action_supply_v1::ActionSupplyProfileV1,
) -> Result<ShopPolicyActionEvidenceV1, String> {
    let candidate_key = action
        .candidate_key
        .clone()
        .ok_or_else(|| format!("shop candidate '{}' has no typed key", action.candidate_id))?;
    let acquisition = acquisition_v1(
        parent,
        &candidate_key,
        formation_needs,
        startup,
        block_plan,
        deck_plan,
    )?;
    let (card_acquisition_policy, card_acquisition_opportunity_cost) =
        card_acquisition_policy_v1(parent, &candidate_key, &acquisition, deck_plan);
    let delta = run_policy_state_delta_v1(&decision.before, &action.after);
    let closed_threat_gaps = delta.closed_threat_gaps;
    let capability_improvements = delta.capability_improvements;
    let threat_relevant_improvements = threat_relevant_capability_improvements_v1(
        &decision.before.threats,
        &decision.before.threat_coverage,
        &action.after.threat_coverage,
    );
    let reinforced_threat_capabilities = capability_improvements
        .iter()
        .filter(|change| {
            change.before == StrategyCapabilityCoverageV1::Supported
                && change.after == StrategyCapabilityCoverageV1::Strong
                && threat_relevant_improvements.contains(&change.capability)
        })
        .map(|change| change.capability)
        .collect::<Vec<_>>();
    let resolved_formation_needs = delta.resolved_formation_needs;
    let added_formation_strengths = delta.added_formation_strengths;
    let matched_consumable_capabilities =
        matched_consumable_capabilities_v1(&decision.before, &acquisition);
    let matched_consumable_packages =
        matched_consumable_packages_v1(&parent.run_state, &acquisition);
    let matched_relic_threats =
        matched_relic_threats_v1(parent, &decision.before.threats, &acquisition);
    let upgrade_scope_before = decision.before.combat_upgrade_coverage.strongest_scope();
    let upgrade_scope_after = action.after.combat_upgrade_coverage.strongest_scope();
    let introduces_status_burden = action.after.deck.status_generators
        > decision.before.deck.status_generators
        && action.after.deck.status_payoffs == 0
        && parent
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::RunicPyramid);
    let added_deck_shape_risks = match &acquisition {
        ShopPolicyAcquisitionV1::Card { card, .. } => {
            deck_shape_candidate_delta_v1(deck_shape, *card).risks
        }
        _ => Vec::new(),
    };
    let introduces_package_debt = matches!(
        &acquisition,
        ShopPolicyAcquisitionV1::Card {
            component_signals,
            ..
        } if component_signals
            .debt_signals
            .iter()
            .any(|signal| is_unresolved_package_payoff_debt_signal_v1(*signal))
    );
    let redundant_upgrade_access = matches!(
        acquisition,
        ShopPolicyAcquisitionV1::Card {
            card: CardId::Armaments | CardId::Apotheosis,
            ..
        }
    ) && upgrade_scope_before.is_some()
        && upgrade_scope_after == upgrade_scope_before;
    let purge_target = match &candidate_key {
        DecisionCandidateKey::ShopPurgeCard { deck_index, .. } => {
            purge_targets.get(deck_index).cloned()
        }
        _ => None,
    };
    let followup = followup_v1(&action.exact.session.engine_state);
    let spent_gold_before_action = parent
        .shop_visit_context()
        .is_some_and(|context| context.spent_gold_in_visit);
    let action_supply = action_supply_delta_v1(
        action_supply_before,
        &action_supply_profile_v1(&action.exact.session.run_state),
    );
    let (guaranteed_realizable_hp_gain, guaranteed_realizable_max_hp_gain) =
        guaranteed_realizable_potion_run_resource_gain_v1(
            &action.exact.session.run_state,
            &acquisition,
        );
    let conditional_combat_on_use_hp_capacity = conditional_combat_on_use_potion_hp_capacity_v1(
        &action.exact.session.run_state,
        &acquisition,
    );
    let conditional_combat_recovery_support =
        conditional_combat_on_use_hp_capacity > 0 && next_route_step_forces_combat_v1(parent);
    let reinforcement_has_card_policy_support =
        reinforcement_has_card_policy_support_v1(card_acquisition_policy);
    let consumable_conversion_support =
        matches!(acquisition, ShopPolicyAcquisitionV1::Potion { .. })
            && (!matched_consumable_capabilities.is_empty()
                || !matched_consumable_packages.is_empty()
                || conditional_combat_recovery_support
                || non_card_strategic_acquisition_supported(parent, &acquisition, followup));
    let durable_action_supply = action_supply_has_durable_marginal_support_v1(
        &acquisition,
        card_acquisition_policy,
        action_supply,
    );
    // A broad card role remains useful audit evidence, but cannot by itself
    // justify consuming a stronger shop alternative such as purge reserve.
    // Consumables have a separate finite conversion claim below; they are not
    // durable merely because they answer one public threat once.
    let durable_asset_support = first_copy_efficient_access(&acquisition)
        || (!reinforced_threat_capabilities.is_empty() && reinforcement_has_card_policy_support)
        || matched_relic_threats
            .iter()
            .any(|match_| match_.exposure.is_committed())
        || !added_formation_strengths.is_empty()
        || upgrade_scope_after > upgrade_scope_before
        || durable_action_supply
        || card_gold_run_reward_conversion_supported_v1(parent, &acquisition)
        || card_policy_has_construction_acquisition_support_v1(
            &acquisition,
            card_acquisition_policy,
        )
        || guaranteed_realizable_max_hp_gain > 0
        || (matches!(acquisition, ShopPolicyAcquisitionV1::Relic { .. })
            && non_card_strategic_acquisition_supported(parent, &acquisition, followup));
    let purchase_support = durable_asset_support || consumable_conversion_support;
    let opens_completed_rewards = followup == ShopPolicyFollowupV1::Reward
        && super::reward_policy_progress_v1(&action.exact.session)?.is_complete();
    let gold_spent = decision
        .before
        .resources
        .gold
        .saturating_sub(action.after.resources.gold);
    let hp_gain = action
        .after
        .resources
        .current_hp
        .saturating_sub(decision.before.resources.current_hp);
    let max_hp_gain = action
        .after
        .resources
        .max_hp
        .saturating_sub(decision.before.resources.max_hp);
    let missing_hp_before = decision
        .before
        .resources
        .max_hp
        .saturating_sub(decision.before.resources.current_hp);
    let missing_hp_after = action
        .after
        .resources
        .max_hp
        .saturating_sub(action.after.resources.current_hp);
    let missing_hp_recovered = missing_hp_before.saturating_sub(missing_hp_after);
    let deck_size_delta =
        action.after.deck.deck_size as isize - decision.before.deck.deck_size as isize;
    let band = shop_policy_band_v1(
        parent,
        &acquisition,
        followup,
        opens_completed_rewards,
        max_hp_gain,
        missing_hp_recovered,
        guaranteed_realizable_max_hp_gain,
        conditional_combat_recovery_support,
        deck_size_delta,
        &closed_threat_gaps,
        &reinforced_threat_capabilities,
        &resolved_formation_needs,
        &added_formation_strengths,
        &matched_consumable_packages,
        &matched_relic_threats,
        upgrade_scope_before,
        upgrade_scope_after,
        introduces_status_burden,
        !added_deck_shape_risks.is_empty(),
        introduces_package_debt,
        redundant_upgrade_access,
        card_acquisition_policy,
        card_acquisition_opportunity_cost,
        spent_gold_before_action,
        purchase_support,
        action_supply,
        purge_target.as_ref(),
    );

    Ok(ShopPolicyActionEvidenceV1 {
        candidate_id: action.candidate_id.clone(),
        candidate_key,
        acquisition,
        band,
        followup,
        gold_spent,
        hp_gain,
        max_hp_gain,
        missing_hp_recovered,
        guaranteed_realizable_hp_gain,
        guaranteed_realizable_max_hp_gain,
        conditional_combat_on_use_hp_capacity,
        deck_size_delta,
        closed_threat_gaps,
        capability_improvements,
        reinforced_threat_capabilities,
        resolved_formation_needs,
        added_formation_strengths,
        matched_consumable_capabilities,
        matched_consumable_packages,
        matched_relic_threats,
        upgrade_scope_before,
        upgrade_scope_after,
        introduces_status_burden,
        added_deck_shape_risks,
        introduces_package_debt,
        redundant_upgrade_access,
        card_acquisition_policy,
        card_acquisition_opportunity_cost,
        spent_gold_before_action,
        durable_asset_support,
        consumable_conversion_support,
        action_supply,
        purge_target,
        surface_index,
    })
}

fn card_acquisition_policy_v1(
    parent: &RunControlSession,
    candidate_key: &DecisionCandidateKey,
    acquisition: &ShopPolicyAcquisitionV1,
    deck_plan: DeckPlanSnapshot,
) -> (
    Option<AcquisitionPolicyDecision>,
    Option<AcquisitionOpportunityCost>,
) {
    let (
        DecisionCandidateKey::ShopBuyCard { price, .. },
        ShopPolicyAcquisitionV1::Card {
            card,
            upgrades,
            construction,
            ..
        },
    ) = (candidate_key, acquisition)
    else {
        return (None, None);
    };
    let purge_reserve = match &parent.engine_state {
        EngineState::Shop(shop)
            if shop.purge_available && parent.run_state.gold >= shop.purge_cost =>
        {
            Some(shop.purge_cost)
        }
        _ => None,
    };
    let admission =
        assess_reward_admission_from_master_deck(&parent.run_state.master_deck, *card, *upgrades);
    let report = assess_card_acquisition_from_construction_v1(
        AcquisitionContext::shop_with_purge_reserve(
            deck_plan,
            parent.run_state.gold,
            *price,
            purge_reserve,
        ),
        *card,
        *upgrades,
        &admission,
        construction.clone(),
    );
    (
        Some(evaluate_deck_construction_contract(&report)),
        Some(report.opportunity_cost),
    )
}

fn acquisition_v1(
    parent: &RunControlSession,
    key: &DecisionCandidateKey,
    formation_needs: &[StrategyDeckFormationNeedV1],
    startup: &DeckStartupProfileV1,
    block_plan: &BlockPlanProfileV1,
    deck_plan: DeckPlanSnapshot,
) -> Result<ShopPolicyAcquisitionV1, String> {
    Ok(match key {
        DecisionCandidateKey::ShopBuyCard { card, upgrades, .. } => {
            let copies_before = parent
                .run_state
                .master_deck
                .iter()
                .filter(|owned| owned.id == *card)
                .count();
            let reward = RewardCard::new(*card, *upgrades);
            let semantics = card_reward_semantic_profile_v1(&reward);
            let admission = assess_reward_admission_from_master_deck(
                &parent.run_state.master_deck,
                *card,
                *upgrades,
            );
            let component_signals = evaluate_card_component_signals_v1(
                &CardComponentSignalContextV1 {
                    same_card_count: copies_before,
                    candidate_upgrades: *upgrades,
                    formation_needs: formation_needs.to_vec(),
                    startup: startup.clone(),
                    block_plan: block_plan.clone(),
                    candidate_unlocks_convertible_strength_payoff:
                        card_unlocks_convertible_strength_payoff_v1(
                            &parent.run_state,
                            *card,
                            *upgrades,
                        ),
                    additional_play_target_supported: card_mechanics_profile_v1(*card)
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
                deck_plan,
                *card,
                *upgrades,
                &admission,
                &component_signals,
            );
            ShopPolicyAcquisitionV1::Card {
                card: *card,
                upgrades: *upgrades,
                copies_before,
                access: card_access_evidence_v1(&reward),
                component_signals,
                construction,
                semantics,
            }
        }
        DecisionCandidateKey::ShopBuyRelic { relic, .. } => {
            let requirements = relic_acquisition_requirements_v1(*relic);
            let requirement_support =
                acquisition_requirements_support_v1(parent, requirements.as_slice());
            ShopPolicyAcquisitionV1::Relic {
                relic: *relic,
                traits: relic_acquisition_traits_v1(*relic),
                requirements,
                requirement_support,
            }
        }
        DecisionCandidateKey::ShopBuyPotion { potion, .. } => {
            let requirements = potion_acquisition_requirements_v1(*potion);
            let requirement_support =
                acquisition_requirements_support_v1(parent, requirements.as_slice());
            ShopPolicyAcquisitionV1::Potion {
                potion: *potion,
                traits: potion_acquisition_traits_v1(*potion),
                combat_upgrade_scope: potion_mechanics_profile_v1(*potion).combat_upgrade_scope,
                requirements,
                requirement_support,
            }
        }
        DecisionCandidateKey::ShopPurgeCard { card, upgrades, .. } => {
            ShopPolicyAcquisitionV1::Purge {
                card: *card,
                upgrades: *upgrades,
            }
        }
        DecisionCandidateKey::ShopOpenRewards => ShopPolicyAcquisitionV1::OpenRewards,
        DecisionCandidateKey::ShopLeave => ShopPolicyAcquisitionV1::Leave,
        other => {
            return Err(format!(
                "exact shop policy received non-shop candidate key {other:?}"
            ))
        }
    })
}

fn acquisition_requirements_support_v1(
    parent: &RunControlSession,
    requirements: &[AcquisitionRequirementV1],
) -> AcquisitionRequirementSupportV1 {
    let mut aggregate = AcquisitionRequirementSupportV1::Current;
    for requirement in requirements {
        match acquisition_requirement_support_v1(parent, *requirement) {
            AcquisitionRequirementSupportV1::Unavailable => {
                return AcquisitionRequirementSupportV1::Unavailable;
            }
            support @ AcquisitionRequirementSupportV1::OwnedUpgradePath { .. } => {
                if aggregate == AcquisitionRequirementSupportV1::Current {
                    aggregate = support;
                }
            }
            support @ AcquisitionRequirementSupportV1::PremiumDuplicateTarget { .. } => {
                aggregate = support;
            }
            AcquisitionRequirementSupportV1::Current => {}
        }
    }
    aggregate
}

fn acquisition_requirement_support_v1(
    parent: &RunControlSession,
    requirement: AcquisitionRequirementV1,
) -> AcquisitionRequirementSupportV1 {
    let satisfied = match requirement {
        AcquisitionRequirementV1::XCostPayoff => parent
            .run_state
            .master_deck
            .iter()
            .any(|card| get_card_definition(card.id).cost == -1),
        AcquisitionRequirementV1::DuplicateTarget => {
            return best_duplicate_target_for_shop_v1(&parent.run_state)
                .map(
                    |target| AcquisitionRequirementSupportV1::PremiumDuplicateTarget {
                        card: target.card,
                        upgrades: target.upgrades,
                        priority: target.priority,
                        role: target.role,
                        stack_behavior: target.stack_behavior,
                    },
                )
                .unwrap_or(AcquisitionRequirementSupportV1::Unavailable);
        }
        AcquisitionRequirementV1::AttackSkillPowerSameTurn => {
            return attack_skill_power_activation_support_v1(&parent.run_state);
        }
        AcquisitionRequirementV1::LowHpDeathInsurance => {
            parent.run_state.current_hp.saturating_mul(2) <= parent.run_state.max_hp
        }
        AcquisitionRequirementV1::RouteEscapeValue => route_escape_value_v1(parent),
        AcquisitionRequirementV1::UpgradeableCombatCards => {
            parent.run_state.master_deck.iter().any(|card| {
                let mut upgraded = card.clone();
                upgrade_card_once_java(&mut upgraded)
            })
        }
    };
    if satisfied {
        AcquisitionRequirementSupportV1::Current
    } else {
        AcquisitionRequirementSupportV1::Unavailable
    }
}

fn attack_skill_power_activation_support_v1(
    run_state: &RunState,
) -> AcquisitionRequirementSupportV1 {
    let energy = 3_i32.saturating_add(
        run_state
            .relics
            .iter()
            .map(|relic| i32::from(energy_master_delta(relic.id)))
            .sum(),
    );
    if attack_skill_power_sequence_exists_v1(&run_state.master_deck, energy) {
        return AcquisitionRequirementSupportV1::Current;
    }

    for index in 0..run_state.master_deck.len() {
        let mut upgraded = run_state.master_deck.clone();
        let energy_cost_before = upgraded[index].combat_cost_without_turn_override_java();
        let energy_gain_before = activation_card_energy_gain_v1(&upgraded[index]);
        if !upgrade_card_once_java(&mut upgraded[index]) {
            continue;
        }
        let energy_cost_after = upgraded[index].combat_cost_without_turn_override_java();
        let energy_gain_after = activation_card_energy_gain_v1(&upgraded[index]);
        if energy_cost_before == energy_cost_after && energy_gain_before == energy_gain_after {
            continue;
        }
        if attack_skill_power_sequence_exists_v1(&upgraded, energy) {
            return AcquisitionRequirementSupportV1::OwnedUpgradePath {
                card: upgraded[index].id,
                lineage: upgraded[index].lineage,
                upgrades_before: upgraded[index].upgrades.saturating_sub(1),
                energy_cost_before,
                energy_cost_after,
                energy_gain_before,
                energy_gain_after,
            };
        }
    }
    AcquisitionRequirementSupportV1::Unavailable
}

#[derive(Clone, Copy)]
struct ActivationCardV1 {
    card_type: CardType,
    energy_cost: i32,
    energy_gain: i32,
}

fn attack_skill_power_sequence_exists_v1(deck: &[CombatCard], starting_energy: i32) -> bool {
    let cards = deck
        .iter()
        .filter_map(|card| {
            let card_type = get_card_definition(card.id).card_type;
            if !matches!(
                card_type,
                CardType::Attack | CardType::Skill | CardType::Power
            ) {
                return None;
            }
            Some(ActivationCardV1 {
                card_type,
                energy_cost: card.combat_cost_without_turn_override_java(),
                energy_gain: activation_card_energy_gain_v1(card),
            })
        })
        .collect::<Vec<_>>();
    let attacks = cards
        .iter()
        .copied()
        .filter(|card| card.card_type == CardType::Attack)
        .collect::<Vec<_>>();
    let skills = cards
        .iter()
        .copied()
        .filter(|card| card.card_type == CardType::Skill)
        .collect::<Vec<_>>();
    let powers = cards
        .iter()
        .copied()
        .filter(|card| card.card_type == CardType::Power)
        .collect::<Vec<_>>();

    attacks.iter().copied().any(|attack| {
        skills.iter().copied().any(|skill| {
            powers.iter().copied().any(|power| {
                let cards = [attack, skill, power];
                [
                    [0, 1, 2],
                    [0, 2, 1],
                    [1, 0, 2],
                    [1, 2, 0],
                    [2, 0, 1],
                    [2, 1, 0],
                ]
                .into_iter()
                .any(|order| activation_sequence_is_payable_v1(cards, order, starting_energy))
            })
        })
    })
}

fn activation_card_energy_gain_v1(card: &CombatCard) -> i32 {
    card_reward_facts_v1(&RewardCard::new(card.id, card.upgrades)).energy_gain
}

fn activation_sequence_is_payable_v1(
    cards: [ActivationCardV1; 3],
    order: [usize; 3],
    starting_energy: i32,
) -> bool {
    let mut energy = starting_energy;
    for index in order {
        let card = cards[index];
        if card.energy_cost < 0 {
            energy = 0;
        } else {
            if card.energy_cost > energy {
                return false;
            }
            energy = energy
                .saturating_sub(card.energy_cost)
                .saturating_add(card.energy_gain);
        }
    }
    true
}

fn route_escape_value_v1(parent: &RunControlSession) -> bool {
    route_window_targets(&parent.run_state)
        .into_iter()
        .any(|target| {
            let family = build_route_path_family_from_target(
                &parent.run_state,
                target.x,
                target.y,
                RouteWindowFactsConfig {
                    horizon_nodes: 5,
                    path_budget: 2_000,
                },
            );
            family.paths.iter().any(|path| {
                path.nodes.iter().any(|node| {
                    matches!(
                        node.room_type,
                        Some(
                            crate::state::map::node::RoomType::MonsterRoom
                                | crate::state::map::node::RoomType::MonsterRoomElite
                        )
                    )
                })
            })
        })
}

fn matched_consumable_capabilities_v1(
    before: &super::RunPolicyStateEvidenceV1,
    acquisition: &ShopPolicyAcquisitionV1,
) -> Vec<StrategyCapabilityKindV1> {
    let ShopPolicyAcquisitionV1::Potion { potion, traits, .. } = acquisition else {
        return Vec::new();
    };
    let required = before
        .threat_coverage
        .gaps
        .iter()
        .flat_map(|gap| gap.required_capabilities.iter().copied())
        .collect::<BTreeSet<_>>();
    let supplied = traits
        .iter()
        .flat_map(|trait_| potion_capabilities(*potion, *trait_))
        .collect::<BTreeSet<_>>();
    required.intersection(&supplied).copied().collect()
}

fn matched_consumable_packages_v1(
    run_state: &RunState,
    acquisition: &ShopPolicyAcquisitionV1,
) -> Vec<StrategyPackageIdV2> {
    let ShopPolicyAcquisitionV1::Potion { traits, .. } = acquisition else {
        return Vec::new();
    };
    if traits.contains(&PotionAcquisitionTraitV1::StrengthGain)
        && strength_profile_v1(run_state).payoffs > 0
    {
        vec![StrategyPackageIdV2::StrengthScaling]
    } else {
        Vec::new()
    }
}

fn guaranteed_realizable_potion_run_resource_gain_v1(
    after_purchase: &RunState,
    acquisition: &ShopPolicyAcquisitionV1,
) -> (i32, i32) {
    let ShopPolicyAcquisitionV1::Potion { potion, traits, .. } = acquisition else {
        return (0, 0);
    };
    if !traits.contains(&PotionAcquisitionTraitV1::PermanentMaxHp)
        || !potion_can_use_out_of_combat(*potion, false)
        || !after_purchase
            .potions
            .iter()
            .flatten()
            .any(|owned| owned.id == *potion && owned.can_use)
    {
        return (0, 0);
    }
    let sacred_bark_multiplier = if after_purchase
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::SacredBark)
    {
        2
    } else {
        1
    };
    let max_hp_gain = get_potion_definition(*potion)
        .base_potency
        .saturating_mul(sacred_bark_multiplier);
    // Java max-HP increase heals by the same amount. Other on-use relic healing
    // is deliberately excluded, so this remains a guaranteed lower bound.
    (max_hp_gain, max_hp_gain)
}

fn conditional_combat_on_use_potion_hp_capacity_v1(
    after_purchase: &RunState,
    acquisition: &ShopPolicyAcquisitionV1,
) -> i32 {
    let ShopPolicyAcquisitionV1::Potion { potion, .. } = acquisition else {
        return 0;
    };
    let active_use_owned = after_purchase
        .potions
        .iter()
        .flatten()
        .any(|owned| owned.id == *potion && owned.can_use);
    let has_ornithopter = after_purchase
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::ToyOrnithopter);
    let healing_blocked = after_purchase
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::MarkOfTheBloom);
    if !active_use_owned || !has_ornithopter || healing_blocked {
        return 0;
    }

    let mut heal = 5;
    if after_purchase
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::MagicFlower)
    {
        heal = crate::content::relics::magic_flower::modify_heal(heal);
    }
    after_purchase
        .max_hp
        .saturating_sub(after_purchase.current_hp)
        .min(heal)
}

fn next_route_step_forces_combat_v1(parent: &RunControlSession) -> bool {
    let targets = route_window_targets(&parent.run_state);
    !targets.is_empty()
        && targets.iter().all(|target| {
            matches!(
                target.room_type,
                Some(
                    crate::state::map::node::RoomType::MonsterRoom
                        | crate::state::map::node::RoomType::MonsterRoomElite
                        | crate::state::map::node::RoomType::MonsterRoomBoss
                )
            )
        })
}

fn matched_relic_threats_v1(
    parent: &RunControlSession,
    threats: &StrategyThreatProfileV1,
    acquisition: &ShopPolicyAcquisitionV1,
) -> Vec<RelicThreatMatchV1> {
    let ShopPolicyAcquisitionV1::Relic { traits, .. } = acquisition else {
        return Vec::new();
    };
    if !traits.contains(&RelicAcquisitionTraitV1::ReactiveDamage) {
        return Vec::new();
    }
    let elite_exposure = elite_route_exposure_v1(parent);
    let mut matches = threats
        .sources
        .iter()
        .filter(|source| source.tag == StrategyThreatTagV1::MultiHit)
        .filter_map(|source| {
            let exposure = match source.source {
                StrategyThreatSourceV1::ActBoss => RelicThreatExposureV1::Boss,
                StrategyThreatSourceV1::ActElitePool => elite_exposure?,
                // The encounter records explain members of the same random
                // elite pool; the pool record is the honest route-level claim.
                StrategyThreatSourceV1::ActEliteEncounter
                | StrategyThreatSourceV1::ActHallwayPool => return None,
            };
            Some(RelicThreatMatchV1 {
                trait_: RelicAcquisitionTraitV1::ReactiveDamage,
                threat: source.tag,
                source: source.source,
                subject: source.subject.clone(),
                exposure,
            })
        })
        .collect::<Vec<_>>();
    matches.dedup();
    matches
}

impl RelicThreatExposureV1 {
    fn is_committed(self) -> bool {
        matches!(self, Self::Boss | Self::ForcedEliteRoute)
    }
}

fn elite_route_exposure_v1(parent: &RunControlSession) -> Option<RelicThreatExposureV1> {
    let targets = route_window_targets(&parent.run_state);
    if targets.is_empty() {
        return None;
    }
    let summaries = targets
        .iter()
        .map(|target| summarize_route_from_target(&parent.run_state, target.x, target.y, 2_000))
        .collect::<Vec<_>>();
    let elite_reachable = summaries.iter().any(|summary| summary.max_elites > 0);
    if !elite_reachable {
        return None;
    }
    let elite_forced = summaries.iter().all(|summary| {
        summary.path_count > 0 && !summary.path_budget_exhausted && summary.min_elites > 0
    });
    Some(if elite_forced {
        RelicThreatExposureV1::ForcedEliteRoute
    } else {
        RelicThreatExposureV1::OptionalEliteRoute
    })
}

fn potion_capabilities(
    potion: PotionId,
    trait_: PotionAcquisitionTraitV1,
) -> Vec<StrategyCapabilityKindV1> {
    use PotionAcquisitionTraitV1 as Trait;
    use StrategyCapabilityKindV1 as Capability;
    match trait_ {
        Trait::CombatDamage => vec![Capability::SingleTargetFrontload],
        Trait::AoeDamage => vec![Capability::MultiTargetControl],
        Trait::CombatBlock | Trait::WeakControl => vec![Capability::SustainedDefense],
        Trait::VulnerableSetup => vec![Capability::SingleTargetFrontload],
        Trait::StrengthGain => {
            let mut capabilities = vec![Capability::SingleTargetFrontload];
            if !potion_mechanics_profile_v1(potion).temporary_strength_burst {
                capabilities.push(Capability::LongFightScaling);
            }
            capabilities
        }
        Trait::EnergyBurst | Trait::CardAccess | Trait::ActionAmplifier => {
            vec![Capability::DrawEnergyConsistency]
        }
        Trait::ExhaustControl => vec![Capability::CardPlayEfficiency],
        Trait::CombatUpgrade => vec![Capability::CardPlayEfficiency],
        Trait::CardDiscovery => Vec::new(),
        Trait::DeathInsurance | Trait::EscapeTool => vec![Capability::SustainedDefense],
        Trait::DebuffControl => vec![Capability::ArtifactStripAccess],
        Trait::PermanentMaxHp => Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn shop_policy_band_v1(
    parent: &RunControlSession,
    acquisition: &ShopPolicyAcquisitionV1,
    followup: ShopPolicyFollowupV1,
    opens_completed_rewards: bool,
    max_hp_gain: i32,
    missing_hp_recovered: i32,
    guaranteed_realizable_max_hp_gain: i32,
    conditional_combat_recovery_support: bool,
    deck_size_delta: isize,
    closed_threat_gaps: &[ShopPolicyThreatGapKeyV1],
    reinforced_threat_capabilities: &[StrategyCapabilityKindV1],
    resolved_formation_needs: &[StrategyDeckFormationNeedV1],
    added_formation_strengths: &[StrategyPackageIdV2],
    matched_consumable_packages: &[StrategyPackageIdV2],
    matched_relic_threats: &[RelicThreatMatchV1],
    upgrade_scope_before: Option<CombatUpgradeScopeV1>,
    upgrade_scope_after: Option<CombatUpgradeScopeV1>,
    introduces_status_burden: bool,
    introduces_deck_shape_risk: bool,
    introduces_package_debt: bool,
    redundant_upgrade_access: bool,
    card_acquisition_policy: Option<AcquisitionPolicyDecision>,
    card_acquisition_opportunity_cost: Option<AcquisitionOpportunityCost>,
    spent_gold_before_action: bool,
    purchase_support: bool,
    action_supply: ActionSupplyDeltaV1,
    purge_target: Option<&DeckMutationCardSnapshotV1>,
) -> ShopPolicyBandV1 {
    if opens_completed_rewards {
        return ShopPolicyBandV1::Liability;
    }
    if matches!(acquisition, ShopPolicyAcquisitionV1::OpenRewards) {
        return ShopPolicyBandV1::ResolvePendingBoundary;
    }
    if missing_hp_recovered > 0 {
        return ShopPolicyBandV1::ImmediateSurvival;
    }
    if introduces_status_burden
        || introduces_deck_shape_risk
        || introduces_package_debt
        || redundant_upgrade_access
        || matches!(
            card_acquisition_policy,
            Some(AcquisitionPolicyDecision {
                verdict: AcquisitionPolicyVerdict::Reject,
                ..
            })
        )
    {
        return ShopPolicyBandV1::Liability;
    }
    if matches!(
        card_acquisition_policy.map(|policy| policy.verdict),
        Some(AcquisitionPolicyVerdict::SkipPreferred)
    ) {
        return ShopPolicyBandV1::SpeculativePurchase;
    }
    if card_acquisition_opportunity_cost == Some(AcquisitionOpportunityCost::SpendsPurgeReserve)
        && card_acquisition_policy.is_some_and(|policy| !policy.allows_acquisition())
        && !purchase_support
    {
        return ShopPolicyBandV1::SpeculativePurchase;
    }
    if spent_gold_before_action
        && matches!(
            acquisition,
            ShopPolicyAcquisitionV1::Card { .. } | ShopPolicyAcquisitionV1::Potion { .. }
        )
        && !card_acquisition_policy.is_some_and(AcquisitionPolicyDecision::allows_acquisition)
        && !purchase_support
    {
        return ShopPolicyBandV1::SpeculativePurchase;
    }
    if !closed_threat_gaps.is_empty() {
        return ShopPolicyBandV1::CloseThreatGap;
    }
    if first_copy_efficient_access(acquisition) {
        return ShopPolicyBandV1::AmplifyStrategicAccess;
    }
    if !reinforced_threat_capabilities.is_empty()
        && reinforcement_has_card_policy_support_v1(card_acquisition_policy)
    {
        return ShopPolicyBandV1::ImproveRequiredCapability;
    }
    let has_current_consumable_requirement = matches!(
        acquisition,
        ShopPolicyAcquisitionV1::Potion {
            requirements,
            requirement_support,
            ..
        } if !requirements.is_empty()
            && acquisition_requirement_is_current_v1(*requirement_support)
    );
    if !matched_consumable_packages.is_empty()
        || has_current_consumable_requirement
        || conditional_combat_recovery_support
    {
        return ShopPolicyBandV1::AcquireTacticalReserve;
    }
    if matched_relic_threats
        .iter()
        .any(|match_| match_.exposure.is_committed())
    {
        return ShopPolicyBandV1::EstablishStrategicAsset;
    }
    if matches!(acquisition, ShopPolicyAcquisitionV1::Purge { .. }) {
        return if deck_size_delta < 0 && purge_target.is_some_and(purge_is_deck_repair_v1) {
            ShopPolicyBandV1::DeckRepair
        } else {
            ShopPolicyBandV1::Liability
        };
    }
    if shop_acquisition_has_resolved_formation_need_v1(acquisition, resolved_formation_needs)
        || !added_formation_strengths.is_empty()
        || upgrade_scope_after > upgrade_scope_before
        || max_hp_gain > 0
        || guaranteed_realizable_max_hp_gain > 0
        || action_supply_has_durable_marginal_support_v1(
            acquisition,
            card_acquisition_policy,
            action_supply,
        )
        || card_policy_has_construction_acquisition_support_v1(acquisition, card_acquisition_policy)
        || card_gold_run_reward_conversion_supported_v1(parent, acquisition)
        || non_card_strategic_acquisition_supported(parent, acquisition, followup)
    {
        return ShopPolicyBandV1::EstablishStrategicAsset;
    }
    if matches!(acquisition, ShopPolicyAcquisitionV1::Leave) {
        return ShopPolicyBandV1::PreserveResources;
    }
    if matches!(
        card_acquisition_policy.map(|policy| policy.verdict),
        Some(AcquisitionPolicyVerdict::Reject)
    ) {
        return ShopPolicyBandV1::Liability;
    }
    ShopPolicyBandV1::SpeculativePurchase
}

fn reinforcement_has_card_policy_support_v1(
    card_acquisition_policy: Option<AcquisitionPolicyDecision>,
) -> bool {
    card_acquisition_policy.is_none_or(AcquisitionPolicyDecision::allows_acquisition)
}

fn card_policy_has_construction_acquisition_support_v1(
    acquisition: &ShopPolicyAcquisitionV1,
    policy: Option<AcquisitionPolicyDecision>,
) -> bool {
    if !policy.is_some_and(AcquisitionPolicyDecision::allows_acquisition) {
        return false;
    }
    let ShopPolicyAcquisitionV1::Card { construction, .. } = acquisition else {
        return false;
    };
    construction.has_shop_acquisition_role_v1()
}

fn action_supply_has_durable_marginal_support_v1(
    acquisition: &ShopPolicyAcquisitionV1,
    card_acquisition_policy: Option<AcquisitionPolicyDecision>,
    action_supply: ActionSupplyDeltaV1,
) -> bool {
    if !action_supply.has_positive_supply() {
        return false;
    }
    match acquisition {
        ShopPolicyAcquisitionV1::Card { copies_before, .. } if *copies_before > 0 => {
            card_acquisition_policy.is_some_and(AcquisitionPolicyDecision::allows_acquisition)
        }
        _ => true,
    }
}

fn purge_is_deck_repair_v1(target: &DeckMutationCardSnapshotV1) -> bool {
    target.removal_benefit.tier != DeckMutationRemovalBenefitTierV1::None
        || matches!(
            target.target_loss.tier,
            DeckMutationTargetLossTierV1::LowValue
                | DeckMutationTargetLossTierV1::RedundantFunctional
        )
}

fn shop_acquisition_has_resolved_formation_need_v1(
    acquisition: &ShopPolicyAcquisitionV1,
    resolved_formation_needs: &[StrategyDeckFormationNeedV1],
) -> bool {
    !resolved_formation_needs.is_empty()
        && matches!(
            acquisition,
            ShopPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::FormationNeedProgress)
        )
}

fn first_copy_efficient_access(acquisition: &ShopPolicyAcquisitionV1) -> bool {
    matches!(
        acquisition,
        ShopPolicyAcquisitionV1::Card {
            copies_before: 0,
            access: Some(CardAccessEvidenceV1 {
                leverage: CardAccessLeverageV1::EfficientBurst,
                ..
            }),
            ..
        }
    )
}

fn non_card_strategic_acquisition_supported(
    parent: &RunControlSession,
    acquisition: &ShopPolicyAcquisitionV1,
    followup: ShopPolicyFollowupV1,
) -> bool {
    match acquisition {
        ShopPolicyAcquisitionV1::Card { .. } => false,
        ShopPolicyAcquisitionV1::Relic {
            traits,
            requirement_support,
            ..
        } => {
            acquisition_requirement_is_current_v1(*requirement_support)
                && traits.iter().any(|trait_| match trait_ {
                    RelicAcquisitionTraitV1::EliteFightLeverage => {
                        parent.run_state.peek_next_elite().is_some()
                    }
                    RelicAcquisitionTraitV1::PotionEffectMultiplier => {
                        parent.run_state.potions.iter().any(Option::is_some)
                    }
                    RelicAcquisitionTraitV1::ReactiveDamage => false,
                    _ => true,
                })
                || followup == ShopPolicyFollowupV1::Reward
        }
        ShopPolicyAcquisitionV1::Potion {
            traits,
            requirements,
            requirement_support,
            ..
        } => {
            traits.contains(&PotionAcquisitionTraitV1::PermanentMaxHp)
                || (!requirements.is_empty()
                    && acquisition_requirement_is_current_v1(*requirement_support))
        }
        ShopPolicyAcquisitionV1::Purge { .. }
        | ShopPolicyAcquisitionV1::OpenRewards
        | ShopPolicyAcquisitionV1::Leave => false,
    }
}

fn card_gold_run_reward_conversion_supported_v1(
    parent: &RunControlSession,
    acquisition: &ShopPolicyAcquisitionV1,
) -> bool {
    let ShopPolicyAcquisitionV1::Card {
        copies_before,
        construction,
        ..
    } = acquisition
    else {
        return false;
    };
    *copies_before == 0
        && construction
            .run_rewards
            .contains(&RunRewardKind::GoldOnFatal)
        && gold_run_reward_has_conversion_window_v1(parent)
}

fn gold_run_reward_has_conversion_window_v1(parent: &RunControlSession) -> bool {
    // Gold survives the first two acts, so a later act can still convert it.
    // In Act 3 and beyond, require a visible non-Boss payoff combat followed
    // by another shop; gold earned on the Boss kill cannot improve this run.
    if parent.run_state.act_num < 3 {
        return true;
    }

    route_window_targets(&parent.run_state)
        .into_iter()
        .any(|target| {
            let family = build_route_path_family_from_target(
                &parent.run_state,
                target.x,
                target.y,
                RouteWindowFactsConfig {
                    horizon_nodes: 15,
                    path_budget: 2_000,
                },
            );
            family.paths.iter().any(|path| {
                let mut payoff_combat_seen = false;
                path.nodes.iter().any(|node| match node.room_type {
                    Some(
                        crate::state::map::node::RoomType::MonsterRoom
                        | crate::state::map::node::RoomType::MonsterRoomElite,
                    ) => {
                        payoff_combat_seen = true;
                        false
                    }
                    Some(crate::state::map::node::RoomType::ShopRoom) => payoff_combat_seen,
                    _ => false,
                })
            })
        })
}

fn acquisition_requirement_is_current_v1(support: AcquisitionRequirementSupportV1) -> bool {
    matches!(
        support,
        AcquisitionRequirementSupportV1::Current
            | AcquisitionRequirementSupportV1::PremiumDuplicateTarget { .. }
    )
}

fn acquisition_requirements_v1(
    acquisition: &ShopPolicyAcquisitionV1,
) -> &[AcquisitionRequirementV1] {
    match acquisition {
        ShopPolicyAcquisitionV1::Relic { requirements, .. }
        | ShopPolicyAcquisitionV1::Potion { requirements, .. } => requirements,
        ShopPolicyAcquisitionV1::Card { .. }
        | ShopPolicyAcquisitionV1::Purge { .. }
        | ShopPolicyAcquisitionV1::OpenRewards
        | ShopPolicyAcquisitionV1::Leave => &[],
    }
}

fn acquisition_requirement_support_for_audit_v1(
    acquisition: &ShopPolicyAcquisitionV1,
) -> Option<AcquisitionRequirementSupportV1> {
    match acquisition {
        ShopPolicyAcquisitionV1::Relic {
            requirement_support,
            ..
        }
        | ShopPolicyAcquisitionV1::Potion {
            requirement_support,
            ..
        } => Some(*requirement_support),
        ShopPolicyAcquisitionV1::Card { .. }
        | ShopPolicyAcquisitionV1::Purge { .. }
        | ShopPolicyAcquisitionV1::OpenRewards
        | ShopPolicyAcquisitionV1::Leave => None,
    }
}

fn followup_v1(engine_state: &EngineState) -> ShopPolicyFollowupV1 {
    match engine_state {
        EngineState::Shop(_) => ShopPolicyFollowupV1::Shop,
        EngineState::RewardScreen(_) | EngineState::RewardOverlay { .. } => {
            ShopPolicyFollowupV1::Reward
        }
        EngineState::RunPendingChoice(_) => ShopPolicyFollowupV1::Selection,
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => ShopPolicyFollowupV1::Map,
        _ => ShopPolicyFollowupV1::Other,
    }
}

fn compare_shop_evidence(
    left: &ShopPolicyActionEvidenceV1,
    right: &ShopPolicyActionEvidenceV1,
) -> Ordering {
    shop_band_priority(left.band)
        .cmp(&shop_band_priority(right.band))
        .then_with(|| compare_blocked_card_acquisition_policy(left, right))
        .then_with(|| {
            right
                .closed_threat_gaps
                .len()
                .cmp(&left.closed_threat_gaps.len())
        })
        .then_with(|| {
            right
                .capability_improvements
                .len()
                .cmp(&left.capability_improvements.len())
        })
        .then_with(|| {
            right
                .matched_consumable_packages
                .len()
                .cmp(&left.matched_consumable_packages.len())
        })
        .then_with(|| right.missing_hp_recovered.cmp(&left.missing_hp_recovered))
        .then_with(|| compare_durable_asset_axes(left, right))
        .then_with(|| {
            right
                .acquisition
                .is_live_upgraded_strength_amplifier()
                .cmp(&left.acquisition.is_live_upgraded_strength_amplifier())
        })
        .then_with(|| {
            right
                .resolved_formation_needs
                .len()
                .cmp(&left.resolved_formation_needs.len())
        })
        .then_with(|| {
            right
                .added_formation_strengths
                .len()
                .cmp(&left.added_formation_strengths.len())
        })
        .then_with(|| {
            compare_purge_targets_v1(left.purge_target.as_ref(), right.purge_target.as_ref())
        })
        .then_with(|| left.gold_spent.cmp(&right.gold_spent))
        .then_with(|| left.surface_index.cmp(&right.surface_index))
}

impl ShopPolicyAcquisitionV1 {
    fn is_live_upgraded_strength_amplifier(&self) -> bool {
        matches!(
            self,
            Self::Card {
                card: CardId::LimitBreak,
                upgrades,
                component_signals,
                ..
            } if *upgrades > 0
                && component_signals
                    .positive_signals
                    .contains(&CardComponentSignalKindV1::StrengthPayoffSupported)
        )
    }
}

fn compare_purge_targets_v1(
    left: Option<&DeckMutationCardSnapshotV1>,
    right: Option<&DeckMutationCardSnapshotV1>,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => right
            .removal_benefit
            .tier
            .cmp(&left.removal_benefit.tier)
            .then_with(|| {
                left.removal_benefit
                    .compensating_signals
                    .len()
                    .cmp(&right.removal_benefit.compensating_signals.len())
            })
            .then_with(|| left.target_loss.tier.cmp(&right.target_loss.tier)),
        _ => Ordering::Equal,
    }
}

fn compare_durable_asset_axes(
    left: &ShopPolicyActionEvidenceV1,
    right: &ShopPolicyActionEvidenceV1,
) -> Ordering {
    let left_supply = left.action_supply.has_positive_supply();
    let right_supply = right.action_supply.has_positive_supply();
    let left_max_hp = left.max_hp_gain.max(left.guaranteed_realizable_max_hp_gain);
    let right_max_hp = right
        .max_hp_gain
        .max(right.guaranteed_realizable_max_hp_gain);
    match (left_supply == right_supply, left_max_hp == right_max_hp) {
        (true, false) => right_max_hp.cmp(&left_max_hp),
        (false, true) => right_supply.cmp(&left_supply),
        // Action supply and maximum HP are distinct durable assets. If each
        // candidate improves a different axis, neither globally dominates.
        _ => Ordering::Equal,
    }
}

fn compare_blocked_card_acquisition_policy(
    left: &ShopPolicyActionEvidenceV1,
    right: &ShopPolicyActionEvidenceV1,
) -> Ordering {
    let left_policy = left.card_acquisition_policy;
    let right_policy = right.card_acquisition_policy;
    match (
        left_policy.map(|policy| policy.verdict),
        right_policy.map(|policy| policy.verdict),
    ) {
        (
            Some(AcquisitionPolicyVerdict::SkipPreferred),
            Some(AcquisitionPolicyVerdict::Unverified | AcquisitionPolicyVerdict::Reject),
        ) => Ordering::Less,
        (
            Some(AcquisitionPolicyVerdict::Unverified | AcquisitionPolicyVerdict::Reject),
            Some(AcquisitionPolicyVerdict::SkipPreferred),
        ) => Ordering::Greater,
        (
            Some(AcquisitionPolicyVerdict::SkipPreferred),
            Some(AcquisitionPolicyVerdict::SkipPreferred),
        ) => Ordering::Equal,
        _ => Ordering::Equal,
    }
}

const fn shop_band_priority(band: ShopPolicyBandV1) -> u8 {
    match band {
        ShopPolicyBandV1::ResolvePendingBoundary => 0,
        ShopPolicyBandV1::ImmediateSurvival => 1,
        ShopPolicyBandV1::CloseThreatGap => 2,
        ShopPolicyBandV1::AmplifyStrategicAccess => 3,
        ShopPolicyBandV1::ImproveRequiredCapability => 4,
        // Both are durable deck improvements. Their exact evidence and
        // opportunity cost must compare before the owner commits all gold to
        // one category merely because it was represented by a different verb.
        ShopPolicyBandV1::AcquireTacticalReserve
        | ShopPolicyBandV1::DeckRepair
        | ShopPolicyBandV1::EstablishStrategicAsset => 5,
        ShopPolicyBandV1::PreserveResources => 6,
        ShopPolicyBandV1::SpeculativePurchase => 7,
        ShopPolicyBandV1::Liability => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::strategy::acquisition::assess_card_construction_v1;
    use crate::content::monsters::factory::EncounterId;
    use crate::content::potions::{Potion, PotionId};
    use crate::content::relics::RelicState;
    use crate::eval::run_control::{build_decision_surface, RunControlConfig, ShopVisitContextV1};
    use crate::runtime::combat::CombatCard;
    use crate::state::map::node::{MapEdge, MapRoomNode, RoomType};
    use crate::state::map::state::MapState;
    use crate::state::rewards::{RewardItem, RewardState};
    use crate::state::shop::{ShopCard, ShopPotion, ShopRelic, ShopState};

    fn policy_candidates<'a>(
        surface: &'a super::super::DecisionSurface,
    ) -> Vec<RunPolicyCandidateV1<'a>> {
        surface
            .view
            .candidates
            .iter()
            .filter_map(|candidate| {
                Some(RunPolicyCandidateV1 {
                    candidate_id: &candidate.id,
                    label: &candidate.label,
                    action: candidate.action.executable_action_ref()?,
                })
            })
            .collect()
    }

    fn candidate_band(decision: &ExactShopPolicyDecisionV1, card: CardId) -> ShopPolicyBandV1 {
        decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: candidate_card,
                        ..
                    } if candidate_card == card
                )
            })
            .expect("card evidence")
            .band
    }

    fn candidate_position(decision: &ExactShopPolicyDecisionV1, card: CardId) -> usize {
        decision
            .evidence
            .iter()
            .position(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: candidate_card,
                        ..
                    } if candidate_card == card
                )
            })
            .expect("card position")
    }

    fn relic_candidate(
        decision: &ExactShopPolicyDecisionV1,
        relic: RelicId,
    ) -> &ShopPolicyActionEvidenceV1 {
        decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyRelic {
                        relic: candidate_relic,
                        ..
                    } if candidate_relic == relic
                )
            })
            .expect("relic evidence")
    }

    fn dollys_mirror_decision(
        cards: &[(CardId, u8)],
    ) -> (RunControlSession, ExactShopPolicyDecisionV1) {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 163;
        session.run_state.master_deck = cards
            .iter()
            .enumerate()
            .map(|(index, (card, upgrades))| {
                let mut owned = CombatCard::for_scenario(*card, index as u32);
                owned.upgrades = *upgrades;
                owned
            })
            .collect();
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.relics.push(ShopRelic {
            relic_id: RelicId::DollysMirror,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_shop_policy_decision_v1(&session, &legal).expect("Dolly's Mirror shop policy");
        (session, decision)
    }

    fn potion_candidate(
        decision: &ExactShopPolicyDecisionV1,
        potion: PotionId,
    ) -> &ShopPolicyActionEvidenceV1 {
        decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyPotion {
                        potion: candidate_potion,
                        ..
                    } if candidate_potion == potion
                )
            })
            .expect("potion evidence")
    }

    fn potion_position(decision: &ExactShopPolicyDecisionV1, potion: PotionId) -> usize {
        decision
            .evidence
            .iter()
            .position(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyPotion {
                        potion: candidate_potion,
                        ..
                    } if candidate_potion == potion
                )
            })
            .expect("potion position")
    }

    #[test]
    fn shop_does_not_buy_an_unfunded_status_payoff_as_a_generic_asset() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 1;
        session.run_state.floor_num = 4;
        session.run_state.gold = 131;
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Strike, 2),
            CombatCard::for_scenario(CardId::Strike, 3),
            CombatCard::for_scenario(CardId::Defend, 4),
            CombatCard::for_scenario(CardId::Defend, 5),
            CombatCard::for_scenario(CardId::Defend, 6),
            CombatCard::for_scenario(CardId::Defend, 7),
            CombatCard::for_scenario(CardId::Bash, 8),
            CombatCard::for_scenario(CardId::Disarm, 9),
            CombatCard::for_scenario(CardId::SwordBoomerang, 10),
        ];
        let mut shop = ShopState::new();
        shop.purge_available = true;
        shop.purge_cost = 75;
        shop.cards.push(ShopCard {
            card_id: CardId::FireBreathing,
            upgrades: 0,
            price: 34,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal).expect("shop policy");
        let payoff = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::FireBreathing,
                        ..
                    }
                )
            })
            .expect("Fire Breathing evidence");

        assert!(payoff.introduces_package_debt);
        assert_eq!(payoff.band, ShopPolicyBandV1::Liability);
        assert!(!matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopBuyCard {
                card: CardId::FireBreathing,
                ..
            }
        ));

        session
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::RecklessCharge, 11));
        let supported_surface = build_decision_surface(&session);
        let supported_legal = policy_candidates(&supported_surface);
        let supported = exact_shop_policy_decision_v1(&session, &supported_legal)
            .expect("supported shop policy");
        let supported_payoff = supported
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::FireBreathing,
                        ..
                    }
                )
            })
            .expect("supported Fire Breathing evidence");
        assert!(!supported_payoff.introduces_package_debt);
        assert_ne!(supported_payoff.band, ShopPolicyBandV1::Liability);
    }

    #[test]
    fn act_one_dense_frontload_competes_with_purge_without_admitting_weak_or_unfunded_cards() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 1;
        session.run_state.floor_num = 4;
        session.run_state.boss_key = Some(EncounterId::TheGuardian);
        session.run_state.current_hp = 80;
        session.run_state.max_hp = 80;
        session.run_state.gold = 131;
        session.run_state.master_deck = [
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
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();
        let mut shop = ShopState::new();
        shop.purge_available = true;
        shop.purge_cost = 75;
        shop.cards.extend([
            ShopCard {
                card_id: CardId::Carnage,
                upgrades: 0,
                price: 76,
                can_buy: true,
                blocked_reason: None,
            },
            ShopCard {
                card_id: CardId::FireBreathing,
                upgrades: 0,
                price: 34,
                can_buy: true,
                blocked_reason: None,
            },
            ShopCard {
                card_id: CardId::IronWave,
                upgrades: 0,
                price: 34,
                can_buy: true,
                blocked_reason: None,
            },
            ShopCard {
                card_id: CardId::SwordBoomerang,
                upgrades: 0,
                price: 51,
                can_buy: true,
                blocked_reason: None,
            },
        ]);
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal).expect("shop policy");
        let carnage = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::Carnage,
                        ..
                    }
                )
            })
            .expect("Carnage evidence");
        assert!(matches!(
            &carnage.acquisition,
            ShopPolicyAcquisitionV1::Card { construction, .. }
                if construction.construction_roles.contains(
                    &crate::ai::strategy::acquisition::AcquisitionConstructionRole::ActOneDenseFrontloadTransition
                )
        ));
        assert_eq!(
            carnage.card_acquisition_opportunity_cost,
            Some(AcquisitionOpportunityCost::SpendsPurgeReserve)
        );
        assert_eq!(carnage.band, ShopPolicyBandV1::CloseThreatGap);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopBuyCard {
                card: CardId::Carnage,
                ..
            }
        ));
        assert_eq!(
            candidate_band(&decision, CardId::FireBreathing),
            ShopPolicyBandV1::Liability
        );
        assert_eq!(
            candidate_band(&decision, CardId::IronWave),
            ShopPolicyBandV1::SpeculativePurchase
        );
        assert_eq!(
            candidate_band(&decision, CardId::SwordBoomerang),
            ShopPolicyBandV1::Liability
        );
    }

    #[test]
    fn shop_does_not_buy_starter_scale_output_for_a_raw_formation_threshold() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 1;
        session.run_state.floor_num = 3;
        session.run_state.gold = 131;
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Strike, 2),
            CombatCard::for_scenario(CardId::Strike, 3),
            CombatCard::for_scenario(CardId::Strike, 4),
            CombatCard::for_scenario(CardId::Strike, 5),
            CombatCard::for_scenario(CardId::Defend, 6),
            CombatCard::for_scenario(CardId::Defend, 7),
            CombatCard::for_scenario(CardId::Defend, 8),
            CombatCard::for_scenario(CardId::Defend, 9),
            CombatCard::for_scenario(CardId::Bash, 10),
            CombatCard::for_scenario(CardId::Clothesline, 11),
        ];
        let mut shop = ShopState::new();
        shop.purge_available = true;
        shop.purge_cost = 75;
        shop.cards.push(ShopCard {
            card_id: CardId::IronWave,
            upgrades: 1,
            price: 34,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal).expect("shop policy");
        let iron_wave = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::IronWave,
                        ..
                    }
                )
            })
            .expect("Iron Wave evidence");

        assert!(iron_wave
            .resolved_formation_needs
            .contains(&StrategyDeckFormationNeedV1::Block));
        assert!(matches!(
            &iron_wave.acquisition,
            ShopPolicyAcquisitionV1::Card {
                component_signals,
                ..
            } if !component_signals
                .positive_signals
                .contains(&CardComponentSignalKindV1::FormationNeedProgress)
                && component_signals
                    .note_signals
                    .contains(&CardComponentSignalKindV1::FormationNeedRoleWithoutTacticalDensity)
        ));
        assert_eq!(iron_wave.band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(!matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopBuyCard {
                card: CardId::IronWave,
                ..
            }
        ));
    }

    #[test]
    fn purge_ranking_keeps_forced_opening_obstruction_ahead_of_starters() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 100;
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
            CombatCard::for_scenario(CardId::Writhe, 3),
        ];
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::DuVuDoll));
        let mut shop = ShopState::new();
        shop.purge_available = true;
        shop.purge_cost = 50;
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal).expect("shop policy");
        let purge_position = |card| {
            decision
                .evidence
                .iter()
                .position(|candidate| {
                    matches!(
                        candidate.candidate_key,
                        DecisionCandidateKey::ShopPurgeCard {
                            card: candidate_card,
                            ..
                        } if candidate_card == card
                    )
                })
                .expect("purge candidate")
        };
        let writhe = &decision.evidence[purge_position(CardId::Writhe)];

        assert!(purge_position(CardId::Writhe) < purge_position(CardId::Strike));
        assert!(purge_position(CardId::Writhe) < purge_position(CardId::Defend));
        assert_eq!(writhe.band, ShopPolicyBandV1::DeckRepair);
        assert_eq!(
            writhe
                .purge_target
                .as_ref()
                .map(|target| target.removal_benefit.tier),
            Some(DeckMutationRemovalBenefitTierV1::ForcedOpeningObstruction)
        );
        assert!(writhe
            .purge_target
            .as_ref()
            .expect("Writhe target")
            .removal_benefit
            .compensating_signals
            .iter()
            .any(|signal| signal == "du_vu_doll_strength"));
    }

    fn orange_pellets_decision(dark_embrace_upgrades: u8) -> ExactShopPolicyDecisionV1 {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 2;
        session.run_state.floor_num = 18;
        session.run_state.current_hp = 71;
        session.run_state.max_hp = 85;
        session.run_state.gold = 175;
        session.run_state.relics = vec![
            RelicState::new(RelicId::BurningBlood),
            RelicState::new(RelicId::PenNib),
            RelicState::new(RelicId::RunicCube),
        ];
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
            (CardId::PowerThrough, 1),
            (CardId::HeavyBlade, 0),
            (CardId::SecondWind, 1),
            (CardId::DarkEmbrace, dark_embrace_upgrades),
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
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.relics.push(ShopRelic {
            relic_id: RelicId::OrangePellets,
            price: 145,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        exact_shop_policy_decision_v1(&session, &legal).expect("Orange Pellets policy")
    }

    fn cauldron_decision_with_potions(potions: Vec<Option<Potion>>) -> ExactShopPolicyDecisionV1 {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 300;
        session.run_state.potions = potions;
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Cauldron,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        exact_shop_policy_decision_v1(&session, &legal).expect("Cauldron policy")
    }

    fn bronze_scales_decision(
        act: u8,
        boss: EncounterId,
        next_rooms: &[RoomType],
    ) -> ExactShopPolicyDecisionV1 {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = act;
        session.run_state.floor_num = if act == 2 { 20 } else { 8 };
        session.run_state.boss_key = Some(boss);
        session.run_state.gold = 206;
        let mut current = MapRoomNode::new(0, 0);
        current.class = Some(RoomType::ShopRoom);
        let next_row = next_rooms
            .iter()
            .enumerate()
            .map(|(x, room_type)| {
                current.edges.insert(MapEdge::new(0, 0, x as i32, 1));
                let mut next = MapRoomNode::new(x as i32, 1);
                next.class = Some(*room_type);
                next
            })
            .collect::<Vec<_>>();
        let mut map = MapState::new(vec![vec![current], next_row]);
        map.current_x = 0;
        map.current_y = 0;
        session.run_state.map = map;
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.relics.push(ShopRelic {
            relic_id: RelicId::BronzeScales,
            price: 153,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        exact_shop_policy_decision_v1(&session, &legal).expect("Bronze Scales policy")
    }

    #[test]
    fn bronze_scales_becomes_a_durable_asset_only_when_public_threats_include_multi_hit() {
        let decision =
            bronze_scales_decision(2, EncounterId::Automaton, &[RoomType::MonsterRoomElite]);
        let scales = relic_candidate(&decision, RelicId::BronzeScales);

        assert!(matches!(
            scales.acquisition,
            ShopPolicyAcquisitionV1::Relic { ref traits, .. }
                if traits.contains(&RelicAcquisitionTraitV1::ReactiveDamage)
        ));
        assert_eq!(
            scales.matched_relic_threats,
            vec![RelicThreatMatchV1 {
                trait_: RelicAcquisitionTraitV1::ReactiveDamage,
                threat: StrategyThreatTagV1::MultiHit,
                source: StrategyThreatSourceV1::ActElitePool,
                subject: "Act2ElitePool".to_string(),
                exposure: RelicThreatExposureV1::ForcedEliteRoute,
            }]
        );
        assert!(scales.durable_asset_support);
        assert_eq!(scales.band, ShopPolicyBandV1::EstablishStrategicAsset);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopBuyRelic {
                relic: RelicId::BronzeScales,
                ..
            }
        ));
    }

    #[test]
    fn bronze_scales_optional_elite_route_is_visible_but_does_not_outvote_gold_preservation() {
        let decision = bronze_scales_decision(
            2,
            EncounterId::Automaton,
            &[RoomType::MonsterRoomElite, RoomType::RestRoom],
        );
        let scales = relic_candidate(&decision, RelicId::BronzeScales);

        assert_eq!(
            scales.matched_relic_threats[0].exposure,
            RelicThreatExposureV1::OptionalEliteRoute
        );
        assert!(!scales.durable_asset_support);
        assert_eq!(scales.band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopLeave
        ));
    }

    #[test]
    fn bronze_scales_trait_does_not_outvote_gold_preservation_without_a_reachable_multi_hit_route()
    {
        let decision = bronze_scales_decision(2, EncounterId::Automaton, &[RoomType::RestRoom]);
        let scales = relic_candidate(&decision, RelicId::BronzeScales);

        assert!(scales.matched_relic_threats.is_empty());
        assert!(!scales.durable_asset_support);
        assert_eq!(scales.band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopLeave
        ));
    }

    #[test]
    fn orange_pellets_upgrade_path_is_visible_but_does_not_spend_gold_as_current_support() {
        let decision = orange_pellets_decision(0);
        let pellets = relic_candidate(&decision, RelicId::OrangePellets);

        assert!(matches!(
            pellets.acquisition,
            ShopPolicyAcquisitionV1::Relic {
                requirement_support: AcquisitionRequirementSupportV1::OwnedUpgradePath {
                    card: CardId::DarkEmbrace,
                    upgrades_before: 0,
                    energy_cost_before: 2,
                    energy_cost_after: 1,
                    energy_gain_before: 0,
                    energy_gain_after: 0,
                    ..
                },
                ..
            }
        ));
        assert_eq!(pellets.band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopLeave
        ));
    }

    #[test]
    fn orange_pellets_current_three_card_line_is_a_supported_asset() {
        let decision = orange_pellets_decision(1);
        let pellets = relic_candidate(&decision, RelicId::OrangePellets);

        assert!(matches!(
            pellets.acquisition,
            ShopPolicyAcquisitionV1::Relic {
                requirement_support: AcquisitionRequirementSupportV1::Current,
                ..
            }
        ));
        assert_eq!(pellets.band, ShopPolicyBandV1::EstablishStrategicAsset);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopBuyRelic {
                relic: RelicId::OrangePellets,
                ..
            }
        ));
    }

    #[test]
    fn orange_pellets_without_an_owned_power_reports_unavailable() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];

        assert_eq!(
            attack_skill_power_activation_support_v1(&run_state),
            AcquisitionRequirementSupportV1::Unavailable
        );
    }

    #[test]
    fn orange_pellets_accepts_an_unconditional_energy_gain_sequence() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::SeeingRed, 2),
            CombatCard::for_scenario(CardId::DemonForm, 3),
        ];

        assert_eq!(
            attack_skill_power_activation_support_v1(&run_state),
            AcquisitionRequirementSupportV1::Current
        );
    }

    #[test]
    fn full_inventory_pending_potions_do_not_reopen_before_leaving_shop() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 16;
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::AttackPotion, 1)),
            Some(Potion::new(PotionId::SpeedPotion, 2)),
            Some(Potion::new(PotionId::EssenceOfSteel, 3)),
        ];
        let mut pending = RewardState::new();
        pending.items.push(RewardItem::Potion {
            potion_id: PotionId::LiquidBronze,
        });
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.pending_reward_overlay = Some(pending);
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_shop_policy_decision_v1(&session, &legal).expect("pending potion shop policy");

        decision
            .prior
            .validate_for(&legal)
            .expect("mixed shop surface keeps positive support for auxiliary potion actions");
        assert!(decision
            .evidence
            .iter()
            .all(|candidate| is_shop_key(&candidate.candidate_key)));

        assert!(
            matches!(
                decision.evidence[0].candidate_key,
                DecisionCandidateKey::ShopLeave
            ),
            "evidence={:#?}",
            decision.evidence
        );
        assert_eq!(
            decision
                .evidence
                .iter()
                .find(|candidate| {
                    matches!(
                        candidate.candidate_key,
                        DecisionCandidateKey::ShopOpenRewards
                    )
                })
                .expect("open rewards evidence")
                .band,
            ShopPolicyBandV1::Liability
        );
    }

    #[test]
    fn declined_pending_card_reward_does_not_reopen_before_leaving_shop() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 0;
        let mut pending = RewardState::new();
        pending.items.push(RewardItem::Card {
            cards: vec![RewardCard::new(CardId::Injury, 0)],
        });
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.pending_reward_overlay = Some(pending);
        session.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal).unwrap();
        assert_eq!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopLeave
        );
        assert!(decision
            .prior
            .entries
            .iter()
            .all(|entry| entry.probability > 0.0));
    }

    #[test]
    fn pending_potion_with_empty_slot_reopens_before_leaving_shop() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 16;
        let mut pending = RewardState::new();
        pending.items.push(RewardItem::Potion {
            potion_id: PotionId::LiquidBronze,
        });
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.pending_reward_overlay = Some(pending);
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_shop_policy_decision_v1(&session, &legal).expect("pending potion shop policy");

        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopOpenRewards
        ));
        assert_eq!(
            decision.evidence[0].band,
            ShopPolicyBandV1::ResolvePendingBoundary
        );
    }

    #[test]
    fn full_inventory_cauldron_is_a_liability_without_a_replacement_policy() {
        let decision = cauldron_decision_with_potions(vec![
            Some(Potion::new(PotionId::AttackPotion, 1)),
            Some(Potion::new(PotionId::SpeedPotion, 2)),
            Some(Potion::new(PotionId::EssenceOfSteel, 3)),
        ]);
        let cauldron = relic_candidate(&decision, RelicId::Cauldron);

        assert_eq!(cauldron.followup, ShopPolicyFollowupV1::Reward);
        assert_eq!(cauldron.band, ShopPolicyBandV1::Liability);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopLeave
        ));
    }

    #[test]
    fn cauldron_remains_strategic_when_a_potion_slot_is_available() {
        let decision = cauldron_decision_with_potions(vec![
            Some(Potion::new(PotionId::AttackPotion, 1)),
            Some(Potion::new(PotionId::SpeedPotion, 2)),
            None,
        ]);
        let cauldron = relic_candidate(&decision, RelicId::Cauldron);

        assert_eq!(cauldron.band, ShopPolicyBandV1::EstablishStrategicAsset);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopBuyRelic {
                relic: RelicId::Cauldron,
                ..
            }
        ));
    }

    #[test]
    fn efficient_first_copy_access_precedes_marginal_frontload_improvement() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 2;
        session.run_state.floor_num = 20;
        session.run_state.boss_key = Some(EncounterId::Automaton);
        session.run_state.current_hp = 76;
        session.run_state.max_hp = 80;
        session.run_state.gold = 86;
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
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.cards.extend([
            ShopCard {
                card_id: CardId::RecklessCharge,
                upgrades: 0,
                price: 33,
                can_buy: true,
                blocked_reason: None,
            },
            ShopCard {
                card_id: CardId::BattleTrance,
                upgrades: 0,
                price: 78,
                can_buy: true,
                blocked_reason: None,
            },
        ]);
        session.engine_state = EngineState::Shop(shop.clone());

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal).expect("access shop policy");
        assert_eq!(
            candidate_band(&decision, CardId::BattleTrance),
            ShopPolicyBandV1::AmplifyStrategicAccess
        );
        assert!(
            candidate_position(&decision, CardId::BattleTrance)
                < candidate_position(&decision, CardId::RecklessCharge),
            "evidence={:#?}",
            decision.evidence
        );

        session
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::BattleTrance, 30_000));
        session.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let duplicate =
            exact_shop_policy_decision_v1(&session, &legal).expect("duplicate access policy");
        assert_ne!(
            candidate_band(&duplicate, CardId::BattleTrance),
            ShopPolicyBandV1::AmplifyStrategicAccess
        );
    }

    #[test]
    fn purge_opportunity_stays_visible_without_erasing_supported_shop_assets() {
        // Keep the complete A1F4 deck: starter density drives both the typed
        // strategic gaps and the still-available low-loss purge alternative.
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 1;
        session.run_state.floor_num = 4;
        session.run_state.boss_key = Some(EncounterId::SlimeBoss);
        session.run_state.current_hp = 85;
        session.run_state.max_hp = 85;
        session.run_state.gold = 135;
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
            CardId::PowerThrough,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();
        let mut shop = ShopState::new();
        shop.purge_cost = 75;
        shop.cards.extend([
            ShopCard {
                card_id: CardId::Uppercut,
                upgrades: 0,
                price: 69,
                can_buy: true,
                blocked_reason: None,
            },
            ShopCard {
                card_id: CardId::SecondWind,
                upgrades: 0,
                price: 35,
                can_buy: true,
                blocked_reason: None,
            },
        ]);
        shop.potions.push(ShopPotion {
            potion_id: PotionId::AttackPotion,
            price: 52,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal).expect("A1F4 shop policy");
        let uppercut = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::Uppercut,
                        ..
                    }
                )
            })
            .expect("Uppercut evidence");

        assert_eq!(
            uppercut.card_acquisition_policy,
            Some(AcquisitionPolicyDecision {
                verdict: AcquisitionPolicyVerdict::ContextTake,
                reason: AcquisitionPolicyReason::ConstructionRoleAccepted,
            })
        );
        assert_eq!(
            uppercut.card_acquisition_opportunity_cost,
            Some(AcquisitionOpportunityCost::SpendsPurgeReserve)
        );
        assert_ne!(uppercut.band, ShopPolicyBandV1::SpeculativePurchase);
        let second_wind = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::SecondWind,
                        ..
                    }
                )
            })
            .expect("Second Wind evidence");
        assert_eq!(
            second_wind.band,
            ShopPolicyBandV1::CloseThreatGap,
            "second_wind={second_wind:#?}"
        );
        assert!(!second_wind.closed_threat_gaps.is_empty());
        let attack_potion_position = decision
            .evidence
            .iter()
            .position(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyPotion {
                        potion: PotionId::AttackPotion,
                        ..
                    }
                )
            })
            .expect("Attack Potion position");
        let attack_potion = &decision.evidence[attack_potion_position];
        assert!(matches!(
            &attack_potion.acquisition,
            ShopPolicyAcquisitionV1::Potion { traits, .. }
                if traits.contains(&PotionAcquisitionTraitV1::CardDiscovery)
                    && !traits.contains(&PotionAcquisitionTraitV1::CardAccess)
        ));
        assert!(attack_potion.matched_consumable_capabilities.is_empty());
        assert_eq!(attack_potion.band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(candidate_position(&decision, CardId::SecondWind) < attack_potion_position);
        assert!(
            Some(candidate_position(&decision, CardId::Uppercut))
                < decision.evidence.iter().position(|candidate| matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopLeave
                ))
        );
    }

    #[test]
    fn supported_hard_gap_and_construction_assets_precede_unverified_cards() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 1;
        session.run_state.floor_num = 4;
        session.run_state.boss_key = Some(EncounterId::SlimeBoss);
        session.run_state.current_hp = 85;
        session.run_state.max_hp = 85;
        session.run_state.gold = 100;
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
            CardId::PowerThrough,
            CardId::SecondWind,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();
        let mut shop = ShopState::new();
        shop.purge_cost = 75;
        shop.cards.extend([
            ShopCard {
                card_id: CardId::Uppercut,
                upgrades: 0,
                price: 69,
                can_buy: true,
                blocked_reason: None,
            },
            ShopCard {
                card_id: CardId::WildStrike,
                upgrades: 0,
                price: 47,
                can_buy: true,
                blocked_reason: None,
            },
            ShopCard {
                card_id: CardId::Armaments,
                upgrades: 0,
                price: 50,
                can_buy: true,
                blocked_reason: None,
            },
        ]);
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal)
            .expect("post-Second Wind A1F4 shop policy");
        let armaments = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::Armaments,
                        ..
                    }
                )
            })
            .expect("Armaments evidence");

        assert_eq!(
            armaments.card_acquisition_policy,
            Some(AcquisitionPolicyDecision {
                verdict: AcquisitionPolicyVerdict::ContextTake,
                reason: AcquisitionPolicyReason::HardGapAcquisition,
            })
        );
        assert_ne!(armaments.band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(
            Some(candidate_position(&decision, CardId::Armaments))
                < decision.evidence.iter().position(|candidate| matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopLeave
                ))
        );
        assert!(
            candidate_position(&decision, CardId::Uppercut)
                < candidate_position(&decision, CardId::WildStrike)
        );
        assert!(
            candidate_position(&decision, CardId::Armaments)
                < candidate_position(&decision, CardId::WildStrike)
        );
    }

    #[test]
    fn additional_marginal_purchase_cannot_empty_a_shop_visit_after_prior_spend() {
        // Model the exact post-bundle shape: the remaining card looks locally
        // affordable only because the visit already consumed 87 of 135 gold.
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 1;
        session.run_state.floor_num = 4;
        session.run_state.gold = 48;
        session.run_state.master_deck = [
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::PowerThrough,
            CardId::SecondWind,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.cards.push(ShopCard {
            card_id: CardId::WildStrike,
            upgrades: 0,
            price: 47,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);
        session.shop_visit_context = Some(ShopVisitContextV1 {
            entry_act: 1,
            entry_floor: 4,
            entry_gold: 135,
            maw_bank_live_at_entry: false,
            membership_card_owned_at_entry: false,
            spent_gold_in_visit: true,
        });

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal)
            .expect("post-purchase marginal shop policy");
        let wild_strike = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::WildStrike,
                        ..
                    }
                )
            })
            .expect("Wild Strike evidence");

        assert!(wild_strike.spent_gold_before_action);
        assert!(!wild_strike.durable_asset_support);
        assert_eq!(wild_strike.band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopLeave
        ));
    }

    #[test]
    fn duplicate_action_amplifier_needs_policy_support_after_prior_spend() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 2;
        session.run_state.floor_num = 20;
        session.run_state.boss_key = Some(EncounterId::Automaton);
        session.run_state.gold = 228;
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
            CardId::FiendFire,
            CardId::Offering,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();
        let mut shop = ShopState::new();
        shop.purge_cost = 75;
        shop.cards.push(ShopCard {
            card_id: CardId::DoubleTap,
            upgrades: 0,
            price: 136,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);
        session.shop_visit_context = Some(ShopVisitContextV1 {
            entry_act: 2,
            entry_floor: 20,
            entry_gold: 276,
            maw_bank_live_at_entry: false,
            membership_card_owned_at_entry: false,
            spent_gold_in_visit: true,
        });

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal)
            .expect("duplicate action-amplifier shop policy");
        let double_tap = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::DoubleTap,
                        ..
                    }
                )
            })
            .expect("Double Tap evidence");

        assert!(double_tap.action_supply.has_positive_supply());
        assert!(matches!(
            double_tap.card_acquisition_policy,
            Some(AcquisitionPolicyDecision {
                verdict: AcquisitionPolicyVerdict::Unverified,
                ..
            })
        ));
        assert!(!double_tap.durable_asset_support);
        assert_eq!(double_tap.band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopPurgeCard {
                card: CardId::Strike,
                ..
            }
        ));
    }

    #[test]
    fn unfunded_payoff_debt_cannot_claim_capability_or_spend_over_leave() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 1;
        session.run_state.floor_num = 2;
        session.run_state.boss_key = Some(EncounterId::TheGuardian);
        session.run_state.current_hp = 80;
        session.run_state.max_hp = 80;
        session.run_state.gold = 93;
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
            CardId::WildStrike,
            CardId::ShrugItOff,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();

        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.cards.extend([
            ShopCard {
                card_id: CardId::HeavyBlade,
                upgrades: 0,
                price: 49,
                can_buy: true,
                blocked_reason: None,
            },
            ShopCard {
                card_id: CardId::Clash,
                upgrades: 0,
                price: 52,
                can_buy: true,
                blocked_reason: None,
            },
        ]);
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_shop_policy_decision_v1(&session, &legal).expect("seed006 F2 shop policy");
        let heavy = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::HeavyBlade,
                        ..
                    }
                )
            })
            .expect("Heavy Blade evidence");
        let clash = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::Clash,
                        ..
                    }
                )
            })
            .expect("Clash evidence");
        let leave = decision
            .evidence
            .iter()
            .position(|candidate| {
                matches!(candidate.candidate_key, DecisionCandidateKey::ShopLeave)
            })
            .expect("leave evidence");

        assert!(!heavy
            .capability_improvements
            .iter()
            .any(|change| change.capability == StrategyCapabilityKindV1::LongFightScaling));
        assert!(heavy.reinforced_threat_capabilities.is_empty());
        assert!(
            matches!(
                &heavy.acquisition,
                ShopPolicyAcquisitionV1::Card {
                    component_signals,
                    ..
                } if component_signals
                    .debt_signals
                    .iter()
                    .any(|signal| is_unresolved_package_payoff_debt_signal_v1(*signal))
            ),
            "acquisition={:#?}",
            heavy.acquisition
        );
        assert_eq!(heavy.band, ShopPolicyBandV1::Liability);
        assert!(clash
            .added_deck_shape_risks
            .iter()
            .any(|risk| matches!(risk, DeckShapeRiskV1::ClashPlayabilityDebt { .. })));
        assert_eq!(clash.band, ShopPolicyBandV1::Liability);
        assert!(leave < candidate_position(&decision, CardId::HeavyBlade));
        assert!(leave < candidate_position(&decision, CardId::Clash));
        assert!(decision
            .prior
            .entries
            .iter()
            .all(|entry| entry.probability.is_finite() && entry.probability > 0.0));
    }

    #[test]
    fn first_armaments_is_an_asset_but_redundant_scope_is_not() {
        let mut first = RunControlSession::new(RunControlConfig::default());
        first.run_state.gold = 100;
        for lineage in 20_000..20_003 {
            first
                .run_state
                .master_deck
                .push(CombatCard::for_scenario(CardId::Impervious, lineage));
        }
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.cards.push(ShopCard {
            card_id: CardId::Armaments,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });
        first.engine_state = EngineState::Shop(shop.clone());
        let first_surface = build_decision_surface(&first);
        let first_legal = policy_candidates(&first_surface);
        let first_decision =
            exact_shop_policy_decision_v1(&first, &first_legal).expect("first Armaments policy");

        let mut duplicate = first.clone();
        duplicate
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Armaments, 10_001));
        duplicate.engine_state = EngineState::Shop(shop);
        let duplicate_surface = build_decision_surface(&duplicate);
        let duplicate_legal = policy_candidates(&duplicate_surface);
        let duplicate_decision = exact_shop_policy_decision_v1(&duplicate, &duplicate_legal)
            .expect("duplicate Armaments policy");

        assert_eq!(
            candidate_band(&first_decision, CardId::Armaments),
            ShopPolicyBandV1::EstablishStrategicAsset
        );
        assert_eq!(
            candidate_band(&duplicate_decision, CardId::Armaments),
            ShopPolicyBandV1::Liability
        );
        assert_eq!(duplicate_decision.prior.entries[0].candidate_id, "leave");
        assert!(duplicate_decision
            .prior
            .entries
            .iter()
            .all(|entry| entry.probability > 0.0));
    }

    #[test]
    fn exact_waffle_heal_precedes_generic_cleanup() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.current_hp = 30;
        session.run_state.max_hp = 80;
        session.run_state.gold = 300;
        let mut shop = ShopState::new();
        shop.purge_cost = 75;
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Waffle,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);

        let decision = exact_shop_policy_decision_v1(&session, &legal).expect("Waffle shop policy");

        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopBuyRelic {
                relic: RelicId::Waffle,
                ..
            }
        ));
        assert_eq!(
            decision.evidence[0].band,
            ShopPolicyBandV1::ImmediateSurvival
        );
        assert!(decision.evidence[0].hp_gain > 0);
        assert!(decision.evidence[0].missing_hp_recovered > 0);
    }

    #[test]
    fn strength_potion_requires_a_real_deck_payoff_to_outrank_generic_block() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 1;
        session.run_state.floor_num = 3;
        session.run_state.boss_key = Some(EncounterId::SlimeBoss);
        session.run_state.gold = 110;
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
            CardId::Whirlwind,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();
        let mut shop = ShopState::new();
        shop.purge_cost = 75;
        shop.potions.extend([
            ShopPotion {
                potion_id: PotionId::BlockPotion,
                price: 49,
                can_buy: true,
                blocked_reason: None,
            },
            ShopPotion {
                potion_id: PotionId::StrengthPotion,
                price: 52,
                can_buy: true,
                blocked_reason: None,
            },
        ]);
        session.engine_state = EngineState::Shop(shop.clone());

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let supported =
            exact_shop_policy_decision_v1(&session, &legal).expect("strength-payoff potion policy");
        assert_eq!(
            potion_candidate(&supported, PotionId::StrengthPotion).matched_consumable_packages,
            vec![StrategyPackageIdV2::StrengthScaling]
        );
        assert!(
            potion_candidate(&supported, PotionId::StrengthPotion).consumable_conversion_support
        );
        assert!(!potion_candidate(&supported, PotionId::StrengthPotion).durable_asset_support);
        assert_eq!(
            potion_candidate(&supported, PotionId::StrengthPotion).band,
            ShopPolicyBandV1::AcquireTacticalReserve
        );
        assert_eq!(
            potion_candidate(&supported, PotionId::BlockPotion).band,
            ShopPolicyBandV1::SpeculativePurchase
        );
        assert_eq!(
            shop_band_priority(ShopPolicyBandV1::AcquireTacticalReserve),
            shop_band_priority(ShopPolicyBandV1::EstablishStrategicAsset),
            "a one-combat reserve must stay useful without outranking every permanent asset by category alone"
        );
        assert!(
            potion_position(&supported, PotionId::StrengthPotion)
                < potion_position(&supported, PotionId::BlockPotion)
        );

        session
            .run_state
            .master_deck
            .retain(|card| card.id != CardId::Whirlwind);
        session.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let unsupported = exact_shop_policy_decision_v1(&session, &legal)
            .expect("no-strength-payoff potion policy");
        assert!(potion_candidate(&unsupported, PotionId::StrengthPotion)
            .matched_consumable_packages
            .is_empty());
        assert!(
            potion_position(&unsupported, PotionId::BlockPotion)
                < potion_position(&unsupported, PotionId::StrengthPotion)
        );
    }

    fn power_potion_recovery_decision(
        next_room: RoomType,
        relics: &[RelicId],
    ) -> ExactShopPolicyDecisionV1 {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.current_hp = 60;
        session.run_state.max_hp = 68;
        session.run_state.gold = 100;
        session.run_state.relics = relics.iter().copied().map(RelicState::new).collect();

        let mut current = MapRoomNode::new(0, 0);
        current.class = Some(RoomType::ShopRoom);
        current.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut next = MapRoomNode::new(0, 1);
        next.class = Some(next_room);
        let mut map = MapState::new(vec![vec![current], vec![next]]);
        map.current_x = 0;
        map.current_y = 0;
        session.run_state.map = map;

        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.potions.push(ShopPotion {
            potion_id: PotionId::PowerPotion,
            price: 54,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        exact_shop_policy_decision_v1(&session, &legal).expect("Power Potion recovery policy")
    }

    #[test]
    fn potion_purchase_keeps_relic_triggered_combat_healing_conditional() {
        let forced_combat = power_potion_recovery_decision(
            RoomType::MonsterRoomElite,
            &[RelicId::ToyOrnithopter, RelicId::MagicFlower],
        );
        let power = potion_candidate(&forced_combat, PotionId::PowerPotion);
        assert_eq!(power.hp_gain, 0, "the purchase itself does not heal");
        assert_eq!(power.missing_hp_recovered, 0);
        assert_eq!(power.guaranteed_realizable_hp_gain, 0);
        assert_eq!(power.conditional_combat_on_use_hp_capacity, 8);
        assert!(power.consumable_conversion_support);
        assert_eq!(power.band, ShopPolicyBandV1::AcquireTacticalReserve);

        let rest_next = power_potion_recovery_decision(
            RoomType::RestRoom,
            &[RelicId::ToyOrnithopter, RelicId::MagicFlower],
        );
        let power = potion_candidate(&rest_next, PotionId::PowerPotion);
        assert_eq!(power.conditional_combat_on_use_hp_capacity, 8);
        assert!(!power.consumable_conversion_support);
        assert_eq!(power.band, ShopPolicyBandV1::SpeculativePurchase);

        let blocked = power_potion_recovery_decision(
            RoomType::MonsterRoomElite,
            &[
                RelicId::ToyOrnithopter,
                RelicId::MagicFlower,
                RelicId::MarkOfTheBloom,
            ],
        );
        let power = potion_candidate(&blocked, PotionId::PowerPotion);
        assert_eq!(power.conditional_combat_on_use_hp_capacity, 0);
        assert!(!power.consumable_conversion_support);
    }

    #[test]
    fn fruit_juice_purchase_accounts_for_the_guaranteed_run_level_use() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 1;
        session.run_state.current_hp = 68;
        session.run_state.max_hp = 75;
        session.run_state.gold = 200;
        session
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Whirlwind, 10_000));
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.potions.extend([
            ShopPotion {
                potion_id: PotionId::FruitJuice,
                price: 109,
                can_buy: true,
                blocked_reason: None,
            },
            ShopPotion {
                potion_id: PotionId::StrengthPotion,
                price: 57,
                can_buy: true,
                blocked_reason: None,
            },
        ]);
        session.engine_state = EngineState::Shop(shop.clone());

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_shop_policy_decision_v1(&session, &legal).expect("Fruit Juice shop policy");
        let fruit_juice = potion_candidate(&decision, PotionId::FruitJuice);
        assert_eq!(
            fruit_juice.hp_gain, 0,
            "purchase itself only fills the belt"
        );
        assert_eq!(fruit_juice.max_hp_gain, 0);
        assert_eq!(fruit_juice.guaranteed_realizable_hp_gain, 5);
        assert_eq!(fruit_juice.guaranteed_realizable_max_hp_gain, 5);
        assert_eq!(fruit_juice.band, ShopPolicyBandV1::EstablishStrategicAsset);
        assert!(fruit_juice.durable_asset_support);
        assert!(matches!(
            &fruit_juice.acquisition,
            ShopPolicyAcquisitionV1::Potion { traits, .. }
                if traits.contains(&PotionAcquisitionTraitV1::PermanentMaxHp)
        ));
        let leave_position = decision
            .evidence
            .iter()
            .position(|candidate| {
                matches!(candidate.candidate_key, DecisionCandidateKey::ShopLeave)
            })
            .expect("leave position");
        assert!(
            potion_position(&decision, PotionId::FruitJuice) < leave_position,
            "a guaranteed permanent resource must remain supported without being mislabeled as an immediate survival emergency"
        );

        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::SacredBark));
        session.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let bark = exact_shop_policy_decision_v1(&session, &legal)
            .expect("Sacred Bark Fruit Juice shop policy");
        assert_eq!(
            potion_candidate(&bark, PotionId::FruitJuice).guaranteed_realizable_max_hp_gain,
            10
        );
    }

    #[test]
    fn temporary_strength_and_vulnerable_potions_do_not_claim_long_fight_scaling() {
        assert_eq!(
            potion_capabilities(
                PotionId::SteroidPotion,
                PotionAcquisitionTraitV1::StrengthGain,
            ),
            vec![StrategyCapabilityKindV1::SingleTargetFrontload]
        );
        assert_eq!(
            potion_capabilities(
                PotionId::FearPotion,
                PotionAcquisitionTraitV1::VulnerableSetup,
            ),
            vec![StrategyCapabilityKindV1::SingleTargetFrontload]
        );
        assert_eq!(
            potion_capabilities(
                PotionId::StrengthPotion,
                PotionAcquisitionTraitV1::StrengthGain,
            ),
            vec![
                StrategyCapabilityKindV1::SingleTargetFrontload,
                StrategyCapabilityKindV1::LongFightScaling,
            ]
        );
    }

    #[test]
    fn anchor_is_a_durable_shop_asset_instead_of_an_untyped_speculation() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 200;
        let mut shop = ShopState::new();
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Anchor,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);

        let decision = exact_shop_policy_decision_v1(&session, &legal).expect("Anchor shop policy");
        let anchor = relic_candidate(&decision, RelicId::Anchor);

        assert!(matches!(
            &anchor.acquisition,
            ShopPolicyAcquisitionV1::Relic { traits, .. }
                if traits.contains(&RelicAcquisitionTraitV1::CoreDefenseOrSurvival)
        ));
        assert!(anchor.durable_asset_support);
        assert_eq!(anchor.band, ShopPolicyBandV1::EstablishStrategicAsset);
    }

    #[test]
    fn full_health_shop_distinguishes_repeatable_supply_from_max_hp_capacity() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.act_num = 3;
        session.run_state.floor_num = 37;
        session.run_state.current_hp = 76;
        session.run_state.max_hp = 76;
        session.run_state.gold = 361;
        for (lineage, card) in [
            CardId::Anger,
            CardId::Whirlwind,
            CardId::FiendFire,
            CardId::DoubleTap,
            CardId::Reaper,
        ]
        .into_iter()
        .enumerate()
        {
            session
                .run_state
                .master_deck
                .push(CombatCard::for_scenario(card, 40_000 + lineage as u32));
        }
        let mut shop = ShopState::new();
        shop.purge_cost = 100;
        shop.relics.extend([
            ShopRelic {
                relic_id: RelicId::Nunchaku,
                price: 150,
                can_buy: true,
                blocked_reason: None,
            },
            ShopRelic {
                relic_id: RelicId::Lantern,
                price: 165,
                can_buy: true,
                blocked_reason: None,
            },
            ShopRelic {
                relic_id: RelicId::Waffle,
                price: 156,
                can_buy: true,
                blocked_reason: None,
            },
        ]);
        session.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);

        let decision =
            exact_shop_policy_decision_v1(&session, &legal).expect("full-health shop policy");
        let nunchaku = relic_candidate(&decision, RelicId::Nunchaku);
        let lantern = relic_candidate(&decision, RelicId::Lantern);
        let waffle = relic_candidate(&decision, RelicId::Waffle);

        assert!(matches!(
            &nunchaku.acquisition,
            ShopPolicyAcquisitionV1::Relic { traits, .. }
                if traits.contains(&RelicAcquisitionTraitV1::ActionSupply)
        ));
        assert_eq!(nunchaku.action_supply.triggered_repeatable_sources, 1);
        assert_eq!(
            nunchaku.action_supply.cost_or_resource_compression_sources,
            1
        );
        assert_eq!(nunchaku.band, ShopPolicyBandV1::EstablishStrategicAsset);
        assert_eq!(lantern.action_supply.opening_once_options, 1);
        assert_eq!(
            lantern.action_supply.cost_or_resource_compression_sources,
            1
        );
        assert_eq!(lantern.band, ShopPolicyBandV1::EstablishStrategicAsset);
        assert_eq!(waffle.hp_gain, waffle.max_hp_gain);
        assert_eq!(waffle.missing_hp_recovered, 0);
        assert_eq!(waffle.band, ShopPolicyBandV1::EstablishStrategicAsset);
    }

    #[test]
    fn forge_blessing_requires_real_upgrade_targets_without_becoming_permanent_coverage() {
        let mut supported = RunControlSession::new(RunControlConfig::default());
        supported.run_state.gold = 100;
        supported
            .run_state
            .relics
            .push(RelicState::new(RelicId::FusionHammer));
        supported
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::DarkEmbrace, 41_000));
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.potions.push(ShopPotion {
            potion_id: PotionId::BlessingOfTheForge,
            price: 48,
            can_buy: true,
            blocked_reason: None,
        });
        supported.engine_state = EngineState::Shop(shop.clone());
        let supported_surface = build_decision_surface(&supported);
        let supported_legal = policy_candidates(&supported_surface);
        let supported_decision = exact_shop_policy_decision_v1(&supported, &supported_legal)
            .expect("supported Forge blessing policy");
        let blessing = potion_candidate(&supported_decision, PotionId::BlessingOfTheForge);

        assert!(matches!(
            &blessing.acquisition,
            ShopPolicyAcquisitionV1::Potion {
                traits,
                combat_upgrade_scope: Some(CombatUpgradeScopeV1::WholeHand),
                requirements,
                requirement_support: AcquisitionRequirementSupportV1::Current,
                ..
            } if traits.contains(&PotionAcquisitionTraitV1::CombatUpgrade)
                && requirements.contains(&AcquisitionRequirementV1::UpgradeableCombatCards)
        ));
        assert_eq!(blessing.band, ShopPolicyBandV1::AcquireTacticalReserve);
        assert_eq!(blessing.upgrade_scope_before, None);
        assert_eq!(blessing.upgrade_scope_after, None);

        let mut unsupported = supported.clone();
        let mut upgraded = CombatCard::for_scenario(CardId::DarkEmbrace, 41_001);
        assert!(upgrade_card_once_java(&mut upgraded));
        unsupported.run_state.master_deck = vec![upgraded];
        unsupported.engine_state = EngineState::Shop(shop);
        let unsupported_surface = build_decision_surface(&unsupported);
        let unsupported_legal = policy_candidates(&unsupported_surface);
        let unsupported_decision = exact_shop_policy_decision_v1(&unsupported, &unsupported_legal)
            .expect("unsupported Forge blessing policy");
        let blessing = potion_candidate(&unsupported_decision, PotionId::BlessingOfTheForge);

        assert!(matches!(
            blessing.acquisition,
            ShopPolicyAcquisitionV1::Potion {
                requirement_support: AcquisitionRequirementSupportV1::Unavailable,
                ..
            }
        ));
        assert_eq!(blessing.band, ShopPolicyBandV1::SpeculativePurchase);
    }

    #[test]
    fn smoke_bomb_is_a_supported_tactical_reserve_only_with_a_visible_escape_edge() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 100;
        for lineage in 30_000..30_003 {
            session
                .run_state
                .master_deck
                .push(CombatCard::for_scenario(CardId::Impervious, lineage));
        }
        let mut combat = MapRoomNode::new(0, 0);
        combat.class = Some(RoomType::MonsterRoom);
        combat.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut rest = MapRoomNode::new(0, 1);
        rest.class = Some(RoomType::RestRoom);
        session.run_state.map = MapState::new(vec![vec![combat], vec![rest]]);
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.potions.push(ShopPotion {
            potion_id: PotionId::SmokeBomb,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop.clone());
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_shop_policy_decision_v1(&session, &legal).expect("route escape shop policy");
        let smoke = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyPotion {
                        potion: PotionId::SmokeBomb,
                        ..
                    }
                )
            })
            .expect("Smoke Bomb evidence");
        assert_eq!(smoke.band, ShopPolicyBandV1::AcquireTacticalReserve);
        assert!(matches!(
            smoke.acquisition,
            ShopPolicyAcquisitionV1::Potion {
                requirement_support: AcquisitionRequirementSupportV1::Current,
                ..
            }
        ));

        let mut no_route = session;
        no_route.run_state.map = MapState::new(Vec::new());
        no_route.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&no_route);
        let legal = policy_candidates(&surface);
        let decision =
            exact_shop_policy_decision_v1(&no_route, &legal).expect("no-route shop policy");
        let smoke = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyPotion {
                        potion: PotionId::SmokeBomb,
                        ..
                    }
                )
            })
            .expect("Smoke Bomb evidence");
        assert_eq!(smoke.band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(matches!(
            smoke.acquisition,
            ShopPolicyAcquisitionV1::Potion {
                requirement_support: AcquisitionRequirementSupportV1::Unavailable,
                ..
            }
        ));
    }

    #[test]
    fn orrery_is_ranked_from_its_real_nested_reward_successor() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 300;
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Orrery,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);

        let decision = exact_shop_policy_decision_v1(&session, &legal).expect("Orrery shop policy");
        let orrery = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyRelic {
                        relic: RelicId::Orrery,
                        ..
                    }
                )
            })
            .expect("Orrery evidence");

        assert_eq!(orrery.followup, ShopPolicyFollowupV1::Reward);
        assert_eq!(orrery.band, ShopPolicyBandV1::EstablishStrategicAsset);
        assert_eq!(decision.prior.entries[0].candidate_id, orrery.candidate_id);
    }

    #[test]
    fn dollys_mirror_selection_does_not_create_value_without_a_premium_target() {
        let (_session, decision) = dollys_mirror_decision(&[
            (CardId::SecondWind, 0),
            (CardId::ShrugItOff, 0),
            (CardId::Clothesline, 0),
            (CardId::SpotWeakness, 0),
        ]);
        let mirror = relic_candidate(&decision, RelicId::DollysMirror);

        assert_eq!(mirror.followup, ShopPolicyFollowupV1::Selection);
        assert!(matches!(
            mirror.acquisition,
            ShopPolicyAcquisitionV1::Relic {
                requirement_support: AcquisitionRequirementSupportV1::Unavailable,
                ..
            }
        ));
        assert!(!mirror.durable_asset_support);
        assert_eq!(mirror.band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopLeave
        ));
    }

    #[test]
    fn dollys_mirror_carries_its_premium_duplicate_target_into_shop_evidence() {
        let (session, decision) =
            dollys_mirror_decision(&[(CardId::SecondWind, 0), (CardId::Offering, 0)]);
        let mirror = relic_candidate(&decision, RelicId::DollysMirror);

        assert!(matches!(
            mirror.acquisition,
            ShopPolicyAcquisitionV1::Relic {
                requirement_support: AcquisitionRequirementSupportV1::PremiumDuplicateTarget {
                    card: CardId::Offering,
                    upgrades: 0,
                    role: DuplicateTargetRoleV1::SetupAccelerator,
                    stack_behavior: DuplicateStackBehaviorV1::Stackable,
                    ..
                },
                ..
            }
        ));
        assert!(mirror.durable_asset_support);
        assert_eq!(mirror.band, ShopPolicyBandV1::EstablishStrategicAsset);
        assert!(matches!(
            decision.evidence[0].candidate_key,
            DecisionCandidateKey::ShopBuyRelic {
                relic: RelicId::DollysMirror,
                ..
            }
        ));

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let audit = decision.audit(&legal).expect("Dolly's Mirror shop audit");
        let mirror_audit = audit
            .candidates
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyRelic {
                        relic: RelicId::DollysMirror,
                        ..
                    }
                )
            })
            .expect("Dolly's Mirror audit evidence");
        assert_eq!(
            mirror_audit.acquisition_requirements,
            vec![AcquisitionRequirementV1::DuplicateTarget]
        );
        assert!(matches!(
            mirror_audit.acquisition_requirement_support,
            Some(AcquisitionRequirementSupportV1::PremiumDuplicateTarget {
                card: CardId::Offering,
                ..
            })
        ));
    }

    #[test]
    fn purge_and_supported_noncritical_cards_share_the_comparison_surface() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 128;
        let upgraded = |card, lineage| {
            let mut card = CombatCard::for_scenario(card, lineage);
            card.upgrades = 1;
            card
        };
        session.run_state.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Strike, 2),
            CombatCard::for_scenario(CardId::Defend, 3),
            CombatCard::for_scenario(CardId::Defend, 4),
            CombatCard::for_scenario(CardId::Bash, 5),
            upgraded(CardId::FeelNoPain, 6),
            upgraded(CardId::FeelNoPain, 7),
            upgraded(CardId::BurningPact, 8),
            upgraded(CardId::TrueGrit, 9),
            upgraded(CardId::FireBreathing, 10),
            upgraded(CardId::WildStrike, 11),
            upgraded(CardId::Whirlwind, 12),
        ];
        let mut shop = ShopState::new();
        shop.purge_cost = 125;
        shop.cards.extend([
            ShopCard {
                card_id: CardId::DarkEmbrace,
                upgrades: 1,
                price: 37,
                can_buy: true,
                blocked_reason: None,
            },
            ShopCard {
                card_id: CardId::Disarm,
                upgrades: 0,
                price: 79,
                can_buy: true,
                blocked_reason: None,
            },
        ]);
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&session, &legal).expect("paired shop policy");

        let dark_embrace = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::DarkEmbrace,
                        ..
                    }
                )
            })
            .expect("Dark Embrace evidence");
        assert!(matches!(
            &dark_embrace.acquisition,
            ShopPolicyAcquisitionV1::Card { construction, .. }
                if construction.construction_roles.contains(
                    &crate::ai::strategy::acquisition::AcquisitionConstructionRole::SupportedPackageComponent
                )
                && !construction.strategic_delta.adds_deployability_debt
        ));
        assert_eq!(
            dark_embrace.card_acquisition_policy,
            Some(AcquisitionPolicyDecision {
                verdict: AcquisitionPolicyVerdict::ContextTake,
                reason: AcquisitionPolicyReason::ConstructionRoleAccepted,
            })
        );
        assert_eq!(dark_embrace.band, ShopPolicyBandV1::EstablishStrategicAsset);
        assert_eq!(
            dark_embrace.card_acquisition_opportunity_cost,
            Some(AcquisitionOpportunityCost::SpendsPurgeReserve)
        );
        let disarm = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopBuyCard {
                        card: CardId::Disarm,
                        ..
                    }
                )
            })
            .expect("Disarm evidence");
        assert_eq!(
            disarm.card_acquisition_policy,
            Some(AcquisitionPolicyDecision {
                verdict: AcquisitionPolicyVerdict::ContextTake,
                reason: AcquisitionPolicyReason::ConstructionRoleAccepted,
            })
        );
        assert_eq!(disarm.band, ShopPolicyBandV1::EstablishStrategicAsset);
        assert_eq!(
            disarm.card_acquisition_opportunity_cost,
            Some(AcquisitionOpportunityCost::SpendsPurgeReserve)
        );
        let strike_purge = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopPurgeCard { deck_index: 0, .. }
                )
            })
            .expect("starter Strike purge");
        assert_eq!(strike_purge.band, ShopPolicyBandV1::DeckRepair);
        assert_eq!(
            strike_purge
                .purge_target
                .as_ref()
                .map(|target| target.target_loss.tier),
            Some(DeckMutationTargetLossTierV1::LowValue)
        );
        let feel_no_pain_purge = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.candidate_key,
                    DecisionCandidateKey::ShopPurgeCard {
                        card: CardId::FeelNoPain,
                        ..
                    }
                )
            })
            .expect("Feel No Pain purge");
        assert_eq!(feel_no_pain_purge.band, ShopPolicyBandV1::Liability);
        let leave_position = decision
            .evidence
            .iter()
            .position(|candidate| {
                matches!(candidate.candidate_key, DecisionCandidateKey::ShopLeave)
            })
            .expect("leave evidence");
        let disarm_position = decision
            .evidence
            .iter()
            .position(|candidate| candidate.candidate_id == disarm.candidate_id)
            .expect("Disarm position");
        let dark_embrace_position = decision
            .evidence
            .iter()
            .position(|candidate| candidate.candidate_id == dark_embrace.candidate_id)
            .expect("Dark Embrace position");
        let strike_purge_position = decision
            .evidence
            .iter()
            .position(|candidate| candidate.candidate_id == strike_purge.candidate_id)
            .expect("Strike purge position");
        assert!(strike_purge_position < leave_position);
        assert!(disarm_position < leave_position);
        assert!(dark_embrace_position < leave_position);
    }

    #[test]
    fn late_act_three_gold_payoff_requires_a_later_shop_conversion_window() {
        fn session_with_route(route: &[RoomType]) -> RunControlSession {
            let mut session = RunControlSession::new(RunControlConfig::default());
            session.run_state.act_num = 3;
            session.run_state.floor_num = 46;
            session.run_state.boss_key = Some(EncounterId::AwakenedOne);
            session.run_state.gold = 428;
            // Exact F46 production deck: Hand of Greed improves a generic
            // efficiency axis here but closes no remaining threat gap.
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
                (CardId::DemonForm, 1),
                (CardId::TrueGrit, 1),
                (CardId::DarkShackles, 0),
                (CardId::Inflame, 0),
                (CardId::FeelNoPain, 0),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (card, upgrades))| {
                let mut owned = CombatCard::for_scenario(card, index as u32);
                owned.upgrades = upgrades;
                owned
            })
            .collect();

            let rows = route
                .iter()
                .enumerate()
                .map(|(y, room_type)| {
                    let mut node = MapRoomNode::new(0, y as i32);
                    node.class = Some(*room_type);
                    if y + 1 < route.len() {
                        node.edges
                            .insert(MapEdge::new(0, y as i32, 0, y as i32 + 1));
                    }
                    vec![node]
                })
                .collect::<Vec<_>>();
            let mut map = MapState::new(rows);
            map.current_x = 0;
            map.current_y = 0;
            session.run_state.map = map;

            let mut shop = ShopState::new();
            shop.purge_available = false;
            shop.cards.push(ShopCard {
                card_id: CardId::HandOfGreed,
                upgrades: 1,
                price: 192,
                can_buy: true,
                blocked_reason: None,
            });
            session.engine_state = EngineState::Shop(shop);
            session
        }

        let no_conversion = session_with_route(&[
            RoomType::ShopRoom,
            RoomType::RestRoom,
            RoomType::MonsterRoomBoss,
        ]);
        let surface = build_decision_surface(&no_conversion);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&no_conversion, &legal)
            .expect("late Act 3 shop policy without conversion");
        assert_eq!(
            candidate_band(&decision, CardId::HandOfGreed),
            ShopPolicyBandV1::SpeculativePurchase
        );
        assert!(
            decision
                .evidence
                .iter()
                .position(|candidate| matches!(
                    candidate.acquisition,
                    ShopPolicyAcquisitionV1::Leave
                ))
                .expect("leave position")
                < candidate_position(&decision, CardId::HandOfGreed)
        );

        let convertible = session_with_route(&[
            RoomType::ShopRoom,
            RoomType::MonsterRoom,
            RoomType::ShopRoom,
            RoomType::RestRoom,
        ]);
        let surface = build_decision_surface(&convertible);
        let legal = policy_candidates(&surface);
        let decision = exact_shop_policy_decision_v1(&convertible, &legal)
            .expect("late Act 3 shop policy with conversion");
        assert_eq!(
            candidate_band(&decision, CardId::HandOfGreed),
            ShopPolicyBandV1::EstablishStrategicAsset
        );
    }

    #[test]
    fn unverified_card_cannot_spend_purge_reserve_on_coarse_shop_signals() {
        let session = RunControlSession::new(RunControlConfig::default());
        let reward = RewardCard::new(CardId::SeverSoul, 0);
        let admission = assess_reward_admission_from_master_deck(
            &session.run_state.master_deck,
            CardId::SeverSoul,
            0,
        );
        let acquisition = ShopPolicyAcquisitionV1::Card {
            card: CardId::SeverSoul,
            upgrades: 0,
            copies_before: 1,
            semantics: card_reward_semantic_profile_v1(&reward),
            access: None,
            component_signals: CardComponentSignalReportV1 {
                card: CardId::SeverSoul,
                roles: Vec::new(),
                positive_signals: Vec::new(),
                debt_signals: Vec::new(),
                note_signals: Vec::new(),
            },
            construction: assess_card_construction_v1(
                DeckPlanSnapshot::from_run_state(&session.run_state),
                CardId::SeverSoul,
                0,
                &admission,
            ),
        };
        let unsupported = Some(AcquisitionPolicyDecision {
            verdict: AcquisitionPolicyVerdict::Unverified,
            reason: AcquisitionPolicyReason::NoPolicySupport,
        });

        let closed_threat_gap = ShopPolicyThreatGapKeyV1 {
            tag: StrategyThreatTagV1::TimedDamageRace,
            source: StrategyThreatSourceV1::ActBoss,
            subject: "test boss".to_string(),
        };
        let band = shop_policy_band_v1(
            &session,
            &acquisition,
            ShopPolicyFollowupV1::Shop,
            false,
            0,
            0,
            0,
            false,
            1,
            &[closed_threat_gap],
            &[StrategyCapabilityKindV1::TimedDamageRace],
            &[],
            &[],
            &[],
            &[],
            None,
            None,
            false,
            false,
            false,
            false,
            unsupported,
            Some(AcquisitionOpportunityCost::SpendsPurgeReserve),
            false,
            false,
            ActionSupplyDeltaV1::default(),
            None,
        );

        assert_eq!(band, ShopPolicyBandV1::SpeculativePurchase);
        assert!(!reinforcement_has_card_policy_support_v1(unsupported));
    }
}
