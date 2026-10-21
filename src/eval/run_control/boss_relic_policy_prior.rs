use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::Serialize;

use crate::ai::action_supply_v1::{
    action_supply_delta_v1, action_supply_profile_v1, ActionSupplyDeltaV1, ActionSupplyProfileV1,
};
use crate::ai::card_analysis_v1::card_analysis_profile_v1;
use crate::ai::card_semantics_v1::{relic_acquisition_traits_v1, RelicAcquisitionTraitV1};
use crate::ai::deck_startup_profile_v1::{deck_startup_profile_v1, DeckStartupProfileV1};
use crate::ai::strategic::{
    has_curse_mitigation_or_synergy_v1, run_debt_projection_for_relic_v1, RunDebtContractKindV1,
    RunDebtProjectionV1,
};
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::deck_strategic_deficit::{StrategicBurdenLevel, StrategicDeficitLevel};
use crate::ai::upgrade_planner_v1::{
    plan_upgrades_v1, UpgradeDebtSeverityV1, UpgradeSlotPressureV1,
};
use crate::content::relics::{energy_master_delta, RelicId};
use crate::state::core::EngineState;
use crate::state::rewards::RewardItem;
use crate::state::RunPendingChoiceReason;

use super::{
    exact_run_policy_decision_v1, owned_run_policy_actions_v1, positive_ranked_run_policy_prior_v1,
    DecisionCandidateKey, ExactRunPolicyActionSuccessorV1, ExactRunPolicyDecisionV1,
    RunControlSession, RunPolicyCandidateV1, RunPolicyPriorV1,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BossRelicPolicyBandV1 {
    SupportedEnergy,
    DeckReconstruction,
    StrategicAsset,
    StrategicRepair,
    StarterUpgrade,
    ConditionalGrowth,
    PreserveState,
    ConstrainedEnergy,
    Liability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BossRelicEnergyPressureV1 {
    Covered,
    Thin,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BossRelicPolicyFollowupV1 {
    ActAdvanced,
    Selection(RunPendingChoiceReason),
    Reward,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BossRelicPolicyActionV1 {
    Pick {
        relic: RelicId,
        energy_gain: u8,
        replaces_existing_relic: bool,
        changed_deck_cards: usize,
        followup: BossRelicPolicyFollowupV1,
        generated_relics: Vec<RelicId>,
        strategic_relic_count: usize,
        traits: Vec<RelicAcquisitionTraitV1>,
        added_debts: Vec<RunDebtContractKindV1>,
        unresolved_debt_terms: usize,
        compounding_debt_count: usize,
        startup: BossRelicStartupDeltaV1,
        action_supply: BossRelicActionSupplyDeltaV1,
    },
    Skip,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BossRelicStartupDeltaV1 {
    pub setup_debt: i16,
    pub setup_payment: i16,
    pub immediate_survival: i16,
    pub payoff_engine: i16,
    pub combat_shape_risk: i16,
    pub strong_draw: i16,
    pub snecko_draw_bonus: i16,
    pub snecko_random_cost_debt: i16,
    pub snecko_high_cost_payoff: i16,
    pub snecko_pellets_cleanup_access: i16,
}

pub type BossRelicActionSupplyDeltaV1 = ActionSupplyDeltaV1;

#[derive(Clone, Debug, PartialEq)]
pub struct BossRelicPolicyActionEvidenceV1 {
    pub candidate_id: String,
    pub candidate_key: DecisionCandidateKey,
    pub action: BossRelicPolicyActionV1,
    pub band: BossRelicPolicyBandV1,
    surface_index: usize,
}

#[derive(Clone, Debug)]
pub struct ExactBossRelicPolicyDecisionV1 {
    pub exact: ExactRunPolicyDecisionV1,
    pub context: BossRelicPolicyAuditContextV1,
    pub evidence: Vec<BossRelicPolicyActionEvidenceV1>,
    pub prior: RunPolicyPriorV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BossRelicPolicyAuditContextV1 {
    pub energy_master: u8,
    pub energy_pressure: BossRelicEnergyPressureV1,
    pub deck_size: usize,
    pub deck_access: StrategicDeficitLevel,
    pub energy_or_playability: StrategicDeficitLevel,
    pub deck_burden: StrategicBurdenLevel,
    pub zero_cost_cards: u8,
    pub low_cost_cards: u8,
    pub high_cost_cards: u8,
    pub x_cost_cards: usize,
    pub self_damage_sources: u8,
    pub upgrade_candidates: usize,
    pub important_upgrade_candidates: usize,
    pub cost_reduction_upgrade_candidates: usize,
    pub unpaid_core_upgrade_debts: usize,
    pub upgrade_slot_pressure: String,
    pub curse_mitigation_or_synergy: bool,
    pub potion_slots: usize,
    pub potion_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BossRelicPolicyAuditCandidateV1 {
    pub owner_rank: usize,
    pub candidate_id: String,
    pub label: String,
    pub candidate_key: DecisionCandidateKey,
    pub action: String,
    pub band: BossRelicPolicyBandV1,
    pub relic: Option<RelicId>,
    pub energy_gain: u8,
    pub replaces_existing_relic: bool,
    pub changed_deck_cards: usize,
    pub followup: Option<String>,
    pub generated_relics: Vec<RelicId>,
    pub strategic_relic_count: usize,
    pub traits: Vec<String>,
    pub added_debts: Vec<String>,
    pub unresolved_debt_terms: usize,
    pub compounding_debt_count: usize,
    pub startup: BossRelicStartupDeltaV1,
    pub action_supply: BossRelicActionSupplyDeltaV1,
    pub surface_index: usize,
    pub prior_probability: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactBossRelicPolicyAuditV1 {
    pub context: BossRelicPolicyAuditContextV1,
    pub candidates: Vec<BossRelicPolicyAuditCandidateV1>,
}

impl ExactBossRelicPolicyDecisionV1 {
    pub fn audit(
        &self,
        legal: &[RunPolicyCandidateV1<'_>],
    ) -> Result<ExactBossRelicPolicyAuditV1, String> {
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
                            "boss relic policy audit could not find legal candidate '{}'",
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
                            "boss relic policy audit could not find prior for candidate '{}'",
                            evidence.candidate_id
                        )
                    })?;
                let (
                    relic,
                    energy_gain,
                    replaces_existing_relic,
                    changed_deck_cards,
                    followup,
                    generated_relics,
                    strategic_relic_count,
                    traits,
                    added_debts,
                    unresolved_debt_terms,
                    compounding_debt_count,
                    startup,
                    action_supply,
                ) = match &evidence.action {
                    BossRelicPolicyActionV1::Pick {
                        relic,
                        energy_gain,
                        replaces_existing_relic,
                        changed_deck_cards,
                        followup,
                        generated_relics,
                        strategic_relic_count,
                        traits,
                        added_debts,
                        unresolved_debt_terms,
                        compounding_debt_count,
                        startup,
                        action_supply,
                    } => (
                        Some(*relic),
                        *energy_gain,
                        *replaces_existing_relic,
                        *changed_deck_cards,
                        Some(format!("{followup:?}")),
                        generated_relics.clone(),
                        *strategic_relic_count,
                        traits.iter().map(|value| format!("{value:?}")).collect(),
                        added_debts
                            .iter()
                            .map(|value| format!("{value:?}"))
                            .collect(),
                        *unresolved_debt_terms,
                        *compounding_debt_count,
                        *startup,
                        *action_supply,
                    ),
                    BossRelicPolicyActionV1::Skip => (
                        None,
                        0,
                        false,
                        0,
                        None,
                        Vec::new(),
                        0,
                        Vec::new(),
                        Vec::new(),
                        0,
                        0,
                        BossRelicStartupDeltaV1::default(),
                        BossRelicActionSupplyDeltaV1::default(),
                    ),
                };
                Ok(BossRelicPolicyAuditCandidateV1 {
                    owner_rank,
                    candidate_id: evidence.candidate_id.clone(),
                    label: legal_candidate.label.to_string(),
                    candidate_key: evidence.candidate_key.clone(),
                    action: format!("{:?}", evidence.action),
                    band: evidence.band,
                    relic,
                    energy_gain,
                    replaces_existing_relic,
                    changed_deck_cards,
                    followup,
                    generated_relics,
                    strategic_relic_count,
                    traits,
                    added_debts,
                    unresolved_debt_terms,
                    compounding_debt_count,
                    startup,
                    action_supply,
                    surface_index: evidence.surface_index,
                    prior_probability,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(ExactBossRelicPolicyAuditV1 {
            context: self.context.clone(),
            candidates,
        })
    }
}

struct BossRelicPolicyContextV1 {
    startup_before: DeckStartupProfileV1,
    action_supply_before: ActionSupplyProfileV1,
    debt: Vec<(RelicId, RunDebtProjectionV1)>,
    audit: BossRelicPolicyAuditContextV1,
}

pub fn exact_boss_relic_policy_prior_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<RunPolicyPriorV1, String> {
    Ok(exact_boss_relic_policy_decision_v1(session, legal)?.prior)
}

pub fn exact_boss_relic_policy_audit_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactBossRelicPolicyAuditV1, String> {
    exact_boss_relic_policy_decision_v1(session, legal)?.audit(legal)
}

pub fn exact_boss_relic_policy_decision_v1(
    session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<ExactBossRelicPolicyDecisionV1, String> {
    if !matches!(session.engine_state, EngineState::BossRelicSelect(_)) {
        return Err("exact boss relic policy requires a BossRelicSelect boundary".to_string());
    }
    let exact = exact_run_policy_decision_v1(session)?;
    let owned = owned_run_policy_actions_v1(&exact, legal, "boss relic", is_boss_relic_key)?;
    let context = boss_relic_policy_context_v1(session, &exact);
    let mut evidence = owned
        .into_iter()
        .map(|(surface_index, successor)| {
            boss_relic_action_evidence_v1(session, successor, surface_index, &context)
        })
        .collect::<Result<Vec<_>, _>>()?;
    evidence.sort_by(compare_boss_relic_evidence);
    let prior = positive_ranked_run_policy_prior_v1(
        legal,
        evidence
            .iter()
            .map(|candidate| candidate.candidate_id.clone()),
    )?;
    Ok(ExactBossRelicPolicyDecisionV1 {
        exact,
        context: context.audit,
        evidence,
        prior,
    })
}

fn is_boss_relic_key(key: &DecisionCandidateKey) -> bool {
    matches!(
        key,
        DecisionCandidateKey::BossRelicPick { .. } | DecisionCandidateKey::BossRelicSkip
    )
}

fn boss_relic_policy_context_v1(
    parent: &RunControlSession,
    exact: &ExactRunPolicyDecisionV1,
) -> BossRelicPolicyContextV1 {
    let debt = exact
        .actions
        .iter()
        .filter_map(|action| match action.candidate_key {
            Some(DecisionCandidateKey::BossRelicPick { relic, .. }) => Some((
                relic,
                run_debt_projection_for_relic_v1(&parent.run_state, relic),
            )),
            _ => None,
        })
        .collect();
    let startup_before = deck_startup_profile_v1(&parent.run_state);
    let deck_plan = DeckPlanSnapshot::from_run_state(&parent.run_state);
    let upgrade_plan = plan_upgrades_v1(&parent.run_state);
    let important_upgrade_candidates = upgrade_plan
        .candidates
        .iter()
        .filter(|candidate| candidate.urgency >= UpgradeDebtSeverityV1::ImportantBeforeBoss)
        .count();
    let cost_reduction_upgrade_candidates = upgrade_plan
        .candidates
        .iter()
        .filter(|candidate| candidate.mechanical_delta.cost_delta > 0)
        .count();
    let x_cost_cards = parent
        .run_state
        .master_deck
        .iter()
        .filter(|card| card_analysis_profile_v1(card.id, card.upgrades).cost < 0)
        .count();
    let energy_master = 3_u8.saturating_add(
        parent
            .run_state
            .relics
            .iter()
            .map(|relic| energy_master_delta(relic.id))
            .sum(),
    );
    let energy_pressure = boss_relic_energy_pressure_v1(
        energy_master,
        deck_plan.deck_size,
        startup_before.high_cost_card_count,
        x_cost_cards,
        deck_plan.strategic_deficit.energy_or_playability,
    );
    let audit = BossRelicPolicyAuditContextV1 {
        energy_master,
        energy_pressure,
        deck_size: deck_plan.deck_size,
        deck_access: deck_plan.strategic_deficit.deck_access,
        energy_or_playability: deck_plan.strategic_deficit.energy_or_playability,
        deck_burden: deck_plan.strategic_deficit.deck_burden,
        zero_cost_cards: startup_before.zero_cost_card_count,
        low_cost_cards: startup_before.low_cost_card_count,
        high_cost_cards: startup_before.high_cost_card_count,
        x_cost_cards,
        self_damage_sources: startup_before.self_damage_source_count,
        upgrade_candidates: upgrade_plan.candidates.len(),
        important_upgrade_candidates,
        cost_reduction_upgrade_candidates,
        unpaid_core_upgrade_debts: upgrade_plan.debt_ledger.unpaid_core_count,
        upgrade_slot_pressure: match upgrade_plan.debt_ledger.upgrade_slots_pressure {
            UpgradeSlotPressureV1::Low => "low",
            UpgradeSlotPressureV1::Medium => "medium",
            UpgradeSlotPressureV1::High => "high",
        }
        .to_string(),
        curse_mitigation_or_synergy: has_curse_mitigation_or_synergy_v1(&parent.run_state),
        potion_slots: parent.run_state.potions.len(),
        potion_count: parent
            .run_state
            .potions
            .iter()
            .filter(|potion| potion.is_some())
            .count(),
    };
    BossRelicPolicyContextV1 {
        startup_before,
        action_supply_before: action_supply_profile_v1(&parent.run_state),
        debt,
        audit,
    }
}

fn boss_relic_energy_pressure_v1(
    energy_master: u8,
    deck_size: usize,
    high_cost_cards: u8,
    x_cost_cards: usize,
    strategic_deficit: StrategicDeficitLevel,
) -> BossRelicEnergyPressureV1 {
    if energy_master >= 4
        || matches!(
            strategic_deficit,
            StrategicDeficitLevel::Adequate | StrategicDeficitLevel::Surplus
        )
    {
        return BossRelicEnergyPressureV1::Covered;
    }
    let concentrated_high_cost =
        high_cost_cards >= 4 && usize::from(high_cost_cards).saturating_mul(5) >= deck_size.max(1);
    if matches!(
        strategic_deficit,
        StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
    ) && (concentrated_high_cost || x_cost_cards >= 2)
    {
        return BossRelicEnergyPressureV1::High;
    }
    BossRelicEnergyPressureV1::Thin
}

fn boss_relic_action_evidence_v1(
    parent: &RunControlSession,
    successor: &ExactRunPolicyActionSuccessorV1,
    surface_index: usize,
    context: &BossRelicPolicyContextV1,
) -> Result<BossRelicPolicyActionEvidenceV1, String> {
    let candidate_key = successor.candidate_key.clone().ok_or_else(|| {
        format!(
            "boss relic candidate '{}' has no typed key",
            successor.candidate_id
        )
    })?;
    let action = match candidate_key {
        DecisionCandidateKey::BossRelicPick { relic, .. } => {
            let child = &successor.exact.session;
            let debt = context
                .debt
                .iter()
                .find(|(candidate, _)| *candidate == relic)
                .map(|(_, projection)| projection)
                .expect("boss relic context covers every exact relic candidate");
            let startup_after = deck_startup_profile_v1(&child.run_state);
            let supply_after = action_supply_profile_v1(&child.run_state);
            let generated_relics = pending_reward_relics_v1(child);
            let mut acquired_relics = vec![relic];
            acquired_relics.extend(generated_relics.iter().copied());
            let strategic_relic_count = acquired_relics
                .iter()
                .filter(|relic| !relic_acquisition_traits_v1(**relic).is_empty())
                .count();
            let mut traits = Vec::new();
            for acquired_relic in &acquired_relics {
                for acquisition_trait in relic_acquisition_traits_v1(*acquired_relic) {
                    if !traits.contains(&acquisition_trait) {
                        traits.push(acquisition_trait);
                    }
                }
            }
            BossRelicPolicyActionV1::Pick {
                relic,
                energy_gain: energy_master_delta(relic),
                replaces_existing_relic: child.run_state.relics.len()
                    <= parent.run_state.relics.len(),
                changed_deck_cards: changed_deck_cards(
                    &parent.run_state.master_deck,
                    &child.run_state.master_deck,
                ),
                followup: followup_v1(parent, child),
                generated_relics,
                strategic_relic_count,
                traits,
                added_debts: debt
                    .added_contracts
                    .iter()
                    .map(|contract| contract.kind)
                    .collect(),
                unresolved_debt_terms: debt
                    .added_contracts
                    .iter()
                    .map(|contract| contract.unresolved.len())
                    .sum(),
                compounding_debt_count: debt.compounding_tags.len(),
                startup: startup_delta_v1(&context.startup_before, &startup_after),
                action_supply: action_supply_delta_v1(&context.action_supply_before, &supply_after),
            }
        }
        DecisionCandidateKey::BossRelicSkip => BossRelicPolicyActionV1::Skip,
        ref other => {
            return Err(format!(
                "exact boss relic policy received non-boss-relic candidate key {other:?}"
            ))
        }
    };
    let band = boss_relic_policy_band_v1(&action, &context.audit);
    Ok(BossRelicPolicyActionEvidenceV1 {
        candidate_id: successor.candidate_id.clone(),
        candidate_key,
        action,
        band,
        surface_index,
    })
}

fn pending_reward_relics_v1(session: &RunControlSession) -> Vec<RelicId> {
    let reward = match &session.engine_state {
        EngineState::RewardScreen(reward) => Some(reward),
        EngineState::RewardOverlay { reward_state, .. } => Some(reward_state),
        _ => session.run_state.reward_state.as_ref(),
    };
    reward
        .into_iter()
        .flat_map(|reward| reward.items.iter())
        .filter_map(|item| match item {
            RewardItem::Relic { relic_id } => Some(*relic_id),
            _ => None,
        })
        .collect()
}

fn followup_v1(parent: &RunControlSession, child: &RunControlSession) -> BossRelicPolicyFollowupV1 {
    match &child.engine_state {
        EngineState::RunPendingChoice(choice) => {
            BossRelicPolicyFollowupV1::Selection(choice.reason)
        }
        EngineState::RewardScreen(_) | EngineState::RewardOverlay { .. } => {
            BossRelicPolicyFollowupV1::Reward
        }
        EngineState::MapNavigation if child.run_state.act_num > parent.run_state.act_num => {
            BossRelicPolicyFollowupV1::ActAdvanced
        }
        _ => BossRelicPolicyFollowupV1::Other,
    }
}

fn changed_deck_cards(
    before: &[crate::runtime::combat::CombatCard],
    after: &[crate::runtime::combat::CombatCard],
) -> usize {
    let before_by_lineage = before
        .iter()
        .map(|card| (card.lineage, (card.id, card.upgrades)))
        .collect::<BTreeMap<_, _>>();
    let after_by_lineage = after
        .iter()
        .map(|card| (card.lineage, (card.id, card.upgrades)))
        .collect::<BTreeMap<_, _>>();
    before_by_lineage
        .iter()
        .filter(|(uuid, card)| after_by_lineage.get(uuid) != Some(card))
        .count()
        .saturating_add(
            after_by_lineage
                .keys()
                .filter(|uuid| !before_by_lineage.contains_key(uuid))
                .count(),
        )
}

fn startup_delta_v1(
    before: &DeckStartupProfileV1,
    after: &DeckStartupProfileV1,
) -> BossRelicStartupDeltaV1 {
    BossRelicStartupDeltaV1 {
        setup_debt: delta_u8(after.setup_debt, before.setup_debt),
        setup_payment: delta_u8(
            after.effective_setup_payment,
            before.effective_setup_payment,
        ),
        immediate_survival: delta_u8(after.immediate_survival, before.immediate_survival),
        payoff_engine: delta_u8(after.payoff_engine, before.payoff_engine),
        combat_shape_risk: delta_u8(after.combat_shape_risk, before.combat_shape_risk),
        strong_draw: delta_u8(
            after.effective_strong_draw_count,
            before.effective_strong_draw_count,
        ),
        snecko_draw_bonus: delta_u8(after.snecko_draw_bonus, before.snecko_draw_bonus),
        snecko_random_cost_debt: delta_u8(
            after.snecko_random_cost_debt,
            before.snecko_random_cost_debt,
        ),
        snecko_high_cost_payoff: delta_u8(
            after.snecko_high_cost_payoff,
            before.snecko_high_cost_payoff,
        ),
        snecko_pellets_cleanup_access: delta_u8(
            after.snecko_pellets_cleanup_access,
            before.snecko_pellets_cleanup_access,
        ),
    }
}

fn delta_u8(after: u8, before: u8) -> i16 {
    i16::from(after) - i16::from(before)
}

fn boss_relic_policy_band_v1(
    action: &BossRelicPolicyActionV1,
    context: &BossRelicPolicyAuditContextV1,
) -> BossRelicPolicyBandV1 {
    let BossRelicPolicyActionV1::Pick {
        energy_gain,
        replaces_existing_relic,
        changed_deck_cards,
        followup,
        traits,
        added_debts,
        unresolved_debt_terms,
        compounding_debt_count,
        startup,
        action_supply,
        ..
    } = action
    else {
        return BossRelicPolicyBandV1::PreserveState;
    };

    if *energy_gain > 0
        && (added_debts.is_empty()
            || smith_lock_is_supported_energy_constraint_v1(added_debts, context))
        && *unresolved_debt_terms == 0
        && *compounding_debt_count == 0
    {
        return BossRelicPolicyBandV1::SupportedEnergy;
    }
    if (*changed_deck_cards > 0 && traits.contains(&RelicAcquisitionTraitV1::DeckMutation))
        || matches!(
            followup,
            BossRelicPolicyFollowupV1::Selection(
                RunPendingChoiceReason::Purge | RunPendingChoiceReason::TransformUpgraded
            )
        )
    {
        return BossRelicPolicyBandV1::DeckReconstruction;
    }
    if strategic_asset_debts_supported_v1(added_debts, context)
        && *unresolved_debt_terms == 0
        && *compounding_debt_count == 0
        && (startup.setup_payment > 0
            || startup.immediate_survival > 0
            || startup.payoff_engine > 0
            || startup.strong_draw > 0
            || action_supply.has_positive_supply()
            || grounded_strategic_trait_v1(traits, context))
    {
        return BossRelicPolicyBandV1::StrategicAsset;
    }
    if repairs_strategic_gap_v1(action, context)
        && *unresolved_debt_terms == 0
        && *compounding_debt_count == 0
    {
        return BossRelicPolicyBandV1::StrategicRepair;
    }
    if *replaces_existing_relic {
        return BossRelicPolicyBandV1::StarterUpgrade;
    }
    if *energy_gain > 0 {
        return BossRelicPolicyBandV1::ConstrainedEnergy;
    }
    if matches!(followup, BossRelicPolicyFollowupV1::Reward) {
        return BossRelicPolicyBandV1::ConditionalGrowth;
    }
    if !added_debts.is_empty() || startup.combat_shape_risk > 0 || startup.setup_debt > 0 {
        return BossRelicPolicyBandV1::Liability;
    }
    BossRelicPolicyBandV1::ConditionalGrowth
}

fn grounded_strategic_trait_v1(
    traits: &[RelicAcquisitionTraitV1],
    context: &BossRelicPolicyAuditContextV1,
) -> bool {
    traits.iter().any(|trait_| match trait_ {
        RelicAcquisitionTraitV1::PotionEffectMultiplier => context.potion_count > 0,
        _ => true,
    })
}

fn repairs_strategic_gap_v1(
    action: &BossRelicPolicyActionV1,
    context: &BossRelicPolicyAuditContextV1,
) -> bool {
    let BossRelicPolicyActionV1::Pick {
        energy_gain,
        traits,
        added_debts,
        startup,
        ..
    } = action
    else {
        return false;
    };

    (*energy_gain > 0
        && added_debts.is_empty()
        && context.energy_pressure == BossRelicEnergyPressureV1::High)
        || (added_debts.as_slice() == [RunDebtContractKindV1::RandomCostDeckShapeDebt]
            && traits.contains(&RelicAcquisitionTraitV1::CoreCardAccess)
            && matches!(
                context.deck_access,
                StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
            )
            && startup.snecko_draw_bonus > 0
            && startup.snecko_high_cost_payoff >= 4
            && startup.snecko_pellets_cleanup_access > 0
            && startup.snecko_random_cost_debt <= 1)
}

fn strategic_asset_debts_supported_v1(
    added_debts: &[RunDebtContractKindV1],
    context: &BossRelicPolicyAuditContextV1,
) -> bool {
    added_debts.is_empty()
        || added_debts.iter().all(|debt| {
            *debt == RunDebtContractKindV1::CurseDebt && context.curse_mitigation_or_synergy
        })
}

fn smith_lock_is_supported_energy_constraint_v1(
    added_debts: &[RunDebtContractKindV1],
    context: &BossRelicPolicyAuditContextV1,
) -> bool {
    !added_debts.is_empty()
        && added_debts
            .iter()
            .all(|debt| *debt == RunDebtContractKindV1::SmithLock)
        && context.energy_master <= 3
        && (context.high_cost_cards > 0 || context.x_cost_cards > 0)
        && context.important_upgrade_candidates == 0
        && context.unpaid_core_upgrade_debts == 0
        && context.upgrade_slot_pressure == "low"
}

fn compare_boss_relic_evidence(
    left: &BossRelicPolicyActionEvidenceV1,
    right: &BossRelicPolicyActionEvidenceV1,
) -> Ordering {
    left.band
        .cmp(&right.band)
        .then_with(|| compare_action_evidence(&left.action, &right.action))
        .then_with(|| left.surface_index.cmp(&right.surface_index))
}

fn compare_action_evidence(
    left: &BossRelicPolicyActionV1,
    right: &BossRelicPolicyActionV1,
) -> Ordering {
    match (left, right) {
        (
            BossRelicPolicyActionV1::Pick {
                energy_gain: left_energy,
                unresolved_debt_terms: left_unresolved,
                compounding_debt_count: left_compounding,
                strategic_relic_count: left_strategic_relics,
                startup: left_startup,
                ..
            },
            BossRelicPolicyActionV1::Pick {
                energy_gain: right_energy,
                unresolved_debt_terms: right_unresolved,
                compounding_debt_count: right_compounding,
                strategic_relic_count: right_strategic_relics,
                startup: right_startup,
                ..
            },
        ) => left_compounding
            .cmp(right_compounding)
            .then_with(|| left_unresolved.cmp(right_unresolved))
            .then_with(|| right_energy.cmp(left_energy))
            .then_with(|| right_strategic_relics.cmp(left_strategic_relics))
            .then_with(|| right_startup.setup_payment.cmp(&left_startup.setup_payment))
            .then_with(|| {
                right_startup
                    .immediate_survival
                    .cmp(&left_startup.immediate_survival)
            })
            .then_with(|| {
                left_startup
                    .combat_shape_risk
                    .cmp(&right_startup.combat_shape_risk)
            }),
        _ => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::potions::{Potion, PotionId};
    use crate::eval::run_control::{build_decision_surface, RunControlConfig};
    use crate::runtime::combat::CombatCard;
    use crate::state::rewards::BossRelicChoiceState;

    fn boss_relic_session(relics: Vec<RelicId>) -> RunControlSession {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = EngineState::BossRelicSelect(BossRelicChoiceState::new(relics));
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

    #[test]
    fn every_exact_boss_relic_action_including_skip_has_positive_support() {
        let session = boss_relic_session(vec![
            RelicId::CoffeeDripper,
            RelicId::RunicPyramid,
            RelicId::PandorasBox,
        ]);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_boss_relic_policy_decision_v1(&session, &legal)
            .expect("exact boss relic decision");

        assert_eq!(decision.evidence.len(), legal.len());
        assert_eq!(decision.prior.entries.len(), legal.len());
        assert!(decision
            .prior
            .entries
            .iter()
            .all(|entry| entry.probability.is_finite() && entry.probability > 0.0));
        assert!(decision
            .evidence
            .iter()
            .any(|entry| matches!(entry.action, BossRelicPolicyActionV1::Skip)));
    }

    #[test]
    fn auxiliary_potion_actions_stay_legal_without_entering_boss_relic_evidence() {
        let mut session = boss_relic_session(vec![RelicId::FusionHammer, RelicId::RunicCube]);
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::ColorlessPotion, 10)),
            Some(Potion::new(PotionId::LiquidMemories, 11)),
            Some(Potion::new(PotionId::Elixir, 12)),
        ];
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_boss_relic_policy_decision_v1(&session, &legal)
            .expect("boss relic owner should ignore auxiliary potion parsing");

        assert_eq!(decision.evidence.len(), 3, "two relics plus skip are owned");
        assert_eq!(decision.prior.entries.len(), legal.len());
        assert!(legal.len() > decision.evidence.len());
        assert!(decision
            .prior
            .entries
            .iter()
            .all(|entry| entry.probability.is_finite() && entry.probability > 0.0));
        assert!(decision.evidence.iter().all(|entry| {
            matches!(
                entry.candidate_key,
                DecisionCandidateKey::BossRelicPick { .. } | DecisionCandidateKey::BossRelicSkip
            )
        }));

        let audit = decision.audit(&legal).expect("typed boss relic audit");
        assert_eq!(audit.candidates.len(), decision.evidence.len());
        assert!(audit.candidates.iter().all(|candidate| {
            candidate.prior_probability.is_finite() && candidate.prior_probability > 0.0
        }));
        assert!(audit.candidates.iter().any(|candidate| {
            candidate.relic == Some(RelicId::FusionHammer)
                && candidate.energy_gain == 1
                && candidate.added_debts.iter().any(|debt| debt == "SmithLock")
        }));
        assert!(audit.candidates.iter().any(|candidate| matches!(
            candidate.candidate_key,
            DecisionCandidateKey::BossRelicSkip
        ) && candidate.relic.is_none()));
    }

    #[test]
    fn nested_relic_effects_remain_real_followup_boundaries() {
        let session = boss_relic_session(vec![RelicId::Astrolabe, RelicId::EmptyCage]);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_boss_relic_policy_decision_v1(&session, &legal)
            .expect("nested boss relic decision");

        assert!(decision.evidence.iter().any(|entry| matches!(
            entry.action,
            BossRelicPolicyActionV1::Pick {
                relic: RelicId::Astrolabe,
                followup: BossRelicPolicyFollowupV1::Selection(
                    RunPendingChoiceReason::TransformUpgraded
                ),
                ..
            }
        )));
        assert!(decision.evidence.iter().any(|entry| matches!(
            entry.action,
            BossRelicPolicyActionV1::Pick {
                relic: RelicId::EmptyCage,
                followup: BossRelicPolicyFollowupV1::Selection(RunPendingChoiceReason::Purge),
                ..
            }
        )));
    }

    #[test]
    fn pandora_uses_the_exact_transformed_deck_not_a_starter_count_prediction() {
        let mut session = boss_relic_session(vec![RelicId::PandorasBox]);
        session.run_state.master_deck = [
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, 20_000 + index as u32))
        .collect();
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_boss_relic_policy_decision_v1(&session, &legal).expect("Pandora decision");
        let pandora = decision
            .evidence
            .iter()
            .find(|entry| {
                matches!(
                    entry.action,
                    BossRelicPolicyActionV1::Pick {
                        relic: RelicId::PandorasBox,
                        ..
                    }
                )
            })
            .expect("Pandora evidence");

        assert!(matches!(
            pandora.action,
            BossRelicPolicyActionV1::Pick {
                changed_deck_cards: 4..,
                ..
            }
        ));
        assert_eq!(pandora.band, BossRelicPolicyBandV1::DeckReconstruction);
    }

    #[test]
    fn adding_calling_bell_curse_is_not_mistaken_for_deck_reconstruction() {
        let session = boss_relic_session(vec![RelicId::CallingBell, RelicId::RunicCube]);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_boss_relic_policy_decision_v1(&session, &legal)
            .expect("Calling Bell boss relic decision");
        let calling_bell = decision
            .evidence
            .iter()
            .find(|entry| {
                matches!(
                    entry.action,
                    BossRelicPolicyActionV1::Pick {
                        relic: RelicId::CallingBell,
                        ..
                    }
                )
            })
            .expect("Calling Bell evidence");

        assert!(matches!(
            calling_bell.action,
            BossRelicPolicyActionV1::Pick {
                changed_deck_cards: 1,
                ref added_debts,
                ref traits,
                ..
            } if added_debts.contains(&RunDebtContractKindV1::CurseDebt)
                && !traits.contains(&RelicAcquisitionTraitV1::DeckMutation)
        ));
        assert_eq!(calling_bell.band, BossRelicPolicyBandV1::ConditionalGrowth);
    }

    #[test]
    fn mitigated_calling_bell_ranks_its_exact_relic_package() {
        let mut session = boss_relic_session(vec![
            RelicId::RunicCube,
            RelicId::CallingBell,
            RelicId::SacredBark,
        ]);
        session
            .run_state
            .relics
            .push(crate::content::relics::RelicState::new(RelicId::BlueCandle));
        session.run_state.common_relic_pool = vec![RelicId::RedSkull];
        session.run_state.uncommon_relic_pool = vec![RelicId::HornCleat];
        session.run_state.rare_relic_pool = vec![RelicId::Calipers];

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_boss_relic_policy_decision_v1(&session, &legal)
            .expect("Calling Bell package decision");
        let calling_bell = decision
            .evidence
            .iter()
            .find(|entry| {
                matches!(
                    entry.action,
                    BossRelicPolicyActionV1::Pick {
                        relic: RelicId::CallingBell,
                        ..
                    }
                )
            })
            .expect("Calling Bell evidence");

        assert!(matches!(
            &calling_bell.action,
            BossRelicPolicyActionV1::Pick {
                generated_relics,
                strategic_relic_count: 2,
                traits,
                unresolved_debt_terms: 0,
                ..
            } if generated_relics
                == &[RelicId::RedSkull, RelicId::HornCleat, RelicId::Calipers]
                && traits.contains(&RelicAcquisitionTraitV1::CoreDefenseOrSurvival)
        ));
        assert_eq!(calling_bell.band, BossRelicPolicyBandV1::StrategicAsset);
        assert!(matches!(
            decision.evidence[0].action,
            BossRelicPolicyActionV1::Pick {
                relic: RelicId::CallingBell,
                ..
            }
        ));
    }

    #[test]
    fn energy_constraints_are_typed_without_removing_the_action() {
        let session = boss_relic_session(vec![RelicId::CoffeeDripper, RelicId::FusionHammer]);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision =
            exact_boss_relic_policy_decision_v1(&session, &legal).expect("energy relic decision");

        for relic in [RelicId::CoffeeDripper, RelicId::FusionHammer] {
            let evidence = decision
                .evidence
                .iter()
                .find(|entry| {
                    matches!(
                        entry.action,
                        BossRelicPolicyActionV1::Pick {
                            relic: found,
                            ..
                        } if found == relic
                    )
                })
                .expect("energy relic evidence");
            assert!(matches!(
                evidence.action,
                BossRelicPolicyActionV1::Pick { energy_gain: 1, .. }
            ));
        }
        assert_eq!(decision.prior.entries.len(), 3);
    }

    #[test]
    fn smith_lock_energy_is_supported_only_when_upgrade_debt_is_low() {
        let context = BossRelicPolicyAuditContextV1 {
            energy_master: 3,
            energy_pressure: BossRelicEnergyPressureV1::High,
            deck_size: 18,
            deck_access: StrategicDeficitLevel::Thin,
            energy_or_playability: StrategicDeficitLevel::Thin,
            deck_burden: StrategicBurdenLevel::Watch,
            zero_cost_cards: 1,
            low_cost_cards: 10,
            high_cost_cards: 4,
            x_cost_cards: 1,
            self_damage_sources: 1,
            upgrade_candidates: 12,
            important_upgrade_candidates: 0,
            cost_reduction_upgrade_candidates: 1,
            unpaid_core_upgrade_debts: 0,
            upgrade_slot_pressure: "low".to_string(),
            curse_mitigation_or_synergy: false,
            potion_slots: 2,
            potion_count: 0,
        };
        let action = BossRelicPolicyActionV1::Pick {
            relic: RelicId::FusionHammer,
            energy_gain: 1,
            replaces_existing_relic: false,
            changed_deck_cards: 0,
            followup: BossRelicPolicyFollowupV1::ActAdvanced,
            generated_relics: Vec::new(),
            strategic_relic_count: 0,
            traits: Vec::new(),
            added_debts: vec![RunDebtContractKindV1::SmithLock],
            unresolved_debt_terms: 0,
            compounding_debt_count: 0,
            startup: BossRelicStartupDeltaV1::default(),
            action_supply: BossRelicActionSupplyDeltaV1::default(),
        };

        assert_eq!(
            boss_relic_policy_band_v1(&action, &context),
            BossRelicPolicyBandV1::SupportedEnergy
        );

        let important_upgrade_context = BossRelicPolicyAuditContextV1 {
            important_upgrade_candidates: 1,
            unpaid_core_upgrade_debts: 1,
            upgrade_slot_pressure: "high".to_string(),
            ..context
        };
        assert_eq!(
            boss_relic_policy_band_v1(&action, &important_upgrade_context),
            BossRelicPolicyBandV1::ConstrainedEnergy
        );
    }

    #[test]
    fn typed_energy_debt_is_not_mistaken_for_debt_free_energy() {
        let session = boss_relic_session(vec![
            RelicId::BustedCrown,
            RelicId::Astrolabe,
            RelicId::RunicPyramid,
        ]);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_boss_relic_policy_decision_v1(&session, &legal)
            .expect("boss relic decision with reward-width debt");
        let busted_crown = decision
            .evidence
            .iter()
            .find(|entry| {
                matches!(
                    entry.action,
                    BossRelicPolicyActionV1::Pick {
                        relic: RelicId::BustedCrown,
                        ..
                    }
                )
            })
            .expect("Busted Crown evidence");

        assert!(matches!(
            busted_crown.action,
            BossRelicPolicyActionV1::Pick {
                ref added_debts,
                ..
            } if added_debts.contains(&RunDebtContractKindV1::RewardWidthDebt)
        ));
        assert_eq!(busted_crown.band, BossRelicPolicyBandV1::ConstrainedEnergy);

        let rank = |relic| {
            decision
                .prior
                .entries
                .iter()
                .position(|entry| {
                    decision.evidence.iter().any(|evidence| {
                        evidence.candidate_id == entry.candidate_id
                            && matches!(
                                evidence.action,
                                BossRelicPolicyActionV1::Pick { relic: found, .. }
                                    if found == relic
                            )
                    })
                })
                .expect("relic remains present in the positive prior")
        };
        assert!(rank(RelicId::Astrolabe) < rank(RelicId::BustedCrown));
        assert!(rank(RelicId::RunicPyramid) < rank(RelicId::BustedCrown));
    }

    #[test]
    fn pellets_backed_snecko_repairs_access_without_promoting_empty_sozu_or_bark() {
        let mut session =
            boss_relic_session(vec![RelicId::SacredBark, RelicId::SneckoEye, RelicId::Sozu]);
        session.run_state.act_num = 2;
        session.run_state.potions = vec![None, None];
        session.run_state.relics = [
            RelicId::BurningBlood,
            RelicId::SingingBowl,
            RelicId::BloodVial,
            RelicId::PandorasBox,
            RelicId::OrangePellets,
            RelicId::Nunchaku,
            RelicId::EternalFeather,
            RelicId::TinyChest,
        ]
        .into_iter()
        .map(crate::content::relics::RelicState::new)
        .collect();
        session.run_state.master_deck = [
            (CardId::Armaments, 0),
            (CardId::AscendersBane, 0),
            (CardId::Bash, 1),
            (CardId::Clothesline, 0),
            (CardId::DoubleTap, 0),
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
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (card, upgrades))| {
            let mut card = CombatCard::for_scenario(card, 30_000 + index as u32);
            card.upgrades = upgrades;
            card
        })
        .collect();

        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_boss_relic_policy_decision_v1(&session, &legal)
            .expect("high-pressure Act 3 boss relic decision");

        assert_eq!(decision.context.energy_master, 3);
        assert_eq!(
            decision.context.energy_pressure,
            BossRelicEnergyPressureV1::High
        );
        assert_eq!(decision.context.potion_count, 0);
        assert_eq!(decision.context.potion_slots, 2);

        let evidence_for = |relic| {
            decision
                .evidence
                .iter()
                .find(|candidate| {
                    matches!(
                        candidate.action,
                        BossRelicPolicyActionV1::Pick { relic: found, .. } if found == relic
                    )
                })
                .expect("requested boss relic evidence")
        };
        assert_eq!(
            evidence_for(RelicId::SacredBark).band,
            BossRelicPolicyBandV1::ConditionalGrowth
        );
        assert_eq!(
            evidence_for(RelicId::Sozu).band,
            BossRelicPolicyBandV1::ConstrainedEnergy
        );
        assert_eq!(
            evidence_for(RelicId::SneckoEye).band,
            BossRelicPolicyBandV1::StrategicRepair
        );
        assert!(matches!(
            evidence_for(RelicId::SneckoEye).action,
            BossRelicPolicyActionV1::Pick {
                startup: BossRelicStartupDeltaV1 {
                    snecko_draw_bonus: 2,
                    snecko_random_cost_debt: 1,
                    snecko_high_cost_payoff: 4..,
                    snecko_pellets_cleanup_access: 1,
                    ..
                },
                ..
            }
        ));
        let snecko_rank = decision
            .evidence
            .iter()
            .position(|candidate| {
                candidate.candidate_id == evidence_for(RelicId::SneckoEye).candidate_id
            })
            .expect("Snecko Eye rank");
        let sozu_rank = decision
            .evidence
            .iter()
            .position(|candidate| {
                candidate.candidate_id == evidence_for(RelicId::Sozu).candidate_id
            })
            .expect("Sozu rank");
        let sacred_bark_rank = decision
            .evidence
            .iter()
            .position(|candidate| {
                candidate.candidate_id == evidence_for(RelicId::SacredBark).candidate_id
            })
            .expect("Sacred Bark rank");
        let skip_rank = decision
            .evidence
            .iter()
            .position(|candidate| matches!(candidate.action, BossRelicPolicyActionV1::Skip))
            .expect("skip rank");
        assert!(snecko_rank < sacred_bark_rank);
        assert!(snecko_rank < skip_rank);
        assert!(skip_rank < sozu_rank);

        session
            .run_state
            .relics
            .retain(|relic| relic.id != RelicId::OrangePellets);
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let without_pellets = exact_boss_relic_policy_decision_v1(&session, &legal)
            .expect("same high-pressure deck without Pellets");
        let snecko = without_pellets
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.action,
                    BossRelicPolicyActionV1::Pick {
                        relic: RelicId::SneckoEye,
                        ..
                    }
                )
            })
            .expect("Snecko remains a legal candidate");
        assert_eq!(snecko.band, BossRelicPolicyBandV1::Liability);
        assert!(matches!(
            snecko.action,
            BossRelicPolicyActionV1::Pick {
                startup: BossRelicStartupDeltaV1 {
                    snecko_pellets_cleanup_access: 0,
                    snecko_random_cost_debt: 2,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn energy_pressure_requires_real_cost_concentration_and_respects_four_energy() {
        assert_eq!(
            boss_relic_energy_pressure_v1(3, 24, 3, 0, StrategicDeficitLevel::Missing),
            BossRelicEnergyPressureV1::Thin
        );
        assert_eq!(
            boss_relic_energy_pressure_v1(3, 24, 6, 0, StrategicDeficitLevel::Missing),
            BossRelicEnergyPressureV1::High
        );
        assert_eq!(
            boss_relic_energy_pressure_v1(3, 30, 4, 0, StrategicDeficitLevel::Thin),
            BossRelicEnergyPressureV1::Thin
        );
        assert_eq!(
            boss_relic_energy_pressure_v1(4, 12, 8, 2, StrategicDeficitLevel::Missing),
            BossRelicEnergyPressureV1::Covered
        );
        assert_eq!(
            boss_relic_energy_pressure_v1(3, 12, 8, 2, StrategicDeficitLevel::Surplus),
            BossRelicEnergyPressureV1::Covered
        );
    }

    #[test]
    fn sacred_bark_is_grounded_when_a_potion_is_already_owned() {
        let mut session = boss_relic_session(vec![RelicId::SacredBark]);
        session.run_state.potions = vec![Some(Potion::new(PotionId::StrengthPotion, 90)), None];
        let surface = build_decision_surface(&session);
        let legal = policy_candidates(&surface);
        let decision = exact_boss_relic_policy_decision_v1(&session, &legal)
            .expect("owned-potion Sacred Bark decision");
        let sacred_bark = decision
            .evidence
            .iter()
            .find(|candidate| {
                matches!(
                    candidate.action,
                    BossRelicPolicyActionV1::Pick {
                        relic: RelicId::SacredBark,
                        ..
                    }
                )
            })
            .expect("Sacred Bark evidence");

        assert_eq!(decision.context.potion_count, 1);
        assert_eq!(sacred_bark.band, BossRelicPolicyBandV1::StrategicAsset);
    }
}
