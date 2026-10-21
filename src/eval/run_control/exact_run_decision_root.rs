//! Fresh complete-run roots recovered from one exact run decision.
//!
//! The input checkpoint owns simulator state and RNG only. The new root gets
//! a fresh learning lineage, empty public history, and a new archive whose
//! initial condition records provenance without depending on an older public
//! observation schema.

use crate::agent::task::{
    run_task_actor_episode_v1, NodeReloadTaskEnvironmentV1, TaskActorIdentityV1, TaskActorIntentV1,
    TaskActorV1, TaskActorViewV1, TaskBudgetV1, TaskInformationAccessV1, TaskInitialConditionV1,
    TaskObjectiveV1, TaskRunSolutionArchiveV1,
};
use sts_run_checkpoint::TaskRunBranchCheckpointArtifactV1;

use super::{
    ironclad_exact_branching_task_spec_with_objective_v1, run_control_session_fingerprint_v3,
    CombatResearchPlanOptionsV1, ExactBranchingTaskEvidenceRequestV1, ExactBranchingTaskWorldV1,
    RunControlSessionCheckpointV1,
};

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExactRunDecisionSourceV1 {
    pub session_id: String,
    pub head_revision: u64,
    pub archive_node_id: String,
    pub public_snapshot_id: String,
}

pub struct ExactRunDecisionRootArtifactsV1 {
    pub archive: TaskRunSolutionArchiveV1,
    pub checkpoints: TaskRunBranchCheckpointArtifactV1,
}

struct ExactRunDecisionRootCaptureActorV1;

impl TaskActorV1 for ExactRunDecisionRootCaptureActorV1 {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only("exact-run-decision-root-capture", 1)
    }

    fn next_intent(&mut self, _view: &TaskActorViewV1) -> TaskActorIntentV1 {
        TaskActorIntentV1::YieldControl
    }
}

