//! Read-only, identity-preserving potion expenditure counterfactuals.
//!
//! Every lane starts from the same exact combat root. The planner filters
//! explicit use/discard inputs by slot instead of deleting inventory from the
//! state, so potion-sensitive simulator behavior and RNG remain unchanged.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

use clap::Args;
use serde::Serialize;
use sts_combat_planner::{
    combat_plan_state_guide_policy_v1, CombatDecisionRoot, LocalTurnGraphWitnessInterruption,
    LocalTurnGraphWitnessStatus, OracleCombatWitness, OracleCombatWitnessDiscoverySource,
    OracleCombatWitnessSatisfaction, TurnOptionAction,
};
use sts_oracle_runtime::ai::card_semantics_v1::{
    potion_acquisition_traits_v1, PotionAcquisitionTraitV1,
};
use sts_oracle_runtime::ai::potion_continuation_context_v1::{
    PotionRunContinuationContextV1, PotionRunContinuationLimitationV1, PotionRunInventoryContextV1,
    PotionRunSupplyContextV1, POTION_RUN_CONTINUATION_CONTEXT_SCHEMA_NAME,
    POTION_RUN_CONTINUATION_CONTEXT_SCHEMA_VERSION,
};
use sts_oracle_runtime::ai::potion_continuation_pressure_v1::{
    potion_continuation_pressure_from_context_v1, PotionContinuationPressureInputsV1,
    PotionContinuationPressureV1, PotionRecoveryContinuationFactsV1, PotionRoutePressureV1,
    PotionShopContinuationFactsV1,
};
use sts_oracle_runtime::ai::route_window_facts::{
    RouteWindowCoverageKind, RouteWindowModality, RouteWindowPredicate, RouteWindowProvenance,
    RouteWindowSubject,
};
use sts_oracle_runtime::ai::strategy::deck_strategic_deficit::{
    assess_deck_strategic_deficit, DeckStrategicDeficit,
};
use sts_oracle_runtime::ai::strategy::run_strategic_facts::RunStrategicFacts;
use sts_oracle_runtime::content::cards::{get_card_definition, is_starter_basic, CardType};
use sts_oracle_runtime::content::potions::{Potion, PotionId};
use sts_oracle_runtime::content::relics::{energy_master_delta, RelicId};
use sts_oracle_runtime::eval::combat_case::{load_combat_case, CombatCase};
use sts_oracle_runtime::eval::combat_case_context::restore_combat_case_oracle_analysis_owner_v2;
use sts_oracle_runtime::eval::run_control::{
    authorized_potion_trial_policy_v1, oracle_potion_rescue_tier_v1,
    retained_combat_tactical_policy_v1, AtomicCombatSearchTraceSummaryV2,
    CombatSearchHpLossLimitV1, CombatSearchStrategicHpQualityFactsV1,
    CombatVictoryContinuationFactsV1, CombatVictoryHpCarryoverV1, OraclePotionRescueTierV1,
    COMBAT_QUALITY_HP_LIMIT_EVALUATOR_V1, COMBAT_SURVIVAL_HP_LIMIT_EVALUATOR_V1,
    COMBAT_VICTORY_CONTINUATION_EVALUATOR_V1,
};
use sts_oracle_runtime::runtime::branch::reconstruct_oracle_combat_context_trace_v1;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_evidence_manifest::{
    write_combat_evidence_manifest, CombatEvidenceManifestEntryV1, CombatEvidenceProducerV1,
    COMBAT_EVIDENCE_MANIFEST_FILE_SUFFIX,
};
use super::combat_graph_search_spec::LocalGraphSearchSpec;
use super::combat_replay_tools::save_combat_inputs;

const SCHEMA_NAME: &str = "OracleCombatCasePotionExpenditureAuditV14";

