//! Run-play adapter for the first PairedRoot vNext vertical slice.
//!
//! The durable schema/store deliberately lives in `sts_paired_root`. This
//! module owns the runtime-specific import and projection work needed to turn
//! one retained Rust decision plus one freshly restored Java frame into a
//! probe root. It never publishes a run-play head mutation.

use crate::adapter_failure::{write_failure_report_if_absent, RunPlayAdapterFailureV1};

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sts_java_parity_compare::{
    construct_rust_scenario_root_v1, reconcile_card_birth_witness_v1, CardBirthTransitionCauseV1,
    CardBirthTransitionContextV1, JavaCardBirthObservedCardV1, ReconciledCardBirthProductV1,
};
use sts_java_parity_live::{
    act as java_live_act, load_scenario_plan, status as java_live_status,
    validate_native_combat_checkpoint_v1, validate_resumed_native_checkpoint_root_v1, ActionHandle,
    ActionReceipt, ConstructedScenarioReport, LiveStatus, ScenarioCapability, ScenarioPlan,
    ValidatedNativeCombatCheckpointV1,
};
use sts_java_parity_projection::{
    canonical_engine_seed_from_java_long, map_room_identity_from_java, map_runtime_settlement,
    neow_runtime_settlement, project_card_pools as java_card_pools,
    project_map_action_space as project_java_map_action_space,
    project_master_deck as java_master_deck_semantics,
    project_neow_action_space as project_java_neow_action_space,
    project_neow_rng_streams as java_neow_rng_streams, project_neow_room as project_java_neow_room,
    project_persistent_potions as java_persistent_potions,
    project_post_neow_rng_streams as java_post_neow_rng_streams,
    project_public_map as java_public_map, project_relics as java_relics,
    project_run_scalars as java_run_scalars, MAP_ACTION_SPACE_SCHEMA_V1,
    MAP_RUNTIME_SETTLEMENT_SCHEMA_V1, NEOW_ACTION_SPACE_SCHEMA_V1, NEOW_RNG_STREAMS_SCHEMA_V1,
    NEOW_ROOM_SCHEMA_V1, NEOW_RUNTIME_SETTLEMENT_SCHEMA_V1,
    PAIRED_PROJECTION_MEDIA_TYPE_V1 as PROJECTION_MEDIA_TYPE,
    PERSISTENT_POTIONS_SCHEMA_V1 as PERSISTENT_POTION_SLOTS_SCHEMA_V1, PUBLIC_MAP_SCHEMA_V1,
};
use sts_java_parity_protocol::{
    canonical_game_id, CardBirthWitness as JavaCardBirthWitness,
    CardBirthZone as JavaCardBirthZone, StateFrame as JavaStateFrame,
};
use sts_oracle_learning::agent::information::combat::{
    combat_public_intent_facts_v1, HiddenInformationReasonV1, ObservationEvidenceKindV1,
};
use sts_oracle_learning::agent::task::{
    TaskBudgetV1, TaskPrivilegedSupportProvenanceV1, TaskResearchPlanAttemptExitV1,
    TaskResearchPlanLaneAttemptV1, TaskResearchPlanOutcomeV1,
};
use sts_oracle_learning::ai::planner_core::PlannerMapRoom;
use sts_oracle_learning::content::cards::{
    ironclad_pool_for_rarity, java_id as java_card_id, CardRarity,
};
use sts_oracle_learning::content::events::neow::{
    choice_identity as rust_neow_choice_identity, NeowDrawback, NeowEligibilityV1, NeowRewardType,
};
use sts_oracle_learning::content::potions::{get_potion_definition, PotionId};
use sts_oracle_learning::content::relics::{RelicId, RelicState};
use sts_oracle_learning::eval::run_control::{
    build_decision_surface, run_control_session_combat_research_portfolio_with_options_v1,
    run_map_comparison_topology_v1, CombatResearchBoundedProgressRoleV1,
    CombatResearchBoundedProgressStateV1, CombatResearchPlanOptionsV1, DecisionCandidateKey,
    RunControlConfig, RunControlSession, RunControlSessionCheckpointV1, RunDecisionAction,
};
use sts_oracle_learning::runtime::combat::{
    CardLineageId, CardLineageOrigin, CombatCard, CombatCardBirthCause, CombatCardBirthReceipt,
    CombatState, Intent, Power,
};
use sts_oracle_learning::runtime::rng::{RngPool, StsRng};
use sts_oracle_learning::sim::combat::{
    apply_combat_input_to_stable_with_card_births_v1, CombatPosition, CombatStepLimits,
    CombatStepper, EngineCombatStepper,
};
use sts_oracle_learning::state::core::{
    ActiveCombat, ClientInput, CombatContext, EngineState, RoomCombatContext,
};
use sts_oracle_learning::state::events::{
    EventActionKind, EventId, EventOptionTransition, EventSelectionKind,
};
use sts_oracle_learning::state::map::node::RoomType;
use sts_oracle_learning::state::rewards::RewardItem;
use sts_paired_root::{
    action_handle, card_binding_id, transition_commitment_body, BlobRefV1, BundleRefV1,
    CapabilityKindV1, CapabilityPlanStatusV1, CapabilityPlanV1, CapabilityProofV1,
    CapabilityStatusV1, CapabilitySurfacePlanV1, CardBindingEvidenceV1, CombatRewardIdentityV1,
    ComparisonStatusV1, CoverageGapScopeV1, CoverageGapV1, CrossRuntimeCardBindingV1,
    DecisionProfileV1, DivergenceBodyV1, DivergenceStageV1, IdentityClaimV1, JavaCardLeaseV1,
    MapRoomIdentityV1, MonsterTargetV1, NeowDrawbackIdentityV1, NeowRewardIdentityV1,
    PairedActionHandleV1, PairedActionV1, PairedHeadCommitStatusV1,
    PairedHeadSelectionResolutionV1, PairedHeadSelectorStatusV1, PairedHeadSelectorV1,
    PairedHeadV1, ParentStateRootV2, ProjectionComparisonV1, ProjectionDomainV1, RestoreQualityV1,
    RootPreflightV1, RootProvenanceV1, RuntimeKindV1, RustCardLineageV1, RustStateSnapshotV2,
    StateRootActivationArtifactV2, StateRootActivationBodyV2, StateRootArtifactV2, StateRootBodyV2,
    StateRootStoreV2, TransitionReceiptBodyV1, TransitionTimingV1, TypedDifferenceV1,
    VersionIdentityV1, WorldProvenanceV1,
};

use crate::driver::{
    create_constructed_decision_session_v1, create_natural_session_v1, RunPlayControllerV1,
};
use crate::model::{RunPlayEvidenceIdentityV1, RunPlayHeadV1};
use crate::storage::RunPlayPathsV1;

const PREPARE_REPORT_SCHEMA_V1: &str = "sts-paired-root-prepare-report-v1";
const CONSTRUCTED_COMBAT_REPORT_SCHEMA_V1: &str = "sts-paired-root-constructed-combat-report-v1";
const PARENTLESS_NEOW_REPORT_SCHEMA_V1: &str = "sts-state-root-parentless-neow-report-v2";
const PREVIEW_REPORT_SCHEMA_V1: &str = "sts-paired-root-preview-report-v1";
const RECOVERY_REPORT_SCHEMA_V1: &str = "sts-paired-root-recovery-report-v1";
const NATIVE_MATERIALIZATION_REPORT_SCHEMA_V1: &str =
    "sts-paired-root-native-materialization-report-v1";
const NATIVE_ACTIVATION_REPORT_SCHEMA_V1: &str = "sts-paired-root-native-activation-report-v1";
const ACTION_OBSERVATION_REPORT_SCHEMA_V6: &str = "sts-paired-root-action-observation-v6";
const SEARCH_REPORT_SCHEMA_V1: &str = "sts-paired-root-search-report-v1";
const SEARCH_INTENT_RESOLUTION_SCHEMA_V1: &str = "sts-paired-root-search-intent-resolution-v1";
const WITNESS_TRANSACTION_PLAN_SCHEMA_V1: &str = "sts-paired-root-witness-transaction-plan-v1";
const WITNESS_TRANSACTION_REPORT_SCHEMA_V1: &str = "sts-paired-root-witness-transaction-report-v1";
const CARD_BIRTH_STABLE_PROOF_SCHEMA_V1: &str = "sts-paired-card-birth-java-proof-v1";
const ACTION_SPACE_SCHEMA_CARD_ONLY_V1: &str = "sts-paired-combat-card-action-space-v1";
const ACTION_SPACE_SCHEMA_POTION_V2: &str = "sts-paired-combat-action-space-v2";
const COMBAT_ROOT_SCHEMA_V1: &str = "sts-paired-combat-root-v1";
const COMBAT_ROOT_SCHEMA_PUBLIC_INTENT_V2: &str = "sts-paired-combat-root-v2";
const REWARD_ROOM_SCHEMA_V1: &str = "sts-paired-combat-reward-room-v1";
const REWARD_ACTION_SPACE_SCHEMA_V1: &str = "sts-paired-combat-reward-action-space-v1";
const REWARD_RUNTIME_SETTLEMENT_SCHEMA_V1: &str = "sts-paired-combat-reward-runtime-settlement-v1";
const CARD_REWARD_ROOM_SCHEMA_V1: &str = "sts-paired-card-reward-room-v1";
const CARD_REWARD_ACTION_SPACE_SCHEMA_V1: &str = "sts-paired-card-reward-action-space-v1";
const CARD_REWARD_RUNTIME_SETTLEMENT_SCHEMA_V1: &str =
    "sts-paired-card-reward-runtime-settlement-v1";
const EVENT_ROOM_SCHEMA_V1: &str = "sts-paired-event-room-v1";
const EVENT_ACTION_SPACE_SCHEMA_V1: &str = "sts-paired-event-action-space-v1";
const EVENT_RUNTIME_SETTLEMENT_SCHEMA_V1: &str = "sts-paired-event-runtime-settlement-v1";
const REWARD_PROFILE_PROJECTION_SCHEMAS_V1: [(ProjectionDomainV1, &str); 3] = [
    (ProjectionDomainV1::Map, PUBLIC_MAP_SCHEMA_V1),
    (ProjectionDomainV1::Room, REWARD_ROOM_SCHEMA_V1),
    (
        ProjectionDomainV1::RuntimeSettlement,
        REWARD_RUNTIME_SETTLEMENT_SCHEMA_V1,
    ),
];
const CARD_REWARD_PROFILE_PROJECTION_SCHEMAS_V1: [(ProjectionDomainV1, &str); 3] = [
    (ProjectionDomainV1::Map, PUBLIC_MAP_SCHEMA_V1),
    (ProjectionDomainV1::Room, CARD_REWARD_ROOM_SCHEMA_V1),
    (
        ProjectionDomainV1::RuntimeSettlement,
        CARD_REWARD_RUNTIME_SETTLEMENT_SCHEMA_V1,
    ),
];
const EVENT_PROFILE_PROJECTION_SCHEMAS_V1: [(ProjectionDomainV1, &str); 3] = [
    (ProjectionDomainV1::Map, PUBLIC_MAP_SCHEMA_V1),
    (ProjectionDomainV1::Room, EVENT_ROOM_SCHEMA_V1),
    (
        ProjectionDomainV1::RuntimeSettlement,
        EVENT_RUNTIME_SETTLEMENT_SCHEMA_V1,
    ),
];
const MAP_PROFILE_PROJECTION_SCHEMAS_V1: [(ProjectionDomainV1, &str); 2] = [
    (ProjectionDomainV1::Map, PUBLIC_MAP_SCHEMA_V1),
    (
        ProjectionDomainV1::RuntimeSettlement,
        MAP_RUNTIME_SETTLEMENT_SCHEMA_V1,
    ),
];
const NEOW_PROFILE_PROJECTION_SCHEMAS_V1: [(ProjectionDomainV1, &str); 3] = [
    (ProjectionDomainV1::Map, PUBLIC_MAP_SCHEMA_V1),
    (ProjectionDomainV1::Room, NEOW_ROOM_SCHEMA_V1),
    (
        ProjectionDomainV1::RuntimeSettlement,
        NEOW_RUNTIME_SETTLEMENT_SCHEMA_V1,
    ),
];
const SUPPORTED_ROOT_VERSION_COMPONENTS_V1: [&str; 6] = [
    "communicationmod_jar",
    "communicationmod_source",
    "run_play_workbench",
    "rust_rng_semantics",
    "rust_simulator_semantics",
    "slay_the_spire_jar",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActionSpaceProjectionVersion {
    CardOnlyV1,
    PotionV2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CombatProjectionVersion {
    LegacyV1,
    PublicIntentV2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CombatProjectionVersions {
    action_space: ActionSpaceProjectionVersion,
    state: CombatProjectionVersion,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparePairedRootRequestV1 {
    pub session: PathBuf,
    pub retained_selector: String,
    pub java_checkpoint_manifest: PathBuf,
    pub java_mirror_plan: PathBuf,
    pub java_live_output: PathBuf,
    pub store_root: PathBuf,
    pub attempt_directory: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct PrepareParentlessNaturalNeowRootRequestV1 {
    pub session: PathBuf,
    pub seed: u64,
    pub ascension: u8,
    pub first_java_frame: PathBuf,
    pub first_java_talk_receipt: PathBuf,
    pub java_recovery_storage: PathBuf,
    pub native_autosave_sha256: String,
    pub java_live_output: PathBuf,
    pub store_root: PathBuf,
    pub attempt_directory: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct PrepareConstructedCombatRootRequestV1 {
    pub session: PathBuf,
    pub scenario_plan: PathBuf,
    pub scenario_report: PathBuf,
    pub java_live_output: PathBuf,
    pub store_root: PathBuf,
    pub attempt_directory: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootSourcePreflightV1 {
    pub schema: String,
    pub status: String,
    pub source_session: String,
    pub source_head_revision: u64,
    pub rust_rng_semantics: String,
    pub rust_simulator_semantics: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootPreparationReportV1 {
    pub schema: String,
    pub status: String,
    pub source_session: String,
    pub retained_selector: String,
    pub source_archive_node_id: String,
    pub java_checkpoint_manifest: String,
    pub java_frame_path: String,
    pub components: Vec<ProjectionPreparationSummaryV1>,
    pub gaps: Vec<CoverageGapV1>,
    pub source_head_guard: SourceHeadGuardV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConstructedCombatRootReportV1 {
    pub schema: String,
    pub status: String,
    pub case_id: String,
    pub scenario_id: String,
    pub recipe_program_id: String,
    pub source_session: String,
    pub source_archive_node_id: String,
    pub scenario_plan_path: String,
    pub scenario_report_path: String,
    pub java_frame_path: String,
    pub components: Vec<ProjectionPreparationSummaryV1>,
    pub gaps: Vec<CoverageGapV1>,
    pub source_head_guard: SourceHeadGuardV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_space_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_runtime_bundle: Option<BundleRefV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_runtime_entrypoint: Option<String>,
    pub java_cold_recovery: RestoreQualityV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParentlessNaturalNeowRootReportV1 {
    pub schema: String,
    pub status: String,
    pub seed: String,
    pub ascension: u8,
    pub neow_offer: String,
    pub source_session: String,
    pub source_archive_node_id: String,
    pub java_frame_path: String,
    pub components: Vec<ProjectionPreparationSummaryV1>,
    pub source_head_guard: SourceHeadGuardV1,
    pub root_id: String,
    pub activation_id: String,
    pub action_space_id: String,
    pub java_runtime_bundle: BundleRefV1,
    pub java_runtime_entrypoint: String,
    pub recovery_certificate: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PreviewPairedRootActionRequestV1 {
    pub source_session: PathBuf,
    pub source_root_id: String,
    pub source_activation_id: String,
    pub action_handle_id: String,
    pub java_live_output: PathBuf,
    pub store_root: PathBuf,
    pub attempt_directory: PathBuf,
    pub adapter_failure_report: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct ObservePairedRootActionsRequestV1 {
    pub root_id: String,
    pub store_root: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct SearchPairedRootRequestV1 {
    pub root_id: String,
    pub store_root: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct RecoverPairedRootRequestV1 {
    pub source_session: PathBuf,
    pub native_root_id: String,
    pub native_activation_id: String,
    pub target_root_id: String,
    pub java_live_output: PathBuf,
    pub store_root: PathBuf,
    pub attempt_directory: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct PreviewPairedRootWitnessTransactionRequestV1 {
    pub source_session: PathBuf,
    pub source_root_id: String,
    pub source_activation_id: String,
    pub witness_plan: PathBuf,
    pub java_live_output: PathBuf,
    pub store_root: PathBuf,
    pub attempt_directory: PathBuf,
    pub adapter_failure_report: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct MaterializePairedRootNativeRequestV1 {
    pub target_root_id: String,
    pub store_root: PathBuf,
    pub output_directory: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct ActivatePairedRootNativeRequestV1 {
    pub target_root_id: String,
    pub run_root: PathBuf,
    pub java_live_output: PathBuf,
    pub store_root: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootPreviewReportV1 {
    pub schema: String,
    pub status: String,
    pub source_root_id: String,
    pub source_activation_id: String,
    pub action: PairedActionHandleV1,
    pub components: Vec<ProjectionPreparationSummaryV1>,
    pub source_head_guard: SourceHeadGuardV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub successor_root_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub successor_activation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergence_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub successor_boundary: Option<PairedRootSuccessorBoundaryV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub java_evidence: Option<BlobRefV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_evidence: Option<BlobRefV1>,
    pub timings: TransitionTimingV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootSuccessorBoundaryV1 {
    pub kind: String,
    pub stable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_phase: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootActionObservationReportV6 {
    pub schema: String,
    pub root_id: String,
    pub action_space_id: String,
    pub decision_context: PairedRootDecisionContextV1,
    pub observed_actions: Vec<PairedRootActionObservationV1>,
    pub authorizations: Vec<PairedRootActionAuthorizationV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootDecisionContextV1 {
    pub profile: DecisionProfileV1,
    pub run: Value,
    pub master_deck: Value,
    pub relics: Value,
    pub potions: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootActionAuthorizationV1 {
    pub handle_id: String,
    pub capability: CapabilityProofV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootActionObservationV1 {
    pub handle: PairedActionHandleV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<PairedRootObservedCardV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub potion: Option<PairedRootObservedPotionV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_reward: Option<CombatRewardIdentityV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_reward: Option<PairedRootObservedCardRewardV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_option: Option<PairedRootObservedEventOptionV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootObservedEventOptionV1 {
    pub action_kind: PairedRootObservedEventActionKindV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect_summary: Option<String>,
    pub followup: PairedRootObservedEventFollowupV1,
    pub repeatable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PairedRootObservedEventActionKindV1 {
    Unknown,
    Leave,
    Continue,
    Accept,
    Decline,
    Fight,
    Trade,
    DeckOperation,
    Gain,
    Special,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PairedRootObservedEventFollowupV1 {
    None,
    EventScreen,
    EventComplete,
    RemoveCardSelection,
    UpgradeCardSelection,
    TransformCardSelection,
    DuplicateCardSelection,
    OfferCardSelection,
    UnknownSelection,
    Reward,
    Combat,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PairedRootObservedCardRewardV1 {
    Card { card_id: String, upgrades: u8 },
    Skip,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootSearchReportV1 {
    pub schema: String,
    pub status: String,
    pub root_id: String,
    pub action_space_id: String,
    pub rust_checkpoint: BlobRefV1,
    pub policy: PairedRootSearchPolicyV1,
    pub summary_exit: TaskResearchPlanAttemptExitV1,
    pub lanes: Vec<TaskResearchPlanLaneAttemptV1>,
    pub witnesses: Vec<PairedRootSearchWitnessV1>,
    pub bounded_frontier: Vec<PairedRootSearchBoundedFrontierV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootSearchPolicyV1 {
    pub policy_id: String,
    pub owner: String,
    pub options: CombatResearchPlanOptionsV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootSearchWitnessV1 {
    pub witness_id: String,
    pub plan_step_count: usize,
    pub translation: PairedRootSearchIntentTranslationV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action_handle_id: Option<String>,
    pub outcome: TaskResearchPlanOutcomeV1,
    pub satisfies_requested_quality: bool,
    pub supporting_provenances: Vec<TaskPrivilegedSupportProvenanceV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootSearchBoundedFrontierV1 {
    pub provenance: TaskPrivilegedSupportProvenanceV1,
    pub max_player_turn: u32,
    pub recent_turn_survival_envelope: Vec<CombatResearchBoundedProgressStateV1>,
    pub paths: Vec<PairedRootSearchBoundedPathV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootSearchBoundedPathV1 {
    pub roles: Vec<CombatResearchBoundedProgressRoleV1>,
    pub endpoint: CombatResearchBoundedProgressStateV1,
    pub translation: PairedRootSearchIntentTranslationV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PairedRootSearchIntentTranslationV1 {
    Complete {
        intents: Vec<PairedRootSearchActionIntentV1>,
    },
    MechanicsGap {
        step_ordinal: usize,
        action_kind: String,
        reason: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PairedRootSearchActionIntentV1 {
    PlayCard {
        rust_lineage: RustCardLineageV1,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<MonsterTargetV1>,
    },
    EndTurn,
    UsePotion {
        slot: u8,
        potion_id: String,
        potency: i32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<MonsterTargetV1>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootSearchIntentResolutionV1 {
    pub schema: String,
    pub status: String,
    pub root_id: String,
    pub action_space_id: String,
    pub witness_id: String,
    pub step_ordinal: usize,
    pub intent: PairedRootSearchActionIntentV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_handle_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootWitnessTransactionPlanV1 {
    pub schema: String,
    pub witness_id: String,
    pub initial_root_id: String,
    pub intents: Vec<PairedRootSearchActionIntentV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootWitnessTransactionReportV1 {
    pub schema: String,
    pub status: String,
    pub witness_id: String,
    pub initial_root_id: String,
    pub reached_root_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_step_ordinal: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    pub steps: Vec<PairedRootWitnessTransactionStepV1>,
    pub source_head_guard: SourceHeadGuardV1,
    pub timings: TransitionTimingV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootWitnessTransactionStepV1 {
    pub step_ordinal: usize,
    pub source_root_id: String,
    pub resolution: PairedRootSearchIntentResolutionV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<PairedRootPreviewReportV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootObservedCardV1 {
    pub card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootObservedPotionV1 {
    pub potion_id: String,
    pub potency: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootRecoveryReportV1 {
    pub schema: String,
    pub status: String,
    pub native_root_id: String,
    pub native_activation_id: String,
    pub target_root_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_activation_id: Option<String>,
    pub steps: Vec<PairedRootRecoveryReportStepV1>,
    pub source_head_guard: SourceHeadGuardV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergence_id: Option<String>,
    pub timings: TransitionTimingV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootRecoveryReportStepV1 {
    pub ordinal: usize,
    pub source_root_id: String,
    pub source_activation_id: String,
    pub transition_commitment_id: String,
    pub action: PairedActionHandleV1,
    pub target_root_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_activation_id: Option<String>,
    pub components: Vec<ProjectionPreparationSummaryV1>,
    pub java_replay_receipt: BlobRefV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergence_id: Option<String>,
    pub timings: TransitionTimingV1,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootNativeMaterializationReportV1 {
    pub schema: String,
    pub requested_target_root_id: String,
    pub native_root_id: String,
    pub recovery_certificate_id: String,
    pub java_bundle_id: String,
    pub content_profile: String,
    pub java_entrypoint: String,
    pub materialized_directory: String,
    pub storage_directory: String,
    pub file_count: usize,
    pub recovery_steps: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootNativeActivationReportV1 {
    pub schema: String,
    pub requested_target_root_id: String,
    pub native_root_id: String,
    pub native_activation_id: String,
    pub source_session: String,
    pub version_compatibility: Vec<PairedRootVersionCompatibilityV1>,
    pub components: Vec<ProjectionPreparationSummaryV1>,
    pub source_head_guard: SourceHeadGuardV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootVersionCompatibilityV1 {
    pub component: String,
    pub stored_identity: String,
    pub active_identity: String,
    pub enforcement: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootCasProofReportV1 {
    pub schema: String,
    pub head_name: String,
    pub source_root_id: String,
    pub steps: Vec<PairedRootCasProofStepV1>,
    pub final_head: PairedHeadV1,
    pub execution_scope: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootCasProofStepV1 {
    pub receipt_id: String,
    pub before: PairedHeadV1,
    pub after: PairedHeadV1,
    pub stale_retry_rejected: bool,
    pub stale_retry_error: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootHeadInitReportV1 {
    pub schema: String,
    pub status: String,
    pub head: PairedHeadV1,
    pub execution_scope: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootHeadCommitReportV1 {
    pub schema: String,
    pub status: PairedHeadCommitStatusV1,
    pub receipt_id: String,
    pub committed_head: PairedHeadV1,
    pub current_head: PairedHeadV1,
    pub execution_scope: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootHeadSelectorReportV1 {
    pub schema: String,
    pub status: PairedHeadSelectorStatusV1,
    pub selector: PairedHeadSelectorV1,
    pub execution_scope: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootHeadResolutionReportV1 {
    pub schema: String,
    pub status: String,
    pub resolution: PairedHeadSelectionResolutionV1,
    pub execution_scope: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairedRootSelectedHeadCommitReportV1 {
    pub schema: String,
    pub selector: PairedHeadSelectorV1,
    pub status: PairedHeadCommitStatusV1,
    pub receipt_id: String,
    pub committed_head: PairedHeadV1,
    pub current_head: PairedHeadV1,
    pub execution_scope: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceHeadGuardV1 {
    pub before_sha256: String,
    pub after_sha256: String,
    pub file_count: usize,
    pub unchanged: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectionPreparationSummaryV1 {
    pub domain: ProjectionDomainV1,
    pub schema: String,
    pub status: ComparisonStatusV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_difference: Option<TypedDifferenceV1>,
}

struct PreparedProjectionV1 {
    comparison: ProjectionComparisonV1,
}

pub(crate) fn prepare_paired_root_probe_v1(
    request: &PreparePairedRootRequestV1,
) -> Result<PairedRootPreparationReportV1, String> {
    // This check must stay ahead of attempt/store creation and any Java-frame
    // dependency. A historical simulator checkpoint cannot become current
    // evidence by being paired with a freshly launched Java runtime.
    preflight_paired_root_source_v1(&request.session)?;
    create_fresh_directory(&request.attempt_directory)?;
    let store = StateRootStoreV2::new(&request.store_root).with_operation_verification_cache();
    store.initialize()?;
    let source_head_before = source_head_inventory(&request.session)?;

    let live = java_live_status(&request.java_live_output)?;
    validate_live_stable_combat_boundary(&live)?;
    let checkpoint = validate_native_combat_checkpoint_v1(
        &request.java_checkpoint_manifest,
        &request.java_mirror_plan,
    )?;

    let mut controller = RunPlayControllerV1::open_at(
        RunPlayPathsV1::new(&request.session),
        &request.retained_selector,
    )?;
    let checkpoint_rust = controller.current_external_exact_checkpoint_v1()?;
    let session = checkpoint_rust.clone().into_session()?;
    let source_archive_node_id = controller.head().position.archive_node_id().to_owned();

    let coverage_gaps = required_java_frame_coverage_gaps(&live.frame);
    if !coverage_gaps.is_empty() {
        let report = preflight_coverage_stop_report(
            request,
            &live,
            &checkpoint,
            controller.head().session_id.clone(),
            source_archive_node_id,
            coverage_gaps,
        );
        return finalize_source_head_guard(request, source_head_before, report);
    }

    let checkpoint = validate_resumed_native_checkpoint_root_v1(
        &request.java_checkpoint_manifest,
        &request.java_mirror_plan,
        &live.frame,
    )?;
    validate_live_root_identity(&live, &checkpoint)?;

    let java_frame_blob = put_json(&store, &live.frame)?;
    let java_master_deck_projection = java_master_deck_semantics(&live.frame)?;
    let java_master_deck_blob = put_json(&store, &java_master_deck_projection)?;
    let rust_identity_projection = rust_identity_projection(&session)?;
    let rust_identity_blob = put_json(&store, &rust_identity_projection)?;
    let bindings =
        build_master_deck_bindings(&live, &session, &java_master_deck_blob, &rust_identity_blob)?;
    let leases = build_java_card_leases(&live, &session, &bindings)?;
    let binding_by_java = leases
        .iter()
        .map(|lease| (lease.java_card_uuid.as_str(), lease.binding_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let binding_by_rust = bindings
        .iter()
        .map(|binding| {
            (
                rust_lineage_key(&binding.rust_lineage),
                binding.binding_id.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut prepared = vec![
        compare_projection(
            &store,
            ProjectionDomainV1::RunScalars,
            "sts-paired-run-scalars-v1",
            java_run_scalars(&live.frame)?,
            rust_run_scalars(&session)?,
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::MasterDeck,
            "sts-paired-master-deck-order-v1",
            java_master_deck_projection,
            rust_master_deck_semantics(&session),
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::Relics,
            "sts-paired-relic-order-counter-v1",
            java_relics(&live.frame)?,
            rust_combat_relics(&session)?,
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::Potions,
            "sts-paired-potion-slots-v1",
            java_potions(&live.frame)?,
            rust_potions(&session)?,
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::RngStreams,
            "sts-paired-rng-streams-v1",
            java_post_neow_rng_streams(&live.frame)?,
            rust_rng_streams(&session)?,
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::CardPools,
            "sts-paired-card-pools-v1",
            java_card_pools(&live.frame)?,
            rust_card_pools(&session)?,
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::Combat,
            combat_projection_schema(CombatProjectionVersion::PublicIntentV2),
            java_combat(
                &live.frame,
                &binding_by_java,
                CombatProjectionVersion::PublicIntentV2,
            )?,
            rust_combat(
                &session,
                &binding_by_rust,
                CombatProjectionVersion::PublicIntentV2,
            )?,
        )?,
    ];
    let java_actions = java_action_space(
        &live,
        &binding_by_java,
        ActionSpaceProjectionVersion::PotionV2,
    )?;
    let rust_actions = rust_action_space(
        &session,
        &binding_by_rust,
        ActionSpaceProjectionVersion::PotionV2,
    )?;
    prepared.push(compare_projection(
        &store,
        ProjectionDomainV1::ActionSpace,
        ACTION_SPACE_SCHEMA_POTION_V2,
        java_actions.clone(),
        rust_actions.clone(),
    )?);
    prepared.push(relational_binding_projection(
        &store,
        &bindings,
        &leases,
        &live.frame,
        &session,
    )?);
    prepared.sort_by_key(|projection| projection.comparison.domain);

    let required_domains = BTreeSet::from([
        ProjectionDomainV1::RunScalars,
        ProjectionDomainV1::MasterDeck,
        ProjectionDomainV1::Relics,
        ProjectionDomainV1::Potions,
        ProjectionDomainV1::RngStreams,
        ProjectionDomainV1::CardPools,
        ProjectionDomainV1::Combat,
        ProjectionDomainV1::ActionSpace,
        ProjectionDomainV1::CardBindings,
    ]);
    let components = prepared
        .iter()
        .map(|projection| ProjectionPreparationSummaryV1 {
            domain: projection.comparison.domain,
            schema: projection.comparison.projection_schema.clone(),
            status: projection.comparison.status,
            first_difference: projection.comparison.first_difference.clone(),
        })
        .collect::<Vec<_>>();
    let failed = prepared.iter().any(|projection| {
        required_domains.contains(&projection.comparison.domain)
            && projection.comparison.status != ComparisonStatusV1::Exact
    });
    if failed {
        let report = PairedRootPreparationReportV1 {
            schema: PREPARE_REPORT_SCHEMA_V1.to_owned(),
            status: "preflight_stopped".to_owned(),
            source_session: controller.head().session_id.clone(),
            retained_selector: request.retained_selector.clone(),
            source_archive_node_id,
            java_checkpoint_manifest: checkpoint.manifest.path,
            java_frame_path: live.frame_path,
            components,
            gaps: Vec::new(),
            source_head_guard: pending_source_head_guard(),
            root_id: None,
            activation_id: None,
        };
        return finalize_source_head_guard(request, source_head_before, report);
    }

    let action_space_id = paired_action_space_id(&java_actions)?;
    let proof = put_json(
        &store,
        &json!({
            "schema": "sts-paired-capability-proof-v1",
            "components": prepared.iter().map(|item| json!({
                "domain": item.comparison.domain,
                "java_sha256": item.comparison.java.sha256,
                "rust_sha256": item.comparison.rust.sha256,
            })).collect::<Vec<_>>(),
        }),
    )?;
    let capability_plan =
        capability_surface_plan_for_profile(DecisionProfileV1::CombatPlayerDecision, &session)?;
    let (gaps, capabilities) = capability_plan.materialize(&proof);

    let rust_snapshot_directory = request.attempt_directory.join("rust-recovery");
    create_fresh_directory(&rust_snapshot_directory)?;
    write_new(
        &rust_snapshot_directory.join("checkpoint.msgpack"),
        &rmp_serde::to_vec_named(&checkpoint_rust)
            .map_err(|error| format!("failed to encode exact Rust checkpoint: {error}"))?,
    )?;
    write_new_json(
        &rust_snapshot_directory.join("source.json"),
        &json!({
            "schema": "sts-paired-rust-recovery-source-v1",
            "session_id": controller.head().session_id,
            "archive_node_id": source_archive_node_id,
            "retained_selector": request.retained_selector,
        }),
    )?;
    let rust_bundle = store.put_directory(&rust_snapshot_directory)?;

    let mut versions = versions_for_root(controller.head(), &live)?;
    versions.sort_by(|left, right| {
        (&left.component, &left.identity).cmp(&(&right.component, &right.identity))
    });
    let mut java_source_identity = vec![
        IdentityClaimV1 {
            kind: "checkpoint_manifest_sha256".to_owned(),
            value: checkpoint.manifest.sha256.clone(),
        },
        IdentityClaimV1 {
            kind: "content_profile".to_owned(),
            value: required_str(&live.frame["parity_internal"], "content_profile")?.to_owned(),
        },
    ];
    java_source_identity
        .sort_by(|left, right| (&left.kind, &left.value).cmp(&(&right.kind, &right.value)));
    let mut rust_source_identity = vec![
        IdentityClaimV1 {
            kind: "archive_node_id".to_owned(),
            value: source_archive_node_id.clone(),
        },
        IdentityClaimV1 {
            kind: "run_play_session_id".to_owned(),
            value: controller.head().session_id.clone(),
        },
    ];
    rust_source_identity
        .sort_by(|left, right| (&left.kind, &left.value).cmp(&(&right.kind, &right.value)));
    let root = store.create_root(StateRootBodyV2 {
        provenance: RootProvenanceV1::Probe,
        world_provenance: WorldProvenanceV1::Natural,
        parent: None,
        decision_profile: DecisionProfileV1::CombatPlayerDecision,
        versions,
        content_profile: identity_claim(&java_source_identity, "content_profile")?.to_owned(),
        rust_snapshot: RustStateSnapshotV2 {
            bundle: rust_bundle,
            entrypoint: "run_control_exact_checkpoint".to_owned(),
            source_identity: rust_source_identity,
        },
        preflight: RootPreflightV1 {
            action_space_id,
            components: prepared
                .into_iter()
                .map(|projection| projection.comparison)
                .collect(),
            uncovered: gaps.clone(),
        },
        card_bindings: bindings,
        capabilities,
    })?;
    let activation = store.create_activation(StateRootActivationBodyV2 {
        root_id: root.root_id.clone(),
        java_runtime_epoch: java_runtime_epoch(&live),
        java_frame: java_frame_blob,
        action_space_id: root.root.preflight.action_space_id.clone(),
        card_leases: leases,
    })?;

    let report = PairedRootPreparationReportV1 {
        schema: PREPARE_REPORT_SCHEMA_V1.to_owned(),
        status: "ready".to_owned(),
        source_session: controller.head().session_id.clone(),
        retained_selector: request.retained_selector.clone(),
        source_archive_node_id,
        java_checkpoint_manifest: checkpoint.manifest.path,
        java_frame_path: live.frame_path,
        components,
        gaps,
        source_head_guard: pending_source_head_guard(),
        root_id: Some(root.root_id),
        activation_id: Some(activation.activation_id),
    };
    finalize_source_head_guard(request, source_head_before, report)
}

pub(crate) fn prepare_constructed_combat_root_v1(
    request: &PrepareConstructedCombatRootRequestV1,
) -> Result<ConstructedCombatRootReportV1, String> {
    if request.session.exists() {
        return Err(format!(
            "constructed combat source session must be create-new: {}",
            request.session.display()
        ));
    }
    create_fresh_directory(&request.attempt_directory)?;
    let store = StateRootStoreV2::new(&request.store_root).with_operation_verification_cache();
    store.initialize()?;

    let plan = load_scenario_plan(&request.scenario_plan)?;
    let report_bytes = fs::read(&request.scenario_report).map_err(|error| {
        format!(
            "failed to read constructed scenario report {}: {error}",
            request.scenario_report.display()
        )
    })?;
    let scenario_report: ConstructedScenarioReport = serde_json::from_slice(&report_bytes)
        .map_err(|error| format!("failed to decode constructed scenario report: {error}"))?;
    let live = java_live_status(&request.java_live_output)?;
    validate_live_stable_combat_boundary(&live)?;
    validate_constructed_scenario_source_v1(
        &request.scenario_plan,
        &plan,
        &scenario_report,
        &live,
    )?;

    let constructed = construct_rust_scenario_root_v1(&plan)?;
    let seed = plan
        .run
        .seed
        .as_deref()
        .ok_or_else(|| "constructed Rust root omitted its declared seed".to_owned())?
        .parse::<u64>()
        .map_err(|_| "constructed Rust root seed is not decimal u64".to_owned())?;
    let ascension_level = u8::try_from(plan.run.ascension)
        .map_err(|_| "constructed Rust root ascension exceeds u8".to_owned())?;
    let mut session = RunControlSession::new(RunControlConfig {
        seed,
        ascension_level,
        final_act: false,
        player_class: "Ironclad",
        ..RunControlConfig::default()
    });
    session.run_state = constructed.run_state;
    session.engine_state = constructed.engine_state.clone();
    session.active_combat = Some(ActiveCombat::new_for_encounter(
        constructed.engine_state,
        constructed.combat,
        constructed.encounter,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));
    session.register_active_combat_sequence_v1()?;
    let checkpoint = RunControlSessionCheckpointV1::from_session(&session);
    let default_budget = TaskBudgetV1 {
        max_world_transitions: 4096,
        max_reloads: 0,
    };
    let combat_plan_options = CombatResearchPlanOptionsV1::default();
    create_constructed_decision_session_v1(
        &RunPlayPathsV1::new(&request.session),
        checkpoint,
        &plan.recipe.scenario_id,
        default_budget,
        combat_plan_options,
    )?;

    let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(&request.session))?;
    let checkpoint_rust = controller.current_external_exact_checkpoint_v1()?;
    let session = checkpoint_rust.clone().into_session()?;
    let source_session = controller.head().session_id.clone();
    let source_archive_node_id = controller.head().position.archive_node_id().to_owned();
    let source_head_before = source_head_inventory(&request.session)?;

    let coverage_gaps = required_java_frame_coverage_gaps(&live.frame);
    if !coverage_gaps.is_empty() {
        let report = ConstructedCombatRootReportV1 {
            schema: CONSTRUCTED_COMBAT_REPORT_SCHEMA_V1.to_owned(),
            status: "preflight_stopped".to_owned(),
            case_id: plan.recipe.case_id.clone(),
            scenario_id: plan.recipe.scenario_id.clone(),
            recipe_program_id: plan.recipe.recipe_id.clone(),
            source_session,
            source_archive_node_id,
            scenario_plan_path: absolute_path_text(&request.scenario_plan)?,
            scenario_report_path: absolute_path_text(&request.scenario_report)?,
            java_frame_path: live.frame_path,
            components: unknown_projection_summaries(&coverage_gaps),
            gaps: coverage_gaps,
            source_head_guard: checked_source_head_guard(&request.session, source_head_before)?,
            root_id: None,
            activation_id: None,
            action_space_id: None,
            java_runtime_bundle: None,
            java_runtime_entrypoint: None,
            java_cold_recovery: RestoreQualityV1::Unsupported,
        };
        return Ok(report);
    }

    let java_frame_blob = put_json(&store, &live.frame)?;
    let recipe_blob = put_json(&store, &plan.recipe)?;
    let bindings = build_constructed_recipe_bindings_v1(&plan, &live, &session, &recipe_blob)?;
    let leases = build_constructed_recipe_leases_v1(&live, &bindings)?;
    let binding_by_java = leases
        .iter()
        .map(|lease| (lease.java_card_uuid.as_str(), lease.binding_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let binding_by_rust = bindings
        .iter()
        .map(|binding| {
            (
                rust_lineage_key(&binding.rust_lineage),
                binding.binding_id.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let java_master_deck_projection = java_master_deck_semantics(&live.frame)?;

    let mut prepared = vec![
        compare_projection(
            &store,
            ProjectionDomainV1::RunScalars,
            "sts-paired-run-scalars-v1",
            java_run_scalars(&live.frame)?,
            rust_run_scalars(&session)?,
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::MasterDeck,
            "sts-paired-master-deck-order-v1",
            java_master_deck_projection,
            rust_master_deck_semantics(&session),
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::Relics,
            "sts-paired-relic-order-counter-v1",
            java_relics(&live.frame)?,
            rust_combat_relics(&session)?,
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::Potions,
            "sts-paired-potion-slots-v1",
            java_potions(&live.frame)?,
            rust_potions(&session)?,
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::RngStreams,
            "sts-paired-rng-streams-v1",
            java_post_neow_rng_streams(&live.frame)?,
            rust_rng_streams(&session)?,
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::CardPools,
            "sts-paired-card-pools-v1",
            java_card_pools(&live.frame)?,
            rust_card_pools(&session)?,
        )?,
        compare_projection(
            &store,
            ProjectionDomainV1::Combat,
            combat_projection_schema(CombatProjectionVersion::PublicIntentV2),
            java_combat(
                &live.frame,
                &binding_by_java,
                CombatProjectionVersion::PublicIntentV2,
            )?,
            rust_combat(
                &session,
                &binding_by_rust,
                CombatProjectionVersion::PublicIntentV2,
            )?,
        )?,
    ];
    let java_actions = java_action_space(
        &live,
        &binding_by_java,
        ActionSpaceProjectionVersion::PotionV2,
    )?;
    let rust_actions = rust_action_space(
        &session,
        &binding_by_rust,
        ActionSpaceProjectionVersion::PotionV2,
    )?;
    prepared.push(compare_projection(
        &store,
        ProjectionDomainV1::ActionSpace,
        ACTION_SPACE_SCHEMA_POTION_V2,
        java_actions.clone(),
        rust_actions,
    )?);
    prepared.push(relational_binding_projection(
        &store,
        &bindings,
        &leases,
        &live.frame,
        &session,
    )?);
    prepared.sort_by_key(|projection| projection.comparison.domain);
    let components = projection_summaries(&prepared);
    if prepared
        .iter()
        .any(|projection| projection.comparison.status != ComparisonStatusV1::Exact)
    {
        return Ok(ConstructedCombatRootReportV1 {
            schema: CONSTRUCTED_COMBAT_REPORT_SCHEMA_V1.to_owned(),
            status: "diverged".to_owned(),
            case_id: plan.recipe.case_id.clone(),
            scenario_id: plan.recipe.scenario_id.clone(),
            recipe_program_id: plan.recipe.recipe_id.clone(),
            source_session,
            source_archive_node_id,
            scenario_plan_path: absolute_path_text(&request.scenario_plan)?,
            scenario_report_path: absolute_path_text(&request.scenario_report)?,
            java_frame_path: live.frame_path,
            components,
            gaps: Vec::new(),
            source_head_guard: checked_source_head_guard(&request.session, source_head_before)?,
            root_id: None,
            activation_id: None,
            action_space_id: None,
            java_runtime_bundle: None,
            java_runtime_entrypoint: None,
            java_cold_recovery: RestoreQualityV1::Unsupported,
        });
    }

    let action_space_id = paired_action_space_id(&java_actions)?;
    let proof = put_json(
        &store,
        &json!({
            "schema": "sts-paired-capability-proof-v1",
            "constructed_scenario_id": plan.recipe.scenario_id,
            "components": prepared.iter().map(|item| json!({
                "domain": item.comparison.domain,
                "java_sha256": item.comparison.java.sha256,
                "rust_sha256": item.comparison.rust.sha256,
            })).collect::<Vec<_>>(),
        }),
    )?;
    let capability_plan =
        capability_surface_plan_for_profile(DecisionProfileV1::CombatPlayerDecision, &session)?;
    let (gaps, capabilities) = capability_plan.materialize(&proof);

    let java_recovery_directory = request.attempt_directory.join("java-recovery-recipe");
    create_fresh_directory(&java_recovery_directory)?;
    write_new(
        &java_recovery_directory.join("scenario-plan.json"),
        &fs::read(&request.scenario_plan)
            .map_err(|error| format!("failed to retain constructed scenario plan: {error}"))?,
    )?;
    write_new(
        &java_recovery_directory.join("scenario-report.json"),
        &report_bytes,
    )?;
    write_new_json(
        &java_recovery_directory.join("recovery-scope.json"),
        &json!({
            "schema": "sts-paired-constructed-java-recovery-scope-v1",
            "status": "cold_recovery_unverified",
            "entrypoint": "fresh_scenario_recipe",
            "scenario_id": plan.recipe.scenario_id,
            "case_id": plan.recipe.case_id,
        }),
    )?;
    let java_bundle = store.put_directory(&java_recovery_directory)?;

    let rust_snapshot_directory = request.attempt_directory.join("rust-recovery");
    create_fresh_directory(&rust_snapshot_directory)?;
    write_new(
        &rust_snapshot_directory.join("checkpoint.msgpack"),
        &rmp_serde::to_vec_named(&checkpoint_rust)
            .map_err(|error| format!("failed to encode constructed Rust checkpoint: {error}"))?,
    )?;
    write_new_json(
        &rust_snapshot_directory.join("source.json"),
        &json!({
            "schema": "sts-paired-constructed-rust-source-v1",
            "session_id": source_session,
            "archive_node_id": source_archive_node_id,
            "scenario_id": plan.recipe.scenario_id,
        }),
    )?;
    let rust_bundle = store.put_directory(&rust_snapshot_directory)?;

    let mut versions = versions_for_root(controller.head(), &live)?;
    versions.sort_by(|left, right| {
        (&left.component, &left.identity).cmp(&(&right.component, &right.identity))
    });
    let root = store.create_root(StateRootBodyV2 {
        provenance: RootProvenanceV1::Forge,
        world_provenance: WorldProvenanceV1::ValidatedConstructed,
        parent: None,
        decision_profile: DecisionProfileV1::CombatPlayerDecision,
        versions,
        content_profile: "all_unlocked".to_owned(),
        rust_snapshot: RustStateSnapshotV2 {
            bundle: rust_bundle,
            entrypoint: "run_control_exact_checkpoint".to_owned(),
            source_identity: vec![
                IdentityClaimV1 {
                    kind: "archive_node_id".to_owned(),
                    value: source_archive_node_id.clone(),
                },
                IdentityClaimV1 {
                    kind: "run_play_session_id".to_owned(),
                    value: source_session.clone(),
                },
                IdentityClaimV1 {
                    kind: "scenario_id".to_owned(),
                    value: plan.recipe.scenario_id.clone(),
                },
            ],
        },
        preflight: RootPreflightV1 {
            action_space_id: action_space_id.clone(),
            components: prepared
                .into_iter()
                .map(|projection| projection.comparison)
                .collect(),
            uncovered: gaps.clone(),
        },
        card_bindings: bindings,
        capabilities,
    })?;
    let activation = store.create_activation(StateRootActivationBodyV2 {
        root_id: root.root_id.clone(),
        java_runtime_epoch: java_runtime_epoch(&live),
        java_frame: java_frame_blob,
        action_space_id: action_space_id.clone(),
        card_leases: leases,
    })?;
    Ok(ConstructedCombatRootReportV1 {
        schema: CONSTRUCTED_COMBAT_REPORT_SCHEMA_V1.to_owned(),
        status: "ready".to_owned(),
        case_id: plan.recipe.case_id.clone(),
        scenario_id: plan.recipe.scenario_id.clone(),
        recipe_program_id: plan.recipe.recipe_id.clone(),
        source_session,
        source_archive_node_id,
        scenario_plan_path: absolute_path_text(&request.scenario_plan)?,
        scenario_report_path: absolute_path_text(&request.scenario_report)?,
        java_frame_path: live.frame_path,
        components,
        gaps,
        source_head_guard: checked_source_head_guard(&request.session, source_head_before)?,
        root_id: Some(root.root_id),
        activation_id: Some(activation.activation_id),
        action_space_id: Some(action_space_id),
        java_runtime_bundle: Some(java_bundle),
        java_runtime_entrypoint: Some("scenario_recipe".to_owned()),
        java_cold_recovery: RestoreQualityV1::Unsupported,
    })
}

pub(crate) fn prepare_parentless_natural_neow_root_v1(
    request: &PrepareParentlessNaturalNeowRootRequestV1,
) -> Result<ParentlessNaturalNeowRootReportV1, String> {
    create_fresh_directory(&request.attempt_directory)?;
    if request.ascension != 20 {
        return Err("parentless natural Neow bootstrap is scoped to A20".to_owned());
    }
    if request.session.exists() {
        return Err(format!(
            "parentless natural Neow session must be create-new: {}",
            request.session.display()
        ));
    }
    if !is_sha256_hex(&request.native_autosave_sha256) {
        return Err("parentless Neow native autosave identity is not SHA-256".to_owned());
    }

    let store = StateRootStoreV2::new(&request.store_root).with_operation_verification_cache();
    store.initialize()?;
    let live = java_live_status(&request.java_live_output)?;
    validate_live_stable_decision_profile(&live, DecisionProfileV1::Neow)?;
    if java_neow_screen(&live)? != 3 {
        return Err("parentless natural root requires the Neow reward-choice screen".to_owned());
    }
    let java_seed = canonical_engine_seed_from_java_long(required_str(
        &live.frame["parity_internal"],
        "run_seed",
    )?)?;
    if java_seed != request.seed.to_string() {
        return Err(format!(
            "parentless Java seed {java_seed:?} does not match requested engine seed {}",
            request.seed
        ));
    }
    if live.frame["public_base"]["run"]["ascension"].as_u64() != Some(u64::from(request.ascension))
        || live.frame["public_base"]["run"]["character"].as_str() != Some("IRONCLAD")
        || live.frame["parity_internal"]["content_profile"].as_str() != Some("all_unlocked")
        || !live.frame["scenario"].is_null()
    {
        return Err(
            "parentless Java root is not a natural all-unlocked A20 Ironclad run".to_owned(),
        );
    }

    let neow_action_count = live
        .actions
        .iter()
        .filter(|action| {
            action.kind == "choose_event_option"
                && action.event_id.as_deref() == Some(NEOW_EVENT_ID)
                && action.neow_reward_type.is_some()
                && action.neow_drawback.is_some()
        })
        .count();
    let (neow_offer, neow_eligibility) = match neow_action_count {
        2 => ("mini", NeowEligibilityV1::Mini),
        4 => ("rich", NeowEligibilityV1::Rich),
        count => {
            return Err(format!(
                "parentless Java Neow root exposes {count} typed rewards instead of two or four"
            ))
        }
    };

    let first_frame_bytes = fs::read(&request.first_java_frame).map_err(|error| {
        format!(
            "failed to read first Java Neow frame {}: {error}",
            request.first_java_frame.display()
        )
    })?;
    let first_frame: Value = serde_json::from_slice(&first_frame_bytes)
        .map_err(|error| format!("failed to decode first Java Neow frame: {error}"))?;
    let first_talk_receipt_bytes = fs::read(&request.first_java_talk_receipt).map_err(|error| {
        format!(
            "failed to read first Java Neow Talk receipt {}: {error}",
            request.first_java_talk_receipt.display()
        )
    })?;
    let first_talk_receipt: ActionReceipt = serde_json::from_slice(&first_talk_receipt_bytes)
        .map_err(|error| format!("failed to decode first Java Neow Talk receipt: {error}"))?;
    let first_frame_id = first_frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "first Java Neow frame omitted its frame identity".to_owned())?;
    let receipt_successor_path = first_talk_receipt
        .successor_frame_path
        .as_deref()
        .map(Path::new)
        .ok_or_else(|| "first Java Neow Talk receipt omitted its successor path".to_owned())?;
    let receipt_successor_bytes = fs::read(receipt_successor_path).map_err(|error| {
        format!(
            "failed to read first Java Neow Talk successor {}: {error}",
            receipt_successor_path.display()
        )
    })?;
    let accepted_talk_action = matches!(
        first_talk_receipt.action_id.as_str(),
        "event_option:Neow:index:0:0" | "event_option:Neow:index:1:0"
    );
    if first_talk_receipt.status != "completed"
        || first_talk_receipt.code.is_some()
        || !accepted_talk_action
        || first_talk_receipt.parameters != json!({})
        || first_talk_receipt
            .producer_result
            .as_ref()
            .is_none_or(|result| {
                result["kind"].as_str() != Some("action_result")
                    || result["status"].as_str() != Some("accepted")
                    || result["action_id"].as_str() != Some(first_talk_receipt.action_id.as_str())
            })
        || first_talk_receipt.successor_frame_id != Some(first_frame_id)
        || first_frame["previous_frame_id"].as_u64() != Some(first_talk_receipt.based_on_frame_id)
        || sha256_bytes(&receipt_successor_bytes) != sha256_bytes(&first_frame_bytes)
    {
        return Err(
            "first Java Neow Talk receipt is not bound to its typed intro action and successor"
                .to_owned(),
        );
    }

    if first_frame != live.frame {
        return Err(
            "parentless Neow activation drifted after its accepted Talk successor".to_owned(),
        );
    }

    create_natural_session_v1(
        &RunPlayPathsV1::new(&request.session),
        sts_oracle_learning::eval::run_control::RunControlConfig {
            seed: request.seed,
            ascension_level: request.ascension,
            neow_eligibility,
            final_act: true,
            ..Default::default()
        },
        TaskBudgetV1 {
            max_world_transitions: 4096,
            max_reloads: 0,
        },
        CombatResearchPlanOptionsV1::default(),
    )?;
    let mut controller = RunPlayControllerV1::open(RunPlayPathsV1::new(&request.session))?;
    let checkpoint_rust = controller.current_external_exact_checkpoint_v1()?;
    let session = checkpoint_rust.clone().into_session()?;
    if rust_neow_room(&session)? != json!({"kind": "neow", "stage": "choose_reward"}) {
        return Err("fresh Rust session did not reach the Neow reward-choice root".to_owned());
    }
    let source_archive_node_id = controller.head().position.archive_node_id().to_owned();
    let source_head_before = source_head_inventory(&request.session)?;

    let java_frame_blob = put_json(&store, &live.frame)?;
    let java_master_deck_projection = java_master_deck_semantics(&live.frame)?;
    let java_master_deck_blob = put_json(&store, &java_master_deck_projection)?;
    let rust_identity_projection = rust_master_deck_identity_projection(&session);
    let rust_identity_blob = put_json(&store, &rust_identity_projection)?;
    let bindings =
        build_master_deck_bindings(&live, &session, &java_master_deck_blob, &rust_identity_blob)?;
    let leases = build_java_card_leases(&live, &session, &bindings)?;
    let projected = project_pair_for_profile(
        &store,
        &live,
        &session,
        &bindings,
        &leases,
        DecisionProfileV1::Neow,
        None,
    )?;
    require_exact_projection_set(
        &projected.prepared,
        "parentless natural Neow root",
        DecisionProfileV1::Neow,
    )?;
    let action_space_id = paired_action_space_id(&projected.java_actions)?;
    let components = projection_summaries(&projected.prepared);

    let proof = put_json(
        &store,
        &json!({
            "schema": "sts-state-root-capability-proof-v2",
            "accepted_talk_receipt_sha256": sha256_bytes(&first_talk_receipt_bytes),
            "components": projected.prepared.iter().map(|item| json!({
                "domain": item.comparison.domain,
                "java_sha256": item.comparison.java.sha256,
                "rust_sha256": item.comparison.rust.sha256,
            })).collect::<Vec<_>>(),
        }),
    )?;

    let java_runtime_directory = request.attempt_directory.join("java-runtime-reset");
    create_fresh_directory(&java_runtime_directory)?;
    let storage_destination = java_runtime_directory.join("storage");
    copy_parentless_recovery_storage(
        &request.java_recovery_storage,
        &storage_destination,
        &request.native_autosave_sha256,
    )?;
    write_new(
        &java_runtime_directory.join("first-root-frame.json"),
        &first_frame_bytes,
    )?;
    write_new(
        &java_runtime_directory.join("talk-receipt.json"),
        &first_talk_receipt_bytes,
    )?;
    write_new_json(&java_runtime_directory.join("root-frame.json"), &live.frame)?;
    write_new_json(
        &java_runtime_directory.join("same-runtime-reset.json"),
        &json!({
            "schema": "sts-state-root-same-runtime-reset-v2",
            "seed": request.seed.to_string(),
            "ascension": request.ascension,
            "neow_offer": neow_offer,
            "content_profile": "all_unlocked",
            "native_autosave_sha256": request.native_autosave_sha256,
            "entrypoint": "native_checkpoint_continue_then_neow_intro",
            "cross_jvm_recovery": "pending_certificate",
        }),
    )?;
    let java_bundle = store.put_directory(&java_runtime_directory)?;

    let rust_snapshot_directory = request.attempt_directory.join("rust-recovery");
    create_fresh_directory(&rust_snapshot_directory)?;
    write_new(
        &rust_snapshot_directory.join("checkpoint.msgpack"),
        &rmp_serde::to_vec_named(&checkpoint_rust)
            .map_err(|error| format!("failed to encode exact Rust checkpoint: {error}"))?,
    )?;
    write_new_json(
        &rust_snapshot_directory.join("source.json"),
        &json!({
            "schema": "sts-paired-parentless-rust-source-v1",
            "session_id": controller.head().session_id,
            "archive_node_id": source_archive_node_id,
            "seed": request.seed.to_string(),
            "objective": "heart_victory",
            "neow_offer": neow_offer,
        }),
    )?;
    let rust_bundle = store.put_directory(&rust_snapshot_directory)?;

    let mut versions = versions_for_root(controller.head(), &live)?;
    versions.sort_by(|left, right| {
        (&left.component, &left.identity).cmp(&(&right.component, &right.identity))
    });
    let capability_plan = capability_surface_plan_for_profile(DecisionProfileV1::Neow, &session)?;
    let (uncovered, capabilities) = capability_plan.materialize(&proof);
    let root = store.create_root(StateRootBodyV2 {
        provenance: RootProvenanceV1::Probe,
        world_provenance: WorldProvenanceV1::Natural,
        parent: None,
        decision_profile: DecisionProfileV1::Neow,
        versions,
        content_profile: "all_unlocked".to_owned(),
        rust_snapshot: RustStateSnapshotV2 {
            bundle: rust_bundle,
            entrypoint: "run_control_exact_checkpoint".to_owned(),
            source_identity: vec![
                IdentityClaimV1 {
                    kind: "archive_node_id".to_owned(),
                    value: source_archive_node_id.clone(),
                },
                IdentityClaimV1 {
                    kind: "run_play_session_id".to_owned(),
                    value: controller.head().session_id.clone(),
                },
            ],
        },
        preflight: RootPreflightV1 {
            action_space_id: action_space_id.clone(),
            components: projected
                .prepared
                .into_iter()
                .map(|projection| projection.comparison)
                .collect(),
            uncovered,
        },
        card_bindings: bindings,
        capabilities,
    })?;
    let activation = store.create_activation(StateRootActivationBodyV2 {
        root_id: root.root_id.clone(),
        java_runtime_epoch: java_runtime_epoch(&live),
        java_frame: java_frame_blob,
        action_space_id: action_space_id.clone(),
        card_leases: leases,
    })?;
    let source_head_guard = checked_source_head_guard(&request.session, source_head_before)?;
    Ok(ParentlessNaturalNeowRootReportV1 {
        schema: PARENTLESS_NEOW_REPORT_SCHEMA_V1.to_owned(),
        status: "ready".to_owned(),
        seed: request.seed.to_string(),
        ascension: request.ascension,
        neow_offer: neow_offer.to_owned(),
        source_session: controller.head().session_id.clone(),
        source_archive_node_id,
        java_frame_path: live.frame_path,
        components,
        source_head_guard,
        root_id: root.root_id,
        activation_id: activation.activation_id,
        action_space_id,
        java_runtime_bundle: java_bundle,
        java_runtime_entrypoint: "native_checkpoint_continue_then_neow_intro".to_owned(),
        recovery_certificate: "pending".to_owned(),
    })
}

fn copy_parentless_recovery_storage(
    source: &Path,
    destination: &Path,
    expected_autosave_sha256: &str,
) -> Result<(), String> {
    let source = fs::canonicalize(source).map_err(|error| {
        format!(
            "failed to resolve parentless recovery storage {}: {error}",
            source.display()
        )
    })?;
    if !source.is_dir() || destination.exists() {
        return Err("parentless recovery storage source/destination is invalid".to_owned());
    }
    fs::create_dir(destination)
        .map_err(|error| format!("failed to create recovery storage directory: {error}"))?;
    for directory in ["preferences", "betaPreferences", "saves"] {
        let from = source.join(directory);
        let to = destination.join(directory);
        if !from.is_dir() {
            return Err(format!(
                "parentless recovery storage omits required {directory} directory"
            ));
        }
        fs::create_dir(&to)
            .map_err(|error| format!("failed to create recovery {directory}: {error}"))?;
        copy_regular_directory_tree(&from, &to)?;
    }
    let autosaves = fs::read_dir(destination.join("saves"))
        .map_err(|error| format!("failed to inspect copied Neow autosave: {error}"))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with("IRONCLAD.autosave")
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    let [autosave] = autosaves.as_slice() else {
        return Err(format!(
            "parentless recovery storage contains {} Ironclad autosaves instead of one",
            autosaves.len()
        ));
    };
    let actual = sha256_bytes(
        &fs::read(autosave)
            .map_err(|error| format!("failed to read copied Neow autosave: {error}"))?,
    );
    if !actual.eq_ignore_ascii_case(expected_autosave_sha256) {
        return Err("parentless recovery autosave changed before root publication".to_owned());
    }
    Ok(())
}

fn copy_regular_directory_tree(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source)
        .map_err(|error| format!("failed to read recovery storage tree: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("failed to enumerate recovery storage: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect recovery storage entry: {error}"))?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir(&target)
                .map_err(|error| format!("failed to create recovery subdirectory: {error}"))?;
            copy_regular_directory_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("failed to copy recovery storage file: {error}"))?;
        } else {
            return Err(format!(
                "recovery storage contains unsupported link {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn preflight_paired_root_source_v1(
    source_session: &Path,
) -> Result<PairedRootSourcePreflightV1, String> {
    let head = RunPlayPathsV1::new(source_session).load_head()?;
    let identity = require_current_source_semantics_v1(&head)?;
    Ok(PairedRootSourcePreflightV1 {
        schema: "sts-paired-root-source-preflight-v1".to_owned(),
        status: "ready".to_owned(),
        source_session: head.session_id.clone(),
        source_head_revision: head.head_revision,
        rust_rng_semantics: identity.rng_semantics_id.clone(),
        rust_simulator_semantics: identity.simulator_semantics_id.clone(),
    })
}

fn require_current_source_semantics_v1(
    head: &RunPlayHeadV1,
) -> Result<&RunPlayEvidenceIdentityV1, String> {
    require_current_source_semantics_identity_v1(head.evidence_identity.as_ref())
}

fn require_current_source_semantics_identity_v1(
    identity: Option<&RunPlayEvidenceIdentityV1>,
) -> Result<&RunPlayEvidenceIdentityV1, String> {
    let stored = identity.ok_or_else(|| {
        "paired-root source has legacy/unscoped Rust semantics; start a fresh current-generation natural run because arbitrary historical replay and relabeling are unsupported".to_owned()
    })?;
    let current = RunPlayEvidenceIdentityV1::current_v1();
    for (component, stored_identity, active_identity) in [
        (
            "rust_rng_semantics",
            stored.rng_semantics_id.as_str(),
            current.rng_semantics_id.as_str(),
        ),
        (
            "rust_simulator_semantics",
            stored.simulator_semantics_id.as_str(),
            current.simulator_semantics_id.as_str(),
        ),
    ] {
        if stored_identity != active_identity {
            return Err(format!(
                "paired-root source semantics drift for {component}: stored={stored_identity:?} active={active_identity:?}; start a fresh current-generation natural run because arbitrary historical replay and relabeling are unsupported"
            ));
        }
    }
    Ok(stored)
}

pub(crate) fn write_preparation_report_new_v1(
    path: &Path,
    report: &PairedRootPreparationReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root prepare report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn write_constructed_combat_root_report_new_v1(
    path: &Path,
    report: &ConstructedCombatRootReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode constructed combat root report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn write_parentless_natural_neow_report_new_v1(
    path: &Path,
    report: &ParentlessNaturalNeowRootReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode parentless Neow report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn write_preview_report_new_v1(
    path: &Path,
    report: &PairedRootPreviewReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root preview report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn write_witness_transaction_report_new_v1(
    path: &Path,
    report: &PairedRootWitnessTransactionReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root witness report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn write_recovery_report_new_v1(
    path: &Path,
    report: &PairedRootRecoveryReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root recovery report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn observe_paired_root_actions_v1(
    request: &ObservePairedRootActionsRequestV1,
) -> Result<PairedRootActionObservationReportV6, String> {
    let store = StateRootStoreV2::new(&request.store_root).with_operation_verification_cache();
    store.initialize()?;
    let root = store.load_root(&request.root_id)?;
    let decision_context = paired_root_decision_context(&store, &root)?;
    let observed_actions = stable_action_observations(&store, &root)?;
    let authorizations = observed_actions
        .iter()
        .map(|observation| {
            let capability =
                require_declared_action_capability(&root, &observation.handle.action)?.clone();
            Ok(PairedRootActionAuthorizationV1 {
                handle_id: observation.handle.handle_id.clone(),
                capability,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PairedRootActionObservationReportV6 {
        schema: ACTION_OBSERVATION_REPORT_SCHEMA_V6.to_owned(),
        root_id: root.root_id,
        action_space_id: root.root.preflight.action_space_id,
        decision_context,
        observed_actions,
        authorizations,
    })
}

fn paired_root_decision_context(
    store: &StateRootStoreV2,
    root: &StateRootArtifactV2,
) -> Result<PairedRootDecisionContextV1, String> {
    Ok(PairedRootDecisionContextV1 {
        profile: root_decision_profile(root),
        run: exact_root_projection(store, root, ProjectionDomainV1::RunScalars)?,
        master_deck: exact_root_projection(store, root, ProjectionDomainV1::MasterDeck)?,
        relics: exact_root_projection(store, root, ProjectionDomainV1::Relics)?,
        potions: exact_root_projection(store, root, ProjectionDomainV1::Potions)?,
        map: root
            .root
            .preflight
            .components
            .iter()
            .any(|component| component.domain == ProjectionDomainV1::Map)
            .then(|| exact_root_projection(store, root, ProjectionDomainV1::Map))
            .transpose()?,
        room: root
            .root
            .preflight
            .components
            .iter()
            .any(|component| component.domain == ProjectionDomainV1::Room)
            .then(|| exact_root_projection(store, root, ProjectionDomainV1::Room))
            .transpose()?,
    })
}

fn exact_root_projection(
    store: &StateRootStoreV2,
    root: &StateRootArtifactV2,
    domain: ProjectionDomainV1,
) -> Result<Value, String> {
    store.load_exact_projection(&root.root_id, domain)
}

pub(crate) fn write_action_observation_report_new_v1(
    path: &Path,
    report: &PairedRootActionObservationReportV6,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root action observation: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn search_paired_root_v1(
    request: &SearchPairedRootRequestV1,
) -> Result<PairedRootSearchReportV1, String> {
    let store = StateRootStoreV2::new(&request.store_root).with_operation_verification_cache();
    store.initialize()?;
    let root = store.load_root(&request.root_id)?;
    require_current_rust_semantics(&root)?;
    if root_decision_profile(&root) != DecisionProfileV1::CombatPlayerDecision {
        return Err("paired-root combat search does not accept a reward decision root".to_owned());
    }
    if root.root.rust_snapshot.entrypoint != "run_control_exact_checkpoint" {
        return Err("paired-root search requires an exact native Rust checkpoint".to_owned());
    }
    let manifest = store.verify_bundle(&root.root.rust_snapshot.bundle)?;
    let checkpoint_file = manifest
        .files
        .iter()
        .find(|file| file.relative_path == "checkpoint.msgpack")
        .ok_or_else(|| "paired-root Rust recovery bundle omits checkpoint.msgpack".to_owned())?;
    let checkpoint_bytes =
        store.read_bundle_file(&root.root.rust_snapshot.bundle, "checkpoint.msgpack")?;
    let checkpoint: RunControlSessionCheckpointV1 = rmp_serde::from_slice(&checkpoint_bytes)
        .map_err(|error| format!("failed to decode paired-root search checkpoint: {error}"))?;
    let session = checkpoint.into_session()?;

    let binding_by_rust = root
        .root
        .card_bindings
        .iter()
        .map(|binding| {
            (
                rust_lineage_key(&binding.rust_lineage),
                binding.binding_id.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let combat_version = combat_projection_version(&root)?;
    let stored_combat = stored_exact_projection(
        &store,
        &root,
        ProjectionDomainV1::Combat,
        combat_projection_schema(combat_version),
    )?;
    if rust_combat(&session, &binding_by_rust, combat_version)? != stored_combat {
        return Err(
            "paired-root search checkpoint no longer matches its combat projection".to_owned(),
        );
    }
    let action_space_version = action_space_projection_version(&root)?;
    let stored_actions = stored_exact_projection(
        &store,
        &root,
        ProjectionDomainV1::ActionSpace,
        action_space_projection_schema(action_space_version),
    )?;
    let active_actions = rust_action_space(&session, &binding_by_rust, action_space_version)?;
    if active_actions != stored_actions
        || paired_action_space_id(&active_actions)? != root.root.preflight.action_space_id
    {
        return Err("paired-root search checkpoint no longer matches its action space".to_owned());
    }

    let options = CombatResearchPlanOptionsV1::default();
    let portfolio = run_control_session_combat_research_portfolio_with_options_v1(
        &session,
        Some(options.clone()),
    )?;
    if portfolio.attempt.candidates.len() != portfolio.witness_input_lines.len() {
        return Err("paired-root search candidate/input materializations disagree".to_owned());
    }
    let position = session.current_active_combat_position()?;
    let combat_sequence = session.combat_sequence();
    let constructed_scenario_instance_id = root_constructed_scenario_instance_id(&root)?;
    let mut witnesses = Vec::with_capacity(portfolio.attempt.candidates.len());
    for (candidate, inputs) in portfolio
        .attempt
        .candidates
        .iter()
        .zip(&portfolio.witness_input_lines)
    {
        if candidate.plan.steps.len() != inputs.len() {
            return Err(
                "paired-root search public plan and exact materialization disagree".to_owned(),
            );
        }
        let translation = translate_search_input_line_v1(
            &position,
            combat_sequence,
            constructed_scenario_instance_id.as_deref(),
            inputs,
            options.max_engine_steps_per_transition,
        )?;
        let witness_id = paired_search_content_id(
            "paired-search-witness-v1",
            &json!({
                "root_id": root.root_id,
                "rust_checkpoint_sha256": checkpoint_file.blob.sha256,
                "translation": translation,
                "outcome": candidate.outcome,
            }),
        )?;
        let next_action_handle_id = first_search_action_handle_v1(&root, &translation)?;
        witnesses.push(PairedRootSearchWitnessV1 {
            witness_id,
            plan_step_count: inputs.len(),
            translation,
            next_action_handle_id,
            outcome: candidate.outcome.clone(),
            satisfies_requested_quality: candidate.satisfies_requested_quality,
            supporting_provenances: candidate.supporting_provenances.clone(),
        });
    }
    let mut bounded_frontier = Vec::with_capacity(portfolio.bounded_progress.len());
    for progress in &portfolio.bounded_progress {
        let mut paths = Vec::with_capacity(progress.paths.len());
        for path in &progress.paths {
            if path.candidate.plan.steps.len() != path.exact_inputs.len() {
                return Err(
                    "paired-root bounded frontier plan and exact materialization disagree"
                        .to_owned(),
                );
            }
            paths.push(PairedRootSearchBoundedPathV1 {
                roles: path.candidate.roles.clone(),
                endpoint: path.candidate.endpoint,
                translation: translate_search_input_line_v1(
                    &position,
                    combat_sequence,
                    constructed_scenario_instance_id.as_deref(),
                    &path.exact_inputs,
                    options.max_engine_steps_per_transition,
                )?,
            });
        }
        bounded_frontier.push(PairedRootSearchBoundedFrontierV1 {
            provenance: progress.provenance.clone(),
            max_player_turn: progress.max_player_turn,
            recent_turn_survival_envelope: progress.recent_turn_survival_envelope.clone(),
            paths,
        });
    }
    let policy_owner = "RunControlSession::current_combat_research_portfolio_with_options_v1";
    let policy_id = paired_search_content_id(
        "paired-search-policy-v1",
        &json!({"owner": policy_owner, "options": options}),
    )?;
    let summary_exit = portfolio.attempt.summary_exit();
    Ok(PairedRootSearchReportV1 {
        schema: SEARCH_REPORT_SCHEMA_V1.to_owned(),
        status: if witnesses.is_empty() {
            "budget_unknown".to_owned()
        } else {
            "witness".to_owned()
        },
        root_id: root.root_id,
        action_space_id: root.root.preflight.action_space_id,
        rust_checkpoint: checkpoint_file.blob.clone(),
        policy: PairedRootSearchPolicyV1 {
            policy_id,
            owner: policy_owner.to_owned(),
            options,
        },
        summary_exit,
        lanes: portfolio.attempt.lanes.clone(),
        witnesses,
        bounded_frontier,
    })
}

pub(crate) fn write_search_report_new_v1(
    path: &Path,
    report: &PairedRootSearchReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root search report: {error}"))?;
    write_new(path, &bytes)
}

fn resolve_paired_root_search_intent_at_root_v1(
    store_root: &Path,
    root_id: &str,
    witness_id: &str,
    step_ordinal: usize,
    intent: PairedRootSearchActionIntentV1,
) -> Result<PairedRootSearchIntentResolutionV1, String> {
    let store = StateRootStoreV2::new(store_root).with_operation_verification_cache();
    store.initialize()?;
    let root = store.load_root(root_id)?;
    require_current_rust_semantics(&root)?;
    let observations = stable_action_observations(&store, &root)?;
    let (action_handle_id, reason) =
        resolve_search_intent_observation_v1(&root, &observations, &intent)?;
    Ok(PairedRootSearchIntentResolutionV1 {
        schema: SEARCH_INTENT_RESOLUTION_SCHEMA_V1.to_owned(),
        status: if action_handle_id.is_some() {
            "ready".to_owned()
        } else {
            "unavailable".to_owned()
        },
        root_id: root.root_id,
        action_space_id: root.root.preflight.action_space_id,
        witness_id: witness_id.to_owned(),
        step_ordinal,
        intent,
        action_handle_id,
        reason,
    })
}

pub(crate) fn materialize_paired_root_native_v1(
    request: &MaterializePairedRootNativeRequestV1,
) -> Result<PairedRootNativeMaterializationReportV1, String> {
    let store = StateRootStoreV2::new(&request.store_root).with_operation_verification_cache();
    store.initialize()?;
    let plan = store.recovery_plan(&request.target_root_id)?;
    let native = store.load_root(&plan.native_root_id)?;
    require_current_rust_semantics(&native)?;
    let recovery_certificate =
        store.load_recovery_certificate(&native.root_id, &plan.recovery_certificate_id)?;
    let java_recovery = &recovery_certificate.certificate.java_recovery;
    if java_recovery.runtime != RuntimeKindV1::JavaOriginalGame {
        return Err("target root has no Java original-game recovery payload".to_owned());
    }
    let java_entrypoint = java_recovery.entrypoint.clone();
    match (java_recovery.restore_quality, java_entrypoint.as_str()) {
        (RestoreQualityV1::NativeRoundTripVerified, "native_checkpoint_continue")
        | (
            RestoreQualityV1::NativeRoundTripVerified,
            sts_paired_root::NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1,
        )
        | (
            RestoreQualityV1::TypedReconstructedVerified,
            "native_checkpoint_continue_then_neow_intro",
        ) => {}
        _ => return Err("target root has no supported native Java launch payload".to_owned()),
    }
    let manifest = store.verify_bundle(&java_recovery.bundle)?;
    let required_files: &[&str] = match java_entrypoint.as_str() {
        "native_checkpoint_continue" => &["capture.json", "checkpoint.json", "root-frame.json"],
        sts_paired_root::NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1 => &[],
        "native_checkpoint_continue_then_neow_intro" => &[
            "first-root-frame.json",
            "reconstruction.json",
            "root-frame.json",
            "talk-receipt.json",
        ],
        _ => unreachable!("entrypoint checked above"),
    };
    for required in required_files {
        if !manifest
            .files
            .iter()
            .any(|file| file.relative_path == *required)
        {
            return Err(format!("native Java recovery bundle omits {required}"));
        }
    }
    if java_entrypoint != sts_paired_root::NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1 {
        // Historical launch bundles wrap original-game storage in a `storage`
        // directory and carry adapter metadata beside it.
        for required_prefix in ["storage/preferences/", "storage/saves/"] {
            if !manifest
                .files
                .iter()
                .any(|file| file.relative_path.starts_with(required_prefix))
            {
                return Err(format!(
                    "native Java recovery bundle omits {required_prefix} files"
                ));
            }
        }
    }
    if java_entrypoint == sts_paired_root::NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1 {
        let content_profile = identity_claim(&java_recovery.source_identity, "content_profile")?;
        let rebuilt =
            store.native_storage_recovery_payload(&java_recovery.bundle, content_profile)?;
        if rebuilt != *java_recovery {
            return Err(
                "native storage recovery payload differs from its validated bundle".to_owned(),
            );
        }
    } else {
        let autosaves = manifest
            .files
            .iter()
            .filter(|file| {
                let Some(name) = file.relative_path.strip_prefix("storage/saves/") else {
                    return false;
                };
                !name.contains('/')
                    && name
                        .strip_suffix("IRONCLAD.autosave")
                        .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with('_'))
            })
            .count();
        if autosaves != 1 {
            return Err(format!(
                "native Java recovery bundle contains {autosaves} Ironclad autosaves instead of one"
            ));
        }
    }
    if java_entrypoint == "native_checkpoint_continue" {
        let checkpoint = manifest
            .files
            .iter()
            .find(|file| file.relative_path == "checkpoint.json")
            .expect("checkpoint presence checked above");
        let checkpoint_claim =
            identity_claim(&java_recovery.source_identity, "checkpoint_manifest_sha256")?;
        if !checkpoint_claim.eq_ignore_ascii_case(&checkpoint.blob.sha256) {
            return Err(
                "native Java recovery bundle checkpoint disagrees with the root identity claim"
                    .to_owned(),
            );
        }
        let autosave_claim =
            identity_claim(&java_recovery.source_identity, "native_autosave_sha256")?;
        let autosave = manifest
            .files
            .iter()
            .find(|file| {
                file.relative_path.starts_with("storage/saves/")
                    && file.relative_path.ends_with("IRONCLAD.autosave")
            })
            .expect("single autosave presence checked above");
        if !autosave_claim.eq_ignore_ascii_case(&autosave.blob.sha256) {
            return Err(
                "native recovery certificate autosave disagrees with its identity claim".to_owned(),
            );
        }
    } else if java_entrypoint == "native_checkpoint_continue_then_neow_intro" {
        let autosave_claim =
            identity_claim(&java_recovery.source_identity, "native_autosave_sha256")?;
        let autosave = manifest
            .files
            .iter()
            .find(|file| {
                file.relative_path.starts_with("storage/saves/")
                    && file.relative_path.ends_with("IRONCLAD.autosave")
            })
            .expect("single autosave presence checked above");
        if !autosave_claim.eq_ignore_ascii_case(&autosave.blob.sha256) {
            return Err(
                "parentless Neow autosave disagrees with the root identity claim".to_owned(),
            );
        }
    }
    let content_profile =
        identity_claim(&java_recovery.source_identity, "content_profile")?.to_owned();

    store.materialize_bundle_new(&java_recovery.bundle, &request.output_directory)?;
    let materialized = fs::canonicalize(&request.output_directory).map_err(|error| {
        format!(
            "failed to resolve native Java materialization {}: {error}",
            request.output_directory.display()
        )
    })?;
    let storage = if java_entrypoint == sts_paired_root::NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1 {
        for relative in sts_paired_root::NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1 {
            fs::create_dir_all(materialized.join(relative)).map_err(|error| {
                format!("failed to complete native storage directory {relative:?}: {error}")
            })?;
        }
        materialized.clone()
    } else {
        materialized.join("storage")
    };
    if !storage.is_dir() {
        return Err("native Java materialization omitted its storage directory".to_owned());
    }
    Ok(PairedRootNativeMaterializationReportV1 {
        schema: NATIVE_MATERIALIZATION_REPORT_SCHEMA_V1.to_owned(),
        requested_target_root_id: request.target_root_id.clone(),
        native_root_id: native.root_id.clone(),
        recovery_certificate_id: plan.recovery_certificate_id,
        java_bundle_id: java_recovery.bundle.bundle_id.clone(),
        content_profile,
        java_entrypoint,
        materialized_directory: materialized.to_string_lossy().into_owned(),
        storage_directory: storage.to_string_lossy().into_owned(),
        file_count: manifest.files.len(),
        recovery_steps: plan.steps.len(),
    })
}

pub(crate) fn write_native_materialization_report_new_v1(
    path: &Path,
    report: &PairedRootNativeMaterializationReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report).map_err(|error| {
        format!("failed to encode paired-root native materialization report: {error}")
    })?;
    write_new(path, &bytes)
}

pub(crate) fn activate_paired_root_native_v1(
    request: &ActivatePairedRootNativeRequestV1,
) -> Result<PairedRootNativeActivationReportV1, String> {
    let store = StateRootStoreV2::new(&request.store_root).with_operation_verification_cache();
    store.initialize()?;
    let plan = store.recovery_plan(&request.target_root_id)?;
    let native = store.load_root(&plan.native_root_id)?;
    let source_session = resolve_source_session(&store, &request.run_root, &native)?;
    let source_head_before = source_head_inventory(&source_session)?;

    let live = java_live_status(&request.java_live_output)?;
    validate_live_stable_decision_profile(&live, root_decision_profile(&native))?;
    let version_compatibility = validate_native_activation_versions(&native, &live)?;
    let checkpoint_bytes =
        store.read_bundle_file(&native.root.rust_snapshot.bundle, "checkpoint.msgpack")?;
    let checkpoint: RunControlSessionCheckpointV1 = rmp_serde::from_slice(&checkpoint_bytes)
        .map_err(|error| format!("failed to decode native Rust checkpoint: {error}"))?;
    let session = checkpoint.into_session()?;
    let leases = build_java_card_leases(&live, &session, &native.root.card_bindings)?;
    let projected = project_pair_for_root(
        &store,
        &live,
        &session,
        &native.root.card_bindings,
        &leases,
        &native,
    )?;
    require_exact_projection_set(
        &projected.prepared,
        "materialized native root",
        root_decision_profile(&native),
    )?;
    if paired_action_space_id(&projected.java_actions)? != native.root.preflight.action_space_id {
        return Err("materialized native root action-space identity changed".to_owned());
    }
    let comparisons = projected
        .prepared
        .iter()
        .map(|projection| projection.comparison.clone())
        .collect::<Vec<_>>();
    if comparisons != native.root.preflight.components {
        return Err("materialized native projections no longer match the stored root".to_owned());
    }
    let activation = store.create_activation(StateRootActivationBodyV2 {
        root_id: native.root_id.clone(),
        java_runtime_epoch: java_runtime_epoch(&live),
        java_frame: put_json(&store, &live.frame)?,
        action_space_id: native.root.preflight.action_space_id.clone(),
        card_leases: leases,
    })?;
    let mut report = PairedRootNativeActivationReportV1 {
        schema: NATIVE_ACTIVATION_REPORT_SCHEMA_V1.to_owned(),
        requested_target_root_id: request.target_root_id.clone(),
        native_root_id: native.root_id,
        native_activation_id: activation.activation_id,
        source_session: source_session.to_string_lossy().into_owned(),
        version_compatibility,
        components: projection_summaries(&projected.prepared),
        source_head_guard: pending_source_head_guard(),
    };
    report.source_head_guard = checked_source_head_guard(&source_session, source_head_before)?;
    Ok(report)
}

pub(crate) fn write_native_activation_report_new_v1(
    path: &Path,
    report: &PairedRootNativeActivationReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report).map_err(|error| {
        format!("failed to encode paired-root native activation report: {error}")
    })?;
    write_new(path, &bytes)
}

pub(crate) fn verify_paired_root_receipt_chain_cas_v1(
    store_root: &Path,
    head_name: &str,
    source_root_id: &str,
    receipt_ids: &[String],
) -> Result<PairedRootCasProofReportV1, String> {
    if receipt_ids.is_empty() {
        return Err("CAS proof requires at least one receipt".to_owned());
    }
    let store = StateRootStoreV2::new(store_root).with_operation_verification_cache();
    store.initialize()?;
    let mut current = store.create_head(head_name, source_root_id)?;
    let mut steps = Vec::with_capacity(receipt_ids.len());
    for receipt_id in receipt_ids {
        let receipt = store.load_receipt(receipt_id)?;
        if receipt.receipt.source_root_id != current.root_id {
            return Err(format!(
                "CAS proof receipt {receipt_id} does not continue head revision {}",
                current.revision
            ));
        }
        let committed = store.commit_receipt(head_name, current.revision, receipt_id)?;
        let stale_retry_error = store
            .commit_receipt(head_name, current.revision, receipt_id)
            .expect_err("stale paired-head CAS unexpectedly succeeded");
        if !stale_retry_error.contains("advanced concurrently") {
            return Err(format!(
                "stale paired-head CAS failed for the wrong reason: {stale_retry_error}"
            ));
        }
        if store.load_head(head_name)? != committed {
            return Err("paired head changed after the rejected stale retry".to_owned());
        }
        steps.push(PairedRootCasProofStepV1 {
            receipt_id: receipt_id.clone(),
            before: current,
            after: committed.clone(),
            stale_retry_rejected: true,
            stale_retry_error,
        });
        current = committed;
    }
    Ok(PairedRootCasProofReportV1 {
        schema: "sts-paired-root-cas-proof-v1".to_owned(),
        head_name: head_name.to_owned(),
        source_root_id: source_root_id.to_owned(),
        steps,
        final_head: current,
        execution_scope: "artifact_pointer_only_no_runtime_execution".to_owned(),
    })
}

pub(crate) fn write_cas_proof_report_new_v1(
    path: &Path,
    report: &PairedRootCasProofReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root CAS proof report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn initialize_paired_root_head_v1(
    store_root: &Path,
    head_name: &str,
    root_id: &str,
) -> Result<PairedRootHeadInitReportV1, String> {
    let store = StateRootStoreV2::new(store_root).with_operation_verification_cache();
    store.initialize()?;
    let head = store.create_head(head_name, root_id)?;
    Ok(PairedRootHeadInitReportV1 {
        schema: "sts-paired-root-head-init-report-v1".to_owned(),
        status: "created".to_owned(),
        head,
        execution_scope: "artifact_pointer_only_no_runtime_execution".to_owned(),
    })
}

pub(crate) fn commit_paired_root_head_receipt_v1(
    store_root: &Path,
    head_name: &str,
    receipt_id: &str,
) -> Result<PairedRootHeadCommitReportV1, String> {
    let store = StateRootStoreV2::new(store_root).with_operation_verification_cache();
    store.initialize()?;
    let outcome = store.commit_receipt_idempotent(head_name, receipt_id)?;
    Ok(PairedRootHeadCommitReportV1 {
        schema: "sts-paired-root-head-commit-report-v1".to_owned(),
        status: outcome.status,
        receipt_id: outcome.receipt_id,
        committed_head: outcome.committed_head,
        current_head: outcome.current_head,
        execution_scope: "artifact_pointer_only_no_runtime_execution".to_owned(),
    })
}

pub(crate) fn write_head_init_report_new_v1(
    path: &Path,
    report: &PairedRootHeadInitReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root head init report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn write_head_commit_report_new_v1(
    path: &Path,
    report: &PairedRootHeadCommitReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root head commit report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn select_paired_root_head_v1(
    store_root: &Path,
    selector_name: &str,
    expected_revision: Option<u64>,
    head_name: &str,
) -> Result<PairedRootHeadSelectorReportV1, String> {
    let store = StateRootStoreV2::new(store_root).with_operation_verification_cache();
    store.initialize()?;
    let outcome = store.select_head(selector_name, expected_revision, head_name)?;
    Ok(PairedRootHeadSelectorReportV1 {
        schema: "sts-paired-root-head-selector-report-v1".to_owned(),
        status: outcome.status,
        selector: outcome.selector,
        execution_scope: "artifact_pointer_selection_only_no_runtime_execution".to_owned(),
    })
}

pub(crate) fn resolve_paired_root_head_v1(
    store_root: &Path,
    selector_name: &str,
) -> Result<PairedRootHeadResolutionReportV1, String> {
    let store = StateRootStoreV2::new(store_root).with_operation_verification_cache();
    store.initialize()?;
    let resolution = store.resolve_head_selector(selector_name)?;
    Ok(PairedRootHeadResolutionReportV1 {
        schema: "sts-paired-root-head-resolution-report-v1".to_owned(),
        status: "resolved".to_owned(),
        resolution,
        execution_scope: "artifact_pointer_observation_only_no_runtime_execution".to_owned(),
    })
}

pub(crate) fn commit_selected_paired_root_head_receipt_v1(
    store_root: &Path,
    selector_name: &str,
    receipt_id: &str,
) -> Result<PairedRootSelectedHeadCommitReportV1, String> {
    let store = StateRootStoreV2::new(store_root).with_operation_verification_cache();
    store.initialize()?;
    let outcome = store.commit_selected_receipt_idempotent(selector_name, receipt_id)?;
    Ok(PairedRootSelectedHeadCommitReportV1 {
        schema: "sts-paired-root-selected-head-commit-report-v1".to_owned(),
        selector: outcome.selector,
        status: outcome.commit.status,
        receipt_id: outcome.commit.receipt_id,
        committed_head: outcome.commit.committed_head,
        current_head: outcome.commit.current_head,
        execution_scope: "artifact_pointer_only_no_runtime_execution".to_owned(),
    })
}

pub(crate) fn write_head_selector_report_new_v1(
    path: &Path,
    report: &PairedRootHeadSelectorReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root head selector report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn write_head_resolution_report_new_v1(
    path: &Path,
    report: &PairedRootHeadResolutionReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("failed to encode paired-root head resolution report: {error}"))?;
    write_new(path, &bytes)
}

pub(crate) fn write_selected_head_commit_report_new_v1(
    path: &Path,
    report: &PairedRootSelectedHeadCommitReportV1,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(report).map_err(|error| {
        format!("failed to encode paired-root selected-head commit report: {error}")
    })?;
    write_new(path, &bytes)
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RustActionReceiptV1 {
    schema: String,
    source_root_id: String,
    action: PairedActionHandleV1,
    progress_message: String,
    successor_checkpoint_sha256: String,
}

struct ProjectedPairV1 {
    prepared: Vec<PreparedProjectionV1>,
    java_actions: Value,
}

pub(crate) fn preview_paired_root_action_v1(
    request: &PreviewPairedRootActionRequestV1,
) -> Result<PairedRootPreviewReportV1, String> {
    create_fresh_directory(&request.attempt_directory)?;
    let source_head_before = source_head_inventory(&request.source_session)?;
    let store = StateRootStoreV2::new(&request.store_root).with_operation_verification_cache();
    store.initialize()?;
    let source = store.load_root(&request.source_root_id)?;
    let source_activation = store.load_activation(&request.source_activation_id)?;

    let live = java_live_status(&request.java_live_output)?;
    validate_live_stable_decision_profile(&live, root_decision_profile(&source))?;
    require_live_activation(&store, &source, &source_activation, &live)?;
    let checkpoint_bytes =
        store.read_bundle_file(&source.root.rust_snapshot.bundle, "checkpoint.msgpack")?;
    let checkpoint: RunControlSessionCheckpointV1 = rmp_serde::from_slice(&checkpoint_bytes)
        .map_err(|error| format!("failed to decode paired Rust checkpoint: {error}"))?;
    let mut session = checkpoint.into_session()?;

    let root_projection = project_pair_for_root(
        &store,
        &live,
        &session,
        &source.root.card_bindings,
        &source_activation.activation.card_leases,
        &source,
    )?;
    require_exact_projection_set(
        &root_projection.prepared,
        "restored source root",
        root_decision_profile(&source),
    )?;
    let restored_action_space_id = paired_action_space_id(&root_projection.java_actions)?;
    if restored_action_space_id != source.root.preflight.action_space_id {
        return Err("restored source root action-space identity changed".to_owned());
    }

    let stored_observations = stable_action_observations(&store, &source)?;
    let stored_handles = stored_observations
        .iter()
        .map(|observation| observation.handle.clone())
        .collect::<Vec<_>>();
    let live_handles = paired_action_handles(&source, &source_activation, &live)?;
    if live_handles != stored_handles {
        return Err(
            "live paired action handles differ from the stored root observation".to_owned(),
        );
    }
    let action = select_action_handle(&stored_handles, &request.action_handle_id)?;
    require_verified_action_capability(&source, &action.action)?;
    let rust_action = rust_decision_action_for_action(&source, &session, &action)?;
    let combat_sequence = session.combat_sequence();
    let rust_card_births = rust_action
        .executable_input()
        .as_ref()
        .map(|input| observe_rust_card_births_for_input_v1(&session, input))
        .transpose()?
        .flatten();
    let java_action_id = java_action_id_for_handle(&source, &source_activation, &live, &action)?;

    let rust_started = Instant::now();
    let rust_outcome = session.apply_decision_action(rust_action)?;
    let rust_execution_ms = rust_started.elapsed().as_millis();
    let successor_checkpoint = RunControlSessionCheckpointV1::from_session(&session);
    let successor_checkpoint_bytes = rmp_serde::to_vec_named(&successor_checkpoint)
        .map_err(|error| format!("failed to encode successor Rust checkpoint: {error}"))?;
    let successor_checkpoint_sha256 = sha256_bytes(&successor_checkpoint_bytes);
    let rust_successor_profile = rust_decision_profile(&session);
    let successor_capability_plan = rust_successor_profile
        .map(|profile| capability_surface_plan_for_profile(profile, &session))
        .transpose()?;

    let java_started = Instant::now();
    let java_receipt = java_live_act(
        &request.java_live_output,
        &java_action_id,
        Duration::from_secs(30),
    )?;
    let java_execution_ms = java_started.elapsed().as_millis();
    if java_receipt.status != "completed"
        || java_receipt.code.is_some()
        || java_receipt.successor_frame_id.is_none()
        || java_receipt.action_id != java_action_id
    {
        return Err(
            "Java action preview did not produce one completed stable successor".to_owned(),
        );
    }
    let successor_live = java_live_status(&request.java_live_output)?;
    let rust_transition_receipt = RustActionReceiptV1 {
        schema: "sts-paired-rust-action-receipt-v1".to_owned(),
        source_root_id: source.root_id.clone(),
        action: action.clone(),
        progress_message: rust_outcome.message,
        successor_checkpoint_sha256,
    };
    let java_receipt_blob = put_json(&store, &java_receipt)?;
    let rust_receipt_blob = put_json(&store, &rust_transition_receipt)?;
    let successor_boundary = live_successor_boundary_v1(&successor_live)?;
    let successor_profile = live_decision_profile(&successor_live)?;
    if successor_profile.is_none() {
        if !successor_boundary.stable {
            return Err("Java action reached a non-stable successor boundary".to_owned());
        }
        let report = PairedRootPreviewReportV1 {
            schema: PREVIEW_REPORT_SCHEMA_V1.to_owned(),
            status: "successor_boundary_unsupported".to_owned(),
            source_root_id: source.root_id,
            source_activation_id: source_activation.activation_id,
            action,
            components: Vec::new(),
            source_head_guard: pending_source_head_guard(),
            successor_root_id: None,
            successor_activation_id: None,
            receipt_id: None,
            divergence_id: None,
            successor_boundary: Some(successor_boundary),
            java_evidence: Some(java_receipt_blob),
            rust_evidence: Some(rust_receipt_blob),
            timings: TransitionTimingV1 {
                rust_execution_ms,
                java_execution_ms,
                comparison_ms: 0,
            },
        };
        return finalize_preview_source_head_guard(request, source_head_before, report);
    }
    let successor_profile = successor_profile.expect("checked above");
    if rust_successor_profile != Some(successor_profile) {
        return Err(format!(
            "Java/Rust successor decision profiles differ: java={successor_profile:?} rust={rust_successor_profile:?}"
        ));
    }
    let successor_capability_plan =
        successor_capability_plan.expect("supported Rust successor profile has a capability plan");
    validate_live_stable_decision_profile(&successor_live, successor_profile)?;
    let (successor_bindings, successor_leases) = match successor_profile {
        DecisionProfileV1::CombatPlayerDecision => build_preview_successor_card_identity_v1(
            &store,
            &source,
            &source_activation,
            &live,
            &successor_live,
            &java_receipt,
            rust_card_births.as_ref(),
            combat_sequence,
        )?,
        DecisionProfileV1::CombatReward
        | DecisionProfileV1::CardReward
        | DecisionProfileV1::Event
        | DecisionProfileV1::MapNavigation
        | DecisionProfileV1::Neow => {
            match build_persistent_master_deck_successor_identity_v1(
                &store,
                &source,
                &successor_live,
                &session,
            ) {
                Ok(identity) => identity,
                Err(failure) => {
                    if let Some(path) = request.adapter_failure_report.as_deref() {
                        write_failure_report_if_absent(path, &failure).map_err(|report_error| {
                            format!("{}; {report_error}", failure.detail)
                        })?;
                    }
                    return Err(failure.detail);
                }
            }
        }
    };

    let comparison_started = Instant::now();
    let successor_projection = project_pair_for_profile(
        &store,
        &successor_live,
        &session,
        &successor_bindings,
        &successor_leases,
        successor_profile,
        (successor_profile == DecisionProfileV1::CombatPlayerDecision).then_some(
            CombatProjectionVersions {
                action_space: ActionSpaceProjectionVersion::PotionV2,
                state: CombatProjectionVersion::PublicIntentV2,
            },
        ),
    )?;
    let comparison_ms = comparison_started.elapsed().as_millis();
    let components = projection_summaries(&successor_projection.prepared);
    let timings = TransitionTimingV1 {
        rust_execution_ms,
        java_execution_ms,
        comparison_ms,
    };
    let comparisons = successor_projection
        .prepared
        .iter()
        .map(|projection| projection.comparison.clone())
        .collect::<Vec<_>>();
    if let Some(difference) = successor_projection
        .prepared
        .iter()
        .find(|projection| projection.comparison.status != ComparisonStatusV1::Exact)
        .and_then(|projection| projection.comparison.first_difference.clone())
    {
        let divergence = store.create_divergence(DivergenceBodyV1 {
            source_root_id: source.root_id.clone(),
            action: Some(action.clone()),
            stage: DivergenceStageV1::SuccessorComparison,
            kind: "successor_projection_diverged".to_owned(),
            difference,
            java_evidence: Some(java_receipt_blob),
            rust_evidence: Some(rust_receipt_blob),
        })?;
        let report = PairedRootPreviewReportV1 {
            schema: PREVIEW_REPORT_SCHEMA_V1.to_owned(),
            status: "diverged".to_owned(),
            source_root_id: source.root_id,
            source_activation_id: source_activation.activation_id,
            action,
            components,
            source_head_guard: pending_source_head_guard(),
            successor_root_id: None,
            successor_activation_id: None,
            receipt_id: None,
            divergence_id: Some(divergence.divergence_id),
            successor_boundary: None,
            java_evidence: None,
            rust_evidence: None,
            timings,
        };
        return finalize_preview_source_head_guard(request, source_head_before, report);
    }

    let rust_snapshot_directory = request.attempt_directory.join("rust-successor-recovery");
    create_fresh_directory(&rust_snapshot_directory)?;
    write_new(
        &rust_snapshot_directory.join("checkpoint.msgpack"),
        &successor_checkpoint_bytes,
    )?;
    write_new_json(
        &rust_snapshot_directory.join("source.json"),
        &json!({
            "schema": "sts-paired-rust-successor-source-v1",
            "source_root_id": source.root_id,
            "action_handle_id": action.handle_id,
        }),
    )?;
    let rust_bundle = store.put_directory(&rust_snapshot_directory)?;

    let action_space_id = paired_action_space_id(&successor_projection.java_actions)?;
    let proof = put_json(
        &store,
        &json!({
            "schema": "sts-paired-capability-proof-v1",
            "source_root_id": source.root_id,
            "action_handle_id": action.handle_id,
            "rust_successor_checkpoint_sha256": rust_transition_receipt.successor_checkpoint_sha256,
            "components": successor_projection.prepared.iter().map(|item| json!({
                "domain": item.comparison.domain,
                "java_sha256": item.comparison.java.sha256,
                "rust_sha256": item.comparison.rust.sha256,
            })).collect::<Vec<_>>(),
        }),
    )?;
    let (uncovered, capabilities) = successor_capability_plan.materialize(&proof);

    let mut receipt_body = TransitionReceiptBodyV1 {
        transition_commitment_id: String::new(),
        source_root_id: source.root_id.clone(),
        successor_root_id: "pending".to_owned(),
        action: action.clone(),
        java_action_receipt: java_receipt_blob,
        rust_transition_receipt: rust_receipt_blob,
        successor_comparisons: comparisons,
        timings: timings.clone(),
    };
    let transition_commitment =
        store.create_transition_commitment(transition_commitment_body(&receipt_body))?;
    receipt_body.transition_commitment_id = transition_commitment.transition_commitment_id;
    let mut rust_source_identity = vec![
        IdentityClaimV1 {
            kind: "action_handle_id".to_owned(),
            value: action.handle_id.clone(),
        },
        IdentityClaimV1 {
            kind: "parent_root_id".to_owned(),
            value: source.root_id.clone(),
        },
    ];
    rust_source_identity
        .sort_by(|left, right| (&left.kind, &left.value).cmp(&(&right.kind, &right.value)));
    let child = store.create_root(StateRootBodyV2 {
        provenance: source.root.provenance,
        world_provenance: source.root.world_provenance,
        parent: Some(ParentStateRootV2 {
            root_id: source.root_id.clone(),
            transition_commitment_id: receipt_body.transition_commitment_id.clone(),
        }),
        decision_profile: successor_profile_field(&source, successor_profile),
        versions: source.root.versions.clone(),
        content_profile: source.root.content_profile.clone(),
        rust_snapshot: RustStateSnapshotV2 {
            bundle: rust_bundle,
            entrypoint: "run_control_exact_checkpoint".to_owned(),
            source_identity: rust_source_identity,
        },
        preflight: RootPreflightV1 {
            action_space_id,
            components: receipt_body.successor_comparisons.clone(),
            uncovered,
        },
        card_bindings: successor_bindings,
        capabilities,
    })?;
    let successor_activation = store.create_activation(StateRootActivationBodyV2 {
        root_id: child.root_id.clone(),
        java_runtime_epoch: java_runtime_epoch(&successor_live),
        java_frame: put_json(&store, &successor_live.frame)?,
        action_space_id: child.root.preflight.action_space_id.clone(),
        card_leases: successor_leases,
    })?;
    receipt_body.successor_root_id = child.root_id.clone();
    let receipt = store.create_receipt(receipt_body)?;
    let report = PairedRootPreviewReportV1 {
        schema: PREVIEW_REPORT_SCHEMA_V1.to_owned(),
        status: "ready".to_owned(),
        source_root_id: source.root_id,
        source_activation_id: source_activation.activation_id,
        action,
        components,
        source_head_guard: pending_source_head_guard(),
        successor_root_id: Some(child.root_id),
        successor_activation_id: Some(successor_activation.activation_id),
        receipt_id: Some(receipt.receipt_id),
        divergence_id: None,
        successor_boundary: None,
        java_evidence: None,
        rust_evidence: None,
        timings,
    };
    finalize_preview_source_head_guard(request, source_head_before, report)
}

pub(crate) fn preview_paired_root_witness_transaction_v1(
    request: &PreviewPairedRootWitnessTransactionRequestV1,
) -> Result<PairedRootWitnessTransactionReportV1, String> {
    create_fresh_directory(&request.attempt_directory)?;
    let source_head_before = source_head_inventory(&request.source_session)?;
    let plan_bytes = fs::read(&request.witness_plan).map_err(|error| {
        format!(
            "failed to read paired-root witness transaction plan {}: {error}",
            request.witness_plan.display()
        )
    })?;
    let plan: PairedRootWitnessTransactionPlanV1 = serde_json::from_slice(&plan_bytes)
        .map_err(|error| format!("failed to decode paired-root witness plan: {error}"))?;
    if plan.schema != WITNESS_TRANSACTION_PLAN_SCHEMA_V1 {
        return Err(format!(
            "unsupported paired-root witness transaction schema {:?}",
            plan.schema
        ));
    }
    if plan.initial_root_id != request.source_root_id {
        return Err("paired-root witness plan is bound to a different initial root".to_owned());
    }
    if !content_id_has_shape(&plan.initial_root_id, "state-root-v2")
        || !content_id_has_shape(&plan.witness_id, "paired-search-witness-v1")
    {
        return Err("paired-root witness plan has an invalid content identity".to_owned());
    }
    if plan.intents.is_empty() || plan.intents.len() > 64 {
        return Err(
            "paired-root witness transaction requires one to sixty-four intents".to_owned(),
        );
    }

    let witness_id = plan.witness_id;
    let initial_root_id = plan.initial_root_id;
    let mut current_root_id = request.source_root_id.clone();
    let mut current_activation_id = request.source_activation_id.clone();
    let mut steps = Vec::with_capacity(plan.intents.len());
    let mut status = "ready".to_owned();
    let mut stopped_step_ordinal = None;
    let mut stop_reason = None;
    let mut total_timings = TransitionTimingV1 {
        rust_execution_ms: 0,
        java_execution_ms: 0,
        comparison_ms: 0,
    };

    for (step_ordinal, intent) in plan.intents.into_iter().enumerate() {
        let source_root_id = current_root_id.clone();
        let resolution = resolve_paired_root_search_intent_at_root_v1(
            &request.store_root,
            &source_root_id,
            &witness_id,
            step_ordinal,
            intent,
        )?;
        let Some(action_handle_id) = resolution.action_handle_id.clone() else {
            status = "intent_unavailable".to_owned();
            stopped_step_ordinal = Some(step_ordinal);
            stop_reason = resolution.reason.clone();
            steps.push(PairedRootWitnessTransactionStepV1 {
                step_ordinal,
                source_root_id,
                resolution,
                preview: None,
            });
            break;
        };

        let step_attempt = request
            .attempt_directory
            .join(format!("step-{step_ordinal:03}"));
        let preview = preview_paired_root_action_v1(&PreviewPairedRootActionRequestV1 {
            source_session: request.source_session.clone(),
            source_root_id: source_root_id.clone(),
            source_activation_id: current_activation_id.clone(),
            action_handle_id,
            java_live_output: request.java_live_output.clone(),
            store_root: request.store_root.clone(),
            attempt_directory: step_attempt,
            adapter_failure_report: request.adapter_failure_report.clone(),
        })?;
        total_timings.rust_execution_ms = total_timings
            .rust_execution_ms
            .saturating_add(preview.timings.rust_execution_ms);
        total_timings.java_execution_ms = total_timings
            .java_execution_ms
            .saturating_add(preview.timings.java_execution_ms);
        total_timings.comparison_ms = total_timings
            .comparison_ms
            .saturating_add(preview.timings.comparison_ms);
        if !preview.source_head_guard.unchanged {
            return Err("paired-root witness step changed the source head".to_owned());
        }

        match preview.status.as_str() {
            "ready" => {
                current_root_id = preview
                    .successor_root_id
                    .clone()
                    .ok_or_else(|| "Exact witness step omitted its successor root".to_owned())?;
                current_activation_id =
                    preview.successor_activation_id.clone().ok_or_else(|| {
                        "Exact witness step omitted its successor activation".to_owned()
                    })?;
                if preview.receipt_id.is_none() {
                    return Err("Exact witness step omitted its transition receipt".to_owned());
                }
            }
            "diverged" => {
                status = "diverged".to_owned();
                stopped_step_ordinal = Some(step_ordinal);
                stop_reason = Some("paired preview diverged".to_owned());
            }
            "successor_boundary_unsupported" => {
                let boundary = preview.successor_boundary.as_ref().ok_or_else(|| {
                    "unsupported witness successor omitted its typed boundary".to_owned()
                })?;
                status = "successor_boundary_unsupported".to_owned();
                stopped_step_ordinal = Some(step_ordinal);
                stop_reason = Some(format!(
                    "paired action reached unsupported successor boundary {}/{}",
                    boundary.kind,
                    boundary.screen_type.as_deref().unwrap_or("unspecified")
                ));
            }
            other => {
                return Err(format!(
                    "paired-root witness step returned unsupported status {other:?}"
                ));
            }
        }
        let should_stop = status != "ready";
        steps.push(PairedRootWitnessTransactionStepV1 {
            step_ordinal,
            source_root_id,
            resolution,
            preview: Some(preview),
        });
        if should_stop {
            break;
        }
    }

    let source_head_guard = checked_source_head_guard(&request.source_session, source_head_before)?;
    Ok(PairedRootWitnessTransactionReportV1 {
        schema: WITNESS_TRANSACTION_REPORT_SCHEMA_V1.to_owned(),
        status,
        witness_id,
        initial_root_id,
        reached_root_id: current_root_id,
        stopped_step_ordinal,
        stop_reason,
        steps,
        source_head_guard,
        timings: total_timings,
    })
}

pub(crate) fn recover_paired_root_v1(
    request: &RecoverPairedRootRequestV1,
) -> Result<PairedRootRecoveryReportV1, String> {
    create_fresh_directory(&request.attempt_directory)?;
    let source_head_before = source_head_inventory(&request.source_session)?;
    let store = StateRootStoreV2::new(&request.store_root).with_operation_verification_cache();
    store.initialize()?;
    let plan = store.recovery_plan(&request.target_root_id)?;
    if plan.native_root_id != request.native_root_id {
        return Err(format!(
            "target root recovers from native root {}, not prepared root {}",
            plan.native_root_id, request.native_root_id
        ));
    }
    let mut current_root = store.load_root(&request.native_root_id)?;
    let mut current_activation = store.load_activation(&request.native_activation_id)?;

    let mut live = java_live_status(&request.java_live_output)?;
    validate_live_stable_decision_profile(&live, root_decision_profile(&current_root))?;
    require_live_activation(&store, &current_root, &current_activation, &live)?;

    let mut steps = Vec::with_capacity(plan.steps.len());
    let mut total_timings = TransitionTimingV1 {
        rust_execution_ms: 0,
        java_execution_ms: 0,
        comparison_ms: 0,
    };
    for (ordinal, planned) in plan.steps.into_iter().enumerate() {
        if planned.source_root_id != current_root.root_id {
            return Err("paired recovery plan is not a contiguous root chain".to_owned());
        }
        let target_root = store.load_root(&planned.target_root_id)?;
        let commitment = store.load_transition_commitment(&planned.transition_commitment_id)?;
        require_verified_action_capability(&current_root, &planned.action.action)?;

        let source_checkpoint_bytes = store.read_bundle_file(
            &current_root.root.rust_snapshot.bundle,
            "checkpoint.msgpack",
        )?;
        let source_checkpoint: RunControlSessionCheckpointV1 =
            rmp_serde::from_slice(&source_checkpoint_bytes)
                .map_err(|error| format!("failed to decode source Rust checkpoint: {error}"))?;
        let source_session = source_checkpoint.into_session()?;
        let source_projection = project_pair_for_root(
            &store,
            &live,
            &source_session,
            &current_root.root.card_bindings,
            &current_activation.activation.card_leases,
            &current_root,
        )?;
        require_exact_projection_set(
            &source_projection.prepared,
            "recovery source root",
            root_decision_profile(&current_root),
        )?;
        if paired_action_space_id(&source_projection.java_actions)?
            != current_root.root.preflight.action_space_id
        {
            return Err("recovery source action-space identity changed".to_owned());
        }
        let source_comparisons = source_projection
            .prepared
            .iter()
            .map(|projection| projection.comparison.clone())
            .collect::<Vec<_>>();
        if source_comparisons != current_root.root.preflight.components {
            return Err("recovery source projections no longer match the stored root".to_owned());
        }
        let handles = paired_action_handles(&current_root, &current_activation, &live)?;
        if !handles.contains(&planned.action) {
            return Err(format!(
                "recovery action {} is not legal at step {}",
                planned.action.handle_id,
                ordinal + 1
            ));
        }
        let java_action_id =
            java_action_id_for_handle(&current_root, &current_activation, &live, &planned.action)?;
        let rust_action =
            rust_decision_action_for_action(&current_root, &source_session, &planned.action)?;
        let combat_sequence = source_session.combat_sequence();
        let rust_card_births = rust_action
            .executable_input()
            .as_ref()
            .map(|input| observe_rust_card_births_for_input_v1(&source_session, input))
            .transpose()?
            .flatten();

        let java_started = Instant::now();
        let java_receipt = java_live_act(
            &request.java_live_output,
            &java_action_id,
            Duration::from_secs(30),
        )?;
        let java_execution_ms = java_started.elapsed().as_millis();
        if java_receipt.status != "completed"
            || java_receipt.code.is_some()
            || java_receipt.successor_frame_id.is_none()
            || java_receipt.action_id != java_action_id
        {
            return Err(format!(
                "Java recovery step {} did not produce one completed stable successor",
                ordinal + 1
            ));
        }
        let successor_live = java_live_status(&request.java_live_output)?;
        validate_live_stable_decision_profile(
            &successor_live,
            root_decision_profile(&target_root),
        )?;
        let target_checkpoint_bytes =
            store.read_bundle_file(&target_root.root.rust_snapshot.bundle, "checkpoint.msgpack")?;
        let target_checkpoint: RunControlSessionCheckpointV1 =
            rmp_serde::from_slice(&target_checkpoint_bytes)
                .map_err(|error| format!("failed to decode target Rust checkpoint: {error}"))?;
        let target_session = target_checkpoint.into_session()?;
        let target_leases = match root_decision_profile(&target_root) {
            DecisionProfileV1::CombatPlayerDecision => recover_successor_card_leases_v1(
                &current_root,
                &current_activation,
                &target_root,
                &live,
                &successor_live,
                &java_receipt,
                rust_card_births.as_ref(),
                combat_sequence,
            )?,
            DecisionProfileV1::CombatReward
            | DecisionProfileV1::CardReward
            | DecisionProfileV1::Event
            | DecisionProfileV1::MapNavigation
            | DecisionProfileV1::Neow => build_java_card_leases(
                &successor_live,
                &target_session,
                &target_root.root.card_bindings,
            )?,
        };
        let comparison_started = Instant::now();
        let projected = project_pair_for_root(
            &store,
            &successor_live,
            &target_session,
            &target_root.root.card_bindings,
            &target_leases,
            &target_root,
        )?;
        let comparison_ms = comparison_started.elapsed().as_millis();
        let components = projection_summaries(&projected.prepared);
        let comparisons = projected
            .prepared
            .iter()
            .map(|projection| projection.comparison.clone())
            .collect::<Vec<_>>();
        let java_replay_receipt = put_json(&store, &java_receipt)?;
        let timings = TransitionTimingV1 {
            rust_execution_ms: 0,
            java_execution_ms,
            comparison_ms,
        };
        total_timings.java_execution_ms = total_timings
            .java_execution_ms
            .saturating_add(java_execution_ms);
        total_timings.comparison_ms = total_timings.comparison_ms.saturating_add(comparison_ms);

        let difference = projected
            .prepared
            .iter()
            .find(|projection| projection.comparison.status != ComparisonStatusV1::Exact)
            .and_then(|projection| projection.comparison.first_difference.clone())
            .or_else(|| {
                (paired_action_space_id(&projected.java_actions)
                    .ok()
                    .as_deref()
                    != Some(target_root.root.preflight.action_space_id.as_str()))
                .then(|| TypedDifferenceV1 {
                    path: "$.action_space_id".to_owned(),
                    kind: "stored_target_action_space_changed".to_owned(),
                    detail: "recovery produced a different logical target action space".to_owned(),
                })
            })
            .or_else(|| {
                (comparisons != target_root.root.preflight.components).then(|| TypedDifferenceV1 {
                    path: "$.preflight.components".to_owned(),
                    kind: "stored_target_projection_changed".to_owned(),
                    detail: "recovery projections no longer match the stored target root"
                        .to_owned(),
                })
            });
        if let Some(difference) = difference {
            let divergence = store.create_divergence(DivergenceBodyV1 {
                source_root_id: current_root.root_id.clone(),
                action: Some(planned.action.clone()),
                stage: DivergenceStageV1::SuccessorComparison,
                kind: "root_recovery_diverged".to_owned(),
                difference,
                java_evidence: Some(java_replay_receipt.clone()),
                rust_evidence: Some(commitment.transition.rust_transition_receipt),
            })?;
            steps.push(PairedRootRecoveryReportStepV1 {
                ordinal: ordinal + 1,
                source_root_id: current_root.root_id,
                source_activation_id: current_activation.activation_id,
                transition_commitment_id: planned.transition_commitment_id,
                action: planned.action,
                target_root_id: target_root.root_id,
                target_activation_id: None,
                components,
                java_replay_receipt,
                divergence_id: Some(divergence.divergence_id.clone()),
                timings,
            });
            let report = PairedRootRecoveryReportV1 {
                schema: RECOVERY_REPORT_SCHEMA_V1.to_owned(),
                status: "diverged".to_owned(),
                native_root_id: request.native_root_id.clone(),
                native_activation_id: request.native_activation_id.clone(),
                target_root_id: request.target_root_id.clone(),
                target_activation_id: None,
                steps,
                source_head_guard: pending_source_head_guard(),
                divergence_id: Some(divergence.divergence_id),
                timings: total_timings,
            };
            return finalize_recovery_source_head_guard(request, source_head_before, report);
        }

        let target_activation = store.create_activation(StateRootActivationBodyV2 {
            root_id: target_root.root_id.clone(),
            java_runtime_epoch: java_runtime_epoch(&successor_live),
            java_frame: put_json(&store, &successor_live.frame)?,
            action_space_id: target_root.root.preflight.action_space_id.clone(),
            card_leases: target_leases,
        })?;
        steps.push(PairedRootRecoveryReportStepV1 {
            ordinal: ordinal + 1,
            source_root_id: current_root.root_id,
            source_activation_id: current_activation.activation_id,
            transition_commitment_id: planned.transition_commitment_id,
            action: planned.action,
            target_root_id: target_root.root_id.clone(),
            target_activation_id: Some(target_activation.activation_id.clone()),
            components,
            java_replay_receipt,
            divergence_id: None,
            timings,
        });
        current_root = target_root;
        current_activation = target_activation;
        live = successor_live;
    }
    if current_root.root_id != request.target_root_id {
        return Err("paired recovery plan did not terminate at its target root".to_owned());
    }
    let report = PairedRootRecoveryReportV1 {
        schema: RECOVERY_REPORT_SCHEMA_V1.to_owned(),
        status: "ready".to_owned(),
        native_root_id: request.native_root_id.clone(),
        native_activation_id: request.native_activation_id.clone(),
        target_root_id: current_root.root_id,
        target_activation_id: Some(current_activation.activation_id),
        steps,
        source_head_guard: pending_source_head_guard(),
        divergence_id: None,
        timings: total_timings,
    };
    finalize_recovery_source_head_guard(request, source_head_before, report)
}

fn root_decision_profile(root: &StateRootArtifactV2) -> DecisionProfileV1 {
    root.root.decision_profile
}

fn successor_profile_field(
    source: &StateRootArtifactV2,
    successor: DecisionProfileV1,
) -> DecisionProfileV1 {
    let _ = source;
    successor
}

fn capability_for_action(action: &PairedActionV1) -> CapabilityKindV1 {
    match action {
        PairedActionV1::ChooseNeowOption { reward, drawback } => {
            neow_capability_for_choice(*reward, *drawback)
        }
        PairedActionV1::LeaveNeow => CapabilityKindV1::LeaveNeow,
        PairedActionV1::PlayCard { .. } => CapabilityKindV1::PlayCard,
        PairedActionV1::EndTurn => CapabilityKindV1::EndTurn,
        PairedActionV1::UsePotion { .. } => CapabilityKindV1::UsePotion,
        PairedActionV1::ClaimCombatReward { .. } | PairedActionV1::CloseCombatReward => {
            CapabilityKindV1::ResolveCombatReward
        }
        PairedActionV1::ChooseCardReward { .. } | PairedActionV1::SkipCardReward => {
            CapabilityKindV1::ResolveCardReward
        }
        PairedActionV1::ChooseEventOption { .. } => CapabilityKindV1::ResolveEvent,
        PairedActionV1::ChooseMapNode { room_type, .. } => match room_type {
            MapRoomIdentityV1::Monster => CapabilityKindV1::EnterCombat,
            MapRoomIdentityV1::QuestionMark => CapabilityKindV1::EnterQuestionMark,
            MapRoomIdentityV1::Elite
            | MapRoomIdentityV1::Boss
            | MapRoomIdentityV1::Rest
            | MapRoomIdentityV1::Shop
            | MapRoomIdentityV1::Treasure
            | MapRoomIdentityV1::Victory
            | MapRoomIdentityV1::Neow => CapabilityKindV1::ChooseMapNode,
        },
    }
}

fn neow_capability_for_choice(
    reward: NeowRewardIdentityV1,
    drawback: NeowDrawbackIdentityV1,
) -> CapabilityKindV1 {
    match (reward, drawback) {
        (NeowRewardIdentityV1::HundredGold, NeowDrawbackIdentityV1::None) => {
            CapabilityKindV1::ChooseNeowHundredGoldNoDrawback
        }
        (NeowRewardIdentityV1::ThreeEnemyKill, NeowDrawbackIdentityV1::None) => {
            CapabilityKindV1::ChooseNeowThreeEnemyKillNoDrawback
        }
        (
            NeowRewardIdentityV1::RemoveCard
            | NeowRewardIdentityV1::UpgradeCard
            | NeowRewardIdentityV1::TransformCard
            | NeowRewardIdentityV1::RemoveTwo
            | NeowRewardIdentityV1::TransformTwoCards,
            _,
        ) => CapabilityKindV1::ChooseNeowDeckSelection,
        (
            NeowRewardIdentityV1::ThreeCards
            | NeowRewardIdentityV1::RandomColorless
            | NeowRewardIdentityV1::ThreeSmallPotions
            | NeowRewardIdentityV1::RandomColorless2
            | NeowRewardIdentityV1::ThreeRareCards,
            _,
        ) => CapabilityKindV1::ChooseNeowReward,
        (
            NeowRewardIdentityV1::ThreeEnemyKill
            | NeowRewardIdentityV1::TenPercentHpBonus
            | NeowRewardIdentityV1::OneRandomRareCard
            | NeowRewardIdentityV1::RandomCommonRelic
            | NeowRewardIdentityV1::HundredGold
            | NeowRewardIdentityV1::OneRareRelic
            | NeowRewardIdentityV1::TwoFiftyGold
            | NeowRewardIdentityV1::TwentyPercentHpBonus
            | NeowRewardIdentityV1::BossRelic,
            _,
        ) => CapabilityKindV1::ChooseNeowClosed,
    }
}

fn required_projection_domains(profile: DecisionProfileV1) -> BTreeSet<ProjectionDomainV1> {
    match profile {
        DecisionProfileV1::CombatPlayerDecision => BTreeSet::from([
            ProjectionDomainV1::RunScalars,
            ProjectionDomainV1::MasterDeck,
            ProjectionDomainV1::Relics,
            ProjectionDomainV1::Potions,
            ProjectionDomainV1::RngStreams,
            ProjectionDomainV1::CardPools,
            ProjectionDomainV1::Combat,
            ProjectionDomainV1::ActionSpace,
            ProjectionDomainV1::CardBindings,
        ]),
        DecisionProfileV1::CombatReward
        | DecisionProfileV1::CardReward
        | DecisionProfileV1::Event
        | DecisionProfileV1::Neow => BTreeSet::from([
            ProjectionDomainV1::RunScalars,
            ProjectionDomainV1::Map,
            ProjectionDomainV1::MasterDeck,
            ProjectionDomainV1::Relics,
            ProjectionDomainV1::Potions,
            ProjectionDomainV1::RngStreams,
            ProjectionDomainV1::CardPools,
            ProjectionDomainV1::Room,
            ProjectionDomainV1::ActionSpace,
            ProjectionDomainV1::CardBindings,
            ProjectionDomainV1::RuntimeSettlement,
        ]),
        DecisionProfileV1::MapNavigation => BTreeSet::from([
            ProjectionDomainV1::RunScalars,
            ProjectionDomainV1::Map,
            ProjectionDomainV1::MasterDeck,
            ProjectionDomainV1::Relics,
            ProjectionDomainV1::Potions,
            ProjectionDomainV1::RngStreams,
            ProjectionDomainV1::CardPools,
            ProjectionDomainV1::ActionSpace,
            ProjectionDomainV1::CardBindings,
            ProjectionDomainV1::RuntimeSettlement,
        ]),
    }
}

fn capability_surface_plan_for_profile(
    profile: DecisionProfileV1,
    session: &RunControlSession,
) -> Result<CapabilitySurfacePlanV1, String> {
    let required = required_projection_domains(profile)
        .into_iter()
        .collect::<Vec<_>>();
    let capabilities = if profile == DecisionProfileV1::Neow {
        let mut capabilities = vec![
            verified_capability_plan(CapabilityKindV1::Observe, &required),
            verified_capability_plan(CapabilityKindV1::ChooseNeowHundredGoldNoDrawback, &required),
            verified_capability_plan(
                CapabilityKindV1::ChooseNeowThreeEnemyKillNoDrawback,
                &required,
            ),
        ];
        for (capability, gap) in [
            (
                CapabilityKindV1::ChooseNeowClosed,
                capability_gap(
                    "neow-closed-transition-unverified-v1",
                    "other immediate Neow reward/drawback shapes are typed and observable, but their dual-runtime successors have not yet been verified",
                ),
            ),
            (
                CapabilityKindV1::ChooseNeowDeckSelection,
                capability_gap(
                    "neow-deck-selection-transition-unverified-v1",
                    "Neow deck-operation rewards require a typed selection successor profile before execution can be verified",
                ),
            ),
            (
                CapabilityKindV1::ChooseNeowReward,
                capability_gap(
                    "neow-reward-transition-unverified-v1",
                    "Neow rewards that open card or potion reward screens require their own typed successor coverage",
                ),
            ),
        ] {
            capabilities.push(unknown_capability_plan(capability, &required, gap));
        }
        capabilities.push(verified_capability_plan(
            CapabilityKindV1::LeaveNeow,
            &required,
        ));
        capabilities
    } else {
        let kinds = match profile {
            DecisionProfileV1::CombatPlayerDecision => vec![
                CapabilityKindV1::Observe,
                CapabilityKindV1::PlayCard,
                CapabilityKindV1::EndTurn,
                CapabilityKindV1::UsePotion,
            ],
            DecisionProfileV1::CombatReward => {
                vec![
                    CapabilityKindV1::Observe,
                    CapabilityKindV1::ResolveCombatReward,
                ]
            }
            DecisionProfileV1::CardReward => vec![
                CapabilityKindV1::Observe,
                CapabilityKindV1::ResolveCardReward,
            ],
            DecisionProfileV1::Event => {
                vec![CapabilityKindV1::Observe, CapabilityKindV1::ResolveEvent]
            }
            DecisionProfileV1::MapNavigation => {
                vec![CapabilityKindV1::Observe, CapabilityKindV1::EnterCombat]
            }
            DecisionProfileV1::Neow => unreachable!("Neow capabilities returned above"),
        };
        let mut capabilities = kinds
            .into_iter()
            .map(|capability| verified_capability_plan(capability, &required))
            .collect::<Vec<_>>();
        if profile == DecisionProfileV1::MapNavigation {
            capabilities.push(CapabilityPlanV1 {
                capability: CapabilityKindV1::EnterQuestionMark,
                required_domains: required.clone(),
                status: question_mark_entry_capability_status(session),
            });
            capabilities.push(unknown_capability_plan(
                CapabilityKindV1::ChooseMapNode,
                &required,
                capability_gap(
                    "map-node-transition-unverified-v1",
                    "ordinary Monster nodes use EnterCombat; question-mark nodes use blind EnterQuestionMark authorization and reveal their successor only after entry; elite, boss, rest, shop, treasure, victory, and Neow remain unverified",
                ),
            ));
        }
        capabilities
    };
    CapabilitySurfacePlanV1::try_new(independent_coverage_gaps_for_profile(profile), capabilities)
}

fn verified_capability_plan(
    capability: CapabilityKindV1,
    required_domains: &[ProjectionDomainV1],
) -> CapabilityPlanV1 {
    CapabilityPlanV1 {
        capability,
        required_domains: required_domains.to_vec(),
        status: CapabilityPlanStatusV1::Verified,
    }
}

fn unknown_capability_plan(
    capability: CapabilityKindV1,
    required_domains: &[ProjectionDomainV1],
    gap: CoverageGapV1,
) -> CapabilityPlanV1 {
    CapabilityPlanV1 {
        capability,
        required_domains: required_domains.to_vec(),
        status: CapabilityPlanStatusV1::Unknown { gap },
    }
}

fn capability_gap(gap_id: impl Into<String>, detail: impl Into<String>) -> CoverageGapV1 {
    CoverageGapV1 {
        gap_id: gap_id.into(),
        domain: ProjectionDomainV1::ActionSpace,
        scope: CoverageGapScopeV1::ActionCapability,
        kind: "capability_not_verified".to_owned(),
        detail: detail.into(),
    }
}

fn question_mark_entry_capability_status(session: &RunControlSession) -> CapabilityPlanStatusV1 {
    if !matches!(session.engine_state, EngineState::MapNavigation) {
        return CapabilityPlanStatusV1::Unknown {
            gap: capability_gap(
                "question-mark-entry-requires-map-root-v1",
                "question-mark entry authorization is only meaningful at a Map root",
            ),
        };
    }
    let has_question_mark =
        build_decision_surface(session)
            .view
            .candidates
            .iter()
            .any(|candidate| {
                matches!(
                    candidate.key.as_ref(),
                    Some(DecisionCandidateKey::RouteSelect {
                        room_type: Some(PlannerMapRoom::QuestionMark),
                        ..
                    })
                ) && candidate.action.executable_action_ref().is_some()
            });
    if !has_question_mark {
        return CapabilityPlanStatusV1::Unknown {
            gap: capability_gap(
                "question-mark-entry-absent-at-this-root-v1",
                "the current Map root has no executable question-mark candidate",
            ),
        };
    }
    // A question mark is public, but the room/event selected behind it is not.
    // Authorization must therefore depend only on the public action surface. The
    // preview transition reveals and classifies the successor after entry; a
    // stable scene without a supported profile is returned through the existing
    // successor_boundary_unsupported result instead of being leaked here.
    CapabilityPlanStatusV1::Verified
}

fn independent_coverage_gaps_for_profile(profile: DecisionProfileV1) -> Vec<CoverageGapV1> {
    let mut gaps = match profile {
        DecisionProfileV1::CombatPlayerDecision => Vec::new(),
        DecisionProfileV1::CombatReward => vec![
            CoverageGapV1 {
                gap_id: "reward-card-identity-unavailable-v1".to_owned(),
                domain: ProjectionDomainV1::Room,
                scope: CoverageGapScopeV1::StateProjection,
                kind: "java_visible_identity_absent".to_owned(),
                detail: "unopened Java card rewards expose choice count and opaque UUIDs, not semantic card IDs"
                    .to_owned(),
            },
            CoverageGapV1 {
                gap_id: "reward-relic-key-and-full-belt-potion-actions-unsupported-v1".to_owned(),
                domain: ProjectionDomainV1::ActionSpace,
                scope: CoverageGapScopeV1::ActionCapability,
                kind: "capability_not_implemented".to_owned(),
                detail: "the CombatReward slice exposes deterministic Gold, StolenGold, card-open actions, potion claims when an empty belt slot exists, and a separate close on an empty page; relic, key, and full-belt potion claims remain unavailable"
                    .to_owned(),
            },
        ],
        DecisionProfileV1::CardReward => vec![
            CoverageGapV1 {
                gap_id: "card-reward-singing-bowl-action-unsupported-v1".to_owned(),
                domain: ProjectionDomainV1::ActionSpace,
                scope: CoverageGapScopeV1::ActionCapability,
                kind: "capability_not_implemented".to_owned(),
                detail: "card take and skip are supported; Singing Bowl remains outside this CardReward action slice"
                    .to_owned(),
            },
        ],
        DecisionProfileV1::Event => vec![CoverageGapV1 {
            gap_id: "event-selection-successor-unverified-v1".to_owned(),
            domain: ProjectionDomainV1::ActionSpace,
            scope: CoverageGapScopeV1::ActionCapability,
            kind: "capability_scoped".to_owned(),
            detail: "the Event profile currently verifies direct typed event options; hand/grid selections and other event-specific successor shapes remain fail-closed"
                .to_owned(),
        }],
        DecisionProfileV1::MapNavigation | DecisionProfileV1::Neow => Vec::new(),
    };
    gaps.sort_by(|left, right| left.gap_id.cmp(&right.gap_id));
    gaps
}

fn action_space_projection_schema(version: ActionSpaceProjectionVersion) -> &'static str {
    match version {
        ActionSpaceProjectionVersion::CardOnlyV1 => ACTION_SPACE_SCHEMA_CARD_ONLY_V1,
        ActionSpaceProjectionVersion::PotionV2 => ACTION_SPACE_SCHEMA_POTION_V2,
    }
}

fn combat_projection_schema(version: CombatProjectionVersion) -> &'static str {
    match version {
        CombatProjectionVersion::LegacyV1 => COMBAT_ROOT_SCHEMA_V1,
        CombatProjectionVersion::PublicIntentV2 => COMBAT_ROOT_SCHEMA_PUBLIC_INTENT_V2,
    }
}

fn combat_projection_version(
    root: &StateRootArtifactV2,
) -> Result<CombatProjectionVersion, String> {
    let combat = root
        .root
        .preflight
        .components
        .iter()
        .find(|component| component.domain == ProjectionDomainV1::Combat)
        .ok_or_else(|| "paired root omitted its combat projection".to_owned())?;
    match combat.projection_schema.as_str() {
        COMBAT_ROOT_SCHEMA_V1 => Ok(CombatProjectionVersion::LegacyV1),
        COMBAT_ROOT_SCHEMA_PUBLIC_INTENT_V2 => Ok(CombatProjectionVersion::PublicIntentV2),
        other => Err(format!(
            "paired root uses unsupported combat projection {other:?}"
        )),
    }
}

fn action_space_projection_version(
    root: &StateRootArtifactV2,
) -> Result<ActionSpaceProjectionVersion, String> {
    let action_space = root
        .root
        .preflight
        .components
        .iter()
        .find(|component| component.domain == ProjectionDomainV1::ActionSpace)
        .ok_or_else(|| "paired root omitted its action-space projection".to_owned())?;
    match action_space.projection_schema.as_str() {
        ACTION_SPACE_SCHEMA_CARD_ONLY_V1 => Ok(ActionSpaceProjectionVersion::CardOnlyV1),
        ACTION_SPACE_SCHEMA_POTION_V2 => Ok(ActionSpaceProjectionVersion::PotionV2),
        other => Err(format!(
            "paired root uses unsupported action-space projection {other:?}"
        )),
    }
}

fn require_verified_capability(
    root: &StateRootArtifactV2,
    capability: CapabilityKindV1,
) -> Result<(), String> {
    let proof = require_declared_capability(root, capability)?;
    if !matches!(proof.status, CapabilityStatusV1::Verified { .. }) {
        return Err(format!(
            "paired root {capability:?} capability is not verified"
        ));
    }
    Ok(())
}

fn require_verified_action_capability(
    root: &StateRootArtifactV2,
    action: &PairedActionV1,
) -> Result<(), String> {
    let proof = require_declared_action_capability(root, action)?;
    if !matches!(proof.status, CapabilityStatusV1::Verified { .. }) {
        return Err(format!(
            "paired root {:?} capability is not verified",
            proof.capability
        ));
    }
    Ok(())
}

fn require_declared_action_capability<'a>(
    root: &'a StateRootArtifactV2,
    action: &PairedActionV1,
) -> Result<&'a CapabilityProofV1, String> {
    let capability = capability_for_action(action);
    if let Ok(proof) = require_declared_capability(root, capability) {
        return Ok(proof);
    }
    if matches!(
        action,
        PairedActionV1::ChooseMapNode {
            room_type: MapRoomIdentityV1::QuestionMark,
            ..
        }
    ) {
        if let Ok(proof) = require_declared_capability(root, CapabilityKindV1::ChooseMapNode) {
            return Ok(proof);
        }
    }
    if matches!(
        action,
        PairedActionV1::ClaimCombatReward { .. } | PairedActionV1::CloseCombatReward
    ) && root_decision_profile(root) == DecisionProfileV1::CombatReward
    {
        if let Ok(proof) = require_declared_capability(root, CapabilityKindV1::ResolveCardReward) {
            return Ok(proof);
        }
    }
    require_declared_capability(root, capability)
}

fn require_declared_capability(
    root: &StateRootArtifactV2,
    capability: CapabilityKindV1,
) -> Result<&CapabilityProofV1, String> {
    root.root
        .capabilities
        .iter()
        .find(|proof| proof.capability == capability)
        .ok_or_else(|| format!("paired root omitted {capability:?} capability"))
}

fn require_live_activation(
    store: &StateRootStoreV2,
    root: &StateRootArtifactV2,
    activation: &StateRootActivationArtifactV2,
    live: &LiveStatus,
) -> Result<(), String> {
    if activation.activation.root_id != root.root_id {
        return Err("paired activation belongs to another logical root".to_owned());
    }
    if activation.activation.java_runtime_epoch != java_runtime_epoch(live) {
        return Err(
            "paired activation belongs to another Java runtime epoch; restore the root and prepare a fresh activation"
                .to_owned(),
        );
    }
    if activation.activation.action_space_id != root.root.preflight.action_space_id {
        return Err("paired activation belongs to another root action space".to_owned());
    }
    if activation.activation.java_frame != put_json(store, &live.frame)? {
        return Err(
            "live Java frame changed after this activation was issued; prepare a fresh activation"
                .to_owned(),
        );
    }
    Ok(())
}

fn project_pair_for_root(
    store: &StateRootStoreV2,
    live: &LiveStatus,
    session: &RunControlSession,
    bindings: &[CrossRuntimeCardBindingV1],
    leases: &[JavaCardLeaseV1],
    root: &StateRootArtifactV2,
) -> Result<ProjectedPairV1, String> {
    let profile = root_decision_profile(root);
    let combat_versions = if profile == DecisionProfileV1::CombatPlayerDecision {
        Some(CombatProjectionVersions {
            action_space: action_space_projection_version(root)?,
            state: combat_projection_version(root)?,
        })
    } else {
        None
    };
    project_pair_for_profile(
        store,
        live,
        session,
        bindings,
        leases,
        profile,
        combat_versions,
    )
}

fn project_pair_for_profile(
    store: &StateRootStoreV2,
    live: &LiveStatus,
    session: &RunControlSession,
    bindings: &[CrossRuntimeCardBindingV1],
    leases: &[JavaCardLeaseV1],
    profile: DecisionProfileV1,
    combat_versions: Option<CombatProjectionVersions>,
) -> Result<ProjectedPairV1, String> {
    match profile {
        DecisionProfileV1::CombatPlayerDecision => {
            let versions = combat_versions
                .ok_or_else(|| "combat projection omitted its schema versions".to_owned())?;
            project_combat_pair(
                store,
                live,
                session,
                bindings,
                leases,
                versions.action_space,
                versions.state,
            )
        }
        DecisionProfileV1::CombatReward
        | DecisionProfileV1::CardReward
        | DecisionProfileV1::Event
        | DecisionProfileV1::MapNavigation
        | DecisionProfileV1::Neow => {
            project_stable_noncombat_pair(store, live, session, bindings, leases, profile)
        }
    }
}

fn project_combat_pair(
    store: &StateRootStoreV2,
    live: &LiveStatus,
    session: &RunControlSession,
    bindings: &[CrossRuntimeCardBindingV1],
    leases: &[JavaCardLeaseV1],
    action_space_version: ActionSpaceProjectionVersion,
    combat_version: CombatProjectionVersion,
) -> Result<ProjectedPairV1, String> {
    let coverage = required_java_frame_coverage_gaps(&live.frame);
    if !coverage.is_empty() {
        return Err(format!(
            "paired successor omitted required Java coverage: {}",
            coverage
                .iter()
                .map(|gap| gap.gap_id.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    let binding_by_java = leases
        .iter()
        .map(|lease| (lease.java_card_uuid.as_str(), lease.binding_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let binding_by_rust = bindings
        .iter()
        .map(|binding| {
            (
                rust_lineage_key(&binding.rust_lineage),
                binding.binding_id.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut prepared = vec![
        compare_projection(
            store,
            ProjectionDomainV1::RunScalars,
            "sts-paired-run-scalars-v1",
            java_run_scalars(&live.frame)?,
            rust_run_scalars(session)?,
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::MasterDeck,
            "sts-paired-master-deck-order-v1",
            java_master_deck_semantics(&live.frame)?,
            rust_master_deck_semantics(session),
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::Relics,
            "sts-paired-relic-order-counter-v1",
            java_relics(&live.frame)?,
            rust_combat_relics(session)?,
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::Potions,
            "sts-paired-potion-slots-v1",
            java_potions(&live.frame)?,
            rust_potions(session)?,
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::RngStreams,
            "sts-paired-rng-streams-v1",
            java_post_neow_rng_streams(&live.frame)?,
            rust_rng_streams(session)?,
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::CardPools,
            "sts-paired-card-pools-v1",
            java_card_pools(&live.frame)?,
            rust_card_pools(session)?,
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::Combat,
            combat_projection_schema(combat_version),
            java_combat(&live.frame, &binding_by_java, combat_version)?,
            rust_combat(session, &binding_by_rust, combat_version)?,
        )?,
    ];
    let java_actions = java_action_space(live, &binding_by_java, action_space_version)?;
    let rust_actions = rust_action_space(session, &binding_by_rust, action_space_version)?;
    prepared.push(compare_projection(
        store,
        ProjectionDomainV1::ActionSpace,
        action_space_projection_schema(action_space_version),
        java_actions.clone(),
        rust_actions,
    )?);
    prepared.push(relational_binding_projection(
        store,
        bindings,
        leases,
        &live.frame,
        session,
    )?);
    prepared.sort_by_key(|projection| projection.comparison.domain);
    Ok(ProjectedPairV1 {
        prepared,
        java_actions,
    })
}

#[derive(Clone, Copy, Debug)]
struct StableNoncombatProjectionContractV1 {
    action_space_schema: &'static str,
    profile_projection_schemas: &'static [(ProjectionDomainV1, &'static str)],
}

struct StableNoncombatSurfaceV1 {
    java_actions: Value,
    rust_actions: Value,
    profile_projections: Vec<StableNoncombatProfileProjectionV1>,
}

struct StableNoncombatProfileProjectionV1 {
    domain: ProjectionDomainV1,
    java: Value,
    rust: Value,
}

fn stable_noncombat_projection_contract(
    profile: DecisionProfileV1,
) -> Result<StableNoncombatProjectionContractV1, String> {
    match profile {
        DecisionProfileV1::CombatReward => Ok(StableNoncombatProjectionContractV1 {
            action_space_schema: REWARD_ACTION_SPACE_SCHEMA_V1,
            profile_projection_schemas: &REWARD_PROFILE_PROJECTION_SCHEMAS_V1,
        }),
        DecisionProfileV1::CardReward => Ok(StableNoncombatProjectionContractV1 {
            action_space_schema: CARD_REWARD_ACTION_SPACE_SCHEMA_V1,
            profile_projection_schemas: &CARD_REWARD_PROFILE_PROJECTION_SCHEMAS_V1,
        }),
        DecisionProfileV1::Event => Ok(StableNoncombatProjectionContractV1 {
            action_space_schema: EVENT_ACTION_SPACE_SCHEMA_V1,
            profile_projection_schemas: &EVENT_PROFILE_PROJECTION_SCHEMAS_V1,
        }),
        DecisionProfileV1::MapNavigation => Ok(StableNoncombatProjectionContractV1 {
            action_space_schema: MAP_ACTION_SPACE_SCHEMA_V1,
            profile_projection_schemas: &MAP_PROFILE_PROJECTION_SCHEMAS_V1,
        }),
        DecisionProfileV1::Neow => Ok(StableNoncombatProjectionContractV1 {
            action_space_schema: NEOW_ACTION_SPACE_SCHEMA_V1,
            profile_projection_schemas: &NEOW_PROFILE_PROJECTION_SCHEMAS_V1,
        }),
        DecisionProfileV1::CombatPlayerDecision => Err(
            "combat player decisions do not use the stable noncombat projection contract"
                .to_owned(),
        ),
    }
}

fn stable_noncombat_surface(
    live: &LiveStatus,
    session: &RunControlSession,
    profile: DecisionProfileV1,
) -> Result<StableNoncombatSurfaceV1, String> {
    validate_live_stable_decision_profile(live, profile)?;
    match profile {
        DecisionProfileV1::CombatReward => Ok(StableNoncombatSurfaceV1 {
            java_actions: java_reward_action_space(live)?,
            rust_actions: rust_reward_action_space(session)?,
            profile_projections: vec![
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::Map,
                    java: java_public_map(&live.frame)?,
                    rust: rust_public_map(session)?,
                },
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::Room,
                    java: java_reward_room(live)?,
                    rust: rust_reward_room(session)?,
                },
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::RuntimeSettlement,
                    java: reward_runtime_settlement(),
                    rust: reward_runtime_settlement(),
                },
            ],
        }),
        DecisionProfileV1::CardReward => Ok(StableNoncombatSurfaceV1 {
            java_actions: java_card_reward_action_space(live)?,
            rust_actions: rust_card_reward_action_space(session)?,
            profile_projections: vec![
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::Map,
                    java: java_public_map(&live.frame)?,
                    rust: rust_public_map(session)?,
                },
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::Room,
                    java: java_card_reward_room(live)?,
                    rust: rust_card_reward_room(session)?,
                },
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::RuntimeSettlement,
                    java: reward_runtime_settlement(),
                    rust: reward_runtime_settlement(),
                },
            ],
        }),
        DecisionProfileV1::Event => Ok(StableNoncombatSurfaceV1 {
            java_actions: java_event_action_space(live)?,
            rust_actions: rust_event_action_space(session)?,
            profile_projections: vec![
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::Map,
                    java: java_public_map(&live.frame)?,
                    rust: rust_public_map(session)?,
                },
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::Room,
                    java: java_event_room(live)?,
                    rust: rust_event_room(session)?,
                },
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::RuntimeSettlement,
                    java: event_runtime_settlement(),
                    rust: event_runtime_settlement(),
                },
            ],
        }),
        DecisionProfileV1::MapNavigation => Ok(StableNoncombatSurfaceV1 {
            java_actions: project_java_map_action_space(&live.frame)?,
            rust_actions: rust_map_action_space(session)?,
            profile_projections: vec![
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::Map,
                    java: java_public_map(&live.frame)?,
                    rust: rust_public_map(session)?,
                },
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::RuntimeSettlement,
                    java: map_runtime_settlement(),
                    rust: map_runtime_settlement(),
                },
            ],
        }),
        DecisionProfileV1::Neow => Ok(StableNoncombatSurfaceV1 {
            java_actions: project_java_neow_action_space(&live.frame)?,
            rust_actions: rust_neow_action_space(session)?,
            profile_projections: vec![
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::Map,
                    java: java_public_map(&live.frame)?,
                    rust: rust_public_map(session)?,
                },
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::Room,
                    java: project_java_neow_room(&live.frame)?,
                    rust: rust_neow_room(session)?,
                },
                StableNoncombatProfileProjectionV1 {
                    domain: ProjectionDomainV1::RuntimeSettlement,
                    java: neow_runtime_settlement(),
                    rust: neow_runtime_settlement(),
                },
            ],
        }),
        DecisionProfileV1::CombatPlayerDecision => {
            Err("combat player decisions do not have a stable noncombat surface".to_owned())
        }
    }
}

fn persistent_noncombat_projections(
    store: &StateRootStoreV2,
    live: &LiveStatus,
    session: &RunControlSession,
    profile: DecisionProfileV1,
) -> Result<Vec<PreparedProjectionV1>, String> {
    let (rng_schema, java_rng, rust_rng) = if profile == DecisionProfileV1::Neow {
        (
            NEOW_RNG_STREAMS_SCHEMA_V1,
            java_neow_rng_streams(&live.frame)?,
            rust_neow_rng_streams(session)?,
        )
    } else {
        (
            "sts-paired-rng-streams-v1",
            java_post_neow_rng_streams(&live.frame)?,
            rust_persistent_rng_streams(session),
        )
    };
    Ok(vec![
        compare_projection(
            store,
            ProjectionDomainV1::RunScalars,
            "sts-paired-run-scalars-v1",
            java_run_scalars(&live.frame)?,
            rust_persistent_run_scalars(session),
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::MasterDeck,
            "sts-paired-master-deck-order-v1",
            java_master_deck_semantics(&live.frame)?,
            rust_master_deck_semantics(session),
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::Relics,
            "sts-paired-relic-order-counter-v1",
            java_relics(&live.frame)?,
            rust_persistent_relics(session)?,
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::Potions,
            PERSISTENT_POTION_SLOTS_SCHEMA_V1,
            java_persistent_potions(&live.frame)?,
            rust_persistent_potions(session),
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::RngStreams,
            rng_schema,
            java_rng,
            rust_rng,
        )?,
        compare_projection(
            store,
            ProjectionDomainV1::CardPools,
            "sts-paired-card-pools-v1",
            java_card_pools(&live.frame)?,
            rust_card_pools(session)?,
        )?,
    ])
}

fn project_stable_noncombat_pair(
    store: &StateRootStoreV2,
    live: &LiveStatus,
    session: &RunControlSession,
    bindings: &[CrossRuntimeCardBindingV1],
    leases: &[JavaCardLeaseV1],
    profile: DecisionProfileV1,
) -> Result<ProjectedPairV1, String> {
    let contract = stable_noncombat_projection_contract(profile)?;
    let surface = stable_noncombat_surface(live, session, profile)?;
    let mut prepared = persistent_noncombat_projections(store, live, session, profile)?;
    let java_actions = surface.java_actions;
    prepared.push(compare_projection(
        store,
        ProjectionDomainV1::ActionSpace,
        contract.action_space_schema,
        java_actions.clone(),
        surface.rust_actions,
    )?);
    let actual_domains = surface
        .profile_projections
        .iter()
        .map(|projection| projection.domain)
        .collect::<BTreeSet<_>>();
    let contract_domains = contract
        .profile_projection_schemas
        .iter()
        .map(|(domain, _)| *domain)
        .collect::<BTreeSet<_>>();
    if actual_domains != contract_domains
        || actual_domains.len() != surface.profile_projections.len()
        || contract_domains.len() != contract.profile_projection_schemas.len()
    {
        return Err("stable noncombat surface does not match its projection contract".to_owned());
    }
    for projection in surface.profile_projections {
        let schema = contract
            .profile_projection_schemas
            .iter()
            .find_map(|(domain, schema)| (*domain == projection.domain).then_some(*schema))
            .ok_or_else(|| {
                "stable noncombat surface contains an undeclared projection domain".to_owned()
            })?;
        prepared.push(compare_projection(
            store,
            projection.domain,
            schema,
            projection.java,
            projection.rust,
        )?);
    }
    prepared.push(relational_binding_projection(
        store,
        bindings,
        leases,
        &live.frame,
        session,
    )?);
    prepared.sort_by_key(|projection| projection.comparison.domain);
    Ok(ProjectedPairV1 {
        prepared,
        java_actions,
    })
}

fn projection_summaries(prepared: &[PreparedProjectionV1]) -> Vec<ProjectionPreparationSummaryV1> {
    prepared
        .iter()
        .map(|projection| ProjectionPreparationSummaryV1 {
            domain: projection.comparison.domain,
            schema: projection.comparison.projection_schema.clone(),
            status: projection.comparison.status,
            first_difference: projection.comparison.first_difference.clone(),
        })
        .collect()
}

fn require_exact_projection_set(
    prepared: &[PreparedProjectionV1],
    label: &str,
    profile: DecisionProfileV1,
) -> Result<(), String> {
    let required = required_projection_domains(profile);
    if let Some(component) = prepared.iter().find(|projection| {
        required.contains(&projection.comparison.domain)
            && projection.comparison.status != ComparisonStatusV1::Exact
    }) {
        return Err(format!(
            "{label} diverged in {:?}: {:?}",
            component.comparison.domain, component.comparison.first_difference
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum StoredActionProjectionV1 {
    PlayCard {
        binding_id: String,
        #[serde(default)]
        target_index: Option<usize>,
        #[serde(default)]
        target_id: Option<String>,
    },
    UsePotion {
        slot: u8,
        potion_id: String,
        potency: i32,
        #[serde(default)]
        target_index: Option<usize>,
        #[serde(default)]
        target_id: Option<String>,
    },
    EndTurn,
    ClaimCombatReward {
        reward: CombatRewardIdentityV1,
        same_reward_ordinal: usize,
    },
    ChooseNeowOption {
        reward: NeowRewardIdentityV1,
        drawback: NeowDrawbackIdentityV1,
    },
    LeaveNeow,
    ChooseCardReward {
        card_id: String,
        upgrades: u8,
        same_card_ordinal: usize,
    },
    SkipCardReward,
    CloseCombatReward,
    ChooseEventOption {
        event_id: String,
        screen: usize,
        option_index: usize,
    },
    ChooseMapNode {
        x: i32,
        y: i32,
        room_type: MapRoomIdentityV1,
    },
}

fn stable_action_observations(
    store: &StateRootStoreV2,
    root: &StateRootArtifactV2,
) -> Result<Vec<PairedRootActionObservationV1>, String> {
    let profile = root_decision_profile(root);
    let mut observations = match profile {
        DecisionProfileV1::CombatPlayerDecision => {
            let action_space_version = action_space_projection_version(root)?;
            let combat_version = combat_projection_version(root)?;
            let action_space = stored_exact_projection(
                store,
                root,
                ProjectionDomainV1::ActionSpace,
                action_space_projection_schema(action_space_version),
            )?;
            let combat = stored_exact_projection(
                store,
                root,
                ProjectionDomainV1::Combat,
                combat_projection_schema(combat_version),
            )?;
            let potions = stored_exact_projection(
                store,
                root,
                ProjectionDomainV1::Potions,
                "sts-paired-potion-slots-v1",
            )?;
            if paired_action_space_id(&action_space)? != root.root.preflight.action_space_id {
                return Err("stored paired action-space identity changed".to_owned());
            }
            build_stable_combat_action_observations(
                &root.root_id,
                &root.root.preflight.action_space_id,
                &root.root.card_bindings,
                action_space,
                &combat,
                &potions,
            )?
        }
        profile @ (DecisionProfileV1::CombatReward
        | DecisionProfileV1::CardReward
        | DecisionProfileV1::Event
        | DecisionProfileV1::MapNavigation
        | DecisionProfileV1::Neow) => {
            let contract = stable_noncombat_projection_contract(profile)?;
            let action_space = stored_exact_projection(
                store,
                root,
                ProjectionDomainV1::ActionSpace,
                contract.action_space_schema,
            )?;
            if paired_action_space_id(&action_space)? != root.root.preflight.action_space_id {
                return Err("stored paired noncombat action-space identity changed".to_owned());
            }
            build_stable_noncombat_action_observations(
                profile,
                &root.root_id,
                &root.root.preflight.action_space_id,
                action_space,
            )?
        }
    };
    if profile == DecisionProfileV1::Event {
        attach_event_public_semantics_v1(store, root, &mut observations)?;
    }
    require_verified_capability(root, CapabilityKindV1::Observe)?;
    for observation in &observations {
        require_declared_action_capability(root, &observation.handle.action)?;
    }
    Ok(observations)
}

fn build_stable_noncombat_action_observations(
    profile: DecisionProfileV1,
    root_id: &str,
    action_space_id: &str,
    action_space: Value,
) -> Result<Vec<PairedRootActionObservationV1>, String> {
    match profile {
        DecisionProfileV1::CombatReward => {
            build_stable_reward_action_observations(root_id, action_space_id, action_space)
        }
        DecisionProfileV1::CardReward => {
            build_stable_card_reward_action_observations(root_id, action_space_id, action_space)
        }
        DecisionProfileV1::Event => {
            build_stable_event_action_observations(root_id, action_space_id, action_space)
        }
        DecisionProfileV1::MapNavigation => {
            build_stable_map_action_observations(root_id, action_space_id, action_space)
        }
        DecisionProfileV1::Neow => {
            build_stable_neow_action_observations(root_id, action_space_id, action_space)
        }
        DecisionProfileV1::CombatPlayerDecision => {
            Err("combat player decisions do not use noncombat action observations".to_owned())
        }
    }
}

enum SearchIntentStepV1 {
    Intent(PairedRootSearchActionIntentV1),
    MechanicsGap(String),
}

fn translate_search_input_line_v1(
    root: &CombatPosition,
    combat_sequence: u64,
    constructed_scenario_instance_id: Option<&str>,
    inputs: &[ClientInput],
    max_engine_steps: usize,
) -> Result<PairedRootSearchIntentTranslationV1, String> {
    if inputs.is_empty() {
        return Err("paired-root search materialization contains an empty line".to_owned());
    }
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut intents = Vec::with_capacity(inputs.len());
    let mut first_gap = None;
    for (step_ordinal, input) in inputs.iter().enumerate() {
        if stepper.choice_for_legal_input(&position, input).is_none() {
            return Err(format!(
                "paired-root search materialization is illegal at step {step_ordinal}: {}",
                input.kind_v1(),
            ));
        }
        match search_intent_for_input_v1(
            &position,
            combat_sequence,
            constructed_scenario_instance_id,
            input,
        )? {
            SearchIntentStepV1::Intent(intent) if first_gap.is_none() => intents.push(intent),
            SearchIntentStepV1::Intent(_) => {}
            SearchIntentStepV1::MechanicsGap(reason) if first_gap.is_none() => {
                first_gap = Some((step_ordinal, input.kind_v1().to_owned(), reason));
                intents.clear();
            }
            SearchIntentStepV1::MechanicsGap(_) => {}
        }
        let result = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps,
                deadline: None,
            },
        );
        if result.truncated || result.timed_out {
            return Err(format!(
                "paired-root search materialization did not reach a stable boundary at step {step_ordinal}"
            ));
        }
        if step_ordinal + 1 < inputs.len()
            && result.terminal != sts_oracle_learning::sim::combat::CombatTerminal::Unresolved
        {
            return Err(format!(
                "paired-root search materialization continued after terminal step {step_ordinal}"
            ));
        }
        position = result.position;
    }
    Ok(match first_gap {
        Some((step_ordinal, action_kind, reason)) => {
            PairedRootSearchIntentTranslationV1::MechanicsGap {
                step_ordinal,
                action_kind,
                reason,
            }
        }
        None => PairedRootSearchIntentTranslationV1::Complete { intents },
    })
}

fn search_intent_for_input_v1(
    position: &CombatPosition,
    combat_sequence: u64,
    constructed_scenario_instance_id: Option<&str>,
    input: &ClientInput,
) -> Result<SearchIntentStepV1, String> {
    match input {
        ClientInput::PlayCard { card_index, target } => {
            let card =
                position.combat.zones.hand.get(*card_index).ok_or_else(|| {
                    format!("search play-card hand index {card_index} is invalid")
                })?;
            let rust_lineage = match card.lineage.origin() {
                CardLineageOrigin::MasterDeck => RustCardLineageV1::MasterDeck {
                    acquisition_ordinal: u64::from(card.lineage.ordinal()),
                },
                CardLineageOrigin::CombatGenerated => RustCardLineageV1::CombatGenerated {
                    combat_sequence,
                    generation_ordinal: u64::from(card.lineage.ordinal()),
                },
                CardLineageOrigin::ConstructedScenario => {
                    RustCardLineageV1::ConstructedScenarioInstance {
                    scenario_instance_id: constructed_scenario_instance_id
                        .ok_or_else(|| {
                            "constructed-scenario lineage has no root-owned scenario instance identity"
                                .to_owned()
                        })?
                        .to_owned(),
                    slot: u64::from(card.lineage.ordinal()),
                }
                },
            };
            Ok(SearchIntentStepV1::Intent(
                PairedRootSearchActionIntentV1::PlayCard {
                    rust_lineage,
                    target: rust_monster_target_v1(position, *target)?,
                },
            ))
        }
        ClientInput::EndTurn => Ok(SearchIntentStepV1::Intent(
            PairedRootSearchActionIntentV1::EndTurn,
        )),
        ClientInput::UsePotion {
            potion_index,
            target,
        } => {
            let potion = position
                .combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .ok_or_else(|| format!("search use-potion slot {potion_index} is absent"))?;
            Ok(SearchIntentStepV1::Intent(
                PairedRootSearchActionIntentV1::UsePotion {
                    slot: u8::try_from(*potion_index)
                        .map_err(|_| "search potion slot exceeds u8".to_owned())?,
                    potion_id: canonical_potion_id(potion.id.stable_id()),
                    potency: rust_potion_potency(&position.combat, potion.id),
                    target: rust_monster_target_v1(position, *target)?,
                },
            ))
        }
        other => Ok(SearchIntentStepV1::MechanicsGap(format!(
            "PairedRoot witness preview does not yet own {}",
            other.kind_v1(),
        ))),
    }
}

fn rust_monster_target_v1(
    position: &CombatPosition,
    target: Option<sts_oracle_learning::EntityId>,
) -> Result<Option<MonsterTargetV1>, String> {
    let Some(target) = target else {
        return Ok(None);
    };
    let index = position
        .combat
        .entities
        .monsters
        .iter()
        .position(|monster| monster.id == target)
        .ok_or_else(|| format!("search target entity {target} is absent"))?;
    let monster = &position.combat.entities.monsters[index];
    let enemy_id = canonical_game_id(monster.enemy_id.stable_id());
    let same_enemy_ordinal = position.combat.entities.monsters[..index]
        .iter()
        .filter(|prior| canonical_game_id(prior.enemy_id.stable_id()) == enemy_id)
        .count();
    Ok(Some(MonsterTargetV1 {
        enemy_id,
        same_enemy_ordinal,
    }))
}

fn paired_search_content_id(prefix: &str, value: &impl Serialize) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode {prefix} identity: {error}"))?;
    Ok(format!("{prefix}-{}", hex_lower(&Sha256::digest(bytes))))
}

fn content_id_has_shape(value: &str, prefix: &str) -> bool {
    let expected_prefix = format!("{prefix}-");
    value.strip_prefix(&expected_prefix).is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn resolve_search_intent_observation_v1(
    root: &StateRootArtifactV2,
    observations: &[PairedRootActionObservationV1],
    intent: &PairedRootSearchActionIntentV1,
) -> Result<(Option<String>, Option<String>), String> {
    let action = match intent {
        PairedRootSearchActionIntentV1::PlayCard {
            rust_lineage,
            target,
        } => {
            let matches = root
                .root
                .card_bindings
                .iter()
                .filter(|binding| binding.rust_lineage == *rust_lineage)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Ok((
                    None,
                    Some(format!(
                        "intent lineage resolves to {} root bindings instead of one",
                        matches.len()
                    )),
                ));
            }
            PairedActionV1::PlayCard {
                binding_id: matches[0].binding_id.clone(),
                target: target.clone(),
            }
        }
        PairedRootSearchActionIntentV1::EndTurn => PairedActionV1::EndTurn,
        PairedRootSearchActionIntentV1::UsePotion {
            slot,
            potion_id,
            potency,
            target,
        } => PairedActionV1::UsePotion {
            slot: *slot,
            potion_id: potion_id.clone(),
            potency: *potency,
            target: target.clone(),
        },
    };
    let matches = observations
        .iter()
        .filter(|observation| observation.handle.action == action)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [observation] => Ok((Some(observation.handle.handle_id.clone()), None)),
        [] => Ok((
            None,
            Some("typed intent is not legal in the current root action space".to_owned()),
        )),
        _ => Err(format!(
            "typed search intent resolves to {} stable actions instead of one",
            matches.len()
        )),
    }
}

fn first_search_action_handle_v1(
    root: &StateRootArtifactV2,
    translation: &PairedRootSearchIntentTranslationV1,
) -> Result<Option<String>, String> {
    let PairedRootSearchIntentTranslationV1::Complete { intents } = translation else {
        return Ok(None);
    };
    let first = intents
        .first()
        .ok_or_else(|| "complete paired-root search translation is empty".to_owned())?;
    let storeless_action = match first {
        PairedRootSearchActionIntentV1::PlayCard {
            rust_lineage,
            target,
        } => {
            let matches = root
                .root
                .card_bindings
                .iter()
                .filter(|binding| binding.rust_lineage == *rust_lineage)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(format!(
                    "search first-step lineage resolves to {} root bindings instead of one",
                    matches.len()
                ));
            }
            PairedActionV1::PlayCard {
                binding_id: matches[0].binding_id.clone(),
                target: target.clone(),
            }
        }
        PairedRootSearchActionIntentV1::EndTurn => PairedActionV1::EndTurn,
        PairedRootSearchActionIntentV1::UsePotion {
            slot,
            potion_id,
            potency,
            target,
        } => PairedActionV1::UsePotion {
            slot: *slot,
            potion_id: potion_id.clone(),
            potency: *potency,
            target: target.clone(),
        },
    };
    Ok(Some(
        action_handle(
            &root.root_id,
            &root.root.preflight.action_space_id,
            storeless_action,
        )?
        .handle_id,
    ))
}

fn build_stable_combat_action_observations(
    root_id: &str,
    action_space_id: &str,
    root_bindings: &[CrossRuntimeCardBindingV1],
    action_space: Value,
    combat: &Value,
    potions: &Value,
) -> Result<Vec<PairedRootActionObservationV1>, String> {
    let stored_actions: Vec<StoredActionProjectionV1> = serde_json::from_value(action_space)
        .map_err(|error| format!("failed to decode stored paired action space: {error}"))?;
    let monsters = combat["monsters"]
        .as_array()
        .ok_or_else(|| "stored paired combat projection omitted monsters".to_owned())?;
    let hand = combat["zones"]["hand"]
        .as_array()
        .ok_or_else(|| "stored paired combat projection omitted the hand".to_owned())?;
    let potion_slots = potions
        .as_array()
        .ok_or_else(|| "stored paired potion projection is not an array".to_owned())?;
    let bindings = root_bindings
        .iter()
        .map(|binding| (binding.binding_id.as_str(), binding))
        .collect::<BTreeMap<_, _>>();
    let mut observations = Vec::with_capacity(stored_actions.len());
    for stored in stored_actions {
        let (action, card, potion) = match stored {
            StoredActionProjectionV1::PlayCard {
                binding_id,
                target_index,
                target_id,
            } => {
                let binding = bindings.get(binding_id.as_str()).ok_or_else(|| {
                    format!("stored action references unknown binding {binding_id}")
                })?;
                let target = match (target_index, target_id) {
                    (None, None) => None,
                    (Some(index), Some(expected_id)) => {
                        let monster = monsters.get(index).ok_or_else(|| {
                            format!("stored action target index {index} is outside combat")
                        })?;
                        let enemy_id = required_str(monster, "id")?;
                        if enemy_id != expected_id {
                            return Err(format!(
                                "stored action target identity changed at index {index}: action={expected_id:?} combat={enemy_id:?}"
                            ));
                        }
                        let same_enemy_ordinal = monsters[..index]
                            .iter()
                            .filter(|monster| monster["id"].as_str() == Some(enemy_id))
                            .count();
                        Some(MonsterTargetV1 {
                            enemy_id: enemy_id.to_owned(),
                            same_enemy_ordinal,
                        })
                    }
                    _ => {
                        return Err(
                            "stored action target index and identity have different presence"
                                .to_owned(),
                        );
                    }
                };
                let matching_cards = hand
                    .iter()
                    .filter(|card| card["binding_id"].as_str() == Some(binding_id.as_str()))
                    .collect::<Vec<_>>();
                if matching_cards.len() != 1 {
                    return Err(format!(
                        "stored action binding {binding_id} resolves to {} hand cards instead of one",
                        matching_cards.len()
                    ));
                }
                let stored_card = matching_cards[0];
                let stored_card_id = required_str(stored_card, "card_id")?;
                let stored_upgrades = u8::try_from(required_u64(stored_card, "upgrades")?)
                    .map_err(|_| "stored paired card upgrades exceed u8".to_owned())?;
                if stored_card_id != binding.card_id || stored_upgrades != binding.upgrades {
                    return Err(format!(
                        "stored action card disagrees with binding {binding_id}"
                    ));
                }
                (
                    PairedActionV1::PlayCard { binding_id, target },
                    Some(PairedRootObservedCardV1 {
                        card_id: stored_card_id.to_owned(),
                        upgrades: stored_upgrades,
                        cost_for_turn: required_i64(stored_card, "cost_for_turn")?,
                    }),
                    None,
                )
            }
            StoredActionProjectionV1::UsePotion {
                slot,
                potion_id,
                potency,
                target_index,
                target_id,
            } => {
                if potency < 0 {
                    return Err("stored potion action has negative potency".to_owned());
                }
                let target = match (target_index, target_id) {
                    (None, None) => None,
                    (Some(index), Some(expected_id)) => {
                        let monster = monsters.get(index).ok_or_else(|| {
                            format!("stored potion target index {index} is outside combat")
                        })?;
                        let enemy_id = required_str(monster, "id")?;
                        if enemy_id != expected_id {
                            return Err(format!(
                                "stored potion target identity changed at index {index}: action={expected_id:?} combat={enemy_id:?}"
                            ));
                        }
                        let same_enemy_ordinal = monsters[..index]
                            .iter()
                            .filter(|monster| monster["id"].as_str() == Some(enemy_id))
                            .count();
                        Some(MonsterTargetV1 {
                            enemy_id: enemy_id.to_owned(),
                            same_enemy_ordinal,
                        })
                    }
                    _ => {
                        return Err(
                            "stored potion target index and identity have different presence"
                                .to_owned(),
                        );
                    }
                };
                let matching_potions = potion_slots
                    .iter()
                    .filter(|potion| potion["slot"].as_u64() == Some(u64::from(slot)))
                    .collect::<Vec<_>>();
                if matching_potions.len() != 1 {
                    return Err(format!(
                        "stored potion action slot {slot} resolves to {} potions instead of one",
                        matching_potions.len()
                    ));
                }
                let stored_potion = matching_potions[0];
                if required_str(stored_potion, "id")? != potion_id
                    || !required_bool(stored_potion, "can_use")?
                {
                    return Err(format!(
                        "stored potion action disagrees with potion slot {slot}"
                    ));
                }
                (
                    PairedActionV1::UsePotion {
                        slot,
                        potion_id: potion_id.clone(),
                        potency,
                        target,
                    },
                    None,
                    Some(PairedRootObservedPotionV1 { potion_id, potency }),
                )
            }
            StoredActionProjectionV1::EndTurn => (PairedActionV1::EndTurn, None, None),
            StoredActionProjectionV1::ClaimCombatReward { .. }
            | StoredActionProjectionV1::ChooseNeowOption { .. }
            | StoredActionProjectionV1::LeaveNeow
            | StoredActionProjectionV1::ChooseCardReward { .. }
            | StoredActionProjectionV1::SkipCardReward
            | StoredActionProjectionV1::CloseCombatReward
            | StoredActionProjectionV1::ChooseEventOption { .. }
            | StoredActionProjectionV1::ChooseMapNode { .. } => {
                return Err("combat action space contains a reward action".to_owned());
            }
        };
        observations.push(PairedRootActionObservationV1 {
            handle: action_handle(root_id, action_space_id, action)?,
            card,
            potion,
            combat_reward: None,
            card_reward: None,
            event_option: None,
        });
    }
    observations.sort_by(|left, right| left.handle.handle_id.cmp(&right.handle.handle_id));
    if observations.is_empty() {
        return Err("stored paired action space is empty".to_owned());
    }
    if observations
        .windows(2)
        .any(|pair| pair[0].handle.handle_id == pair[1].handle.handle_id)
    {
        return Err("stored action space produced duplicate paired handles".to_owned());
    }
    Ok(observations)
}

fn build_stable_reward_action_observations(
    root_id: &str,
    action_space_id: &str,
    action_space: Value,
) -> Result<Vec<PairedRootActionObservationV1>, String> {
    let stored_actions: Vec<StoredActionProjectionV1> = serde_json::from_value(action_space)
        .map_err(|error| format!("failed to decode stored paired reward action space: {error}"))?;
    let mut observations = Vec::with_capacity(stored_actions.len());
    for stored in stored_actions {
        let (action, combat_reward) = match stored {
            StoredActionProjectionV1::ClaimCombatReward {
                reward,
                same_reward_ordinal,
            } => (
                PairedActionV1::ClaimCombatReward {
                    reward: reward.clone(),
                    same_reward_ordinal,
                },
                Some(reward),
            ),
            StoredActionProjectionV1::CloseCombatReward => {
                (PairedActionV1::CloseCombatReward, None)
            }
            _ => return Err("reward action space contains another profile action".to_owned()),
        };
        observations.push(PairedRootActionObservationV1 {
            handle: action_handle(root_id, action_space_id, action)?,
            card: None,
            potion: None,
            combat_reward,
            card_reward: None,
            event_option: None,
        });
    }
    observations.sort_by(|left, right| left.handle.handle_id.cmp(&right.handle.handle_id));
    if observations
        .windows(2)
        .any(|pair| pair[0].handle.handle_id == pair[1].handle.handle_id)
    {
        return Err("stored reward action space produced duplicate paired handles".to_owned());
    }
    Ok(observations)
}

fn build_stable_neow_action_observations(
    root_id: &str,
    action_space_id: &str,
    action_space: Value,
) -> Result<Vec<PairedRootActionObservationV1>, String> {
    let stored_actions: Vec<StoredActionProjectionV1> = serde_json::from_value(action_space)
        .map_err(|error| format!("failed to decode stored Neow action space: {error}"))?;
    let mut observations = Vec::with_capacity(stored_actions.len());
    for stored in stored_actions {
        let action = match stored {
            StoredActionProjectionV1::ChooseNeowOption { reward, drawback } => {
                PairedActionV1::ChooseNeowOption { reward, drawback }
            }
            StoredActionProjectionV1::LeaveNeow => PairedActionV1::LeaveNeow,
            _ => return Err("Neow action space contains another profile action".to_owned()),
        };
        observations.push(PairedRootActionObservationV1 {
            handle: action_handle(root_id, action_space_id, action)?,
            card: None,
            potion: None,
            combat_reward: None,
            card_reward: None,
            event_option: None,
        });
    }
    observations.sort_by(|left, right| left.handle.handle_id.cmp(&right.handle.handle_id));
    if observations.is_empty() {
        return Err("stored Neow action space is empty".to_owned());
    }
    if observations
        .windows(2)
        .any(|pair| pair[0].handle.handle_id == pair[1].handle.handle_id)
    {
        return Err("stored Neow action space produced duplicate handles".to_owned());
    }
    Ok(observations)
}

fn build_stable_event_action_observations(
    root_id: &str,
    action_space_id: &str,
    action_space: Value,
) -> Result<Vec<PairedRootActionObservationV1>, String> {
    let stored_actions: Vec<StoredActionProjectionV1> = serde_json::from_value(action_space)
        .map_err(|error| format!("failed to decode stored event action space: {error}"))?;
    let mut observations = Vec::with_capacity(stored_actions.len());
    for stored in stored_actions {
        let StoredActionProjectionV1::ChooseEventOption {
            event_id,
            screen,
            option_index,
        } = stored
        else {
            return Err("event action space contains another profile action".to_owned());
        };
        observations.push(PairedRootActionObservationV1 {
            handle: action_handle(
                root_id,
                action_space_id,
                PairedActionV1::ChooseEventOption {
                    event_id,
                    screen,
                    option_index,
                },
            )?,
            card: None,
            potion: None,
            combat_reward: None,
            card_reward: None,
            event_option: None,
        });
    }
    observations.sort_by(|left, right| left.handle.handle_id.cmp(&right.handle.handle_id));
    if observations.is_empty() {
        return Err("stored event action space is empty".to_owned());
    }
    if observations
        .windows(2)
        .any(|pair| pair[0].handle.handle_id == pair[1].handle.handle_id)
    {
        return Err("stored event action space produced duplicate handles".to_owned());
    }
    Ok(observations)
}

fn attach_event_public_semantics_v1(
    store: &StateRootStoreV2,
    root: &StateRootArtifactV2,
    observations: &mut [PairedRootActionObservationV1],
) -> Result<(), String> {
    require_current_rust_semantics(root)?;
    let session = exact_rust_session_for_public_semantics_v1(store, root)?;
    let stored_actions = stored_exact_projection(
        store,
        root,
        ProjectionDomainV1::ActionSpace,
        EVENT_ACTION_SPACE_SCHEMA_V1,
    )?;
    let regenerated_actions = rust_event_action_space(&session)?;
    if regenerated_actions != stored_actions
        || paired_action_space_id(&regenerated_actions)? != root.root.preflight.action_space_id
    {
        return Err("Rust checkpoint no longer reproduces the Exact event action space".to_owned());
    }

    let (event_id, screen) = rust_event_identity(&session)?;
    for observation in observations {
        let PairedActionV1::ChooseEventOption {
            event_id: observed_event_id,
            screen: observed_screen,
            option_index,
        } = &observation.handle.action
        else {
            return Err("Event observation contains an action from another profile".to_owned());
        };
        if observed_event_id != event_id.stable_id() || *observed_screen != screen {
            return Err(
                "Event observation disagrees with its exact Rust checkpoint identity".to_owned(),
            );
        }
        observation.event_option = Some(observed_event_option_semantics_v1(
            &session,
            event_id,
            screen,
            *option_index,
        )?);
    }
    Ok(())
}

fn observed_event_option_semantics_v1(
    session: &RunControlSession,
    event_id: EventId,
    screen: usize,
    option_index: usize,
) -> Result<PairedRootObservedEventOptionV1, String> {
    if rust_event_identity(session)? != (event_id, screen) {
        return Err("Event semantic request disagrees with its Rust decision identity".to_owned());
    }
    let options = sts_oracle_learning::engine::event_handler::get_event_options(&session.run_state);
    let option = options
        .get(option_index)
        .ok_or_else(|| "Event observation points outside the public option surface".to_owned())?;
    let surface = build_decision_surface(session);
    let matching = surface
        .view
        .candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.key.as_ref(),
                Some(DecisionCandidateKey::EventOption {
                    event_id: candidate_event_id,
                    screen: candidate_screen,
                    option_index: candidate_option_index,
                    action,
                }) if *candidate_event_id == event_id
                    && *candidate_screen == screen
                    && *candidate_option_index == option_index
                    && *action == option.semantics.action
            ) && candidate.action.executable_action_ref().is_some()
        })
        .collect::<Vec<_>>();
    let [candidate] = matching.as_slice() else {
        return Err(format!(
            "Event option {option_index} resolves to {} executable semantic candidates instead of one",
            matching.len()
        ));
    };
    Ok(PairedRootObservedEventOptionV1 {
        action_kind: observed_event_action_kind_v1(option.semantics.action),
        effect_summary: candidate
            .resolution
            .as_ref()
            .and_then(|resolution| resolution.effect_note()),
        followup: observed_event_followup_v1(option.semantics.transition),
        repeatable: option.semantics.repeatable,
    })
}

fn exact_rust_session_for_public_semantics_v1(
    store: &StateRootStoreV2,
    root: &StateRootArtifactV2,
) -> Result<RunControlSession, String> {
    if root.root.rust_snapshot.entrypoint != "run_control_exact_checkpoint" {
        return Err("Event public semantics require an exact native Rust checkpoint".to_owned());
    }
    let checkpoint_bytes =
        store.read_bundle_file(&root.root.rust_snapshot.bundle, "checkpoint.msgpack")?;
    let checkpoint: RunControlSessionCheckpointV1 = rmp_serde::from_slice(&checkpoint_bytes)
        .map_err(|error| format!("failed to decode Event public-semantics checkpoint: {error}"))?;
    checkpoint.into_session()
}

fn observed_event_action_kind_v1(action: EventActionKind) -> PairedRootObservedEventActionKindV1 {
    match action {
        EventActionKind::Unknown => PairedRootObservedEventActionKindV1::Unknown,
        EventActionKind::Leave => PairedRootObservedEventActionKindV1::Leave,
        EventActionKind::Continue => PairedRootObservedEventActionKindV1::Continue,
        EventActionKind::Accept => PairedRootObservedEventActionKindV1::Accept,
        EventActionKind::Decline => PairedRootObservedEventActionKindV1::Decline,
        EventActionKind::Fight => PairedRootObservedEventActionKindV1::Fight,
        EventActionKind::Trade => PairedRootObservedEventActionKindV1::Trade,
        EventActionKind::DeckOperation => PairedRootObservedEventActionKindV1::DeckOperation,
        EventActionKind::Gain => PairedRootObservedEventActionKindV1::Gain,
        EventActionKind::Special => PairedRootObservedEventActionKindV1::Special,
    }
}

fn observed_event_followup_v1(
    transition: EventOptionTransition,
) -> PairedRootObservedEventFollowupV1 {
    match transition {
        EventOptionTransition::None => PairedRootObservedEventFollowupV1::None,
        EventOptionTransition::AdvanceScreen => PairedRootObservedEventFollowupV1::EventScreen,
        EventOptionTransition::Complete => PairedRootObservedEventFollowupV1::EventComplete,
        EventOptionTransition::OpenSelection(selection) => match selection {
            EventSelectionKind::None => PairedRootObservedEventFollowupV1::UnknownSelection,
            EventSelectionKind::RemoveCard => {
                PairedRootObservedEventFollowupV1::RemoveCardSelection
            }
            EventSelectionKind::UpgradeCard => {
                PairedRootObservedEventFollowupV1::UpgradeCardSelection
            }
            EventSelectionKind::TransformCard => {
                PairedRootObservedEventFollowupV1::TransformCardSelection
            }
            EventSelectionKind::DuplicateCard => {
                PairedRootObservedEventFollowupV1::DuplicateCardSelection
            }
            EventSelectionKind::OfferCard => PairedRootObservedEventFollowupV1::OfferCardSelection,
        },
        EventOptionTransition::OpenReward => PairedRootObservedEventFollowupV1::Reward,
        EventOptionTransition::StartCombat => PairedRootObservedEventFollowupV1::Combat,
    }
}

fn build_stable_map_action_observations(
    root_id: &str,
    action_space_id: &str,
    action_space: Value,
) -> Result<Vec<PairedRootActionObservationV1>, String> {
    let stored_actions: Vec<StoredActionProjectionV1> = serde_json::from_value(action_space)
        .map_err(|error| format!("failed to decode stored map action space: {error}"))?;
    let mut observations = Vec::with_capacity(stored_actions.len());
    for stored in stored_actions {
        let StoredActionProjectionV1::ChooseMapNode { x, y, room_type } = stored else {
            return Err("map action space contains another profile action".to_owned());
        };
        observations.push(PairedRootActionObservationV1 {
            handle: action_handle(
                root_id,
                action_space_id,
                PairedActionV1::ChooseMapNode { x, y, room_type },
            )?,
            card: None,
            potion: None,
            combat_reward: None,
            card_reward: None,
            event_option: None,
        });
    }
    observations.sort_by(|left, right| left.handle.handle_id.cmp(&right.handle.handle_id));
    if observations
        .windows(2)
        .any(|pair| pair[0].handle.handle_id == pair[1].handle.handle_id)
    {
        return Err("stored map action space produced duplicate paired handles".to_owned());
    }
    Ok(observations)
}

fn build_stable_card_reward_action_observations(
    root_id: &str,
    action_space_id: &str,
    action_space: Value,
) -> Result<Vec<PairedRootActionObservationV1>, String> {
    let stored_actions: Vec<StoredActionProjectionV1> = serde_json::from_value(action_space)
        .map_err(|error| format!("failed to decode stored card-reward action space: {error}"))?;
    let mut observations = Vec::with_capacity(stored_actions.len());
    for stored in stored_actions {
        let (action, card_reward) = match stored {
            StoredActionProjectionV1::ChooseCardReward {
                card_id,
                upgrades,
                same_card_ordinal,
            } => (
                PairedActionV1::ChooseCardReward {
                    card_id: card_id.clone(),
                    upgrades,
                    same_card_ordinal,
                },
                PairedRootObservedCardRewardV1::Card { card_id, upgrades },
            ),
            StoredActionProjectionV1::SkipCardReward => (
                PairedActionV1::SkipCardReward,
                PairedRootObservedCardRewardV1::Skip,
            ),
            _ => return Err("card-reward action space contains another profile action".to_owned()),
        };
        observations.push(PairedRootActionObservationV1 {
            handle: action_handle(root_id, action_space_id, action)?,
            card: None,
            potion: None,
            combat_reward: None,
            card_reward: Some(card_reward),
            event_option: None,
        });
    }
    observations.sort_by(|left, right| left.handle.handle_id.cmp(&right.handle.handle_id));
    if observations
        .windows(2)
        .any(|pair| pair[0].handle.handle_id == pair[1].handle.handle_id)
    {
        return Err("stored card-reward action space produced duplicate handles".to_owned());
    }
    Ok(observations)
}

fn stored_exact_projection(
    store: &StateRootStoreV2,
    root: &StateRootArtifactV2,
    domain: ProjectionDomainV1,
    schema: &str,
) -> Result<Value, String> {
    let projection = root
        .root
        .preflight
        .components
        .iter()
        .find(|projection| projection.domain == domain)
        .ok_or_else(|| format!("paired root omitted {domain:?} projection"))?;
    if projection.status != ComparisonStatusV1::Exact || projection.projection_schema != schema {
        return Err(format!(
            "paired root {domain:?} projection is not exact {schema}"
        ));
    }
    let java: Value = serde_json::from_slice(&store.read_blob(&projection.java)?)
        .map_err(|error| format!("failed to decode stored Java {domain:?} projection: {error}"))?;
    let rust: Value = serde_json::from_slice(&store.read_blob(&projection.rust)?)
        .map_err(|error| format!("failed to decode stored Rust {domain:?} projection: {error}"))?;
    if java != rust {
        return Err(format!(
            "stored exact {domain:?} projection has different Java and Rust values"
        ));
    }
    Ok(java)
}

fn paired_action_handles(
    root: &StateRootArtifactV2,
    activation: &StateRootActivationArtifactV2,
    live: &LiveStatus,
) -> Result<Vec<PairedActionHandleV1>, String> {
    let profile = root_decision_profile(root);
    if matches!(
        profile,
        DecisionProfileV1::CombatReward
            | DecisionProfileV1::CardReward
            | DecisionProfileV1::Event
            | DecisionProfileV1::MapNavigation
            | DecisionProfileV1::Neow
    ) {
        let java_actions = match profile {
            DecisionProfileV1::CombatReward => java_reward_action_space(live)?,
            DecisionProfileV1::CardReward => java_card_reward_action_space(live)?,
            DecisionProfileV1::Event => java_event_action_space(live)?,
            DecisionProfileV1::MapNavigation => project_java_map_action_space(&live.frame)?,
            DecisionProfileV1::Neow => project_java_neow_action_space(&live.frame)?,
            DecisionProfileV1::CombatPlayerDecision => unreachable!("profile checked above"),
        };
        return Ok(build_stable_noncombat_action_observations(
            profile,
            &root.root_id,
            &root.root.preflight.action_space_id,
            java_actions,
        )?
        .into_iter()
        .map(|observation| observation.handle)
        .collect());
    }

    let action_space_version = action_space_projection_version(root)?;
    let bindings = activation
        .activation
        .card_leases
        .iter()
        .map(|lease| (lease.java_card_uuid.as_str(), lease.binding_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut handles = Vec::new();
    for candidate in &live.actions {
        let action = match candidate.kind.as_str() {
            "play_card" => {
                let uuid = candidate
                    .card_uuid
                    .as_deref()
                    .ok_or_else(|| "Java play-card action omitted UUID".to_owned())?;
                let binding_id = bindings
                    .get(uuid)
                    .copied()
                    .ok_or_else(|| format!("Java action UUID {uuid} is not root-bound"))?;
                PairedActionV1::PlayCard {
                    binding_id: binding_id.to_owned(),
                    target: java_monster_target(&live.frame, candidate.target_monster_index)?,
                }
            }
            "use_potion" if action_space_version == ActionSpaceProjectionVersion::PotionV2 => {
                let slot = u8::try_from(
                    candidate
                        .potion_slot
                        .ok_or_else(|| "Java potion action omitted its slot".to_owned())?,
                )
                .map_err(|_| "Java potion slot exceeds u8".to_owned())?;
                let potion_id = canonical_potion_id(
                    candidate
                        .potion_id
                        .as_deref()
                        .ok_or_else(|| "Java potion action omitted its identity".to_owned())?,
                );
                let potency = candidate
                    .potion_potency
                    .ok_or_else(|| "Java potion action omitted its potency".to_owned())?;
                PairedActionV1::UsePotion {
                    slot,
                    potion_id,
                    potency,
                    target: java_monster_target(&live.frame, candidate.target_monster_index)?,
                }
            }
            "end_turn" => PairedActionV1::EndTurn,
            _ => continue,
        };
        handles.push(action_handle(
            &root.root_id,
            &root.root.preflight.action_space_id,
            action,
        )?);
    }
    handles.sort_by(|left, right| left.handle_id.cmp(&right.handle_id));
    if handles
        .windows(2)
        .any(|pair| pair[0].handle_id == pair[1].handle_id)
    {
        return Err("Java action space produced duplicate paired action handles".to_owned());
    }
    Ok(handles)
}

fn java_supported_reward_identity(
    candidate: &ActionHandle,
    has_empty_potion_slot: bool,
) -> Result<Option<CombatRewardIdentityV1>, String> {
    if candidate.kind != "claim_combat_reward" {
        return Ok(None);
    }
    let reward = match candidate.reward_type.as_deref() {
        Some("GOLD") => CombatRewardIdentityV1::Gold {
            amount: i32::try_from(
                candidate
                    .reward_gold
                    .ok_or_else(|| "Java gold reward action omitted its amount".to_owned())?,
            )
            .map_err(|_| "Java gold reward amount exceeds i32".to_owned())?,
        },
        Some("STOLEN_GOLD") => {
            CombatRewardIdentityV1::StolenGold {
                amount: i32::try_from(candidate.reward_gold.ok_or_else(|| {
                    "Java stolen-gold reward action omitted its amount".to_owned()
                })?)
                .map_err(|_| "Java stolen-gold reward amount exceeds i32".to_owned())?,
            }
        }
        Some("CARD") => CombatRewardIdentityV1::Card {
            choice_count: candidate
                .reward_id
                .as_deref()
                .map(|ids| ids.split(',').filter(|id| !id.is_empty()).count())
                .ok_or_else(|| {
                    "Java card reward action omitted its opaque choice identities".to_owned()
                })?,
        },
        Some("POTION") if has_empty_potion_slot => CombatRewardIdentityV1::Potion {
            potion_id: canonical_potion_id(
                candidate
                    .reward_id
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "Java potion reward action omitted its identity".to_owned())?,
            ),
        },
        _ => return Ok(None),
    };
    Ok(Some(reward))
}

fn java_supported_reward_occurrences(
    live: &LiveStatus,
) -> Result<Vec<(&ActionHandle, CombatRewardIdentityV1, usize)>, String> {
    let has_empty_potion_slot = java_has_empty_potion_slot(live)?;
    let mut indexed = Vec::new();
    for candidate in &live.actions {
        let Some(reward) = java_supported_reward_identity(candidate, has_empty_potion_slot)? else {
            continue;
        };
        indexed.push((
            candidate
                .reward_index
                .ok_or_else(|| "Java supported reward action omitted its index".to_owned())?,
            candidate,
            reward,
        ));
    }
    indexed.sort_by_key(|(index, _, _)| *index);
    if indexed.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err("Java reward action space repeats a reward index".to_owned());
    }
    let mut occurrences = BTreeMap::<CombatRewardIdentityV1, usize>::new();
    Ok(indexed
        .into_iter()
        .map(|(_, candidate, reward)| {
            let same_reward_ordinal = *occurrences.get(&reward).unwrap_or(&0);
            occurrences.insert(reward.clone(), same_reward_ordinal + 1);
            (candidate, reward, same_reward_ordinal)
        })
        .collect())
}

fn java_card_reward_occurrences(
    live: &LiveStatus,
) -> Result<Vec<(&ActionHandle, String, u8, usize)>, String> {
    let mut indexed = live
        .actions
        .iter()
        .filter(|candidate| candidate.kind == "choose_card_reward")
        .map(|candidate| {
            let index = candidate
                .reward_index
                .ok_or_else(|| "Java card choice action omitted reward_index".to_owned())?;
            let card_id = candidate
                .card_id
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Java card choice action omitted card_id".to_owned())?;
            let upgrades = u8::try_from(
                candidate
                    .card_upgrades
                    .ok_or_else(|| "Java card choice action omitted upgrades".to_owned())?,
            )
            .map_err(|_| "Java card choice upgrades exceed u8".to_owned())?;
            Ok((index, candidate, card_id.to_owned(), upgrades))
        })
        .collect::<Result<Vec<_>, String>>()?;
    indexed.sort_by_key(|(index, _, _, _)| *index);
    if indexed
        .iter()
        .enumerate()
        .any(|(expected, (actual, _, _, _))| expected != *actual)
    {
        return Err("Java card choice actions are not a contiguous ordered list".to_owned());
    }
    let mut occurrences = BTreeMap::<(String, u8), usize>::new();
    Ok(indexed
        .into_iter()
        .map(|(_, candidate, card_id, upgrades)| {
            let key = (card_id.clone(), upgrades);
            let same_card_ordinal = *occurrences.get(&key).unwrap_or(&0);
            occurrences.insert(key, same_card_ordinal + 1);
            (candidate, card_id, upgrades, same_card_ordinal)
        })
        .collect())
}

fn select_action_handle(
    handles: &[PairedActionHandleV1],
    handle_id: &str,
) -> Result<PairedActionHandleV1, String> {
    let matches = handles
        .iter()
        .filter(|handle| handle.handle_id == handle_id)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "paired action handle {handle_id:?} resolves to {} stored actions instead of one",
            matches.len()
        ));
    }
    Ok(matches[0].clone())
}

fn rust_decision_action_for_action(
    root: &StateRootArtifactV2,
    session: &RunControlSession,
    handle: &PairedActionHandleV1,
) -> Result<RunDecisionAction, String> {
    match &handle.action {
        PairedActionV1::ChooseNeowOption { reward, drawback } => {
            let event = session
                .run_state
                .event_state
                .as_ref()
                .filter(|event| event.id == EventId::Neow && event.current_screen == 1)
                .ok_or_else(|| "Rust root is not the Neow reward-choice screen".to_owned())?;
            let matches = build_decision_surface(session)
                .view
                .candidates
                .into_iter()
                .filter_map(|candidate| {
                    let DecisionCandidateKey::EventOption {
                        event_id: EventId::Neow,
                        screen: 1,
                        option_index,
                        ..
                    } = candidate.key?
                    else {
                        return None;
                    };
                    let (candidate_reward, candidate_drawback) =
                        rust_neow_choice_identity(event, option_index)?;
                    (rust_neow_reward_identity(candidate_reward) == *reward
                        && rust_neow_drawback_identity(candidate_drawback) == *drawback)
                        .then(|| candidate.action.executable_action())
                        .flatten()
                })
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(format!(
                    "Rust Neow root omits one unique {reward:?}/{drawback:?} action: found {}",
                    matches.len()
                ));
            };
            Ok(action.clone())
        }
        PairedActionV1::LeaveNeow => {
            session
                .run_state
                .event_state
                .as_ref()
                .filter(|event| event.id == EventId::Neow && event.current_screen == 2)
                .ok_or_else(|| "Rust root is not the completed Neow screen".to_owned())?;
            let matches = build_decision_surface(session)
                .view
                .candidates
                .into_iter()
                .filter_map(|candidate| {
                    matches!(
                        candidate.key,
                        Some(DecisionCandidateKey::EventOption {
                            event_id: EventId::Neow,
                            screen: 2,
                            action: EventActionKind::Leave,
                            ..
                        })
                    )
                    .then(|| candidate.action.executable_action())
                    .flatten()
                })
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(format!(
                    "Rust completed Neow root exposes {} leave actions instead of one",
                    matches.len()
                ));
            };
            Ok(action.clone())
        }
        PairedActionV1::PlayCard { binding_id, target } => {
            let combat = active_combat(session)?;
            let binding = root
                .root
                .card_bindings
                .iter()
                .find(|binding| binding.binding_id == *binding_id)
                .ok_or_else(|| "paired card action references an unknown binding".to_owned())?;
            let lineage = rust_lineage_key(&binding.rust_lineage);
            let combat_sequence = session.combat_sequence();
            let binding_keys = BTreeMap::from([(lineage.clone(), binding.binding_id.as_str())]);
            let card_index = combat
                .zones
                .hand
                .iter()
                .position(|card| {
                    runtime_rust_lineage_key(card.lineage, combat_sequence, &binding_keys)
                        .is_ok_and(|candidate| candidate == lineage)
                })
                .ok_or_else(|| format!("Rust hand omits bound lineage {lineage}"))?;
            let target = target
                .as_ref()
                .map(|target| {
                    let mut ordinal = 0usize;
                    for monster in &combat.entities.monsters {
                        if canonical_game_id(monster.enemy_id.stable_id()) != target.enemy_id {
                            continue;
                        }
                        if ordinal == target.same_enemy_ordinal {
                            return Ok(monster.id);
                        }
                        ordinal += 1;
                    }
                    Err(format!(
                        "Rust combat omits target {} occurrence {}",
                        target.enemy_id, target.same_enemy_ordinal
                    ))
                })
                .transpose()?;
            Ok(RunDecisionAction::Input(ClientInput::PlayCard {
                card_index,
                target,
            }))
        }
        PairedActionV1::EndTurn => {
            active_combat(session)?;
            Ok(RunDecisionAction::Input(ClientInput::EndTurn))
        }
        PairedActionV1::UsePotion {
            slot,
            potion_id,
            potency,
            target,
        } => {
            let combat = active_combat(session)?;
            let potion_index = usize::from(*slot);
            let potion = combat
                .entities
                .potions
                .get(potion_index)
                .and_then(Option::as_ref)
                .ok_or_else(|| format!("Rust potion slot {slot} is absent"))?;
            if canonical_potion_id(potion.id.stable_id()) != *potion_id
                || rust_potion_potency(combat, potion.id) != *potency
            {
                return Err(format!(
                    "Rust potion slot {slot} disagrees with the paired action identity"
                ));
            }
            let target = target
                .as_ref()
                .map(|target| {
                    let mut ordinal = 0usize;
                    for monster in &combat.entities.monsters {
                        if canonical_game_id(monster.enemy_id.stable_id()) != target.enemy_id {
                            continue;
                        }
                        if ordinal == target.same_enemy_ordinal {
                            return Ok(monster.id);
                        }
                        ordinal += 1;
                    }
                    Err(format!(
                        "Rust combat omits potion target {} occurrence {}",
                        target.enemy_id, target.same_enemy_ordinal
                    ))
                })
                .transpose()?;
            Ok(RunDecisionAction::Input(ClientInput::UsePotion {
                potion_index,
                target,
            }))
        }
        PairedActionV1::ClaimCombatReward {
            reward,
            same_reward_ordinal,
        } => {
            let EngineState::RewardScreen(reward_state) = &session.engine_state else {
                return Err("Rust root is not an ordinary combat reward screen".to_owned());
            };
            if reward_state.pending_card_choice.is_some() {
                return Err("Rust combat reward screen has an open card choice".to_owned());
            }
            let has_empty_potion_slot = session.run_state.potions.iter().any(Option::is_none);
            let mut ordinal = 0usize;
            for (index, item) in reward_state.items.iter().enumerate() {
                if rust_supported_reward_identity(item, has_empty_potion_slot).as_ref()
                    != Some(reward)
                {
                    continue;
                }
                if ordinal == *same_reward_ordinal {
                    return Ok(RunDecisionAction::Input(ClientInput::ClaimReward(index)));
                }
                ordinal += 1;
            }
            Err(format!(
                "Rust reward screen omits {reward:?} occurrence {same_reward_ordinal}"
            ))
        }
        PairedActionV1::ChooseCardReward {
            card_id,
            upgrades,
            same_card_ordinal,
        } => {
            let EngineState::RewardScreen(reward_state) = &session.engine_state else {
                return Err("Rust root is not a card reward screen".to_owned());
            };
            let cards = reward_state
                .pending_card_choice
                .as_ref()
                .ok_or_else(|| "Rust card reward omitted pending choices".to_owned())?;
            let mut ordinal = 0usize;
            for (index, card) in cards.iter().enumerate() {
                let candidate_id = java_card_id(card.id);
                let candidate_upgrades = session
                    .run_state
                    .preview_obtain_card_upgrades(card.id, card.upgrades);
                if candidate_id != card_id || candidate_upgrades != *upgrades {
                    continue;
                }
                if ordinal == *same_card_ordinal {
                    return Ok(RunDecisionAction::Input(ClientInput::SelectCard(index)));
                }
                ordinal += 1;
            }
            Err(format!(
                "Rust card reward omits {card_id}+{upgrades} occurrence {same_card_ordinal}"
            ))
        }
        PairedActionV1::SkipCardReward => {
            let EngineState::RewardScreen(reward_state) = &session.engine_state else {
                return Err("Rust root is not a card reward screen".to_owned());
            };
            let reward_item_index = reward_state
                .pending_card_reward_index
                .ok_or_else(|| "Rust card reward omitted its source item index".to_owned())?;
            Ok(RunDecisionAction::SkipCardReward { reward_item_index })
        }
        PairedActionV1::CloseCombatReward => {
            let EngineState::RewardScreen(reward_state) = &session.engine_state else {
                return Err("Rust root is not an ordinary combat reward screen".to_owned());
            };
            if !reward_state.skippable
                || reward_state.pending_card_choice.is_some()
                || !reward_state.items.is_empty()
            {
                return Err(
                    "paired close-combat-reward is restricted to an empty skippable reward screen"
                        .to_owned(),
                );
            }
            Ok(RunDecisionAction::Input(ClientInput::Proceed))
        }
        PairedActionV1::ChooseEventOption {
            event_id,
            screen,
            option_index,
        } => {
            let (active_event, active_screen) = rust_event_identity(session)?;
            if active_event.stable_id() != event_id || active_screen != *screen {
                return Err("Rust event action source identity changed".to_owned());
            }
            let matches = build_decision_surface(session)
                .view
                .candidates
                .iter()
                .filter(|candidate| {
                    matches!(
                        candidate.key.as_ref(),
                        Some(DecisionCandidateKey::EventOption {
                            event_id: candidate_event,
                            screen: candidate_screen,
                            option_index: candidate_option,
                            ..
                        }) if candidate_event == &active_event
                            && candidate_screen == screen
                            && candidate_option == option_index
                    )
                })
                .filter_map(|candidate| candidate.action.executable_action())
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(format!(
                    "Rust event omits one unique option {event_id}:{screen}:{option_index}: found {}",
                    matches.len()
                ));
            };
            Ok(action.clone())
        }
        PairedActionV1::ChooseMapNode { x, y, room_type } => {
            let surface = build_decision_surface(session);
            let matches = surface
                .view
                .candidates
                .iter()
                .filter(|candidate| {
                    matches!(
                        candidate.key.as_ref(),
                        Some(DecisionCandidateKey::RouteSelect {
                            x: candidate_x,
                            y: candidate_y,
                            room_type: Some(candidate_room),
                            ..
                        }) if candidate_x == x
                            && candidate_y == y
                            && map_room_identity_from_planner(*candidate_room) == *room_type
                    )
                })
                .filter_map(|candidate| candidate.action.executable_action())
                .collect::<Vec<_>>();
            let [action] = matches.as_slice() else {
                return Err(format!(
                    "Rust map omits one unique node ({x},{y}) {room_type:?}: found {}",
                    matches.len()
                ));
            };
            Ok(action.clone())
        }
    }
}

fn rust_supported_reward_identity(
    item: &RewardItem,
    has_empty_potion_slot: bool,
) -> Option<CombatRewardIdentityV1> {
    match item {
        RewardItem::Gold { amount } => Some(CombatRewardIdentityV1::Gold { amount: *amount }),
        RewardItem::StolenGold { amount } => {
            Some(CombatRewardIdentityV1::StolenGold { amount: *amount })
        }
        RewardItem::Card { cards } if !cards.is_empty() => Some(CombatRewardIdentityV1::Card {
            choice_count: cards.len(),
        }),
        RewardItem::Potion { potion_id } if has_empty_potion_slot => {
            Some(CombatRewardIdentityV1::Potion {
                potion_id: canonical_potion_id(potion_id.stable_id()),
            })
        }
        _ => None,
    }
}

fn java_action_id_for_handle(
    root: &StateRootArtifactV2,
    activation: &StateRootActivationArtifactV2,
    live: &LiveStatus,
    handle: &PairedActionHandleV1,
) -> Result<String, String> {
    let matches = match &handle.action {
        PairedActionV1::ChooseNeowOption { reward, drawback } => live
            .actions
            .iter()
            .filter(|candidate| {
                candidate.kind == "choose_event_option"
                    && candidate.event_id.as_deref() == Some(NEOW_EVENT_ID)
                    && candidate
                        .neow_reward_type
                        .as_deref()
                        .map(java_neow_reward_identity)
                        .transpose()
                        .is_ok_and(|identity| identity.as_ref() == Some(reward))
                    && candidate
                        .neow_drawback
                        .as_deref()
                        .map(java_neow_drawback_identity)
                        .transpose()
                        .is_ok_and(|identity| identity.as_ref() == Some(drawback))
            })
            .collect::<Vec<_>>(),
        PairedActionV1::LeaveNeow => live
            .actions
            .iter()
            .filter(|candidate| {
                candidate.kind == "choose_event_option"
                    && candidate.event_id.as_deref() == Some(NEOW_EVENT_ID)
                    && candidate.neow_reward_type.is_none()
                    && candidate.neow_drawback.is_none()
            })
            .collect::<Vec<_>>(),
        PairedActionV1::PlayCard { binding_id, target } => {
            let lease = activation
                .activation
                .card_leases
                .iter()
                .find(|lease| lease.binding_id == *binding_id)
                .ok_or_else(|| {
                    "paired card action references an unleased Java binding".to_owned()
                })?;
            if !root
                .root
                .card_bindings
                .iter()
                .any(|binding| binding.binding_id == lease.binding_id)
            {
                return Err(
                    "paired activation leases a binding outside its logical root".to_owned(),
                );
            }
            live.actions
                .iter()
                .filter(|candidate| {
                    candidate.kind == "play_card"
                        && candidate.card_uuid.as_deref() == Some(lease.java_card_uuid.as_str())
                        && java_monster_target(&live.frame, candidate.target_monster_index)
                            .ok()
                            .as_ref()
                            == Some(target)
                })
                .collect::<Vec<_>>()
        }
        PairedActionV1::EndTurn => live
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "end_turn")
            .collect::<Vec<_>>(),
        PairedActionV1::UsePotion {
            slot,
            potion_id,
            potency,
            target,
        } => live
            .actions
            .iter()
            .filter(|candidate| {
                candidate.kind == "use_potion"
                    && candidate.potion_slot == Some(usize::from(*slot))
                    && candidate
                        .potion_id
                        .as_deref()
                        .map(canonical_potion_id)
                        .as_deref()
                        == Some(potion_id.as_str())
                    && candidate.potion_potency == Some(*potency)
                    && java_monster_target(&live.frame, candidate.target_monster_index)
                        .ok()
                        .as_ref()
                        == Some(target)
            })
            .collect::<Vec<_>>(),
        PairedActionV1::ClaimCombatReward {
            reward,
            same_reward_ordinal,
        } => java_supported_reward_occurrences(live)?
            .into_iter()
            .filter_map(|(candidate, candidate_reward, ordinal)| {
                (&candidate_reward == reward && ordinal == *same_reward_ordinal)
                    .then_some(candidate)
            })
            .collect(),
        PairedActionV1::ChooseCardReward {
            card_id,
            upgrades,
            same_card_ordinal,
        } => java_card_reward_occurrences(live)?
            .into_iter()
            .filter_map(|(candidate, candidate_id, candidate_upgrades, ordinal)| {
                (&candidate_id == card_id
                    && candidate_upgrades == *upgrades
                    && ordinal == *same_card_ordinal)
                    .then_some(candidate)
            })
            .collect(),
        PairedActionV1::SkipCardReward => live
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "skip_card_reward")
            .collect(),
        PairedActionV1::CloseCombatReward => live
            .actions
            .iter()
            .filter(|candidate| candidate.kind == "close_combat_reward")
            .collect(),
        PairedActionV1::ChooseEventOption {
            event_id,
            screen,
            option_index,
        } => {
            let (active_event, active_screen) = java_event_identity(live)?;
            if active_event.stable_id() != event_id || active_screen != *screen {
                return Err("Java event action source identity changed".to_owned());
            }
            live.actions
                .iter()
                .filter(|candidate| {
                    candidate.kind == "choose_event_option"
                        && candidate.option_index == Some(*option_index)
                        && candidate
                            .event_id
                            .as_deref()
                            .is_some_and(|raw| EventId::from_stable_id(raw) == Some(active_event))
                })
                .collect()
        }
        PairedActionV1::ChooseMapNode { x, y, room_type } => java_map_action_occurrences(live)?
            .into_iter()
            .filter_map(|(candidate, candidate_x, candidate_y, candidate_room)| {
                (candidate_x == *x && candidate_y == *y && candidate_room == *room_type)
                    .then_some(candidate)
            })
            .collect(),
    };
    if matches.len() != 1 {
        return Err(format!(
            "paired handle resolves to {} Java actions instead of one",
            matches.len()
        ));
    }
    Ok(matches[0].action_id.clone())
}

fn java_monster_target(
    frame: &Value,
    target_index: Option<usize>,
) -> Result<Option<MonsterTargetV1>, String> {
    let Some(target_index) = target_index else {
        return Ok(None);
    };
    let monsters = frame["public_base"]["combat"]["monsters"]
        .as_array()
        .ok_or_else(|| "Java combat omitted monsters".to_owned())?;
    let target = monsters
        .get(target_index)
        .ok_or_else(|| format!("Java target index {target_index} is outside combat"))?;
    let enemy_id = canonical_game_id(required_str(target, "id")?);
    let same_enemy_ordinal = monsters[..target_index]
        .iter()
        .filter(|monster| {
            monster["id"]
                .as_str()
                .is_some_and(|id| canonical_game_id(id) == enemy_id)
        })
        .count();
    Ok(Some(MonsterTargetV1 {
        enemy_id,
        same_enemy_ordinal,
    }))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

fn validate_live_root_identity(
    live: &LiveStatus,
    checkpoint: &ValidatedNativeCombatCheckpointV1,
) -> Result<(), String> {
    if live.frame["parity_internal"]["content_profile"].as_str()
        != Some(checkpoint.content_profile.as_str())
    {
        return Err(
            "fresh Java checkpoint restore is not bound to the checkpoint content profile"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_constructed_scenario_source_v1(
    plan_path: &Path,
    plan: &ScenarioPlan,
    report: &ConstructedScenarioReport,
    live: &LiveStatus,
) -> Result<(), String> {
    if report.schema != "sts_java_parity_constructed_scenario_report_v2"
        || report.provenance != "constructed_validated"
        || !report.passed
        || report.failure.is_some()
        || report.evidence_scope.natural_reachability
        || report.evidence_scope.strategy_evidence
        || report.evidence_scope.training_eligible
        || !report.evidence_scope.constructed_root_verified
        || report.evidence_scope.simulator_comparison
    {
        return Err(
            "constructed Java report does not carry the exact scoped root proof".to_owned(),
        );
    }
    if !same_canonical_file(plan_path, Path::new(&report.scenario_plan_path))?
        || !report
            .scenario_plan_sha256
            .eq_ignore_ascii_case(&sha256_file_local(plan_path)?)
    {
        return Err("constructed Java report is bound to another scenario plan".to_owned());
    }
    let root = report
        .root
        .as_ref()
        .ok_or_else(|| "constructed Java report omitted its root proof".to_owned())?;
    let live_frame_id = live.frame["frame_id"]
        .as_u64()
        .ok_or_else(|| "constructed Java live frame omitted frame_id".to_owned())?;
    if root.boundary != "combat_player_decision"
        || root.frame_id != live_frame_id
        || !same_canonical_file(Path::new(&root.frame_path), Path::new(&live.frame_path))?
        || !root
            .frame_sha256
            .eq_ignore_ascii_case(&sha256_file_local(Path::new(&live.frame_path))?)
    {
        return Err("constructed Java report root differs from the current live frame".to_owned());
    }
    let recipe = serde_json::to_value(&plan.recipe)
        .map_err(|error| format!("failed to encode constructed recipe: {error}"))?;
    if root.scenario_context != recipe || live.frame["scenario"] != recipe {
        return Err("constructed Java root scenario context differs from its plan".to_owned());
    }
    if report.session.session_id != live.manifest.session_id
        || report.session.connection_id != live.manifest.connection_id
        || report.session.process_id != live.manifest.process_id
    {
        return Err("constructed Java report belongs to another live runtime".to_owned());
    }
    Ok(())
}

fn same_canonical_file(left: &Path, right: &Path) -> Result<bool, String> {
    let left = fs::canonicalize(left)
        .map_err(|error| format!("failed to resolve {}: {error}", left.display()))?;
    let right = fs::canonicalize(right)
        .map_err(|error| format!("failed to resolve {}: {error}", right.display()))?;
    Ok(left == right)
}

fn absolute_path_text(path: &Path) -> Result<String, String> {
    fs::canonicalize(path)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| format!("failed to resolve {}: {error}", path.display()))
}

fn sha256_file_local(path: &Path) -> Result<String, String> {
    fs::read(path)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|error| format!("failed to hash {}: {error}", path.display()))
}

fn unknown_projection_summaries(gaps: &[CoverageGapV1]) -> Vec<ProjectionPreparationSummaryV1> {
    required_domain_schemas()
        .into_iter()
        .map(|(domain, schema)| {
            let gap = gaps.iter().find(|gap| gap.domain == domain);
            ProjectionPreparationSummaryV1 {
                domain,
                schema: schema.to_owned(),
                status: ComparisonStatusV1::Unknown,
                first_difference: Some(TypedDifferenceV1 {
                    path: gap
                        .map(|gap| gap.gap_id.clone())
                        .unwrap_or_else(|| "$".to_owned()),
                    kind: gap
                        .map(|gap| gap.kind.clone())
                        .unwrap_or_else(|| "comparison_blocked_by_coverage".to_owned()),
                    detail: gap.map(|gap| gap.detail.clone()).unwrap_or_else(|| {
                        "domain was not compared because another required field was absent"
                            .to_owned()
                    }),
                }),
            }
        })
        .collect()
}

fn validate_live_stable_combat_boundary(live: &LiveStatus) -> Result<(), String> {
    validate_live_stable_decision_profile(live, DecisionProfileV1::CombatPlayerDecision)
}

fn rust_decision_profile(session: &RunControlSession) -> Option<DecisionProfileV1> {
    match &session.engine_state {
        EngineState::CombatPlayerTurn => Some(DecisionProfileV1::CombatPlayerDecision),
        EngineState::RewardScreen(reward) if reward.pending_card_choice.is_some() => {
            Some(DecisionProfileV1::CardReward)
        }
        EngineState::RewardScreen(_) => Some(DecisionProfileV1::CombatReward),
        EngineState::MapNavigation => Some(DecisionProfileV1::MapNavigation),
        EngineState::EventRoom => session.run_state.event_state.as_ref().map(|event| {
            if event.id == EventId::Neow {
                DecisionProfileV1::Neow
            } else {
                DecisionProfileV1::Event
            }
        }),
        EngineState::CombatProcessing
        | EngineState::RewardOverlay { .. }
        | EngineState::TreasureRoom(_)
        | EngineState::Campfire
        | EngineState::Shop(_)
        | EngineState::MapOverlay { .. }
        | EngineState::CombatStart(_)
        | EngineState::PendingChoice(_)
        | EngineState::RunPendingChoice(_)
        | EngineState::BossRelicSelect(_)
        | EngineState::GameOver(_)
        | EngineState::DoubleBossComplete
        | EngineState::Act3VictoryComplete
        | EngineState::TrueVictoryComplete => None,
    }
}

fn live_decision_profile(live: &LiveStatus) -> Result<Option<DecisionProfileV1>, String> {
    if live.frame["boundary"]["stable"].as_bool() != Some(true) {
        return Ok(None);
    }
    match live.frame["boundary"]["kind"].as_str() {
        Some("combat_player_decision") => Ok(Some(DecisionProfileV1::CombatPlayerDecision)),
        Some("run_decision")
            if live.frame["public_base"]["decision"]["kind"].as_str() == Some("event")
                && live.frame["public_base"]["decision"]["event_id"].as_str()
                    == Some(NEOW_EVENT_ID)
                && matches!(
                    live.frame["public_base"]["decision"]["event_screen_index"].as_i64(),
                    Some(3 | 99)
                ) =>
        {
            Ok(Some(DecisionProfileV1::Neow))
        }
        Some("run_decision")
            if live.frame["public_base"]["decision"]["kind"].as_str() == Some("event")
                && live.frame["public_base"]["decision"]["event_id"]
                    .as_str()
                    .is_some_and(|event_id| event_id != NEOW_EVENT_ID) =>
        {
            java_event_identity(live)?;
            Ok(Some(DecisionProfileV1::Event))
        }
        Some("run_decision")
            if live.frame["boundary"]["evidence"]["screen_type"].as_str()
                == Some("COMBAT_REWARD")
                && live.frame["boundary"]["evidence"]["room_phase"].as_str()
                    == Some("COMPLETE")
                && live.frame["public_base"]["decision"]["kind"].as_str()
                    == Some("combat_reward") =>
        {
            Ok(Some(DecisionProfileV1::CombatReward))
        }
        Some("run_decision")
            if live.frame["boundary"]["evidence"]["screen_type"].as_str()
                == Some("CARD_REWARD")
                && live.frame["boundary"]["evidence"]["room_phase"].as_str()
                    == Some("COMPLETE")
                && live.frame["public_base"]["decision"]["kind"].as_str()
                    == Some("card_reward") =>
        {
            Ok(Some(DecisionProfileV1::CardReward))
        }
        Some("run_decision")
            if live.frame["boundary"]["evidence"]["screen_type"].as_str() == Some("MAP")
                && live.frame["boundary"]["evidence"]["room_phase"].as_str()
                    == Some("COMPLETE")
                && live.frame["public_base"]["decision"]["kind"].as_str() == Some("map") =>
        {
            Ok(Some(DecisionProfileV1::MapNavigation))
        }
        _ => Ok(None),
    }
}

fn validate_live_stable_decision_profile(
    live: &LiveStatus,
    expected: DecisionProfileV1,
) -> Result<(), String> {
    let actual = live_decision_profile(live)?;
    if actual != Some(expected) {
        return Err(format!(
            "Java frame is not a stable {expected:?} decision: actual={actual:?}"
        ));
    }
    Ok(())
}

fn live_successor_boundary_v1(live: &LiveStatus) -> Result<PairedRootSuccessorBoundaryV1, String> {
    let boundary = live
        .frame
        .get("boundary")
        .and_then(Value::as_object)
        .ok_or_else(|| "Java successor frame omitted its typed boundary".to_owned())?;
    let kind = boundary
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java successor boundary omitted its kind".to_owned())?;
    let stable = boundary
        .get("stable")
        .and_then(Value::as_bool)
        .ok_or_else(|| "Java successor boundary omitted its stability".to_owned())?;
    let evidence = boundary.get("evidence").and_then(Value::as_object);
    Ok(PairedRootSuccessorBoundaryV1 {
        kind: kind.to_owned(),
        stable,
        screen_type: evidence
            .and_then(|value| value.get("screen_type"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        room_phase: evidence
            .and_then(|value| value.get("room_phase"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn required_domain_schemas() -> [(ProjectionDomainV1, &'static str); 9] {
    [
        (ProjectionDomainV1::RunScalars, "sts-paired-run-scalars-v1"),
        (
            ProjectionDomainV1::MasterDeck,
            "sts-paired-master-deck-order-v1",
        ),
        (
            ProjectionDomainV1::Relics,
            "sts-paired-relic-order-counter-v1",
        ),
        (ProjectionDomainV1::Potions, "sts-paired-potion-slots-v1"),
        (ProjectionDomainV1::RngStreams, "sts-paired-rng-streams-v1"),
        (ProjectionDomainV1::CardPools, "sts-paired-card-pools-v1"),
        (
            ProjectionDomainV1::Combat,
            COMBAT_ROOT_SCHEMA_PUBLIC_INTENT_V2,
        ),
        (
            ProjectionDomainV1::ActionSpace,
            ACTION_SPACE_SCHEMA_POTION_V2,
        ),
        (
            ProjectionDomainV1::CardBindings,
            "sts-paired-master-deck-binding-relation-v1",
        ),
    ]
}

fn required_java_frame_coverage_gaps(frame: &Value) -> Vec<CoverageGapV1> {
    let required = [
        (
            "/parity_internal/content_profile",
            ProjectionDomainV1::RunScalars,
        ),
        (
            "/parity_internal/master_deck",
            ProjectionDomainV1::MasterDeck,
        ),
        ("/parity_internal/relics", ProjectionDomainV1::Relics),
        ("/public_base/combat/potions", ProjectionDomainV1::Potions),
        ("/rng_internal/streams", ProjectionDomainV1::RngStreams),
        ("/rng_internal/run_streams", ProjectionDomainV1::RngStreams),
        ("/parity_internal/card_pools", ProjectionDomainV1::CardPools),
        ("/parity_internal/zones", ProjectionDomainV1::Combat),
        ("/parity_internal/monsters", ProjectionDomainV1::Combat),
        ("/parity_internal/player_powers", ProjectionDomainV1::Combat),
        ("/public_base/run", ProjectionDomainV1::RunScalars),
        ("/public_base/combat", ProjectionDomainV1::Combat),
        ("/action_space/actions", ProjectionDomainV1::ActionSpace),
    ];
    let mut gaps = required
        .into_iter()
        .filter(|(pointer, _)| frame.pointer(pointer).is_none())
        .map(|(pointer, domain)| CoverageGapV1 {
            gap_id: format!(
                "java-frame-field-{}",
                pointer.trim_start_matches('/').replace('/', "-")
            ),
            domain,
            scope: CoverageGapScopeV1::StateProjection,
            kind: "required_java_field_absent".to_owned(),
            detail: format!("fresh Java frame omitted required typed field {pointer}"),
        })
        .collect::<Vec<_>>();

    if frame.pointer("/parity_internal/master_deck").is_none() {
        for domain in [ProjectionDomainV1::CardBindings, ProjectionDomainV1::Combat] {
            gaps.push(CoverageGapV1 {
                gap_id: format!("java-frame-master-deck-required-for-{domain:?}").to_lowercase(),
                domain,
                scope: CoverageGapScopeV1::StateProjection,
                kind: "source_identity_evidence_absent".to_owned(),
                detail: "master-deck UUID observations are required before combat cards can be bound to Rust lineages".to_owned(),
            });
        }
    }
    gaps.sort_by(|left, right| (left.domain, &left.gap_id).cmp(&(right.domain, &right.gap_id)));
    gaps
}

fn preflight_coverage_stop_report(
    request: &PreparePairedRootRequestV1,
    live: &LiveStatus,
    checkpoint: &ValidatedNativeCombatCheckpointV1,
    source_session: String,
    source_archive_node_id: String,
    gaps: Vec<CoverageGapV1>,
) -> PairedRootPreparationReportV1 {
    let components = required_domain_schemas()
        .into_iter()
        .map(|(domain, schema)| {
            let gap = gaps.iter().find(|gap| gap.domain == domain);
            ProjectionPreparationSummaryV1 {
                domain,
                schema: schema.to_owned(),
                status: ComparisonStatusV1::Unknown,
                first_difference: Some(TypedDifferenceV1 {
                    path: gap
                        .map(|gap| gap.gap_id.clone())
                        .unwrap_or_else(|| "$".to_owned()),
                    kind: gap
                        .map(|gap| gap.kind.clone())
                        .unwrap_or_else(|| "preflight_blocked_by_coverage".to_owned()),
                    detail: gap.map(|gap| gap.detail.clone()).unwrap_or_else(|| {
                        "domain was not compared because another required Java field was absent"
                            .to_owned()
                    }),
                }),
            }
        })
        .collect();
    PairedRootPreparationReportV1 {
        schema: PREPARE_REPORT_SCHEMA_V1.to_owned(),
        status: "preflight_stopped".to_owned(),
        source_session,
        retained_selector: request.retained_selector.clone(),
        source_archive_node_id,
        java_checkpoint_manifest: checkpoint.manifest.path.clone(),
        java_frame_path: live.frame_path.clone(),
        components,
        gaps,
        source_head_guard: pending_source_head_guard(),
        root_id: None,
        activation_id: None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceHeadInventoryV1 {
    sha256: String,
    file_count: usize,
}

fn pending_source_head_guard() -> SourceHeadGuardV1 {
    SourceHeadGuardV1 {
        before_sha256: String::new(),
        after_sha256: String::new(),
        file_count: 0,
        unchanged: false,
    }
}

fn identity_claim<'a>(claims: &'a [IdentityClaimV1], kind: &str) -> Result<&'a str, String> {
    let matches = claims
        .iter()
        .filter(|claim| claim.kind == kind)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "paired recovery payload has {} {kind:?} claims instead of one",
            matches.len()
        ));
    }
    Ok(matches[0].value.as_str())
}

fn resolve_source_session(
    store: &StateRootStoreV2,
    run_root: &Path,
    native: &StateRootArtifactV2,
) -> Result<PathBuf, String> {
    let ancestry = store
        .logical_ancestry(&native.root_id)?
        .into_iter()
        .map(|root_id| store.load_root(&root_id))
        .collect::<Result<Vec<_>, String>>()?;
    let (expected_session_id, expected_archive_node_id) = inherited_run_play_source_identity(
        ancestry
            .iter()
            .map(|root| root.root.rust_snapshot.source_identity.as_slice()),
    )?;
    let root_versions = supported_root_version_map(&native.root.versions)?;
    let expected_simulator_semantics = root_versions
        .get("rust_simulator_semantics")
        .expect("supported paired-root version component checked above");
    let expected_rng_semantics = root_versions
        .get("rust_rng_semantics")
        .expect("supported paired-root version component checked above");
    let run_root = fs::canonicalize(run_root).map_err(|error| {
        format!(
            "failed to resolve run-play root {}: {error}",
            run_root.display()
        )
    })?;
    let mut matches = Vec::new();
    for entry in fs::read_dir(&run_root).map_err(|error| {
        format!(
            "failed to inspect run-play root {}: {error}",
            run_root.display()
        )
    })? {
        let entry = entry.map_err(|error| format!("failed to enumerate run-play root: {error}"))?;
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "failed to inspect run-play session {}: {error}",
                entry.path().display()
            )
        })?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let heads = entry.path().join("heads");
        if !heads.is_dir() {
            continue;
        }
        let mut latest = None;
        for head in fs::read_dir(&heads)
            .map_err(|error| format!("failed to inspect {}: {error}", heads.display()))?
        {
            let head =
                head.map_err(|error| format!("failed to enumerate source heads: {error}"))?;
            if !head
                .file_type()
                .map_err(|error| format!("failed to inspect {}: {error}", head.path().display()))?
                .is_file()
            {
                continue;
            }
            let name = head.file_name();
            let name = name.to_string_lossy();
            let Some(revision) = name
                .strip_prefix("head-")
                .and_then(|name| name.strip_suffix(".json"))
                .and_then(|revision| revision.parse::<u64>().ok())
            else {
                continue;
            };
            if latest
                .as_ref()
                .is_none_or(|(current, _): &(u64, PathBuf)| revision > *current)
            {
                latest = Some((revision, head.path()));
            }
        }
        let Some((_, latest)) = latest else {
            continue;
        };
        let head: Value = serde_json::from_slice(&fs::read(&latest).map_err(|error| {
            format!("failed to read source head {}: {error}", latest.display())
        })?)
        .map_err(|error| format!("failed to decode source head {}: {error}", latest.display()))?;
        if source_head_matches_recovery_identity(
            &head,
            &expected_session_id,
            &expected_archive_node_id,
            expected_simulator_semantics,
            expected_rng_semantics,
        ) {
            let path = entry.path();
            matches.push((path.clone(), source_head_inventory(&path)?));
        }
    }
    let selected = choose_equivalent_source_session(matches, &expected_session_id)?;
    fs::canonicalize(&selected).map_err(|error| {
        format!(
            "failed to resolve paired source session {}: {error}",
            selected.display()
        )
    })
}

fn inherited_run_play_source_identity<'a>(
    claim_sets: impl IntoIterator<Item = &'a [IdentityClaimV1]>,
) -> Result<(String, String), String> {
    let mut source = None;
    for claims in claim_sets {
        let session_ids = claims
            .iter()
            .filter(|claim| claim.kind == "run_play_session_id")
            .collect::<Vec<_>>();
        let archive_node_ids = claims
            .iter()
            .filter(|claim| claim.kind == "archive_node_id")
            .collect::<Vec<_>>();
        if session_ids.is_empty() && archive_node_ids.is_empty() {
            continue;
        }
        if session_ids.len() != 1 || archive_node_ids.len() != 1 {
            return Err(
                "paired logical ancestry contains an incomplete run-play source identity"
                    .to_owned(),
            );
        }
        if source.is_some() {
            return Err(
                "paired logical ancestry contains multiple run-play source identities".to_owned(),
            );
        }
        source = Some((
            session_ids[0].value.clone(),
            archive_node_ids[0].value.clone(),
        ));
    }
    source.ok_or_else(|| "paired logical ancestry has no run-play source identity".to_owned())
}

fn source_head_matches_recovery_identity(
    head: &Value,
    expected_session_id: &str,
    expected_archive_node_id: &str,
    expected_simulator_semantics: &str,
    expected_rng_semantics: &str,
) -> bool {
    head["session_id"].as_str() == Some(expected_session_id)
        && head["position"]["archive_node_id"].as_str() == Some(expected_archive_node_id)
        && head["evidence_identity"]["simulator_semantics_id"].as_str()
            == Some(expected_simulator_semantics)
        && head["evidence_identity"]["rng_semantics_id"].as_str() == Some(expected_rng_semantics)
}

fn choose_equivalent_source_session(
    mut matches: Vec<(PathBuf, SourceHeadInventoryV1)>,
    expected_session_id: &str,
) -> Result<PathBuf, String> {
    if matches.is_empty() {
        return Err(format!(
            "run-play root contains no session for paired source session {expected_session_id:?}"
        ));
    }
    matches.sort_by(|left, right| left.0.cmp(&right.0));
    let expected_inventory = &matches[0].1;
    if matches
        .iter()
        .any(|(_, inventory)| inventory != expected_inventory)
    {
        return Err(format!(
            "run-play root contains {} divergent sessions for paired source session {expected_session_id:?}",
            matches.len()
        ));
    }
    Ok(matches.remove(0).0)
}

fn checked_source_head_guard(
    session: &Path,
    before: SourceHeadInventoryV1,
) -> Result<SourceHeadGuardV1, String> {
    let after = source_head_inventory(session)?;
    let unchanged = before.file_count == after.file_count && before.sha256 == after.sha256;
    let guard = SourceHeadGuardV1 {
        before_sha256: before.sha256,
        after_sha256: after.sha256,
        file_count: after.file_count,
        unchanged,
    };
    if !unchanged {
        return Err("paired-root operation changed the source run-play head inventory".to_owned());
    }
    Ok(guard)
}

fn source_head_inventory(session: &Path) -> Result<SourceHeadInventoryV1, String> {
    let directory = session.join("heads");
    let mut files = fs::read_dir(&directory)
        .map_err(|error| {
            format!(
                "failed to inspect source head directory {}: {error}",
                directory.display()
            )
        })?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("failed to enumerate source heads: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    files.retain(|path| path.is_file());
    files.sort();
    let mut digest = Sha256::new();
    for path in &files {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("source head path is not UTF-8: {}", path.display()))?;
        let bytes = fs::read(path)
            .map_err(|error| format!("failed to read source head {}: {error}", path.display()))?;
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(Sha256::digest(&bytes));
    }
    Ok(SourceHeadInventoryV1 {
        sha256: hex_lower(&digest.finalize()),
        file_count: files.len(),
    })
}

fn finalize_source_head_guard(
    request: &PreparePairedRootRequestV1,
    before: SourceHeadInventoryV1,
    mut report: PairedRootPreparationReportV1,
) -> Result<PairedRootPreparationReportV1, String> {
    let after = source_head_inventory(&request.session)?;
    let unchanged = before.file_count == after.file_count && before.sha256 == after.sha256;
    report.source_head_guard = SourceHeadGuardV1 {
        before_sha256: before.sha256,
        after_sha256: after.sha256,
        file_count: after.file_count,
        unchanged,
    };
    if !unchanged {
        return Err(
            "paired-root preparation changed the source run-play head inventory".to_owned(),
        );
    }
    Ok(report)
}

fn finalize_preview_source_head_guard(
    request: &PreviewPairedRootActionRequestV1,
    before: SourceHeadInventoryV1,
    mut report: PairedRootPreviewReportV1,
) -> Result<PairedRootPreviewReportV1, String> {
    let after = source_head_inventory(&request.source_session)?;
    let unchanged = before.file_count == after.file_count && before.sha256 == after.sha256;
    report.source_head_guard = SourceHeadGuardV1 {
        before_sha256: before.sha256,
        after_sha256: after.sha256,
        file_count: after.file_count,
        unchanged,
    };
    if !unchanged {
        return Err("paired-root preview changed the source run-play head inventory".to_owned());
    }
    Ok(report)
}

fn finalize_recovery_source_head_guard(
    request: &RecoverPairedRootRequestV1,
    before: SourceHeadInventoryV1,
    mut report: PairedRootRecoveryReportV1,
) -> Result<PairedRootRecoveryReportV1, String> {
    let after = source_head_inventory(&request.source_session)?;
    let unchanged = before.file_count == after.file_count && before.sha256 == after.sha256;
    report.source_head_guard = SourceHeadGuardV1 {
        before_sha256: before.sha256,
        after_sha256: after.sha256,
        file_count: after.file_count,
        unchanged,
    };
    if !unchanged {
        return Err("paired-root recovery changed the source run-play head inventory".to_owned());
    }
    Ok(report)
}

fn build_constructed_recipe_bindings_v1(
    plan: &ScenarioPlan,
    live: &LiveStatus,
    session: &RunControlSession,
    recipe_blob: &BlobRefV1,
) -> Result<Vec<CrossRuntimeCardBindingV1>, String> {
    let mut declared_decks =
        plan.recipe
            .capabilities
            .iter()
            .filter_map(|capability| match capability {
                ScenarioCapability::MasterDeck(deck) => Some(deck),
                _ => None,
            });
    let deck = declared_decks
        .next()
        .ok_or_else(|| "constructed recipe omitted its exact master deck".to_owned())?;
    if declared_decks.next().is_some() {
        return Err("constructed recipe contains multiple master decks".to_owned());
    }
    let java = live.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "constructed Java root omitted parity_internal.master_deck".to_owned())?;
    let rust = &session.run_state.master_deck;
    if java.len() != deck.cards.len() || rust.len() != deck.cards.len() {
        return Err(format!(
            "constructed recipe deck cardinality differs: recipe={} Java={} Rust={}",
            deck.cards.len(),
            java.len(),
            rust.len()
        ));
    }

    let mut bindings = Vec::with_capacity(deck.cards.len());
    for (index, ((declared, java), rust)) in deck.cards.iter().zip(java).zip(rust).enumerate() {
        let slot = u64::try_from(index)
            .map_err(|_| "constructed recipe slot exceeds u64".to_owned())?
            .checked_add(1)
            .ok_or_else(|| "constructed recipe slot overflowed".to_owned())?;
        if java["ordinal"].as_u64() != Some(index as u64)
            || java["card_id"].as_str() != Some(declared.card_id.as_str())
            || java["upgrades"].as_u64() != Some(u64::from(declared.upgrades))
            || java_card_id(rust.id) != declared.card_id
            || u32::from(rust.upgrades) != declared.upgrades
            || rust.lineage.origin() != CardLineageOrigin::ConstructedScenario
            || u64::from(rust.lineage.ordinal()) != slot
        {
            return Err(format!(
                "constructed recipe source evidence diverged at declared slot {slot}"
            ));
        }
        let rust_lineage = RustCardLineageV1::ConstructedScenarioInstance {
            scenario_instance_id: plan.recipe.scenario_id.clone(),
            slot,
        };
        let mut binding = CrossRuntimeCardBindingV1 {
            binding_id: String::new(),
            rust_lineage,
            card_id: declared.card_id.clone(),
            upgrades: rust.upgrades,
            evidence: CardBindingEvidenceV1::ConstructedScenario {
                scenario_instance_id: plan.recipe.scenario_id.clone(),
                slot,
                recipe: recipe_blob.clone(),
            },
        };
        binding.binding_id = card_binding_id(&binding)?;
        bindings.push(binding);
    }
    bindings.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    Ok(bindings)
}

fn build_constructed_recipe_leases_v1(
    live: &LiveStatus,
    bindings: &[CrossRuntimeCardBindingV1],
) -> Result<Vec<JavaCardLeaseV1>, String> {
    let java = live.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "constructed Java activation omitted its master deck".to_owned())?;
    let mut leases = Vec::with_capacity(bindings.len());
    let mut seen_uuids = BTreeSet::new();
    for binding in bindings {
        let CardBindingEvidenceV1::ConstructedScenario {
            scenario_instance_id,
            slot,
            ..
        } = &binding.evidence
        else {
            return Err("constructed activation contains non-recipe binding".to_owned());
        };
        if !matches!(
            &binding.rust_lineage,
            RustCardLineageV1::ConstructedScenarioInstance {
                scenario_instance_id: lineage_scenario,
                slot: lineage_slot,
            } if lineage_scenario == scenario_instance_id && lineage_slot == slot
        ) {
            return Err("constructed activation binding lineage/evidence differs".to_owned());
        }
        let ordinal = usize::try_from(
            slot.checked_sub(1)
                .ok_or_else(|| "constructed recipe uses slot zero".to_owned())?,
        )
        .map_err(|_| "constructed recipe slot exceeds this platform".to_owned())?;
        let card = java
            .get(ordinal)
            .ok_or_else(|| format!("constructed Java activation omits slot {slot}"))?;
        if card["ordinal"].as_u64() != Some(ordinal as u64)
            || card["card_id"].as_str() != Some(binding.card_id.as_str())
            || card["upgrades"].as_u64() != Some(u64::from(binding.upgrades))
        {
            return Err(format!(
                "constructed Java activation changed declared slot {slot}"
            ));
        }
        let java_card_uuid = card["card_uuid"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("constructed Java activation slot {slot} omitted UUID"))?;
        if !seen_uuids.insert(java_card_uuid) {
            return Err(
                "constructed Java activation reused one UUID for multiple slots".to_owned(),
            );
        }
        leases.push(JavaCardLeaseV1 {
            binding_id: binding.binding_id.clone(),
            java_card_uuid: java_card_uuid.to_owned(),
        });
    }
    leases.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    Ok(leases)
}

fn build_master_deck_bindings(
    live: &LiveStatus,
    session: &RunControlSession,
    java_projection: &BlobRefV1,
    rust_projection: &BlobRefV1,
) -> Result<Vec<CrossRuntimeCardBindingV1>, String> {
    let java = live.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "Java root omitted parity_internal.master_deck".to_owned())?;
    if java.len() != session.run_state.master_deck.len() {
        return Err(format!(
            "master-deck binding cardinality differs: Java={} Rust={}",
            java.len(),
            session.run_state.master_deck.len()
        ));
    }
    let mut bindings = Vec::with_capacity(java.len());
    for (ordinal, (java, rust)) in java.iter().zip(&session.run_state.master_deck).enumerate() {
        if java["ordinal"].as_u64() != Some(ordinal as u64)
            || java["card_id"].as_str() != Some(java_card_id(rust.id))
            || java["upgrades"].as_u64() != Some(u64::from(rust.upgrades))
        {
            return Err(format!(
                "master-deck source observation diverged at current ordinal {ordinal}"
            ));
        }
        let rust_lineage = project_rust_lineage(rust.lineage)?;
        let mut binding = CrossRuntimeCardBindingV1 {
            binding_id: String::new(),
            rust_lineage,
            card_id: java_card_id(rust.id).to_owned(),
            upgrades: rust.upgrades,
            evidence: CardBindingEvidenceV1::MasterDeckObservation {
                java_master_deck_ordinal: ordinal as u64,
                rust_master_deck_ordinal: ordinal as u64,
                java_projection: java_projection.clone(),
                rust_projection: rust_projection.clone(),
            },
        };
        binding.binding_id = card_binding_id(&binding)?;
        bindings.push(binding);
    }
    bindings.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    Ok(bindings)
}

fn observe_rust_card_births_for_input_v1(
    session: &RunControlSession,
    input: &ClientInput,
) -> Result<Option<CombatCardBirthReceipt>, String> {
    if !matches!(
        input,
        ClientInput::PlayCard { .. } | ClientInput::EndTurn | ClientInput::UsePotion { .. }
    ) {
        return Ok(None);
    }
    let position = session.current_active_combat_position()?;
    let cause = match input {
        ClientInput::PlayCard { card_index, .. } => {
            let producer_lineage = position
                .combat
                .zones
                .hand
                .get(*card_index)
                .ok_or_else(|| {
                    format!("card-birth observation hand index {card_index} is invalid")
                })?
                .lineage;
            CombatCardBirthCause::CardPlay { producer_lineage }
        }
        ClientInput::EndTurn => CombatCardBirthCause::EndTurnResolution,
        ClientInput::UsePotion { potion_index, .. } => {
            let potion = position
                .combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    format!("card-birth observation potion slot {potion_index} is absent")
                })?;
            CombatCardBirthCause::PotionUse {
                potion_id: potion.id,
                slot: *potion_index,
            }
        }
        _ => unreachable!("non-combat inputs returned before combat observation"),
    };
    let observed = apply_combat_input_to_stable_with_card_births_v1(
        &position,
        input.clone(),
        CombatStepLimits {
            max_engine_steps: 2_000,
            deadline: None,
        },
        cause,
    );
    if observed.step.truncated || observed.step.timed_out {
        return Err("Rust card-birth observation did not reach a stable successor".to_owned());
    }
    Ok(Some(observed.card_births))
}

fn java_birth_observed_cards_v1(
    frame: &JavaStateFrame,
) -> Result<Vec<JavaCardBirthObservedCardV1>, String> {
    let zones = frame
        .parity_internal
        .zones
        .as_object()
        .ok_or_else(|| "Java card-birth frame omitted parity_internal.zones".to_owned())?;
    let mut cards = Vec::new();
    for zone_name in ["draw", "hand", "discard", "exhaust", "limbo"] {
        let zone_cards = zones
            .get(zone_name)
            .and_then(|zone| zone["cards"].as_array())
            .ok_or_else(|| format!("Java card-birth frame omitted {zone_name} cards"))?;
        for card in zone_cards {
            cards.push(JavaCardBirthObservedCardV1 {
                java_uuid: required_str(card, "instance_uuid")?.to_owned(),
                card_id: required_str(card, "card_id")?.to_owned(),
                upgrades: u8::try_from(required_u64(card, "upgrades")?)
                    .map_err(|_| "Java card-birth upgrades exceed u8".to_owned())?,
            });
        }
    }
    Ok(cards)
}

fn project_runtime_lineage_v1(
    source: &StateRootArtifactV2,
    lineage: CardLineageId,
    combat_sequence: u64,
) -> Result<RustCardLineageV1, String> {
    match lineage.origin() {
        CardLineageOrigin::MasterDeck => Ok(RustCardLineageV1::MasterDeck {
            acquisition_ordinal: u64::from(lineage.ordinal()),
        }),
        CardLineageOrigin::CombatGenerated => Ok(RustCardLineageV1::CombatGenerated {
            combat_sequence,
            generation_ordinal: u64::from(lineage.ordinal()),
        }),
        CardLineageOrigin::ConstructedScenario => {
            Ok(RustCardLineageV1::ConstructedScenarioInstance {
                scenario_instance_id: root_constructed_scenario_instance_id(source)?.ok_or_else(
                    || {
                        "constructed runtime lineage has no root-owned scenario instance identity"
                            .to_owned()
                    },
                )?,
                slot: u64::from(lineage.ordinal()),
            })
        }
    }
}

struct ReconciledCardBirthTransitionV1 {
    products: Vec<ReconciledCardBirthProductV1>,
    stable_java_proof: Option<Value>,
}

fn reconcile_card_birth_transition_v1(
    source: &StateRootArtifactV2,
    source_activation: &StateRootActivationArtifactV2,
    before: &LiveStatus,
    after: &LiveStatus,
    java_receipt: &ActionReceipt,
    rust_receipt: Option<&CombatCardBirthReceipt>,
    combat_sequence: u64,
) -> Result<ReconciledCardBirthTransitionV1, String> {
    let Some(rust_receipt) = rust_receipt else {
        return Ok(ReconciledCardBirthTransitionV1 {
            products: Vec::new(),
            stable_java_proof: None,
        });
    };
    let (context_cause, stable_cause) = match &rust_receipt.cause {
        CombatCardBirthCause::CardPlay { producer_lineage } => {
            let projected_producer =
                project_runtime_lineage_v1(source, *producer_lineage, combat_sequence)?;
            let producer_bindings = source
                .root
                .card_bindings
                .iter()
                .filter(|binding| binding.rust_lineage == projected_producer)
                .collect::<Vec<_>>();
            if producer_bindings.len() != 1 {
                return Err(format!(
                    "card-birth producer lineage resolves to {} stable bindings instead of one",
                    producer_bindings.len()
                ));
            }
            let producer_leases = source_activation
                .activation
                .card_leases
                .iter()
                .filter(|lease| lease.binding_id == producer_bindings[0].binding_id)
                .collect::<Vec<_>>();
            if producer_leases.len() != 1 {
                return Err(format!(
                    "card-birth producer binding resolves to {} Java leases instead of one",
                    producer_leases.len()
                ));
            }
            (
                CardBirthTransitionCauseV1::CardPlay {
                    producer_java_uuid: producer_leases[0].java_card_uuid.clone(),
                    producer_rust_lineage: *producer_lineage,
                },
                json!({
                    "kind": "card_play",
                    "producer_binding_id": producer_bindings[0].binding_id
                }),
            )
        }
        CombatCardBirthCause::EndTurnResolution => (
            CardBirthTransitionCauseV1::EndTurnResolution,
            json!({"kind": "end_turn_resolution"}),
        ),
        CombatCardBirthCause::PotionUse { potion_id, slot } => {
            let candidates = before
                .actions
                .iter()
                .filter(|candidate| candidate.action_id == java_receipt.action_id)
                .collect::<Vec<_>>();
            if candidates.len() != 1
                || candidates[0].kind != "use_potion"
                || candidates[0].potion_slot != Some(*slot)
            {
                return Err(format!(
                    "potion card-birth cause resolves to {} matching Java actions",
                    candidates.len()
                ));
            }
            let java_potion_id = candidates[0]
                .potion_id
                .clone()
                .ok_or_else(|| "Java potion action omitted its identity".to_owned())?;
            (
                CardBirthTransitionCauseV1::PotionUse {
                    java_potion_id: java_potion_id.clone(),
                    rust_potion_id: *potion_id,
                    slot: *slot,
                },
                json!({
                    "kind": "potion_use",
                    "potion_id": java_potion_id,
                    "slot": slot
                }),
            )
        }
        _ => return Err("PairedRoot does not yet own this typed card-birth cause".to_owned()),
    };
    let before_frame: JavaStateFrame = serde_json::from_value(before.frame.clone())
        .map_err(|error| format!("failed to decode source Java card-birth frame: {error}"))?;
    let after_frame: JavaStateFrame = serde_json::from_value(after.frame.clone())
        .map_err(|error| format!("failed to decode successor Java card-birth frame: {error}"))?;
    let successor_frame_id = java_receipt
        .successor_frame_id
        .ok_or_else(|| "Java card-birth action omitted its successor frame".to_owned())?;
    let products = reconcile_card_birth_witness_v1(
        &CardBirthTransitionContextV1 {
            from_frame_id: before_frame.frame_id,
            to_frame_id: successor_frame_id,
            request_id: java_receipt.request_id.clone(),
            action_id: java_receipt.action_id.clone(),
            cause: context_cause,
        },
        after_frame.card_birth_witness.as_ref(),
        rust_receipt,
        &java_birth_observed_cards_v1(&before_frame)?,
        &java_birth_observed_cards_v1(&after_frame)?,
    )?;
    let java_witness = after_frame
        .card_birth_witness
        .as_ref()
        .ok_or_else(|| "reconciled Java card birth omitted its typed witness".to_owned())?;
    Ok(ReconciledCardBirthTransitionV1 {
        products,
        stable_java_proof: Some(stable_java_birth_proof_v1(stable_cause, java_witness)?),
    })
}

fn stable_java_birth_proof_v1(
    stable_cause: Value,
    java_witness: &JavaCardBirthWitness,
) -> Result<Value, String> {
    let stable_products = java_witness
        .products
        .iter()
        .map(|product| {
            let first_zone = product
                .first_zone
                .ok_or("ordinary birth proof requires a concrete first zone")?;
            Ok(json!({
                "generation_index": product.generation_index,
                "card_id": product.card_id,
                "upgrades": product.upgrades,
                "first_zone": stable_java_birth_zone_v1(first_zone)
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({
        "schema": CARD_BIRTH_STABLE_PROOF_SCHEMA_V1,
        "cause": stable_cause,
        "complete": java_witness.complete,
        "products": stable_products
    }))
}

fn stable_java_birth_zone_v1(zone: JavaCardBirthZone) -> &'static str {
    match zone {
        JavaCardBirthZone::Draw => "draw",
        JavaCardBirthZone::Hand => "hand",
        JavaCardBirthZone::Discard => "discard",
        JavaCardBirthZone::Exhaust => "exhaust",
        JavaCardBirthZone::Limbo => "limbo",
    }
}

fn build_preview_successor_card_identity_v1(
    store: &StateRootStoreV2,
    source: &StateRootArtifactV2,
    source_activation: &StateRootActivationArtifactV2,
    before: &LiveStatus,
    after: &LiveStatus,
    java_receipt: &ActionReceipt,
    rust_receipt: Option<&CombatCardBirthReceipt>,
    combat_sequence: u64,
) -> Result<(Vec<CrossRuntimeCardBindingV1>, Vec<JavaCardLeaseV1>), String> {
    let reconciled = reconcile_card_birth_transition_v1(
        source,
        source_activation,
        before,
        after,
        java_receipt,
        rust_receipt,
        combat_sequence,
    )?;
    let mut bindings = source.root.card_bindings.clone();
    let mut leases = source_activation.activation.card_leases.clone();
    if reconciled.products.is_empty() {
        return Ok((bindings, leases));
    }
    let java_witness = put_json(
        store,
        reconciled
            .stable_java_proof
            .as_ref()
            .ok_or_else(|| "reconciled card births omitted their stable Java proof".to_owned())?,
    )?;
    let rust_receipt =
        rust_receipt.ok_or_else(|| "reconciled card births omitted the Rust receipt".to_owned())?;
    let rust_receipt_blob = put_json(store, rust_receipt)?;
    for product in reconciled.products {
        let rust_lineage =
            project_runtime_lineage_v1(source, product.rust_card.lineage, combat_sequence)?;
        if bindings
            .iter()
            .any(|binding| binding.rust_lineage == rust_lineage)
        {
            return Err("card-birth product lineage was already bound".to_owned());
        }
        let mut binding = CrossRuntimeCardBindingV1 {
            binding_id: String::new(),
            rust_lineage,
            card_id: java_card_id(product.rust_card.id).to_owned(),
            upgrades: product.rust_card.upgrades,
            evidence: CardBindingEvidenceV1::CapturedBirth {
                java_witness: java_witness.clone(),
                rust_receipt: rust_receipt_blob.clone(),
                generation_index: u64::from(product.generation_index),
            },
        };
        binding.binding_id = card_binding_id(&binding)?;
        leases.push(JavaCardLeaseV1 {
            binding_id: binding.binding_id.clone(),
            java_card_uuid: product.java_uuid,
        });
        bindings.push(binding);
    }
    bindings.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    leases.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    Ok((bindings, leases))
}

fn build_persistent_master_deck_successor_identity_v1(
    store: &StateRootStoreV2,
    source: &StateRootArtifactV2,
    successor_live: &LiveStatus,
    successor_session: &RunControlSession,
) -> Result<(Vec<CrossRuntimeCardBindingV1>, Vec<JavaCardLeaseV1>), RunPlayAdapterFailureV1> {
    let java_semantics = java_master_deck_semantics(&successor_live.frame)?;
    let rust_semantics = rust_master_deck_semantics(successor_session);
    if java_semantics != rust_semantics {
        return Err(RunPlayAdapterFailureV1::persistent_master_deck_diverged(
            &java_semantics,
            &rust_semantics,
        ));
    }
    let java_projection = put_json(store, &java_semantics)?;
    let rust_projection = put_json(store, &rust_semantics)?;
    let java_master = successor_live.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "Java successor omitted parity_internal.master_deck".to_owned())?;
    if java_master.len() != successor_session.run_state.master_deck.len() {
        return Err("persistent master-deck identity cardinality differs"
            .to_owned()
            .into());
    }
    let mut bindings = Vec::with_capacity(java_master.len());
    for (ordinal, rust) in successor_session.run_state.master_deck.iter().enumerate() {
        let rust_lineage = project_persistent_rust_lineage_v1(source, rust.lineage)?;
        let java = &java_master[ordinal];
        if java["ordinal"].as_u64() != Some(ordinal as u64)
            || java["card_id"].as_str() != Some(java_card_id(rust.id))
            || java["upgrades"].as_u64() != Some(u64::from(rust.upgrades))
        {
            return Err(format!(
                "new persistent card identity diverged at master-deck ordinal {ordinal}"
            )
            .into());
        }
        let mut source_matches = source
            .root
            .card_bindings
            .iter()
            .filter(|binding| binding.rust_lineage == rust_lineage);
        let source_binding = source_matches.next();
        if source_matches.next().is_some() {
            return Err(format!(
                "source root repeats persistent lineage at master-deck ordinal {ordinal}"
            )
            .into());
        }
        if let Some(existing) = source_binding.filter(|binding| {
            binding.card_id == java_card_id(rust.id) && binding.upgrades == rust.upgrades
        }) {
            bindings.push(existing.clone());
            continue;
        }
        let evidence = match &rust_lineage {
            RustCardLineageV1::MasterDeck { .. } => {
                CardBindingEvidenceV1::MasterDeckObservation {
                    java_master_deck_ordinal: ordinal as u64,
                    rust_master_deck_ordinal: ordinal as u64,
                    java_projection: java_projection.clone(),
                    rust_projection: rust_projection.clone(),
                }
            }
            RustCardLineageV1::ConstructedScenario { .. }
            | RustCardLineageV1::ConstructedScenarioInstance { .. } => source_binding
                .map(|binding| binding.evidence.clone())
                .ok_or_else(|| {
                    format!(
                        "new constructed persistent lineage appeared at master-deck ordinal {ordinal}"
                    )
                })?,
            RustCardLineageV1::CombatGenerated { .. } => {
                return Err("persistent master deck contains combat-generated lineage"
                    .to_owned()
                    .into())
            }
        };
        let mut binding = CrossRuntimeCardBindingV1 {
            binding_id: String::new(),
            rust_lineage,
            card_id: java_card_id(rust.id).to_owned(),
            upgrades: rust.upgrades,
            evidence,
        };
        binding.binding_id = card_binding_id(&binding)?;
        bindings.push(binding);
    }
    bindings.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    if bindings.len() != successor_session.run_state.master_deck.len() {
        return Err(format!(
            "persistent successor has {} master-deck bindings for {} cards",
            bindings.len(),
            successor_session.run_state.master_deck.len()
        )
        .into());
    }
    let leases =
        build_persistent_java_card_leases_v1(source, successor_live, successor_session, &bindings)?;
    Ok((bindings, leases))
}

fn recover_successor_card_leases_v1(
    source: &StateRootArtifactV2,
    source_activation: &StateRootActivationArtifactV2,
    target: &StateRootArtifactV2,
    before: &LiveStatus,
    after: &LiveStatus,
    java_receipt: &ActionReceipt,
    rust_receipt: Option<&CombatCardBirthReceipt>,
    combat_sequence: u64,
) -> Result<Vec<JavaCardLeaseV1>, String> {
    if source.root.card_bindings.iter().any(|binding| {
        !target
            .root
            .card_bindings
            .iter()
            .any(|target_binding| target_binding == binding)
    }) {
        return Err("recovery target dropped a source card binding".to_owned());
    }
    let new_bindings = target
        .root
        .card_bindings
        .iter()
        .filter(|binding| {
            !source
                .root
                .card_bindings
                .iter()
                .any(|source_binding| source_binding.binding_id == binding.binding_id)
        })
        .collect::<Vec<_>>();
    let reconciled = reconcile_card_birth_transition_v1(
        source,
        source_activation,
        before,
        after,
        java_receipt,
        rust_receipt,
        combat_sequence,
    )?;
    if new_bindings.len() != reconciled.products.len() {
        return Err(format!(
            "recovery target has {} new bindings but transition reconciled {} births",
            new_bindings.len(),
            reconciled.products.len()
        ));
    }
    let mut leases = source_activation.activation.card_leases.clone();
    for product in reconciled.products {
        let projected_lineage =
            project_runtime_lineage_v1(source, product.rust_card.lineage, combat_sequence)?;
        let matches = new_bindings
            .iter()
            .filter(|binding| {
                binding.rust_lineage == projected_lineage
                    && binding.card_id == java_card_id(product.rust_card.id)
                    && binding.upgrades == product.rust_card.upgrades
                    && matches!(
                        binding.evidence,
                        CardBindingEvidenceV1::CapturedBirth {
                            generation_index,
                            ..
                        } if generation_index == u64::from(product.generation_index)
                    )
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "replayed card birth resolves to {} stable target bindings instead of one",
                matches.len()
            ));
        }
        leases.push(JavaCardLeaseV1 {
            binding_id: matches[0].binding_id.clone(),
            java_card_uuid: product.java_uuid,
        });
    }
    leases.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    if leases.len() != target.root.card_bindings.len() {
        return Err("recovery activation lease count differs from target bindings".to_owned());
    }
    Ok(leases)
}

fn project_persistent_rust_lineage_v1(
    source: &StateRootArtifactV2,
    lineage: CardLineageId,
) -> Result<RustCardLineageV1, String> {
    match lineage.origin() {
        CardLineageOrigin::MasterDeck => Ok(RustCardLineageV1::MasterDeck {
            acquisition_ordinal: u64::from(lineage.ordinal()),
        }),
        CardLineageOrigin::ConstructedScenario => Ok(RustCardLineageV1::ConstructedScenarioInstance {
            scenario_instance_id: root_constructed_scenario_instance_id(source)?.ok_or_else(|| {
                "constructed persistent lineage has no root-owned scenario instance identity".to_owned()
            })?,
            slot: u64::from(lineage.ordinal()),
        }),
        CardLineageOrigin::CombatGenerated => {
            Err("persistent master deck contains combat-generated lineage".to_owned())
        }
    }
}

fn build_persistent_java_card_leases_v1(
    source: &StateRootArtifactV2,
    live: &LiveStatus,
    session: &RunControlSession,
    bindings: &[CrossRuntimeCardBindingV1],
) -> Result<Vec<JavaCardLeaseV1>, String> {
    let java_master = live.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "persistent Java activation omitted its master deck".to_owned())?;
    let rust_master = &session.run_state.master_deck;
    if java_master.len() != rust_master.len() || bindings.len() != rust_master.len() {
        return Err("persistent activation deck/binding cardinality differs".to_owned());
    }

    let mut leases = Vec::with_capacity(bindings.len());
    let mut seen_bindings = BTreeSet::new();
    let mut seen_uuids = BTreeSet::new();
    for (ordinal, (java, rust)) in java_master.iter().zip(rust_master).enumerate() {
        let rust_lineage = project_persistent_rust_lineage_v1(source, rust.lineage)?;
        let matches = bindings
            .iter()
            .filter(|binding| binding.rust_lineage == rust_lineage)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "persistent master-deck ordinal {ordinal} resolves to {} bindings instead of one",
                matches.len()
            ));
        }
        let binding = matches[0];
        if java["ordinal"].as_u64() != Some(ordinal as u64)
            || java["card_id"].as_str() != Some(binding.card_id.as_str())
            || java["upgrades"].as_u64() != Some(u64::from(binding.upgrades))
            || java_card_id(rust.id) != binding.card_id
            || rust.upgrades != binding.upgrades
            || !seen_bindings.insert(binding.binding_id.as_str())
        {
            return Err(format!(
                "persistent activation source evidence changed at master-deck ordinal {ordinal}"
            ));
        }
        let java_card_uuid = java["card_uuid"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("persistent Java activation card {ordinal} omitted its runtime UUID")
            })?;
        if !seen_uuids.insert(java_card_uuid) {
            return Err("persistent Java activation reused one UUID".to_owned());
        }
        leases.push(JavaCardLeaseV1 {
            binding_id: binding.binding_id.clone(),
            java_card_uuid: java_card_uuid.to_owned(),
        });
    }
    leases.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    Ok(leases)
}

fn build_java_card_leases(
    live: &LiveStatus,
    session: &RunControlSession,
    bindings: &[CrossRuntimeCardBindingV1],
) -> Result<Vec<JavaCardLeaseV1>, String> {
    let java_master = live.frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "Java activation omitted parity_internal.master_deck".to_owned())?;
    let rust_master = &session.run_state.master_deck;
    let mut leases = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let CardBindingEvidenceV1::MasterDeckObservation {
            java_master_deck_ordinal,
            rust_master_deck_ordinal,
            ..
        } = &binding.evidence
        else {
            return Err(
                "this PairedRoot slice cannot activate a non-master-deck card binding".to_owned(),
            );
        };
        let java_ordinal = usize::try_from(*java_master_deck_ordinal)
            .map_err(|_| "Java master-deck binding ordinal exceeds this platform".to_owned())?;
        let rust_ordinal = usize::try_from(*rust_master_deck_ordinal)
            .map_err(|_| "Rust master-deck binding ordinal exceeds this platform".to_owned())?;
        let java = java_master.get(java_ordinal).ok_or_else(|| {
            format!("Java activation omits bound master-deck ordinal {java_ordinal}")
        })?;
        let rust = rust_master.get(rust_ordinal).ok_or_else(|| {
            format!("Rust activation omits bound master-deck ordinal {rust_ordinal}")
        })?;
        if java["ordinal"].as_u64() != Some(*java_master_deck_ordinal)
            || java["card_id"].as_str() != Some(binding.card_id.as_str())
            || java["upgrades"].as_u64() != Some(u64::from(binding.upgrades))
            || java_card_id(rust.id) != binding.card_id
            || rust.upgrades != binding.upgrades
            || project_rust_lineage(rust.lineage)? != binding.rust_lineage
        {
            return Err(format!(
                "activation source evidence changed for binding {}",
                binding.binding_id
            ));
        }
        let java_card_uuid = java["card_uuid"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                format!("Java activation card {java_ordinal} omitted its runtime UUID")
            })?;
        leases.push(JavaCardLeaseV1 {
            binding_id: binding.binding_id.clone(),
            java_card_uuid: java_card_uuid.to_owned(),
        });
    }
    leases.sort_by(|left, right| left.binding_id.cmp(&right.binding_id));
    Ok(leases)
}

fn project_rust_lineage(lineage: CardLineageId) -> Result<RustCardLineageV1, String> {
    match lineage.origin() {
        CardLineageOrigin::MasterDeck => Ok(RustCardLineageV1::MasterDeck {
            acquisition_ordinal: u64::from(lineage.ordinal()),
        }),
        CardLineageOrigin::CombatGenerated => {
            Err("a natural PairedRoot master deck contains combat-generated lineage".to_owned())
        }
        CardLineageOrigin::ConstructedScenario => {
            Err("a natural PairedRoot master deck contains constructed-scenario lineage".to_owned())
        }
    }
}

fn rust_lineage_key(lineage: &RustCardLineageV1) -> String {
    match lineage {
        RustCardLineageV1::MasterDeck {
            acquisition_ordinal,
        } => format!("master_deck:{acquisition_ordinal}"),
        RustCardLineageV1::CombatGenerated {
            combat_sequence,
            generation_ordinal,
        } => format!("combat_generated:{combat_sequence}:{generation_ordinal}"),
        RustCardLineageV1::ConstructedScenario { recipe_id, slot } => {
            format!("constructed_scenario_legacy:{recipe_id}:{slot}")
        }
        RustCardLineageV1::ConstructedScenarioInstance {
            scenario_instance_id,
            slot,
        } => {
            format!("constructed_scenario_instance:{scenario_instance_id}:{slot}")
        }
    }
}

fn runtime_rust_lineage_key(
    lineage: CardLineageId,
    combat_sequence: u64,
    bindings: &BTreeMap<String, &str>,
) -> Result<String, String> {
    match lineage.origin() {
        CardLineageOrigin::MasterDeck => Ok(format!("master_deck:{}", lineage.ordinal())),
        CardLineageOrigin::CombatGenerated => Ok(format!(
            "combat_generated:{combat_sequence}:{}",
            lineage.ordinal()
        )),
        CardLineageOrigin::ConstructedScenario => {
            let suffix = format!(":{}", lineage.ordinal());
            let matches = bindings
                .keys()
                .filter(|key| key.starts_with("constructed_scenario") && key.ends_with(&suffix))
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [key] => Ok((*key).clone()),
                _ => Err(format!(
                    "constructed scenario slot {} resolves to {} root recipe identities",
                    lineage.ordinal(),
                    matches.len()
                )),
            }
        }
    }
}

fn root_constructed_scenario_instance_id(
    root: &StateRootArtifactV2,
) -> Result<Option<String>, String> {
    let mut scenario_instance_ids =
        root.root
            .card_bindings
            .iter()
            .filter_map(|binding| match &binding.rust_lineage {
                RustCardLineageV1::ConstructedScenarioInstance {
                    scenario_instance_id,
                    ..
                } => Some(scenario_instance_id.as_str()),
                RustCardLineageV1::ConstructedScenario { recipe_id, .. } => {
                    Some(recipe_id.as_str())
                }
                _ => None,
            });
    let Some(first) = scenario_instance_ids.next() else {
        return Ok(None);
    };
    if first.is_empty() || scenario_instance_ids.any(|candidate| candidate != first) {
        return Err(
            "paired root contains inconsistent constructed scenario instance identities".to_owned(),
        );
    }
    for binding in &root.root.card_bindings {
        match &binding.rust_lineage {
            RustCardLineageV1::ConstructedScenarioInstance {
                scenario_instance_id,
                slot,
            } => {
                if scenario_instance_id != first
                    || !matches!(
                        &binding.evidence,
                        CardBindingEvidenceV1::ConstructedScenario {
                            scenario_instance_id: evidence_scenario,
                            slot: evidence_slot,
                            ..
                        } if evidence_scenario == scenario_instance_id && evidence_slot == slot
                    )
                {
                    return Err(
                        "constructed lineage is not bound to matching scenario evidence".to_owned(),
                    );
                }
            }
            RustCardLineageV1::ConstructedScenario { recipe_id, slot } => {
                if recipe_id != first
                    || !matches!(
                        &binding.evidence,
                        CardBindingEvidenceV1::ConstructedRecipe {
                            recipe_id: evidence_recipe,
                            slot: evidence_slot,
                            ..
                        } if evidence_recipe == recipe_id && evidence_slot == slot
                    )
                {
                    return Err(
                        "legacy constructed lineage is not bound to matching recipe evidence"
                            .to_owned(),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(Some(first.to_owned()))
}

fn compare_projection(
    store: &StateRootStoreV2,
    domain: ProjectionDomainV1,
    schema: &str,
    java: Value,
    rust: Value,
) -> Result<PreparedProjectionV1, String> {
    let first_difference = first_json_difference("$", &java, &rust);
    Ok(PreparedProjectionV1 {
        comparison: ProjectionComparisonV1 {
            domain,
            projection_schema: schema.to_owned(),
            java: put_json(store, &java)?,
            rust: put_json(store, &rust)?,
            status: if first_difference.is_some() {
                ComparisonStatusV1::Diverged
            } else {
                ComparisonStatusV1::Exact
            },
            first_difference,
        },
    })
}

fn relational_binding_projection(
    store: &StateRootStoreV2,
    bindings: &[CrossRuntimeCardBindingV1],
    leases: &[JavaCardLeaseV1],
    frame: &Value,
    session: &RunControlSession,
) -> Result<PreparedProjectionV1, String> {
    let java_master = frame["parity_internal"]["master_deck"]
        .as_array()
        .ok_or_else(|| "Java root omitted parity_internal.master_deck".to_owned())?;
    let by_uuid = leases
        .iter()
        .map(|lease| (lease.java_card_uuid.as_str(), lease.binding_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let by_lineage = bindings
        .iter()
        .map(|binding| {
            (
                rust_lineage_key(&binding.rust_lineage),
                binding.binding_id.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let java = java_master
        .iter()
        .map(|card| {
            let uuid = required_str(card, "card_uuid")?;
            Ok(json!({
                "binding_id": by_uuid.get(uuid).copied().ok_or_else(|| format!("unbound Java master-deck UUID {uuid}"))?,
                "ordinal": required_u64(card, "ordinal")?,
                "card_id": required_str(card, "card_id")?,
                "upgrades": required_u64(card, "upgrades")?,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let combat_sequence = session.combat_sequence();
    let rust = session
        .run_state
        .master_deck
        .iter()
        .enumerate()
        .map(|(ordinal, card)| {
            let lineage = runtime_rust_lineage_key(card.lineage, combat_sequence, &by_lineage)?;
            Ok(json!({
                "binding_id": by_lineage.get(&lineage).copied().ok_or_else(|| format!("unbound Rust master-deck lineage {lineage}"))?,
                "ordinal": ordinal,
                "card_id": java_card_id(card.id),
                "upgrades": card.upgrades,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    compare_projection(
        store,
        ProjectionDomainV1::CardBindings,
        "sts-paired-master-deck-binding-relation-v1",
        Value::Array(java),
        Value::Array(rust),
    )
}

fn rust_run_scalars(session: &RunControlSession) -> Result<Value, String> {
    let run = &session.run_state;
    let player = &active_combat(session)?.entities.player;
    Ok(json!({
        "seed": run.seed.to_string(),
        "ascension": run.ascension_level,
        "character": rust_character_id(run.player_class),
        "act": run.act_num,
        "floor": run.floor_num,
        "current_hp": player.current_hp,
        "max_hp": player.max_hp,
        "gold": player.gold,
        "final_act_available": run.is_final_act_available,
        "keys": run.keys,
    }))
}

fn rust_persistent_run_scalars(session: &RunControlSession) -> Value {
    let run = &session.run_state;
    json!({
        "seed": run.seed.to_string(),
        "ascension": run.ascension_level,
        "character": rust_character_id(run.player_class),
        "act": run.act_num,
        "floor": run.floor_num,
        "current_hp": run.current_hp,
        "max_hp": run.max_hp,
        "gold": run.gold,
        "final_act_available": run.is_final_act_available,
        "keys": run.keys,
    })
}

fn rust_master_deck_semantics(session: &RunControlSession) -> Value {
    Value::Array(
        session
            .run_state
            .master_deck
            .iter()
            .enumerate()
            .map(|(ordinal, card)| {
                json!({
                    "ordinal": ordinal,
                    "card_id": java_card_id(card.id),
                    "upgrades": card.upgrades,
                })
            })
            .collect(),
    )
}

fn rust_combat_relics(session: &RunControlSession) -> Result<Value, String> {
    rust_relic_state_projection(
        &active_combat(session)?.entities.player.relics,
        "combat player",
    )
}

fn rust_persistent_relics(session: &RunControlSession) -> Result<Value, String> {
    rust_relic_state_projection(&session.run_state.relics, "persistent run")
}

fn rust_relic_state_projection(relics: &[RelicState], scope: &str) -> Result<Value, String> {
    if relics
        .iter()
        .any(|relic| relic.amount != 0 || (relic.used_up && relic.id != RelicId::NeowsLament))
    {
        return Err(format!(
            "PairedRoot {scope} relic projection cannot hide non-default amount or unsupported used_up state"
        ));
    }
    Ok(Value::Array(
        relics
            .iter()
            .enumerate()
            .map(|(ordinal, relic)| {
                let mut projected = json!({
                    "ordinal": ordinal,
                    "id": canonical_game_id(relic.id.stable_id()),
                    "counter": relic.counter,
                });
                if relic.used_up {
                    projected["used_up"] = Value::Bool(true);
                }
                projected
            })
            .collect(),
    ))
}

fn java_potions(frame: &Value) -> Result<Value, String> {
    let potions = frame["public_base"]["combat"]["potions"]
        .as_array()
        .ok_or_else(|| "Java combat frame omitted potion slots".to_owned())?;
    Ok(Value::Array(
        potions
            .iter()
            .filter_map(|potion| {
                let slot = required_u64(potion, "slot_index");
                let empty = required_bool(potion, "empty");
                match (slot, empty) {
                    (Ok(_), Ok(true)) => None,
                    (Ok(slot), Ok(false)) => Some((|| {
                        let potion_slot = required_u64(potion, "potion_slot")?;
                        if potion_slot != slot {
                            return Err(format!(
                                "occupied Java potion slot index {slot} disagrees with potion slot {potion_slot}"
                            ));
                        }
                        Ok(json!({
                    "slot": slot,
                    "id": canonical_potion_id(required_str(potion, "potion_id")?),
                    "can_use": required_bool(potion, "can_use")?,
                    "can_discard": required_bool(potion, "can_discard")?,
                    "target_required": required_bool(potion, "target_required")?,
                        }))
                    })()),
                    (Err(error), _) | (_, Err(error)) => Some(Err(error)),
                }
            })
            .collect::<Result<Vec<_>, String>>()?,
    ))
}

fn java_has_empty_potion_slot(live: &LiveStatus) -> Result<bool, String> {
    let potions = java_persistent_potions(&live.frame)?;
    Ok(potions
        .as_array()
        .expect("java_persistent_potions always returns an array")
        .iter()
        .any(|slot| slot["id"].is_null()))
}

fn rust_potions(session: &RunControlSession) -> Result<Value, String> {
    let combat = active_combat(session)?;
    Ok(Value::Array(
        combat
            .entities
            .potions
            .iter()
            .enumerate()
            .filter_map(|(slot, potion)| {
                potion.as_ref().map(|potion| {
                    json!({
                        "slot": slot,
                        "id": canonical_potion_id(potion.id.stable_id()),
                        "can_use": potion.can_use,
                        "can_discard": potion.can_discard,
                        "target_required": potion.requires_target,
                    })
                })
            })
            .collect(),
    ))
}

fn rust_persistent_potions(session: &RunControlSession) -> Value {
    Value::Array(
        session
            .run_state
            .potions
            .iter()
            .enumerate()
            .map(|(slot, potion)| {
                json!({
                    "slot": slot,
                    "id": potion
                        .as_ref()
                        .map(|potion| canonical_potion_id(potion.id.stable_id())),
                })
            })
            .collect(),
    )
}

fn rust_potion_potency(combat: &CombatState, potion_id: PotionId) -> i32 {
    let multiplier = if combat.entities.player.has_relic(RelicId::SacredBark) {
        2
    } else {
        1
    };
    get_potion_definition(potion_id).base_potency * multiplier
}

fn rust_rng_streams(session: &RunControlSession) -> Result<Value, String> {
    let combat = active_combat(session)?;
    Ok(rust_rng_streams_from_pool(session, &combat.rng.pool))
}

fn rust_persistent_rng_streams(session: &RunControlSession) -> Value {
    rust_rng_streams_from_pool(session, &session.run_state.rng_pool)
}

fn rust_neow_rng_streams(session: &RunControlSession) -> Result<Value, String> {
    let event = session
        .run_state
        .event_state
        .as_ref()
        .filter(|event| event.id == EventId::Neow)
        .ok_or_else(|| "Rust event room omitted its Neow state".to_owned())?;
    let Value::Array(mut streams) = rust_persistent_rng_streams(session) else {
        unreachable!("Rust RNG projection is always an array")
    };
    streams.retain(|stream| stream["owner"].as_str() != Some(JAVA_MISC_RNG_ID));
    match event.current_screen {
        1 => streams.push(rng_projection(
            JAVA_NEOW_RNG_ID,
            "run",
            session
                .run_state
                .neow_rng
                .as_ref()
                .ok_or_else(|| "Rust Rich Neow reward page omitted its RNG".to_owned())?,
        )),
        2 => {}
        screen => {
            return Err(format!(
                "Rust Neow screen {screen} is outside the stable profile"
            ))
        }
    }
    streams.sort_by(|left, right| left["owner"].as_str().cmp(&right["owner"].as_str()));
    Ok(Value::Array(streams))
}

fn rust_rng_streams_from_pool(session: &RunControlSession, pool: &RngPool) -> Value {
    let mut streams = vec![
        rng_projection(
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.aiRng",
            "floor",
            &pool.ai_rng,
        ),
        rng_projection(
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.shuffleRng",
            "floor",
            &pool.shuffle_rng,
        ),
        rng_projection(
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRandomRng",
            "floor",
            &pool.card_random_rng,
        ),
        rng_projection(
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.miscRng",
            "floor",
            &pool.misc_rng,
        ),
        rng_projection(
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.monsterHpRng",
            "floor",
            &pool.monster_hp_rng,
        ),
        rng_projection(
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.potionRng",
            "run",
            &pool.potion_rng,
        ),
        rng_projection(
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng",
            "run",
            &session.run_state.map_rng,
        ),
        rng_projection(
            "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRng",
            "run",
            &session.run_state.rng_pool.card_rng,
        ),
    ];
    // Rust retains the historical NeowEvent.rng value outside this pool, but
    // no post-Neow transition consumes it. The matching Java projection drops
    // the still-visible static field for the same future-capability reason.
    streams.sort_by(|left, right| left["owner"].as_str().cmp(&right["owner"].as_str()));
    Value::Array(streams)
}

fn rng_projection(owner: &str, scope: &str, rng: &StsRng) -> Value {
    json!({
        "owner": owner,
        "scope": scope,
        "state_hex": [format!("{:016X}", rng.seed0), format!("{:016X}", rng.seed1)],
        "counter": comparable_rng_counter(owner, i64::from(rng.counter)),
    })
}

fn comparable_rng_counter(owner: &str, counter: i64) -> Value {
    if owner == "com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng" {
        // Map generation passes mapRng.random directly to Collections.shuffle.
        // Those calls advance RandomXS128 but bypass the original game's
        // wrapper counter. Rust intentionally counts the same state advances,
        // so only state_hex is cross-runtime comparable for this owner.
        Value::Null
    } else {
        json!(counter)
    }
}

fn rust_card_pools(session: &RunControlSession) -> Result<Value, String> {
    if session.run_state.player_class != "Ironclad" {
        return Err("first PairedRoot card-pool projection supports Ironclad only".to_owned());
    }
    let ids = |rarity| {
        ironclad_pool_for_rarity(rarity)
            .iter()
            .map(|id| java_card_id(*id).to_owned())
            .collect::<Vec<_>>()
    };
    Ok(json!({
        "common": ids(CardRarity::Common),
        "uncommon": ids(CardRarity::Uncommon),
        "rare": ids(CardRarity::Rare),
        "colorless_combat": sts_oracle_learning::content::cards::random_colorless_in_combat_pool()
            .into_iter()
            .map(|id| java_card_id(id).to_owned())
            .collect::<Vec<_>>(),
    }))
}

fn java_combat(
    frame: &Value,
    bindings: &BTreeMap<&str, &str>,
    version: CombatProjectionVersion,
) -> Result<Value, String> {
    let combat = &frame["public_base"]["combat"];
    let zones = frame["parity_internal"]["zones"]
        .as_object()
        .ok_or_else(|| "Java frame omitted parity_internal.zones".to_owned())?;
    let cards = |name: &str, reverse: bool| -> Result<Value, String> {
        let mut values = zones
            .get(name)
            .and_then(|zone| zone["cards"].as_array())
            .ok_or_else(|| format!("Java frame omitted {name} card zone"))?
            .iter()
            .map(|card| java_bound_card(card, bindings))
            .collect::<Result<Vec<_>, _>>()?;
        if reverse {
            values.reverse();
        }
        Ok(Value::Array(values))
    };
    let public_monsters = combat["monsters"]
        .as_array()
        .ok_or_else(|| "Java frame omitted public combat monsters".to_owned())?;
    let internal_monsters = frame["parity_internal"]["monsters"]
        .as_array()
        .ok_or_else(|| "Java frame omitted internal combat monsters".to_owned())?;
    if public_monsters.len() != internal_monsters.len() {
        return Err("Java public/internal monster cardinality differs".to_owned());
    }
    let monsters = public_monsters
        .iter()
        .zip(internal_monsters)
        .map(|(public, internal)| {
            let mut projected = json!({
                "id": canonical_game_id(required_str(public, "id")?),
                "hp": required_i64(public, "current_hp")?,
                "max_hp": required_i64(public, "max_hp")?,
                "block": required_i64(public, "block")?,
                "dead_or_escaped": required_bool(public, "dead_or_escaped")?,
                "next_move_id": required_i64(internal, "next_move_id")?,
                "intent": canonical_game_id(required_str(internal, "intent")?),
                "move_history": internal["move_history"].clone(),
                "powers": java_powers(&internal["powers"])?
            });
            if version == CombatProjectionVersion::PublicIntentV2 {
                projected["public_intent"] = java_public_intent(public)?;
            }
            Ok(projected)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({
        "turn_index": required_u64(combat, "turn")?.saturating_sub(1),
        "player": {
            "current_hp": required_i64(&combat["player"], "current_hp")?,
            "max_hp": required_i64(&combat["player"], "max_hp")?,
            "block": required_i64(&combat["player"], "block")?,
            "energy": required_i64(&combat["player"], "energy")?,
            "powers": java_powers(&frame["parity_internal"]["player_powers"])?
        },
        "zones": {
            "draw_top_first": cards("draw", true)?,
            "hand": cards("hand", false)?,
            "discard_bottom_first": cards("discard", false)?,
            "exhaust_bottom_first": cards("exhaust", false)?,
            "limbo": cards("limbo", false)?,
        },
        "monsters": monsters,
    }))
}

fn rust_combat(
    session: &RunControlSession,
    bindings: &BTreeMap<String, &str>,
    version: CombatProjectionVersion,
) -> Result<Value, String> {
    let combat = active_combat(session)?;
    let combat_sequence = session.combat_sequence();
    let cards = |source: Vec<&CombatCard>| -> Result<Value, String> {
        Ok(Value::Array(
            source
                .into_iter()
                .map(|card| rust_bound_card(card, combat_sequence, bindings))
                .collect::<Result<Vec<_>, _>>()?,
        ))
    };
    let monsters = combat
        .entities
        .monsters
        .iter()
        .map(|monster| {
            let intent = serde_json::to_value(
                sts_oracle_learning::sim::combat_projection::project_monster_move_preview_in_combat(
                    combat, monster,
                )
                .visible_intent,
            )
            .map_err(|error| format!("failed to project Rust monster intent: {error}"))?
            .as_str()
            .ok_or_else(|| "Rust monster intent did not serialize as a typed token".to_owned())?
            .to_owned();
            let mut projected = json!({
                "id": canonical_game_id(monster.enemy_id.stable_id()),
                "hp": monster.current_hp,
                "max_hp": monster.max_hp,
                "block": monster.block,
                "dead_or_escaped": monster.is_dead_or_escaped(),
                "next_move_id": monster.planned_move_id(),
                "intent": canonical_game_id(&intent),
                "move_history": monster.move_history(),
                "powers": rust_powers(combat.entities.power_db.get(&monster.id).map(Vec::as_slice).unwrap_or(&[])),
            });
            if version == CombatProjectionVersion::PublicIntentV2 {
                projected["public_intent"] = rust_public_intent(combat, monster.id)?;
            }
            Ok(projected)
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({
        "turn_index": combat.turn.turn_count,
        "player": {
            "current_hp": combat.entities.player.current_hp,
            "max_hp": combat.entities.player.max_hp,
            "block": combat.entities.player.block,
            "energy": combat.turn.energy,
            "powers": rust_powers(combat.entities.power_db.get(&combat.entities.player.id).map(Vec::as_slice).unwrap_or(&[])),
        },
        "zones": {
            "draw_top_first": cards(combat.zones.draw_pile.iter().collect())?,
            "hand": cards(combat.zones.hand.iter().collect())?,
            "discard_bottom_first": cards(combat.zones.discard_pile.iter().collect())?,
            "exhaust_bottom_first": cards(combat.zones.exhaust_pile.iter().collect())?,
            "limbo": cards(combat.zones.limbo.iter().collect())?,
        },
        "monsters": monsters,
    }))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "evidence", rename_all = "snake_case", deny_unknown_fields)]
enum CombatPublicIntentProjectionV2 {
    VisibleExact {
        visible_intent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        damage_per_hit: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hits: Option<u8>,
    },
    Hidden {
        hidden_reason: HiddenInformationReasonV1,
    },
}

impl CombatPublicIntentProjectionV2 {
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::VisibleExact {
                visible_intent,
                damage_per_hit,
                hits,
            } => {
                if visible_intent.is_empty() {
                    return Err("visible public intent omitted its kind".to_owned());
                }
                if damage_per_hit.is_some() != hits.is_some() {
                    return Err(
                        "public intent damage_per_hit and hits have different presence".to_owned(),
                    );
                }
            }
            Self::Hidden { hidden_reason }
                if !matches!(
                    hidden_reason,
                    HiddenInformationReasonV1::RunicDome
                        | HiddenInformationReasonV1::IntentNotVisible
                ) =>
            {
                return Err("public intent used an unrelated hidden reason".to_owned());
            }
            Self::Hidden { .. } => {}
        }
        Ok(())
    }
}

fn java_public_intent(monster: &Value) -> Result<Value, String> {
    let mut intent: CombatPublicIntentProjectionV2 = serde_json::from_value(
        monster
            .get("public_intent")
            .cloned()
            .ok_or_else(|| "Java public combat monster omitted public_intent".to_owned())?,
    )
    .map_err(|error| format!("failed to decode Java public intent: {error}"))?;
    if let CombatPublicIntentProjectionV2::VisibleExact { visible_intent, .. } = &mut intent {
        visible_intent.make_ascii_lowercase();
    }
    intent.validate()?;
    serde_json::to_value(intent)
        .map_err(|error| format!("failed to encode Java public intent: {error}"))
}

fn rust_public_intent(combat: &CombatState, monster_id: usize) -> Result<Value, String> {
    let facts = combat_public_intent_facts_v1(combat, monster_id);
    let intent = match facts.evidence {
        ObservationEvidenceKindV1::VisibleExact => {
            let visible = facts
                .intent
                .ok_or_else(|| "visible Rust public intent omitted its kind".to_owned())?;
            let hits = match &visible {
                Intent::Attack { hits, .. }
                | Intent::AttackBuff { hits, .. }
                | Intent::AttackDebuff { hits, .. }
                | Intent::AttackDefend { hits, .. } => Some(*hits),
                _ => None,
            };
            CombatPublicIntentProjectionV2::VisibleExact {
                visible_intent: visible.kind_v1().to_owned(),
                damage_per_hit: facts.preview_damage_per_hit,
                hits,
            }
        }
        ObservationEvidenceKindV1::Hidden => CombatPublicIntentProjectionV2::Hidden {
            hidden_reason: facts
                .hidden_reason
                .ok_or_else(|| "hidden Rust public intent omitted its reason".to_owned())?,
        },
        other => {
            return Err(format!(
                "Rust public intent used unsupported observation evidence {other:?}"
            ));
        }
    };
    intent.validate()?;
    serde_json::to_value(intent)
        .map_err(|error| format!("failed to encode Rust public intent: {error}"))
}

fn java_bound_card(card: &Value, bindings: &BTreeMap<&str, &str>) -> Result<Value, String> {
    let uuid = required_str(card, "instance_uuid")?;
    Ok(json!({
        "binding_id": bindings.get(uuid).copied().ok_or_else(|| format!("Java combat card UUID {uuid} has no source binding"))?,
        "card_id": required_str(card, "card_id")?,
        "upgrades": required_u64(card, "upgrades")?,
        "cost_for_turn": required_i64(card, "cost_for_turn")?,
    }))
}

fn rust_bound_card(
    card: &CombatCard,
    combat_sequence: u64,
    bindings: &BTreeMap<String, &str>,
) -> Result<Value, String> {
    let lineage = runtime_rust_lineage_key(card.lineage, combat_sequence, bindings)?;
    Ok(json!({
        "binding_id": bindings.get(&lineage).copied().ok_or_else(|| format!("Rust combat card lineage {lineage} has no source binding"))?,
        "card_id": java_card_id(card.id),
        "upgrades": card.upgrades,
        "cost_for_turn": card.cost_for_turn_java(),
    }))
}

fn java_powers(value: &Value) -> Result<Value, String> {
    let powers = value
        .as_array()
        .ok_or_else(|| "Java frame omitted a power list".to_owned())?;
    Ok(Value::Array(
        powers
            .iter()
            .map(|power| {
                Ok(json!({
                    "id": canonical_power_id(required_str(power, "id")?),
                    "amount": required_i64(power, "amount")?,
                }))
            })
            .collect::<Result<Vec<_>, String>>()?,
    ))
}

fn rust_powers(powers: &[Power]) -> Value {
    Value::Array(
        powers
            .iter()
            .map(|power| {
                json!({
                    "id": canonical_power_id(power.power_type.stable_id()),
                    "amount": power.amount,
                })
            })
            .collect(),
    )
}

fn java_action_space(
    live: &LiveStatus,
    bindings: &BTreeMap<&str, &str>,
    version: ActionSpaceProjectionVersion,
) -> Result<Value, String> {
    let monsters = live.frame["public_base"]["combat"]["monsters"]
        .as_array()
        .ok_or_else(|| "Java frame omitted combat monsters".to_owned())?;
    let mut actions = Vec::new();
    for action in &live.actions {
        match action.kind.as_str() {
            "play_card" => {
                let uuid = action
                    .card_uuid
                    .as_deref()
                    .ok_or_else(|| "Java play-card action omitted card UUID".to_owned())?;
                let target = action
                    .target_monster_index
                    .map(|index| {
                        monsters
                            .get(index)
                            .and_then(|monster| monster["id"].as_str())
                            .map(canonical_game_id)
                            .ok_or_else(|| format!("Java action target index {index} is invalid"))
                    })
                    .transpose()?;
                actions.push(json!({
                    "kind": "play_card",
                    "binding_id": bindings.get(uuid).copied().ok_or_else(|| format!("Java action card UUID {uuid} is unbound"))?,
                    "target_index": action.target_monster_index,
                    "target_id": target,
                }));
            }
            "use_potion" if version == ActionSpaceProjectionVersion::PotionV2 => {
                let slot = action
                    .potion_slot
                    .ok_or_else(|| "Java use-potion action omitted its slot".to_owned())?;
                let potion_id = canonical_potion_id(
                    action
                        .potion_id
                        .as_deref()
                        .ok_or_else(|| "Java use-potion action omitted its identity".to_owned())?,
                );
                let potency = action
                    .potion_potency
                    .ok_or_else(|| "Java use-potion action omitted its potency".to_owned())?;
                if potency < 0 {
                    return Err("Java use-potion action has negative potency".to_owned());
                }
                let target = action
                    .target_monster_index
                    .map(|index| {
                        monsters
                            .get(index)
                            .and_then(|monster| monster["id"].as_str())
                            .map(canonical_game_id)
                            .ok_or_else(|| format!("Java potion target index {index} is invalid"))
                    })
                    .transpose()?;
                actions.push(json!({
                    "kind": "use_potion",
                    "slot": slot,
                    "potion_id": potion_id,
                    "potency": potency,
                    "target_index": action.target_monster_index,
                    "target_id": target,
                }));
            }
            "end_turn" => actions.push(json!({"kind": "end_turn"})),
            _ => {}
        }
    }
    sort_json_values(&mut actions)?;
    Ok(Value::Array(actions))
}

fn rust_action_space(
    session: &RunControlSession,
    bindings: &BTreeMap<String, &str>,
    version: ActionSpaceProjectionVersion,
) -> Result<Value, String> {
    let combat = active_combat(session)?;
    let combat_sequence = session.combat_sequence();
    let surface = build_decision_surface(session);
    let mut actions = Vec::new();
    for input in surface.visible_executable_inputs {
        match input {
            ClientInput::PlayCard { card_index, target } => {
                let card = combat
                    .zones
                    .hand
                    .get(card_index)
                    .ok_or_else(|| format!("Rust action hand index {card_index} is invalid"))?;
                let lineage = runtime_rust_lineage_key(card.lineage, combat_sequence, bindings)?;
                let target_index = target
                    .map(|entity| {
                        combat
                            .entities
                            .monsters
                            .iter()
                            .position(|monster| monster.id == entity)
                            .ok_or_else(|| format!("Rust action target entity {entity} is absent"))
                    })
                    .transpose()?;
                let target_id = target_index.map(|index| {
                    canonical_game_id(combat.entities.monsters[index].enemy_id.stable_id())
                });
                actions.push(json!({
                    "kind": "play_card",
                    "binding_id": bindings.get(&lineage).copied().ok_or_else(|| format!("Rust action card lineage {lineage} is unbound"))?,
                    "target_index": target_index,
                    "target_id": target_id,
                }));
            }
            ClientInput::UsePotion {
                potion_index,
                target,
            } if version == ActionSpaceProjectionVersion::PotionV2 => {
                let potion = combat
                    .entities
                    .potions
                    .get(potion_index)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| format!("Rust action potion slot {potion_index} is absent"))?;
                let target_index = target
                    .map(|entity| {
                        combat
                            .entities
                            .monsters
                            .iter()
                            .position(|monster| monster.id == entity)
                            .ok_or_else(|| format!("Rust potion target entity {entity} is absent"))
                    })
                    .transpose()?;
                let target_id = target_index.map(|index| {
                    canonical_game_id(combat.entities.monsters[index].enemy_id.stable_id())
                });
                actions.push(json!({
                    "kind": "use_potion",
                    "slot": potion_index,
                    "potion_id": canonical_potion_id(potion.id.stable_id()),
                    "potency": rust_potion_potency(combat, potion.id),
                    "target_index": target_index,
                    "target_id": target_id,
                }));
            }
            ClientInput::EndTurn => actions.push(json!({"kind": "end_turn"})),
            _ => {}
        }
    }
    sort_json_values(&mut actions)?;
    Ok(Value::Array(actions))
}

fn java_event_identity(live: &LiveStatus) -> Result<(EventId, usize), String> {
    let decision = &live.frame["public_base"]["decision"];
    if decision["kind"].as_str() != Some("event") {
        return Err("Java frame is not a typed event decision".to_owned());
    }
    let identity = &decision["event_identity"];
    if identity["status"].as_str() != Some("supported") {
        return Err("Java event identity is not a supported base-game identity".to_owned());
    }
    if identity["runtime_class"]
        .as_str()
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return Err("Java event identity omitted its runtime diagnostic".to_owned());
    }
    let raw_event_id = decision["event_id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Java event decision omitted its event identity".to_owned())?;
    if identity["event_id"].as_str() != Some(raw_event_id) {
        return Err("Java event decision and identity diagnostic disagree".to_owned());
    }
    let event_id = EventId::from_stable_id(raw_event_id)
        .ok_or_else(|| format!("Java event identity {raw_event_id:?} is unsupported"))?;
    if event_id == EventId::Neow {
        return Err("generic Event profile cannot claim the Neow event".to_owned());
    }
    let screen = decision["event_screen_index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "Java event decision omitted a non-negative screen index".to_owned())?;
    Ok((event_id, screen))
}

fn rust_event_identity(session: &RunControlSession) -> Result<(EventId, usize), String> {
    if !matches!(session.engine_state, EngineState::EventRoom) {
        return Err("Rust root is not a stable event-room decision".to_owned());
    }
    let event = session
        .run_state
        .event_state
        .as_ref()
        .ok_or_else(|| "Rust event room omitted its event state".to_owned())?;
    if event.id == EventId::Neow {
        return Err("generic Event profile cannot claim the Neow event".to_owned());
    }
    Ok((event.id, event.current_screen))
}

fn java_event_room(live: &LiveStatus) -> Result<Value, String> {
    let (event_id, screen) = java_event_identity(live)?;
    Ok(json!({
        "kind": "event",
        "event_id": event_id.stable_id(),
        "screen": screen,
    }))
}

fn rust_event_room(session: &RunControlSession) -> Result<Value, String> {
    let (event_id, screen) = rust_event_identity(session)?;
    Ok(json!({
        "kind": "event",
        "event_id": event_id.stable_id(),
        "screen": screen,
    }))
}

fn java_event_action_space(live: &LiveStatus) -> Result<Value, String> {
    let (event_id, screen) = java_event_identity(live)?;
    let mut option_indices = BTreeSet::new();
    let mut values = live
        .actions
        .iter()
        .filter(|action| action.kind == "choose_event_option")
        .map(|action| {
            let raw_action_event = action
                .event_id
                .as_deref()
                .ok_or_else(|| "Java event action omitted its event identity".to_owned())?;
            let action_event = EventId::from_stable_id(raw_action_event).ok_or_else(|| {
                format!("Java event action identity {raw_action_event:?} is unsupported")
            })?;
            let option_index = action
                .option_index
                .ok_or_else(|| "Java event action omitted its option index".to_owned())?;
            if action_event != event_id || !option_indices.insert(option_index) {
                return Err(
                    "Java event action space changed event identity or repeated an option"
                        .to_owned(),
                );
            }
            serde_json::to_value(StoredActionProjectionV1::ChooseEventOption {
                event_id: event_id.stable_id().to_owned(),
                screen,
                option_index,
            })
            .map_err(|error| format!("failed to encode Java event action: {error}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    if values.is_empty() {
        return Err("Java event action space is empty".to_owned());
    }
    sort_json_values(&mut values)?;
    Ok(Value::Array(values))
}

fn rust_event_action_space(session: &RunControlSession) -> Result<Value, String> {
    let (event_id, screen) = rust_event_identity(session)?;
    let mut option_indices = BTreeSet::new();
    let mut values = Vec::new();
    for candidate in &build_decision_surface(session).view.candidates {
        let Some(DecisionCandidateKey::EventOption {
            event_id: candidate_event,
            screen: candidate_screen,
            option_index,
            ..
        }) = candidate.key.as_ref()
        else {
            continue;
        };
        if candidate.action.executable_action_ref().is_none() {
            continue;
        }
        if *candidate_event != event_id
            || *candidate_screen != screen
            || !option_indices.insert(*option_index)
        {
            return Err(
                "Rust event action space changed event identity or repeated an option".to_owned(),
            );
        }
        values.push(
            serde_json::to_value(StoredActionProjectionV1::ChooseEventOption {
                event_id: event_id.stable_id().to_owned(),
                screen,
                option_index: *option_index,
            })
            .map_err(|error| format!("failed to encode Rust event action: {error}"))?,
        );
    }
    if values.is_empty() {
        return Err("Rust event action space is empty".to_owned());
    }
    sort_json_values(&mut values)?;
    Ok(Value::Array(values))
}

fn event_runtime_settlement() -> Value {
    json!({
        "native_autosave": "may_differ_from_native_root",
        "resident_completion": "retire",
    })
}

const NEOW_EVENT_ID: &str = "Neow";
const JAVA_NEOW_RNG_ID: &str = "com.megacrit.cardcrawl.neow.NeowEvent.rng";
const JAVA_MISC_RNG_ID: &str = "com.megacrit.cardcrawl.dungeons.AbstractDungeon.miscRng";

fn java_neow_screen(live: &LiveStatus) -> Result<i64, String> {
    let decision = &live.frame["public_base"]["decision"];
    if decision["kind"].as_str() != Some("event")
        || decision["event_id"].as_str() != Some(NEOW_EVENT_ID)
    {
        return Err("Java frame is not a typed Neow event decision".to_owned());
    }
    decision["event_screen_index"]
        .as_i64()
        .ok_or_else(|| "Java Neow decision omitted its screen index".to_owned())
}

fn rust_neow_room(session: &RunControlSession) -> Result<Value, String> {
    if !matches!(session.engine_state, EngineState::EventRoom) {
        return Err("Rust Neow root is not an event-room decision".to_owned());
    }
    let event = session
        .run_state
        .event_state
        .as_ref()
        .filter(|event| event.id == EventId::Neow)
        .ok_or_else(|| "Rust event room omitted its Neow state".to_owned())?;
    let stage = match event.current_screen {
        1 => "choose_reward",
        2 => "leave",
        screen => {
            return Err(format!(
                "Rust Neow screen {screen} is outside the stable profile"
            ))
        }
    };
    Ok(json!({"kind": "neow", "stage": stage}))
}

fn rust_neow_action_space(session: &RunControlSession) -> Result<Value, String> {
    if !matches!(session.engine_state, EngineState::EventRoom) {
        return Err("Rust Neow root is not an event-room decision".to_owned());
    }
    let event = session
        .run_state
        .event_state
        .as_ref()
        .filter(|event| event.id == EventId::Neow)
        .ok_or_else(|| "Rust event room omitted its Neow state".to_owned())?;
    let surface = build_decision_surface(session);
    let candidates = surface
        .view
        .candidates
        .iter()
        .filter_map(|candidate| {
            let DecisionCandidateKey::EventOption {
                event_id: EventId::Neow,
                screen,
                option_index,
                action,
            } = candidate.key.as_ref()?
            else {
                return None;
            };
            candidate
                .action
                .executable_action_ref()
                .map(|_| (*screen, *option_index, *action))
        })
        .collect::<Vec<_>>();
    let actions = match event.current_screen {
        1 => {
            if !matches!(candidates.len(), 2 | 4) {
                return Err(format!(
                    "Rust Neow reward screen exposes {} typed actions instead of two or four",
                    candidates.len()
                ));
            }
            let mut identities = BTreeSet::new();
            candidates
                .into_iter()
                .map(|(screen, option_index, _)| {
                    if screen != 1 {
                        return Err("Rust Neow reward candidate reports another screen".to_owned());
                    }
                    let (reward, drawback) = rust_neow_choice_identity(event, option_index)
                        .ok_or_else(|| {
                            "Rust Neow candidate omitted simulator-owned identity".to_owned()
                        })?;
                    let reward = rust_neow_reward_identity(reward);
                    let drawback = rust_neow_drawback_identity(drawback);
                    if !identities.insert((reward, drawback)) {
                        return Err("Rust Neow action space repeats a typed identity".to_owned());
                    }
                    Ok(StoredActionProjectionV1::ChooseNeowOption { reward, drawback })
                })
                .collect::<Result<Vec<_>, String>>()?
        }
        2 => {
            let [(screen, _, action)] = candidates.as_slice() else {
                return Err(format!(
                    "Rust completed Neow screen exposes {} leave actions instead of one",
                    candidates.len()
                ));
            };
            if *screen != 2 || *action != EventActionKind::Leave {
                return Err(
                    "Rust completed Neow candidate is not the typed leave action".to_owned(),
                );
            }
            vec![StoredActionProjectionV1::LeaveNeow]
        }
        other => {
            return Err(format!(
                "Rust Neow screen {other} is outside the stable profile"
            ))
        }
    };
    let mut values = actions
        .into_iter()
        .map(|action| {
            serde_json::to_value(action)
                .map_err(|error| format!("failed to encode Rust Neow action: {error}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    sort_json_values(&mut values)?;
    Ok(Value::Array(values))
}

fn java_neow_reward_identity(value: &str) -> Result<NeowRewardIdentityV1, String> {
    match value {
        "THREE_ENEMY_KILL" => Ok(NeowRewardIdentityV1::ThreeEnemyKill),
        "TEN_PERCENT_HP_BONUS" => Ok(NeowRewardIdentityV1::TenPercentHpBonus),
        "THREE_CARDS" => Ok(NeowRewardIdentityV1::ThreeCards),
        "ONE_RANDOM_RARE_CARD" => Ok(NeowRewardIdentityV1::OneRandomRareCard),
        "REMOVE_CARD" => Ok(NeowRewardIdentityV1::RemoveCard),
        "UPGRADE_CARD" => Ok(NeowRewardIdentityV1::UpgradeCard),
        "TRANSFORM_CARD" => Ok(NeowRewardIdentityV1::TransformCard),
        "RANDOM_COLORLESS" => Ok(NeowRewardIdentityV1::RandomColorless),
        "THREE_SMALL_POTIONS" => Ok(NeowRewardIdentityV1::ThreeSmallPotions),
        "RANDOM_COMMON_RELIC" => Ok(NeowRewardIdentityV1::RandomCommonRelic),
        "HUNDRED_GOLD" => Ok(NeowRewardIdentityV1::HundredGold),
        "RANDOM_COLORLESS_2" => Ok(NeowRewardIdentityV1::RandomColorless2),
        "REMOVE_TWO" => Ok(NeowRewardIdentityV1::RemoveTwo),
        "ONE_RARE_RELIC" => Ok(NeowRewardIdentityV1::OneRareRelic),
        "THREE_RARE_CARDS" => Ok(NeowRewardIdentityV1::ThreeRareCards),
        "TWO_FIFTY_GOLD" => Ok(NeowRewardIdentityV1::TwoFiftyGold),
        "TRANSFORM_TWO_CARDS" => Ok(NeowRewardIdentityV1::TransformTwoCards),
        "TWENTY_PERCENT_HP_BONUS" => Ok(NeowRewardIdentityV1::TwentyPercentHpBonus),
        "BOSS_RELIC" => Ok(NeowRewardIdentityV1::BossRelic),
        other => Err(format!("Java Neow reward {other:?} is unsupported")),
    }
}

fn java_neow_drawback_identity(value: &str) -> Result<NeowDrawbackIdentityV1, String> {
    match value {
        "NONE" => Ok(NeowDrawbackIdentityV1::None),
        "TEN_PERCENT_HP_LOSS" => Ok(NeowDrawbackIdentityV1::TenPercentHpLoss),
        "NO_GOLD" => Ok(NeowDrawbackIdentityV1::NoGold),
        "CURSE" => Ok(NeowDrawbackIdentityV1::Curse),
        "PERCENT_DAMAGE" => Ok(NeowDrawbackIdentityV1::PercentDamage),
        other => Err(format!("Java Neow drawback {other:?} is unsupported")),
    }
}

fn rust_neow_reward_identity(value: NeowRewardType) -> NeowRewardIdentityV1 {
    match value {
        NeowRewardType::ThreeEnemyKill => NeowRewardIdentityV1::ThreeEnemyKill,
        NeowRewardType::TenPercentHpBonus => NeowRewardIdentityV1::TenPercentHpBonus,
        NeowRewardType::ThreeCards => NeowRewardIdentityV1::ThreeCards,
        NeowRewardType::OneRandomRareCard => NeowRewardIdentityV1::OneRandomRareCard,
        NeowRewardType::RemoveCard => NeowRewardIdentityV1::RemoveCard,
        NeowRewardType::UpgradeCard => NeowRewardIdentityV1::UpgradeCard,
        NeowRewardType::TransformCard => NeowRewardIdentityV1::TransformCard,
        NeowRewardType::RandomColorless => NeowRewardIdentityV1::RandomColorless,
        NeowRewardType::ThreeSmallPotions => NeowRewardIdentityV1::ThreeSmallPotions,
        NeowRewardType::RandomCommonRelic => NeowRewardIdentityV1::RandomCommonRelic,
        NeowRewardType::HundredGold => NeowRewardIdentityV1::HundredGold,
        NeowRewardType::RandomColorless2 => NeowRewardIdentityV1::RandomColorless2,
        NeowRewardType::RemoveTwo => NeowRewardIdentityV1::RemoveTwo,
        NeowRewardType::OneRareRelic => NeowRewardIdentityV1::OneRareRelic,
        NeowRewardType::ThreeRareCards => NeowRewardIdentityV1::ThreeRareCards,
        NeowRewardType::TwoFiftyGold => NeowRewardIdentityV1::TwoFiftyGold,
        NeowRewardType::TransformTwoCards => NeowRewardIdentityV1::TransformTwoCards,
        NeowRewardType::TwentyPercentHpBonus => NeowRewardIdentityV1::TwentyPercentHpBonus,
        NeowRewardType::BossRelic => NeowRewardIdentityV1::BossRelic,
    }
}

fn rust_neow_drawback_identity(value: NeowDrawback) -> NeowDrawbackIdentityV1 {
    match value {
        NeowDrawback::None => NeowDrawbackIdentityV1::None,
        NeowDrawback::TenPercentHpLoss => NeowDrawbackIdentityV1::TenPercentHpLoss,
        NeowDrawback::NoGold => NeowDrawbackIdentityV1::NoGold,
        NeowDrawback::Curse => NeowDrawbackIdentityV1::Curse,
        NeowDrawback::PercentDamage => NeowDrawbackIdentityV1::PercentDamage,
    }
}

fn java_reward_room(live: &LiveStatus) -> Result<Value, String> {
    let mut indexed = live
        .actions
        .iter()
        .filter(|action| action.kind == "claim_combat_reward")
        .map(|action| {
            Ok((
                action
                    .reward_index
                    .ok_or_else(|| "Java combat reward action omitted its index".to_owned())?,
                java_reward_item_projection(action)?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    indexed.sort_by_key(|(index, _)| *index);
    for (expected, (actual, _)) in indexed.iter().enumerate() {
        if expected != *actual {
            return Err(format!(
                "Java combat rewards are not a contiguous ordered list: expected {expected}, found {actual}"
            ));
        }
    }
    Ok(json!({
        "kind": "combat_reward",
        "items": indexed.into_iter().map(|(_, item)| item).collect::<Vec<_>>(),
        "close_available": live.actions.iter().any(|action| action.kind == "close_combat_reward"),
    }))
}

fn java_reward_item_projection(action: &ActionHandle) -> Result<Value, String> {
    let reward_type = action
        .reward_type
        .as_deref()
        .ok_or_else(|| "Java combat reward action omitted its type".to_owned())?;
    match reward_type {
        "GOLD" | "STOLEN_GOLD" => Ok(json!({
            "kind": reward_type.to_ascii_lowercase(),
            "amount": action.reward_gold.ok_or_else(|| "Java gold reward omitted its amount".to_owned())?,
        })),
        "CARD" => {
            let count = action
                .reward_id
                .as_deref()
                .map(|ids| ids.split(',').filter(|id| !id.is_empty()).count())
                .ok_or_else(|| {
                    "Java card reward omitted its opaque choice identities".to_owned()
                })?;
            Ok(json!({"kind": "card", "choice_count": count}))
        }
        "RELIC" => Ok(json!({
            "kind": "relic",
            "id": canonical_game_id(action.reward_id.as_deref().ok_or_else(|| "Java relic reward omitted its identity".to_owned())?),
        })),
        "POTION" => Ok(json!({
            "kind": "potion",
            "id": canonical_potion_id(action.reward_id.as_deref().ok_or_else(|| "Java potion reward omitted its identity".to_owned())?),
        })),
        "EMERALD_KEY" => Ok(json!({"kind": "emerald_key"})),
        "SAPPHIRE_KEY" => Ok(json!({"kind": "sapphire_key"})),
        other => Err(format!("Java combat reward type {other:?} is unsupported")),
    }
}

fn rust_reward_room(session: &RunControlSession) -> Result<Value, String> {
    let EngineState::RewardScreen(reward) = &session.engine_state else {
        return Err("Rust successor is not an ordinary reward screen".to_owned());
    };
    if reward.pending_card_choice.is_some() || reward.pending_card_reward_index.is_some() {
        return Err("Rust reward profile does not cover an opened card reward".to_owned());
    }
    let items = reward
        .items
        .iter()
        .map(rust_reward_item_projection)
        .collect::<Vec<_>>();
    Ok(json!({
        "kind": "combat_reward",
        "items": items,
        "close_available": reward.skippable,
    }))
}

fn java_card_reward_room(live: &LiveStatus) -> Result<Value, String> {
    let reward = live.frame["public_base"]["decision"]["card_reward"]
        .as_object()
        .ok_or_else(|| "Java card-reward frame omitted typed reward state".to_owned())?;
    let source_kind = reward
        .get("source_kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "Java card reward omitted source_kind".to_owned())?;
    if source_kind != "ordinary_reward" {
        return Err(format!(
            "Java card reward source {source_kind:?} is outside the ordinary reward profile"
        ));
    }
    let cards = reward
        .get("cards")
        .and_then(Value::as_array)
        .ok_or_else(|| "Java card reward omitted cards".to_owned())?
        .iter()
        .enumerate()
        .map(|(expected_index, card)| {
            let reward_index = usize::try_from(required_u64(card, "reward_index")?)
                .map_err(|_| "Java card reward index exceeds usize".to_owned())?;
            if reward_index != expected_index {
                return Err(format!(
                    "Java card reward order is not contiguous: expected {expected_index}, found {reward_index}"
                ));
            }
            Ok(json!({
                "card_id": required_str(card, "card_id")?,
                "upgrades": required_u64(card, "upgrades")?,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({
        "kind": "card_reward",
        "source_kind": source_kind,
        "cards": cards,
        "skip_available": required_bool(&Value::Object(reward.clone()), "skip_available")?,
        "singing_bowl_available": required_bool(&Value::Object(reward.clone()), "singing_bowl_available")?,
    }))
}

fn rust_card_reward_room(session: &RunControlSession) -> Result<Value, String> {
    let EngineState::RewardScreen(reward) = &session.engine_state else {
        return Err("Rust successor is not a card reward screen".to_owned());
    };
    let cards = reward
        .pending_card_choice
        .as_ref()
        .ok_or_else(|| "Rust card reward omitted its pending choices".to_owned())?;
    if reward.pending_card_reward_index.is_none() {
        return Err("Rust ordinary card reward omitted its source item index".to_owned());
    }
    Ok(json!({
        "kind": "card_reward",
        "source_kind": "ordinary_reward",
        "cards": cards.iter().map(|card| json!({
            "card_id": java_card_id(card.id),
            "upgrades": session.run_state.preview_obtain_card_upgrades(card.id, card.upgrades),
        })).collect::<Vec<_>>(),
        "skip_available": true,
        "singing_bowl_available": session.run_state.relics.iter().any(|relic| relic.id == RelicId::SingingBowl),
    }))
}

fn java_card_reward_action_space(live: &LiveStatus) -> Result<Value, String> {
    let mut indexed = live
        .actions
        .iter()
        .filter(|action| action.kind == "choose_card_reward")
        .map(|action| {
            let index = action
                .reward_index
                .ok_or_else(|| "Java card choice action omitted reward_index".to_owned())?;
            let card_id = action
                .card_id
                .as_deref()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Java card choice action omitted card_id".to_owned())?;
            let upgrades = u8::try_from(
                action
                    .card_upgrades
                    .ok_or_else(|| "Java card choice action omitted upgrades".to_owned())?,
            )
            .map_err(|_| "Java card choice upgrades exceed u8".to_owned())?;
            Ok((index, card_id.to_owned(), upgrades))
        })
        .collect::<Result<Vec<_>, String>>()?;
    indexed.sort_by_key(|(index, _, _)| *index);
    if indexed
        .iter()
        .enumerate()
        .any(|(expected, (actual, _, _))| expected != *actual)
    {
        return Err("Java card choice actions are not a contiguous ordered list".to_owned());
    }
    let mut occurrences = BTreeMap::<(String, u8), usize>::new();
    let mut actions = Vec::with_capacity(indexed.len() + 1);
    for (_, card_id, upgrades) in indexed {
        let key = (card_id.clone(), upgrades);
        let same_card_ordinal = *occurrences.get(&key).unwrap_or(&0);
        occurrences.insert(key, same_card_ordinal + 1);
        actions.push(StoredActionProjectionV1::ChooseCardReward {
            card_id,
            upgrades,
            same_card_ordinal,
        });
    }
    let skip_count = live
        .actions
        .iter()
        .filter(|action| action.kind == "skip_card_reward")
        .count();
    if skip_count > 1 {
        return Err("Java card reward repeats its skip action".to_owned());
    }
    if skip_count == 1 {
        actions.push(StoredActionProjectionV1::SkipCardReward);
    }
    let mut values = actions
        .into_iter()
        .map(|action| {
            serde_json::to_value(action)
                .map_err(|error| format!("failed to encode Java card-reward action: {error}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    sort_json_values(&mut values)?;
    Ok(Value::Array(values))
}

fn rust_card_reward_action_space(session: &RunControlSession) -> Result<Value, String> {
    let EngineState::RewardScreen(reward) = &session.engine_state else {
        return Err("Rust root is not a card reward screen".to_owned());
    };
    let cards = reward
        .pending_card_choice
        .as_ref()
        .ok_or_else(|| "Rust card reward omitted pending choices".to_owned())?;
    if reward.pending_card_reward_index.is_none() {
        return Err("Rust ordinary card reward omitted its source item index".to_owned());
    }
    let mut occurrences = BTreeMap::<(String, u8), usize>::new();
    let mut actions = Vec::with_capacity(cards.len() + 1);
    for card in cards {
        let card_id = java_card_id(card.id).to_owned();
        let upgrades = session
            .run_state
            .preview_obtain_card_upgrades(card.id, card.upgrades);
        let key = (card_id.clone(), upgrades);
        let same_card_ordinal = *occurrences.get(&key).unwrap_or(&0);
        occurrences.insert(key, same_card_ordinal + 1);
        actions.push(StoredActionProjectionV1::ChooseCardReward {
            card_id,
            upgrades,
            same_card_ordinal,
        });
    }
    actions.push(StoredActionProjectionV1::SkipCardReward);
    let mut values = actions
        .into_iter()
        .map(|action| {
            serde_json::to_value(action)
                .map_err(|error| format!("failed to encode Rust card-reward action: {error}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    sort_json_values(&mut values)?;
    Ok(Value::Array(values))
}

fn rust_reward_item_projection(item: &RewardItem) -> Value {
    match item {
        RewardItem::Gold { amount } => json!({"kind": "gold", "amount": amount}),
        RewardItem::StolenGold { amount } => {
            json!({"kind": "stolen_gold", "amount": amount})
        }
        RewardItem::Card { cards } => json!({"kind": "card", "choice_count": cards.len()}),
        RewardItem::Relic { relic_id } => {
            json!({"kind": "relic", "id": canonical_game_id(relic_id.stable_id())})
        }
        RewardItem::Potion { potion_id } => {
            json!({"kind": "potion", "id": canonical_potion_id(potion_id.stable_id())})
        }
        RewardItem::EmeraldKey => json!({"kind": "emerald_key"}),
        RewardItem::SapphireKey => json!({"kind": "sapphire_key"}),
    }
}

fn java_reward_action_space(live: &LiveStatus) -> Result<Value, String> {
    let mut actions = Vec::new();
    for (_, reward, same_reward_ordinal) in java_supported_reward_occurrences(live)? {
        actions.push(StoredActionProjectionV1::ClaimCombatReward {
            reward,
            same_reward_ordinal,
        });
    }
    let claim_count = live
        .actions
        .iter()
        .filter(|action| action.kind == "claim_combat_reward")
        .count();
    let close_count = live
        .actions
        .iter()
        .filter(|action| action.kind == "close_combat_reward")
        .count();
    if claim_count == 0 && close_count == 1 {
        actions.push(StoredActionProjectionV1::CloseCombatReward);
    }
    if close_count > 1 {
        return Err("Java combat reward repeats its close action".to_owned());
    }
    let mut values = actions
        .into_iter()
        .map(|action| {
            serde_json::to_value(action)
                .map_err(|error| format!("failed to encode Java reward action: {error}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    sort_json_values(&mut values)?;
    Ok(Value::Array(values))
}

fn rust_reward_action_space(session: &RunControlSession) -> Result<Value, String> {
    let EngineState::RewardScreen(reward_state) = &session.engine_state else {
        return Err("Rust root is not an ordinary combat reward screen".to_owned());
    };
    if reward_state.pending_card_choice.is_some() {
        return Err("Rust reward action profile does not cover an open card choice".to_owned());
    }
    let mut occurrences = BTreeMap::<CombatRewardIdentityV1, usize>::new();
    let mut values = Vec::new();
    let has_empty_potion_slot = session.run_state.potions.iter().any(Option::is_none);
    for item in &reward_state.items {
        let Some(reward) = rust_supported_reward_identity(item, has_empty_potion_slot) else {
            continue;
        };
        let same_reward_ordinal = *occurrences.get(&reward).unwrap_or(&0);
        occurrences.insert(reward.clone(), same_reward_ordinal + 1);
        values.push(
            serde_json::to_value(StoredActionProjectionV1::ClaimCombatReward {
                reward,
                same_reward_ordinal,
            })
            .map_err(|error| format!("failed to encode Rust reward action: {error}"))?,
        );
    }
    if reward_state.items.is_empty() && reward_state.skippable {
        values.push(
            serde_json::to_value(StoredActionProjectionV1::CloseCombatReward)
                .map_err(|error| format!("failed to encode Rust reward close action: {error}"))?,
        );
    }
    sort_json_values(&mut values)?;
    Ok(Value::Array(values))
}

fn reward_runtime_settlement() -> Value {
    json!({
        "native_autosave": "may_differ_from_native_root",
        "resident_completion": "retire",
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PublicMapProjectionV1 {
    current_x: i32,
    current_y: i32,
    boss_id: Option<String>,
    boss_anchor: Option<PublicMapEdgeProjectionV1>,
    nodes: Vec<PublicMapNodeProjectionV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PublicMapNodeProjectionV1 {
    x: i32,
    y: i32,
    room_type: MapRoomIdentityV1,
    has_emerald_key: bool,
    edges: Vec<PublicMapEdgeProjectionV1>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
struct PublicMapEdgeProjectionV1 {
    destination_x: i32,
    destination_y: i32,
}

fn rust_public_map(session: &RunControlSession) -> Result<Value, String> {
    let public = run_map_comparison_topology_v1(session);
    let mut projection = PublicMapProjectionV1 {
        current_x: public.current_x,
        current_y: public.current_y,
        boss_id: public.boss.map(|boss| canonical_game_id(boss.stable_id())),
        boss_anchor: None,
        nodes: public
            .nodes
            .into_iter()
            .map(|node| PublicMapNodeProjectionV1 {
                x: node.x,
                y: node.y,
                room_type: map_room_identity_from_planner(node.room),
                has_emerald_key: node.has_emerald_key,
                edges: node
                    .edges
                    .into_iter()
                    .map(|edge| PublicMapEdgeProjectionV1 {
                        destination_x: edge.destination_x,
                        destination_y: edge.destination_y,
                    })
                    .collect(),
            })
            .collect(),
    };
    validate_and_order_public_map(&mut projection, "Rust")?;
    serde_json::to_value(projection)
        .map_err(|error| format!("failed to encode Rust public map: {error}"))
}

fn validate_and_order_public_map(
    map: &mut PublicMapProjectionV1,
    runtime: &str,
) -> Result<(), String> {
    if (map.current_x == -1) != (map.current_y == -1) {
        return Err(format!(
            "{runtime} public map has a partial pre-entry coordinate"
        ));
    }
    if map.current_x < -1 || map.current_y < -1 {
        return Err(format!("{runtime} public map has a negative coordinate"));
    }
    let mut coordinates = BTreeSet::new();
    for node in &mut map.nodes {
        if node.x < 0 || node.y < 0 || !coordinates.insert((node.x, node.y)) {
            return Err(format!(
                "{runtime} public map has an invalid or repeated node ({},{})",
                node.x, node.y
            ));
        }
        node.edges.sort();
        if node.edges.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(format!(
                "{runtime} public map repeats an edge from ({},{})",
                node.x, node.y
            ));
        }
        if node
            .edges
            .iter()
            .any(|edge| edge.destination_x < 0 || edge.destination_y < 0)
        {
            return Err(format!(
                "{runtime} public map has a negative edge destination from ({},{})",
                node.x, node.y
            ));
        }
    }
    let external_destinations = map
        .nodes
        .iter()
        .flat_map(|node| &node.edges)
        .filter(|edge| !coordinates.contains(&(edge.destination_x, edge.destination_y)))
        .cloned()
        .collect::<BTreeSet<_>>();
    map.boss_anchor = match external_destinations.len() {
        0 => None,
        1 if map.boss_id.is_some() => external_destinations.into_iter().next(),
        1 => {
            return Err(format!(
                "{runtime} public map has a boss anchor without a boss identity"
            ))
        }
        count => {
            return Err(format!(
                "{runtime} public map has {count} distinct external edge destinations"
            ))
        }
    };
    let current_is_boss_anchor = map.boss_anchor.as_ref().is_some_and(|anchor| {
        (anchor.destination_x, anchor.destination_y) == (map.current_x, map.current_y)
    });
    if map.current_x >= 0
        && !coordinates.contains(&(map.current_x, map.current_y))
        && !current_is_boss_anchor
    {
        return Err(format!(
            "{runtime} public map current coordinate ({},{}) is absent from topology",
            map.current_x, map.current_y
        ));
    }
    for node in &map.nodes {
        for edge in &node.edges {
            if !coordinates.contains(&(edge.destination_x, edge.destination_y))
                && map.boss_anchor.as_ref() != Some(edge)
            {
                return Err(format!(
                    "{runtime} public map edge from ({},{}) targets absent node ({},{})",
                    node.x, node.y, edge.destination_x, edge.destination_y
                ));
            }
        }
    }
    map.nodes.sort_by_key(|node| (node.y, node.x));
    Ok(())
}

fn java_map_action_occurrences(
    live: &LiveStatus,
) -> Result<Vec<(&ActionHandle, i32, i32, MapRoomIdentityV1)>, String> {
    let mut coordinates = BTreeSet::new();
    live.actions
        .iter()
        .filter(|action| action.kind == "choose_map_node")
        .map(|action| {
            let x = i32::try_from(
                action
                    .map_x
                    .ok_or_else(|| "Java map action omitted x".to_owned())?,
            )
            .map_err(|_| "Java map action x exceeds i32".to_owned())?;
            let y = i32::try_from(
                action
                    .map_y
                    .ok_or_else(|| "Java map action omitted y".to_owned())?,
            )
            .map_err(|_| "Java map action y exceeds i32".to_owned())?;
            if x < 0 || y < 0 {
                return Err("Java map action has negative coordinates".to_owned());
            }
            if !coordinates.insert((x, y)) {
                return Err(format!("Java map repeats node coordinates ({x},{y})"));
            }
            let room_type = map_room_identity_from_java(
                action
                    .map_room_type
                    .as_deref()
                    .ok_or_else(|| "Java map action omitted room type".to_owned())?,
            )?;
            Ok((action, x, y, room_type))
        })
        .collect()
}

fn rust_map_action_space(session: &RunControlSession) -> Result<Value, String> {
    if !matches!(session.engine_state, EngineState::MapNavigation) {
        return Err("Rust root is not a stable map-navigation screen".to_owned());
    }
    let surface = build_decision_surface(session);
    let mut coordinates = BTreeSet::new();
    let mut values = Vec::new();
    for candidate in &surface.view.candidates {
        let Some(DecisionCandidateKey::RouteSelect {
            x,
            y,
            room_type: Some(room_type),
            ..
        }) = candidate.key.as_ref()
        else {
            continue;
        };
        if candidate.action.executable_action_ref().is_none() {
            continue;
        }
        if !coordinates.insert((*x, *y)) {
            return Err(format!("Rust map repeats node coordinates ({x},{y})"));
        }
        values.push(
            serde_json::to_value(StoredActionProjectionV1::ChooseMapNode {
                x: *x,
                y: *y,
                room_type: map_room_identity_from_planner(*room_type),
            })
            .map_err(|error| format!("failed to encode Rust map action: {error}"))?,
        );
    }
    sort_json_values(&mut values)?;
    Ok(Value::Array(values))
}

fn map_room_identity_from_planner(value: PlannerMapRoom) -> MapRoomIdentityV1 {
    match value {
        PlannerMapRoom::QuestionMark => MapRoomIdentityV1::QuestionMark,
        PlannerMapRoom::Monster => MapRoomIdentityV1::Monster,
        PlannerMapRoom::Elite => MapRoomIdentityV1::Elite,
        PlannerMapRoom::Boss => MapRoomIdentityV1::Boss,
        PlannerMapRoom::Rest => MapRoomIdentityV1::Rest,
        PlannerMapRoom::Shop => MapRoomIdentityV1::Shop,
        PlannerMapRoom::Treasure => MapRoomIdentityV1::Treasure,
        PlannerMapRoom::Victory => MapRoomIdentityV1::Victory,
    }
}

fn rust_identity_projection(session: &RunControlSession) -> Result<Value, String> {
    let mut projection = rust_master_deck_identity_projection(session);
    projection["schema"] = json!("sts-paired-rust-combat-card-identity-v1");
    projection["combat_zones"] = rust_zone_identity(active_combat(session)?);
    Ok(projection)
}

fn rust_master_deck_identity_projection(session: &RunControlSession) -> Value {
    json!({
        "schema": "sts-paired-rust-master-deck-identity-v1",
        "master_deck": session.run_state.master_deck.iter().enumerate().map(|(ordinal, card)| json!({
            "ordinal": ordinal,
            "lineage": card.lineage.to_string(),
            "card_id": java_card_id(card.id),
            "upgrades": card.upgrades,
        })).collect::<Vec<_>>(),
    })
}

fn rust_zone_identity(combat: &CombatState) -> Value {
    let cards = |cards: Vec<&CombatCard>| {
        cards
            .into_iter()
            .map(|card| json!({"lineage": card.lineage.to_string(), "card_id": java_card_id(card.id), "upgrades": card.upgrades}))
            .collect::<Vec<_>>()
    };
    json!({
        "draw_top_first": cards(combat.zones.draw_pile.iter().collect()),
        "hand": cards(combat.zones.hand.iter().collect()),
        "discard_bottom_first": cards(combat.zones.discard_pile.iter().collect()),
        "exhaust_bottom_first": cards(combat.zones.exhaust_pile.iter().collect()),
        "limbo": cards(combat.zones.limbo.iter().collect()),
    })
}

fn active_combat(session: &RunControlSession) -> Result<&CombatState, String> {
    session
        .active_combat
        .as_ref()
        .map(|active| &active.combat_state)
        .ok_or_else(|| "retained Rust decision is not an active combat root".to_owned())
}

fn paired_action_space_id(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode paired action space: {error}"))?;
    let digest = Sha256::digest(&bytes);
    Ok(format!("paired-action-space-v1-{}", hex_lower(&digest)))
}

fn require_current_rust_semantics(root: &StateRootArtifactV2) -> Result<(), String> {
    let stored = supported_root_version_map(&root.root.versions)?;
    let current = RunPlayEvidenceIdentityV1::current_v1();
    for (component, active_identity) in [
        ("rust_rng_semantics", current.rng_semantics_id.as_str()),
        (
            "rust_simulator_semantics",
            current.simulator_semantics_id.as_str(),
        ),
    ] {
        let stored_identity = stored
            .get(component)
            .expect("supported paired-root version component checked above");
        if *stored_identity != active_identity {
            return Err(format!(
                "paired-root version drift for {component}: stored={stored_identity:?} active={active_identity:?}"
            ));
        }
    }
    Ok(())
}

fn validate_native_activation_versions(
    root: &StateRootArtifactV2,
    live: &LiveStatus,
) -> Result<Vec<PairedRootVersionCompatibilityV1>, String> {
    let current_semantics = RunPlayEvidenceIdentityV1::current_v1();
    let active = BTreeMap::from([
        (
            "communicationmod_jar",
            live.manifest.loaded_mod_jar_sha256.clone(),
        ),
        (
            "communicationmod_source",
            live.manifest.source_commit.clone(),
        ),
        ("run_play_workbench", current_workbench_identity()?),
        ("rust_rng_semantics", current_semantics.rng_semantics_id),
        (
            "rust_simulator_semantics",
            current_semantics.simulator_semantics_id,
        ),
        ("slay_the_spire_jar", live.manifest.sts_jar_sha256.clone()),
    ]);
    compare_supported_root_versions(&root.root.versions, &active)
}

fn compare_supported_root_versions(
    root_versions: &[VersionIdentityV1],
    active: &BTreeMap<&str, String>,
) -> Result<Vec<PairedRootVersionCompatibilityV1>, String> {
    let stored = supported_root_version_map(root_versions)?;
    let mut compatibility = Vec::with_capacity(SUPPORTED_ROOT_VERSION_COMPONENTS_V1.len());
    for component in SUPPORTED_ROOT_VERSION_COMPONENTS_V1 {
        let stored_identity = (*stored
            .get(component)
            .expect("supported paired-root version component checked above"))
        .to_owned();
        let active_identity = active
            .get(component)
            .expect("active paired-root version component is complete")
            .clone();
        let producer_provenance = component == "run_play_workbench";
        let exact = version_identities_equal(component, &stored_identity, &active_identity);
        if !producer_provenance && !exact {
            return Err(format!(
                "paired-root version drift for {component}: stored={stored_identity:?} active={active_identity:?}"
            ));
        }
        compatibility.push(PairedRootVersionCompatibilityV1 {
            component: component.to_owned(),
            stored_identity: stored_identity.clone(),
            active_identity: active_identity.clone(),
            enforcement: if producer_provenance {
                "producer_provenance".to_owned()
            } else if component.starts_with("rust_") {
                "semantic_contract".to_owned()
            } else {
                "runtime_build".to_owned()
            },
            status: if exact {
                "exact".to_owned()
            } else {
                "producer_changed_semantics_compatible".to_owned()
            },
        });
    }
    Ok(compatibility)
}

fn version_identities_equal(component: &str, stored: &str, active: &str) -> bool {
    if matches!(
        component,
        "communicationmod_jar"
            | "communicationmod_source"
            | "run_play_workbench"
            | "slay_the_spire_jar"
    ) {
        stored.eq_ignore_ascii_case(active)
    } else {
        stored == active
    }
}

fn supported_root_version_map(
    root_versions: &[VersionIdentityV1],
) -> Result<BTreeMap<&str, &str>, String> {
    let mut versions = BTreeMap::new();
    for version in root_versions {
        if versions
            .insert(version.component.as_str(), version.identity.as_str())
            .is_some()
        {
            return Err(format!(
                "paired root repeats version component {:?}",
                version.component
            ));
        }
    }
    let actual = versions.keys().copied().collect::<BTreeSet<_>>();
    let supported = SUPPORTED_ROOT_VERSION_COMPONENTS_V1
        .into_iter()
        .collect::<BTreeSet<_>>();
    if actual != supported {
        return Err(format!(
            "paired root version component set is unsupported: expected={} actual={}",
            supported.into_iter().collect::<Vec<_>>().join(","),
            actual.into_iter().collect::<Vec<_>>().join(",")
        ));
    }
    Ok(versions)
}

fn current_workbench_identity() -> Result<String, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("failed to resolve current run-play executable: {error}"))?;
    let bytes = fs::read(&executable).map_err(|error| {
        format!(
            "failed to read current run-play executable {}: {error}",
            executable.display()
        )
    })?;
    Ok(sha256_bytes(&bytes))
}

fn versions_for_root(
    head: &crate::model::RunPlayHeadV1,
    live: &LiveStatus,
) -> Result<Vec<VersionIdentityV1>, String> {
    let mut versions = vec![
        VersionIdentityV1 {
            component: "communicationmod_source".to_owned(),
            identity: live.manifest.source_commit.clone(),
        },
        VersionIdentityV1 {
            component: "communicationmod_jar".to_owned(),
            identity: live.manifest.loaded_mod_jar_sha256.clone(),
        },
        VersionIdentityV1 {
            component: "slay_the_spire_jar".to_owned(),
            identity: live.manifest.sts_jar_sha256.clone(),
        },
        VersionIdentityV1 {
            component: "run_play_workbench".to_owned(),
            identity: current_workbench_identity()?,
        },
    ];
    if let Some(identity) = &head.evidence_identity {
        versions.extend([
            VersionIdentityV1 {
                component: "rust_rng_semantics".to_owned(),
                identity: identity.rng_semantics_id.clone(),
            },
            VersionIdentityV1 {
                component: "rust_simulator_semantics".to_owned(),
                identity: identity.simulator_semantics_id.clone(),
            },
        ]);
    }
    Ok(versions)
}

fn java_runtime_epoch(live: &LiveStatus) -> String {
    format!(
        "{}:{}:{}",
        live.manifest.producer_session_id, live.manifest.connection_id, live.manifest.capture_id
    )
}

fn canonical_potion_id(value: &str) -> String {
    canonical_game_id(value)
}

fn canonical_power_id(value: &str) -> String {
    match value {
        "Weak" | "Weakened" => "weakened".to_owned(),
        "Regrow" | "Life Link" => "lifelink".to_owned(),
        "Regen" | "Regenerate" => "regenerate".to_owned(),
        other => canonical_game_id(other),
    }
}

fn rust_character_id(value: &str) -> &'static str {
    match value {
        "Ironclad" => "IRONCLAD",
        "Silent" => "THE_SILENT",
        "Defect" => "DEFECT",
        "Watcher" => "WATCHER",
        _ => "UNKNOWN",
    }
}

fn first_json_difference(path: &str, java: &Value, rust: &Value) -> Option<TypedDifferenceV1> {
    match (java, rust) {
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                return Some(TypedDifferenceV1 {
                    path: path.to_owned(),
                    kind: "array_length_mismatch".to_owned(),
                    detail: format!("Java={} Rust={}", left.len(), right.len()),
                });
            }
            left.iter()
                .zip(right)
                .enumerate()
                .find_map(|(index, (left, right))| {
                    first_json_difference(&format!("{path}[{index}]"), left, right)
                })
        }
        (Value::Object(left), Value::Object(right)) => {
            let keys = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
            for key in keys {
                match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) => {
                        if let Some(difference) =
                            first_json_difference(&format!("{path}.{key}"), left, right)
                        {
                            return Some(difference);
                        }
                    }
                    _ => {
                        return Some(TypedDifferenceV1 {
                            path: format!("{path}.{key}"),
                            kind: "field_presence_mismatch".to_owned(),
                            detail: format!(
                                "Java={} Rust={}",
                                left.contains_key(key),
                                right.contains_key(key)
                            ),
                        });
                    }
                }
            }
            None
        }
        _ if java == rust => None,
        _ => Some(TypedDifferenceV1 {
            path: path.to_owned(),
            kind: "value_mismatch".to_owned(),
            detail: format!("Java={} Rust={}", compact_json(java), compact_json(rust)),
        }),
    }
}

fn compact_json(value: &Value) -> String {
    let value = serde_json::to_string(value).unwrap_or_else(|_| "<unencodable>".to_owned());
    if value.len() > 200 {
        format!("{}…", &value[..200])
    } else {
        value
    }
}

fn sort_json_values(values: &mut [Value]) -> Result<(), String> {
    let mut encoded = values
        .iter()
        .map(|value| serde_json::to_string(value).map(|key| (key, value.clone())))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to order typed action space: {error}"))?;
    encoded.sort_by(|left, right| left.0.cmp(&right.0));
    for (target, (_, value)) in values.iter_mut().zip(encoded) {
        *target = value;
    }
    Ok(())
}

fn put_json<T: Serialize>(store: &StateRootStoreV2, value: &T) -> Result<BlobRefV1, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode paired projection: {error}"))?;
    store.put_bytes(PROJECTION_MEDIA_TYPE, &bytes)
}

fn create_fresh_directory(path: &Path) -> Result<(), String> {
    fs::create_dir(path).map_err(|error| {
        format!(
            "paired-root attempt directory must be fresh {}: {error}",
            path.display()
        )
    })
}

fn write_new_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode {}: {error}", path.display()))?;
    write_new(path, &bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("typed Java field {key} is absent or empty"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64, String> {
    value[key]
        .as_u64()
        .ok_or_else(|| format!("typed Java field {key} is not an unsigned integer"))
}

fn required_i64(value: &Value, key: &str) -> Result<i64, String> {
    value[key]
        .as_i64()
        .ok_or_else(|| format!("typed Java field {key} is not an integer"))
}

fn required_bool(value: &Value, key: &str) -> Result<bool, String> {
    value[key]
        .as_bool()
        .ok_or_else(|| format!("typed Java field {key} is not a boolean"))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_active_combat() -> RunControlSession {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = sts_oracle_learning::state::core::EngineState::CombatPlayerTurn;
        session.active_combat = Some(sts_oracle_learning::state::core::ActiveCombat::new(
            sts_oracle_learning::state::core::EngineState::CombatPlayerTurn,
            sts_oracle_learning::test_support::blank_test_combat(),
            sts_oracle_learning::state::core::CombatContext::Room(
                sts_oracle_learning::state::core::RoomCombatContext {
                    room_type: sts_oracle_learning::state::map::node::RoomType::MonsterRoom,
                },
            ),
        ));
        session
    }

    fn compatible_root_versions() -> Vec<VersionIdentityV1> {
        SUPPORTED_ROOT_VERSION_COMPONENTS_V1
            .into_iter()
            .map(|component| VersionIdentityV1 {
                component: component.to_owned(),
                identity: format!("{component}-identity"),
            })
            .collect()
    }

    fn compatible_active_versions() -> BTreeMap<&'static str, String> {
        SUPPORTED_ROOT_VERSION_COMPONENTS_V1
            .into_iter()
            .map(|component| (component, format!("{component}-identity")))
            .collect()
    }

    #[test]
    fn java_signed_long_seed_is_compared_as_the_engine_u64_bit_pattern() {
        assert_eq!(
            canonical_engine_seed_from_java_long("-5898947908679738850").unwrap(),
            "12547796165029812766"
        );
        assert_eq!(
            canonical_engine_seed_from_java_long("12547796165029812766").unwrap(),
            "12547796165029812766"
        );
        assert!(canonical_engine_seed_from_java_long("13K5KDL").is_err());
    }

    #[test]
    fn run_scalars_use_visible_active_combat_player_resources() {
        let mut session = session_with_active_combat();
        session.run_state.current_hp = 42;
        session.run_state.max_hp = 80;
        session.run_state.gold = 99;
        let player = &mut session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .entities
            .player;
        player.current_hp = 35;
        player.max_hp = 84;
        player.gold = 123;

        let projected = rust_run_scalars(&session).unwrap();

        assert_eq!(projected["current_hp"], 35);
        assert_eq!(projected["max_hp"], 84);
        assert_eq!(projected["gold"], 123);
        assert_eq!(projected["act"], session.run_state.act_num);
        assert_eq!(projected["floor"], session.run_state.floor_num);
    }

    #[test]
    fn action_space_v2_adds_typed_potions_without_reinterpreting_v1() {
        let mut session = session_with_active_combat();
        let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
        let mut monster = sts_oracle_learning::test_support::planned_monster(
            sts_oracle_learning::content::monsters::EnemyId::JawWorm,
            1,
        );
        monster.set_planned_visible_spec(Some(
            sts_oracle_learning::runtime::monster_move::MonsterMoveSpec::Unknown,
        ));
        combat.entities.monsters = vec![monster];
        combat.entities.potions = vec![Some(sts_oracle_learning::content::potions::Potion::new(
            PotionId::BlockPotion,
            41,
        ))];
        let bindings = BTreeMap::new();

        let v1 = rust_action_space(
            &session,
            &bindings,
            ActionSpaceProjectionVersion::CardOnlyV1,
        )
        .unwrap();
        let v2 =
            rust_action_space(&session, &bindings, ActionSpaceProjectionVersion::PotionV2).unwrap();

        assert!(v1
            .as_array()
            .unwrap()
            .iter()
            .all(|action| action["kind"] != "use_potion"));
        let potion = v2
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["kind"] == "use_potion")
            .unwrap_or_else(|| panic!("v2 action space omitted potion: {v2}"));
        assert_eq!(potion["slot"], 0);
        assert_eq!(potion["potion_id"], "blockpotion");
        assert_eq!(potion["potency"], 12);
        assert_eq!(potion["target_index"], Value::Null);
        assert_eq!(
            action_space_projection_schema(ActionSpaceProjectionVersion::CardOnlyV1),
            ACTION_SPACE_SCHEMA_CARD_ONLY_V1
        );
        assert_eq!(
            action_space_projection_schema(ActionSpaceProjectionVersion::PotionV2),
            ACTION_SPACE_SCHEMA_POTION_V2
        );
    }

    #[test]
    fn combat_projection_v2_compares_public_intent_without_leaking_runic_dome() {
        let java = json!({
            "public_intent": {
                "evidence": "visible_exact",
                "visible_intent": "ATTACK_DEBUFF",
                "damage_per_hit": 8,
                "hits": 2
            }
        });
        assert_eq!(
            java_public_intent(&java).unwrap(),
            json!({
                "evidence": "visible_exact",
                "visible_intent": "attack_debuff",
                "damage_per_hit": 8,
                "hits": 2
            })
        );

        let mut combat = sts_oracle_learning::test_support::blank_test_combat();
        let mut monster = sts_oracle_learning::test_support::planned_monster(
            sts_oracle_learning::content::monsters::EnemyId::JawWorm,
            1,
        );
        monster.id = 7;
        combat.entities.monsters = vec![monster];
        combat.set_monster_protocol_visible_intent(
            7,
            Intent::AttackDebuff {
                damage: 11,
                hits: 2,
            },
        );
        combat.set_monster_protocol_preview_damage_per_hit(7, 8);
        assert_eq!(
            rust_public_intent(&combat, 7).unwrap(),
            java_public_intent(&java).unwrap()
        );

        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicDome));
        assert_eq!(
            rust_public_intent(&combat, 7).unwrap(),
            json!({
                "evidence": "hidden",
                "hidden_reason": "runic_dome"
            })
        );
        assert_eq!(
            combat_projection_schema(CombatProjectionVersion::LegacyV1),
            COMBAT_ROOT_SCHEMA_V1
        );
        assert_eq!(
            combat_projection_schema(CombatProjectionVersion::PublicIntentV2),
            COMBAT_ROOT_SCHEMA_PUBLIC_INTENT_V2
        );
    }

    #[test]
    fn combat_projection_v2_rejects_incoherent_public_intent_shapes() {
        let missing_hits = json!({
            "public_intent": {
                "evidence": "visible_exact",
                "visible_intent": "ATTACK",
                "damage_per_hit": 6
            }
        });
        assert!(java_public_intent(&missing_hits)
            .unwrap_err()
            .contains("different presence"));

        let leaking_hidden = json!({
            "public_intent": {
                "evidence": "hidden",
                "visible_intent": "ATTACK",
                "hidden_reason": "runic_dome"
            }
        });
        assert!(java_public_intent(&leaking_hidden).is_err());
    }

    fn covered_frame() -> Value {
        json!({
            "boundary": {"kind": "combat_player_decision", "stable": true},
            "public_base": {
                "run": {},
                "combat": {"potions": []}
            },
            "parity_internal": {
                "content_profile": "all_unlocked",
                "master_deck": [],
                "relics": [],
                "card_pools": {},
                "zones": {},
                "monsters": [],
                "player_powers": []
            },
            "rng_internal": {"streams": [], "run_streams": []},
            "action_space": {"actions": []}
        })
    }

    #[test]
    fn stored_action_projection_yields_stable_card_potion_and_end_turn_handles() {
        let binding_id = format!("paired-card-binding-v1-{}", "a".repeat(64));
        let blob = BlobRefV1 {
            sha256: "b".repeat(64),
            bytes: 2,
            media_type: "application/json".to_owned(),
        };
        let bindings = vec![CrossRuntimeCardBindingV1 {
            binding_id: binding_id.clone(),
            rust_lineage: RustCardLineageV1::MasterDeck {
                acquisition_ordinal: 0,
            },
            card_id: "Strike_R".to_owned(),
            upgrades: 0,
            evidence: CardBindingEvidenceV1::MasterDeckObservation {
                java_master_deck_ordinal: 0,
                rust_master_deck_ordinal: 0,
                java_projection: blob.clone(),
                rust_projection: blob,
            },
        }];
        let root_id = format!("state-root-v2-{}", "c".repeat(64));
        let action_space_id = "paired-action-space-v1-fixture";
        let action_space = json!([
            {
                "kind": "play_card",
                "binding_id": binding_id,
                "target_index": 1,
                "target_id": "spikeslimem"
            },
            {
                "kind": "use_potion",
                "slot": 0,
                "potion_id": "blockpotion",
                "potency": 12
            },
            {"kind": "end_turn"}
        ]);
        let combat = json!({
            "monsters": [
                {"id": "spikeslimem"},
                {"id": "spikeslimem"}
            ],
            "zones": {"hand": [{
                "binding_id": bindings[0].binding_id.clone(),
                "card_id": "Strike_R",
                "upgrades": 0,
                "cost_for_turn": 1
            }]}
        });
        let potions = json!([{
            "slot": 0,
            "id": "blockpotion",
            "can_use": true,
            "can_discard": true,
            "target_required": false
        }]);

        let observed = build_stable_combat_action_observations(
            &root_id,
            action_space_id,
            &bindings,
            action_space,
            &combat,
            &potions,
        )
        .unwrap();
        assert_eq!(observed.len(), 3);
        let play = observed
            .iter()
            .find(|action| matches!(&action.handle.action, PairedActionV1::PlayCard { .. }))
            .unwrap();
        let PairedActionV1::PlayCard { target, .. } = &play.handle.action else {
            unreachable!();
        };
        assert_eq!(target.as_ref().unwrap().same_enemy_ordinal, 1);
        assert_eq!(play.card.as_ref().unwrap().card_id, "Strike_R");
        assert_eq!(play.card.as_ref().unwrap().cost_for_turn, 1);
        let potion = observed
            .iter()
            .find(|action| matches!(&action.handle.action, PairedActionV1::UsePotion { .. }))
            .unwrap();
        let PairedActionV1::UsePotion {
            slot,
            potion_id,
            potency,
            target,
        } = &potion.handle.action
        else {
            unreachable!();
        };
        assert_eq!(*slot, 0);
        assert_eq!(potion_id, "blockpotion");
        assert_eq!(*potency, 12);
        assert!(target.is_none());
        assert_eq!(potion.potion.as_ref().unwrap().potion_id, "blockpotion");
        assert!(observed
            .iter()
            .any(|action| matches!(&action.handle.action, PairedActionV1::EndTurn)));
        assert!(observed
            .iter()
            .all(|action| action.handle.source_root_id == root_id));
    }

    #[test]
    fn reward_action_space_uses_semantic_identity_and_duplicate_occurrence() {
        let mut session = RunControlSession::new(Default::default());
        let mut reward = sts_oracle_learning::state::rewards::RewardState::new();
        reward.items = vec![
            RewardItem::Gold { amount: 12 },
            RewardItem::Card {
                cards: vec![
                    sts_oracle_learning::state::rewards::RewardCard::new(
                        sts_oracle_learning::content::cards::CardId::Cleave,
                        0,
                    ),
                    sts_oracle_learning::state::rewards::RewardCard::new(
                        sts_oracle_learning::content::cards::CardId::WildStrike,
                        0,
                    ),
                    sts_oracle_learning::state::rewards::RewardCard::new(
                        sts_oracle_learning::content::cards::CardId::Armaments,
                        0,
                    ),
                ],
            },
            RewardItem::Gold { amount: 12 },
            RewardItem::StolenGold { amount: 12 },
        ];
        session.engine_state = EngineState::RewardScreen(reward);

        let projected = rust_reward_action_space(&session).unwrap();
        let actions: Vec<StoredActionProjectionV1> =
            serde_json::from_value(projected.clone()).unwrap();
        assert_eq!(actions.len(), 4);
        assert!(actions.iter().any(|action| matches!(
            action,
            StoredActionProjectionV1::ClaimCombatReward {
                reward: CombatRewardIdentityV1::Gold { amount: 12 },
                same_reward_ordinal: 0,
            }
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            StoredActionProjectionV1::ClaimCombatReward {
                reward: CombatRewardIdentityV1::Gold { amount: 12 },
                same_reward_ordinal: 1,
            }
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            StoredActionProjectionV1::ClaimCombatReward {
                reward: CombatRewardIdentityV1::StolenGold { amount: 12 },
                same_reward_ordinal: 0,
            }
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            StoredActionProjectionV1::ClaimCombatReward {
                reward: CombatRewardIdentityV1::Card { choice_count: 3 },
                same_reward_ordinal: 0,
            }
        )));

        let observed = build_stable_reward_action_observations(
            &format!("state-root-v2-{}", "c".repeat(64)),
            "paired-action-space-v1-reward-fixture",
            projected,
        )
        .unwrap();
        assert_eq!(observed.len(), 4);
        assert!(observed.iter().all(|item| {
            item.card.is_none() && item.potion.is_none() && item.combat_reward.is_some()
        }));
    }

    #[test]
    fn reward_action_space_exposes_potion_only_with_an_empty_belt_slot() {
        let mut session = RunControlSession::new(Default::default());
        let mut reward = sts_oracle_learning::state::rewards::RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::WeakenPotion,
        }];
        session.engine_state = EngineState::RewardScreen(reward);
        session.run_state.potions = vec![None, None];

        let projected = rust_reward_action_space(&session).unwrap();
        let actions: Vec<StoredActionProjectionV1> = serde_json::from_value(projected).unwrap();
        assert!(matches!(
            actions.as_slice(),
            [StoredActionProjectionV1::ClaimCombatReward {
                reward: CombatRewardIdentityV1::Potion { potion_id },
                same_reward_ordinal: 0,
            }] if potion_id == "weakenpotion"
        ));

        session.run_state.potions = vec![
            Some(sts_oracle_learning::content::potions::Potion::new(
                PotionId::BlockPotion,
                1,
            )),
            Some(sts_oracle_learning::content::potions::Potion::new(
                PotionId::SwiftPotion,
                2,
            )),
        ];
        let projected = rust_reward_action_space(&session).unwrap();
        let actions: Vec<StoredActionProjectionV1> = serde_json::from_value(projected).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn empty_reward_action_space_exposes_one_separate_close() {
        let mut session = RunControlSession::new(Default::default());
        let mut reward = sts_oracle_learning::state::rewards::RewardState::new();
        reward.skippable = true;
        session.engine_state = EngineState::RewardScreen(reward);

        let projected = rust_reward_action_space(&session).unwrap();
        let actions: Vec<StoredActionProjectionV1> =
            serde_json::from_value(projected.clone()).unwrap();
        assert!(matches!(
            actions.as_slice(),
            [StoredActionProjectionV1::CloseCombatReward]
        ));

        let observed = build_stable_reward_action_observations(
            &format!("state-root-v2-{}", "e".repeat(64)),
            "paired-action-space-v1-empty-reward-fixture",
            projected,
        )
        .unwrap();
        assert_eq!(observed.len(), 1);
        assert!(matches!(
            observed[0].handle.action,
            PairedActionV1::CloseCombatReward
        ));
        assert!(observed[0].combat_reward.is_none());
    }

    #[test]
    fn map_action_space_uses_typed_coordinates_and_room_identity() {
        use sts_oracle_learning::state::map::node::{MapEdge, MapRoomNode, RoomType};

        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::MapNavigation;
        let mut current = MapRoomNode::new(0, 0);
        current.class = Some(RoomType::MonsterRoom);
        current.edges.insert(MapEdge::new(0, 0, 0, 1));
        current.edges.insert(MapEdge::new(0, 0, 1, 1));
        let mut rest = MapRoomNode::new(0, 1);
        rest.class = Some(RoomType::RestRoom);
        let mut elite = MapRoomNode::new(1, 1);
        elite.class = Some(RoomType::MonsterRoomElite);
        session.run_state.map.graph = vec![vec![current], vec![rest, elite]];
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = 0;

        assert_eq!(
            rust_public_map(&session).unwrap(),
            json!({
                "current_x": 0,
                "current_y": 0,
                "boss_id": "slimeboss",
                "boss_anchor": null,
                "nodes": [
                    {
                        "x": 0,
                        "y": 0,
                        "room_type": "monster",
                        "has_emerald_key": false,
                        "edges": [
                            {"destination_x": 0, "destination_y": 1},
                            {"destination_x": 1, "destination_y": 1}
                        ]
                    },
                    {
                        "x": 0,
                        "y": 1,
                        "room_type": "rest",
                        "has_emerald_key": false,
                        "edges": []
                    },
                    {
                        "x": 1,
                        "y": 1,
                        "room_type": "elite",
                        "has_emerald_key": false,
                        "edges": []
                    }
                ]
            })
        );
        let projected = rust_map_action_space(&session).unwrap();
        let actions: Vec<StoredActionProjectionV1> =
            serde_json::from_value(projected.clone()).unwrap();
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().any(|action| matches!(
            action,
            StoredActionProjectionV1::ChooseMapNode {
                x: 0,
                y: 1,
                room_type: MapRoomIdentityV1::Rest,
            }
        )));
        assert!(actions.iter().any(|action| matches!(
            action,
            StoredActionProjectionV1::ChooseMapNode {
                x: 1,
                y: 1,
                room_type: MapRoomIdentityV1::Elite,
            }
        )));

        let observed = build_stable_map_action_observations(
            &format!("state-root-v2-{}", "f".repeat(64)),
            "paired-action-space-v1-map-fixture",
            projected,
        )
        .unwrap();
        assert_eq!(observed.len(), 2);
        assert!(observed
            .iter()
            .all(|item| matches!(item.handle.action, PairedActionV1::ChooseMapNode { .. })));
    }

    #[test]
    fn public_map_keeps_one_typed_external_boss_anchor() {
        let anchor = PublicMapEdgeProjectionV1 {
            destination_x: 3,
            destination_y: 16,
        };
        let mut map = PublicMapProjectionV1 {
            current_x: -1,
            current_y: -1,
            boss_id: Some("hexaghost".to_owned()),
            boss_anchor: None,
            nodes: vec![PublicMapNodeProjectionV1 {
                x: 2,
                y: 14,
                room_type: MapRoomIdentityV1::Rest,
                has_emerald_key: false,
                edges: vec![anchor.clone()],
            }],
        };

        validate_and_order_public_map(&mut map, "fixture").unwrap();
        assert_eq!(map.boss_anchor, Some(anchor));
    }

    #[test]
    fn card_reward_action_space_uses_semantic_card_occurrence_and_skip() {
        let mut session = RunControlSession::new(Default::default());
        let cards = vec![
            sts_oracle_learning::state::rewards::RewardCard::new(
                sts_oracle_learning::content::cards::CardId::Anger,
                0,
            ),
            sts_oracle_learning::state::rewards::RewardCard::new(
                sts_oracle_learning::content::cards::CardId::Anger,
                0,
            ),
            sts_oracle_learning::state::rewards::RewardCard::new(
                sts_oracle_learning::content::cards::CardId::Cleave,
                0,
            ),
        ];
        let mut reward = sts_oracle_learning::state::rewards::RewardState::new();
        reward.items = vec![RewardItem::Card {
            cards: cards.clone(),
        }];
        reward.pending_card_choice = Some(cards);
        reward.pending_card_reward_index = Some(0);
        session.engine_state = EngineState::RewardScreen(reward);

        let projected = rust_card_reward_action_space(&session).unwrap();
        let actions: Vec<StoredActionProjectionV1> =
            serde_json::from_value(projected.clone()).unwrap();
        assert_eq!(actions.len(), 4);
        assert!(actions.iter().any(|action| matches!(
            action,
            StoredActionProjectionV1::ChooseCardReward {
                card_id,
                upgrades: 0,
                same_card_ordinal: 1,
            } if card_id == "Anger"
        )));
        assert!(actions
            .iter()
            .any(|action| matches!(action, StoredActionProjectionV1::SkipCardReward)));

        let observed = build_stable_card_reward_action_observations(
            &format!("state-root-v2-{}", "d".repeat(64)),
            "paired-action-space-v1-card-reward-fixture",
            projected,
        )
        .unwrap();
        assert_eq!(observed.len(), 4);
        assert!(observed.iter().all(|item| {
            item.card.is_none()
                && item.potion.is_none()
                && item.combat_reward.is_none()
                && item.card_reward.is_some()
        }));
    }

    #[test]
    fn reward_run_scalars_use_persistent_postcombat_resources() {
        let mut session = RunControlSession::new(Default::default());
        session.active_combat = None;
        session.engine_state = EngineState::RewardScreen(Default::default());
        session.run_state.current_hp = 17;
        session.run_state.max_hp = 71;
        session.run_state.gold = 44;

        let projected = rust_persistent_run_scalars(&session);

        assert_eq!(projected["current_hp"], 17);
        assert_eq!(projected["max_hp"], 71);
        assert_eq!(projected["gold"], 44);
    }

    #[test]
    fn combat_reward_uses_its_own_stable_noncombat_contract() {
        let contract =
            stable_noncombat_projection_contract(DecisionProfileV1::CombatReward).unwrap();

        assert_eq!(contract.action_space_schema, REWARD_ACTION_SPACE_SCHEMA_V1);
        assert_eq!(
            contract.profile_projection_schemas,
            REWARD_PROFILE_PROJECTION_SCHEMAS_V1
        );
    }

    #[test]
    fn card_reward_uses_its_own_stable_noncombat_contract() {
        let contract = stable_noncombat_projection_contract(DecisionProfileV1::CardReward).unwrap();

        assert_eq!(
            contract.action_space_schema,
            CARD_REWARD_ACTION_SPACE_SCHEMA_V1
        );
        assert_eq!(
            contract.profile_projection_schemas,
            CARD_REWARD_PROFILE_PROJECTION_SCHEMAS_V1
        );
    }

    #[test]
    fn event_profile_uses_typed_identity_screen_and_option_actions() {
        let contract = stable_noncombat_projection_contract(DecisionProfileV1::Event).unwrap();
        assert_eq!(contract.action_space_schema, EVENT_ACTION_SPACE_SCHEMA_V1);
        assert_eq!(
            contract.profile_projection_schemas,
            EVENT_PROFILE_PROJECTION_SCHEMAS_V1
        );

        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        session.run_state.event_state = Some(sts_oracle_learning::state::events::EventState::new(
            EventId::ShiningLight,
        ));
        assert_eq!(
            rust_event_room(&session).unwrap(),
            json!({"kind": "event", "event_id": "ShiningLight", "screen": 0})
        );
        let projected = rust_event_action_space(&session).unwrap();
        let actions: Vec<StoredActionProjectionV1> =
            serde_json::from_value(projected.clone()).unwrap();
        assert_eq!(actions.len(), 2);
        assert!(actions.iter().any(|action| matches!(
            action,
            StoredActionProjectionV1::ChooseEventOption {
                event_id,
                screen: 0,
                option_index: 0,
            } if event_id == "ShiningLight"
        )));
        let observed = build_stable_event_action_observations(
            &format!("state-root-v2-{}", "8".repeat(64)),
            "paired-action-space-v1-event-fixture",
            projected,
        )
        .unwrap();
        assert_eq!(observed.len(), 2);
        assert!(observed
            .iter()
            .all(|item| matches!(item.handle.action, PairedActionV1::ChooseEventOption { .. })));
    }

    #[test]
    fn golden_shrine_public_semantics_are_compact_and_not_localized_button_text() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        session.run_state.ascension_level = 20;
        session.run_state.event_state = Some(sts_oracle_learning::state::events::EventState::new(
            EventId::GoldenShrine,
        ));

        let pray =
            observed_event_option_semantics_v1(&session, EventId::GoldenShrine, 0, 0).unwrap();
        assert_eq!(
            pray,
            PairedRootObservedEventOptionV1 {
                action_kind: PairedRootObservedEventActionKindV1::Gain,
                effect_summary: Some("gain 50 gold".to_owned()),
                followup: PairedRootObservedEventFollowupV1::EventScreen,
                repeatable: false,
            }
        );

        let desecrate =
            observed_event_option_semantics_v1(&session, EventId::GoldenShrine, 0, 1).unwrap();
        assert_eq!(
            desecrate.effect_summary.as_deref(),
            Some("gain 275 gold; obtain 1 specific curse Regret")
        );
        assert_eq!(
            desecrate.followup,
            PairedRootObservedEventFollowupV1::EventScreen
        );

        let leave =
            observed_event_option_semantics_v1(&session, EventId::GoldenShrine, 0, 2).unwrap();
        assert_eq!(
            leave,
            PairedRootObservedEventOptionV1 {
                action_kind: PairedRootObservedEventActionKindV1::Leave,
                effect_summary: None,
                followup: PairedRootObservedEventFollowupV1::EventComplete,
                repeatable: false,
            }
        );
    }

    #[test]
    fn map_navigation_uses_map_domain_in_its_noncombat_contract() {
        let contract =
            stable_noncombat_projection_contract(DecisionProfileV1::MapNavigation).unwrap();

        assert_eq!(contract.action_space_schema, MAP_ACTION_SPACE_SCHEMA_V1);
        assert_eq!(
            contract.profile_projection_schemas,
            MAP_PROFILE_PROJECTION_SCHEMAS_V1
        );
        assert!(contract
            .profile_projection_schemas
            .iter()
            .any(|(domain, _)| *domain == ProjectionDomainV1::Map));
        assert!(!contract
            .profile_projection_schemas
            .iter()
            .any(|(domain, _)| *domain == ProjectionDomainV1::Room));
    }

    #[test]
    fn map_capabilities_keep_question_mark_separate_from_other_map_nodes() {
        let proof = BlobRefV1 {
            sha256: "6".repeat(64),
            bytes: 2,
            media_type: "application/json".to_owned(),
        };
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::MapNavigation;
        let plan = capability_surface_plan_for_profile(DecisionProfileV1::MapNavigation, &session)
            .unwrap();
        let (gaps, capabilities) = plan.materialize(&proof);

        assert!(capabilities
            .windows(2)
            .all(|pair| pair[0].capability < pair[1].capability));
        assert!(capabilities.iter().any(|item| {
            item.capability == CapabilityKindV1::EnterCombat
                && matches!(item.status, CapabilityStatusV1::Verified { .. })
        }));
        assert!(capabilities.iter().any(|item| {
            item.capability == CapabilityKindV1::ChooseMapNode
                && matches!(item.status, CapabilityStatusV1::Unknown { .. })
        }));
        assert!(capabilities.iter().any(|item| {
            item.capability == CapabilityKindV1::EnterQuestionMark
                && matches!(item.status, CapabilityStatusV1::Unknown { .. })
        }));
        for capability in &capabilities {
            if let CapabilityStatusV1::Unknown { gap_id } = &capability.status {
                assert!(gaps.iter().any(|gap| {
                    gap.gap_id == *gap_id && gap.scope == CoverageGapScopeV1::ActionCapability
                }));
            }
        }
        assert_eq!(
            capability_for_action(&PairedActionV1::ChooseMapNode {
                x: 1,
                y: 1,
                room_type: MapRoomIdentityV1::QuestionMark,
            }),
            CapabilityKindV1::EnterQuestionMark
        );
        assert_eq!(
            capability_for_action(&PairedActionV1::ChooseMapNode {
                x: 0,
                y: 0,
                room_type: MapRoomIdentityV1::Monster,
            }),
            CapabilityKindV1::EnterCombat
        );
        assert_eq!(
            capability_for_action(&PairedActionV1::ChooseMapNode {
                x: 1,
                y: 1,
                room_type: MapRoomIdentityV1::Elite,
            }),
            CapabilityKindV1::ChooseMapNode
        );
    }

    #[test]
    fn question_mark_authorization_does_not_depend_on_the_hidden_successor() {
        use sts_oracle_learning::state::map::node::{MapEdge, MapRoomNode, RoomType};

        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::MapNavigation;
        let mut current = MapRoomNode::new(0, 0);
        current.class = Some(RoomType::MonsterRoom);
        current.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut question = MapRoomNode::new(0, 1);
        question.class = Some(RoomType::EventRoom);
        session.run_state.map.graph = vec![vec![current], vec![question]];
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = 0;

        let mut hidden_monster = session.clone();
        hidden_monster.run_state.event_generator.monster_chance = 1.0;
        hidden_monster.run_state.event_generator.shop_chance = 0.0;
        hidden_monster.run_state.event_generator.treasure_chance = 0.0;

        let mut hidden_shop = session;
        hidden_shop.run_state.event_generator.monster_chance = 0.0;
        hidden_shop.run_state.event_generator.shop_chance = 1.0;
        hidden_shop.run_state.event_generator.treasure_chance = 0.0;

        assert_eq!(
            question_mark_entry_capability_status(&hidden_monster),
            CapabilityPlanStatusV1::Verified
        );
        assert_eq!(
            question_mark_entry_capability_status(&hidden_shop),
            CapabilityPlanStatusV1::Verified
        );
    }

    #[test]
    fn rust_successor_profile_is_derived_before_live_execution() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::MapNavigation;
        assert_eq!(
            rust_decision_profile(&session),
            Some(DecisionProfileV1::MapNavigation)
        );

        session.engine_state = EngineState::EventRoom;
        session.run_state.event_state = Some(sts_oracle_learning::state::events::EventState::new(
            EventId::BigFish,
        ));
        assert_eq!(
            rust_decision_profile(&session),
            Some(DecisionProfileV1::Event)
        );

        session.run_state.event_state = Some(sts_oracle_learning::state::events::EventState::new(
            EventId::Neow,
        ));
        assert_eq!(
            rust_decision_profile(&session),
            Some(DecisionProfileV1::Neow)
        );

        let mut reward = sts_oracle_learning::state::rewards::RewardState::new();
        session.engine_state = EngineState::RewardScreen(reward.clone());
        assert_eq!(
            rust_decision_profile(&session),
            Some(DecisionProfileV1::CombatReward)
        );
        reward.pending_card_choice = Some(Vec::new());
        session.engine_state = EngineState::RewardScreen(reward);
        assert_eq!(
            rust_decision_profile(&session),
            Some(DecisionProfileV1::CardReward)
        );
    }

    #[test]
    fn neow_uses_a_distinct_stable_noncombat_contract() {
        let contract = stable_noncombat_projection_contract(DecisionProfileV1::Neow).unwrap();

        assert_eq!(contract.action_space_schema, NEOW_ACTION_SPACE_SCHEMA_V1);
        assert_eq!(
            contract.profile_projection_schemas,
            NEOW_PROFILE_PROJECTION_SCHEMAS_V1
        );
        assert!(required_projection_domains(DecisionProfileV1::Neow)
            .contains(&ProjectionDomainV1::Relics));
        assert!(required_projection_domains(DecisionProfileV1::Neow)
            .contains(&ProjectionDomainV1::Potions));
    }

    #[test]
    fn rust_neow_action_space_uses_simulator_owned_identities() {
        use sts_oracle_learning::content::events::neow::handle_choice;

        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        handle_choice(&mut session.engine_state, &mut session.run_state, 0);

        let projected = rust_neow_action_space(&session).unwrap();
        let actions: Vec<StoredActionProjectionV1> =
            serde_json::from_value(projected.clone()).unwrap();
        assert_eq!(actions.len(), 4);
        assert!(actions
            .iter()
            .all(|action| matches!(action, StoredActionProjectionV1::ChooseNeowOption { .. })));

        let observed = build_stable_neow_action_observations(
            &format!("state-root-v2-{}", "9".repeat(64)),
            "paired-action-space-v1-neow-fixture",
            projected,
        )
        .unwrap();
        assert_eq!(observed.len(), 4);
        assert!(observed
            .iter()
            .all(|item| matches!(item.handle.action, PairedActionV1::ChooseNeowOption { .. })));
    }

    #[test]
    fn rust_neow_rng_scope_ends_when_the_reward_has_settled() {
        use sts_oracle_learning::content::events::neow::handle_choice;

        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        handle_choice(&mut session.engine_state, &mut session.run_state, 0);

        let owners = |value: Value| {
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|stream| stream["owner"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert!(owners(rust_neow_rng_streams(&session).unwrap())
            .iter()
            .any(|owner| owner == JAVA_NEOW_RNG_ID));

        session
            .run_state
            .event_state
            .as_mut()
            .unwrap()
            .current_screen = 2;
        assert!(owners(rust_neow_rng_streams(&session).unwrap())
            .iter()
            .all(|owner| owner != JAVA_NEOW_RNG_ID));
    }

    #[test]
    fn neow_verifies_only_scoped_closed_shapes_and_the_explicit_leave() {
        let proof = BlobRefV1 {
            sha256: "7".repeat(64),
            bytes: 2,
            media_type: "application/json".to_owned(),
        };
        let plan = capability_surface_plan_for_profile(
            DecisionProfileV1::Neow,
            &RunControlSession::new(Default::default()),
        )
        .unwrap();
        let (gaps, capabilities) = plan.materialize(&proof);

        assert_eq!(capabilities.len(), 7);
        assert!(capabilities
            .windows(2)
            .all(|pair| pair[0].capability < pair[1].capability));
        assert!(capabilities.iter().any(|item| {
            item.capability == CapabilityKindV1::Observe
                && matches!(item.status, CapabilityStatusV1::Verified { .. })
        }));
        assert!(capabilities.iter().any(|item| {
            item.capability == CapabilityKindV1::ChooseNeowHundredGoldNoDrawback
                && matches!(item.status, CapabilityStatusV1::Verified { .. })
        }));
        assert!(capabilities.iter().any(|item| {
            item.capability == CapabilityKindV1::ChooseNeowThreeEnemyKillNoDrawback
                && matches!(item.status, CapabilityStatusV1::Verified { .. })
        }));
        assert!(capabilities.iter().any(|item| {
            item.capability == CapabilityKindV1::LeaveNeow
                && matches!(item.status, CapabilityStatusV1::Verified { .. })
        }));
        for capability in [
            CapabilityKindV1::ChooseNeowClosed,
            CapabilityKindV1::ChooseNeowDeckSelection,
            CapabilityKindV1::ChooseNeowReward,
        ] {
            assert!(capabilities.iter().any(|item| {
                item.capability == capability
                    && matches!(item.status, CapabilityStatusV1::Unknown { .. })
            }));
        }
        assert_eq!(gaps.len(), 3);
        for capability in &capabilities {
            if let CapabilityStatusV1::Unknown { gap_id } = &capability.status {
                assert!(gaps.iter().any(|gap| gap.gap_id == *gap_id));
            }
        }
        assert_eq!(
            capability_for_action(&PairedActionV1::ChooseNeowOption {
                reward: NeowRewardIdentityV1::ThreeEnemyKill,
                drawback: NeowDrawbackIdentityV1::None,
            }),
            CapabilityKindV1::ChooseNeowThreeEnemyKillNoDrawback
        );
        assert_eq!(
            capability_for_action(&PairedActionV1::ChooseNeowOption {
                reward: NeowRewardIdentityV1::RemoveCard,
                drawback: NeowDrawbackIdentityV1::None,
            }),
            CapabilityKindV1::ChooseNeowDeckSelection
        );
        assert_eq!(
            capability_for_action(&PairedActionV1::ChooseNeowOption {
                reward: NeowRewardIdentityV1::ThreeCards,
                drawback: NeowDrawbackIdentityV1::None,
            }),
            CapabilityKindV1::ChooseNeowReward
        );
        assert_eq!(
            capability_for_action(&PairedActionV1::ChooseNeowOption {
                reward: NeowRewardIdentityV1::HundredGold,
                drawback: NeowDrawbackIdentityV1::None,
            }),
            CapabilityKindV1::ChooseNeowHundredGoldNoDrawback
        );
        assert_eq!(
            capability_for_action(&PairedActionV1::ChooseNeowOption {
                reward: NeowRewardIdentityV1::HundredGold,
                drawback: NeowDrawbackIdentityV1::NoGold,
            }),
            CapabilityKindV1::ChooseNeowClosed
        );
    }

    #[test]
    fn combat_profile_is_rejected_by_stable_noncombat_contract() {
        let error = stable_noncombat_projection_contract(DecisionProfileV1::CombatPlayerDecision)
            .unwrap_err();

        assert!(error.contains("combat player decisions"));
        assert!(error.contains("do not use"));
    }

    #[test]
    fn legacy_java_frame_missing_identity_and_pool_fields_stops_as_typed_coverage() {
        let mut frame = covered_frame();
        frame["parity_internal"]
            .as_object_mut()
            .unwrap()
            .remove("content_profile");
        frame["parity_internal"]
            .as_object_mut()
            .unwrap()
            .remove("master_deck");
        frame["parity_internal"]
            .as_object_mut()
            .unwrap()
            .remove("card_pools");

        let gaps = required_java_frame_coverage_gaps(&frame);
        assert!(gaps.iter().any(|gap| {
            gap.domain == ProjectionDomainV1::MasterDeck
                && gap.scope == CoverageGapScopeV1::StateProjection
                && gap.kind == "required_java_field_absent"
        }));
        assert!(gaps.iter().any(|gap| {
            gap.domain == ProjectionDomainV1::CardBindings
                && gap.scope == CoverageGapScopeV1::StateProjection
                && gap.kind == "source_identity_evidence_absent"
        }));
        assert!(gaps.iter().any(|gap| {
            gap.domain == ProjectionDomainV1::CardPools
                && gap.scope == CoverageGapScopeV1::StateProjection
                && gap.kind == "required_java_field_absent"
        }));
        assert!(gaps.iter().any(|gap| {
            gap.domain == ProjectionDomainV1::RunScalars && gap.detail.contains("content_profile")
        }));
    }

    #[test]
    fn complete_first_slice_frame_has_no_structural_coverage_gap() {
        assert!(required_java_frame_coverage_gaps(&covered_frame()).is_empty());
    }

    #[test]
    fn adapter_only_producer_change_preserves_semantic_compatibility() {
        let stored = compatible_root_versions();
        let mut active = compatible_active_versions();
        active.insert("run_play_workbench", "new-adapter-build".to_owned());

        let report = compare_supported_root_versions(&stored, &active).unwrap();
        let workbench = report
            .iter()
            .find(|version| version.component == "run_play_workbench")
            .unwrap();
        assert_eq!(workbench.enforcement, "producer_provenance");
        assert_eq!(workbench.status, "producer_changed_semantics_compatible");
    }

    #[test]
    fn hexadecimal_build_identities_are_case_insensitive() {
        let mut stored = compatible_root_versions();
        stored
            .iter_mut()
            .find(|version| version.component == "communicationmod_jar")
            .unwrap()
            .identity = "ABCDEF0123456789".to_owned();
        let mut active = compatible_active_versions();
        active.insert("communicationmod_jar", "abcdef0123456789".to_owned());

        let report = compare_supported_root_versions(&stored, &active).unwrap();
        assert_eq!(
            report
                .iter()
                .find(|version| version.component == "communicationmod_jar")
                .unwrap()
                .status,
            "exact"
        );
    }

    #[test]
    fn simulator_semantics_drift_stops_native_activation() {
        let stored = compatible_root_versions();
        let mut active = compatible_active_versions();
        active.insert("rust_simulator_semantics", "changed-semantics".to_owned());

        let error = compare_supported_root_versions(&stored, &active).unwrap_err();
        assert!(error.contains("version drift for rust_simulator_semantics"));
    }

    #[test]
    fn unknown_version_component_stops_native_activation() {
        let mut stored = compatible_root_versions();
        stored.push(VersionIdentityV1 {
            component: "unreviewed_runtime".to_owned(),
            identity: "v1".to_owned(),
        });

        let error =
            compare_supported_root_versions(&stored, &compatible_active_versions()).unwrap_err();
        assert!(error.contains("version component set is unsupported"));
    }

    #[test]
    fn master_deck_binding_refuses_non_master_lineage_without_placeholder_identity() {
        assert_eq!(
            project_rust_lineage(CardLineageId::master_deck(7)).unwrap(),
            RustCardLineageV1::MasterDeck {
                acquisition_ordinal: 7
            }
        );
        assert!(project_rust_lineage(CardLineageId::combat_generated(7)).is_err());
        assert!(project_rust_lineage(CardLineageId::constructed_scenario(7)).is_err());
    }

    #[test]
    fn action_space_difference_reports_the_first_typed_field() {
        let java = json!([{"kind": "play_card", "binding_id": "binding-a"}]);
        let rust = json!([{"kind": "play_card", "binding_id": "binding-b"}]);
        let difference = first_json_difference("$", &java, &rust).unwrap();
        assert_eq!(difference.path, "$[0].binding_id");
        assert_eq!(difference.kind, "value_mismatch");
    }

    #[test]
    fn java_weak_potion_and_rust_weaken_potion_share_one_typed_identity() {
        assert_eq!(canonical_potion_id("Weak Potion"), "weakenpotion");
        assert_eq!(canonical_potion_id("WeakenPotion"), "weakenpotion");
    }

    #[test]
    fn post_neow_rng_projection_drops_only_the_dead_event_owned_stream() {
        let stream = |owner: &str, scope: &str| {
            json!({
                "owner": owner,
                "scope": scope,
                "state_hex": ["0000000000000001", "0000000000000002"],
                "counter": 0,
            })
        };
        let frame = json!({
            "rng_internal": {
                "streams": [stream(JAVA_MISC_RNG_ID, "floor")],
                "run_streams": [
                    stream("com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng", "run"),
                    stream(JAVA_NEOW_RNG_ID, "run"),
                ],
            },
        });

        let projected = java_post_neow_rng_streams(&frame).unwrap();
        let owners = projected
            .as_array()
            .unwrap()
            .iter()
            .map(|stream| stream["owner"].as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(owners.len(), 2);
        assert!(owners.contains(&JAVA_MISC_RNG_ID));
        assert!(owners.contains(&"com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng"));
        assert!(!owners.contains(&JAVA_NEOW_RNG_ID));
    }

    #[test]
    fn duplicate_source_sessions_are_reusable_only_when_head_inventories_are_identical() {
        let inventory = SourceHeadInventoryV1 {
            sha256: "a".repeat(64),
            file_count: 1,
        };
        let selected = choose_equivalent_source_session(
            vec![
                (PathBuf::from("second"), inventory.clone()),
                (PathBuf::from("first"), inventory.clone()),
            ],
            "session",
        )
        .unwrap();
        assert_eq!(selected, PathBuf::from("first"));

        let divergent = choose_equivalent_source_session(
            vec![
                (PathBuf::from("first"), inventory),
                (
                    PathBuf::from("second"),
                    SourceHeadInventoryV1 {
                        sha256: "b".repeat(64),
                        file_count: 1,
                    },
                ),
            ],
            "session",
        )
        .unwrap_err();
        assert!(divergent.contains("2 divergent sessions"));
        assert!(choose_equivalent_source_session(Vec::new(), "session")
            .unwrap_err()
            .contains("no session"));
    }

    #[test]
    fn source_session_resolution_requires_the_root_semantics_and_archive_position() {
        let head = json!({
            "session_id": "session-v1",
            "position": {"kind": "decision", "archive_node_id": "node-v1"},
            "evidence_identity": {
                "simulator_semantics_id": "sim-v14",
                "rng_semantics_id": "rng-v1",
            },
        });

        assert!(source_head_matches_recovery_identity(
            &head,
            "session-v1",
            "node-v1",
            "sim-v14",
            "rng-v1",
        ));
        assert!(!source_head_matches_recovery_identity(
            &head,
            "session-v1",
            "node-v1",
            "sim-v13",
            "rng-v1",
        ));
        assert!(!source_head_matches_recovery_identity(
            &head,
            "session-v1",
            "other-node",
            "sim-v14",
            "rng-v1",
        ));
    }

    #[test]
    fn descendant_recovery_inherits_one_logical_run_play_source() {
        let origin = vec![
            IdentityClaimV1 {
                kind: "archive_node_id".to_owned(),
                value: "node-v1".to_owned(),
            },
            IdentityClaimV1 {
                kind: "run_play_session_id".to_owned(),
                value: "session-v1".to_owned(),
            },
        ];
        let descendant = vec![IdentityClaimV1 {
            kind: "parent_root_id".to_owned(),
            value: format!("state-root-v2-{}", "a".repeat(64)),
        }];

        assert_eq!(
            inherited_run_play_source_identity([origin.as_slice(), descendant.as_slice()]).unwrap(),
            ("session-v1".to_owned(), "node-v1".to_owned())
        );
        assert!(
            inherited_run_play_source_identity([origin.as_slice(), origin.as_slice(),])
                .unwrap_err()
                .contains("multiple")
        );
        assert!(inherited_run_play_source_identity([origin[..1].as_ref()])
            .unwrap_err()
            .contains("incomplete"));
    }

    #[test]
    fn java_potion_projection_uses_belt_index_and_omits_empty_placeholders() {
        let frame = json!({
            "public_base": {"combat": {"potions": [
                {
                    "slot_index": 0,
                    "potion_slot": -1,
                    "potion_id": "Potion Slot",
                    "empty": true,
                    "can_use": false,
                    "can_discard": false,
                    "target_required": false
                },
                {
                    "slot_index": 1,
                    "potion_slot": 1,
                    "potion_id": "Fire Potion",
                    "empty": false,
                    "can_use": true,
                    "can_discard": true,
                    "target_required": true
                }
            ]}}
        });

        assert_eq!(
            java_potions(&frame).unwrap(),
            json!([{
                "slot": 1,
                "id": "firepotion",
                "can_use": true,
                "can_discard": true,
                "target_required": true
            }])
        );
    }

    #[test]
    fn persistent_potion_projection_preserves_empty_slot_capacity() {
        let frame = json!({
            "parity_internal": {"potions": [
                {
                    "slot_index": 0,
                    "potion_slot": -1,
                    "potion_id": "Potion Slot",
                    "empty": true,
                    "can_use": false,
                    "can_discard": false,
                    "target_required": false
                },
                {
                    "slot_index": 1,
                    "potion_slot": 1,
                    "potion_id": "Fire Potion",
                    "empty": false,
                    "can_use": false,
                    "can_discard": true,
                    "target_required": true
                }
            ]}
        });
        let mut session = RunControlSession::new(Default::default());
        session.run_state.potions = vec![
            None,
            Some(sts_oracle_learning::content::potions::Potion::new(
                PotionId::FirePotion,
                91,
            )),
        ];

        let expected = json!([
            {"slot": 0, "id": null},
            {"slot": 1, "id": "firepotion"}
        ]);
        assert_eq!(java_persistent_potions(&frame).unwrap(), expected);
        assert_eq!(rust_persistent_potions(&session), expected);
    }

    #[test]
    fn combat_relic_projection_reads_battle_start_state_not_the_run_shell() {
        let mut session = session_with_active_combat();
        let mut persistent_lament = RelicState::new(RelicId::NeowsLament);
        persistent_lament.counter = 3;
        session.run_state.relics = vec![persistent_lament];

        let mut combat_lament = RelicState::new(RelicId::NeowsLament);
        combat_lament.counter = 2;
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .entities
            .player
            .relics = vec![combat_lament];

        assert_eq!(
            rust_combat_relics(&session).unwrap()[0]["counter"],
            json!(2)
        );
        assert_eq!(
            rust_persistent_relics(&session).unwrap()[0]["counter"],
            json!(3)
        );
    }

    #[test]
    fn expired_neows_lament_used_up_state_is_visible_in_both_projections() {
        let frame = json!({
            "parity_internal": {
                "relics": [{
                    "index": 0,
                    "relic_id": "NeowsBlessing",
                    "counter": -2,
                }],
            },
        });
        let mut session = session_with_active_combat();
        let mut lament = RelicState::new(RelicId::NeowsLament);
        lament.counter = -2;
        lament.used_up = true;
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .entities
            .player
            .relics = vec![lament];

        let java = java_relics(&frame).unwrap();
        let rust = rust_combat_relics(&session).unwrap();
        assert_eq!(java, rust);
        assert_eq!(java[0]["used_up"], json!(true));
    }

    #[test]
    fn unsupported_non_default_relic_runtime_state_remains_a_coverage_gap() {
        let mut burning_blood = RelicState::new(RelicId::BurningBlood);
        burning_blood.used_up = true;
        assert!(rust_relic_state_projection(&[burning_blood], "test")
            .unwrap_err()
            .contains("unsupported used_up"));

        let mut lament = RelicState::new(RelicId::NeowsLament);
        lament.amount = 1;
        assert!(rust_relic_state_projection(&[lament], "test")
            .unwrap_err()
            .contains("non-default amount"));
    }

    #[test]
    fn map_rng_counter_is_not_claimed_cross_runtime_when_state_is_exact() {
        assert_eq!(
            comparable_rng_counter("com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng", 96),
            Value::Null
        );
        assert_eq!(
            comparable_rng_counter(
                "com.megacrit.cardcrawl.dungeons.AbstractDungeon.cardRng",
                21
            ),
            json!(21)
        );
    }

    #[test]
    fn paired_root_source_preflight_accepts_current_semantics() {
        let identity = RunPlayEvidenceIdentityV1::current_v1();
        assert_eq!(
            require_current_source_semantics_identity_v1(Some(&identity)).unwrap(),
            &identity
        );
    }

    #[test]
    fn paired_root_source_preflight_rejects_unscoped_and_historical_semantics() {
        let unscoped = require_current_source_semantics_identity_v1(None).unwrap_err();
        assert!(unscoped.contains("legacy/unscoped"));

        let mut historical = RunPlayEvidenceIdentityV1::current_v1();
        historical.simulator_semantics_id = "sts-simulator-run-semantics-20260901-v10".to_owned();
        let drift = require_current_source_semantics_identity_v1(Some(&historical)).unwrap_err();
        assert!(drift.contains("rust_simulator_semantics"));
        assert!(drift.contains("v10"));
        assert!(drift.contains(&RunPlayEvidenceIdentityV1::current_v1().simulator_semantics_id));
        assert!(drift.contains("fresh current-generation natural run"));
        assert!(drift.contains("arbitrary historical replay"));
    }

    #[test]
    fn stable_birth_proof_excludes_fresh_runtime_identity() {
        let witness = |request: &str, producer: &str, product: &str| {
            serde_json::from_value::<JavaCardBirthWitness>(json!({
                "kind": "card_birth_witness_v1",
                "from_frame_id": 7,
                "to_frame_id": 8,
                "request_id": request,
                "action_id": format!("play_card:{producer}:monster:2"),
                "cause": {"kind": "card_play", "producer_java_uuid": producer},
                "complete": true,
                "products": [{
                    "generation_index": 0,
                    "java_uuid": product,
                    "card_id": "Anger",
                    "upgrades": 0,
                    "first_zone": "discard"
                }]
            }))
            .unwrap()
        };
        let first = witness("request-a", "producer-a", "product-a");
        let second = witness("request-b", "producer-b", "product-b");
        let stable_cause = json!({
            "kind": "card_play",
            "producer_binding_id": format!("paired-card-binding-v1-{}", "a".repeat(64))
        });

        let first_proof = stable_java_birth_proof_v1(stable_cause.clone(), &first).unwrap();
        let second_proof = stable_java_birth_proof_v1(stable_cause.clone(), &second).unwrap();

        assert_eq!(first_proof, second_proof);
        let encoded = serde_json::to_string(&first_proof).unwrap();
        assert!(!encoded.contains("request-a"));
        assert!(!encoded.contains("producer-a"));
        assert!(!encoded.contains("product-a"));
        let mut missing_zone = first;
        missing_zone.products[0].first_zone = None;
        assert!(stable_java_birth_proof_v1(stable_cause, &missing_zone).is_err());
    }
}
