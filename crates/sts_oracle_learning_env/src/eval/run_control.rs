//! Exact single-episode environments; no model projection or policy objective.

pub use sts_oracle_run_control::eval::run_control::*;

#[path = "../../../../src/eval/run_control/combat_learning_env.rs"]
mod combat_learning_env;
#[path = "../../../../src/eval/run_control/combat_learning_root_artifact.rs"]
mod combat_learning_root_artifact;
#[path = "../../../../src/eval/run_control/combat_research_plan.rs"]
mod combat_research_plan;
#[path = "../../../../src/eval/run_control/constructed_campfire.rs"]
mod constructed_campfire;
#[path = "../../../../src/eval/run_control/constructed_card_reward.rs"]
mod constructed_card_reward;
#[path = "../../../../src/eval/run_control/constructed_strategic.rs"]
mod constructed_strategic;
#[path = "../../../../src/eval/run_control/exact_branching_task_world.rs"]
mod exact_branching_task_world;
#[path = "../../../../src/eval/run_control/learning_env.rs"]
mod learning_env;
pub use constructed_campfire::constructed_campfire_env_v1;
pub use constructed_card_reward::{
    constructed_card_reward_env_v1, ConstructedCardRewardRecipeV1, ConstructedRewardOfferV1,
};
pub use constructed_strategic::{ConstructedFirstActLoadoutV1, ConstructedStrategicCardV1};
#[path = "../../../../src/eval/run_control/native_autosave_task_world.rs"]
mod native_autosave_task_world;
#[path = "../../../../src/eval/run_control/privileged_strategic_support.rs"]
mod privileged_strategic_support;
#[path = "../../../../src/eval/run_control/task_world_support.rs"]
mod task_world_support;

pub use combat_learning_env::{
    combat_entry_floor_chance_population_v1, combat_public_chance_particle_checkpoints_v1,
    CombatEntryFloorChancePopulationV1, CombatLearningBoundaryV1, CombatLearningEnvCheckpointV1,
    CombatLearningEnvV1, CombatLearningEpisodeIdentityV1, CombatLearningResourceSnapshotV1,
    CombatLearningRootContextV1, CombatLearningRootIdentityV1, CombatLearningRootV1,
    CombatLearningStepV1, CombatLearningTerminalOutcomeV1,
};
pub use combat_learning_root_artifact::{
    production_combat_case_from_root_artifact_v1, CombatLearningRootArtifactV1,
    CombatLearningRootBatchArtifactV1, COMBAT_LEARNING_ROOT_ARTIFACT_FORMAT_VERSION,
    COMBAT_LEARNING_ROOT_ARTIFACT_MAGIC,
};
pub use combat_research_plan::{
    run_control_session_combat_research_portfolio_with_options_v1,
    run_control_session_whole_combat_portfolio_v1, AtomicCombatResearchPlanChallengerOptionsV1,
    CombatMilestoneClosureResearchOptionsV1, CombatMilestoneResearchCandidateV1,
    CombatMilestoneResearchMaterializationV1, CombatResearchBoundedProgressMaterializationV1,
    CombatResearchBoundedProgressPathMaterializationV1, CombatResearchBoundedProgressPathV1,
    CombatResearchBoundedProgressRoleV1, CombatResearchBoundedProgressStateV1,
    CombatResearchBoundedProgressV1, CombatResearchLaneScheduleV1, CombatResearchLaneV1,
    CombatResearchPlanOptionsV1, CombatResearchPortfolioV1, CombatResearchProgressEventV1,
    CombatResearchReplayMaterializationV1, CombatResearchReplayStepV1,
    CompleteTurnProposalBeamResearchOptionsV1, WholeCombatResearchV1,
    ATOMIC_COMBAT_RESEARCH_PLAN_CHALLENGER_V1, COMPLETE_TURN_PROPOSAL_BEAM_RESEARCH_PLAN_V1,
    MILESTONE_CLOSURE_RESEARCH_PLAN_PRODUCER_V1, RESIDENT_COMBAT_MILESTONE_PLAN_PRODUCER_V1,
    RESIDENT_COMBAT_RESEARCH_PLAN_PRODUCER_V1, WHOLE_COMBAT_RESEARCH_PLAN_PRODUCER_V1,
};
pub use exact_branching_task_world::{
    ironclad_a0_exact_branching_environment_v1, ironclad_a0_exact_branching_task_spec_v1,
    ironclad_a0_privileged_reference_environment_v1,
    ironclad_a0_production_combat_case_exact_branching_environment_v1,
    ironclad_a0_production_combat_case_privileged_reference_environment_v1,
    ironclad_exact_branching_task_spec_v1, ironclad_exact_branching_task_spec_with_objective_v1,
    ironclad_privileged_reference_environment_v1,
    ironclad_privileged_reference_environment_with_neow_and_objective_v1,
    ironclad_privileged_reference_environment_with_neow_v1, ExactBranchingCombatEntryEvidenceV1,
    ExactBranchingCommittedCombatEvidenceV1, ExactBranchingTaskEvidenceRequestV1,
    ExactBranchingTaskExecutionEvidenceV1, ExactBranchingTaskWorldV1,
    SimulatorExactScopeCheckpointIdentityV1, SimulatorExactScopeCheckpointV1,
    SimulatorExactScopeRootV1,
};
#[doc(hidden)]
pub use learning_env::LearningPreparedActionV3;
pub use learning_env::{
    LearningActionV1, LearningBoundaryKindV1, LearningBoundaryV1, LearningCombatBoundaryV1,
    LearningEnvCheckpointV3, LearningEnvV3, LearningObservationCompletenessV1,
    LearningPublicRunContextV1, LearningStepV3, LearningStrategicBoundaryV1,
    LearningStrategicContextKindV1, LearningTerminalOutcomeV1,
};
pub use native_autosave_task_world::{
    ironclad_a0_native_autosave_environment_v1, ironclad_a0_native_autosave_task_spec_v1,
    ironclad_native_autosave_environment_v1, ironclad_native_autosave_task_spec_v1,
    NativeAutosaveTaskWorldV1, SimulatorNativeAutosaveAnchorV1,
};
