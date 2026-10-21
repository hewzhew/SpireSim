mod accepted_combat_line_evidence;
mod atomic_combat_search;
mod atomic_combat_search_attempt;
mod atomic_combat_search_rejection;
mod atomic_combat_search_render;
mod atomic_combat_search_setup;
mod auto_capture;
mod auto_step;
mod boss_relic_policy_prior;
mod bounded_run_driver;
mod campfire_policy_prior;
mod card_reward_policy_prior;
mod combat_auto_policy;
mod combat_candidate_line;
mod combat_case_adjudication;
mod combat_case_candidate_census;
mod combat_case_retained_candidates;
mod combat_complete_line_repair;
mod combat_complete_line_scoring;
mod combat_complete_line_search;
mod combat_complete_line_solver;
mod combat_line_adjudication;
mod combat_line_executor;
mod combat_line_outcome;
mod combat_line_selector;
mod combat_line_trace;
mod combat_no_win_fallback;
mod combat_quality_target;
mod combat_resolution;
mod combat_start;
mod decision_action;
mod decision_case;
mod decision_surface;
#[cfg(test)]
mod decision_surface_tests;
mod decision_transaction;
mod exact_run_model;
mod forced_transition;
mod input_gate;
mod map_projection;
mod next_hint;
mod noncombat_boundary;
mod noncombat_policy_annotation;
mod oracle_combat_policy;
mod oracle_combat_witness_budget;
mod oracle_combat_witness_contract;
mod oracle_combat_witness_options;
mod oracle_combat_witness_work;
mod oracle_neow;
mod oracle_resident_combat_witness_evidence;
mod oracle_resident_combat_witness_job;
mod oracle_run_explorer;
mod oracle_selection_cursor;
pub mod outcome;
mod panels;
#[cfg(test)]
mod pending_choice_card_contract_tests;
mod persistent_burden_cutpoint_probe;
mod planner_boundary_capture;
mod planner_capture;
mod potion_rescue_policy;
mod potion_reward_policy_prior;
mod progress_journal;
mod progress_options;
mod progress_replay;
mod progress_step;
pub mod registry;
mod render;
mod reward_auto;
mod reward_policy_progress;
mod route_policy;
mod route_policy_prior;
mod run_choice_policy_audit;
mod run_policy_evidence;
mod run_policy_prior;
mod selection_surface;
mod session;
mod session_trace;
mod shop_legal;
mod shop_policy_prior;
mod strategic_checkpoint_probe;
mod strategic_encounter_probe;
mod strategic_mechanism_probe;
mod strategic_probe_calibration;
mod trace_annotation;
mod transition_report;
mod view_model;

