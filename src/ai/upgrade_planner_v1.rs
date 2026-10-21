use crate::ai::block_plan_profile_v1::{block_plan_profile_v1, BlockPlanReadinessV1};
use crate::ai::boss_mechanics_v1::{
    boss_mechanic_pressure_profile_v1, BossMechanicMissingAnswerV1, BossMechanicPressurePointV1,
    BossMechanicRedFlagV1,
};
use crate::ai::card_analysis_v1::{
    card_analysis_profile_v1, CardAnalysisDeckSourceV1, CardAnalysisUpgradeRedundancyGroupV1,
    CardAnalysisUpgradeStackBehaviorV1,
};
use crate::ai::card_semantics_v1::{
    card_owned_upgrade_output_v1, card_reward_semantic_profile_v1, CardOwnedUpgradeOutputV1,
    CardRewardSemanticRoleV1,
};
use crate::ai::combat_upgrade_coverage_v1::{
    combat_upgrade_coverage_profile_v1, CombatUpgradeCoverageProfileV1, CombatUpgradeScopeV1,
};
use crate::ai::strategy::power_tempo::mummified_hand_power_tempo_v1;
use crate::ai::strength_profile_v1::strength_profile_v1;
use crate::content::cards::{get_card_definition, CardId};
use crate::content::monsters::factory::EncounterId;
use crate::content::relics::RelicId;
use crate::engine::campfire_handler::campfire_rest_heal_capacity;
use crate::runtime::combat::{CardLineageId, CombatCard};
use crate::state::run::RunState;