#[derive(Debug, Args)]
pub(super) struct CombatCasePotionExpenditureAuditArgs {
    /// Exact combat root to audit. The file is loaded read-only.
    #[arg(long)]
    case: PathBuf,
    /// Largest initial-potion subset opened in one isolated lane.
    /// Zero runs only the no-potion lane.
    #[arg(long, default_value_t = 1)]
    max_combination_size: usize,
    /// Safety bound for combinatorial lane expansion, including no-potion.
    #[arg(long, default_value_t = 16)]
    max_lanes: usize,
    /// Optional strategic final-HP reserve reported for every exact witness.
    #[arg(long)]
    survival_reserve_hp: Option<i32>,
    /// Save each replay-verified lane witness as
    /// `<lane-id>.actions.json` below this directory.
    #[arg(long, value_name = "DIRECTORY")]
    export_witness_actions_dir: Option<PathBuf>,
    /// Lab-only parity control: replay and preload one exact winning action
    /// list as the initial incumbent in every compatible lane.
    #[arg(long, value_name = "ACTIONS_JSON")]
    restore_witness_actions: Option<PathBuf>,
    /// Add the same typed combat-plan state guide used by production combat
    /// search while preserving independent exact potion-slot lanes.
    #[arg(long)]
    typed_plan_guide: bool,
    /// Lab-only parity control: give each non-empty identity lane the same
    /// unchanged-root potion challenge used by production run search.
    #[arg(long)]
    authorized_root_potion_trial: bool,
    /// Include explicit potion-discard actions. Disabled by default because a
    /// discard normally has no combat payoff; enable only for concrete slot
    /// generation or revive-priority cases.
    #[arg(long)]
    include_discard_actions: bool,
    /// Exact generation work granted independently to every lane.
    #[arg(long, default_value_t = 250_000)]
    max_nodes: usize,
    /// Lab-only parity control: use the production-style HP-loss satisfaction
    /// instead of searching every lane until budget or exhaustion.
    #[arg(long)]
    max_hp_loss: Option<u32>,
    /// Scheduler selections granted independently to every lane.
    #[arg(long, default_value_t = 1_000_000)]
    max_selections: usize,
    /// Wall allowance in milliseconds for each lane, not for the whole audit.
    #[arg(long, default_value_t = 10_000)]
    wall_ms_per_lane: u64,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
    #[arg(long, default_value_t = 50_000)]
    uniform_exploration_ppm: u32,
    #[arg(long, default_value_t = 4)]
    generation_quantum_work: usize,
    #[arg(long, default_value_t = 32)]
    max_turn_depth: usize,
    /// Contract assertion for durable case-specific regression commands.
    #[arg(long)]
    expect_no_potion_min_final_hp: Option<i32>,
    /// Require the no-potion witness to Pareto-dominate every compliant
    /// witness that actually consumes a potion.
    #[arg(long)]
    expect_no_potion_dominates_consuming: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionVerifiedWinRescueTierV1 {
    BoundedQuality,
    FindAnyWin,
    Excluded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionSharedStrategyCoverageV1 {
    Classified,
    Unclassified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionMechanicalRoleV1 {
    SingleTargetDamage,
    MultiTargetDamage,
    DamageOverTime,
    WeakControl,
    VulnerableControl,
    ImmediateBlock,
    ImmediateHealing,
    EnergyBurst,
    PersistentStrength,
    PersistentDexterity,
    TemporaryDexterity,
    TemporaryStrength,
    CardDraw,
    PersistentFocus,
    RandomAttackDiscovery,
    RandomSkillDiscovery,
    RandomPowerDiscovery,
    RandomColorlessDiscovery,
    MiracleGeneration,
    TemporaryUpgrade,
    Artifact,
    DelayedHealing,
    PlatedArmor,
    Thorns,
    RandomTopdeckPlay,
    NextCardDuplication,
    ShivGeneration,
    OrbCapacity,
    DiscardRecovery,
    HandRedraw,
    HandExhaust,
    StanceControl,
    DeathInsurance,
    Escape,
    MaxHpGain,
    PotionGeneration,
    CardDrawAndCostRandomization,
    Intangible,
    Metallicize,
    RitualScaling,
    Divinity,
    DarkOrbGeneration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionContinuationDependencyV1 {
    CurrentHpDeficit,
    FutureEncounterDamagePattern,
    FutureEnemyCountAndHealth,
    FutureFightLength,
    FutureHandAndDrawOrder,
    FutureDiscardState,
    DeckSynergy,
    RandomOutcomePool,
    HighValueCardTarget,
    DebuffTiming,
    LowHpInsuranceNeed,
    RouteEscapeValue,
    EmptyPotionSlotsAndAcquisitionRules,
    OrbPlan,
    StancePlan,
    OutOfCombatTiming,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionResourceV1 {
    slot: usize,
    id: String,
    uuid: u32,
    can_use: bool,
    can_discard: bool,
    verified_win_rescue_tier: PotionVerifiedWinRescueTierV1,
    shared_strategy_traits: Vec<PotionAcquisitionTraitV1>,
    shared_strategy_coverage: PotionSharedStrategyCoverageV1,
    mechanical_role: PotionMechanicalRoleV1,
    continuation_dependencies: Vec<PotionContinuationDependencyV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionExpenditureModeV1 {
    Use,
    Discard,
    Passive,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionExpenditureEventV1 {
    action_index: usize,
    slot: usize,
    id: String,
    uuid: u32,
    mode: PotionExpenditureModeV1,
    verified_win_rescue_tier: PotionVerifiedWinRescueTierV1,
}

#[derive(Clone, Debug)]
struct PotionAuditLaneSpec {
    lane_id: String,
    allowed_slot_mask: u64,
    allowed_potions: Vec<PotionResourceV1>,
    max_explicit_expenditures: u32,
}

#[derive(Clone, Debug, Serialize)]
struct PotionAuditSearchSettingsV1 {
    max_combination_size: usize,
    max_lanes: usize,
    survival_reserve_hp: Option<i32>,
    typed_plan_guide: bool,
    restore_witness_actions: Option<PathBuf>,
    authorized_root_potion_trial: bool,
    include_discard_actions: bool,
    max_nodes_per_lane: usize,
    max_hp_loss: Option<u32>,
    max_selections_per_lane: usize,
    wall_ms_per_lane: u64,
    max_engine_steps_per_transition: usize,
    uniform_exploration_ppm: u32,
    generation_quantum_work: usize,
    max_turn_depth: usize,
    satisfaction: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct PotionAuditLaneCountersV1 {
    selections: usize,
    generation_work: usize,
    engine_steps: usize,
    exact_nodes: usize,
    terminal_win_options: usize,
    witness_replay_attempts: usize,
    witness_replay_improvements: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionMarginalAssessmentV1 {
    NoPotionBaseline,
    NoPotionFrontierExhaustedUnderContract,
    NoPotionWitnessNotFoundUnderAllowance,
    CrossesSurvivalReserve,
    ImprovesFinalHp,
    SameFinalHpWithExtraResource,
    WorseFinalHpWithExtraResource,
    NoAdditionalPotionConsumed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum VerifiedWinPotionDispositionV1 {
    NoPotionSpent,
    BoundedQualityOnly,
    ContainsReservedResource,
    ContainsExcludedResource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionPolicyReviewFlagV1 {
    ReservedResourceCrossesSurvivalReserve,
    ReservedResourceImprovesHpWithoutCrossingReserve,
    AdmittedResourceIsParetoDominated,
    AdmittedResourceHasNoHpBenefit,
    DelayedHealRequiresExtraTurns,
    ExcludedResourceConsumed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
enum PotionSpendAdjudicationV1 {
    NoPotionBaseline,
    NoAdditionalPotionConsumed,
    UnknownWithoutNoPotionWitness {
        baseline_frontier_exhausted: bool,
    },
    RejectDominated {
        dominated_by: Vec<String>,
    },
    RejectNonPositiveHpGain {
        final_hp_delta: i32,
    },
    CompareContinuationValue {
        immediate_hp_gain: i32,
        break_even_retained_value_hp: i32,
        final_turn_delta: i64,
        potion_expenditures: usize,
        spend_urgency_question: PotionSpendUrgencyQuestionV1,
        retained_value_evidence: PotionRetainedValueEvidenceV1,
    },
    ExcludedFromVictorySpend,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionSurvivalReserveDeltaV1 {
    reserve_hp: i32,
    baseline_shortfall_hp: i32,
    candidate_shortfall_hp: i32,
    shortfall_reduction_hp: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    shortfall_reduction_ppm: Option<i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionSpendUrgencyQuestionStatusV1 {
    ValidatedExactRoot,
    Unavailable,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionSpendUrgencyQuestionLimitationV1 {
    RunContextUnavailable,
    ContinuationPressureUnavailable,
    CombatVictoryContinuationUnavailable,
    StrategicHpQualityUnavailable,
    RunContextRejected,
    ContinuationPressureRejected,
    CombatVictoryContinuationRejected,
    StrategicHpQualityRejected,
    ValidatedRunContextMissingPayload,
    ValidatedContinuationPressureMissingPayload,
    ValidatedCombatVictoryContinuationMissingPayload,
    ValidatedStrategicHpQualityMissingPayload,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionRouteOrderUnavailableReasonV1 {
    MissingTypedFact,
    ConflictingTypedFacts,
    UnknownModality,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum PotionRouteOrderEvidenceV1 {
    Validated {
        modality: RouteWindowModality,
        provenance: RouteWindowProvenance,
        horizon_nodes: usize,
    },
    Unavailable {
        reason: PotionRouteOrderUnavailableReasonV1,
        observed_modality: Option<RouteWindowModality>,
        provenance: Option<RouteWindowProvenance>,
        horizon_nodes: Option<usize>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionRouteOrderingFactsV1 {
    coverage_kind: RouteWindowCoverageKind,
    window_starts_after_current_decision: bool,
    future_known_combat_before_campfire: PotionRouteOrderEvidenceV1,
    future_known_combat_before_shop: PotionRouteOrderEvidenceV1,
    future_elite_before_campfire: PotionRouteOrderEvidenceV1,
    future_campfire_before_elite: PotionRouteOrderEvidenceV1,
    future_shop_before_known_combat: PotionRouteOrderEvidenceV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum PotionCombatVictoryContinuationEvidenceV1 {
    ValidatedCapturedFact {
        evaluator: String,
        hp_carryover: CombatVictoryHpCarryoverV1,
    },
    UnavailableLegacyCase,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionHpLossLimitAssessmentV1 {
    limit: CombatSearchHpLossLimitV1,
    baseline_policy_hp_loss: u32,
    candidate_policy_hp_loss: u32,
    baseline_satisfies: bool,
    candidate_satisfies: bool,
    candidate_crosses_from_unsatisfied_to_satisfied: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum PotionStrategicHpQualityEvidenceV1 {
    ValidatedCapturedFact {
        survival_evaluator: String,
        quality_evaluator: String,
        entry_current_hp: i32,
        entry_max_hp: i32,
        baseline_final_hp: i32,
        candidate_final_hp: i32,
        survival: PotionHpLossLimitAssessmentV1,
        quality: PotionHpLossLimitAssessmentV1,
    },
    UnavailableLegacyCase,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionSpendUrgencyFactsV1 {
    configured_survival_reserve_delta: Option<PotionSurvivalReserveDeltaV1>,
    combat_victory_continuation: PotionCombatVictoryContinuationEvidenceV1,
    strategic_hp_quality: PotionStrategicHpQualityEvidenceV1,
    inventory: PotionRunInventoryContextV1,
    supply: PotionRunSupplyContextV1,
    route: PotionRoutePressureV1,
    route_ordering: PotionRouteOrderingFactsV1,
    recovery: PotionRecoveryContinuationFactsV1,
    shop: PotionShopContinuationFactsV1,
    current_combat_reward_size_gate_unknown: bool,
    future_potion_identity_unknown: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionSpendUrgencyQuestionV1 {
    status: PotionSpendUrgencyQuestionStatusV1,
    run_context_status: PotionRunContinuationProjectionStatusV1,
    continuation_pressure_status: PotionContinuationPressureProjectionStatusV1,
    facts: Option<PotionSpendUrgencyFactsV1>,
    limitations: Vec<PotionSpendUrgencyQuestionLimitationV1>,
}

#[derive(Clone, Debug, Serialize)]
struct PotionMarginalComparisonV1 {
    final_hp_delta: Option<i32>,
    final_turn_delta: Option<i64>,
    action_count_delta: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    survival_reserve_delta: Option<PotionSurvivalReserveDeltaV1>,
    assessment: PotionMarginalAssessmentV1,
}

#[derive(Clone, Debug, Serialize)]
struct PotionAuditWitnessV1 {
    final_hp: i32,
    hp_loss: i32,
    final_player_turn: u32,
    turns_elapsed: u32,
    action_count: usize,
    explicit_potion_action_count: usize,
    potion_expenditures: Vec<PotionExpenditureEventV1>,
    verified_win_potion_disposition: VerifiedWinPotionDispositionV1,
    policy_review_flags: Vec<PotionPolicyReviewFlagV1>,
    lane_compliant: bool,
    meets_survival_reserve: Option<bool>,
    relative_to_no_potion: Option<PotionMarginalComparisonV1>,
    pareto_frontier: bool,
    dominated_by: Vec<String>,
    shadow_spend_adjudication: Option<PotionSpendAdjudicationV1>,
}

#[derive(Clone, Debug, Serialize)]
struct PotionAuditLaneResultV1 {
    lane_id: String,
    allowed_slot_mask: u64,
    allowed_potions: Vec<PotionResourceV1>,
    max_explicit_expenditures: u32,
    status: String,
    elapsed_ms: u64,
    counters: PotionAuditLaneCountersV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    exported_witness_actions: Option<PathBuf>,
    witness: Option<PotionAuditWitnessV1>,
}

#[derive(Clone, Debug, Serialize)]
struct PotionAuditLimitationsV1 {
    lane_absence_is_budget_unknown_unless_frontier_exhausted: bool,
    run_context_rejected_on_exact_root_mismatch: bool,
    continuation_pressure_rejected_without_exact_reconstruction: bool,
    combat_victory_continuation_requires_consistent_owner_capture: bool,
    strategic_hp_quality_requires_consistent_owner_capture: bool,
    retained_value_evidence_is_non_authoritative: bool,
    continuation_value_not_in_combat_case: Vec<&'static str>,
    passive_consumption_handling: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionCurrentCombatStakeV1 {
    Normal,
    Elite,
    Boss,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionInventoryPressureV1 {
    slot_capacity: usize,
    occupied_slots: usize,
    empty_slots: usize,
    inventory_full: bool,
    new_potion_would_require_replacement_if_obtainable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionRelicContextV1 {
    sacred_bark: bool,
    toy_ornithopter: bool,
    white_beast_statue: bool,
    sozu: bool,
    potion_belt: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionContinuationUnknownV1 {
    NextEncounterIdentity,
    RouteBeforeNextEliteOrBoss,
    FuturePotionDropRollAndIdentity,
    FuturePotionReplacementCandidate,
    FutureHandAndDrawOrder,
    FutureRestSiteAvailability,
}

#[derive(Clone, Debug, Serialize)]
struct PotionContinuationContextV1 {
    act: u8,
    floor: i32,
    current_combat_stake: PotionCurrentCombatStakeV1,
    current_hp: i32,
    max_hp: i32,
    deck_size: usize,
    inventory: PotionInventoryPressureV1,
    relics: PotionRelicContextV1,
    deck_strategic_deficit: DeckStrategicDeficit,
    run_level_projection: PotionRunContinuationProjectionV1,
    unavailable_future_context: Vec<PotionContinuationUnknownV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionRunContinuationProjectionStatusV1 {
    ValidatedExactRoot,
    UnavailableLegacyCase,
    RejectedRootMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionRunContinuationMismatchV1 {
    field: &'static str,
    expected: String,
    observed: String,
}

#[derive(Clone, Debug, Serialize)]
struct PotionRunContinuationProjectionV1 {
    status: PotionRunContinuationProjectionStatusV1,
    source: Option<&'static str>,
    attempt_index: Option<usize>,
    attempt_source: Option<String>,
    attempt_lane: Option<String>,
    mismatches: Vec<PotionRunContinuationMismatchV1>,
    captured_context: Option<PotionRunContinuationContextV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionContinuationPressureProjectionStatusV1 {
    ValidatedExactRoot,
    UnavailableLegacyCase,
    RejectedMismatch,
    RejectedWithoutValidatedRunContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionContinuationPressureMismatchV1 {
    field: String,
    expected: String,
    observed: String,
}

#[derive(Clone, Debug, Serialize)]
struct PotionContinuationPressureProjectionV1 {
    status: PotionContinuationPressureProjectionStatusV1,
    source: Option<&'static str>,
    attempt_index: Option<usize>,
    attempt_source: Option<String>,
    attempt_lane: Option<String>,
    mismatches: Vec<PotionContinuationPressureMismatchV1>,
    captured_pressure: Option<PotionContinuationPressureV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CombatVictoryContinuationProjectionStatusV1 {
    ValidatedCapturedFact,
    UnavailableLegacyCase,
    RejectedMismatch,
    RejectedWithoutValidatedRunContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct CombatVictoryContinuationMismatchV1 {
    field: String,
    expected: String,
    observed: String,
}

#[derive(Clone, Debug, Serialize)]
struct CombatVictoryContinuationProjectionV1 {
    status: CombatVictoryContinuationProjectionStatusV1,
    source: Option<&'static str>,
    attempt_index: Option<usize>,
    attempt_source: Option<String>,
    attempt_lane: Option<String>,
    mismatches: Vec<CombatVictoryContinuationMismatchV1>,
    captured_facts: Option<CombatVictoryContinuationFactsV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum StrategicHpQualityProjectionStatusV1 {
    ValidatedCapturedFact,
    UnavailableLegacyCase,
    RejectedMismatch,
    RejectedWithoutValidatedRunContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct StrategicHpQualityMismatchV1 {
    field: String,
    expected: String,
    observed: String,
}

#[derive(Clone, Debug, Serialize)]
struct StrategicHpQualityProjectionV1 {
    status: StrategicHpQualityProjectionStatusV1,
    source: Option<&'static str>,
    attempt_index: Option<usize>,
    attempt_source: Option<String>,
    attempt_lane: Option<String>,
    mismatches: Vec<StrategicHpQualityMismatchV1>,
    captured_facts: Option<CombatSearchStrategicHpQualityFactsV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PotionContinuationEvidenceCoverageV1 {
    ExactCurrentRoot,
    PartialCurrentRoot,
    PartialRunWindow,
    FutureUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionContinuationDependencyEvidenceV1 {
    potion_uuid: u32,
    potion_id: String,
    dependency: PotionContinuationDependencyV1,
    coverage: PotionContinuationEvidenceCoverageV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct PotionRetainedValueEvidenceV1 {
    run_context_status: PotionRunContinuationProjectionStatusV1,
    continuation_pressure_status: PotionContinuationPressureProjectionStatusV1,
    route_window_coverage: Option<RouteWindowCoverageKind>,
    validated_continuation_pressure: Option<PotionContinuationPressureV1>,
    exact_consumed_resources: Vec<PotionResourceV1>,
    unmatched_expenditure_uuids: Vec<u32>,
    dependency_evidence: Vec<PotionContinuationDependencyEvidenceV1>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct CombatCasePotionExpenditureAuditV14 {
    schema_name: &'static str,
    case: PathBuf,
    root_exact_state_hash: String,
    initial_hp: i32,
    initial_player_turn: u32,
    root_potions: Vec<PotionResourceV1>,
    production_context_reconstruction: ProductionContextReconstructionV1,
    continuation_context: PotionContinuationContextV1,
    continuation_pressure_projection: PotionContinuationPressureProjectionV1,
    combat_victory_continuation_projection: CombatVictoryContinuationProjectionV1,
    strategic_hp_quality_projection: StrategicHpQualityProjectionV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    export_witness_actions_dir: Option<PathBuf>,
    settings: PotionAuditSearchSettingsV1,
    lanes: Vec<PotionAuditLaneResultV1>,
    pareto_lane_ids: Vec<String>,
    limitations: PotionAuditLimitationsV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProductionContextReconstructionStatusV1 {
    NotAvailable,
    ValidatedExactRoot,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ProductionContextReconstructionV1 {
    status: ProductionContextReconstructionStatusV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub(super) fn run(
    args: CombatCasePotionExpenditureAuditArgs,
) -> Result<CombatCasePotionExpenditureAuditV14, String> {
    let CombatCasePotionExpenditureAuditArgs {
        case,
        max_combination_size,
        max_lanes,
        survival_reserve_hp,
        export_witness_actions_dir,
        restore_witness_actions,
        typed_plan_guide,
        authorized_root_potion_trial,
        include_discard_actions,
        max_nodes,
        max_hp_loss,
        max_selections,
        wall_ms_per_lane,
        max_engine_steps_per_transition,
        uniform_exploration_ppm,
        generation_quantum_work,
        max_turn_depth,
        expect_no_potion_min_final_hp,
        expect_no_potion_dominates_consuming,
    } = args;
    if max_lanes == 0 {
        return Err("potion audit max-lanes must be positive".to_owned());
    }
    if wall_ms_per_lane == 0 {
        return Err("potion audit wall-ms-per-lane must be positive".to_owned());
    }

    let loaded = load_combat_case(&case)?;
    let (reconstructed_context_trace, production_context_reconstruction) = if loaded
        .production_context
        .as_ref()
        .and_then(|context| context.production_owner.as_ref())
        .is_none()
    {
        (
            None,
            ProductionContextReconstructionV1 {
                status: ProductionContextReconstructionStatusV1::NotAvailable,
                error: None,
            },
        )
    } else {
        match restore_combat_case_oracle_analysis_owner_v2(
            &loaded.core,
            loaded.production_context.as_ref(),
        )
        .and_then(|(session, _)| reconstruct_oracle_combat_context_trace_v1(&session))
        {
            Ok(trace) => (
                Some(trace),
                ProductionContextReconstructionV1 {
                    status: ProductionContextReconstructionStatusV1::ValidatedExactRoot,
                    error: None,
                },
            ),
            Err(error) => (
                None,
                ProductionContextReconstructionV1 {
                    status: ProductionContextReconstructionStatusV1::Rejected,
                    error: Some(error),
                },
            ),
        }
    };
    let root = CombatDecisionRoot::new(loaded.core.position.clone())
        .map_err(|error| format!("invalid potion audit combat root: {error:?}"))?;
    let root_exact_state_hash = root.exact_state_hash().to_owned();
    let initial_hp = loaded.core.position.combat.entities.player.current_hp;
    let initial_player_turn = loaded.core.position.combat.turn.turn_count;
    let root_potions = root_potion_resources(&loaded.core.position)?;
    let run_level_projection = project_saved_run_continuation_context_with_reconstructed(
        &loaded,
        reconstructed_context_trace.as_ref(),
    );
    let continuation_pressure_projection =
        project_saved_potion_continuation_pressure_with_reconstructed(
            &loaded,
            &run_level_projection,
            reconstructed_context_trace.as_ref(),
        );
    let combat_victory_continuation_projection =
        project_saved_combat_victory_continuation_with_reconstructed(
            &loaded,
            &run_level_projection,
            reconstructed_context_trace.as_ref(),
        );
    let strategic_hp_quality_projection = project_saved_strategic_hp_quality_with_reconstructed(
        &loaded,
        &run_level_projection,
        &combat_victory_continuation_projection,
        reconstructed_context_trace.as_ref(),
    );
    let run_level_projection_for_evidence = run_level_projection.clone();
    let continuation_pressure_projection_for_evidence = continuation_pressure_projection.clone();
    let combat_victory_continuation_projection_for_evidence =
        combat_victory_continuation_projection.clone();
    let strategic_hp_quality_projection_for_evidence = strategic_hp_quality_projection.clone();
    let continuation_context = potion_continuation_context(
        loaded.core.run.act,
        loaded.core.run.floor,
        &loaded.core.position,
        run_level_projection,
    );
    let restored_witness = restore_witness_actions
        .as_deref()
        .map(|path| {
            load_exact_action_witness(&loaded.core.position, path, max_engine_steps_per_transition)
        })
        .transpose()?;
    let lane_specs = build_lane_specs(&root_potions, max_combination_size, max_lanes)?;
    let base_policy = retained_combat_tactical_policy_v1();
    let mut lanes = Vec::with_capacity(lane_specs.len());
    let mut evidence_manifest_entries = Vec::new();

    for lane in lane_specs {
        let mut lane_policy = base_policy.clone();
        if authorized_root_potion_trial && lane.allowed_slot_mask != 0 {
            lane_policy = authorized_potion_trial_policy_v1(
                lane_policy,
                loaded.core.position.clone(),
                lane.allowed_slot_mask,
            );
        }
        if typed_plan_guide {
            lane_policy = combat_plan_state_guide_policy_v1(lane_policy);
        }
        let search_spec = LocalGraphSearchSpec::from_controls(
            max_nodes,
            max_selections,
            wall_ms_per_lane,
            max_engine_steps_per_transition,
            uniform_exploration_ppm,
            generation_quantum_work,
            max_turn_depth,
            Some(lane.max_explicit_expenditures),
            include_discard_actions,
            Some(lane.allowed_slot_mask),
            None,
            None,
        );
        let satisfaction = max_hp_loss
            .map(OracleCombatWitnessSatisfaction::HpLossAtMost)
            .unwrap_or(OracleCombatWitnessSatisfaction::BudgetOrExhaustion);
        let config = search_spec.planner_config(satisfaction);
        let lane_root = CombatDecisionRoot::new(loaded.core.position.clone())
            .map_err(|error| format!("invalid potion audit lane root: {error:?}"))?;
        if lane_root.exact_state_hash() != root_exact_state_hash {
            return Err(format!(
                "potion audit lane '{}' did not preserve the exact root",
                lane.lane_id
            ));
        }
        let mut session = sts_combat_planner::LocalTurnGraphWitnessSession::with_policy(
            lane_root,
            config,
            lane_policy,
        );
        if let Some(witness) = restored_witness.as_ref() {
            session.restore_verified_witness(witness.clone())?;
        }
        let started = Instant::now();
        let report = session.advance(search_spec.quantum(), &EngineCombatStepper);
        let elapsed_ms = duration_millis_u64(started.elapsed());
        let exported_witness_actions =
            match (export_witness_actions_dir.as_ref(), report.witness.as_ref()) {
                (Some(directory), Some(witness)) => {
                    let path = directory.join(format!("{}.actions.json", lane.lane_id));
                    let actions = witness
                        .actions
                        .iter()
                        .map(|action| action.input.clone())
                        .collect::<Vec<_>>();
                    save_combat_inputs(&path, actions.iter().cloned())?;
                    evidence_manifest_entries.push(CombatEvidenceManifestEntryV1::from_actions(
                        lane.lane_id.clone(),
                        vec![path.clone()],
                        &actions,
                        CombatTerminal::Win,
                        Some(witness.final_position.combat.entities.player.current_hp),
                    )?);
                    Some(path)
                }
                _ => None,
            };
        let witness = report
            .witness
            .as_ref()
            .map(|witness| {
                summarize_witness(
                    &loaded.core.position,
                    witness.actions.as_slice(),
                    &witness.final_position,
                    initial_hp,
                    initial_player_turn,
                    lane.allowed_slot_mask,
                    lane.max_explicit_expenditures,
                    survival_reserve_hp,
                    max_engine_steps_per_transition,
                )
            })
            .transpose()?;
        lanes.push(PotionAuditLaneResultV1 {
            lane_id: lane.lane_id,
            allowed_slot_mask: lane.allowed_slot_mask,
            allowed_potions: lane.allowed_potions,
            max_explicit_expenditures: lane.max_explicit_expenditures,
            status: status_label(&report.status),
            elapsed_ms,
            counters: PotionAuditLaneCountersV1 {
                selections: report.counters.selections,
                generation_work: report.counters.generation_work,
                engine_steps: report.counters.engine_steps,
                exact_nodes: report.counters.exact_nodes,
                terminal_win_options: report.counters.terminal_win_options,
                witness_replay_attempts: report.counters.witness_replay_attempts,
                witness_replay_improvements: report.counters.witness_replay_improvements,
            },
            exported_witness_actions,
            witness,
        });
    }

    annotate_marginal_comparisons(&mut lanes, survival_reserve_hp);
    annotate_pareto_frontier(&mut lanes);
    annotate_shadow_spend_adjudications(
        &mut lanes,
        &root_potions,
        &run_level_projection_for_evidence,
        &continuation_pressure_projection_for_evidence,
        &combat_victory_continuation_projection_for_evidence,
        &strategic_hp_quality_projection_for_evidence,
    );
    annotate_policy_review_flags(&mut lanes);
    validate_expectations(
        &lanes,
        expect_no_potion_min_final_hp,
        expect_no_potion_dominates_consuming,
    )?;
    if let Some(directory) = export_witness_actions_dir
        .as_ref()
        .filter(|_| !evidence_manifest_entries.is_empty())
    {
        write_combat_evidence_manifest(
            &directory.join(COMBAT_EVIDENCE_MANIFEST_FILE_SUFFIX),
            CombatEvidenceProducerV1::PotionExpenditureAudit,
            root_exact_state_hash.clone(),
            case.clone(),
            evidence_manifest_entries,
        )?;
    }
    let pareto_lane_ids = lanes
        .iter()
        .filter_map(|lane| {
            lane.witness
                .as_ref()
                .filter(|witness| witness.pareto_frontier)
                .map(|_| lane.lane_id.clone())
        })
        .collect();

    Ok(CombatCasePotionExpenditureAuditV14 {
        schema_name: SCHEMA_NAME,
        case,
        root_exact_state_hash,
        initial_hp,
        initial_player_turn,
        root_potions,
        production_context_reconstruction,
        continuation_context,
        continuation_pressure_projection,
        combat_victory_continuation_projection,
        strategic_hp_quality_projection,
        export_witness_actions_dir,
        settings: PotionAuditSearchSettingsV1 {
            max_combination_size,
            max_lanes,
            survival_reserve_hp,
            typed_plan_guide,
            restore_witness_actions,
            authorized_root_potion_trial,
            include_discard_actions,
            max_nodes_per_lane: max_nodes,
            max_hp_loss,
            max_selections_per_lane: max_selections,
            wall_ms_per_lane,
            max_engine_steps_per_transition,
            uniform_exploration_ppm,
            generation_quantum_work,
            max_turn_depth,
            satisfaction: if max_hp_loss.is_some() {
                "hp_loss_at_most"
            } else {
                "budget_or_exhaustion"
            },
        },
        lanes,
        pareto_lane_ids,
        limitations: PotionAuditLimitationsV1 {
            lane_absence_is_budget_unknown_unless_frontier_exhausted: true,
            run_context_rejected_on_exact_root_mismatch: true,
            continuation_pressure_rejected_without_exact_reconstruction: true,
            combat_victory_continuation_requires_consistent_owner_capture: true,
            strategic_hp_quality_requires_consistent_owner_capture: true,
            retained_value_evidence_is_non_authoritative: true,
            continuation_value_not_in_combat_case: vec![
                "forced_rest_avoidance_beyond_route_window",
                "exact_future_encounter_sequence",
                "future_potion_reward_identity",
                "future_encounter_specific_counterplay",
            ],
            passive_consumption_handling:
                "replay-detected; a disallowed passive expenditure makes the lane non-compliant",
        },
    })
}

fn load_exact_action_witness(
    start: &CombatPosition,
    path: &std::path::Path,
    max_engine_steps_per_transition: usize,
) -> Result<OracleCombatWitness, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read restored witness actions: {error}"))?;
    let inputs: Vec<ClientInput> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse restored witness actions: {error}"))?;
    if inputs.is_empty() {
        return Err("restored witness action list is empty".to_owned());
    }

    let stepper = EngineCombatStepper;
    let mut position = start.clone();
    let mut actions = Vec::with_capacity(inputs.len());
    let mut replay_engine_steps = 0usize;
    for (index, input) in inputs.into_iter().enumerate() {
        if stepper.choice_for_legal_input(&position, &input).is_none() {
            return Err(format!(
                "restored witness action {index} is not legal at its exact state"
            ));
        }
        let result = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if result.truncated || result.timed_out {
            return Err(format!(
                "restored witness action {index} did not reach a stable state"
            ));
        }
        replay_engine_steps = replay_engine_steps.saturating_add(result.engine_steps);
        actions.push(TurnOptionAction {
            input,
            expected_successor_hash:
                sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
                    &result.position.engine,
                    &result.position.combat,
                )
                .into(),
            engine_steps: result.engine_steps,
        });
        position = result.position;
    }
    if stepper.terminal(&position) != CombatTerminal::Win {
        return Err("restored witness actions do not reach terminal victory".to_owned());
    }

    Ok(OracleCombatWitness {
        negative_log_policy: actions.len() as f64,
        actions,
        final_position: position,
        replay_engine_steps,
        discovery_source: OracleCombatWitnessDiscoverySource::RestoredExactActions,
    })
}

fn potion_continuation_context(
    act: u8,
    floor: i32,
    position: &CombatPosition,
    run_level_projection: PotionRunContinuationProjectionV1,
) -> PotionContinuationContextV1 {
    let combat = &position.combat;
    let player = &combat.entities.player;
    let deck = &combat.meta.master_deck_snapshot;
    let slot_capacity = combat.entities.potions.len();
    let occupied_slots = combat
        .entities
        .potions
        .iter()
        .filter(|slot| slot.is_some())
        .count();
    let empty_slots = slot_capacity.saturating_sub(occupied_slots);
    let current_combat_stake = if combat.meta.is_boss_fight {
        PotionCurrentCombatStakeV1::Boss
    } else if combat.meta.is_elite_fight {
        PotionCurrentCombatStakeV1::Elite
    } else {
        PotionCurrentCombatStakeV1::Normal
    };
    let has_relic = |id| player.relics.iter().any(|relic| relic.id == id);
    let strategic_facts = RunStrategicFacts {
        entering_act: act,
        starter_basic_count: deck.iter().filter(|card| is_starter_basic(card.id)).count(),
        curse_count: deck
            .iter()
            .filter(|card| get_card_definition(card.id).card_type == CardType::Curse)
            .count(),
        has_energy_relic: player
            .relics
            .iter()
            .any(|relic| energy_master_delta(relic.id) > 0),
        has_runic_pyramid: has_relic(RelicId::RunicPyramid),
    };
    let unavailable_future_context = vec![
        PotionContinuationUnknownV1::NextEncounterIdentity,
        PotionContinuationUnknownV1::RouteBeforeNextEliteOrBoss,
        PotionContinuationUnknownV1::FuturePotionDropRollAndIdentity,
        PotionContinuationUnknownV1::FuturePotionReplacementCandidate,
        PotionContinuationUnknownV1::FutureHandAndDrawOrder,
        PotionContinuationUnknownV1::FutureRestSiteAvailability,
    ];

    PotionContinuationContextV1 {
        act,
        floor,
        current_combat_stake,
        current_hp: player.current_hp,
        max_hp: player.max_hp,
        deck_size: deck.len(),
        inventory: PotionInventoryPressureV1 {
            slot_capacity,
            occupied_slots,
            empty_slots,
            inventory_full: empty_slots == 0,
            new_potion_would_require_replacement_if_obtainable: empty_slots == 0,
        },
        relics: PotionRelicContextV1 {
            sacred_bark: has_relic(RelicId::SacredBark),
            toy_ornithopter: has_relic(RelicId::ToyOrnithopter),
            white_beast_statue: has_relic(RelicId::WhiteBeastStatue),
            sozu: has_relic(RelicId::Sozu),
            potion_belt: has_relic(RelicId::PotionBelt),
        },
        deck_strategic_deficit: assess_deck_strategic_deficit(deck, strategic_facts),
        run_level_projection,
        unavailable_future_context,
    }
}

struct SelectedContextTrace<'a> {
    source: &'static str,
    attempt_index: Option<usize>,
    trace: &'a AtomicCombatSearchTraceSummaryV2,
}

fn select_context_trace<'a>(
    case: &'a CombatCase,
    reconstructed: Option<&'a AtomicCombatSearchTraceSummaryV2>,
    has_fact: impl Fn(&AtomicCombatSearchTraceSummaryV2) -> bool,
) -> Option<SelectedContextTrace<'a>> {
    if let Some((index, trace)) = case
        .atomic_combat_search_attempts
        .iter()
        .enumerate()
        .rev()
        .find(|(_, trace)| has_fact(trace))
    {
        return Some(SelectedContextTrace {
            source: "atomic_combat_search_attempts",
            attempt_index: Some(index),
            trace,
        });
    }
    if let Some(trace) = case
        .failed_atomic_combat_search
        .as_ref()
        .filter(|trace| has_fact(trace))
    {
        return Some(SelectedContextTrace {
            source: "failed_atomic_combat_search",
            attempt_index: None,
            trace,
        });
    }
    reconstructed
        .filter(|trace| has_fact(trace))
        .map(|trace| SelectedContextTrace {
            source: "reconstructed_production_context",
            attempt_index: None,
            trace,
        })
}

#[cfg(test)]
fn project_saved_run_continuation_context(case: &CombatCase) -> PotionRunContinuationProjectionV1 {
    project_saved_run_continuation_context_with_reconstructed(case, None)
}

#[cfg(test)]
fn project_saved_potion_continuation_pressure(
    case: &CombatCase,
    run_level_projection: &PotionRunContinuationProjectionV1,
) -> PotionContinuationPressureProjectionV1 {
    project_saved_potion_continuation_pressure_with_reconstructed(case, run_level_projection, None)
}

#[cfg(test)]
fn project_saved_combat_victory_continuation(
    case: &CombatCase,
    run_level_projection: &PotionRunContinuationProjectionV1,
) -> CombatVictoryContinuationProjectionV1 {
    project_saved_combat_victory_continuation_with_reconstructed(case, run_level_projection, None)
}

#[cfg(test)]
fn project_saved_strategic_hp_quality(
    case: &CombatCase,
    run_level_projection: &PotionRunContinuationProjectionV1,
    combat_victory_projection: &CombatVictoryContinuationProjectionV1,
) -> StrategicHpQualityProjectionV1 {
    project_saved_strategic_hp_quality_with_reconstructed(
        case,
        run_level_projection,
        combat_victory_projection,
        None,
    )
}

fn project_saved_run_continuation_context_with_reconstructed(
    case: &CombatCase,
    reconstructed: Option<&AtomicCombatSearchTraceSummaryV2>,
) -> PotionRunContinuationProjectionV1 {
    let Some(selected) = select_context_trace(case, reconstructed, |trace| {
        trace.potion_continuation_context.is_some()
    }) else {
        return PotionRunContinuationProjectionV1 {
            status: PotionRunContinuationProjectionStatusV1::UnavailableLegacyCase,
            source: None,
            attempt_index: None,
            attempt_source: None,
            attempt_lane: None,
            mismatches: Vec::new(),
            captured_context: None,
        };
    };
    let attempt = selected.trace;
    let context = attempt
        .potion_continuation_context
        .as_ref()
        .expect("filtered continuation context")
        .clone();
    let mut mismatches = validate_saved_run_continuation_context(case, &context);
    let conflicting_contexts = case
        .atomic_combat_search_attempts
        .iter()
        .filter_map(|attempt| attempt.potion_continuation_context.as_ref())
        .chain(
            case.failed_atomic_combat_search
                .as_ref()
                .and_then(|attempt| attempt.potion_continuation_context.as_ref()),
        )
        .filter(|other| *other != &context)
        .count();
    if conflicting_contexts > 0 {
        mismatches.push(PotionRunContinuationMismatchV1 {
            field: "trace_context_consistency",
            expected: "all captured contexts identical".to_owned(),
            observed: format!("{conflicting_contexts} conflicting context(s)"),
        });
    }
    let status = if mismatches.is_empty() {
        PotionRunContinuationProjectionStatusV1::ValidatedExactRoot
    } else {
        PotionRunContinuationProjectionStatusV1::RejectedRootMismatch
    };

    PotionRunContinuationProjectionV1 {
        status,
        source: Some(selected.source),
        attempt_index: selected.attempt_index,
        attempt_source: Some(attempt.source.clone()),
        attempt_lane: attempt.lane.clone(),
        mismatches,
        captured_context: Some(context),
    }
}

fn project_saved_potion_continuation_pressure_with_reconstructed(
    case: &CombatCase,
    run_level_projection: &PotionRunContinuationProjectionV1,
    reconstructed: Option<&AtomicCombatSearchTraceSummaryV2>,
) -> PotionContinuationPressureProjectionV1 {
    let Some(selected) = select_context_trace(case, reconstructed, |trace| {
        trace.potion_continuation_pressure.is_some()
    }) else {
        return PotionContinuationPressureProjectionV1 {
            status: PotionContinuationPressureProjectionStatusV1::UnavailableLegacyCase,
            source: None,
            attempt_index: None,
            attempt_source: None,
            attempt_lane: None,
            mismatches: Vec::new(),
            captured_pressure: None,
        };
    };
    let attempt = selected.trace;
    let pressure = attempt
        .potion_continuation_pressure
        .as_ref()
        .expect("filtered continuation pressure")
        .clone();
    let mut mismatches = Vec::new();
    let all_attempts = case
        .atomic_combat_search_attempts
        .iter()
        .chain(case.failed_atomic_combat_search.iter());
    let missing_pressures = all_attempts
        .clone()
        .filter(|other| other.potion_continuation_pressure.is_none())
        .count();
    if missing_pressures > 0 {
        mismatches.push(PotionContinuationPressureMismatchV1 {
            field: "trace_pressure_presence_consistency".to_owned(),
            expected: "pressure present on every saved search summary".to_owned(),
            observed: format!("{missing_pressures} summary or summaries without pressure"),
        });
    }
    let conflicting_pressures = all_attempts
        .filter_map(|other| other.potion_continuation_pressure.as_ref())
        .filter(|other| *other != &pressure)
        .count();
    if conflicting_pressures > 0 {
        mismatches.push(PotionContinuationPressureMismatchV1 {
            field: "trace_pressure_consistency".to_owned(),
            expected: "all captured pressures identical".to_owned(),
            observed: format!("{conflicting_pressures} conflicting pressure(s)"),
        });
    }

    let status = if run_level_projection.status
        != PotionRunContinuationProjectionStatusV1::ValidatedExactRoot
    {
        mismatches.push(PotionContinuationPressureMismatchV1 {
            field: "run_context_status".to_owned(),
            expected: "validated_exact_root".to_owned(),
            observed: run_context_projection_status_label(run_level_projection.status).to_owned(),
        });
        PotionContinuationPressureProjectionStatusV1::RejectedWithoutValidatedRunContext
    } else {
        let context = run_level_projection
            .captured_context
            .as_ref()
            .expect("validated run continuation context");
        let expected = potion_continuation_pressure_from_context_v1(
            context,
            PotionContinuationPressureInputsV1 {
                current_gold: case.core.run.gold,
                coffee_dripper_blocks_rest: case
                    .core
                    .position
                    .combat
                    .entities
                    .player
                    .relics
                    .iter()
                    .any(|relic| relic.id == RelicId::CoffeeDripper),
            },
        );
        validate_saved_potion_continuation_pressure(&mut mismatches, &expected, &pressure);
        if mismatches.is_empty() {
            PotionContinuationPressureProjectionStatusV1::ValidatedExactRoot
        } else {
            PotionContinuationPressureProjectionStatusV1::RejectedMismatch
        }
    };

    PotionContinuationPressureProjectionV1 {
        status,
        source: Some(selected.source),
        attempt_index: selected.attempt_index,
        attempt_source: Some(attempt.source.clone()),
        attempt_lane: attempt.lane.clone(),
        mismatches,
        captured_pressure: Some(pressure),
    }
}

fn project_saved_combat_victory_continuation_with_reconstructed(
    case: &CombatCase,
    run_level_projection: &PotionRunContinuationProjectionV1,
    reconstructed: Option<&AtomicCombatSearchTraceSummaryV2>,
) -> CombatVictoryContinuationProjectionV1 {
    let Some(selected) = select_context_trace(case, reconstructed, |trace| {
        trace.combat_victory_continuation.is_some()
    }) else {
        return CombatVictoryContinuationProjectionV1 {
            status: CombatVictoryContinuationProjectionStatusV1::UnavailableLegacyCase,
            source: None,
            attempt_index: None,
            attempt_source: None,
            attempt_lane: None,
            mismatches: Vec::new(),
            captured_facts: None,
        };
    };
    let attempt = selected.trace;
    let facts = attempt
        .combat_victory_continuation
        .as_ref()
        .expect("filtered combat victory continuation")
        .clone();
    let all_attempts = case
        .atomic_combat_search_attempts
        .iter()
        .chain(case.failed_atomic_combat_search.iter());
    let mut mismatches = Vec::new();
    let missing_facts = all_attempts
        .clone()
        .filter(|other| other.combat_victory_continuation.is_none())
        .count();
    if missing_facts > 0 {
        mismatches.push(CombatVictoryContinuationMismatchV1 {
            field: "trace_fact_presence_consistency".to_owned(),
            expected: "fact present on every saved search summary".to_owned(),
            observed: format!("{missing_facts} summary or summaries without the fact"),
        });
    }
    let conflicting_facts = all_attempts
        .filter_map(|other| other.combat_victory_continuation.as_ref())
        .filter(|other| *other != &facts)
        .count();
    if conflicting_facts > 0 {
        mismatches.push(CombatVictoryContinuationMismatchV1 {
            field: "trace_fact_consistency".to_owned(),
            expected: "all captured facts identical".to_owned(),
            observed: format!("{conflicting_facts} conflicting fact(s)"),
        });
    }
    if facts.evaluator != COMBAT_VICTORY_CONTINUATION_EVALUATOR_V1 {
        mismatches.push(CombatVictoryContinuationMismatchV1 {
            field: "evaluator".to_owned(),
            expected: COMBAT_VICTORY_CONTINUATION_EVALUATOR_V1.to_owned(),
            observed: facts.evaluator.clone(),
        });
    }

    let status = if run_level_projection.status
        != PotionRunContinuationProjectionStatusV1::ValidatedExactRoot
    {
        mismatches.push(CombatVictoryContinuationMismatchV1 {
            field: "run_context_status".to_owned(),
            expected: "validated_exact_root".to_owned(),
            observed: run_context_projection_status_label(run_level_projection.status).to_owned(),
        });
        CombatVictoryContinuationProjectionStatusV1::RejectedWithoutValidatedRunContext
    } else {
        if facts.hp_carryover
            == CombatVictoryHpCarryoverV1::GuaranteedFullHealBeforeNextDamageBearingDecision
        {
            let context = run_level_projection
                .captured_context
                .as_ref()
                .expect("validated run continuation context");
            if !case.core.position.combat.meta.is_boss_fight {
                mismatches.push(CombatVictoryContinuationMismatchV1 {
                    field: "combat_kind".to_owned(),
                    expected: "boss".to_owned(),
                    observed: "non_boss".to_owned(),
                });
            }
            if context.act >= 3 {
                mismatches.push(CombatVictoryContinuationMismatchV1 {
                    field: "act".to_owned(),
                    expected: "act below 3".to_owned(),
                    observed: context.act.to_string(),
                });
            }
            if case.core.position.combat.meta.ascension_level >= 5 {
                mismatches.push(CombatVictoryContinuationMismatchV1 {
                    field: "ascension".to_owned(),
                    expected: "ascension below 5".to_owned(),
                    observed: case.core.position.combat.meta.ascension_level.to_string(),
                });
            }
        }
        if mismatches.is_empty() {
            CombatVictoryContinuationProjectionStatusV1::ValidatedCapturedFact
        } else {
            CombatVictoryContinuationProjectionStatusV1::RejectedMismatch
        }
    };

    CombatVictoryContinuationProjectionV1 {
        status,
        source: Some(selected.source),
        attempt_index: selected.attempt_index,
        attempt_source: Some(attempt.source.clone()),
        attempt_lane: attempt.lane.clone(),
        mismatches,
        captured_facts: Some(facts),
    }
}

fn project_saved_strategic_hp_quality_with_reconstructed(
    case: &CombatCase,
    run_level_projection: &PotionRunContinuationProjectionV1,
    combat_victory_projection: &CombatVictoryContinuationProjectionV1,
    reconstructed: Option<&AtomicCombatSearchTraceSummaryV2>,
) -> StrategicHpQualityProjectionV1 {
    let Some(selected) = select_context_trace(case, reconstructed, |trace| {
        trace.strategic_hp_quality.is_some()
    }) else {
        return StrategicHpQualityProjectionV1 {
            status: StrategicHpQualityProjectionStatusV1::UnavailableLegacyCase,
            source: None,
            attempt_index: None,
            attempt_source: None,
            attempt_lane: None,
            mismatches: Vec::new(),
            captured_facts: None,
        };
    };
    let attempt = selected.trace;
    let facts = attempt
        .strategic_hp_quality
        .as_ref()
        .expect("filtered strategic HP quality")
        .clone();
    let all_attempts = case
        .atomic_combat_search_attempts
        .iter()
        .chain(case.failed_atomic_combat_search.iter());
    let mut mismatches = Vec::new();
    let missing_facts = all_attempts
        .clone()
        .filter(|other| other.strategic_hp_quality.is_none())
        .count();
    if missing_facts > 0 {
        mismatches.push(StrategicHpQualityMismatchV1 {
            field: "trace_fact_presence_consistency".to_owned(),
            expected: "fact present on every saved search summary".to_owned(),
            observed: format!("{missing_facts} summary or summaries without the fact"),
        });
    }
    let conflicting_facts = all_attempts
        .filter_map(|other| other.strategic_hp_quality.as_ref())
        .filter(|other| *other != &facts)
        .count();
    if conflicting_facts > 0 {
        mismatches.push(StrategicHpQualityMismatchV1 {
            field: "trace_fact_consistency".to_owned(),
            expected: "all captured facts identical".to_owned(),
            observed: format!("{conflicting_facts} conflicting fact(s)"),
        });
    }
    if facts.survival_evaluator != COMBAT_SURVIVAL_HP_LIMIT_EVALUATOR_V1 {
        mismatches.push(StrategicHpQualityMismatchV1 {
            field: "survival_evaluator".to_owned(),
            expected: COMBAT_SURVIVAL_HP_LIMIT_EVALUATOR_V1.to_owned(),
            observed: facts.survival_evaluator.clone(),
        });
    }
    if facts.quality_evaluator != COMBAT_QUALITY_HP_LIMIT_EVALUATOR_V1 {
        mismatches.push(StrategicHpQualityMismatchV1 {
            field: "quality_evaluator".to_owned(),
            expected: COMBAT_QUALITY_HP_LIMIT_EVALUATOR_V1.to_owned(),
            observed: facts.quality_evaluator.clone(),
        });
    }
    let root_player = &case.core.position.combat.entities.player;
    if facts.entry_current_hp != root_player.current_hp {
        mismatches.push(StrategicHpQualityMismatchV1 {
            field: "entry_current_hp".to_owned(),
            expected: root_player.current_hp.to_string(),
            observed: facts.entry_current_hp.to_string(),
        });
    }
    if facts.entry_max_hp != root_player.max_hp {
        mismatches.push(StrategicHpQualityMismatchV1 {
            field: "entry_max_hp".to_owned(),
            expected: root_player.max_hp.to_string(),
            observed: facts.entry_max_hp.to_string(),
        });
    }
    match (facts.survival_hp_loss_limit, facts.quality_hp_loss_limit) {
        (
            CombatSearchHpLossLimitV1::Limited {
                max_hp_loss: survival,
            },
            CombatSearchHpLossLimitV1::Limited {
                max_hp_loss: quality,
            },
        ) if quality > survival => {
            mismatches.push(StrategicHpQualityMismatchV1 {
                field: "quality_hp_loss_limit".to_owned(),
                expected: format!("at most survival limit {survival}"),
                observed: quality.to_string(),
            });
        }
        (CombatSearchHpLossLimitV1::Unlimited, CombatSearchHpLossLimitV1::Unlimited)
        | (CombatSearchHpLossLimitV1::Limited { .. }, CombatSearchHpLossLimitV1::Limited { .. }) => {
        }
        (survival, quality) => {
            mismatches.push(StrategicHpQualityMismatchV1 {
                field: "hp_loss_limit_kind_consistency".to_owned(),
                expected: "survival and quality limits both limited or both unlimited".to_owned(),
                observed: format!("survival={survival:?}, quality={quality:?}"),
            });
        }
    }
    if combat_victory_projection.status
        == CombatVictoryContinuationProjectionStatusV1::ValidatedCapturedFact
        && combat_victory_projection
            .captured_facts
            .as_ref()
            .is_some_and(|victory| {
                victory.hp_carryover
                    == CombatVictoryHpCarryoverV1::GuaranteedFullHealBeforeNextDamageBearingDecision
            })
        && (facts.survival_hp_loss_limit != CombatSearchHpLossLimitV1::Unlimited
            || facts.quality_hp_loss_limit != CombatSearchHpLossLimitV1::Unlimited)
    {
        mismatches.push(StrategicHpQualityMismatchV1 {
            field: "guaranteed_full_heal_limits".to_owned(),
            expected: "survival and quality limits both unlimited".to_owned(),
            observed: format!(
                "survival={:?}, quality={:?}",
                facts.survival_hp_loss_limit, facts.quality_hp_loss_limit
            ),
        });
    }

    let status = if run_level_projection.status
        != PotionRunContinuationProjectionStatusV1::ValidatedExactRoot
    {
        mismatches.push(StrategicHpQualityMismatchV1 {
            field: "run_context_status".to_owned(),
            expected: "validated_exact_root".to_owned(),
            observed: run_context_projection_status_label(run_level_projection.status).to_owned(),
        });
        StrategicHpQualityProjectionStatusV1::RejectedWithoutValidatedRunContext
    } else if mismatches.is_empty() {
        StrategicHpQualityProjectionStatusV1::ValidatedCapturedFact
    } else {
        StrategicHpQualityProjectionStatusV1::RejectedMismatch
    };

    StrategicHpQualityProjectionV1 {
        status,
        source: Some(selected.source),
        attempt_index: selected.attempt_index,
        attempt_source: Some(attempt.source.clone()),
        attempt_lane: attempt.lane.clone(),
        mismatches,
        captured_facts: Some(facts),
    }
}

fn run_context_projection_status_label(
    status: PotionRunContinuationProjectionStatusV1,
) -> &'static str {
    match status {
        PotionRunContinuationProjectionStatusV1::ValidatedExactRoot => "validated_exact_root",
        PotionRunContinuationProjectionStatusV1::UnavailableLegacyCase => "unavailable_legacy_case",
        PotionRunContinuationProjectionStatusV1::RejectedRootMismatch => "rejected_root_mismatch",
    }
}

fn validate_saved_potion_continuation_pressure(
    mismatches: &mut Vec<PotionContinuationPressureMismatchV1>,
    expected: &PotionContinuationPressureV1,
    observed: &PotionContinuationPressureV1,
) {
    push_pressure_mismatch(
        mismatches,
        "schema_name",
        &expected.schema_name,
        &observed.schema_name,
    );
    push_pressure_mismatch(
        mismatches,
        "schema_version",
        &expected.schema_version,
        &observed.schema_version,
    );
    push_pressure_mismatch(
        mismatches,
        "capture_boundary",
        &expected.capture_boundary,
        &observed.capture_boundary,
    );
    push_pressure_mismatch(mismatches, "act", &expected.act, &observed.act);
    push_pressure_mismatch(mismatches, "floor", &expected.floor, &observed.floor);
    push_pressure_mismatch(
        mismatches,
        "visible_boss",
        &expected.visible_boss,
        &observed.visible_boss,
    );
    push_pressure_mismatch(
        mismatches,
        "inventory",
        &expected.inventory,
        &observed.inventory,
    );
    push_pressure_mismatch(mismatches, "supply", &expected.supply, &observed.supply);
    push_pressure_mismatch(mismatches, "route", &expected.route, &observed.route);
    push_pressure_mismatch(mismatches, "shop", &expected.shop, &observed.shop);
    push_pressure_mismatch(
        mismatches,
        "recovery",
        &expected.recovery,
        &observed.recovery,
    );
    push_pressure_mismatch(
        mismatches,
        "limitations",
        &expected.limitations,
        &observed.limitations,
    );
}

fn push_pressure_mismatch<T>(
    mismatches: &mut Vec<PotionContinuationPressureMismatchV1>,
    field: &'static str,
    expected: &T,
    observed: &T,
) where
    T: PartialEq + std::fmt::Debug,
{
    if expected != observed {
        mismatches.push(PotionContinuationPressureMismatchV1 {
            field: field.to_owned(),
            expected: format!("{expected:?}"),
            observed: format!("{observed:?}"),
        });
    }
}

fn validate_saved_run_continuation_context(
    case: &CombatCase,
    context: &PotionRunContinuationContextV1,
) -> Vec<PotionRunContinuationMismatchV1> {
    let combat = &case.core.position.combat;
    let expected_occupied_slots = combat
        .entities
        .potions
        .iter()
        .filter(|slot| slot.is_some())
        .count();
    let mut mismatches = Vec::new();
    push_context_mismatch(
        &mut mismatches,
        "schema_name",
        POTION_RUN_CONTINUATION_CONTEXT_SCHEMA_NAME,
        context.schema_name.as_str(),
    );
    push_context_mismatch(
        &mut mismatches,
        "schema_version",
        POTION_RUN_CONTINUATION_CONTEXT_SCHEMA_VERSION,
        context.schema_version,
    );
    push_context_mismatch(
        &mut mismatches,
        "capture_boundary",
        "before_atomic_combat_search",
        context.capture_boundary.as_str(),
    );
    push_context_mismatch(&mut mismatches, "act", case.core.run.act, context.act);
    push_context_mismatch(&mut mismatches, "floor", case.core.run.floor, context.floor);
    push_context_mismatch(
        &mut mismatches,
        "current_hp",
        combat.entities.player.current_hp,
        context.current_hp,
    );
    push_context_mismatch(
        &mut mismatches,
        "max_hp",
        combat.entities.player.max_hp,
        context.max_hp,
    );
    push_context_mismatch(
        &mut mismatches,
        "slot_capacity",
        combat.entities.potions.len(),
        context.inventory.slot_capacity,
    );
    push_context_mismatch(
        &mut mismatches,
        "occupied_slots",
        expected_occupied_slots,
        context.inventory.occupied_slots,
    );
    mismatches
}

fn push_context_mismatch<T>(
    mismatches: &mut Vec<PotionRunContinuationMismatchV1>,
    field: &'static str,
    expected: T,
    observed: T,
) where
    T: PartialEq + ToString,
{
    if expected != observed {
        mismatches.push(PotionRunContinuationMismatchV1 {
            field,
            expected: expected.to_string(),
            observed: observed.to_string(),
        });
    }
}

fn root_potion_resources(position: &CombatPosition) -> Result<Vec<PotionResourceV1>, String> {
    position
        .combat
        .entities
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| potion.as_ref().map(|potion| (slot, potion)))
        .map(|(slot, potion)| {
            if slot >= u64::BITS as usize {
                return Err(format!(
                    "potion slot {slot} exceeds the audit's 64-slot exact mask"
                ));
            }
            Ok(potion_resource(slot, potion))
        })
        .collect()
}

fn potion_resource(slot: usize, potion: &Potion) -> PotionResourceV1 {
    let shared_strategy_traits = potion_acquisition_traits_v1(potion.id);
    let shared_strategy_coverage = if shared_strategy_traits.is_empty() {
        PotionSharedStrategyCoverageV1::Unclassified
    } else {
        PotionSharedStrategyCoverageV1::Classified
    };
    PotionResourceV1 {
        slot,
        id: potion.id.stable_id().to_string(),
        uuid: potion.uuid,
        can_use: potion.can_use,
        can_discard: potion.can_discard,
        verified_win_rescue_tier: potion_rescue_tier(potion.id),
        shared_strategy_traits,
        shared_strategy_coverage,
        mechanical_role: potion_mechanical_role(potion.id),
        continuation_dependencies: potion_continuation_dependencies(potion.id),
    }
}

fn potion_mechanical_role(id: PotionId) -> PotionMechanicalRoleV1 {
    use PotionId as Id;
    use PotionMechanicalRoleV1 as Role;
    match id {
        Id::FirePotion => Role::SingleTargetDamage,
        Id::ExplosivePotion => Role::MultiTargetDamage,
        Id::PoisonPotion => Role::DamageOverTime,
        Id::WeakenPotion => Role::WeakControl,
        Id::FearPotion => Role::VulnerableControl,
        Id::BlockPotion => Role::ImmediateBlock,
        Id::BloodPotion => Role::ImmediateHealing,
        Id::EnergyPotion => Role::EnergyBurst,
        Id::StrengthPotion => Role::PersistentStrength,
        Id::DexterityPotion => Role::PersistentDexterity,
        Id::SpeedPotion => Role::TemporaryDexterity,
        Id::SteroidPotion => Role::TemporaryStrength,
        Id::SwiftPotion => Role::CardDraw,
        Id::FocusPotion => Role::PersistentFocus,
        Id::AttackPotion => Role::RandomAttackDiscovery,
        Id::SkillPotion => Role::RandomSkillDiscovery,
        Id::PowerPotion => Role::RandomPowerDiscovery,
        Id::ColorlessPotion => Role::RandomColorlessDiscovery,
        Id::BottledMiracle => Role::MiracleGeneration,
        Id::BlessingOfTheForge => Role::TemporaryUpgrade,
        Id::AncientPotion => Role::Artifact,
        Id::RegenPotion => Role::DelayedHealing,
        Id::EssenceOfSteel => Role::PlatedArmor,
        Id::LiquidBronze => Role::Thorns,
        Id::DistilledChaosPotion => Role::RandomTopdeckPlay,
        Id::DuplicationPotion => Role::NextCardDuplication,
        Id::CunningPotion => Role::ShivGeneration,
        Id::PotionOfCapacity => Role::OrbCapacity,
        Id::LiquidMemories => Role::DiscardRecovery,
        Id::GamblersBrew => Role::HandRedraw,
        Id::Elixir => Role::HandExhaust,
        Id::StancePotion => Role::StanceControl,
        Id::FairyPotion => Role::DeathInsurance,
        Id::SmokeBomb => Role::Escape,
        Id::FruitJuice => Role::MaxHpGain,
        Id::EntropicBrew => Role::PotionGeneration,
        Id::SneckoOil => Role::CardDrawAndCostRandomization,
        Id::GhostInAJar => Role::Intangible,
        Id::HeartOfIron => Role::Metallicize,
        Id::CultistPotion => Role::RitualScaling,
        Id::Ambrosia => Role::Divinity,
        Id::EssenceOfDarkness => Role::DarkOrbGeneration,
    }
}

fn potion_continuation_dependencies(id: PotionId) -> Vec<PotionContinuationDependencyV1> {
    use PotionContinuationDependencyV1 as Dependency;
    use PotionId as Id;
    match id {
        Id::FirePotion | Id::ExplosivePotion => {
            vec![Dependency::FutureEnemyCountAndHealth]
        }
        Id::PoisonPotion => vec![
            Dependency::FutureEnemyCountAndHealth,
            Dependency::FutureFightLength,
        ],
        Id::WeakenPotion => vec![
            Dependency::FutureEncounterDamagePattern,
            Dependency::FutureFightLength,
        ],
        Id::FearPotion => vec![
            Dependency::FutureEnemyCountAndHealth,
            Dependency::FutureFightLength,
        ],
        Id::BlockPotion | Id::GhostInAJar => {
            vec![Dependency::FutureEncounterDamagePattern]
        }
        Id::BloodPotion => vec![Dependency::CurrentHpDeficit, Dependency::OutOfCombatTiming],
        Id::EnergyPotion | Id::SwiftPotion | Id::BottledMiracle => {
            vec![Dependency::FutureHandAndDrawOrder]
        }
        Id::StrengthPotion | Id::DexterityPotion => {
            vec![Dependency::FutureFightLength, Dependency::DeckSynergy]
        }
        Id::SpeedPotion | Id::SteroidPotion => vec![
            Dependency::FutureHandAndDrawOrder,
            Dependency::DeckSynergy,
            Dependency::DebuffTiming,
        ],
        Id::FocusPotion => vec![
            Dependency::FutureFightLength,
            Dependency::DeckSynergy,
            Dependency::OrbPlan,
        ],
        Id::AttackPotion | Id::SkillPotion | Id::PowerPotion | Id::ColorlessPotion => vec![
            Dependency::FutureHandAndDrawOrder,
            Dependency::DeckSynergy,
            Dependency::RandomOutcomePool,
        ],
        Id::BlessingOfTheForge => vec![
            Dependency::FutureHandAndDrawOrder,
            Dependency::HighValueCardTarget,
        ],
        Id::AncientPotion => vec![Dependency::DebuffTiming, Dependency::DeckSynergy],
        Id::RegenPotion => vec![Dependency::CurrentHpDeficit, Dependency::FutureFightLength],
        Id::EssenceOfSteel | Id::LiquidBronze | Id::HeartOfIron => vec![
            Dependency::FutureEncounterDamagePattern,
            Dependency::FutureFightLength,
        ],
        Id::DistilledChaosPotion | Id::GamblersBrew | Id::SneckoOil => {
            vec![Dependency::FutureHandAndDrawOrder]
        }
        Id::DuplicationPotion => vec![
            Dependency::FutureHandAndDrawOrder,
            Dependency::DeckSynergy,
            Dependency::HighValueCardTarget,
        ],
        Id::CunningPotion => vec![Dependency::FutureHandAndDrawOrder, Dependency::DeckSynergy],
        Id::PotionOfCapacity | Id::EssenceOfDarkness => {
            vec![Dependency::DeckSynergy, Dependency::OrbPlan]
        }
        Id::LiquidMemories => vec![
            Dependency::FutureHandAndDrawOrder,
            Dependency::FutureDiscardState,
            Dependency::DeckSynergy,
            Dependency::HighValueCardTarget,
        ],
        Id::Elixir => vec![Dependency::FutureHandAndDrawOrder, Dependency::DeckSynergy],
        Id::StancePotion | Id::Ambrosia => {
            vec![Dependency::FutureHandAndDrawOrder, Dependency::StancePlan]
        }
        Id::FairyPotion => vec![Dependency::LowHpInsuranceNeed],
        Id::SmokeBomb => vec![Dependency::RouteEscapeValue],
        Id::FruitJuice => vec![Dependency::OutOfCombatTiming],
        Id::EntropicBrew => vec![Dependency::EmptyPotionSlotsAndAcquisitionRules],
        Id::CultistPotion => vec![Dependency::FutureFightLength],
    }
}

fn potion_rescue_tier(id: PotionId) -> PotionVerifiedWinRescueTierV1 {
    match oracle_potion_rescue_tier_v1(id) {
        OraclePotionRescueTierV1::BoundedQuality => PotionVerifiedWinRescueTierV1::BoundedQuality,
        OraclePotionRescueTierV1::FindAnyWin => PotionVerifiedWinRescueTierV1::FindAnyWin,
        OraclePotionRescueTierV1::Excluded => PotionVerifiedWinRescueTierV1::Excluded,
    }
}

fn build_lane_specs(
    resources: &[PotionResourceV1],
    max_combination_size: usize,
    max_lanes: usize,
) -> Result<Vec<PotionAuditLaneSpec>, String> {
    let mut lanes = vec![PotionAuditLaneSpec {
        lane_id: "no_potion".to_owned(),
        allowed_slot_mask: 0,
        allowed_potions: Vec::new(),
        max_explicit_expenditures: 0,
    }];
    let largest = max_combination_size.min(resources.len());
    for size in 1..=largest {
        let mut subsets = Vec::new();
        collect_resource_subsets(resources, size, 0, &mut Vec::new(), &mut subsets);
        for subset in subsets {
            let allowed_slot_mask = subset
                .iter()
                .fold(0_u64, |mask, resource| mask | (1_u64 << resource.slot));
            let slots = subset
                .iter()
                .map(|resource| resource.slot.to_string())
                .collect::<Vec<_>>()
                .join("_");
            let ids = subset
                .iter()
                .map(|resource| snake_case_debug_name(&resource.id))
                .collect::<Vec<_>>()
                .join("_");
            lanes.push(PotionAuditLaneSpec {
                lane_id: format!("slots_{slots}_{ids}"),
                allowed_slot_mask,
                max_explicit_expenditures: subset.len().try_into().unwrap_or(u32::MAX),
                allowed_potions: subset,
            });
        }
    }
    if lanes.len() > max_lanes {
        return Err(format!(
            "potion audit would create {} lanes, exceeding --max-lanes {max_lanes}",
            lanes.len()
        ));
    }
    Ok(lanes)
}

fn collect_resource_subsets(
    resources: &[PotionResourceV1],
    remaining: usize,
    start: usize,
    current: &mut Vec<PotionResourceV1>,
    output: &mut Vec<Vec<PotionResourceV1>>,
) {
    if remaining == 0 {
        output.push(current.clone());
        return;
    }
    let final_start = resources.len().saturating_sub(remaining);
    for index in start..=final_start {
        current.push(resources[index].clone());
        collect_resource_subsets(resources, remaining - 1, index + 1, current, output);
        current.pop();
    }
}

fn snake_case_debug_name(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 4);
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            output.push('_');
        }
        output.push(character.to_ascii_lowercase());
    }
    output
}

#[allow(clippy::too_many_arguments)]
fn summarize_witness(
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    final_position: &CombatPosition,
    initial_hp: i32,
    initial_player_turn: u32,
    allowed_slot_mask: u64,
    max_explicit_expenditures: u32,
    survival_reserve_hp: Option<i32>,
    max_engine_steps_per_transition: usize,
) -> Result<PotionAuditWitnessV1, String> {
    let potion_expenditures =
        replay_potion_expenditures(root, actions, max_engine_steps_per_transition)?;
    let explicit_potion_action_count = potion_expenditures
        .iter()
        .filter(|event| event.mode != PotionExpenditureModeV1::Passive)
        .count();
    let verified_win_potion_disposition = verified_win_potion_disposition(&potion_expenditures);
    let all_slots_allowed = potion_expenditures.iter().all(|event| {
        event.slot < u64::BITS as usize && allowed_slot_mask & (1_u64 << event.slot) != 0
    });
    let lane_compliant =
        all_slots_allowed && potion_expenditures.len() <= max_explicit_expenditures as usize;
    let final_hp = final_position.combat.entities.player.current_hp;
    let final_player_turn = final_position.combat.turn.turn_count;
    Ok(PotionAuditWitnessV1 {
        final_hp,
        hp_loss: initial_hp.saturating_sub(final_hp),
        final_player_turn,
        turns_elapsed: final_player_turn.saturating_sub(initial_player_turn),
        action_count: actions.len(),
        explicit_potion_action_count,
        potion_expenditures,
        verified_win_potion_disposition,
        policy_review_flags: Vec::new(),
        lane_compliant,
        meets_survival_reserve: survival_reserve_hp.map(|reserve| final_hp >= reserve),
        relative_to_no_potion: None,
        pareto_frontier: false,
        dominated_by: Vec::new(),
        shadow_spend_adjudication: None,
    })
}

fn replay_potion_expenditures(
    root: &CombatPosition,
    actions: &[TurnOptionAction],
    max_engine_steps_per_transition: usize,
) -> Result<Vec<PotionExpenditureEventV1>, String> {
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut events = Vec::new();
    for (action_index, action) in actions.iter().enumerate() {
        let before = position.combat.entities.potions.clone();
        let explicit_slot = match action.input {
            ClientInput::UsePotion { potion_index, .. } => {
                Some((potion_index, PotionExpenditureModeV1::Use))
            }
            ClientInput::DiscardPotion(slot) => Some((slot, PotionExpenditureModeV1::Discard)),
            _ => None,
        };
        let explicit_uuid = if let Some((slot, mode)) = explicit_slot {
            let potion = before.get(slot).and_then(Option::as_ref).ok_or_else(|| {
                format!("verified witness potion action {action_index} refers to empty slot {slot}")
            })?;
            events.push(PotionExpenditureEventV1 {
                action_index,
                slot,
                id: potion.id.stable_id().to_string(),
                uuid: potion.uuid,
                mode,
                verified_win_rescue_tier: potion_rescue_tier(potion.id),
            });
            Some(potion.uuid)
        } else {
            None
        };
        let result = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if result.truncated {
            return Err(format!(
                "potion audit replay exceeded the transition limit at action {action_index}"
            ));
        }
        for (slot, potion) in before
            .iter()
            .enumerate()
            .filter_map(|(slot, potion)| potion.as_ref().map(|potion| (slot, potion)))
        {
            let remains = result
                .position
                .combat
                .entities
                .potions
                .iter()
                .flatten()
                .any(|after| after.uuid == potion.uuid);
            if !remains && explicit_uuid != Some(potion.uuid) {
                events.push(PotionExpenditureEventV1 {
                    action_index,
                    slot,
                    id: potion.id.stable_id().to_string(),
                    uuid: potion.uuid,
                    mode: PotionExpenditureModeV1::Passive,
                    verified_win_rescue_tier: potion_rescue_tier(potion.id),
                });
            }
        }
        position = result.position;
    }
    Ok(events)
}

fn verified_win_potion_disposition(
    events: &[PotionExpenditureEventV1],
) -> VerifiedWinPotionDispositionV1 {
    if events
        .iter()
        .any(|event| event.verified_win_rescue_tier == PotionVerifiedWinRescueTierV1::Excluded)
    {
        VerifiedWinPotionDispositionV1::ContainsExcludedResource
    } else if events
        .iter()
        .any(|event| event.verified_win_rescue_tier == PotionVerifiedWinRescueTierV1::FindAnyWin)
    {
        VerifiedWinPotionDispositionV1::ContainsReservedResource
    } else if events.is_empty() {
        VerifiedWinPotionDispositionV1::NoPotionSpent
    } else {
        VerifiedWinPotionDispositionV1::BoundedQualityOnly
    }
}

fn annotate_marginal_comparisons(
    lanes: &mut [PotionAuditLaneResultV1],
    survival_reserve_hp: Option<i32>,
) {
    let baseline_lane = lanes.iter().find(|lane| lane.lane_id == "no_potion");
    let baseline_frontier_exhausted =
        baseline_lane.is_some_and(|lane| lane.status == "frontier_exhausted");
    let baseline = baseline_lane
        .and_then(|lane| lane.witness.as_ref())
        .map(|witness| {
            (
                witness.final_hp,
                witness.final_player_turn,
                witness.action_count,
                witness.meets_survival_reserve,
            )
        });
    for lane in lanes {
        let Some(witness) = lane.witness.as_mut() else {
            continue;
        };
        if lane.lane_id == "no_potion" {
            witness.relative_to_no_potion = Some(PotionMarginalComparisonV1 {
                final_hp_delta: Some(0),
                final_turn_delta: Some(0),
                action_count_delta: Some(0),
                survival_reserve_delta: survival_reserve_hp.map(|reserve_hp| {
                    survival_reserve_delta(witness.final_hp, witness.final_hp, reserve_hp)
                }),
                assessment: PotionMarginalAssessmentV1::NoPotionBaseline,
            });
            continue;
        }
        let Some((base_hp, base_turn, base_actions, base_meets_reserve)) = baseline else {
            witness.relative_to_no_potion = Some(PotionMarginalComparisonV1 {
                final_hp_delta: None,
                final_turn_delta: None,
                action_count_delta: None,
                survival_reserve_delta: None,
                assessment: if baseline_frontier_exhausted {
                    PotionMarginalAssessmentV1::NoPotionFrontierExhaustedUnderContract
                } else {
                    PotionMarginalAssessmentV1::NoPotionWitnessNotFoundUnderAllowance
                },
            });
            continue;
        };
        let consumes_potion = !witness.potion_expenditures.is_empty();
        let assessment = if survival_reserve_hp.is_some()
            && base_meets_reserve == Some(false)
            && witness.meets_survival_reserve == Some(true)
        {
            PotionMarginalAssessmentV1::CrossesSurvivalReserve
        } else if !consumes_potion {
            PotionMarginalAssessmentV1::NoAdditionalPotionConsumed
        } else if witness.final_hp > base_hp {
            PotionMarginalAssessmentV1::ImprovesFinalHp
        } else if witness.final_hp == base_hp {
            PotionMarginalAssessmentV1::SameFinalHpWithExtraResource
        } else {
            PotionMarginalAssessmentV1::WorseFinalHpWithExtraResource
        };
        witness.relative_to_no_potion = Some(PotionMarginalComparisonV1 {
            final_hp_delta: Some(witness.final_hp.saturating_sub(base_hp)),
            final_turn_delta: Some(i64::from(witness.final_player_turn) - i64::from(base_turn)),
            action_count_delta: Some(witness.action_count as i64 - base_actions as i64),
            survival_reserve_delta: survival_reserve_hp
                .map(|reserve_hp| survival_reserve_delta(base_hp, witness.final_hp, reserve_hp)),
            assessment,
        });
    }
}

fn survival_reserve_delta(
    baseline_final_hp: i32,
    candidate_final_hp: i32,
    reserve_hp: i32,
) -> PotionSurvivalReserveDeltaV1 {
    let baseline_shortfall_hp = reserve_hp.saturating_sub(baseline_final_hp).max(0);
    let candidate_shortfall_hp = reserve_hp.saturating_sub(candidate_final_hp).max(0);
    let shortfall_reduction_hp = baseline_shortfall_hp.saturating_sub(candidate_shortfall_hp);
    let shortfall_reduction_ppm = (baseline_shortfall_hp > 0).then(|| {
        let numerator = i64::from(shortfall_reduction_hp).saturating_mul(1_000_000);
        let ppm = numerator / i64::from(baseline_shortfall_hp);
        ppm.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    });
    PotionSurvivalReserveDeltaV1 {
        reserve_hp,
        baseline_shortfall_hp,
        candidate_shortfall_hp,
        shortfall_reduction_hp,
        shortfall_reduction_ppm,
    }
}

fn annotate_pareto_frontier(lanes: &mut [PotionAuditLaneResultV1]) {
    let snapshots = lanes
        .iter()
        .filter_map(|lane| {
            lane.witness.as_ref().map(|witness| {
                (
                    lane.lane_id.clone(),
                    witness.final_hp,
                    witness.final_player_turn,
                    witness.action_count,
                    expenditure_identity_set(&witness.potion_expenditures),
                    witness.lane_compliant,
                )
            })
        })
        .collect::<Vec<_>>();
    for lane in lanes {
        let Some(witness) = lane.witness.as_mut() else {
            continue;
        };
        if !witness.lane_compliant {
            continue;
        }
        let target_resources = expenditure_identity_set(&witness.potion_expenditures);
        witness.dominated_by = snapshots
            .iter()
            .filter(|(other_id, ..)| other_id != &lane.lane_id)
            .filter(|(_, _, _, _, _, compliant)| *compliant)
            .filter(
                |(_, other_hp, other_turn, other_actions, other_resources, _)| {
                    dominates(
                        *other_hp,
                        *other_turn,
                        *other_actions,
                        other_resources,
                        witness.final_hp,
                        witness.final_player_turn,
                        witness.action_count,
                        &target_resources,
                    )
                },
            )
            .map(|(other_id, ..)| other_id.clone())
            .collect();
        witness.pareto_frontier = witness.dominated_by.is_empty();
    }
}

fn annotate_shadow_spend_adjudications(
    lanes: &mut [PotionAuditLaneResultV1],
    root_potions: &[PotionResourceV1],
    run_level_projection: &PotionRunContinuationProjectionV1,
    continuation_pressure_projection: &PotionContinuationPressureProjectionV1,
    combat_victory_continuation_projection: &CombatVictoryContinuationProjectionV1,
    strategic_hp_quality_projection: &StrategicHpQualityProjectionV1,
) {
    let no_potion_final_hp = lanes
        .iter()
        .find(|lane| lane.lane_id == "no_potion")
        .and_then(|lane| lane.witness.as_ref())
        .map(|witness| witness.final_hp);
    for lane in lanes {
        let Some(witness) = lane.witness.as_mut() else {
            continue;
        };
        let adjudication = if lane.lane_id == "no_potion" {
            PotionSpendAdjudicationV1::NoPotionBaseline
        } else if witness.potion_expenditures.is_empty() {
            PotionSpendAdjudicationV1::NoAdditionalPotionConsumed
        } else {
            let comparison = witness.relative_to_no_potion.as_ref();
            let baseline_frontier_exhausted = comparison.is_some_and(|comparison| {
                comparison.assessment
                    == PotionMarginalAssessmentV1::NoPotionFrontierExhaustedUnderContract
            });
            let Some((final_hp_delta, final_turn_delta)) = comparison
                .and_then(|comparison| comparison.final_hp_delta.zip(comparison.final_turn_delta))
            else {
                witness.shadow_spend_adjudication =
                    Some(PotionSpendAdjudicationV1::UnknownWithoutNoPotionWitness {
                        baseline_frontier_exhausted,
                    });
                continue;
            };
            let comparison = comparison.expect("marginal comparison with HP and turn deltas");
            if !witness.lane_compliant
                || witness.verified_win_potion_disposition
                    == VerifiedWinPotionDispositionV1::ContainsExcludedResource
            {
                PotionSpendAdjudicationV1::ExcludedFromVictorySpend
            } else if !witness.pareto_frontier {
                PotionSpendAdjudicationV1::RejectDominated {
                    dominated_by: witness.dominated_by.clone(),
                }
            } else if final_hp_delta <= 0 {
                PotionSpendAdjudicationV1::RejectNonPositiveHpGain { final_hp_delta }
            } else {
                let baseline_final_hp = no_potion_final_hp
                    .or_else(|| {
                        comparison
                            .final_hp_delta
                            .map(|delta| witness.final_hp.saturating_sub(delta))
                    })
                    .expect("comparison with exact no-potion HP delta");
                PotionSpendAdjudicationV1::CompareContinuationValue {
                    immediate_hp_gain: final_hp_delta,
                    break_even_retained_value_hp: final_hp_delta,
                    final_turn_delta,
                    potion_expenditures: witness.potion_expenditures.len(),
                    spend_urgency_question: spend_urgency_question(
                        comparison,
                        baseline_final_hp,
                        witness.final_hp,
                        run_level_projection,
                        continuation_pressure_projection,
                        combat_victory_continuation_projection,
                        strategic_hp_quality_projection,
                    ),
                    retained_value_evidence: retained_value_evidence(
                        witness,
                        root_potions,
                        run_level_projection,
                        continuation_pressure_projection,
                    ),
                }
            }
        };
        witness.shadow_spend_adjudication = Some(adjudication);
    }
}

fn spend_urgency_question(
    comparison: &PotionMarginalComparisonV1,
    baseline_final_hp: i32,
    candidate_final_hp: i32,
    run_level_projection: &PotionRunContinuationProjectionV1,
    continuation_pressure_projection: &PotionContinuationPressureProjectionV1,
    combat_victory_continuation_projection: &CombatVictoryContinuationProjectionV1,
    strategic_hp_quality_projection: &StrategicHpQualityProjectionV1,
) -> PotionSpendUrgencyQuestionV1 {
    use CombatVictoryContinuationProjectionStatusV1 as VictoryStatus;
    use PotionContinuationPressureProjectionStatusV1 as PressureStatus;
    use PotionRunContinuationProjectionStatusV1 as RunStatus;
    use PotionSpendUrgencyQuestionLimitationV1 as Limitation;
    use PotionSpendUrgencyQuestionStatusV1 as Status;
    use StrategicHpQualityProjectionStatusV1 as QualityStatus;

    let mut limitations = Vec::new();
    match run_level_projection.status {
        RunStatus::ValidatedExactRoot => {}
        RunStatus::UnavailableLegacyCase => limitations.push(Limitation::RunContextUnavailable),
        RunStatus::RejectedRootMismatch => limitations.push(Limitation::RunContextRejected),
    }
    match continuation_pressure_projection.status {
        PressureStatus::ValidatedExactRoot => {}
        PressureStatus::UnavailableLegacyCase => {
            limitations.push(Limitation::ContinuationPressureUnavailable);
        }
        PressureStatus::RejectedMismatch | PressureStatus::RejectedWithoutValidatedRunContext => {
            limitations.push(Limitation::ContinuationPressureRejected);
        }
    }
    match combat_victory_continuation_projection.status {
        VictoryStatus::ValidatedCapturedFact => {}
        VictoryStatus::UnavailableLegacyCase => {
            limitations.push(Limitation::CombatVictoryContinuationUnavailable);
        }
        VictoryStatus::RejectedMismatch | VictoryStatus::RejectedWithoutValidatedRunContext => {
            limitations.push(Limitation::CombatVictoryContinuationRejected);
        }
    }
    match strategic_hp_quality_projection.status {
        QualityStatus::ValidatedCapturedFact => {}
        QualityStatus::UnavailableLegacyCase => {
            limitations.push(Limitation::StrategicHpQualityUnavailable);
        }
        QualityStatus::RejectedMismatch | QualityStatus::RejectedWithoutValidatedRunContext => {
            limitations.push(Limitation::StrategicHpQualityRejected);
        }
    }

    let rejected = matches!(run_level_projection.status, RunStatus::RejectedRootMismatch)
        || matches!(
            continuation_pressure_projection.status,
            PressureStatus::RejectedMismatch | PressureStatus::RejectedWithoutValidatedRunContext
        )
        || matches!(
            combat_victory_continuation_projection.status,
            VictoryStatus::RejectedMismatch | VictoryStatus::RejectedWithoutValidatedRunContext
        )
        || matches!(
            strategic_hp_quality_projection.status,
            QualityStatus::RejectedMismatch | QualityStatus::RejectedWithoutValidatedRunContext
        );
    if rejected {
        return PotionSpendUrgencyQuestionV1 {
            status: Status::Rejected,
            run_context_status: run_level_projection.status,
            continuation_pressure_status: continuation_pressure_projection.status,
            facts: None,
            limitations,
        };
    }
    if run_level_projection.status != RunStatus::ValidatedExactRoot
        || continuation_pressure_projection.status != PressureStatus::ValidatedExactRoot
    {
        return PotionSpendUrgencyQuestionV1 {
            status: Status::Unavailable,
            run_context_status: run_level_projection.status,
            continuation_pressure_status: continuation_pressure_projection.status,
            facts: None,
            limitations,
        };
    }

    let Some(context) = run_level_projection.captured_context.as_ref() else {
        limitations.push(Limitation::ValidatedRunContextMissingPayload);
        return PotionSpendUrgencyQuestionV1 {
            status: Status::Unavailable,
            run_context_status: run_level_projection.status,
            continuation_pressure_status: continuation_pressure_projection.status,
            facts: None,
            limitations,
        };
    };
    let Some(pressure) = continuation_pressure_projection.captured_pressure.as_ref() else {
        limitations.push(Limitation::ValidatedContinuationPressureMissingPayload);
        return PotionSpendUrgencyQuestionV1 {
            status: Status::Unavailable,
            run_context_status: run_level_projection.status,
            continuation_pressure_status: continuation_pressure_projection.status,
            facts: None,
            limitations,
        };
    };
    if combat_victory_continuation_projection.status == VictoryStatus::ValidatedCapturedFact
        && combat_victory_continuation_projection
            .captured_facts
            .is_none()
    {
        limitations.push(Limitation::ValidatedCombatVictoryContinuationMissingPayload);
        return PotionSpendUrgencyQuestionV1 {
            status: Status::Unavailable,
            run_context_status: run_level_projection.status,
            continuation_pressure_status: continuation_pressure_projection.status,
            facts: None,
            limitations,
        };
    }
    if strategic_hp_quality_projection.status == QualityStatus::ValidatedCapturedFact
        && strategic_hp_quality_projection.captured_facts.is_none()
    {
        limitations.push(Limitation::ValidatedStrategicHpQualityMissingPayload);
        return PotionSpendUrgencyQuestionV1 {
            status: Status::Unavailable,
            run_context_status: run_level_projection.status,
            continuation_pressure_status: continuation_pressure_projection.status,
            facts: None,
            limitations,
        };
    }

    PotionSpendUrgencyQuestionV1 {
        status: Status::ValidatedExactRoot,
        run_context_status: run_level_projection.status,
        continuation_pressure_status: continuation_pressure_projection.status,
        facts: Some(PotionSpendUrgencyFactsV1 {
            configured_survival_reserve_delta: comparison.survival_reserve_delta.clone(),
            combat_victory_continuation: combat_victory_continuation_evidence(
                combat_victory_continuation_projection,
            ),
            strategic_hp_quality: strategic_hp_quality_evidence(
                strategic_hp_quality_projection,
                baseline_final_hp,
                candidate_final_hp,
            ),
            inventory: pressure.inventory.clone(),
            supply: pressure.supply.clone(),
            route: pressure.route.clone(),
            route_ordering: route_ordering_facts(context),
            recovery: pressure.recovery.clone(),
            shop: pressure.shop.clone(),
            current_combat_reward_size_gate_unknown: context
                .limitations
                .contains(&PotionRunContinuationLimitationV1::CurrentCombatRewardSizeGateUnknown),
            future_potion_identity_unknown: context
                .limitations
                .contains(&PotionRunContinuationLimitationV1::FuturePotionIdentityUnknownUntilRoll),
        }),
        limitations,
    }
}

fn combat_victory_continuation_evidence(
    projection: &CombatVictoryContinuationProjectionV1,
) -> PotionCombatVictoryContinuationEvidenceV1 {
    if projection.status == CombatVictoryContinuationProjectionStatusV1::ValidatedCapturedFact {
        let facts = projection
            .captured_facts
            .as_ref()
            .expect("validated combat victory continuation fact");
        PotionCombatVictoryContinuationEvidenceV1::ValidatedCapturedFact {
            evaluator: facts.evaluator.clone(),
            hp_carryover: facts.hp_carryover,
        }
    } else {
        PotionCombatVictoryContinuationEvidenceV1::UnavailableLegacyCase
    }
}

fn strategic_hp_quality_evidence(
    projection: &StrategicHpQualityProjectionV1,
    baseline_final_hp: i32,
    candidate_final_hp: i32,
) -> PotionStrategicHpQualityEvidenceV1 {
    if projection.status == StrategicHpQualityProjectionStatusV1::ValidatedCapturedFact {
        let facts = projection
            .captured_facts
            .as_ref()
            .expect("validated strategic HP quality fact");
        let baseline_policy_hp_loss = facts
            .entry_current_hp
            .saturating_sub(baseline_final_hp)
            .max(0) as u32;
        let candidate_policy_hp_loss = facts
            .entry_current_hp
            .saturating_sub(candidate_final_hp)
            .max(0) as u32;
        PotionStrategicHpQualityEvidenceV1::ValidatedCapturedFact {
            survival_evaluator: facts.survival_evaluator.clone(),
            quality_evaluator: facts.quality_evaluator.clone(),
            entry_current_hp: facts.entry_current_hp,
            entry_max_hp: facts.entry_max_hp,
            baseline_final_hp,
            candidate_final_hp,
            survival: hp_loss_limit_assessment(
                facts.survival_hp_loss_limit,
                baseline_policy_hp_loss,
                candidate_policy_hp_loss,
            ),
            quality: hp_loss_limit_assessment(
                facts.quality_hp_loss_limit,
                baseline_policy_hp_loss,
                candidate_policy_hp_loss,
            ),
        }
    } else {
        PotionStrategicHpQualityEvidenceV1::UnavailableLegacyCase
    }
}

fn hp_loss_limit_assessment(
    limit: CombatSearchHpLossLimitV1,
    baseline_policy_hp_loss: u32,
    candidate_policy_hp_loss: u32,
) -> PotionHpLossLimitAssessmentV1 {
    let satisfies = |hp_loss| match limit {
        CombatSearchHpLossLimitV1::Limited { max_hp_loss } => hp_loss <= max_hp_loss,
        CombatSearchHpLossLimitV1::Unlimited => true,
    };
    let baseline_satisfies = satisfies(baseline_policy_hp_loss);
    let candidate_satisfies = satisfies(candidate_policy_hp_loss);
    PotionHpLossLimitAssessmentV1 {
        limit,
        baseline_policy_hp_loss,
        candidate_policy_hp_loss,
        baseline_satisfies,
        candidate_satisfies,
        candidate_crosses_from_unsatisfied_to_satisfied: !baseline_satisfies && candidate_satisfies,
    }
}

fn route_ordering_facts(context: &PotionRunContinuationContextV1) -> PotionRouteOrderingFactsV1 {
    PotionRouteOrderingFactsV1 {
        coverage_kind: context.route_window.coverage.kind,
        window_starts_after_current_decision: context
            .route_window
            .cursor
            .starts_after_current_decision,
        future_known_combat_before_campfire: route_order_evidence(
            context,
            RouteWindowSubject::KnownCombat,
            RouteWindowSubject::Campfire,
        ),
        future_known_combat_before_shop: route_order_evidence(
            context,
            RouteWindowSubject::KnownCombat,
            RouteWindowSubject::Shop,
        ),
        future_elite_before_campfire: route_order_evidence(
            context,
            RouteWindowSubject::Elite,
            RouteWindowSubject::Campfire,
        ),
        future_campfire_before_elite: route_order_evidence(
            context,
            RouteWindowSubject::Campfire,
            RouteWindowSubject::Elite,
        ),
        future_shop_before_known_combat: route_order_evidence(
            context,
            RouteWindowSubject::Shop,
            RouteWindowSubject::KnownCombat,
        ),
    }
}

fn route_order_evidence(
    context: &PotionRunContinuationContextV1,
    subject: RouteWindowSubject,
    before: RouteWindowSubject,
) -> PotionRouteOrderEvidenceV1 {
    let matches = context
        .route_window
        .facts
        .iter()
        .filter(|fact| fact.predicate == (RouteWindowPredicate::OccursBefore { subject, before }))
        .collect::<Vec<_>>();
    let [fact] = matches.as_slice() else {
        return PotionRouteOrderEvidenceV1::Unavailable {
            reason: if matches.is_empty() {
                PotionRouteOrderUnavailableReasonV1::MissingTypedFact
            } else {
                PotionRouteOrderUnavailableReasonV1::ConflictingTypedFacts
            },
            observed_modality: None,
            provenance: None,
            horizon_nodes: None,
        };
    };
    if fact.modality == RouteWindowModality::Unknown {
        return PotionRouteOrderEvidenceV1::Unavailable {
            reason: PotionRouteOrderUnavailableReasonV1::UnknownModality,
            observed_modality: Some(fact.modality),
            provenance: Some(fact.provenance),
            horizon_nodes: Some(fact.horizon_nodes),
        };
    }
    PotionRouteOrderEvidenceV1::Validated {
        modality: fact.modality,
        provenance: fact.provenance,
        horizon_nodes: fact.horizon_nodes,
    }
}

fn retained_value_evidence(
    witness: &PotionAuditWitnessV1,
    root_potions: &[PotionResourceV1],
    run_level_projection: &PotionRunContinuationProjectionV1,
    continuation_pressure_projection: &PotionContinuationPressureProjectionV1,
) -> PotionRetainedValueEvidenceV1 {
    let run_context_status = run_level_projection.status;
    let continuation_pressure_status = continuation_pressure_projection.status;
    let validated_continuation_pressure = continuation_pressure_projection
        .captured_pressure
        .as_ref()
        .filter(|_| {
            continuation_pressure_status
                == PotionContinuationPressureProjectionStatusV1::ValidatedExactRoot
        })
        .cloned();
    let route_window_coverage = validated_continuation_pressure
        .as_ref()
        .map(|pressure| pressure.route.coverage_kind);
    let expenditure_uuids = witness
        .potion_expenditures
        .iter()
        .map(|event| event.uuid)
        .collect::<BTreeSet<_>>();
    let exact_consumed_resources = root_potions
        .iter()
        .filter(|resource| expenditure_uuids.contains(&resource.uuid))
        .cloned()
        .collect::<Vec<_>>();
    let matched_uuids = exact_consumed_resources
        .iter()
        .map(|resource| resource.uuid)
        .collect::<BTreeSet<_>>();
    let unmatched_expenditure_uuids = expenditure_uuids
        .difference(&matched_uuids)
        .copied()
        .collect::<Vec<_>>();
    let dependency_evidence = exact_consumed_resources
        .iter()
        .flat_map(|resource| {
            resource
                .continuation_dependencies
                .iter()
                .copied()
                .map(|dependency| PotionContinuationDependencyEvidenceV1 {
                    potion_uuid: resource.uuid,
                    potion_id: resource.id.clone(),
                    dependency,
                    coverage: continuation_dependency_coverage(
                        dependency,
                        run_context_status,
                        route_window_coverage,
                    ),
                })
        })
        .collect();

    PotionRetainedValueEvidenceV1 {
        run_context_status,
        continuation_pressure_status,
        route_window_coverage,
        validated_continuation_pressure,
        exact_consumed_resources,
        unmatched_expenditure_uuids,
        dependency_evidence,
    }
}

fn continuation_dependency_coverage(
    dependency: PotionContinuationDependencyV1,
    run_context_status: PotionRunContinuationProjectionStatusV1,
    route_window_coverage: Option<RouteWindowCoverageKind>,
) -> PotionContinuationEvidenceCoverageV1 {
    use PotionContinuationDependencyV1 as Dependency;
    use PotionContinuationEvidenceCoverageV1 as Coverage;

    match dependency {
        Dependency::CurrentHpDeficit => Coverage::ExactCurrentRoot,
        Dependency::DeckSynergy
        | Dependency::HighValueCardTarget
        | Dependency::LowHpInsuranceNeed
        | Dependency::OrbPlan
        | Dependency::StancePlan => Coverage::PartialCurrentRoot,
        Dependency::EmptyPotionSlotsAndAcquisitionRules
            if run_context_status
                == PotionRunContinuationProjectionStatusV1::ValidatedExactRoot =>
        {
            Coverage::PartialRunWindow
        }
        Dependency::EmptyPotionSlotsAndAcquisitionRules => Coverage::PartialCurrentRoot,
        Dependency::RouteEscapeValue
            if route_window_coverage
                .is_some_and(|kind| kind != RouteWindowCoverageKind::UnavailableMap) =>
        {
            Coverage::PartialRunWindow
        }
        Dependency::FutureEncounterDamagePattern
        | Dependency::FutureEnemyCountAndHealth
        | Dependency::FutureFightLength
        | Dependency::FutureHandAndDrawOrder
        | Dependency::FutureDiscardState
        | Dependency::RandomOutcomePool
        | Dependency::DebuffTiming
        | Dependency::RouteEscapeValue
        | Dependency::OutOfCombatTiming => Coverage::FutureUnknown,
    }
}

fn annotate_policy_review_flags(lanes: &mut [PotionAuditLaneResultV1]) {
    for lane in lanes {
        let Some(witness) = lane.witness.as_mut() else {
            continue;
        };
        let assessment = witness
            .relative_to_no_potion
            .as_ref()
            .map(|comparison| comparison.assessment);
        match witness.verified_win_potion_disposition {
            VerifiedWinPotionDispositionV1::NoPotionSpent => {}
            VerifiedWinPotionDispositionV1::BoundedQualityOnly => {
                if !witness.pareto_frontier {
                    witness
                        .policy_review_flags
                        .push(PotionPolicyReviewFlagV1::AdmittedResourceIsParetoDominated);
                }
                if matches!(
                    assessment,
                    Some(
                        PotionMarginalAssessmentV1::SameFinalHpWithExtraResource
                            | PotionMarginalAssessmentV1::WorseFinalHpWithExtraResource
                    )
                ) {
                    witness
                        .policy_review_flags
                        .push(PotionPolicyReviewFlagV1::AdmittedResourceHasNoHpBenefit);
                }
            }
            VerifiedWinPotionDispositionV1::ContainsReservedResource => {
                if assessment == Some(PotionMarginalAssessmentV1::CrossesSurvivalReserve) {
                    witness
                        .policy_review_flags
                        .push(PotionPolicyReviewFlagV1::ReservedResourceCrossesSurvivalReserve);
                } else if assessment == Some(PotionMarginalAssessmentV1::ImprovesFinalHp) {
                    witness.policy_review_flags.push(
                        PotionPolicyReviewFlagV1::ReservedResourceImprovesHpWithoutCrossingReserve,
                    );
                }
            }
            VerifiedWinPotionDispositionV1::ContainsExcludedResource => {
                witness
                    .policy_review_flags
                    .push(PotionPolicyReviewFlagV1::ExcludedResourceConsumed);
            }
        }
        let delayed_regen = witness
            .potion_expenditures
            .iter()
            .any(|event| event.id == "RegenPotion")
            && witness
                .relative_to_no_potion
                .as_ref()
                .and_then(|comparison| comparison.final_turn_delta)
                .is_some_and(|delta| delta > 0);
        if delayed_regen {
            witness
                .policy_review_flags
                .push(PotionPolicyReviewFlagV1::DelayedHealRequiresExtraTurns);
        }
    }
}

fn expenditure_identity_set(events: &[PotionExpenditureEventV1]) -> BTreeSet<u32> {
    events.iter().map(|event| event.uuid).collect()
}

#[allow(clippy::too_many_arguments)]
fn dominates(
    left_hp: i32,
    left_turn: u32,
    left_actions: usize,
    left_resources: &BTreeSet<u32>,
    right_hp: i32,
    right_turn: u32,
    right_actions: usize,
    right_resources: &BTreeSet<u32>,
) -> bool {
    let no_worse = left_hp >= right_hp
        && left_turn <= right_turn
        && left_actions <= right_actions
        && left_resources.is_subset(right_resources);
    let strictly_better = left_hp > right_hp
        || left_turn < right_turn
        || left_actions < right_actions
        || left_resources != right_resources;
    no_worse && strictly_better
}

fn validate_expectations(
    lanes: &[PotionAuditLaneResultV1],
    expect_no_potion_min_final_hp: Option<i32>,
    expect_no_potion_dominates_consuming: bool,
) -> Result<(), String> {
    let no_potion = lanes
        .iter()
        .find(|lane| lane.lane_id == "no_potion")
        .and_then(|lane| lane.witness.as_ref());
    if let Some(expected) = expect_no_potion_min_final_hp {
        let actual = no_potion
            .filter(|witness| witness.lane_compliant)
            .map(|witness| witness.final_hp)
            .ok_or_else(|| {
                "expected a compliant no-potion witness, but none was found".to_owned()
            })?;
        if actual < expected {
            return Err(format!(
                "no-potion final HP {actual} is below expected minimum {expected}"
            ));
        }
    }
    if expect_no_potion_dominates_consuming {
        let no_potion = no_potion
            .filter(|witness| witness.lane_compliant)
            .ok_or_else(|| {
                "cannot assert dominance without a compliant no-potion witness".to_owned()
            })?;
        let no_potion_resources = expenditure_identity_set(&no_potion.potion_expenditures);
        let consuming = lanes
            .iter()
            .filter_map(|lane| {
                lane.witness
                    .as_ref()
                    .filter(|witness| {
                        witness.lane_compliant && !witness.potion_expenditures.is_empty()
                    })
                    .map(|witness| (lane.lane_id.as_str(), witness))
            })
            .collect::<Vec<_>>();
        if consuming.is_empty() {
            return Err(
                "expected consuming witnesses to compare, but no compliant lane consumed a potion"
                    .to_owned(),
            );
        }
        let failures = consuming
            .into_iter()
            .filter_map(|(lane_id, witness)| {
                (!dominates(
                    no_potion.final_hp,
                    no_potion.final_player_turn,
                    no_potion.action_count,
                    &no_potion_resources,
                    witness.final_hp,
                    witness.final_player_turn,
                    witness.action_count,
                    &expenditure_identity_set(&witness.potion_expenditures),
                ))
                .then_some(lane_id)
            })
            .collect::<Vec<_>>();
        if !failures.is_empty() {
            return Err(format!(
                "no-potion witness does not dominate consuming lanes: {}",
                failures.join(", ")
            ));
        }
    }
    Ok(())
}

fn status_label(status: &LocalTurnGraphWitnessStatus) -> String {
    match status {
        LocalTurnGraphWitnessStatus::WitnessFound => "witness_found".to_owned(),
        LocalTurnGraphWitnessStatus::FrontierExhausted => "frontier_exhausted".to_owned(),
        LocalTurnGraphWitnessStatus::MechanicsGap => "mechanics_gap".to_owned(),
        LocalTurnGraphWitnessStatus::ReplayMismatch(error) => {
            format!("replay_mismatch:{error:?}")
        }
        LocalTurnGraphWitnessStatus::Partial(interruption) => format!(
            "partial:{}",
            match interruption {
                LocalTurnGraphWitnessInterruption::SelectionBudget => "selection_budget",
                LocalTurnGraphWitnessInterruption::GenerationWorkBudget => {
                    "generation_work_budget"
                }
                LocalTurnGraphWitnessInterruption::EngineStepBudget => "engine_step_budget",
                LocalTurnGraphWitnessInterruption::Deadline => "deadline",
            }
        ),
    }
}

fn duration_millis_u64(duration: std::time::Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_combat_planner::{
        LocalTurnGraphWitnessConfig, LocalTurnGraphWitnessQuantum, LocalTurnGraphWitnessSession,
    };
    use sts_oracle_runtime::ai::potion_continuation_context_v1::potion_run_continuation_context_v1;
    use sts_oracle_runtime::ai::potion_continuation_pressure_v1::potion_continuation_pressure_v1;
    use sts_oracle_runtime::ai::route_window_facts::{
        RouteWindowFact, RouteWindowKind, RouteWindowScope,
    };
    use sts_oracle_runtime::ai::strategy::deck_strategic_deficit::StrategicPackageEvidence;
    use sts_oracle_runtime::content::cards::CardId;
    use sts_oracle_runtime::content::monsters::EnemyId;
    use sts_oracle_runtime::content::potions::{Potion, PotionId, ALL_POTIONS};
    use sts_oracle_runtime::content::relics::{RelicId, RelicState};
    use sts_oracle_runtime::eval::combat_case::{
        CombatCaseGap, CombatCaseRngSummary, CombatCaseRunSummary, CombatCaseSource,
        CombatCaseWitnessBudgetV1,
    };
    use sts_oracle_runtime::eval::run_control::AtomicCombatSearchTraceSummaryV2;
    use sts_oracle_runtime::runtime::combat::CombatCard;
    use sts_oracle_runtime::state::core::EngineState;
    use sts_oracle_runtime::state::RunState;

    fn resource(slot: usize, id: PotionId, uuid: u32) -> PotionResourceV1 {
        potion_resource(slot, &Potion::new(id, uuid))
    }

    fn expenditure(uuid: u32) -> PotionExpenditureEventV1 {
        PotionExpenditureEventV1 {
            action_index: 0,
            slot: uuid as usize,
            id: "TestPotion".to_owned(),
            uuid,
            mode: PotionExpenditureModeV1::Use,
            verified_win_rescue_tier: PotionVerifiedWinRescueTierV1::BoundedQuality,
        }
    }

    fn combat_case_with_trace_run_context() -> CombatCase {
        let mut run_state = RunState::new(7, 0, false, "Ironclad");
        run_state.act_num = 2;
        run_state.floor_num = 32;
        let mut combat = sts_oracle_runtime::test_support::blank_test_combat();
        combat.entities.player.current_hp = 31;
        combat.entities.player.max_hp = 72;
        combat.entities.potions = vec![Some(Potion::new(PotionId::RegenPotion, 50)), None];
        let context = potion_run_continuation_context_v1(&run_state, &combat);
        let pressure = potion_continuation_pressure_v1(&run_state, &context);
        let attempt = AtomicCombatSearchTraceSummaryV2 {
            source: "search_combat".to_owned(),
            lane: Some("no_potion_primary".to_owned()),
            potion_continuation_context: Some(context),
            potion_continuation_pressure: Some(pressure),
            combat_victory_continuation: Some(
                CombatVictoryContinuationFactsV1::from_guaranteed_room_boss_full_heal(false),
            ),
            strategic_hp_quality: Some(CombatSearchStrategicHpQualityFactsV1::from_owner_limits(
                31,
                72,
                sts_oracle_runtime::eval::run_control::RunControlHpLossLimit::Limit(13),
                sts_oracle_runtime::eval::run_control::RunControlHpLossLimit::Limit(13),
            )),
            ..AtomicCombatSearchTraceSummaryV2::default()
        };
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

        CombatCase::new(
            CombatCaseSource {
                seed: 7,
                ascension: 0,
                generation: 1,
                branch_id: 0,
                parent_id: None,
            },
            CombatCaseGap {
                boundary: "Combat".to_owned(),
                reason: "no win".to_owned(),
                witness_budget: CombatCaseWitnessBudgetV1::AtomicExactV2 {
                    primary_nodes: 100,
                    primary_wall_ms: 10,
                    rescue_nodes: 100,
                    rescue_wall_ms: 10,
                },
            },
            CombatCaseRunSummary {
                act: run_state.act_num,
                floor: run_state.floor_num,
                hp: position.combat.entities.player.current_hp,
                max_hp: position.combat.entities.player.max_hp,
                gold: run_state.gold,
                deck_size: run_state.master_deck.len(),
                relic_count: run_state.relics.len(),
                potion_slots: position.combat.entities.potions.len(),
            },
            vec![attempt.clone()],
            Some(attempt),
            Vec::new(),
            CombatCaseRngSummary::from_pool(&run_state.rng_pool),
            position,
        )
    }

    fn policy_lane(
        lane_id: &str,
        event: PotionExpenditureEventV1,
        disposition: VerifiedWinPotionDispositionV1,
        assessment: PotionMarginalAssessmentV1,
        final_turn_delta: i64,
        pareto_frontier: bool,
    ) -> PotionAuditLaneResultV1 {
        PotionAuditLaneResultV1 {
            lane_id: lane_id.to_owned(),
            allowed_slot_mask: 1,
            allowed_potions: Vec::new(),
            max_explicit_expenditures: 1,
            status: "partial:generation_work_budget".to_owned(),
            elapsed_ms: 0,
            counters: PotionAuditLaneCountersV1 {
                selections: 0,
                generation_work: 0,
                engine_steps: 0,
                exact_nodes: 0,
                terminal_win_options: 0,
                witness_replay_attempts: 0,
                witness_replay_improvements: 0,
            },
            exported_witness_actions: None,
            witness: Some(PotionAuditWitnessV1 {
                final_hp: 30,
                hp_loss: 10,
                final_player_turn: 5,
                turns_elapsed: 5,
                action_count: 10,
                explicit_potion_action_count: 1,
                potion_expenditures: vec![event],
                verified_win_potion_disposition: disposition,
                policy_review_flags: Vec::new(),
                lane_compliant: true,
                meets_survival_reserve: Some(true),
                relative_to_no_potion: Some(PotionMarginalComparisonV1 {
                    final_hp_delta: Some(10),
                    final_turn_delta: Some(final_turn_delta),
                    action_count_delta: Some(1),
                    survival_reserve_delta: None,
                    assessment,
                }),
                pareto_frontier,
                dominated_by: Vec::new(),
                shadow_spend_adjudication: None,
            }),
        }
    }

    fn shadow_adjudication(mut lane: PotionAuditLaneResultV1) -> PotionSpendAdjudicationV1 {
        let run_level_projection = PotionRunContinuationProjectionV1 {
            status: PotionRunContinuationProjectionStatusV1::UnavailableLegacyCase,
            source: None,
            attempt_index: None,
            attempt_source: None,
            attempt_lane: None,
            mismatches: Vec::new(),
            captured_context: None,
        };
        let pressure_projection = PotionContinuationPressureProjectionV1 {
            status: PotionContinuationPressureProjectionStatusV1::UnavailableLegacyCase,
            source: None,
            attempt_index: None,
            attempt_source: None,
            attempt_lane: None,
            mismatches: Vec::new(),
            captured_pressure: None,
        };
        let victory_projection = CombatVictoryContinuationProjectionV1 {
            status: CombatVictoryContinuationProjectionStatusV1::UnavailableLegacyCase,
            source: None,
            attempt_index: None,
            attempt_source: None,
            attempt_lane: None,
            mismatches: Vec::new(),
            captured_facts: None,
        };
        let quality_projection = StrategicHpQualityProjectionV1 {
            status: StrategicHpQualityProjectionStatusV1::UnavailableLegacyCase,
            source: None,
            attempt_index: None,
            attempt_source: None,
            attempt_lane: None,
            mismatches: Vec::new(),
            captured_facts: None,
        };
        annotate_shadow_spend_adjudications(
            std::slice::from_mut(&mut lane),
            &[],
            &run_level_projection,
            &pressure_projection,
            &victory_projection,
            &quality_projection,
        );
        lane.witness
            .unwrap()
            .shadow_spend_adjudication
            .expect("shadow spend adjudication")
    }

    #[test]
    fn lane_specs_keep_exact_slot_identity_and_bounded_combinations() {
        let resources = vec![
            resource(0, PotionId::BlockPotion, 10),
            resource(2, PotionId::SkillPotion, 20),
        ];
        let lanes = build_lane_specs(&resources, 2, 8).expect("lane specs");

        assert_eq!(lanes.len(), 4);
        assert_eq!(lanes[0].lane_id, "no_potion");
        assert_eq!(lanes[1].allowed_slot_mask, 1);
        assert_eq!(lanes[2].allowed_slot_mask, 1 << 2);
        assert_eq!(lanes[3].allowed_slot_mask, 1 | (1 << 2));
        assert_eq!(
            lanes[3]
                .allowed_potions
                .iter()
                .map(|resource| resource.uuid)
                .collect::<Vec<_>>(),
            vec![10, 20]
        );
    }

    #[test]
    fn potion_resources_expose_shared_strategy_coverage_without_guessing_missing_traits() {
        let strength = resource(0, PotionId::StrengthPotion, 10);
        assert_eq!(
            strength.shared_strategy_traits,
            vec![PotionAcquisitionTraitV1::StrengthGain]
        );
        assert_eq!(
            strength.shared_strategy_coverage,
            PotionSharedStrategyCoverageV1::Classified
        );

        let fruit_juice = resource(1, PotionId::FruitJuice, 15);
        assert_eq!(
            fruit_juice.shared_strategy_traits,
            vec![PotionAcquisitionTraitV1::PermanentMaxHp]
        );
        assert_eq!(
            fruit_juice.shared_strategy_coverage,
            PotionSharedStrategyCoverageV1::Classified
        );

        let regen = resource(2, PotionId::RegenPotion, 20);
        assert!(regen.shared_strategy_traits.is_empty());
        assert_eq!(
            regen.shared_strategy_coverage,
            PotionSharedStrategyCoverageV1::Unclassified
        );
    }

    #[test]
    fn mechanical_roles_and_continuation_dependencies_cover_every_potion_identity() {
        for id in ALL_POTIONS {
            let resource = resource(0, *id, 10);
            assert!(
                !resource.continuation_dependencies.is_empty(),
                "{id:?} needs an explicit continuation dependency"
            );
        }

        let regen = resource(0, PotionId::RegenPotion, 20);
        assert_eq!(
            regen.mechanical_role,
            PotionMechanicalRoleV1::DelayedHealing
        );
        assert!(regen
            .continuation_dependencies
            .contains(&PotionContinuationDependencyV1::FutureFightLength));

        let duplication = resource(0, PotionId::DuplicationPotion, 30);
        assert_eq!(
            duplication.mechanical_role,
            PotionMechanicalRoleV1::NextCardDuplication
        );
        assert!(duplication
            .continuation_dependencies
            .contains(&PotionContinuationDependencyV1::HighValueCardTarget));
    }

    #[test]
    fn continuation_context_keeps_exact_inventory_relic_and_deck_pressure() {
        let mut combat = sts_oracle_runtime::test_support::blank_test_combat();
        combat.meta.is_boss_fight = true;
        combat.meta.master_deck_snapshot = vec![
            CombatCard::for_scenario(CardId::HeavyBlade, 10),
            CombatCard::for_scenario(CardId::Inflame, 20),
        ]
        .into();
        combat.entities.potions = vec![
            Some(Potion::new(PotionId::DuplicationPotion, 30)),
            Some(Potion::new(PotionId::SkillPotion, 40)),
            Some(Potion::new(PotionId::AttackPotion, 50)),
        ];
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::SacredBark));
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::WhiteBeastStatue));
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::Sozu));
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

        let context = potion_continuation_context(
            2,
            32,
            &position,
            PotionRunContinuationProjectionV1 {
                status: PotionRunContinuationProjectionStatusV1::UnavailableLegacyCase,
                source: None,
                attempt_index: None,
                attempt_source: None,
                attempt_lane: None,
                mismatches: Vec::new(),
                captured_context: None,
            },
        );

        assert_eq!(
            context.current_combat_stake,
            PotionCurrentCombatStakeV1::Boss
        );
        assert_eq!(context.deck_size, 2);
        assert_eq!(context.inventory.slot_capacity, 3);
        assert_eq!(context.inventory.occupied_slots, 3);
        assert!(context.inventory.inventory_full);
        assert!(
            context
                .inventory
                .new_potion_would_require_replacement_if_obtainable
        );
        assert!(context.relics.sacred_bark);
        assert!(context.relics.white_beast_statue);
        assert!(context.relics.sozu);
        assert!(context
            .deck_strategic_deficit
            .package_evidence
            .contains(&StrategicPackageEvidence::StrengthScaling));
        assert!(context
            .unavailable_future_context
            .contains(&PotionContinuationUnknownV1::NextEncounterIdentity));
        assert!(context
            .unavailable_future_context
            .contains(&PotionContinuationUnknownV1::RouteBeforeNextEliteOrBoss));
        assert_eq!(
            context.run_level_projection.status,
            PotionRunContinuationProjectionStatusV1::UnavailableLegacyCase
        );
    }

    #[test]
    fn saved_run_context_is_used_only_when_it_matches_the_exact_combat_root() {
        let case = combat_case_with_trace_run_context();

        let projection = project_saved_run_continuation_context(&case);

        assert_eq!(
            projection.status,
            PotionRunContinuationProjectionStatusV1::ValidatedExactRoot
        );
        assert_eq!(projection.source, Some("atomic_combat_search_attempts"));
        assert_eq!(projection.attempt_index, Some(0));
        assert_eq!(
            projection.attempt_lane.as_deref(),
            Some("no_potion_primary")
        );
        assert!(projection.mismatches.is_empty());
        assert!(projection.captured_context.is_some());

        let mut mismatched_case = case;
        mismatched_case.atomic_combat_search_attempts[0]
            .potion_continuation_context
            .as_mut()
            .unwrap()
            .current_hp += 1;
        let rejected = project_saved_run_continuation_context(&mismatched_case);

        assert_eq!(
            rejected.status,
            PotionRunContinuationProjectionStatusV1::RejectedRootMismatch
        );
        assert!(rejected
            .mismatches
            .iter()
            .any(|mismatch| mismatch.field == "current_hp"));
        assert!(rejected
            .mismatches
            .iter()
            .any(|mismatch| mismatch.field == "trace_context_consistency"));
    }

    #[test]
    fn exact_production_context_can_replace_missing_search_trace_facts() {
        let mut case = combat_case_with_trace_run_context();
        let mut reconstructed = case.atomic_combat_search_attempts.remove(0);
        case.failed_atomic_combat_search = None;
        reconstructed.source = "reconstructed_exact_production_context".to_string();

        let run_projection =
            project_saved_run_continuation_context_with_reconstructed(&case, Some(&reconstructed));
        let pressure_projection = project_saved_potion_continuation_pressure_with_reconstructed(
            &case,
            &run_projection,
            Some(&reconstructed),
        );
        let victory_projection = project_saved_combat_victory_continuation_with_reconstructed(
            &case,
            &run_projection,
            Some(&reconstructed),
        );
        let quality_projection = project_saved_strategic_hp_quality_with_reconstructed(
            &case,
            &run_projection,
            &victory_projection,
            Some(&reconstructed),
        );

        assert_eq!(
            run_projection.status,
            PotionRunContinuationProjectionStatusV1::ValidatedExactRoot
        );
        assert_eq!(
            run_projection.source,
            Some("reconstructed_production_context")
        );
        assert_eq!(
            pressure_projection.status,
            PotionContinuationPressureProjectionStatusV1::ValidatedExactRoot
        );
        assert_eq!(
            victory_projection.status,
            CombatVictoryContinuationProjectionStatusV1::ValidatedCapturedFact
        );
        assert_eq!(
            quality_projection.status,
            StrategicHpQualityProjectionStatusV1::ValidatedCapturedFact
        );
    }

    #[test]
    fn legacy_case_without_saved_run_context_stays_explicitly_unavailable() {
        let mut case = combat_case_with_trace_run_context();
        case.atomic_combat_search_attempts.clear();
        case.failed_atomic_combat_search = None;

        let projection = project_saved_run_continuation_context(&case);

        assert_eq!(
            projection.status,
            PotionRunContinuationProjectionStatusV1::UnavailableLegacyCase
        );
        assert!(projection.captured_context.is_none());
        assert!(projection.mismatches.is_empty());
    }

    #[test]
    fn saved_pressure_is_valid_only_when_rebuilt_from_the_exact_case_facts() {
        let case = combat_case_with_trace_run_context();
        let run_projection = project_saved_run_continuation_context(&case);

        let pressure_projection =
            project_saved_potion_continuation_pressure(&case, &run_projection);

        assert_eq!(
            pressure_projection.status,
            PotionContinuationPressureProjectionStatusV1::ValidatedExactRoot
        );
        assert_eq!(
            pressure_projection.source,
            Some("atomic_combat_search_attempts")
        );
        assert_eq!(pressure_projection.attempt_index, Some(0));
        assert!(pressure_projection.mismatches.is_empty());
        assert!(pressure_projection.captured_pressure.is_some());
    }

    #[test]
    fn pressure_projection_rejects_gold_route_and_campfire_tampering() {
        let assert_rejected_field = |mut case: CombatCase,
                                     mutate: fn(&mut PotionContinuationPressureV1),
                                     expected_field: &str| {
            mutate(
                case.atomic_combat_search_attempts[0]
                    .potion_continuation_pressure
                    .as_mut()
                    .unwrap(),
            );
            case.failed_atomic_combat_search = Some(case.atomic_combat_search_attempts[0].clone());
            let run_projection = project_saved_run_continuation_context(&case);
            let pressure_projection =
                project_saved_potion_continuation_pressure(&case, &run_projection);
            assert_eq!(
                pressure_projection.status,
                PotionContinuationPressureProjectionStatusV1::RejectedMismatch
            );
            assert!(
                pressure_projection
                    .mismatches
                    .iter()
                    .any(|mismatch| mismatch.field == expected_field),
                "expected a {expected_field} mismatch, got {:?}",
                pressure_projection.mismatches
            );
        };

        assert_rejected_field(
            combat_case_with_trace_run_context(),
            |pressure| pressure.shop.current_gold += 1,
            "shop",
        );
        assert_rejected_field(
            combat_case_with_trace_run_context(),
            |pressure| pressure.route.observed_path_count += 1,
            "route",
        );
        assert_rejected_field(
            combat_case_with_trace_run_context(),
            |pressure| pressure.recovery.campfire_observed_on_some_covered_path = true,
            "recovery",
        );
    }

    #[test]
    fn pressure_projection_rejects_missing_and_conflicting_trace_summaries() {
        let mut missing = combat_case_with_trace_run_context();
        let mut summary_without_pressure = missing.atomic_combat_search_attempts[0].clone();
        summary_without_pressure.potion_continuation_pressure = None;
        missing
            .atomic_combat_search_attempts
            .push(summary_without_pressure);
        let run_projection = project_saved_run_continuation_context(&missing);
        let missing_projection =
            project_saved_potion_continuation_pressure(&missing, &run_projection);
        assert_eq!(
            missing_projection.status,
            PotionContinuationPressureProjectionStatusV1::RejectedMismatch
        );
        assert!(missing_projection
            .mismatches
            .iter()
            .any(|mismatch| { mismatch.field == "trace_pressure_presence_consistency" }));

        let mut conflicting = combat_case_with_trace_run_context();
        let mut conflicting_summary = conflicting.atomic_combat_search_attempts[0].clone();
        conflicting_summary
            .potion_continuation_pressure
            .as_mut()
            .unwrap()
            .shop
            .current_gold += 1;
        conflicting
            .atomic_combat_search_attempts
            .push(conflicting_summary);
        let run_projection = project_saved_run_continuation_context(&conflicting);
        let conflicting_projection =
            project_saved_potion_continuation_pressure(&conflicting, &run_projection);
        assert_eq!(
            conflicting_projection.status,
            PotionContinuationPressureProjectionStatusV1::RejectedMismatch
        );
        assert!(conflicting_projection
            .mismatches
            .iter()
            .any(|mismatch| mismatch.field == "trace_pressure_consistency"));
    }

    #[test]
    fn pressure_projection_requires_a_validated_run_context() {
        let mut case = combat_case_with_trace_run_context();
        case.atomic_combat_search_attempts[0]
            .potion_continuation_context
            .as_mut()
            .unwrap()
            .current_hp += 1;
        let run_projection = project_saved_run_continuation_context(&case);

        let pressure_projection =
            project_saved_potion_continuation_pressure(&case, &run_projection);

        assert_eq!(
            pressure_projection.status,
            PotionContinuationPressureProjectionStatusV1::RejectedWithoutValidatedRunContext
        );
        assert!(pressure_projection
            .mismatches
            .iter()
            .any(|mismatch| mismatch.field == "run_context_status"));
    }

    #[test]
    fn serialized_legacy_case_without_pressure_remains_compatible_and_unavailable() {
        let case = combat_case_with_trace_run_context();
        let mut payload = serde_json::to_value(case).expect("serialize combat case");
        for attempt in payload["atomic_combat_search_attempts"]
            .as_array_mut()
            .expect("combat search attempts")
        {
            attempt
                .as_object_mut()
                .expect("combat search summary")
                .remove("potion_continuation_pressure");
        }
        payload["failed_atomic_combat_search"]
            .as_object_mut()
            .expect("failed search summary")
            .remove("potion_continuation_pressure");
        let restored: CombatCase =
            serde_json::from_value(payload).expect("deserialize legacy combat case");
        let run_projection = project_saved_run_continuation_context(&restored);

        let pressure_projection =
            project_saved_potion_continuation_pressure(&restored, &run_projection);

        assert_eq!(
            run_projection.status,
            PotionRunContinuationProjectionStatusV1::ValidatedExactRoot
        );
        assert_eq!(
            pressure_projection.status,
            PotionContinuationPressureProjectionStatusV1::UnavailableLegacyCase
        );
        assert!(pressure_projection.captured_pressure.is_none());
        assert!(pressure_projection.mismatches.is_empty());
    }

    #[test]
    fn retained_value_evidence_keeps_exact_identity_and_dependency_uncertainty() {
        let case = combat_case_with_trace_run_context();
        let projection = project_saved_run_continuation_context(&case);
        let pressure_projection = project_saved_potion_continuation_pressure(&case, &projection);
        let regen = resource(0, PotionId::RegenPotion, 50);
        let mut regen_event = expenditure(50);
        regen_event.slot = 0;
        regen_event.id = "RegenPotion".to_owned();
        let lane = policy_lane(
            "regen",
            regen_event,
            VerifiedWinPotionDispositionV1::ContainsReservedResource,
            PotionMarginalAssessmentV1::ImprovesFinalHp,
            3,
            true,
        );

        let evidence = retained_value_evidence(
            lane.witness.as_ref().unwrap(),
            std::slice::from_ref(&regen),
            &projection,
            &pressure_projection,
        );

        assert_eq!(
            evidence.run_context_status,
            PotionRunContinuationProjectionStatusV1::ValidatedExactRoot
        );
        assert_eq!(
            evidence.continuation_pressure_status,
            PotionContinuationPressureProjectionStatusV1::ValidatedExactRoot
        );
        assert!(evidence.validated_continuation_pressure.is_some());
        assert_eq!(evidence.exact_consumed_resources, vec![regen.clone()]);
        assert!(evidence.unmatched_expenditure_uuids.is_empty());
        assert!(evidence.dependency_evidence.iter().any(|dependency| {
            dependency.dependency == PotionContinuationDependencyV1::CurrentHpDeficit
                && dependency.coverage == PotionContinuationEvidenceCoverageV1::ExactCurrentRoot
        }));
        assert!(evidence.dependency_evidence.iter().any(|dependency| {
            dependency.dependency == PotionContinuationDependencyV1::FutureFightLength
                && dependency.coverage == PotionContinuationEvidenceCoverageV1::FutureUnknown
        }));

        let mut tampered_case = combat_case_with_trace_run_context();
        tampered_case.atomic_combat_search_attempts[0]
            .potion_continuation_pressure
            .as_mut()
            .unwrap()
            .shop
            .current_gold += 1;
        tampered_case.failed_atomic_combat_search =
            Some(tampered_case.atomic_combat_search_attempts[0].clone());
        let tampered_run_projection = project_saved_run_continuation_context(&tampered_case);
        let tampered_pressure_projection =
            project_saved_potion_continuation_pressure(&tampered_case, &tampered_run_projection);
        let rejected_evidence = retained_value_evidence(
            lane.witness.as_ref().unwrap(),
            std::slice::from_ref(&regen),
            &tampered_run_projection,
            &tampered_pressure_projection,
        );
        assert_eq!(
            rejected_evidence.continuation_pressure_status,
            PotionContinuationPressureProjectionStatusV1::RejectedMismatch
        );
        assert!(rejected_evidence.validated_continuation_pressure.is_none());
        assert!(rejected_evidence.route_window_coverage.is_none());
    }

    #[test]
    fn dependency_coverage_does_not_overclaim_missing_route_or_supply_facts() {
        assert_eq!(
            continuation_dependency_coverage(
                PotionContinuationDependencyV1::RouteEscapeValue,
                PotionRunContinuationProjectionStatusV1::ValidatedExactRoot,
                Some(RouteWindowCoverageKind::CompleteWithinHorizon),
            ),
            PotionContinuationEvidenceCoverageV1::PartialRunWindow
        );
        assert_eq!(
            continuation_dependency_coverage(
                PotionContinuationDependencyV1::RouteEscapeValue,
                PotionRunContinuationProjectionStatusV1::ValidatedExactRoot,
                Some(RouteWindowCoverageKind::UnavailableMap),
            ),
            PotionContinuationEvidenceCoverageV1::FutureUnknown
        );
        assert_eq!(
            continuation_dependency_coverage(
                PotionContinuationDependencyV1::EmptyPotionSlotsAndAcquisitionRules,
                PotionRunContinuationProjectionStatusV1::UnavailableLegacyCase,
                None,
            ),
            PotionContinuationEvidenceCoverageV1::PartialCurrentRoot
        );
    }

    #[test]
    fn pareto_dominance_requires_a_resource_subset_and_no_worse_combat_axes() {
        let none = BTreeSet::new();
        let skill = BTreeSet::from([20]);
        let weak = BTreeSet::from([30]);

        assert!(dominates(93, 6, 23, &none, 93, 9, 49, &skill));
        assert!(!dominates(92, 6, 23, &none, 93, 9, 49, &skill));
        assert!(!dominates(93, 6, 23, &weak, 90, 9, 49, &skill));
    }

    #[test]
    fn expenditure_identity_uses_uuid_not_only_potion_kind_or_count() {
        let first = expenditure_identity_set(&[expenditure(10)]);
        let second = expenditure_identity_set(&[expenditure(20)]);

        assert!(!first.is_subset(&second));
        assert!(!second.is_subset(&first));
    }

    #[test]
    fn policy_review_flags_expose_reserved_upside_and_admitted_waste() {
        let mut regen = expenditure(10);
        regen.id = "RegenPotion".to_owned();
        regen.verified_win_rescue_tier = PotionVerifiedWinRescueTierV1::FindAnyWin;
        let mut lanes = vec![
            policy_lane(
                "regen",
                regen,
                VerifiedWinPotionDispositionV1::ContainsReservedResource,
                PotionMarginalAssessmentV1::CrossesSurvivalReserve,
                3,
                true,
            ),
            policy_lane(
                "fire",
                expenditure(20),
                VerifiedWinPotionDispositionV1::BoundedQualityOnly,
                PotionMarginalAssessmentV1::SameFinalHpWithExtraResource,
                1,
                false,
            ),
        ];

        annotate_policy_review_flags(&mut lanes);

        let regen_flags = &lanes[0].witness.as_ref().unwrap().policy_review_flags;
        assert!(
            regen_flags.contains(&PotionPolicyReviewFlagV1::ReservedResourceCrossesSurvivalReserve)
        );
        assert!(regen_flags.contains(&PotionPolicyReviewFlagV1::DelayedHealRequiresExtraTurns));
        let fire_flags = &lanes[1].witness.as_ref().unwrap().policy_review_flags;
        assert!(fire_flags.contains(&PotionPolicyReviewFlagV1::AdmittedResourceIsParetoDominated));
        assert!(fire_flags.contains(&PotionPolicyReviewFlagV1::AdmittedResourceHasNoHpBenefit));
    }

    #[test]
    fn reserve_delta_keeps_large_near_crossing_gain_as_exact_shadow_fact() {
        let delta = survival_reserve_delta(9, 28, 30);

        assert_eq!(delta.reserve_hp, 30);
        assert_eq!(delta.baseline_shortfall_hp, 21);
        assert_eq!(delta.candidate_shortfall_hp, 2);
        assert_eq!(delta.shortfall_reduction_hp, 19);
        assert_eq!(delta.shortfall_reduction_ppm, Some(904_761));
    }

    #[test]
    fn reserve_delta_does_not_invent_a_fraction_when_baseline_is_already_safe() {
        let delta = survival_reserve_delta(55, 60, 30);

        assert_eq!(delta.baseline_shortfall_hp, 0);
        assert_eq!(delta.candidate_shortfall_hp, 0);
        assert_eq!(delta.shortfall_reduction_hp, 0);
        assert_eq!(delta.shortfall_reduction_ppm, None);
    }

    #[test]
    fn spend_urgency_question_validates_exact_root_and_preserves_typed_route_order() {
        let case = combat_case_with_trace_run_context();
        let source_order = case.atomic_combat_search_attempts[0]
            .potion_continuation_context
            .as_ref()
            .expect("captured continuation context")
            .route_window
            .facts
            .iter()
            .find(|fact| {
                fact.predicate
                    == (RouteWindowPredicate::OccursBefore {
                        subject: RouteWindowSubject::KnownCombat,
                        before: RouteWindowSubject::Campfire,
                    })
            })
            .expect("typed combat-before-campfire fact");
        let expected_order = if source_order.modality == RouteWindowModality::Unknown {
            PotionRouteOrderEvidenceV1::Unavailable {
                reason: PotionRouteOrderUnavailableReasonV1::UnknownModality,
                observed_modality: Some(source_order.modality),
                provenance: Some(source_order.provenance),
                horizon_nodes: Some(source_order.horizon_nodes),
            }
        } else {
            PotionRouteOrderEvidenceV1::Validated {
                modality: source_order.modality,
                provenance: source_order.provenance,
                horizon_nodes: source_order.horizon_nodes,
            }
        };
        let run_projection = project_saved_run_continuation_context(&case);
        let pressure_projection =
            project_saved_potion_continuation_pressure(&case, &run_projection);
        let victory_projection = project_saved_combat_victory_continuation(&case, &run_projection);
        let quality_projection =
            project_saved_strategic_hp_quality(&case, &run_projection, &victory_projection);
        let comparison = PotionMarginalComparisonV1 {
            final_hp_delta: Some(19),
            final_turn_delta: Some(1),
            action_count_delta: Some(2),
            survival_reserve_delta: Some(survival_reserve_delta(9, 28, 30)),
            assessment: PotionMarginalAssessmentV1::ImprovesFinalHp,
        };

        let question = spend_urgency_question(
            &comparison,
            9,
            28,
            &run_projection,
            &pressure_projection,
            &victory_projection,
            &quality_projection,
        );

        assert_eq!(
            question.status,
            PotionSpendUrgencyQuestionStatusV1::ValidatedExactRoot
        );
        assert!(question.limitations.is_empty());
        let facts = question.facts.expect("validated urgency facts");
        assert_eq!(
            facts
                .configured_survival_reserve_delta
                .as_ref()
                .map(|delta| delta.shortfall_reduction_hp),
            Some(19)
        );
        assert_eq!(
            facts.combat_victory_continuation,
            PotionCombatVictoryContinuationEvidenceV1::ValidatedCapturedFact {
                evaluator: COMBAT_VICTORY_CONTINUATION_EVALUATOR_V1.to_owned(),
                hp_carryover: CombatVictoryHpCarryoverV1::NotGuaranteedByRoomBossActTransition,
            }
        );
        let PotionStrategicHpQualityEvidenceV1::ValidatedCapturedFact {
            survival, quality, ..
        } = &facts.strategic_hp_quality
        else {
            panic!("validated strategic HP quality evidence");
        };
        assert!(survival.candidate_crosses_from_unsatisfied_to_satisfied);
        assert!(quality.candidate_crosses_from_unsatisfied_to_satisfied);
        assert_eq!(facts.inventory.occupied_slots, 1);
        assert_eq!(facts.shop.current_gold, case.core.run.gold);
        assert!(facts.future_potion_identity_unknown);
        assert_eq!(
            facts.route_ordering.future_known_combat_before_campfire,
            expected_order
        );

        let mut missing_victory_payload = victory_projection.clone();
        missing_victory_payload.captured_facts = None;
        let unavailable = spend_urgency_question(
            &comparison,
            9,
            28,
            &run_projection,
            &pressure_projection,
            &missing_victory_payload,
            &quality_projection,
        );
        assert_eq!(
            unavailable.status,
            PotionSpendUrgencyQuestionStatusV1::Unavailable
        );
        assert!(unavailable.limitations.contains(
            &PotionSpendUrgencyQuestionLimitationV1::ValidatedCombatVictoryContinuationMissingPayload
        ));

        let mut missing_quality_payload = quality_projection.clone();
        missing_quality_payload.captured_facts = None;
        let unavailable = spend_urgency_question(
            &comparison,
            9,
            28,
            &run_projection,
            &pressure_projection,
            &victory_projection,
            &missing_quality_payload,
        );
        assert_eq!(
            unavailable.status,
            PotionSpendUrgencyQuestionStatusV1::Unavailable
        );
        assert!(unavailable.limitations.contains(
            &PotionSpendUrgencyQuestionLimitationV1::ValidatedStrategicHpQualityMissingPayload
        ));
    }

    #[test]
    fn spend_urgency_question_is_unavailable_for_legacy_context() {
        let mut case = combat_case_with_trace_run_context();
        for attempt in &mut case.atomic_combat_search_attempts {
            attempt.potion_continuation_context = None;
            attempt.potion_continuation_pressure = None;
            attempt.combat_victory_continuation = None;
            attempt.strategic_hp_quality = None;
        }
        if let Some(failed) = case.failed_atomic_combat_search.as_mut() {
            failed.potion_continuation_context = None;
            failed.potion_continuation_pressure = None;
            failed.combat_victory_continuation = None;
            failed.strategic_hp_quality = None;
        }
        let run_projection = project_saved_run_continuation_context(&case);
        let pressure_projection =
            project_saved_potion_continuation_pressure(&case, &run_projection);
        let victory_projection = project_saved_combat_victory_continuation(&case, &run_projection);
        let quality_projection =
            project_saved_strategic_hp_quality(&case, &run_projection, &victory_projection);
        let comparison = PotionMarginalComparisonV1 {
            final_hp_delta: Some(3),
            final_turn_delta: Some(0),
            action_count_delta: Some(1),
            survival_reserve_delta: None,
            assessment: PotionMarginalAssessmentV1::ImprovesFinalHp,
        };

        let question = spend_urgency_question(
            &comparison,
            10,
            13,
            &run_projection,
            &pressure_projection,
            &victory_projection,
            &quality_projection,
        );

        assert_eq!(
            question.status,
            PotionSpendUrgencyQuestionStatusV1::Unavailable
        );
        assert!(question.facts.is_none());
        assert!(question
            .limitations
            .contains(&PotionSpendUrgencyQuestionLimitationV1::RunContextUnavailable));
        assert!(question
            .limitations
            .contains(&PotionSpendUrgencyQuestionLimitationV1::ContinuationPressureUnavailable));
    }

    #[test]
    fn spend_urgency_question_keeps_legacy_victory_fact_explicitly_unavailable() {
        let mut case = combat_case_with_trace_run_context();
        for attempt in &mut case.atomic_combat_search_attempts {
            attempt.combat_victory_continuation = None;
        }
        if let Some(failed) = case.failed_atomic_combat_search.as_mut() {
            failed.combat_victory_continuation = None;
        }
        let run_projection = project_saved_run_continuation_context(&case);
        let pressure_projection =
            project_saved_potion_continuation_pressure(&case, &run_projection);
        let victory_projection = project_saved_combat_victory_continuation(&case, &run_projection);
        let quality_projection =
            project_saved_strategic_hp_quality(&case, &run_projection, &victory_projection);
        let comparison = PotionMarginalComparisonV1 {
            final_hp_delta: Some(5),
            final_turn_delta: Some(0),
            action_count_delta: Some(1),
            survival_reserve_delta: None,
            assessment: PotionMarginalAssessmentV1::ImprovesFinalHp,
        };

        let question = spend_urgency_question(
            &comparison,
            10,
            15,
            &run_projection,
            &pressure_projection,
            &victory_projection,
            &quality_projection,
        );

        assert_eq!(
            question.status,
            PotionSpendUrgencyQuestionStatusV1::ValidatedExactRoot
        );
        assert!(question.limitations.contains(
            &PotionSpendUrgencyQuestionLimitationV1::CombatVictoryContinuationUnavailable
        ));
        assert_eq!(
            question
                .facts
                .expect("available V10 facts")
                .combat_victory_continuation,
            PotionCombatVictoryContinuationEvidenceV1::UnavailableLegacyCase
        );
    }

    #[test]
    fn spend_urgency_question_keeps_legacy_quality_fact_explicitly_unavailable() {
        let mut case = combat_case_with_trace_run_context();
        for attempt in &mut case.atomic_combat_search_attempts {
            attempt.strategic_hp_quality = None;
        }
        if let Some(failed) = case.failed_atomic_combat_search.as_mut() {
            failed.strategic_hp_quality = None;
        }
        let run_projection = project_saved_run_continuation_context(&case);
        let pressure_projection =
            project_saved_potion_continuation_pressure(&case, &run_projection);
        let victory_projection = project_saved_combat_victory_continuation(&case, &run_projection);
        let quality_projection =
            project_saved_strategic_hp_quality(&case, &run_projection, &victory_projection);
        let comparison = PotionMarginalComparisonV1 {
            final_hp_delta: Some(5),
            final_turn_delta: Some(0),
            action_count_delta: Some(1),
            survival_reserve_delta: None,
            assessment: PotionMarginalAssessmentV1::ImprovesFinalHp,
        };

        let question = spend_urgency_question(
            &comparison,
            10,
            15,
            &run_projection,
            &pressure_projection,
            &victory_projection,
            &quality_projection,
        );

        assert_eq!(
            question.status,
            PotionSpendUrgencyQuestionStatusV1::ValidatedExactRoot
        );
        assert!(question
            .limitations
            .contains(&PotionSpendUrgencyQuestionLimitationV1::StrategicHpQualityUnavailable));
        assert_eq!(
            question
                .facts
                .expect("available V10 facts")
                .strategic_hp_quality,
            PotionStrategicHpQualityEvidenceV1::UnavailableLegacyCase
        );
    }

    #[test]
    fn combat_victory_projection_rejects_structurally_impossible_full_heal_claim() {
        let mut case = combat_case_with_trace_run_context();
        case.core.position.combat.meta.is_boss_fight = true;
        let fact = CombatVictoryContinuationFactsV1::from_guaranteed_room_boss_full_heal(true);
        case.atomic_combat_search_attempts[0].combat_victory_continuation = Some(fact.clone());
        case.failed_atomic_combat_search = Some(case.atomic_combat_search_attempts[0].clone());
        let run_projection = project_saved_run_continuation_context(&case);

        let validated = project_saved_combat_victory_continuation(&case, &run_projection);

        assert_eq!(
            validated.status,
            CombatVictoryContinuationProjectionStatusV1::ValidatedCapturedFact
        );
        assert_eq!(validated.captured_facts, Some(fact));
        assert!(validated.mismatches.is_empty());

        case.core.position.combat.meta.ascension_level = 5;
        let rejected = project_saved_combat_victory_continuation(&case, &run_projection);
        assert_eq!(
            rejected.status,
            CombatVictoryContinuationProjectionStatusV1::RejectedMismatch
        );
        assert!(rejected
            .mismatches
            .iter()
            .any(|mismatch| mismatch.field == "ascension"));
    }

    #[test]
    fn strategic_hp_quality_projection_rejects_root_and_limit_mismatches() {
        let mut case = combat_case_with_trace_run_context();
        let facts = case.atomic_combat_search_attempts[0]
            .strategic_hp_quality
            .as_mut()
            .expect("captured strategic HP quality");
        facts.entry_current_hp += 1;
        facts.quality_hp_loss_limit = CombatSearchHpLossLimitV1::Limited { max_hp_loss: 14 };
        case.failed_atomic_combat_search = Some(case.atomic_combat_search_attempts[0].clone());
        let run_projection = project_saved_run_continuation_context(&case);
        let victory_projection = project_saved_combat_victory_continuation(&case, &run_projection);

        let rejected =
            project_saved_strategic_hp_quality(&case, &run_projection, &victory_projection);

        assert_eq!(
            rejected.status,
            StrategicHpQualityProjectionStatusV1::RejectedMismatch
        );
        assert!(rejected
            .mismatches
            .iter()
            .any(|mismatch| mismatch.field == "entry_current_hp"));
        assert!(rejected
            .mismatches
            .iter()
            .any(|mismatch| mismatch.field == "quality_hp_loss_limit"));

        let facts = case.atomic_combat_search_attempts[0]
            .strategic_hp_quality
            .as_mut()
            .expect("captured strategic HP quality");
        facts.entry_current_hp = case.core.position.combat.entities.player.current_hp;
        facts.quality_hp_loss_limit = CombatSearchHpLossLimitV1::Unlimited;
        case.failed_atomic_combat_search = Some(case.atomic_combat_search_attempts[0].clone());
        let rejected =
            project_saved_strategic_hp_quality(&case, &run_projection, &victory_projection);
        assert!(rejected
            .mismatches
            .iter()
            .any(|mismatch| mismatch.field == "hp_loss_limit_kind_consistency"));
    }

    #[test]
    fn spend_urgency_question_rejects_mismatched_pressure() {
        let mut case = combat_case_with_trace_run_context();
        case.atomic_combat_search_attempts[0]
            .potion_continuation_pressure
            .as_mut()
            .unwrap()
            .shop
            .current_gold += 1;
        case.failed_atomic_combat_search = Some(case.atomic_combat_search_attempts[0].clone());
        let run_projection = project_saved_run_continuation_context(&case);
        let pressure_projection =
            project_saved_potion_continuation_pressure(&case, &run_projection);
        let victory_projection = project_saved_combat_victory_continuation(&case, &run_projection);
        let quality_projection =
            project_saved_strategic_hp_quality(&case, &run_projection, &victory_projection);
        let comparison = PotionMarginalComparisonV1 {
            final_hp_delta: Some(7),
            final_turn_delta: Some(0),
            action_count_delta: Some(1),
            survival_reserve_delta: Some(survival_reserve_delta(9, 16, 30)),
            assessment: PotionMarginalAssessmentV1::ImprovesFinalHp,
        };

        let question = spend_urgency_question(
            &comparison,
            9,
            16,
            &run_projection,
            &pressure_projection,
            &victory_projection,
            &quality_projection,
        );

        assert_eq!(
            question.status,
            PotionSpendUrgencyQuestionStatusV1::Rejected
        );
        assert!(question.facts.is_none());
        assert!(question
            .limitations
            .contains(&PotionSpendUrgencyQuestionLimitationV1::ContinuationPressureRejected));
    }

    #[test]
    fn route_ordering_uses_typed_occurs_before_modality() {
        let case = combat_case_with_trace_run_context();
        let mut context = case.atomic_combat_search_attempts[0]
            .potion_continuation_context
            .clone()
            .expect("captured continuation context");
        let predicate = RouteWindowPredicate::OccursBefore {
            subject: RouteWindowSubject::KnownCombat,
            before: RouteWindowSubject::Campfire,
        };
        context
            .route_window
            .facts
            .retain(|fact| fact.predicate != predicate);
        context.route_window.facts.push(RouteWindowFact {
            window: RouteWindowKind::Danger,
            predicate: predicate.clone(),
            modality: RouteWindowModality::Can,
            scope: RouteWindowScope::PathFamily,
            horizon_nodes: 5,
            provenance: RouteWindowProvenance::SomeCoveredPath,
        });

        let ordering = route_ordering_facts(&context);

        assert_eq!(
            ordering.future_known_combat_before_campfire,
            PotionRouteOrderEvidenceV1::Validated {
                modality: RouteWindowModality::Can,
                provenance: RouteWindowProvenance::SomeCoveredPath,
                horizon_nodes: 5,
            }
        );

        let fact = context
            .route_window
            .facts
            .iter_mut()
            .find(|fact| fact.predicate == predicate)
            .expect("typed route-order fact");
        fact.modality = RouteWindowModality::Unknown;
        fact.provenance = RouteWindowProvenance::PartialObservation;
        let ordering = route_ordering_facts(&context);
        assert_eq!(
            ordering.future_known_combat_before_campfire,
            PotionRouteOrderEvidenceV1::Unavailable {
                reason: PotionRouteOrderUnavailableReasonV1::UnknownModality,
                observed_modality: Some(RouteWindowModality::Unknown),
                provenance: Some(RouteWindowProvenance::PartialObservation),
                horizon_nodes: Some(5),
            }
        );
    }

    #[test]
    fn shadow_spend_adjudication_preserves_baseline_and_budget_unknowns() {
        let baseline = policy_lane(
            "no_potion",
            expenditure(10),
            VerifiedWinPotionDispositionV1::BoundedQualityOnly,
            PotionMarginalAssessmentV1::ImprovesFinalHp,
            0,
            false,
        );
        assert_eq!(
            shadow_adjudication(baseline),
            PotionSpendAdjudicationV1::NoPotionBaseline
        );

        let mut no_spend = policy_lane(
            "power",
            expenditure(20),
            VerifiedWinPotionDispositionV1::NoPotionSpent,
            PotionMarginalAssessmentV1::NoPotionWitnessNotFoundUnderAllowance,
            0,
            false,
        );
        let no_spend_witness = no_spend.witness.as_mut().unwrap();
        no_spend_witness.potion_expenditures.clear();
        no_spend_witness
            .relative_to_no_potion
            .as_mut()
            .unwrap()
            .final_hp_delta = None;
        assert_eq!(
            shadow_adjudication(no_spend),
            PotionSpendAdjudicationV1::NoAdditionalPotionConsumed
        );

        let mut unknown = policy_lane(
            "fire",
            expenditure(30),
            VerifiedWinPotionDispositionV1::BoundedQualityOnly,
            PotionMarginalAssessmentV1::NoPotionFrontierExhaustedUnderContract,
            0,
            false,
        );
        let unknown_comparison = unknown
            .witness
            .as_mut()
            .unwrap()
            .relative_to_no_potion
            .as_mut()
            .unwrap();
        unknown_comparison.final_hp_delta = None;
        unknown_comparison.final_turn_delta = None;
        assert_eq!(
            shadow_adjudication(unknown),
            PotionSpendAdjudicationV1::UnknownWithoutNoPotionWitness {
                baseline_frontier_exhausted: true,
            }
        );
    }

    #[test]
    fn shadow_spend_adjudication_applies_safety_and_break_even_priority() {
        let mut excluded = policy_lane(
            "smoke",
            expenditure(10),
            VerifiedWinPotionDispositionV1::ContainsExcludedResource,
            PotionMarginalAssessmentV1::WorseFinalHpWithExtraResource,
            4,
            false,
        );
        excluded
            .witness
            .as_mut()
            .unwrap()
            .dominated_by
            .push("no_potion".to_owned());
        assert_eq!(
            shadow_adjudication(excluded),
            PotionSpendAdjudicationV1::ExcludedFromVictorySpend
        );

        let mut non_compliant = policy_lane(
            "passive",
            expenditure(15),
            VerifiedWinPotionDispositionV1::BoundedQualityOnly,
            PotionMarginalAssessmentV1::ImprovesFinalHp,
            0,
            false,
        );
        non_compliant.witness.as_mut().unwrap().lane_compliant = false;
        assert_eq!(
            shadow_adjudication(non_compliant),
            PotionSpendAdjudicationV1::ExcludedFromVictorySpend
        );

        let mut dominated = policy_lane(
            "block",
            expenditure(20),
            VerifiedWinPotionDispositionV1::BoundedQualityOnly,
            PotionMarginalAssessmentV1::WorseFinalHpWithExtraResource,
            1,
            false,
        );
        let dominated_witness = dominated.witness.as_mut().unwrap();
        dominated_witness
            .relative_to_no_potion
            .as_mut()
            .unwrap()
            .final_hp_delta = Some(-1);
        dominated_witness.dominated_by.push("no_potion".to_owned());
        assert_eq!(
            shadow_adjudication(dominated),
            PotionSpendAdjudicationV1::RejectDominated {
                dominated_by: vec!["no_potion".to_owned()],
            }
        );

        let mut no_gain = policy_lane(
            "strength",
            expenditure(30),
            VerifiedWinPotionDispositionV1::BoundedQualityOnly,
            PotionMarginalAssessmentV1::SameFinalHpWithExtraResource,
            -10,
            true,
        );
        no_gain
            .witness
            .as_mut()
            .unwrap()
            .relative_to_no_potion
            .as_mut()
            .unwrap()
            .final_hp_delta = Some(0);
        assert_eq!(
            shadow_adjudication(no_gain),
            PotionSpendAdjudicationV1::RejectNonPositiveHpGain { final_hp_delta: 0 }
        );

        let crosses_reserve = policy_lane(
            "duplication",
            expenditure(40),
            VerifiedWinPotionDispositionV1::ContainsReservedResource,
            PotionMarginalAssessmentV1::CrossesSurvivalReserve,
            2,
            true,
        );
        let PotionSpendAdjudicationV1::CompareContinuationValue {
            immediate_hp_gain,
            spend_urgency_question,
            ..
        } = shadow_adjudication(crosses_reserve)
        else {
            panic!("configured reserve crossing should remain a continuation question");
        };
        assert_eq!(immediate_hp_gain, 10);
        assert_eq!(
            spend_urgency_question.status,
            PotionSpendUrgencyQuestionStatusV1::Unavailable
        );

        let continuation = policy_lane(
            "regen",
            expenditure(50),
            VerifiedWinPotionDispositionV1::ContainsReservedResource,
            PotionMarginalAssessmentV1::ImprovesFinalHp,
            3,
            true,
        );
        let PotionSpendAdjudicationV1::CompareContinuationValue {
            immediate_hp_gain,
            break_even_retained_value_hp,
            final_turn_delta,
            potion_expenditures,
            spend_urgency_question,
            retained_value_evidence,
        } = shadow_adjudication(continuation)
        else {
            panic!("expected continuation-value comparison");
        };
        assert_eq!(immediate_hp_gain, 10);
        assert_eq!(break_even_retained_value_hp, 10);
        assert_eq!(final_turn_delta, 3);
        assert_eq!(potion_expenditures, 1);
        assert_eq!(
            spend_urgency_question.status,
            PotionSpendUrgencyQuestionStatusV1::Unavailable
        );
        assert_eq!(
            retained_value_evidence.run_context_status,
            PotionRunContinuationProjectionStatusV1::UnavailableLegacyCase
        );
        assert!(retained_value_evidence.exact_consumed_resources.is_empty());
        assert_eq!(
            retained_value_evidence.unmatched_expenditure_uuids,
            vec![50]
        );
    }

    #[test]
    fn isolated_fire_potion_lane_can_rescue_a_proven_no_potion_loss() {
        let mut combat = sts_oracle_runtime::test_support::blank_test_combat();
        combat.entities.player.current_hp = 1;
        combat.entities.monsters = vec![sts_oracle_runtime::test_support::planned_monster(
            EnemyId::JawWorm,
            1,
        )];
        combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 70))];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let search = |allowed_potion_slots, max_potions_used| {
            let root = CombatDecisionRoot::new(position.clone()).expect("combat root");
            let mut config = LocalTurnGraphWitnessConfig {
                satisfaction: OracleCombatWitnessSatisfaction::BudgetOrExhaustion,
                max_turn_depth: 2,
                max_potions_used: Some(max_potions_used),
                ..LocalTurnGraphWitnessConfig::default()
            };
            config.generator.max_engine_steps_per_transition = 256;
            config.generator.allowed_potion_slots = Some(allowed_potion_slots);
            let mut session = LocalTurnGraphWitnessSession::with_policy(
                root,
                config,
                retained_combat_tactical_policy_v1(),
            );
            session.advance(
                LocalTurnGraphWitnessQuantum {
                    additional_selections: 10_000,
                    additional_generation_work: 10_000,
                    additional_engine_steps: 2_560_000,
                    deadline: None,
                },
                &EngineCombatStepper,
            )
        };

        let no_potion = search(0, 0);
        assert_eq!(
            no_potion.status,
            LocalTurnGraphWitnessStatus::FrontierExhausted
        );
        assert!(no_potion.witness.is_none());

        let fire = search(1, 1);
        let witness = fire.witness.expect("Fire Potion should rescue the combat");
        let summary = summarize_witness(
            &position,
            &witness.actions,
            &witness.final_position,
            1,
            1,
            1,
            1,
            Some(1),
            256,
        )
        .expect("potion summary");
        assert_eq!(summary.final_hp, 1);
        assert!(summary.lane_compliant);
        assert_eq!(
            summary
                .potion_expenditures
                .iter()
                .map(|event| (event.id.as_str(), event.mode))
                .collect::<Vec<_>>(),
            vec![("FirePotion", PotionExpenditureModeV1::Use)]
        );
    }

    #[test]
    fn replay_marks_disallowed_fairy_revive_as_passive_lane_expenditure() {
        let mut combat = sts_oracle_runtime::test_support::blank_test_combat();
        combat.entities.player.current_hp = 1;
        combat.entities.monsters = vec![sts_oracle_runtime::test_support::planned_monster(
            EnemyId::JawWorm,
            1,
        )];
        combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 71))];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let actions = vec![TurnOptionAction {
            input: ClientInput::EndTurn,
            expected_successor_hash: "unused-by-audit-replay".into(),
            engine_steps: 0,
        }];

        let events = replay_potion_expenditures(&position, &actions, 256)
            .expect("Fairy Potion replay attribution");

        assert_eq!(
            events
                .iter()
                .map(|event| (event.id.as_str(), event.mode))
                .collect::<Vec<_>>(),
            vec![("FairyPotion", PotionExpenditureModeV1::Passive)]
        );
    }
}