pub use accepted_combat_line_evidence::{
    accepted_combat_line_evidence_v1, AcceptedCombatLineEvidenceV1,
};
pub use atomic_combat_search::AtomicCombatSearchWorkV2;
pub use atomic_combat_search_attempt::{
    AtomicCombatSearchAttemptV2, AtomicVerifiedCombatCandidateV2,
};
pub use auto_capture::AutoCombatCaptureConfig;
pub use boss_relic_policy_prior::{
    exact_boss_relic_policy_audit_v1, exact_boss_relic_policy_decision_v1,
    exact_boss_relic_policy_prior_v1, BossRelicActionSupplyDeltaV1, BossRelicEnergyPressureV1,
    BossRelicPolicyActionEvidenceV1, BossRelicPolicyActionV1, BossRelicPolicyAuditCandidateV1,
    BossRelicPolicyAuditContextV1, BossRelicPolicyBandV1, BossRelicPolicyFollowupV1,
    BossRelicStartupDeltaV1, ExactBossRelicPolicyAuditV1, ExactBossRelicPolicyDecisionV1,
};
pub use bounded_run_driver::{
    BoundedRunDriveErrorV1, BoundedRunDriveResultV1, BoundedRunDriveStopV1, BoundedRunDriver,
    BoundedRunResultV1, BoundedRunStepContextV1, BoundedRunStepControlV1,
};
pub use campfire_policy_prior::{
    campfire_rest_is_efficient_recovery_v1, exact_campfire_policy_audit_v1,
    exact_campfire_policy_decision_v1, exact_campfire_policy_prior_v1,
    CampfirePolicyActionEvidenceV1, CampfirePolicyActionV1, CampfirePolicyAuditCandidateV1,
    CampfirePolicyBandV1, CampfireRecoveryContextV1, ExactCampfirePolicyAuditV1,
    ExactCampfirePolicyDecisionV1,
};
pub use card_reward_policy_prior::{
    exact_card_reward_policy_audit_v1, exact_card_reward_policy_decision_v1,
    exact_card_reward_policy_prior_v1, exact_direct_card_acquisition_policy_audit_v1,
    exact_direct_card_acquisition_policy_decision_v1,
    exact_direct_card_acquisition_policy_prior_v1, CardRewardBossDamagePlanImprovementV1,
    CardRewardPolicyAcquisitionV1, CardRewardPolicyActionEvidenceV1,
    CardRewardPolicyAuditCandidateV1, CardRewardPolicyBandV1, ExactCardRewardPolicyAuditV1,
    ExactCardRewardPolicyDecisionV1, EXACT_CARD_REWARD_POLICY_AUDIT_SCHEMA_NAME,
    EXACT_CARD_REWARD_POLICY_AUDIT_SCHEMA_VERSION,
};
pub use combat_case_adjudication::{
    adjudicate_combat_case_line_v1, CombatCaseAdjudicationProbeV1, COMBAT_CASE_PROJECTION_TRUST_V1,
};
pub use combat_case_candidate_census::{
    adjudicate_combat_case_candidates_v1, CombatCaseCandidateAdjudicationCensusV1,
    CombatCaseCandidateCensusConclusionV1, CombatCaseCandidateOutcomeSummaryV1,
    CombatCaseCandidateReplayFailureV1, CombatCaseGainedCurseCountV1,
};
pub use combat_line_adjudication::{
    CombatLineAdjudicationV1, CombatLineCleanlinessV1, CombatLineObservedOutcomeV1,
    CombatLineRejectionReasonV1,
};
pub use combat_quality_target::{
    strategic_combat_persistent_payoff_matters_v1, strategic_combat_quality_hp_loss_limit_v1,
    strategic_combat_survival_hp_loss_limit_v1, strategic_combat_victory_reaches_full_heal_v1,
};
pub use combat_resolution::{
    RunCombatResolutionBoundaryV1, RunCombatResolutionKindV1, RunCombatResolutionV1,
    RUN_COMBAT_RESOLUTION_SCHEMA_NAME, RUN_COMBAT_RESOLUTION_SCHEMA_VERSION,
};
pub use decision_action::RunDecisionAction;
pub use decision_case::{
    default_run_decision_case_path, save_run_decision_case_v3, RunDecisionCaseV3,
    RUN_DECISION_CASE_SCHEMA_NAME, RUN_DECISION_CASE_SCHEMA_VERSION,
};
pub use decision_surface::{build_decision_surface, DecisionSurface};
pub use decision_transaction::{
    RunDecisionBoundaryV1, RunDecisionCandidateSnapshotV1, RunDecisionSelectionSourceV1,
    RunDecisionSelectionV1, RunDecisionTransactionV1, RUN_DECISION_TRANSACTION_SCHEMA_NAME,
    RUN_DECISION_TRANSACTION_SCHEMA_VERSION,
};
pub use exact_run_model::{exact_run_decision_successor_v1, ExactRunDecisionSuccessorV1};
pub use forced_transition::{
    RunForcedTransitionKindV1, RunForcedTransitionV1, RUN_FORCED_TRANSITION_SCHEMA_NAME,
    RUN_FORCED_TRANSITION_SCHEMA_VERSION,
};
pub use oracle_combat_policy::{
    authorized_potion_trial_policy_v1, darkling_survival_guide_service_bias_v1,
    retained_combat_tactical_policy_v1,
};
pub use oracle_combat_witness_budget::{
    OracleRunCombatWitnessBudgetsV1, OracleRunCombatWitnessQualityPolicyV1,
};
pub use oracle_combat_witness_contract::{
    OracleCombatWitnessCandidateDispositionV1, OracleCombatWitnessCheckpointV1,
    OracleCombatWitnessFailureV1,
};
pub use oracle_combat_witness_options::OracleCombatWitnessOptionsV1;
pub use oracle_neow::{
    expand_oracle_neow_candidates_v1, ordered_oracle_neow_root_candidate_ids_v1,
    CompletedNeowCandidateV1, NeowOracleExpansionV1, NeowOracleReplayStepV1,
    UnresolvedNeowCandidateV1,
};
pub use oracle_resident_combat_witness_evidence::OracleResidentCombatWitnessEvidenceV1;
pub use oracle_resident_combat_witness_job::OracleResidentCombatWitnessJobV1;
pub use oracle_run_explorer::{
    drive_oracle_run_explorer_v1, run_control_continuation_fingerprint_v1,
    run_control_session_fingerprint_v3, seed_oracle_run_explorer_from_checkpoint_v1,
    seed_oracle_run_explorer_from_session_v1, seed_oracle_run_explorer_v1,
    ExactDuplicateOracleRunBranchV1, LazyOracleRunDecisionV1, OracleCombatSearchResumeKindV1,
    OraclePendingCombatEnemyV1, OraclePendingCombatSummaryV1, OracleRunActiveCombatCheckpointV1,
    OracleRunBoundaryV1, OracleRunBranchCheckpointV1, OracleRunBranchV1,
    OracleRunCheckpointPayloadsV1, OracleRunCombatEdgeOrderFnV1, OracleRunCombatEdgeProbeV1,
    OracleRunCombatEvidenceKindV1, OracleRunDecisionAnnotationFnV1,
    OracleRunDeferredCombatCheckpointV1, OracleRunExploreBudgetV1, OracleRunExploreResultV1,
    OracleRunExploreStopV1, OracleRunExplorerCheckpointV1, OracleRunExplorerCombatCommitV1,
    OracleRunExplorerDecisionCommitV1, OracleRunExplorerExplicitTransactionsV1,
    OracleRunExplorerV1, OracleRunJournalNodeCheckpointV1, OracleRunReplayStepV1,
    OracleRunSessionPayloadRefsV1, OracleRunUnresolvedCombatV1, OracleRunWorkKindV1,
};
pub use outcome::{
    load_combat_baseline_outcome_v1, save_combat_baseline_outcome_v1, CombatBaselineOutcomeV1,
    COMBAT_BASELINE_OUTCOME_SCHEMA_NAME, COMBAT_BASELINE_OUTCOME_SCHEMA_VERSION,
};