#[derive(Clone, Debug, PartialEq)]
pub struct UpgradePlanV1 {
    pub combat_upgrade_coverage: CombatUpgradeCoverageProfileV1,
    pub candidates: Vec<UpgradeCandidateV1>,
    pub debt_ledger: UpgradeDebtLedgerV1,
    pub rest_vs_smith: RestVsSmithPlanV1,
    pub best_smith: Option<usize>,
    pub best_smith_debt_paid: Option<UpgradeDebtKindV1>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpgradeDebtLedgerV1 {
    pub debts: Vec<UpgradeDebtV1>,
    pub unpaid_core_count: usize,
    pub upgrade_slots_pressure: UpgradeSlotPressureV1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpgradeDebtV1 {
    pub kind: UpgradeDebtKindV1,
    pub severity: UpgradeDebtSeverityV1,
    pub required_by: String,
    pub candidate_deck_indices: Vec<usize>,
    pub if_unpaid: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum UpgradeDebtKindV1 {
    ControlledExhaust,
    StasisRecovery,
    HyperbeamBlock,
    PhaseBurst,
    ExecuteBlock,
    ExhaustPayoffSetupTempo,
    PowerSetupTempo,
    AccessRecovery,
    ScalingSetup,
    DebuffCoverage,
    TransitionalFrontload,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UpgradeDebtSeverityV1 {
    Avoid,
    Defer,
    Opportunistic,
    UsefulSoon,
    ImportantBeforeBoss,
    CriticalBeforeBoss,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UpgradeSlotPressureV1 {
    #[default]
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpgradeCandidateV1 {
    pub deck_index: usize,
    pub card_lineage: CardLineageId,
    pub card: CardId,
    pub upgrades: u8,
    pub label: String,
    pub mechanical_delta: UpgradeMechanicalDeltaV1,
    pub combat_upgrade_class: CombatUpgradeCandidateClassV1,
    pub combat_upgrade_credit: CombatUpgradeCreditV1,
    pub roles: Vec<UpgradeRoleV1>,
    pub redundancy: RedundancyProfileV1,
    pub pays_debts: Vec<UpgradeDebtKindV1>,
    pub opportunity_costs: Vec<String>,
    pub urgency: UpgradeDebtSeverityV1,
    pub verdict: UpgradeVerdictV1,
    pub evidence: Vec<String>,
    pub risks: Vec<String>,
}

impl UpgradeCandidateV1 {
    pub fn summary_label(&self) -> String {
        format!(
            "{} verdict={:?} urgency={:?} roles=[{}] debts=[{}]",
            self.label,
            self.verdict,
            self.urgency,
            self.roles
                .iter()
                .map(|role| role.label())
                .collect::<Vec<_>>()
                .join(","),
            self.pays_debts
                .iter()
                .map(|debt| debt.label())
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpgradeMechanicalDeltaV1 {
    /// Scoped before/after parameters and delivery rules for selective inspection.
    pub output_change: Option<CardOwnedUpgradeOutputV1>,
    /// Positive means cheaper; negative means the actual upgrade raises cost.
    pub cost_delta: i32,
    /// Unconditional base contributions only; unresolved changes remain above.
    pub damage_delta: i32,
    pub block_delta: i32,
    pub magic_delta: i32,
    pub exhaust_control_delta: bool,
    pub exhaust_removed_delta: bool,
    pub ethereal_removed_delta: bool,
    pub innate_delta: bool,
    pub notes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatUpgradeCandidateClassV1 {
    OrdinaryStat,
    TimingSensitive,
    NoCombatCredit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatUpgradeCreditV1 {
    None,
    EvidenceOnly {
        scope: CombatUpgradeScopeV1,
    },
    PriorityReduced {
        scope: CombatUpgradeScopeV1,
        previous_urgency: UpgradeDebtSeverityV1,
        resulting_urgency: UpgradeDebtSeverityV1,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum UpgradeRoleV1 {
    CoreMechanic,
    EngineEnabler,
    Consistency,
    DefensiveSurvival,
    Scaling,
    PhaseBurst,
    DebuffCoverage,
    FrontloadDamage,
    TransitionalPower,
    UnresolvedOutputChange,
    LowMarginalRepeat,
    Speculative,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedundancyGroupV1 {
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
    Generic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StackBehaviorV1 {
    NonStackingOnce,
    StackableIntensity,
    DurationCoverage,
    RedundantAfterFirst,
    DensityPositive,
    DensityNegative,
    ComboThreshold,
    Generic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RedundancyProfileV1 {
    pub group: RedundancyGroupV1,
    pub stack_behavior: StackBehaviorV1,
    pub same_card_count: usize,
    pub existing_group_count: usize,
    pub saturated: bool,
    pub notes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UpgradeVerdictV1 {
    Avoid,
    Defer,
    Opportunistic,
    Useful,
    Important,
    CoreDebtPayment,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RestVsSmithPlanV1 {
    pub current_hp: i32,
    pub max_hp: i32,
    pub effective_rest_heal: i32,
    pub rest_heal_cap: i32,
    pub recovery_sources: Vec<String>,
    pub best_smith_debt_paid: Option<UpgradeDebtKindV1>,
    pub verdict: RestVsSmithVerdictV1,
    pub reasons: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestVsSmithVerdictV1 {
    RestFavored,
    SmithFavored,
    NeedsRouteRisk,
}

impl UpgradeDebtKindV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::ControlledExhaust => "controlled_exhaust",
            Self::StasisRecovery => "stasis_recovery",
            Self::HyperbeamBlock => "hyperbeam_block",
            Self::PhaseBurst => "phase_burst",
            Self::ExecuteBlock => "execute_block",
            Self::ExhaustPayoffSetupTempo => "exhaust_payoff_setup_tempo",
            Self::PowerSetupTempo => "power_setup_tempo",
            Self::AccessRecovery => "access_recovery",
            Self::ScalingSetup => "scaling_setup",
            Self::DebuffCoverage => "debuff_coverage",
            Self::TransitionalFrontload => "transitional_frontload",
        }
    }
}

impl UpgradeRoleV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::CoreMechanic => "core_mechanic",
            Self::EngineEnabler => "engine_enabler",
            Self::Consistency => "consistency",
            Self::DefensiveSurvival => "defensive_survival",
            Self::Scaling => "scaling",
            Self::PhaseBurst => "phase_burst",
            Self::DebuffCoverage => "debuff_coverage",
            Self::FrontloadDamage => "frontload_damage",
            Self::TransitionalPower => "transitional_power",
            Self::UnresolvedOutputChange => "unresolved_output_change",
            Self::LowMarginalRepeat => "low_marginal_repeat",
            Self::Speculative => "speculative",
        }
    }
}

pub fn plan_upgrades_v1(run_state: &RunState) -> UpgradePlanV1 {
    let combat_upgrade_coverage = combat_upgrade_coverage_profile_v1(run_state);
    let mut candidates = enumerate_upgrade_candidates(run_state);
    let debt_ledger = build_upgrade_debt_ledger(run_state, &candidates);
    for candidate in &mut candidates {
        apply_debt_ledger(candidate, &debt_ledger);
        apply_combat_upgrade_coverage(candidate, &combat_upgrade_coverage);
    }
    candidates.sort_by(compare_upgrade_candidates);
    let best_smith = candidates.first().map(|candidate| candidate.deck_index);
    let best_smith_debt_paid = best_smith_debt_paid(&candidates);
    let rest_vs_smith = rest_vs_smith_plan(run_state, best_smith_debt_paid);
    let mut notes = Vec::new();
    if candidates
        .iter()
        .any(|candidate| candidate.roles.contains(&UpgradeRoleV1::LowMarginalRepeat))
    {
        notes.push("upgrade planner detected low-marginal repeat upgrade targets".to_string());
    }
    if debt_ledger.unpaid_core_count > 0 {
        notes.push(format!(
            "upgrade planner has {} unpaid core upgrade debt(s)",
            debt_ledger.unpaid_core_count
        ));
    }

    UpgradePlanV1 {
        combat_upgrade_coverage,
        candidates,
        debt_ledger,
        rest_vs_smith,
        best_smith,
        best_smith_debt_paid,
        notes,
    }
}

pub fn upgrade_candidate_for_deck_index_v1(
    run_state: &RunState,
    deck_index: usize,
) -> Option<UpgradeCandidateV1> {
    plan_upgrades_v1(run_state)
        .candidates
        .into_iter()
        .find(|candidate| candidate.deck_index == deck_index)
}

pub fn upgrade_candidate_for_card_lineage_v1(
    run_state: &RunState,
    card_lineage: CardLineageId,
) -> Option<UpgradeCandidateV1> {
    plan_upgrades_v1(run_state)
        .candidates
        .into_iter()
        .find(|candidate| candidate.card_lineage == card_lineage)
}

pub fn upgrade_candidate_score_hint_v1(candidate: &UpgradeCandidateV1) -> i32 {
    let verdict_rank = match candidate.verdict {
        UpgradeVerdictV1::CoreDebtPayment => 1_200,
        UpgradeVerdictV1::Important => 1_000,
        UpgradeVerdictV1::Useful => 650,
        UpgradeVerdictV1::Opportunistic => 300,
        UpgradeVerdictV1::Defer => 80,
        UpgradeVerdictV1::Avoid => 0,
    };
    let urgency_rank = match candidate.urgency {
        UpgradeDebtSeverityV1::CriticalBeforeBoss => 300,
        UpgradeDebtSeverityV1::ImportantBeforeBoss => 220,
        UpgradeDebtSeverityV1::UsefulSoon => 120,
        UpgradeDebtSeverityV1::Opportunistic => 40,
        UpgradeDebtSeverityV1::Defer | UpgradeDebtSeverityV1::Avoid => 0,
    };
    verdict_rank + urgency_rank + candidate.pays_debts.len() as i32 * 25
}

pub fn upgrade_candidate_strategy_tag_v1(candidate: &UpgradeCandidateV1) -> Option<String> {
    candidate
        .pays_debts
        .iter()
        .max_by_key(|debt| upgrade_debt_strategy_tag_priority(**debt))
        .map(|debt| format!("upgrade_debt:{}", debt.label()))
        .or_else(|| {
            candidate
                .roles
                .first()
                .map(|role| format!("upgrade_role:{}", role.label()))
        })
}

fn upgrade_debt_strategy_tag_priority(debt: UpgradeDebtKindV1) -> u8 {
    match debt {
        UpgradeDebtKindV1::StasisRecovery => 95,
        UpgradeDebtKindV1::HyperbeamBlock => 92,
        UpgradeDebtKindV1::PhaseBurst => 90,
        UpgradeDebtKindV1::ExhaustPayoffSetupTempo => 88,
        UpgradeDebtKindV1::ExecuteBlock => 85,
        UpgradeDebtKindV1::ScalingSetup => 80,
        UpgradeDebtKindV1::DebuffCoverage => 75,
        UpgradeDebtKindV1::PowerSetupTempo => 72,
        UpgradeDebtKindV1::AccessRecovery => 70,
        UpgradeDebtKindV1::ControlledExhaust => 65,
        UpgradeDebtKindV1::TransitionalFrontload => 50,
    }
}

pub fn upgrade_plan_evidence_for_deck_index_v1(
    run_state: &RunState,
    deck_index: usize,
) -> Vec<String> {
    let plan = plan_upgrades_v1(run_state);
    let mut evidence = Vec::new();
    if let Some(candidate) = plan
        .candidates
        .iter()
        .find(|candidate| candidate.deck_index == deck_index)
    {
        evidence.push(format!("upgrade_plan: {}", candidate.summary_label()));
        evidence.push(format!(
            "combat_upgrade_coverage: class={:?} credit={:?}",
            candidate.combat_upgrade_class, candidate.combat_upgrade_credit
        ));
        evidence.push(format!(
            "upgrade_delta: cost={} damage={} block={} magic={} exhaust_control={} exhaust_removed={} ethereal_removed={}",
            candidate.mechanical_delta.cost_delta,
            candidate.mechanical_delta.damage_delta,
            candidate.mechanical_delta.block_delta,
            candidate.mechanical_delta.magic_delta,
            candidate.mechanical_delta.exhaust_control_delta,
            candidate.mechanical_delta.exhaust_removed_delta,
            candidate.mechanical_delta.ethereal_removed_delta
        ));
        evidence.push(format!(
            "upgrade_redundancy: group={:?} stack={:?} same_card_count={} existing_group_count={} saturated={}",
            candidate.redundancy.group,
            candidate.redundancy.stack_behavior,
            candidate.redundancy.same_card_count,
            candidate.redundancy.existing_group_count,
            candidate.redundancy.saturated
        ));
        evidence.extend(
            candidate
                .evidence
                .iter()
                .map(|item| format!("upgrade_evidence: {item}")),
        );
        evidence.extend(
            candidate
                .risks
                .iter()
                .map(|item| format!("upgrade_risk: {item}")),
        );
    }
    if plan.best_smith == Some(deck_index) {
        evidence.push("upgrade_plan: candidate is current best smith target".to_string());
    }
    evidence.push(format!(
        "rest_vs_smith: {:?} effective_heal={} best_smith_debt={}",
        plan.rest_vs_smith.verdict,
        plan.rest_vs_smith.effective_rest_heal,
        plan.best_smith_debt_paid
            .map(|debt| debt.label())
            .unwrap_or("-")
    ));
    evidence
}

fn enumerate_upgrade_candidates(run_state: &RunState) -> Vec<UpgradeCandidateV1> {
    run_state
        .master_deck
        .iter()
        .enumerate()
        .filter(|(_, card)| can_upgrade_for_planner(card))
        .map(|(deck_index, card)| build_upgrade_candidate(run_state, deck_index, card))
        .collect()
}

fn can_upgrade_for_planner(card: &CombatCard) -> bool {
    crate::content::cards::can_upgrade_card_once(card)
}

fn build_upgrade_candidate(
    run_state: &RunState,
    deck_index: usize,
    card: &CombatCard,
) -> UpgradeCandidateV1 {
    let mechanical_delta = mechanical_upgrade_delta(run_state, card);
    let redundancy = redundancy_profile(run_state, card.id);
    let mut roles = upgrade_roles(card, &mechanical_delta, &redundancy);
    let mut evidence = Vec::new();
    let mut risks = Vec::new();
    evidence.push(format!(
        "mechanical upgrade delta cost={} damage={} block={} magic={}",
        mechanical_delta.cost_delta,
        mechanical_delta.damage_delta,
        mechanical_delta.block_delta,
        mechanical_delta.magic_delta
    ));
    for note in &mechanical_delta.notes {
        evidence.push(note.clone());
    }
    for note in &redundancy.notes {
        evidence.push(note.clone());
    }
    if !mechanical_upgrade_has_potential_value(&mechanical_delta) {
        roles.clear();
        push_role(&mut roles, UpgradeRoleV1::Speculative);
        risks.push("upgrade has no observed benefit in the current run context".to_string());
    }
    if redundancy.saturated {
        push_role(&mut roles, UpgradeRoleV1::LowMarginalRepeat);
        risks.push("upgrade target appears to be a saturated repeated function".to_string());
    }
    if is_starter(card.id) {
        risks.push(
            "starter upgrade has low long-run priority unless no better debt exists".to_string(),
        );
    }
    apply_supported_payoff_upgrade_roles_v1(
        run_state,
        card,
        &mechanical_delta,
        &mut roles,
        &mut evidence,
    );

    let mut candidate = UpgradeCandidateV1 {
        deck_index,
        card_lineage: card.lineage,
        card: card.id,
        upgrades: card.upgrades,
        label: card_label(card.id, card.upgrades),
        mechanical_delta,
        combat_upgrade_class: CombatUpgradeCandidateClassV1::NoCombatCredit,
        combat_upgrade_credit: CombatUpgradeCreditV1::None,
        roles,
        redundancy,
        pays_debts: Vec::new(),
        opportunity_costs: Vec::new(),
        urgency: UpgradeDebtSeverityV1::Opportunistic,
        verdict: UpgradeVerdictV1::Opportunistic,
        evidence,
        risks,
    };
    apply_boss_specific_evidence(run_state, &mut candidate);
    candidate
}

fn apply_supported_payoff_upgrade_roles_v1(
    run_state: &RunState,
    card: &CombatCard,
    delta: &UpgradeMechanicalDeltaV1,
    roles: &mut Vec<UpgradeRoleV1>,
    evidence: &mut Vec<String>,
) {
    if card.id == CardId::BodySlam
        && delta.cost_delta > 0
        && block_plan_profile_v1(run_state).readiness >= BlockPlanReadinessV1::Supported
    {
        push_role(roles, UpgradeRoleV1::Consistency);
        evidence.push(
            "supported block plan makes Body Slam cost reduction a repeatable deployment upgrade"
                .to_string(),
        );
    }

    if card.id == CardId::HeavyBlade
        && delta.magic_delta > 0
        && strength_profile_v1(run_state).stable_sources > 0
    {
        push_role(roles, UpgradeRoleV1::Scaling);
        evidence.push(
            "stable strength source makes Heavy Blade multiplier growth a live scaling upgrade"
                .to_string(),
        );
    }
}

fn mechanical_upgrade_delta(run_state: &RunState, card: &CombatCard) -> UpgradeMechanicalDeltaV1 {
    let analysis = card_analysis_profile_v1(card.id, card.upgrades);
    let Some(output_change) = card_owned_upgrade_output_v1(card) else {
        return UpgradeMechanicalDeltaV1 {
            notes: vec!["no representable legal one-step upgrade".to_string()],
            ..UpgradeMechanicalDeltaV1::default()
        };
    };
    let mut delta = UpgradeMechanicalDeltaV1 {
        cost_delta: output_change.before.energy_cost - output_change.after.energy_cost,
        damage_delta: output_change.unconditional_damage_delta().unwrap_or(0),
        block_delta: output_change.unconditional_block_delta().unwrap_or(0),
        magic_delta: output_change.after.base_magic - output_change.before.base_magic,
        output_change: Some(output_change),
        exhaust_control_delta: analysis.is_upgrade_exhaust_control_delta,
        exhaust_removed_delta: analysis.is_upgrade_exhaust_removed_delta,
        ethereal_removed_delta: analysis.is_upgrade_ethereal_removed_delta,
        innate_delta: analysis.is_upgrade_innate_delta,
        notes: Vec::new(),
    };
    if delta
        .output_change
        .as_ref()
        .is_some_and(|change| change.has_unresolved_output_change())
    {
        delta.notes.push("output parameters or delivery change with conditions, choices or unreviewed scope; inspect before/after rules instead of assuming a flat gain".to_string());
    }
    if delta.cost_delta < 0 {
        delta
            .notes
            .push("upgrade resets modified energy cost upward".to_string());
    }
    if delta.exhaust_control_delta {
        delta.notes.push(
            "upgrade changes random/limited target control into a controlled effect".to_string(),
        );
    }
    if delta.exhaust_removed_delta {
        delta
            .notes
            .push("upgrade removes exhaust or substantially improves repeat usability".to_string());
    }
    if delta.ethereal_removed_delta {
        delta
            .notes
            .push("upgrade removes ethereal and retains the card across turns".to_string());
    }
    if delta.cost_delta > 0 {
        if run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::SneckoEye)
        {
            delta.cost_delta = 0;
            delta.notes.push(
                "Snecko Eye randomizes costs on draw, so this base-cost reduction has no ordinary draw value"
                    .to_string(),
            );
        } else {
            delta
                .notes
                .push("upgrade lowers base energy cost".to_string());
        }
    }
    delta
}

fn mechanical_upgrade_has_potential_value(delta: &UpgradeMechanicalDeltaV1) -> bool {
    delta.cost_delta > 0
        || delta.damage_delta != 0
        || delta.block_delta != 0
        || delta.magic_delta != 0
        || delta.exhaust_control_delta
        || delta.exhaust_removed_delta
        || delta.ethereal_removed_delta
        || delta.innate_delta
        || delta
            .output_change
            .as_ref()
            .is_some_and(|change| change.has_unresolved_output_change())
}

fn upgrade_roles(
    card: &CombatCard,
    delta: &UpgradeMechanicalDeltaV1,
    redundancy: &RedundancyProfileV1,
) -> Vec<UpgradeRoleV1> {
    let analysis = card_analysis_profile_v1(card.id, card.upgrades);
    let mut roles = Vec::new();
    if analysis.is_upgrade_core_mechanic {
        push_role(&mut roles, UpgradeRoleV1::CoreMechanic);
    }
    if analysis.is_upgrade_engine_enabler {
        push_role(&mut roles, UpgradeRoleV1::EngineEnabler);
    }
    if analysis.is_upgrade_consistency {
        push_role(&mut roles, UpgradeRoleV1::Consistency);
    }
    if analysis.is_upgrade_defensive_survival {
        push_role(&mut roles, UpgradeRoleV1::DefensiveSurvival);
    }
    if analysis.is_upgrade_scaling {
        push_role(&mut roles, UpgradeRoleV1::Scaling);
    }
    if analysis.is_upgrade_phase_burst {
        push_role(&mut roles, UpgradeRoleV1::PhaseBurst);
    }
    if matches!(
        redundancy.group,
        RedundancyGroupV1::WeakApplication | RedundancyGroupV1::VulnerableApplication
    ) || analysis.is_upgrade_debuff_coverage_candidate
    {
        push_role(&mut roles, UpgradeRoleV1::DebuffCoverage);
    }
    if delta.damage_delta > 0 {
        push_role(&mut roles, UpgradeRoleV1::FrontloadDamage);
    }
    if delta.damage_delta > 0 && analysis.cost >= 2 {
        push_role(&mut roles, UpgradeRoleV1::TransitionalPower);
    }
    if delta
        .output_change
        .as_ref()
        .is_some_and(|change| change.has_unresolved_output_change())
    {
        push_role(&mut roles, UpgradeRoleV1::UnresolvedOutputChange);
    }
    if roles.is_empty() {
        push_role(&mut roles, UpgradeRoleV1::Speculative);
    }
    roles
}

fn redundancy_profile(run_state: &RunState, card: CardId) -> RedundancyProfileV1 {
    let group = redundancy_group(card);
    let stack_behavior = stack_behavior(card);
    let same_card_count = run_state
        .master_deck
        .iter()
        .filter(|entry| entry.id == card)
        .count();
    let existing_group_count = run_state
        .master_deck
        .iter()
        .filter(|entry| card_supplies_redundancy_group(entry.id, group))
        .count();
    let saturated =
        redundancy_saturated(group, stack_behavior, same_card_count, existing_group_count);
    let mut notes = Vec::new();
    if same_card_count > 1 {
        notes.push(format!("same_card_count={same_card_count}"));
    }
    if existing_group_count > same_card_count && !matches!(group, RedundancyGroupV1::Generic) {
        notes.push(format!(
            "redundancy_group={:?} existing_group_count={existing_group_count}",
            group
        ));
    }
    if saturated {
        notes.push(format!(
            "redundancy group {:?} is saturated for upgrade marginal value",
            group
        ));
    }
    RedundancyProfileV1 {
        group,
        stack_behavior,
        same_card_count,
        existing_group_count,
        saturated,
        notes,
    }
}

fn card_supplies_redundancy_group(card: CardId, group: RedundancyGroupV1) -> bool {
    match group {
        RedundancyGroupV1::VulnerableApplication => {
            card_analysis_profile_v1(card, 0).vulnerable_support
                != crate::ai::card_analysis_v1::CardAnalysisVulnerableSupportV1::None
        }
        RedundancyGroupV1::WeakApplication => {
            card_reward_semantic_profile_v1(&crate::state::rewards::RewardCard::new(card, 0))
                .roles
                .contains(&CardRewardSemanticRoleV1::Weak)
        }
        _ => redundancy_group(card) == group,
    }
}

fn redundancy_group(card: CardId) -> RedundancyGroupV1 {
    match card_analysis_profile_v1(card, 0).upgrade_redundancy_group {
        CardAnalysisUpgradeRedundancyGroupV1::WeakApplication => RedundancyGroupV1::WeakApplication,
        CardAnalysisUpgradeRedundancyGroupV1::VulnerableApplication => {
            RedundancyGroupV1::VulnerableApplication
        }
        CardAnalysisUpgradeRedundancyGroupV1::ControlledExhaust => {
            RedundancyGroupV1::ControlledExhaust
        }
        CardAnalysisUpgradeRedundancyGroupV1::MassExhaust => RedundancyGroupV1::MassExhaust,
        CardAnalysisUpgradeRedundancyGroupV1::DrawCantrip => RedundancyGroupV1::DrawCantrip,
        CardAnalysisUpgradeRedundancyGroupV1::FrontloadBigAttack => {
            RedundancyGroupV1::FrontloadBigAttack
        }
        CardAnalysisUpgradeRedundancyGroupV1::PersistentStrengthScaling => {
            RedundancyGroupV1::PersistentStrengthScaling
        }
        CardAnalysisUpgradeRedundancyGroupV1::ExhaustPayoffPower => {
            RedundancyGroupV1::ExhaustPayoffPower
        }
        CardAnalysisUpgradeRedundancyGroupV1::NonStackingPower => {
            RedundancyGroupV1::NonStackingPower
        }
        CardAnalysisUpgradeRedundancyGroupV1::BurstBlock => RedundancyGroupV1::BurstBlock,
        CardAnalysisUpgradeRedundancyGroupV1::Generic => RedundancyGroupV1::Generic,
    }
}

fn stack_behavior(card: CardId) -> StackBehaviorV1 {
    match card_analysis_profile_v1(card, 0).upgrade_stack_behavior {
        CardAnalysisUpgradeStackBehaviorV1::DurationCoverage => StackBehaviorV1::DurationCoverage,
        CardAnalysisUpgradeStackBehaviorV1::DensityPositive => StackBehaviorV1::DensityPositive,
        CardAnalysisUpgradeStackBehaviorV1::DensityNegative => StackBehaviorV1::DensityNegative,
        CardAnalysisUpgradeStackBehaviorV1::RedundantAfterFirst => {
            StackBehaviorV1::RedundantAfterFirst
        }
        CardAnalysisUpgradeStackBehaviorV1::StackableIntensity => {
            StackBehaviorV1::StackableIntensity
        }
        CardAnalysisUpgradeStackBehaviorV1::ComboThreshold => StackBehaviorV1::ComboThreshold,
        CardAnalysisUpgradeStackBehaviorV1::NonStackingOnce => StackBehaviorV1::NonStackingOnce,
        CardAnalysisUpgradeStackBehaviorV1::Generic => StackBehaviorV1::Generic,
    }
}

fn redundancy_saturated(
    group: RedundancyGroupV1,
    behavior: StackBehaviorV1,
    same_card_count: usize,
    existing_group_count: usize,
) -> bool {
    match behavior {
        StackBehaviorV1::DurationCoverage => same_card_count >= 2 || existing_group_count >= 3,
        StackBehaviorV1::RedundantAfterFirst | StackBehaviorV1::NonStackingOnce => {
            same_card_count >= 2
        }
        StackBehaviorV1::DensityNegative => same_card_count >= 2 || existing_group_count >= 3,
        StackBehaviorV1::StackableIntensity => same_card_count >= 3,
        StackBehaviorV1::DensityPositive => false,
        StackBehaviorV1::ComboThreshold => false,
        StackBehaviorV1::Generic => {
            matches!(group, RedundancyGroupV1::Generic) && same_card_count >= 3
        }
    }
}

fn apply_boss_specific_evidence(run_state: &RunState, candidate: &mut UpgradeCandidateV1) {
    let Some(boss) = run_state.boss_key else {
        return;
    };
    let pressure = boss_mechanic_pressure_profile_v1(run_state, boss);
    if boss == EncounterId::TheChamp {
        if pressure.has_pressure(BossMechanicPressurePointV1::ChampTransitionWindow)
            && candidate.roles.contains(&UpgradeRoleV1::PhaseBurst)
        {
            candidate.evidence.push(
                "boss pressure: The Champ transition window values burst upgrades".to_string(),
            );
        }
        if pressure.has_pressure(BossMechanicPressurePointV1::ExecuteBlockCheck)
            && candidate.roles.contains(&UpgradeRoleV1::DefensiveSurvival)
        {
            candidate.evidence.push(
                "boss pressure: The Champ execute check values defensive upgrades".to_string(),
            );
        }
        if matches!(
            candidate.redundancy.group,
            RedundancyGroupV1::WeakApplication | RedundancyGroupV1::VulnerableApplication
        ) && pressure.has_red_flag(BossMechanicRedFlagV1::PrematureChampTransitionRisk)
        {
            candidate.risks.push(
                "The Champ clears debuffs at phase transition, so extra debuff duration is not a complete boss plan"
                    .to_string(),
            );
        }
    }
}

fn build_upgrade_debt_ledger(
    run_state: &RunState,
    candidates: &[UpgradeCandidateV1],
) -> UpgradeDebtLedgerV1 {
    let mut debts = Vec::new();
    let boss_pressure = run_state
        .boss_key
        .map(|boss| boss_mechanic_pressure_profile_v1(run_state, boss));
    add_debt_if_candidates(
        &mut debts,
        UpgradeDebtKindV1::ControlledExhaust,
        candidates,
        |candidate| {
            candidate.roles.contains(&UpgradeRoleV1::CoreMechanic)
                && candidate.roles.contains(&UpgradeRoleV1::EngineEnabler)
        },
        UpgradeDebtSeverityV1::ImportantBeforeBoss,
        pressure_label(run_state, "controlled exhaust"),
        "unpaid controlled exhaust makes exhaust packages less reliable",
    );
    if boss_pressure.as_ref().is_some_and(|pressure| {
        pressure.has_pressure(BossMechanicPressurePointV1::StasisKeyCardAccess)
    }) {
        add_debt_if_candidates(
            &mut debts,
            UpgradeDebtKindV1::StasisRecovery,
            candidates,
            |candidate| {
                card_analysis_profile_v1(candidate.card, candidate.upgrades)
                    .is_upgrade_stasis_recovery_candidate
            },
            UpgradeDebtSeverityV1::ImportantBeforeBoss,
            pressure_label(run_state, "stasis recovery"),
            "unpaid stasis recovery upgrade can leave key defensive cards unusable on the boss turn",
        );
    }
    if boss_pressure.as_ref().is_some_and(|pressure| {
        pressure.has_pressure(BossMechanicPressurePointV1::HyperbeamTurnSixCheck)
    }) {
        add_debt_if_candidates(
            &mut debts,
            UpgradeDebtKindV1::HyperbeamBlock,
            candidates,
            |candidate| {
                card_analysis_profile_v1(candidate.card, candidate.upgrades)
                    .is_upgrade_hyperbeam_block_candidate
            },
            UpgradeDebtSeverityV1::ImportantBeforeBoss,
            pressure_label(run_state, "hyperbeam block"),
            "unpaid hyperbeam block upgrade can leave the Automaton turn-six check undercovered",
        );
    }
    add_debt_if_candidates(
        &mut debts,
        UpgradeDebtKindV1::PhaseBurst,
        candidates,
        |candidate| {
            candidate.roles.contains(&UpgradeRoleV1::PhaseBurst)
                && !candidate.roles.contains(&UpgradeRoleV1::LowMarginalRepeat)
        },
        phase_burst_severity(run_state),
        pressure_label(run_state, "phase burst"),
        "unpaid burst upgrade can leave boss transition windows underpowered",
    );
    add_debt_if_candidates(
        &mut debts,
        UpgradeDebtKindV1::ExecuteBlock,
        candidates,
        |candidate| candidate.roles.contains(&UpgradeRoleV1::DefensiveSurvival),
        execute_block_severity(run_state),
        pressure_label(run_state, "execute block"),
        "unpaid defensive upgrade can force later rest or high combat loss",
    );
    add_debt_if_candidates(
        &mut debts,
        UpgradeDebtKindV1::AccessRecovery,
        candidates,
        |candidate| candidate.roles.contains(&UpgradeRoleV1::Consistency),
        UpgradeDebtSeverityV1::UsefulSoon,
        pressure_label(run_state, "access recovery"),
        "unpaid access upgrade slows setup and recovery from poor draws",
    );
    let has_exhaust_executor = run_state
        .master_deck
        .iter()
        .any(|card| card_analysis_profile_v1(card.id, card.upgrades).is_startup_exhaust_engine);
    let has_mummified_hand = run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::MummifiedHand);
    if has_exhaust_executor && !has_mummified_hand {
        add_debt_if_candidates(
            &mut debts,
            UpgradeDebtKindV1::ExhaustPayoffSetupTempo,
            candidates,
            |candidate| {
                candidate.mechanical_delta.cost_delta > 0
                    && card_analysis_profile_v1(candidate.card, candidate.upgrades)
                        .has_exhaust_payoff
            },
            UpgradeDebtSeverityV1::ImportantBeforeBoss,
            pressure_label(run_state, "supported exhaust-payoff setup tempo"),
            "unpaid exhaust-payoff cost reduction can keep an otherwise supported exhaust engine from starting on curve",
        );
    }
    add_debt_if_candidates(
        &mut debts,
        UpgradeDebtKindV1::PowerSetupTempo,
        candidates,
        |candidate| {
            candidate.mechanical_delta.cost_delta > 0
                && candidate.roles.iter().any(|role| {
                    matches!(
                        role,
                        UpgradeRoleV1::CoreMechanic
                            | UpgradeRoleV1::EngineEnabler
                            | UpgradeRoleV1::Scaling
                    )
                })
                && mummified_hand_power_tempo_v1(run_state, candidate.card, candidate.upgrades)
                    .is_some()
        },
        UpgradeDebtSeverityV1::UsefulSoon,
        pressure_label(run_state, "Mummified Hand power setup tempo"),
        "unpaid Power cost reduction delays both the engine and its Mummified Hand trigger",
    );
    add_debt_if_candidates(
        &mut debts,
        UpgradeDebtKindV1::ScalingSetup,
        candidates,
        |candidate| candidate.roles.contains(&UpgradeRoleV1::Scaling),
        scaling_severity(run_state),
        pressure_label(run_state, "scaling setup"),
        "unpaid scaling upgrade can leave long fights without a plan",
    );
    add_debt_if_candidates(
        &mut debts,
        UpgradeDebtKindV1::DebuffCoverage,
        candidates,
        |candidate| {
            candidate.roles.contains(&UpgradeRoleV1::DebuffCoverage)
                && !candidate.roles.contains(&UpgradeRoleV1::LowMarginalRepeat)
        },
        UpgradeDebtSeverityV1::UsefulSoon,
        pressure_label(run_state, "debuff coverage"),
        "unpaid debuff duration can reduce safe damage windows",
    );
    add_debt_if_candidates(
        &mut debts,
        UpgradeDebtKindV1::TransitionalFrontload,
        candidates,
        |candidate| {
            candidate.roles.contains(&UpgradeRoleV1::TransitionalPower)
                && !candidate.roles.contains(&UpgradeRoleV1::LowMarginalRepeat)
        },
        UpgradeDebtSeverityV1::Opportunistic,
        pressure_label(run_state, "frontload"),
        "unpaid frontload upgrade is usually replaceable once core debt exists",
    );

    let unpaid_core_count = debts
        .iter()
        .filter(|debt| debt.severity >= UpgradeDebtSeverityV1::ImportantBeforeBoss)
        .count();
    let upgrade_slots_pressure = match unpaid_core_count {
        0 => UpgradeSlotPressureV1::Low,
        1 => UpgradeSlotPressureV1::Medium,
        _ => UpgradeSlotPressureV1::High,
    };
    UpgradeDebtLedgerV1 {
        debts,
        unpaid_core_count,
        upgrade_slots_pressure,
    }
}

fn add_debt_if_candidates<F>(
    debts: &mut Vec<UpgradeDebtV1>,
    kind: UpgradeDebtKindV1,
    candidates: &[UpgradeCandidateV1],
    predicate: F,
    severity: UpgradeDebtSeverityV1,
    required_by: String,
    if_unpaid: &'static str,
) where
    F: Fn(&UpgradeCandidateV1) -> bool,
{
    let candidate_deck_indices = candidates
        .iter()
        .filter(|candidate| predicate(candidate))
        .map(|candidate| candidate.deck_index)
        .collect::<Vec<_>>();
    if candidate_deck_indices.is_empty() || severity <= UpgradeDebtSeverityV1::Defer {
        return;
    }
    debts.push(UpgradeDebtV1 {
        kind,
        severity,
        required_by,
        candidate_deck_indices,
        if_unpaid: vec![if_unpaid.to_string()],
    });
}

fn phase_burst_severity(run_state: &RunState) -> UpgradeDebtSeverityV1 {
    if run_state.boss_key == Some(EncounterId::TheChamp) && run_state.act_num == 2 {
        UpgradeDebtSeverityV1::ImportantBeforeBoss
    } else {
        UpgradeDebtSeverityV1::UsefulSoon
    }
}

fn execute_block_severity(run_state: &RunState) -> UpgradeDebtSeverityV1 {
    if matches!(
        run_state.boss_key,
        Some(EncounterId::TheChamp | EncounterId::Automaton)
    ) && run_state.act_num == 2
    {
        UpgradeDebtSeverityV1::ImportantBeforeBoss
    } else {
        UpgradeDebtSeverityV1::UsefulSoon
    }
}

fn scaling_severity(run_state: &RunState) -> UpgradeDebtSeverityV1 {
    if let Some(boss) = run_state.boss_key {
        let pressure = boss_mechanic_pressure_profile_v1(run_state, boss);
        if pressure.has_missing_answer(BossMechanicMissingAnswerV1::ChampTransitionBurst)
            || pressure.has_missing_answer(BossMechanicMissingAnswerV1::HasteBurstOrSetupPlan)
        {
            return UpgradeDebtSeverityV1::ImportantBeforeBoss;
        }
    }
    UpgradeDebtSeverityV1::UsefulSoon
}

fn pressure_label(run_state: &RunState, fallback: &str) -> String {
    run_state
        .boss_key
        .map(|boss| format!("{}:{fallback}", boss.stable_id()))
        .unwrap_or_else(|| fallback.to_string())
}

fn apply_debt_ledger(candidate: &mut UpgradeCandidateV1, debt_ledger: &UpgradeDebtLedgerV1) {
    for debt in &debt_ledger.debts {
        if debt.candidate_deck_indices.contains(&candidate.deck_index) {
            candidate.pays_debts.push(debt.kind);
            candidate.urgency = candidate.urgency.max(debt.severity);
            candidate.evidence.push(format!(
                "pays upgrade debt {} severity={:?} required_by={}",
                debt.kind.label(),
                debt.severity,
                debt.required_by
            ));
        } else if debt.severity >= UpgradeDebtSeverityV1::ImportantBeforeBoss {
            candidate.opportunity_costs.push(format!(
                "does not pay high-severity debt {}",
                debt.kind.label()
            ));
        }
    }

    candidate.verdict = upgrade_verdict_for_candidate(candidate);
}

fn upgrade_verdict_for_candidate(candidate: &UpgradeCandidateV1) -> UpgradeVerdictV1 {
    if candidate.roles.contains(&UpgradeRoleV1::LowMarginalRepeat) {
        return if candidate.urgency >= UpgradeDebtSeverityV1::ImportantBeforeBoss {
            UpgradeVerdictV1::Defer
        } else {
            UpgradeVerdictV1::Avoid
        };
    }

    match candidate.urgency {
        UpgradeDebtSeverityV1::CriticalBeforeBoss => UpgradeVerdictV1::CoreDebtPayment,
        UpgradeDebtSeverityV1::ImportantBeforeBoss => UpgradeVerdictV1::Important,
        UpgradeDebtSeverityV1::UsefulSoon => UpgradeVerdictV1::Useful,
        UpgradeDebtSeverityV1::Opportunistic => UpgradeVerdictV1::Opportunistic,
        UpgradeDebtSeverityV1::Defer => UpgradeVerdictV1::Defer,
        UpgradeDebtSeverityV1::Avoid => UpgradeVerdictV1::Avoid,
    }
}

fn combat_upgrade_candidate_class(candidate: &UpgradeCandidateV1) -> CombatUpgradeCandidateClassV1 {
    if matches!(candidate.card, CardId::Armaments | CardId::Apotheosis)
        || candidate.mechanical_delta.innate_delta
    {
        return CombatUpgradeCandidateClassV1::NoCombatCredit;
    }
    if candidate.mechanical_delta.cost_delta != 0
        || candidate.mechanical_delta.exhaust_control_delta
        || candidate.mechanical_delta.exhaust_removed_delta
        || candidate.mechanical_delta.ethereal_removed_delta
    {
        return CombatUpgradeCandidateClassV1::TimingSensitive;
    }

    let timing_sensitive_role = candidate.roles.iter().any(|role| {
        !matches!(
            role,
            UpgradeRoleV1::FrontloadDamage
                | UpgradeRoleV1::TransitionalPower
                | UpgradeRoleV1::LowMarginalRepeat
                | UpgradeRoleV1::Speculative
        )
    });
    if timing_sensitive_role {
        return CombatUpgradeCandidateClassV1::TimingSensitive;
    }

    if candidate.mechanical_delta.damage_delta > 0
        || candidate.mechanical_delta.block_delta > 0
        || candidate.mechanical_delta.magic_delta != 0
    {
        CombatUpgradeCandidateClassV1::OrdinaryStat
    } else {
        CombatUpgradeCandidateClassV1::NoCombatCredit
    }
}

fn lower_upgrade_urgency_one_level(urgency: UpgradeDebtSeverityV1) -> UpgradeDebtSeverityV1 {
    match urgency {
        UpgradeDebtSeverityV1::CriticalBeforeBoss | UpgradeDebtSeverityV1::ImportantBeforeBoss => {
            urgency
        }
        UpgradeDebtSeverityV1::UsefulSoon => UpgradeDebtSeverityV1::Opportunistic,
        UpgradeDebtSeverityV1::Opportunistic => UpgradeDebtSeverityV1::Defer,
        UpgradeDebtSeverityV1::Defer => UpgradeDebtSeverityV1::Avoid,
        UpgradeDebtSeverityV1::Avoid => UpgradeDebtSeverityV1::Avoid,
    }
}

fn combat_upgrade_source_evidence(coverage: &CombatUpgradeCoverageProfileV1) -> String {
    coverage
        .sources
        .iter()
        .map(|source| {
            format!(
                "{:?}@{}+{}:{:?}",
                source.card, source.deck_index, source.upgrades, source.scope
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn apply_combat_upgrade_coverage(
    candidate: &mut UpgradeCandidateV1,
    coverage: &CombatUpgradeCoverageProfileV1,
) {
    candidate.combat_upgrade_class = combat_upgrade_candidate_class(candidate);
    let Some(scope) = coverage.strongest_scope() else {
        candidate.combat_upgrade_credit = CombatUpgradeCreditV1::None;
        return;
    };

    candidate.evidence.push(format!(
        "combat upgrade coverage sources={} class={:?}",
        combat_upgrade_source_evidence(coverage),
        candidate.combat_upgrade_class
    ));
    if candidate.combat_upgrade_class != CombatUpgradeCandidateClassV1::OrdinaryStat {
        candidate.evidence.push(
            match candidate.combat_upgrade_class {
                CombatUpgradeCandidateClassV1::TimingSensitive => {
                    "combat upgrade credit withheld: timing-sensitive or unrecognized role/delta"
                }
                CombatUpgradeCandidateClassV1::NoCombatCredit => {
                    "combat upgrade credit withheld: provider, innate, or no ordinary stat delta"
                }
                CombatUpgradeCandidateClassV1::OrdinaryStat => unreachable!(),
            }
            .to_string(),
        );
        candidate.combat_upgrade_credit = CombatUpgradeCreditV1::EvidenceOnly { scope };
        return;
    }

    let broad_coverage = coverage.has_scope(CombatUpgradeScopeV1::WholeHand)
        || coverage.has_scope(CombatUpgradeScopeV1::AllCombatZones);
    let selected_coverage_applies = coverage.has_scope(CombatUpgradeScopeV1::SelectedCardInHand)
        && (is_starter(candidate.card)
            || candidate.roles.contains(&UpgradeRoleV1::LowMarginalRepeat));
    if !broad_coverage && !selected_coverage_applies {
        candidate.evidence.push(
            "combat upgrade credit withheld: selected-card access is too narrow for this target"
                .to_string(),
        );
        candidate.combat_upgrade_credit = CombatUpgradeCreditV1::EvidenceOnly { scope };
        return;
    }
    if candidate.urgency >= UpgradeDebtSeverityV1::ImportantBeforeBoss {
        candidate.evidence.push(
            "combat upgrade credit withheld: high-severity permanent upgrade debt".to_string(),
        );
        candidate.combat_upgrade_credit = CombatUpgradeCreditV1::EvidenceOnly { scope };
        return;
    }

    let previous_urgency = candidate.urgency;
    let resulting_urgency = lower_upgrade_urgency_one_level(previous_urgency);
    if resulting_urgency == previous_urgency {
        candidate.combat_upgrade_credit = CombatUpgradeCreditV1::EvidenceOnly { scope };
        return;
    }

    candidate.urgency = resulting_urgency;
    candidate.verdict = upgrade_verdict_for_candidate(candidate);
    candidate.combat_upgrade_credit = CombatUpgradeCreditV1::PriorityReduced {
        scope,
        previous_urgency,
        resulting_urgency,
    };
    candidate.evidence.push(format!(
        "combat upgrade coverage reduced urgency {:?}->{:?}",
        previous_urgency, resulting_urgency
    ));
}

fn best_smith_debt_paid(candidates: &[UpgradeCandidateV1]) -> Option<UpgradeDebtKindV1> {
    candidates
        .iter()
        .find(|candidate| candidate.verdict >= UpgradeVerdictV1::Useful)
        .and_then(|candidate| candidate.pays_debts.first().copied())
}

fn rest_vs_smith_plan(
    run_state: &RunState,
    best_smith_debt_paid: Option<UpgradeDebtKindV1>,
) -> RestVsSmithPlanV1 {
    let rest_heal_cap = campfire_rest_heal_capacity(run_state.max_hp, &run_state.relics);
    let missing_hp = run_state.max_hp.saturating_sub(run_state.current_hp).max(0);
    let effective_rest_heal = missing_hp.min(rest_heal_cap);
    let recovery_sources = recovery_sources(run_state);
    let mut reasons = Vec::new();
    reasons.push(format!(
        "effective rest heal is {effective_rest_heal}/{rest_heal_cap}"
    ));
    if !recovery_sources.is_empty() {
        reasons.push(format!(
            "recovery sources visible: {}",
            recovery_sources.join(",")
        ));
    }
    if let Some(debt) = best_smith_debt_paid {
        reasons.push(format!("best smith pays {}", debt.label()));
    }
    let verdict = if run_state.max_hp > 0 && run_state.current_hp * 100 < run_state.max_hp * 45 {
        RestVsSmithVerdictV1::RestFavored
    } else if effective_rest_heal <= 12 && best_smith_debt_paid.is_some() {
        RestVsSmithVerdictV1::SmithFavored
    } else {
        RestVsSmithVerdictV1::NeedsRouteRisk
    };
    RestVsSmithPlanV1 {
        current_hp: run_state.current_hp,
        max_hp: run_state.max_hp,
        effective_rest_heal,
        rest_heal_cap,
        recovery_sources,
        best_smith_debt_paid,
        verdict,
        reasons,
    }
}

fn recovery_sources(run_state: &RunState) -> Vec<String> {
    let mut sources = Vec::new();
    for relic in &run_state.relics {
        match relic.id {
            RelicId::BurningBlood => sources.push("BurningBlood(+6 after combat)".to_string()),
            RelicId::BlackBlood => sources.push("BlackBlood(+12 after combat)".to_string()),
            RelicId::MeatOnTheBone => sources.push("MeatOnTheBone(low HP recovery)".to_string()),
            RelicId::Pantograph => sources.push("Pantograph(boss heal)".to_string()),
            _ => {}
        }
    }
    if run_state
        .master_deck
        .iter()
        .any(|card| card_analysis_profile_v1(card.id, card.upgrades).has_combat_sustain)
    {
        sources.push("Reaper(combat healing if supported)".to_string());
    }
    sources
}

fn compare_upgrade_candidates(
    left: &UpgradeCandidateV1,
    right: &UpgradeCandidateV1,
) -> std::cmp::Ordering {
    right
        .verdict
        .cmp(&left.verdict)
        .then_with(|| right.urgency.cmp(&left.urgency))
        .then_with(|| right.pays_debts.len().cmp(&left.pays_debts.len()))
        .then_with(|| {
            left.roles
                .contains(&UpgradeRoleV1::LowMarginalRepeat)
                .cmp(&right.roles.contains(&UpgradeRoleV1::LowMarginalRepeat))
        })
        .then_with(|| left.deck_index.cmp(&right.deck_index))
}

fn push_role(roles: &mut Vec<UpgradeRoleV1>, role: UpgradeRoleV1) {
    if !roles.contains(&role) {
        roles.push(role);
    }
}

fn card_label(card: CardId, upgrades: u8) -> String {
    let name = get_card_definition(card).name;
    if upgrades == 0 {
        name.to_string()
    } else {
        format!("{name}+{upgrades}")
    }
}

fn is_starter(card: CardId) -> bool {
    matches!(
        card_analysis_profile_v1(card, 0).source,
        CardAnalysisDeckSourceV1::StarterStrike
            | CardAnalysisDeckSourceV1::StarterDefend
            | CardAnalysisDeckSourceV1::StarterUnique
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::RelicState;

    #[test]
    fn upgrade_damage_delta_uses_actual_before_after_output() {
        let run = RunState::new(1, 0, false, "Ironclad");
        for (id, upgrades, expected) in [
            (CardId::Pummel, 0, 2),
            (CardId::DaggerSpray, 0, 4),
            (CardId::GlassKnife, 0, 8),
            (CardId::SearingBlow, 5, 9),
            (CardId::Wish, 0, 0),
            (CardId::Whirlwind, 0, 0),
            (CardId::GrandFinale, 0, 0),
        ] {
            let mut card = CombatCard::for_scenario(id, 1);
            card.upgrades = upgrades;
            assert_eq!(
                mechanical_upgrade_delta(&run, &card).damage_delta,
                expected,
                "{id:?}"
            );
        }
    }

    #[test]
    fn conditional_block_upgrade_is_not_flat_block() {
        let run = RunState::new(1, 0, false, "Ironclad");
        for id in [
            CardId::Wish,
            CardId::SecondWind,
            CardId::EscapePlan,
            CardId::ReinforcedBody,
            CardId::Stack,
        ] {
            let card = CombatCard::for_scenario(id, 1);
            assert_eq!(
                mechanical_upgrade_delta(&run, &card).block_delta,
                0,
                "{id:?}"
            );
        }
    }

    #[test]
    fn upgrade_plan_retains_unresolved_gains_without_frontload_claims() {
        for id in [
            CardId::Whirlwind,
            CardId::Wish,
            CardId::SecondWind,
            CardId::GrandFinale,
        ] {
            let mut run = RunState::new(1, 0, false, "Ironclad");
            run.master_deck = vec![CombatCard::for_scenario(id, 1)];
            let plan = plan_upgrades_v1(&run);
            let candidate = &plan.candidates[0];
            assert!(
                candidate
                    .roles
                    .contains(&UpgradeRoleV1::UnresolvedOutputChange),
                "{id:?}"
            );
            assert!(
                !candidate.roles.contains(&UpgradeRoleV1::FrontloadDamage),
                "{id:?}"
            );
            assert!(
                mechanical_upgrade_has_potential_value(&candidate.mechanical_delta),
                "{id:?}"
            );
            assert!(candidate
                .mechanical_delta
                .output_change
                .as_ref()
                .unwrap()
                .has_unresolved_output_change());
        }
    }

    #[test]
    fn upgrade_delta_retains_cost_increases_after_previous_reduction() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        let mut card = CombatCard::for_scenario(CardId::Corruption, 1);
        card.cost_modifier = -3;
        let delta = mechanical_upgrade_delta(&run, &card);
        assert_eq!(delta.cost_delta, -2);
        assert_eq!(delta.output_change.unwrap().after.energy_cost, 2);
        run.master_deck = vec![card];
        let plan = plan_upgrades_v1(&run);
        assert_eq!(plan.candidates[0].roles, vec![UpgradeRoleV1::Speculative]);
    }

    #[test]
    fn snecko_removes_pure_cost_upgrade_debt() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![CombatCard::for_scenario(CardId::Corruption, 1)];
        run_state.relics.push(RelicState::new(RelicId::SneckoEye));

        let candidate = plan_upgrades_v1(&run_state)
            .candidates
            .into_iter()
            .next()
            .expect("Corruption remains a legal smith target");

        assert_eq!(candidate.mechanical_delta.cost_delta, 0);
        assert_eq!(candidate.roles, vec![UpgradeRoleV1::Speculative]);
        assert!(!candidate
            .pays_debts
            .contains(&UpgradeDebtKindV1::ExhaustPayoffSetupTempo));
        assert!(candidate
            .evidence
            .iter()
            .any(|item| item.contains("Snecko Eye")));
    }

    #[test]
    fn disarm_upgrade_is_defensive_survival() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![CombatCard::for_scenario(CardId::Disarm, 1)];

        let candidate = plan_upgrades_v1(&run_state)
            .candidates
            .into_iter()
            .next()
            .expect("Disarm should be an upgrade candidate");

        assert!(candidate.roles.contains(&UpgradeRoleV1::DefensiveSurvival));
    }

    #[test]
    fn apparition_upgrade_delta_keeps_ethereal_distinct_from_exhaust() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.master_deck = vec![CombatCard::for_scenario(CardId::Apparition, 1)];

        let candidate = plan_upgrades_v1(&run_state)
            .candidates
            .into_iter()
            .next()
            .expect("unupgraded Apparition should be an upgrade candidate");

        assert!(candidate.mechanical_delta.ethereal_removed_delta);
        assert!(!candidate.mechanical_delta.exhaust_removed_delta);
    }

    #[test]
    fn repeated_apparition_upgrades_keep_independent_reliability_value() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.master_deck = (0..5)
            .map(|index| CombatCard::for_scenario(CardId::Apparition, 100 + index))
            .collect();

        let plan = plan_upgrades_v1(&run);
        let apparitions = plan
            .candidates
            .iter()
            .filter(|candidate| candidate.card == CardId::Apparition)
            .collect::<Vec<_>>();

        assert_eq!(apparitions.len(), 5);
        assert!(apparitions.iter().all(|candidate| {
            candidate.redundancy.stack_behavior == StackBehaviorV1::DensityPositive
                && !candidate.roles.contains(&UpgradeRoleV1::LowMarginalRepeat)
        }));
    }

    #[test]
    fn upgrade_planner_marks_second_clothesline_as_low_marginal_repeat() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.act_num = 2;
        run_state.boss_key = Some(EncounterId::TheChamp);
        run_state.master_deck.clear();
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Clothesline, 1));
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Clothesline, 2));
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::TrueGrit, 3));
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Uppercut, 4));

        let plan = plan_upgrades_v1(&run_state);
        let clothesline = plan
            .candidates
            .iter()
            .find(|candidate| candidate.card == CardId::Clothesline)
            .expect("clothesline should be upgrade candidate");

        assert!(clothesline
            .roles
            .contains(&UpgradeRoleV1::LowMarginalRepeat));
        assert!(clothesline.verdict <= UpgradeVerdictV1::Defer);
    }

    #[test]
    fn upgrade_planner_prefers_true_grit_debt_over_repeat_debuff() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.act_num = 2;
        run_state.boss_key = Some(EncounterId::TheChamp);
        run_state.master_deck.clear();
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Clothesline, 1));
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Clothesline, 2));
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::TrueGrit, 3));
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::SecondWind, 4));

        let plan = plan_upgrades_v1(&run_state);

        assert_eq!(plan.best_smith, Some(2));
        assert!(plan
            .debt_ledger
            .debts
            .iter()
            .any(|debt| debt.kind == UpgradeDebtKindV1::ControlledExhaust));
    }

    #[test]
    fn mummified_hand_exhaust_deck_prefers_survival_upgrade_over_redundant_bash() {
        // Keep the complete A1F15 public deck because duplicate debuff
        // coverage, exhaust support, and Mummified Hand tempo interact.
        let mut run = RunState::new(20260730007, 0, false, "Ironclad");
        run.act_num = 1;
        run.floor_num = 15;
        run.current_hp = 72;
        run.max_hp = 85;
        run.boss_key = Some(EncounterId::SlimeBoss);
        run.relics.push(RelicState::new(RelicId::MummifiedHand));
        run.master_deck = [
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
            CardId::Uppercut,
            CardId::SecondWind,
            CardId::Cleave,
            CardId::ShrugItOff,
            CardId::DarkEmbrace,
            CardId::Uppercut,
            CardId::WildStrike,
            CardId::DarkEmbrace,
            CardId::DemonForm,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();

        let plan = plan_upgrades_v1(&run);
        let bash = plan
            .candidates
            .iter()
            .find(|candidate| candidate.card == CardId::Bash)
            .expect("Bash upgrade candidate");
        let dark_embrace = plan
            .candidates
            .iter()
            .find(|candidate| candidate.card == CardId::DarkEmbrace)
            .expect("Dark Embrace upgrade candidate");

        assert_eq!(plan.best_smith, Some(10), "plan={plan:#?}");
        assert!(dark_embrace
            .pays_debts
            .contains(&UpgradeDebtKindV1::PowerSetupTempo));
        assert!(bash.roles.contains(&UpgradeRoleV1::LowMarginalRepeat));
        assert!(bash.verdict <= UpgradeVerdictV1::Defer);
    }

    #[test]
    fn supported_exhaust_engine_prioritizes_dark_embrace_cost_reduction() {
        let mut run = RunState::new(20260730007, 0, false, "Ironclad");
        run.act_num = 1;
        run.floor_num = 15;
        run.current_hp = 71;
        run.max_hp = 85;
        run.boss_key = Some(EncounterId::SlimeBoss);
        run.master_deck = [
            CardId::PowerThrough,
            CardId::SecondWind,
            CardId::DarkEmbrace,
            CardId::ThunderClap,
            CardId::ShrugItOff,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, card)| CombatCard::for_scenario(card, index as u32))
        .collect();

        let plan = plan_upgrades_v1(&run);
        let dark_embrace = plan
            .candidates
            .iter()
            .find(|candidate| candidate.card == CardId::DarkEmbrace)
            .expect("Dark Embrace upgrade candidate");

        assert_eq!(plan.best_smith, Some(2), "plan={plan:#?}");
        assert_eq!(
            dark_embrace.urgency,
            UpgradeDebtSeverityV1::ImportantBeforeBoss
        );
        assert!(dark_embrace
            .pays_debts
            .contains(&UpgradeDebtKindV1::ExhaustPayoffSetupTempo));
    }

    #[test]
    fn upgrade_candidates_bind_to_uuid_after_deck_reordering() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.act_num = 2;
        run.boss_key = Some(EncounterId::TheChamp);
        run.master_deck = vec![
            CombatCard::for_scenario(CardId::Strike, 7001),
            CombatCard::for_scenario(CardId::TrueGrit, 7002),
            CombatCard::for_scenario(CardId::SecondWind, 7003),
        ];

        let candidate = plan_upgrades_v1(&run)
            .candidates
            .into_iter()
            .find(|candidate| candidate.card_lineage == crate::test_support::scenario_lineage(7002))
            .expect("True Grit should be found by stable card lineage");
        assert_eq!(
            candidate.card_lineage,
            crate::test_support::scenario_lineage(7002)
        );
        assert_eq!(candidate.deck_index, 1);

        run.master_deck.swap(0, 1);
        let reordered = upgrade_candidate_for_card_lineage_v1(
            &run,
            crate::test_support::scenario_lineage(7002),
        )
        .expect("the same True Grit should survive deck reordering");
        assert_eq!(
            reordered.card_lineage,
            crate::test_support::scenario_lineage(7002)
        );
        assert_eq!(reordered.deck_index, 0);
    }

    #[test]
    fn best_smith_debt_is_owned_by_upgrade_plan_and_mirrored_for_legacy() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.act_num = 2;
        run.boss_key = Some(EncounterId::TheChamp);
        run.master_deck = vec![
            CombatCard::for_scenario(CardId::TrueGrit, 7002),
            CombatCard::for_scenario(CardId::SecondWind, 7003),
        ];

        let plan = plan_upgrades_v1(&run);

        assert_eq!(
            plan.best_smith_debt_paid,
            Some(UpgradeDebtKindV1::ControlledExhaust)
        );
        assert_eq!(
            plan.best_smith_debt_paid,
            plan.rest_vs_smith.best_smith_debt_paid
        );
    }

    #[test]
    fn rest_vs_smith_flags_low_effective_heal_when_upgrade_debt_exists() {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.act_num = 2;
        run_state.boss_key = Some(EncounterId::TheChamp);
        run_state.current_hp = 69;
        run_state.max_hp = 80;
        run_state.master_deck.clear();
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::TrueGrit, 1));
        run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::SecondWind, 2));

        let plan = plan_upgrades_v1(&run_state);

        assert_eq!(plan.rest_vs_smith.effective_rest_heal, 11);
        assert_eq!(
            plan.rest_vs_smith.verdict,
            RestVsSmithVerdictV1::SmithFavored
        );
    }

    #[test]
    fn selected_armaments_credit_is_limited_to_ordinary_starter_targets() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.master_deck = vec![
            CombatCard::for_scenario(CardId::Armaments, 1001),
            CombatCard::for_scenario(CardId::Strike, 1002),
            CombatCard::for_scenario(CardId::Cleave, 1003),
        ];

        let plan = plan_upgrades_v1(&run);
        let strike = plan
            .candidates
            .iter()
            .find(|candidate| candidate.card == CardId::Strike)
            .expect("Strike should remain an upgrade candidate");
        let cleave = plan
            .candidates
            .iter()
            .find(|candidate| candidate.card == CardId::Cleave)
            .expect("Cleave should remain an upgrade candidate");

        assert_eq!(
            strike.combat_upgrade_class,
            CombatUpgradeCandidateClassV1::OrdinaryStat
        );
        assert!(matches!(
            strike.combat_upgrade_credit,
            CombatUpgradeCreditV1::PriorityReduced {
                scope: CombatUpgradeScopeV1::SelectedCardInHand,
                previous_urgency: UpgradeDebtSeverityV1::Opportunistic,
                resulting_urgency: UpgradeDebtSeverityV1::Defer,
            }
        ));
        assert_eq!(strike.verdict, UpgradeVerdictV1::Defer);
        assert_eq!(
            cleave.combat_upgrade_class,
            CombatUpgradeCandidateClassV1::OrdinaryStat
        );
        assert_eq!(
            cleave.combat_upgrade_credit,
            CombatUpgradeCreditV1::EvidenceOnly {
                scope: CombatUpgradeScopeV1::SelectedCardInHand,
            }
        );
        assert_eq!(cleave.urgency, UpgradeDebtSeverityV1::Opportunistic);
    }

    #[test]
    fn armaments_plus_reduces_an_ordinary_nonstarter_by_one_level() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        let mut armaments_plus = CombatCard::for_scenario(CardId::Armaments, 1001);
        armaments_plus.upgrades = 1;
        run.master_deck = vec![
            armaments_plus,
            CombatCard::for_scenario(CardId::Cleave, 1002),
        ];

        let plan = plan_upgrades_v1(&run);
        let cleave = plan
            .candidates
            .iter()
            .find(|candidate| candidate.card == CardId::Cleave)
            .expect("Cleave should remain an upgrade candidate");

        assert_eq!(
            cleave.combat_upgrade_class,
            CombatUpgradeCandidateClassV1::OrdinaryStat
        );
        assert!(matches!(
            cleave.combat_upgrade_credit,
            CombatUpgradeCreditV1::PriorityReduced {
                scope: CombatUpgradeScopeV1::WholeHand,
                previous_urgency: UpgradeDebtSeverityV1::Opportunistic,
                resulting_urgency: UpgradeDebtSeverityV1::Defer,
            }
        ));
        assert_eq!(cleave.verdict, UpgradeVerdictV1::Defer);
    }

    #[test]
    fn combat_upgrade_coverage_does_not_discount_providers_innate_or_phase_burst() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.act_num = 2;
        run.boss_key = Some(EncounterId::TheChamp);
        let mut armaments_plus = CombatCard::for_scenario(CardId::Armaments, 1001);
        armaments_plus.upgrades = 1;
        run.master_deck = vec![
            armaments_plus,
            CombatCard::for_scenario(CardId::Armaments, 1002),
            CombatCard::for_scenario(CardId::BootSequence, 1003),
            CombatCard::for_scenario(CardId::Whirlwind, 1004),
        ];

        let plan = plan_upgrades_v1(&run);
        let candidate = |card| {
            plan.candidates
                .iter()
                .find(|candidate| candidate.card == card)
                .expect("protected card should remain an upgrade candidate")
        };

        assert_eq!(
            candidate(CardId::Armaments).combat_upgrade_class,
            CombatUpgradeCandidateClassV1::NoCombatCredit
        );
        assert_eq!(
            candidate(CardId::BootSequence).combat_upgrade_class,
            CombatUpgradeCandidateClassV1::NoCombatCredit
        );
        assert_eq!(
            candidate(CardId::Whirlwind).combat_upgrade_class,
            CombatUpgradeCandidateClassV1::TimingSensitive
        );
        assert_eq!(
            candidate(CardId::Whirlwind).urgency,
            UpgradeDebtSeverityV1::ImportantBeforeBoss
        );
        assert!(!matches!(
            candidate(CardId::Whirlwind).combat_upgrade_credit,
            CombatUpgradeCreditV1::PriorityReduced { .. }
        ));
    }
}
