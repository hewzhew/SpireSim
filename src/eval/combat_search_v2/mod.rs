mod benchmark;
mod benchmark_gate;
mod guidance_lab;
mod policy_compare;
mod prior_hints;
mod resource_frontier_probe;
mod rollout_compare_attribution;
mod root_proposal_probe;
mod start;
mod turn_plan_guidance_lab;

#[cfg(any(
    not(feature = "combat-search-driver"),
    feature = "oracle-lab",
    feature = "control-full"
))]
pub use benchmark::validate_combat_search_v2_benchmark_schema_header;
pub use benchmark::{
    load_combat_search_v2_benchmark, run_combat_search_v2_benchmark,
    CombatSearchV2BaselineComparison, CombatSearchV2BaselineOutcomeSpec,
    CombatSearchV2BaselineVerdict, CombatSearchV2BenchmarkBaselineSpec,
    CombatSearchV2BenchmarkCaseReport, CombatSearchV2BenchmarkCaseSpec,
    CombatSearchV2BenchmarkExpectedFingerprints, CombatSearchV2BenchmarkInputKind,
    CombatSearchV2BenchmarkReport, CombatSearchV2BenchmarkSpec, CombatSearchV2BenchmarkSummary,
    CombatSearchV2InputFingerprintReport, CombatSearchV2LoadedBenchmark,
    CombatSearchV2LoadedBenchmarkCase, CombatSearchV2LoadedBenchmarkInput,
};
pub use benchmark_gate::{
    CombatSearchV2BenchmarkGateCase, CombatSearchV2BenchmarkGateCaseMetrics,
    CombatSearchV2BenchmarkGateFocusCount, CombatSearchV2BenchmarkGateReport,
    CombatSearchV2BenchmarkGateRequirements, CombatSearchV2BenchmarkGateStatus,
    CombatSearchV2BenchmarkGateSummary,
};
pub use guidance_lab::{
    run_combat_search_guidance_lab_benchmark_v1, run_combat_search_guidance_lab_v1,
    CombatSearchGuidanceLabBenchmarkCaseV1, CombatSearchGuidanceLabBenchmarkSummaryV1,
    CombatSearchGuidanceLabBenchmarkV1Report, CombatSearchGuidanceLabCandidateV1,
    CombatSearchGuidanceLabChildSearchV1, CombatSearchGuidanceLabRootV1,
    CombatSearchGuidanceLabSummaryV1, CombatSearchGuidanceLabTargetV1,
    CombatSearchGuidanceLabTrajectoryV1, CombatSearchGuidanceLabV1Report,
};
pub use policy_compare::{
    compare_combat_search_v2_rollout_policies, compare_combat_search_v2_turn_plan_policies,
    CombatSearchV2PolicyComparisonCase, CombatSearchV2PolicyComparisonReport,
    CombatSearchV2PolicyComparisonRun, CombatSearchV2PolicyComparisonSummary,
    CombatSearchV2PolicyComparisonVerdict, CombatSearchV2RolloutPolicyComparisonCase,
    CombatSearchV2RolloutPolicyComparisonReport, CombatSearchV2RolloutPolicyComparisonRun,
    CombatSearchV2RolloutPolicyComparisonSummary, CombatSearchV2RolloutPolicyComparisonVerdict,
};
pub use prior_hints::{
    load_combat_root_action_prior_hints_jsonl_v0, load_combat_turn_plan_prior_hints_jsonl_v0,
    parse_combat_root_action_prior_hints_jsonl_v0, parse_combat_turn_plan_prior_hints_jsonl_v0,
};
pub use resource_frontier_probe::{
    run_combat_resource_frontier_probe_v1, CombatResourceFrontierCandidateV1,
    CombatResourceFrontierImprovementV1, CombatResourceFrontierLaneReportV1,
    CombatResourceFrontierProbeConfigV1, CombatResourceFrontierProbeStopV1,
    CombatResourceFrontierProbeV1Report, CombatResourceFrontierServiceObservationV1,
};
pub use rollout_compare_attribution::{
    CombatSearchV2RolloutPolicyFirstActionDiff, CombatSearchV2RolloutPolicyFirstDiffContext,
};
pub use root_proposal_probe::{
    run_combat_root_proposal_priority_matrix_v1, run_combat_root_proposal_probe_v1,
    CombatRootProposalObservationV1, CombatRootProposalOutcomeV1,
    CombatRootProposalPriorityMatrixRunV1, CombatRootProposalPriorityMatrixV1Report,
    CombatRootProposalProbeConfigV1, CombatRootProposalProbeSummaryV1,
    CombatRootProposalProbeV1Report,
};
pub use start::{
    load_combat_search_v2_snapshot, load_combat_search_v2_start, run_combat_search_v2_loaded_start,
    CombatSearchV2LoadedStart, CombatSearchV2RunOptions, CombatSearchV2SingleRun,
};
pub use turn_plan_guidance_lab::{
    run_combat_turn_plan_guidance_lab_benchmark_v1, run_combat_turn_plan_guidance_lab_v1,
    CombatTurnPlanGuidanceActionSequenceAlignmentV1, CombatTurnPlanGuidanceBaselineComparisonV1,
    CombatTurnPlanGuidanceLabBenchmarkCaseV1, CombatTurnPlanGuidanceLabBenchmarkSummaryV1,
    CombatTurnPlanGuidanceLabBenchmarkV1Report, CombatTurnPlanGuidanceLabCandidateV1,
    CombatTurnPlanGuidanceLabSummaryV1, CombatTurnPlanGuidanceLabV1Report,
    CombatTurnPlanGuidanceOutcomeDeltaV1, CombatTurnPlanGuidancePlanSnapshotV1,
    CombatTurnPlanGuidanceSearchSnapshotV1, CombatTurnPlanGuidanceSelectedComparisonV1,
    CombatTurnPlanTacticalTraceV1,
};
