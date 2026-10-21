//! Model-facing projections and batched adapters over exact learning environments.

pub use sts_complete_turn_view::{
    CompleteTurnCandidateEvidenceV1, CompleteTurnCandidateViewV1,
    CompleteTurnCompletedEndpointLayerViewV1, CompleteTurnDecisionViewV1,
    CompleteTurnDemonstrationDecisionV1, CompleteTurnObservationContractV1,
    CompleteTurnPrefixCandidateViewV1, CompleteTurnPrefixLayerViewV1,
    CompleteTurnPublicPrefixBoundaryV1, CompleteTurnPublicPrefixStateV1,
    CompleteTurnSelectedActionV1, CompleteTurnWinningDemonstrationV1,
    COMPLETE_TURN_COMPLETED_ENDPOINT_LAYER_VIEW_SCHEMA, COMPLETE_TURN_DECISION_VIEW_SCHEMA,
    COMPLETE_TURN_PREFIX_LAYER_VIEW_SCHEMA, COMPLETE_TURN_WINNING_DEMONSTRATION_SCHEMA,
};
pub use sts_oracle_learning_env::eval::run_control::*;
pub use sts_run_checkpoint::*;

#[path = "../../../../src/eval/run_control/combat_learning_env_pool.rs"]
mod combat_learning_env_pool;
#[path = "../../../../src/eval/run_control/combat_plan_continuation.rs"]
mod combat_plan_continuation;
#[path = "../../../../src/eval/run_control/exact_branch_planner.rs"]
mod exact_branch_planner;
#[path = "../../../../src/eval/run_control/exact_run_decision_root.rs"]
mod exact_run_decision_root;
#[path = "../../../../src/eval/run_control/learning_env_pool.rs"]
mod learning_env_pool;
#[path = "../../../../src/eval/run_control/learning_model_input.rs"]
mod learning_model_input;
#[path = "../../../../src/eval/run_control/natural_run_root.rs"]
mod natural_run_root;
#[path = "../../../../src/eval/run_control/privileged_reference_actor.rs"]
mod privileged_reference_actor;
#[path = "../../../../src/eval/run_control/public_actor_evaluation.rs"]
mod public_actor_evaluation;
#[path = "../../../../src/eval/run_control/public_deterministic_baseline.rs"]
mod public_deterministic_baseline;
#[path = "../../../../src/eval/run_control/public_information_snapshot.rs"]
mod public_information_snapshot;
#[path = "../../../../src/eval/run_control/public_policy_candidate_admission.rs"]
mod public_policy_candidate_admission;
#[path = "../../../../src/eval/run_control/run_solution_continuation.rs"]
mod run_solution_continuation;
#[path = "../../../../src/eval/run_control/task_legality_smoke_actor.rs"]
mod task_legality_smoke_actor;
#[path = "../../../../src/eval/run_control/task_public_scope_improver.rs"]
mod task_public_scope_improver;