pub fn learning_selection_input_is_allowed_v1(
    session: &RunControlSession,
    input: &crate::state::core::ClientInput,
) -> Option<bool> {
    selection_surface::current_selection_input_is_allowed(session, input)
}
pub use map_projection::{
    planner_public_map_v1, run_map_comparison_topology_v1, RunMapComparisonTopologyV1,
};
pub use persistent_burden_cutpoint_probe::{
    probe_combat_case_persistent_burden_cutpoints_v1, CombatCasePersistentBurdenCutpointProbeV1,
    PersistentBurdenCutpointActionDomainV1, PersistentBurdenCutpointAggregateV1,
    PersistentBurdenCutpointConclusionV1, PersistentBurdenCutpointInputOutcomeKindV1,
    PersistentBurdenCutpointInputOutcomeV1, PersistentBurdenCutpointSummaryV1,
    PersistentBurdenEnemyPlanChangeV1, PersistentBurdenGainedCurseCountV1,
    PERSISTENT_BURDEN_CUTPOINT_LIMIT_V1,
};
pub use planner_boundary_capture::{
    build_planner_boundary_capture_coverage_report_v1, capture_planner_boundary_ticket_v1,
    capture_planner_boundary_yield_v1, planner_shop_inventory_v1, PlannerBoundaryCandidateLinkV1,
    PlannerBoundaryCaptureCoverageReportV1, PlannerBoundaryCaptureSegmentV1,
    PlannerBoundaryCaptureTicketV1, PlannerBoundaryMutationKindV1, PlannerBoundarySiteCoverageV1,
    PlannerBoundaryVisitOutcomeV1, PlannerBoundaryVisitV1, PlannerBoundaryYieldKindV1,
    PLANNER_BOUNDARY_CAPTURE_SEGMENT_SCHEMA_NAME, PLANNER_BOUNDARY_CAPTURE_SEGMENT_SCHEMA_VERSION,
};
pub use planner_capture::{
    build_planner_capture_coverage_report, build_planner_capture_dataset,
    PlannerCaptureCoverageReport, PlannerCaptureDataset, PlannerDecisionSiteCoverage,
};
pub use potion_rescue_policy::{
    oracle_active_victory_potion_slot_mask_v1, oracle_potion_rescue_tier_v1,
    OraclePotionRescueTierV1,
};
pub use potion_reward_policy_prior::{
    potion_reward_policy_audit_v1, PotionRewardHeldEvidenceV1, PotionRewardPolicyAuditV1,
    PotionRewardPolicyDecisionV1, POTION_REWARD_POLICY_AUDIT_SCHEMA_NAME,
    POTION_REWARD_POLICY_AUDIT_SCHEMA_VERSION,
};
pub use progress_journal::{
    RunProgressJournalV1, RUN_PROGRESS_JOURNAL_SCHEMA_NAME, RUN_PROGRESS_JOURNAL_SCHEMA_VERSION,
};
pub use progress_options::{
    AtomicCombatSearchAdvanceV2, AtomicCombatSearchOptionsV2, AtomicCombatSearchQuantumV2,
    OracleCombatWitnessAdvanceV1, OracleCombatWitnessQuantumV1, RunControlAutoStepOptions,
    RunControlCombatSegmentMode, RunControlHpLossLimit, RunControlRouteAutomationMode,
};
pub use progress_replay::{
    exact_audit_run_progress_journal_policy_v1, exact_census_run_progress_journal_combat_roots_v1,
    exact_diagnose_run_progress_journal_v1, exact_replay_run_progress_journal_identity_v1,
    exact_replay_run_progress_journal_prefix_v1, exact_replay_run_progress_journal_v1,
    run_progress_journal_fingerprint_v1, run_progress_journal_prefix_fingerprint_v1,
    splice_exact_combat_resolution_v1, ExactRunProgressReplayReportV1,
    ExactRunWitnessCombatRootCensusV1, ExactRunWitnessDiagnosisReportV1,
    ExactRunWitnessIdentityReportV1, ExactRunWitnessPolicyAuditReportV1,
    RunWitnessCombatRootIdentityV1, RunWitnessCombatRootOriginV1, RunWitnessCombatTimelineEntryV1,
    RunWitnessCurrentHpEpochV1, RunWitnessFullHpResetV1, RunWitnessLineIdentityV1,
    RunWitnessPotionSnapshotV1, RunWitnessRecoveryPivotV1, RunWitnessResourceSnapshotV1,
    RunWitnessStrategicDecisionV1, WitnessPolicyDecisionAuditV1,
    RUN_WITNESS_JOURNAL_FINGERPRINT_ALGORITHM_V1,
};
pub use progress_step::{RunControlAutoStopKind, RunControlAutoStopV1, RunProgressStepV1};
pub use registry::{add_case_to_benchmark_registry, BenchmarkCasePaths};
pub use render::{
    render_auto_applied_step_compact_v1, render_progress_step_compact_v1,
    render_run_control_details, render_run_control_raw, render_run_control_state,
};
pub use reward_auto::{
    apply_reward_policy_step, apply_reward_potion_space_step, reward_policy_has_claimable_step,
    reward_surface_has_only_unclaimable_potions, RewardAutomationConfig,
};
pub use reward_policy_progress::{reward_policy_progress_v1, RewardPolicyProgressV1};
pub use route_policy_prior::{
    exact_route_policy_audit_v1, exact_route_policy_decision_v1, exact_route_policy_prior_v1,
    ExactRoutePolicyAuditV1, ExactRoutePolicyDecisionV1, RoutePolicyActionEvidenceV1,
    RoutePolicyActionV1, RoutePolicyArrivalV1, RoutePolicyAuditCandidateV1, RoutePolicyBandV1,
    RoutePolicyContextV1, RoutePolicyPathEvidenceV1,
};
pub use run_choice_policy_audit::{
    exact_run_choice_policy_audit_v1, ExactRunChoicePolicyAuditV1, RunChoiceDuplicateTargetAuditV1,
    RunChoicePolicyAuditCandidateV1,
};
pub use run_policy_evidence::{
    exact_run_policy_decision_v1, run_policy_state_delta_v1, run_policy_state_evidence_v1,
    ExactRunPolicyActionSuccessorV1, ExactRunPolicyDecisionV1, RunPolicyCapabilityChangeV1,
    RunPolicyCapabilityRuleChangeV1, RunPolicyStateDeltaV1, RunPolicyStateEvidenceV1,
    RunPolicyThreatGapKeyV1,
};
pub use run_policy_prior::{
    owned_run_policy_actions_v1, positive_grouped_run_policy_prior_v1,
    positive_ranked_run_policy_prior_v1, uniform_run_policy_prior_v1, RunActionPriorV1,
    RunPolicyCandidateV1, RunPolicyPriorFnV1, RunPolicyPriorIdentityV1, RunPolicyPriorV1,
};
pub use session::{
    canonical_player_class, AtomicCombatSearchRejectionV2, RecentCombatAttritionV1,
    RunControlAutoAppliedKindV1, RunControlAutoAppliedStepV1, RunControlConfig, RunControlSession,
    RunControlSessionCheckpointV1, RunProgressOutcome, ShopVisitContextV1,
};
pub use session_trace::{
    load_session_trace_v1, SessionTraceArtifactKind, SessionTraceArtifactRefV1,
    SessionTraceBoundaryFingerprintV1, SessionTraceBoundaryRecordV1, SessionTraceCandidateV1,
    SessionTraceCombatFingerprintV1, SessionTraceLineageRoleV1, SessionTraceLineageV1,
    SessionTraceRewardAutomationV1, SessionTraceRunConfigV1, SessionTraceSelectionResolution,
    SessionTraceStepSourceV1, SessionTraceStepV1, SessionTraceV1, SESSION_TRACE_SCHEMA_NAME,
    SESSION_TRACE_SCHEMA_VERSION,
};
pub use shop_legal::render_shop_purchase_block_reason_v1;
pub(crate) use shop_legal::{
    shop_merchandise_purchase_block_reason_v1, shop_potion_purchase_block_reason_v1,
    shop_purge_purchase_block_reason_v1,
};
pub use shop_policy_prior::{
    exact_shop_policy_audit_v1, exact_shop_policy_decision_v1, exact_shop_policy_prior_v1,
    AcquisitionRequirementSupportV1, ExactShopPolicyAuditV1, ExactShopPolicyDecisionV1,
    ShopPolicyAcquisitionV1, ShopPolicyActionEvidenceV1, ShopPolicyAuditCandidateV1,
    ShopPolicyBandV1, ShopPolicyCapabilityChangeV1, ShopPolicyFollowupV1, ShopPolicyThreatGapKeyV1,
};
pub use strategic_checkpoint_probe::{
    run_strategic_checkpoint_probe_decomposition_v1, StrategicCheckpointProbeDecompositionV1,
    StrategicCheckpointProbeOmissionV1, StrategicCheckpointProbeStateSummaryV1,
    StrategicCheckpointProbeVariantKindV1, StrategicCheckpointProbeVariantV1,
    StrategicCheckpointReferenceRelationV1, STRATEGIC_CHECKPOINT_PROBE_SCHEMA_NAME,
    STRATEGIC_CHECKPOINT_PROBE_SCHEMA_VERSION,
};
pub use strategic_encounter_probe::{
    public_encounter_probe_plan_v1, run_strategic_encounter_probe_suite_v1,
    run_strategic_encounter_probes_v1, strategic_encounter_probe_plan_v1,
    strategic_encounter_probe_scenarios_v1, StrategicCapabilityPredictionV1,
    StrategicEncounterEntryPotionExpenditureV1, StrategicEncounterFrontierObservationV1,
    StrategicEncounterHeuristicEvidenceV1, StrategicEncounterPotionExpenditureCountV1,
    StrategicEncounterPrimaryEvidenceV1, StrategicEncounterProbeBudgetReportV1,
    StrategicEncounterProbeBudgetV1, StrategicEncounterProbeEncounterSummaryV1,
    StrategicEncounterProbeHpBasisV1, StrategicEncounterProbeObservationV1,
    StrategicEncounterProbePotionUseV1, StrategicEncounterProbeReportV1,
    StrategicEncounterProbeSpecV1, StrategicEncounterRolloutObservationV1,
    StrategicEncounterWinObservationV1, StrategicEncounterWinResourcesV1,
    STRATEGIC_ENCOUNTER_PROBE_SCHEMA_NAME, STRATEGIC_ENCOUNTER_PROBE_SCHEMA_VERSION,
};
pub use strategic_mechanism_probe::{
    run_strategic_mechanism_probes_v1, strategic_mechanism_probe_plan_v1, StrategicMechanismKindV1,
    StrategicMechanismProbeObservationV1, StrategicMechanismProbeOutcomeV1,
    StrategicMechanismProbeReportV1, StrategicMechanismProbeSpecV1,
    STRATEGIC_MECHANISM_PROBE_SCHEMA_NAME, STRATEGIC_MECHANISM_PROBE_SCHEMA_VERSION,
};
pub use strategic_probe_calibration::{
    run_strategic_probe_calibration_v1, strategic_combat_edge_shadow_order_v1,
    strategic_probe_resolved_label_v1, strategic_probe_shadow_order_key_v1,
    validate_strategic_probe_shadow_ordering_v1, StrategicProbeCalibrationObservationV1,
    StrategicProbeCalibrationPartitionV1, StrategicProbeCalibrationReportV1,
    StrategicProbeFidelityConsistencyV1, StrategicProbeFidelityV1,
    StrategicProbeHeldOutOrderingValidationV1, StrategicProbeOrderingCalibrationCaseV1,
    StrategicProbeOwnerAuthorityV1, StrategicProbeResolvedLabelV1,
    StrategicProbeSchedulingAuthorityV1, StrategicProbeShadowFidelityV1,
    StrategicProbeShadowObservationV1, StrategicProbeShadowOrderKeyV1,
    STRATEGIC_PROBE_CALIBRATION_SCHEMA_NAME, STRATEGIC_PROBE_CALIBRATION_SCHEMA_VERSION,
};
pub use trace_annotation::{
    annotations_have_combat_automation_trajectory_v1, atomic_combat_search_trace_summaries,
    combat_automation_trajectories_v1, AtomicCombatSearchTraceSummaryV2, CardRewardFunctionV1,
    CardRewardObligationDeltaV1, CardRewardObligationSourceV1, CardRewardOwnerProvenanceV1,
    CombatAutomationActionV1, CombatAutomationAnswerClaimV1, CombatAutomationAnswerSourceV1,
    CombatAutomationCardOriginV1, CombatAutomationMonsterStateV1,
    CombatAutomationOpportunityStateV1, CombatAutomationPotionStateV1, CombatAutomationStepStateV1,
    CombatAutomationTrajectoryRecordV1, CombatAutomationTrajectorySource,
    CombatSearchHpLossLimitV1, CombatSearchPerformanceSnapshotV1,
    CombatSearchStrategicHpQualityFactsV1, CombatSearchTerminalLineSummary,
    CombatVictoryContinuationFactsV1, CombatVictoryHpCarryoverV1, RunControlTraceAnnotationV1,
    COMBAT_QUALITY_HP_LIMIT_EVALUATOR_V1, COMBAT_SURVIVAL_HP_LIMIT_EVALUATOR_V1,
    COMBAT_VICTORY_CONTINUATION_EVALUATOR_V1,
};
pub use transition_report::{
    ActionResult as RunActionResultV1, ActionResultChange as RunActionResultChangeV1,
    CardSnapshot as RunActionCardSnapshotV1, CombatPlayerResult as RunActionCombatPlayerResultV1,
    MonsterSnapshot as RunActionMonsterSnapshotV1, PileCounts as RunActionPileCountsV1,
    RunApplyStatus as RunActionApplyStatusV1, RunEndResult as RunActionEndResultV1,
    RunKey as RunActionKeyV1, ValueChange as RunActionValueChangeV1,
};
pub use view_model::DecisionCandidateKey;
