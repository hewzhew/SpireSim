//! Exact, resumable planning over complete player-turn options.
//!
//! It turns the simulator's exact legal input surface into replayable options
//! ending at the next supported combat boundary. Optional policies guide work
//! order but never change legality or terminal truth. Partial action prefixes
//! remain private generator work.
//! The crate boundary also keeps planner iteration out of the core unit-test
//! harness; production integration belongs to the control layer.

mod atomic_witness;
mod automatic_action_surface;
mod combat_draft;
mod complete_turn_proposal_beam;
mod depth_beam_turn;
mod generator;
mod local_turn_graph_search;
mod policy;
mod policy_discrepancy_search;
mod replay;
mod search_spec;
mod selection_transaction;
mod trajectory_query;
mod types;
mod witness;

pub use atomic_witness::{
    materialize_exact_action_line, replay_atomic_actions, AtomicWitnessReplayError,
    ExactAtomicWitness,
};
pub use combat_draft::{
    CombatDraftContinuation, CombatDraftError, CombatDraftFrameHandle, CombatDraftOptionCoverage,
    CombatDraftOptionHandle, CombatDraftSelectionOutcome, CombatDraftSelectionOutcomePage,
    CombatDraftSession,
};
pub use complete_turn_proposal_beam::{
    search_complete_turn_proposal_beam_v1,
    search_complete_turn_proposal_beam_with_win_acceptance_v1, CompleteTurnProposalBeamConfigV1,
    CompleteTurnProposalBeamCountersV1, CompleteTurnProposalBeamGuideEnvelopeV1,
    CompleteTurnProposalBeamGuideRepresentativeV1, CompleteTurnProposalBeamInterruptionV1,
    CompleteTurnProposalBeamLayerV1, CompleteTurnProposalBeamQuantumV1,
    CompleteTurnProposalBeamReportV1, CompleteTurnProposalBeamSessionConfigV1,
    CompleteTurnProposalBeamSessionV1, CompleteTurnProposalBeamWinV1,
};
pub use depth_beam_turn::{
    generate_depth_beam_turn_options, DepthBeamTurnBudget, DepthBeamTurnConfig,
    DepthBeamTurnCounters, DepthBeamTurnInterruption, DepthBeamTurnLayerReport,
    DepthBeamTurnReport, DepthBeamTurnStatus,
};
pub use generator::{
    LiveActionTransitionSnapshot, SelectionIntent, SelectionIntentError, SelectionSource,
    SelectionSourcePage, TurnOptionGeneratorSession, DETAIL_TIMING_SAMPLE_INTERVAL,
};
pub use local_turn_graph_search::{
    replay_oracle_combat_witness, root_initial_expansion_work_for_budget,
    summarize_oracle_combat_witness_outcome, LocalTurnGraphDepthServiceSnapshot,
    LocalTurnGraphEdgeSnapshot, LocalTurnGraphGuideServiceSnapshot,
    LocalTurnGraphMilestoneClosurePathSnapshot, LocalTurnGraphPlanAnnotationEnableError,
    LocalTurnGraphPlanTransitionEdgeSnapshot, LocalTurnGraphPlanTransitionPathSnapshot,
    LocalTurnGraphPolicyLineReport, LocalTurnGraphRetainedGuidePromiseSnapshot,
    LocalTurnGraphRootActionFamilySnapshot, LocalTurnGraphServicedStateSnapshot,
    LocalTurnGraphStateSnapshot, LocalTurnGraphStorageSnapshot,
    LocalTurnGraphTerminalOutcomeSnapshotV1, LocalTurnGraphWitnessConfig,
    LocalTurnGraphWitnessCounters, LocalTurnGraphWitnessInterruption, LocalTurnGraphWitnessQuantum,
    LocalTurnGraphWitnessReport, LocalTurnGraphWitnessSession, LocalTurnGraphWitnessStatus,
    PublicPlanTurnProposalConfig, DEFAULT_BACKED_GENERATION_QUANTUM_WORK,
    DEFAULT_ROOT_INITIAL_EXPANSION_WORK,
};
pub use policy::{
    combat_plan_selection_timing_policy_v1, combat_plan_state_guide_policy_v1, CombatActionPolicy,
    CombatGuideLaneId, CombatGuideServiceBias, CombatPlanSelectionTimingPolicyV1,
    CombatPlanStateGuidePolicyV1, CombatPolicyChoice, CombatSelectionPreferences, CombatStateGuide,
    CombatStateGuideRank, SharedCombatActionPolicy, UniformCombatActionPolicy,
    COMBAT_PLAN_STATE_GUIDE_LANE_V1,
};
pub use policy_discrepancy_search::{
    PolicyDiscrepancyConfig, PolicyDiscrepancyCounters, PolicyDiscrepancyInterruption,
    PolicyDiscrepancyQuantum, PolicyDiscrepancyReport, PolicyDiscrepancySession,
    PolicyDiscrepancyStateDiagnostic, PolicyDiscrepancyStatus, PolicyDiscrepancyTrajectoryAudit,
    PolicyDiscrepancyTrajectoryDeviation, PolicyDiscrepancyTurnMacroConfig,
};
pub use replay::{
    replay_turn_option, ReplayError, ReplayFailure, ReplayLimits, VerifiedTurnOptionReplay,
};
pub use search_spec::{
    LocalTurnGraphGuideServiceBiasSpecV1, LocalTurnGraphPlannerSettingsV1,
    LocalTurnGraphSearchAllowanceV1, LocalTurnGraphSearchSpecV1,
};
pub use selection_transaction::SelectionTransactionCursor;
pub use sts_complete_turn_proposal::ExactCompleteTurnProposalConfigV1;
pub use trajectory_query::{
    evaluate_combat_trajectory_query_v1, replay_combat_trajectory_events_v1,
    CombatTrajectoryAtomV1, CombatTrajectoryEventV1, CombatTrajectoryQueryCompileErrorV1,
    CombatTrajectoryQueryEvaluationV1, CombatTrajectoryQueryV1,
    CombatTrajectoryRequirementEvaluationV1,
};
pub use types::{
    CombatDecisionRoot, CombatDecisionRootError, CombatPlanningCounters, CombatPlanningQuantum,
    CompleteTurnOption, CompleteTurnOptionBoundary, GenerationInterruption, ReplaySuccessorHash,
    TurnOptionAction, TurnOptionGenerationDiagnostics, TurnOptionGenerationGap,
    TurnOptionGenerationGapKind, TurnOptionGenerationReport, TurnOptionGenerationStatus,
    TurnOptionGeneratorConfig,
};
pub use witness::{
    exact_trajectory_potion_expenditures, OracleCombatDeepStateSnapshot,
    OracleCombatGuideQueueSnapshot, OracleCombatGuideRankSnapshot, OracleCombatWitness,
    OracleCombatWitnessDiscoverySource, OracleCombatWitnessProgressSnapshot,
    OracleCombatWitnessReplayError, OracleCombatWitnessSatisfaction,
    OracleCombatWitnessStateProgressSnapshot,
};

#[cfg(test)]
mod tests;