pub use combat_learning_env_pool::{
    CombatLearningEnvPoolError, CombatLearningEnvPoolModelBatchV1, CombatLearningEnvPoolSlotStepV1,
    CombatLearningEnvPoolStepV1, CombatLearningEnvPoolV1,
};
pub use combat_plan_continuation::{
    evaluate_combat_plan_continuations_v1, CombatPlanContinuationBranchV1,
    CombatPlanContinuationEndpointV1, CombatPlanContinuationLimitV1,
    CombatPlanContinuationOptionsV1, CombatPlanContinuationReportV1, CombatPlanContinuationStopV1,
    CombatPlanContinuationWorkV1, COMBAT_PLAN_CONTINUATION_PRODUCER_V1,
    COMBAT_PLAN_CONTINUATION_REVISION_V1,
};
pub use exact_branch_planner::CombatOutcomeFirstControllerV1;
pub use exact_run_decision_root::{
    exact_run_decision_root_artifacts_v1, ExactRunDecisionRootArtifactsV1, ExactRunDecisionSourceV1,
};
pub use learning_env_pool::{
    FixedCombatBatchEvidenceV1, FixedCombatBranchBatchV1, FixedCombatBranchChoiceV1,
    FixedCombatFallbackReceiptV1, FixedCombatFallbackStepV1, FixedCombatOptionsV1,
    FixedCombatReceiptV1, FixedCombatResearchV1, FixedCombatSearchBackendV1,
    FixedCombatStepErrorV1, FixedCombatStepV1, LearningDecisionPathV3, LearningDecisionTokenV3,
    LearningEnvPoolError, LearningEnvPoolModelBatchV3, LearningEnvPoolV3,
    LearningExecutionInputsV1, LearningTransitionV3, FIXED_COMBAT_EXECUTOR_REVISION_V1,
};
pub use learning_model_input::{
    CombatLearningPotionPolicyV1, LearningCandidateSurfaceV1, LearningCombatAtomicActionV1,
    LearningCombatIndexedChoiceV1, LearningCombatModelObservationV1, LearningCombatMonsterV1,
    LearningCombatMonstersV1, LearningCombatSelectionDomainSemanticsV1,
    LearningCombatSelectionDomainV1, LearningCombatSelectionFamilyV1, LearningDenseActionMaskV1,
    LearningModelBatchV1, LearningModelCandidateSemanticsV1, LearningModelCandidateV1,
    LearningModelChoiceV1, LearningModelDecisionV1, LearningModelInputError,
    LearningModelObservationV1, LearningRunSelectionFamilyV1,
    LearningSelectionCandidateSemanticsV1, LearningSelectionCandidateV1,
    LearningSelectionDecisionV1, LearningSelectionDraftV1, LearningSelectionModelBatchV1,
    LearningSelectionModelRowV1, LearningSelectionStepV1, LearningStrategicActionSemanticsV2,
    LearningStrategicActionV2, LearningStrategicCardV2, LearningStrategicModelObservationV1,
    LearningStrategicPotionSlotV1, LearningStrategicPotionV1,
};
pub use natural_run_root::{natural_run_root_artifacts_v1, NaturalRunRootArtifactsV1};
pub use privileged_reference_actor::{
    privileged_reference_task_actor_v1, CombatResearchDeckCardV1,
    CombatResearchPlanAttemptReceiptV1, CombatResearchPlanCandidateReceiptV1,
    CombatResearchPlanSelectionV1, PrivilegedReferenceTaskActorV1,
    PRIVILEGED_REFERENCE_ACTOR_NAME_V1, PRIVILEGED_REFERENCE_ACTOR_REVISION_V1,
};
pub use public_actor_evaluation::{
    evaluate_privileged_reference_actor_census_v9,
    evaluate_privileged_reference_actor_census_with_evidence_v9,
    evaluate_privileged_reference_actor_combat_case_census_v9,
    evaluate_privileged_reference_actor_combat_case_census_with_evidence_v9,
    evaluate_privileged_reference_actor_combat_case_episode_v9,
    evaluate_privileged_reference_actor_combat_case_episode_with_evidence_v9,
    evaluate_privileged_reference_actor_episode_v9,
    evaluate_privileged_reference_actor_episode_with_evidence_v9,
    evaluate_public_deterministic_actor_census_v9, evaluate_public_deterministic_actor_episode_v9,
    evaluate_public_rule_search_actor_census_for_task_v9,
    evaluate_public_rule_search_actor_episode_for_task_v9,
    evaluate_public_structured_actor_census_for_task_v9,
    evaluate_public_structured_actor_census_v9,
    evaluate_public_structured_actor_episode_for_task_v9,
    evaluate_public_structured_actor_episode_v9, PrivilegedReferenceCensusEvidenceV1,
    PrivilegedReferenceEpisodeEvidenceRequestV1, PrivilegedReferenceEpisodeEvidenceV1,
    PublicActorBudgetReceiptV1, PublicActorCensusSummaryV1, PublicActorCensusV9,
    PublicActorCombatActionKindV1, PublicActorCombatActionV1, PublicActorCombatEnemyV1,
    PublicActorCombatExitV1, PublicActorCombatHandCardV1, PublicActorCombatTraceV1,
    PublicActorCombatTurnV1, PublicActorControlReceiptV1, PublicActorDecisionContextV1,
    PublicActorEpisodeReceiptV9, PublicActorExecutionEnvironmentV1, PublicActorLastDecisionV1,
    PublicActorStrategicCandidateV1, PublicActorStrategicDecisionV1,
    PublicActorStrategicDeckCardV1, PublicActorStrategicStateV1, PublicCombatPlanEventCountsV1,
    PUBLIC_ACTOR_CENSUS_SCHEMA_V9, PUBLIC_ACTOR_EPISODE_RECEIPT_SCHEMA_V9,
};
pub use public_deterministic_baseline::{
    public_deterministic_baseline_after_reward_proceed_v1,
    public_deterministic_baseline_capabilities_v1, public_deterministic_baseline_decision_v1,
    public_deterministic_baseline_manifest_v1, public_deterministic_baseline_selection_decision_v1,
    public_deterministic_task_decision_v1, public_rule_search_task_actor_v1,
    public_structured_deterministic_task_actor_v1, PublicBaselineCapabilityV1,
    PublicBaselineCoverageV1, PublicBaselineInformationAccessV1, PublicBaselineLimitationV1,
    PublicDeterministicBaselineDecisionV1, PublicDeterministicBaselineManifestV1,
    PublicDeterministicTaskActorV1, PublicDeterministicTaskDecisionV1, PublicRuleSearchStatsV1,
    PublicRuleSearchTaskActorV1, PublicStructuredDeterministicTaskActorV1,
    BASELINE_LIMIT_COARSE_CONTENT_V1, BASELINE_LIMIT_EVENT_RISK_V1, BASELINE_LIMIT_NO_BELIEF_V1,
    BASELINE_LIMIT_POTION_OPPORTUNITY_V1, BASELINE_LIMIT_SELECTION_V1,
    BASELINE_LIMIT_SHORT_HORIZON_V1, PUBLIC_DETERMINISTIC_BASELINE_MANIFEST_SCHEMA_V1,
    PUBLIC_DETERMINISTIC_BASELINE_NAME_V1, PUBLIC_DETERMINISTIC_BASELINE_REVISION_V1,
    PUBLIC_DETERMINISTIC_TASK_ACTOR_NAME_V1, PUBLIC_RULE_SEARCH_TASK_ACTOR_NAME_V1,
    PUBLIC_RULE_SEARCH_TASK_ACTOR_REVISION_V1, PUBLIC_STRUCTURED_DETERMINISTIC_TASK_ACTOR_NAME_V1,
    PUBLIC_STRUCTURED_DETERMINISTIC_TASK_ACTOR_REVISION_V1,
};
pub use public_information_snapshot::{
    learning_public_information_snapshot_v1,
    learning_public_information_snapshot_with_potion_policy_v1,
    learning_public_information_snapshot_with_surface_v1, learning_public_selection_snapshot_v1,
    learning_public_selection_snapshot_with_surface_v1,
    LEARNING_PUBLIC_CANDIDATE_SURFACE_SCHEMA_NAME,
    LEARNING_PUBLIC_CANDIDATE_SURFACE_SCHEMA_VERSION,
    LEARNING_PUBLIC_COMBAT_HISTORY_SNAPSHOT_SCHEMA_NAME,
    LEARNING_PUBLIC_COMBAT_OBSERVATION_SCHEMA_NAME,
    LEARNING_PUBLIC_COMBAT_OBSERVATION_SCHEMA_VERSION,
    LEARNING_PUBLIC_HISTORY_SNAPSHOT_SCHEMA_VERSION,
    LEARNING_PUBLIC_SELECTION_HISTORY_SNAPSHOT_SCHEMA_NAME,
    LEARNING_PUBLIC_SELECTION_OBSERVATION_SCHEMA_NAME,
    LEARNING_PUBLIC_SELECTION_OBSERVATION_SCHEMA_VERSION,
    LEARNING_PUBLIC_STRATEGIC_HISTORY_SNAPSHOT_SCHEMA_NAME,
    LEARNING_PUBLIC_STRATEGIC_OBSERVATION_SCHEMA_NAME,
    LEARNING_PUBLIC_STRATEGIC_OBSERVATION_SCHEMA_VERSION,
};
pub use run_solution_continuation::{
    resume_privileged_reference_run_continuation_v2, resume_run_until_next_public_decision_v1,
    resume_run_until_next_strategic_decision_v1, task_run_branch_decision_in_validated_session_v1,
    task_run_branch_decision_inventory_v1, task_run_branch_decision_v1,
    task_run_paused_decision_node_id_v1, ExactRunPlayAdvanceV1, ExactRunPlayHotSnapshotV1,
    ExactRunPlayInspectionStopV1, ExactRunPlayInspectionV1, ExactRunPlayOpenProfileV1,
    ExactRunPlayPreparedLineProfileV1, ExactRunPlayPreparedLineV1, ExactRunPlayProbeLineV1,
    ExactRunPlayProbeStepV1, ExactRunPlaySessionV1, ExactRunPlayStepProfileV1, ExactRunPlayStepV1,
    PrivilegedReferenceRunContinuationV2, TaskRunBranchCandidateInventoryV1,
    TaskRunBranchDecisionInventoryArtifactV1, TaskRunBranchDecisionInventoryV1,
    TaskRunForcedPublicDeviationV2, CONTROLLED_RUN_CONTINUATION_ACTOR_NAME_V1,
    CONTROLLED_RUN_CONTINUATION_ACTOR_REVISION_V1, FORCED_RUN_CONTINUATION_ACTOR_NAME_V2,
    FORCED_RUN_CONTINUATION_ACTOR_REVISION_V2, MANUAL_RUN_CONTINUATION_ACTOR_NAME_V1,
    MANUAL_RUN_CONTINUATION_ACTOR_REVISION_V1, TASK_RUN_BRANCH_DECISION_INVENTORY_SCHEMA_V1,
};
pub use task_legality_smoke_actor::{
    first_legal_task_choice_v1, legal_task_choice_for_candidate_v1, FirstLegalTaskActorV1,
    FIRST_LEGAL_TASK_ACTOR_NAME_V1, FIRST_LEGAL_TASK_ACTOR_REVISION_V1,
};
pub use task_public_scope_improver::{
    public_neow_scope_improver_actor_v1, public_neow_scope_improver_traces_v1,
    ConservativePublicDominanceControllerV1, PublicNeowPrefixProposalGeneratorV1,
    PublicNeowScopeImproverActorV1, PublicStrategicEnvelopeEvaluatorV1,
    PUBLIC_NEOW_SCOPE_IMPROVER_NAME_V1, PUBLIC_NEOW_SCOPE_IMPROVER_REVISION_V1,
};

#[cfg(test)]
#[path = "../../../../src/eval/run_control/learning_env/smoke_tests.rs"]
mod learning_env_smoke_tests;