pub fn exact_run_decision_root_artifacts_v1(
    mut checkpoint: RunControlSessionCheckpointV1,
    source: ExactRunDecisionSourceV1,
    objective: TaskObjectiveV1,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
) -> Result<ExactRunDecisionRootArtifactsV1, String> {
    budget.validate()?;
    plan_options.validate()?;
    checkpoint.clear_combat_diagnostics_for_external_checkpoint();
    for (description, value) in [
        ("source session id", source.session_id.as_str()),
        ("source archive node id", source.archive_node_id.as_str()),
        (
            "source public snapshot id",
            source.public_snapshot_id.as_str(),
        ),
    ] {
        if value.trim().is_empty() {
            return Err(format!(
                "exact run decision root requires a non-empty {description}"
            ));
        }
    }

    let session = checkpoint.clone().into_session()?;
    let player_class = match super::canonical_player_class(session.run_state.player_class)? {
        "Ironclad" => crate::ai::planner_core::PlannerPlayerClass::Ironclad,
        "Silent" => crate::ai::planner_core::PlannerPlayerClass::Silent,
        "Defect" => crate::ai::planner_core::PlannerPlayerClass::Defect,
        "Watcher" => crate::ai::planner_core::PlannerPlayerClass::Watcher,
        _ => return Err("exact run root has an unsupported character".into()),
    };
    let seed = session.run_state.seed;
    let ascension_level = session.run_state.ascension_level;
    let act = session.run_state.act_num;
    let floor = session.run_state.floor_num;
    let root_exact_state_hash = run_control_session_fingerprint_v3(&session);

    let mut task =
        ironclad_exact_branching_task_spec_with_objective_v1(ascension_level, objective)?;
    task.player_class = player_class;
    task.initial_condition = TaskInitialConditionV1::ExactRunDecision {
        source_seed: seed,
        act,
        floor,
        root_exact_state_hash,
        source_session_id: source.session_id,
        source_head_revision: source.head_revision,
        source_archive_node_id: source.archive_node_id,
        source_public_snapshot_id: source.public_snapshot_id,
    };
    task.information_access =
        TaskInformationAccessV1::PublicHistoryLegalReloadMemoryAndPrivilegedSupport;
    task.validate()?;

    let world = ExactBranchingTaskWorldV1::from_run_decision_checkpoint_v1(checkpoint)?
        .with_combat_research_plans(plan_options)?
        .with_execution_evidence_request(evidence_request)?;
    let mut environment = NodeReloadTaskEnvironmentV1::new(world, task, budget)?;
    let episode =
        run_task_actor_episode_v1(&mut environment, &mut ExactRunDecisionRootCaptureActorV1)
            .map_err(|error| format!("failed to capture exact run decision root: {error}"))?;
    let point = environment
        .capture_research_run_branch_point_v1()?
        .map_anchor(|anchor| anchor.durable_checkpoint_v1());
    let mut archive = TaskRunSolutionArchiveV1::new(episode.final_view.task.clone())?;
    archive.record_episode(seed, "exact-run-decision-root-v1", budget, &episode)?;
    let checkpoints =
        TaskRunBranchCheckpointArtifactV1::from_branch_points(&archive, seed, [point])?;
    Ok(ExactRunDecisionRootArtifactsV1 {
        archive,
        checkpoints,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::events::neow::NeowEligibilityV1;
    use crate::eval::run_control::{natural_run_root_artifacts_v1, ExactRunPlaySessionV1};

    #[test]
    fn exact_run_decision_reroot_preserves_state_and_restarts_public_history() {
        for player_class in ["Ironclad", "Silent", "Defect", "Watcher"] {
            let budget = TaskBudgetV1 {
                max_world_transitions: 16,
                max_reloads: 0,
            };
            let plan_options = CombatResearchPlanOptionsV1::default();
            let natural = natural_run_root_artifacts_v1(
                crate::eval::run_control::RunControlConfig {
                    seed: 17,
                    player_class,
                    ascension_level: 20,
                    neow_eligibility: NeowEligibilityV1::Rich,
                    final_act: true,
                    ..Default::default()
                },
                budget,
                plan_options.clone(),
                ExactBranchingTaskEvidenceRequestV1::default(),
            )
            .expect("natural source root");
            let natural_root_id = natural
                .checkpoints
                .identities()
                .next()
                .expect("natural checkpoint")
                .archive_node_id
                .clone();
            let source = ExactRunPlaySessionV1::open(
                natural.archive,
                natural.checkpoints,
                &natural_root_id,
                budget,
                plan_options.clone(),
                ExactBranchingTaskEvidenceRequestV1::default(),
            )
            .expect("source exact session");
            let source_decision = source
                .current_public_decision()
                .expect("source decision")
                .clone();
            let source_checkpoint = source
                .current_run_decision_checkpoint_v1()
                .expect("source exact checkpoint");
            let source_fingerprint = run_control_session_fingerprint_v3(
                &source_checkpoint
                    .clone()
                    .into_session()
                    .expect("source session"),
            );

            let reroot = exact_run_decision_root_artifacts_v1(
                source_checkpoint,
                ExactRunDecisionSourceV1 {
                    session_id: "source-session".to_string(),
                    head_revision: 7,
                    archive_node_id: natural_root_id,
                    public_snapshot_id: source_decision.information.snapshot_id.clone(),
                },
                TaskObjectiveV1::HeartVictory,
                budget,
                plan_options.clone(),
                ExactBranchingTaskEvidenceRequestV1::default(),
            )
            .expect("exact reroot artifacts");
            assert!(matches!(
                reroot.archive.task.initial_condition,
                TaskInitialConditionV1::ExactRunDecision { .. }
            ));
            assert_eq!(reroot.archive.nodes.len(), 1);
            assert_eq!(reroot.archive.nodes[0].depth, 0);
            let reroot_root_id = reroot
                .checkpoints
                .identities()
                .next()
                .expect("reroot checkpoint")
                .archive_node_id
                .clone();
            let reopened = ExactRunPlaySessionV1::open(
                reroot.archive,
                reroot.checkpoints,
                &reroot_root_id,
                budget,
                plan_options,
                ExactBranchingTaskEvidenceRequestV1::default(),
            )
            .expect("reopened exact reroot");
            assert_eq!(reopened.current_public_decision(), Some(&source_decision));
            let reopened_checkpoint = reopened
                .current_run_decision_checkpoint_v1()
                .expect("reopened exact checkpoint");
            assert_eq!(
                run_control_session_fingerprint_v3(
                    &reopened_checkpoint
                        .into_session()
                        .expect("reopened session")
                ),
                source_fingerprint
            );
        }
    }
}
