use super::*;
use crate::agent::task::{
    run_task_actor_episode_v1, TaskActorBoundaryV1, TaskActorIdentityV1, TaskActorIntentV1,
    TaskActorV1, TaskEpisodeOutcomeV1, TaskPublicChoiceV1,
};
use crate::eval::run_control::{
    ironclad_a0_exact_branching_environment_v1, ironclad_a0_exact_branching_task_spec_v1,
};

struct UnsupportedActor;

impl TaskActorV1 for UnsupportedActor {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only("checkpoint-fixture", 1)
    }

    fn next_intent(&mut self, _view: &TaskActorViewV1) -> TaskActorIntentV1 {
        TaskActorIntentV1::UnsupportedDecisionSurface {
            reason: "root-only checkpoint fixture".to_string(),
        }
    }
}

struct OneScopeActor {
    phase: u8,
    reload_once: bool,
}

impl TaskActorV1 for OneScopeActor {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only("checkpoint-scope-fixture", 1)
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        if !view.committed_history.completed_scopes.is_empty() {
            return TaskActorIntentV1::YieldControl;
        }
        match (&view.boundary, self.phase) {
            (TaskActorBoundaryV1::Decision { .. }, 0 | 2) => {
                self.phase += 1;
                TaskActorIntentV1::Act {
                    choice: TaskPublicChoiceV1::one(0),
                }
            }
            (TaskActorBoundaryV1::Decision { .. }, 1) if self.reload_once => {
                self.phase += 1;
                TaskActorIntentV1::Reload
            }
            (TaskActorBoundaryV1::Decision { .. }, 1 | 3) => {
                self.phase += 1;
                TaskActorIntentV1::CommitScope
            }
            (
                TaskActorBoundaryV1::CommitReady | TaskActorBoundaryV1::ExploredScopeExit { .. },
                _,
            ) => TaskActorIntentV1::CommitScope,
            (TaskActorBoundaryV1::Decision { .. }, _) => TaskActorIntentV1::YieldControl,
            (TaskActorBoundaryV1::Terminal { .. }, _) => unreachable!("runner handles terminal"),
        }
    }
}

fn fixture(
    seed: u64,
) -> (
    TaskRunSolutionArchiveV1,
    TaskRunBranchCheckpointArtifactV1,
    TaskBudgetV1,
) {
    let budget = TaskBudgetV1 {
        max_world_transitions: 2,
        max_reloads: 0,
    };
    let mut environment =
        ironclad_a0_exact_branching_environment_v1(seed, budget).expect("exact environment");
    let point = environment
        .capture_research_run_branch_point_v1()
        .expect("fresh exact root")
        .map_anchor(|anchor| anchor.durable_checkpoint_v1());
    let episode = run_task_actor_episode_v1(&mut environment, &mut UnsupportedActor)
        .expect("root-only episode");
    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Failure { .. }
    ));
    let mut archive = TaskRunSolutionArchiveV1::new(ironclad_a0_exact_branching_task_spec_v1())
        .expect("run archive");
    archive
        .record_episode(seed, "checkpoint-fixture", budget, &episode)
        .expect("record episode");
    let artifact = TaskRunBranchCheckpointArtifactV1::from_branch_points(&archive, seed, [point])
        .expect("checkpoint artifact");
    (archive, artifact, budget)
}

fn scope_fixture(
    seed: u64,
    reload_once: bool,
) -> (TaskRunSolutionArchiveV1, TaskRunBranchCheckpointArtifactV1) {
    let budget = TaskBudgetV1 {
        max_world_transitions: 32,
        max_reloads: u32::from(reload_once),
    };
    let mut environment =
        ironclad_a0_exact_branching_environment_v1(seed, budget).expect("exact environment");
    let episode = run_task_actor_episode_v1(
        &mut environment,
        &mut OneScopeActor {
            phase: 0,
            reload_once,
        },
    )
    .expect("one committed scope episode");
    assert!(
        matches!(episode.outcome, TaskEpisodeOutcomeV1::Paused { .. }),
        "unexpected fixture outcome {:?} at {:?}",
        episode.outcome,
        episode.final_view.boundary
    );
    assert_eq!(episode.final_view.legal_knowledge.completed_scopes.len(), 1);
    assert_eq!(
        episode.final_view.legal_knowledge.completed_scopes[0]
            .attempts
            .len(),
        usize::from(reload_once) + 1
    );
    let point = environment
        .capture_research_run_branch_point_v1()
        .expect("fresh successor root")
        .map_anchor(|anchor| anchor.durable_checkpoint_v1());
    let mut archive = TaskRunSolutionArchiveV1::new(ironclad_a0_exact_branching_task_spec_v1())
        .expect("run archive");
    archive
        .record_episode(seed, "checkpoint-scope-fixture", budget, &episode)
        .expect("record episode");
    let artifact = TaskRunBranchCheckpointArtifactV1::from_branch_points(&archive, seed, [point])
        .expect("checkpoint artifact");
    (archive, artifact)
}

#[test]
fn artifact_round_trip_binds_archive_node_and_restores_exact_root() {
    let (archive, artifact, budget) = fixture(29);
    let payload = artifact.encode(4 * 1024 * 1024).expect("encode");
    let limit_error = artifact
        .encode(payload.len() - 1)
        .expect_err("bounded writer rejects oversized payload");
    assert!(limit_error.contains("caller-provided byte limit"));
    let version_start = TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC.len();
    assert_eq!(
        &payload[version_start..version_start + std::mem::size_of::<u32>()],
        &TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_FORMAT_VERSION.to_be_bytes()
    );
    let decoded =
        TaskRunBranchCheckpointArtifactV1::decode_for_archive(&payload, &archive, 4 * 1024 * 1024)
            .expect("decode");
    let (workspace_decoded, profile) =
        TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive_profiled_v1(
            &payload,
            &archive,
            4 * 1024 * 1024,
        )
        .expect("profiled workspace decode");
    assert_eq!(workspace_decoded, decoded);
    assert_eq!(profile.validation_archive_joins, Duration::ZERO);
    assert_eq!(decoded.checkpoint_count(), 1);
    let node_id = decoded
        .identities()
        .next()
        .expect("checkpoint identity")
        .archive_node_id
        .clone();
    let mut restored =
        ironclad_a0_exact_branching_environment_v1(29, budget).expect("fresh restore target");
    let view = decoded
        .restore_into_environment(&archive, &node_id, &mut restored, budget)
        .expect("restore exact branch");
    assert!(view.committed_history.steps.is_empty());
    assert_eq!(
        view.node_memory.expect("fresh memory").attempts[0].root,
        archive.nodes[0]
            .initial_public_decision
            .clone()
            .expect("archive root")
    );
    let repeated = restored
        .capture_research_run_branch_point_v1()
        .expect("restored root remains fresh")
        .map_anchor(|anchor| anchor.durable_checkpoint_v1());
    let merged = decoded
        .clone()
        .merged_with_branch_points(&archive, 29, [repeated])
        .expect("same node and exact identity deduplicate");
    assert_eq!(merged.checkpoint_count(), 1);
    assert!(decoded
        .clone()
        .merged_with_branch_points(&archive, 30, [])
        .is_err());

    assert!(TaskRunBranchCheckpointArtifactV1::decode_for_archive(&payload, &archive, 16).is_err());
    let mut trailing = payload.clone();
    trailing.push(0);
    assert!(TaskRunBranchCheckpointArtifactV1::decode_for_archive(
        &trailing,
        &archive,
        4 * 1024 * 1024,
    )
    .is_err());
    let mut unsupported_version = payload.clone();
    unsupported_version[version_start..version_start + std::mem::size_of::<u32>()]
        .copy_from_slice(&99u32.to_be_bytes());
    assert!(TaskRunBranchCheckpointArtifactV1::decode_for_archive(
        &unsupported_version,
        &archive,
        4 * 1024 * 1024,
    )
    .is_err());
    let mut invalid_magic = payload;
    invalid_magic[0] ^= 0xff;
    assert!(TaskRunBranchCheckpointArtifactV1::decode_for_archive(
        &invalid_magic,
        &archive,
        4 * 1024 * 1024,
    )
    .is_err());
}

#[test]
fn exact_state_reroot_reads_only_selected_v3_checkpoint_and_checks_its_identity() {
    let (archive, artifact, _) = fixture(31);
    let node_id = artifact
        .identities()
        .next()
        .unwrap()
        .archive_node_id
        .clone();
    let payload = artifact.encode(4 * 1024 * 1024).unwrap();
    let (checkpoint, source_decision) =
        TaskRunBranchCheckpointArtifactV1::selected_exact_state_for_reroot_v1(
            &payload,
            &archive,
            &node_id,
            4 * 1024 * 1024,
        )
        .unwrap();
    assert_eq!(checkpoint.into_session().unwrap().run_state.seed, 31);
    assert_eq!(
        source_decision.information.snapshot_id,
        artifact
            .identities()
            .next()
            .unwrap()
            .root_public_snapshot_id
    );
    assert!(
        TaskRunBranchCheckpointArtifactV1::selected_exact_state_for_reroot_v1(
            &payload,
            &archive,
            "missing-node",
            4 * 1024 * 1024,
        )
        .is_err()
    );

    let mut disk =
        TaskRunBranchCheckpointDiskArtifactV3::from_runtime_artifact_v1(&artifact).unwrap();
    disk.checkpoints[0].identity.exact_state_id = "wrong-state".into();
    let mut corrupted = BoundedArtifactWriter::new(
        4 * 1024 * 1024,
        TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_FORMAT_VERSION,
    )
    .unwrap();
    rmp_serde::encode::write_named(&mut corrupted, &disk).unwrap();
    assert!(
        TaskRunBranchCheckpointArtifactV1::selected_exact_state_for_reroot_v1(
            &corrupted.finish(),
            &archive,
            &node_id,
            4 * 1024 * 1024,
        )
        .unwrap_err()
        .contains("identity is invalid")
    );
}

#[test]
fn legacy_v1_decodes_and_reencodes_as_archive_backed_v3() {
    let (archive, artifact, _) = fixture(37);
    let legacy = encode_legacy_artifact_v1(&artifact, 4 * 1024 * 1024).expect("legacy encode");
    let decoded =
        TaskRunBranchCheckpointArtifactV1::decode_for_archive(&legacy, &archive, 4 * 1024 * 1024)
            .expect("legacy decode");
    let migrated = decoded.encode(4 * 1024 * 1024).expect("V3 encode");
    let version_start = TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC.len();
    assert_eq!(
        &legacy[version_start..version_start + std::mem::size_of::<u32>()],
        &TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_LEGACY_FORMAT_VERSION.to_be_bytes()
    );
    assert_eq!(
        &migrated[version_start..version_start + std::mem::size_of::<u32>()],
        &TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_FORMAT_VERSION.to_be_bytes()
    );
    TaskRunBranchCheckpointArtifactV1::decode_for_archive(&migrated, &archive, 4 * 1024 * 1024)
        .expect("migrated decode");
}

#[test]
fn shared_history_v2_decodes_and_reencodes_as_archive_backed_v3() {
    let (archive, artifact, _) = fixture(41);
    let v2 = encode_artifact_v2(&artifact, 4 * 1024 * 1024).expect("V2 encode");
    let decoded =
        TaskRunBranchCheckpointArtifactV1::decode_for_archive(&v2, &archive, 4 * 1024 * 1024)
            .expect("V2 decode");
    let migrated = decoded.encode(4 * 1024 * 1024).expect("V3 encode");
    let version_start = TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC.len();
    assert_eq!(
        &v2[version_start..version_start + std::mem::size_of::<u32>()],
        &TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_SHARED_HISTORY_FORMAT_VERSION.to_be_bytes()
    );
    assert_eq!(
        &migrated[version_start..version_start + std::mem::size_of::<u32>()],
        &TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_FORMAT_VERSION.to_be_bytes()
    );
    TaskRunBranchCheckpointArtifactV1::decode_for_archive(&migrated, &archive, 4 * 1024 * 1024)
        .expect("migrated decode");
}

#[test]
fn v3_reconstructs_a_single_selected_attempt_from_the_validated_archive() {
    let (archive, artifact) = scope_fixture(43, false);
    let v2 = encode_artifact_v2(&artifact, 8 * 1024 * 1024).expect("V2 encode");
    let v3 = artifact.encode(8 * 1024 * 1024).expect("V3 encode");
    assert!(v3.len() < v2.len());
    let header_bytes = artifact_header_bytes();
    let disk: TaskRunBranchCheckpointDiskArtifactV3 =
        rmp_serde::from_slice(&v3[header_bytes..]).expect("V3 disk payload");
    assert!(disk.completed_scopes.iter().any(|record| matches!(
        record.knowledge,
        TaskRunBranchScopeKnowledgeV3::ArchiveSelected { .. }
    )));
    let decoded =
        TaskRunBranchCheckpointArtifactV1::decode_for_archive(&v3, &archive, 8 * 1024 * 1024)
            .expect("V3 decode");
    assert_eq!(decoded, artifact);
}

#[test]
fn v3_keeps_multi_attempt_scope_knowledge_expanded() {
    let (archive, artifact) = scope_fixture(47, true);
    let v3 = artifact.encode(8 * 1024 * 1024).expect("V3 encode");
    let disk: TaskRunBranchCheckpointDiskArtifactV3 =
        rmp_serde::from_slice(&v3[artifact_header_bytes()..]).expect("V3 disk payload");
    assert!(disk.completed_scopes.iter().all(|record| matches!(
        record.knowledge,
        TaskRunBranchScopeKnowledgeV3::Expanded { .. }
    )));
    let decoded =
        TaskRunBranchCheckpointArtifactV1::decode_for_archive(&v3, &archive, 8 * 1024 * 1024)
            .expect("V3 decode");
    assert_eq!(decoded, artifact);
}

#[test]
fn artifact_rejects_tampered_exact_identity_and_wrong_archive() {
    let (archive, mut artifact, _) = fixture(29);
    artifact.checkpoints[0]
        .identity
        .exact_state_id
        .push_str("-tampered");
    assert!(artifact.encode(4 * 1024 * 1024).is_err());

    let (_, valid, _) = fixture(31);
    let payload = valid.encode(4 * 1024 * 1024).expect("encode valid");
    assert!(TaskRunBranchCheckpointArtifactV1::decode_for_archive(
        &payload,
        &archive,
        4 * 1024 * 1024,
    )
    .is_err());
}

#[test]
fn artifact_reports_checkpoint_index_and_archive_node_for_invalid_exact_state() {
    let (_archive, artifact, _) = fixture(41);
    let node_id = artifact.checkpoints[0].identity.archive_node_id.clone();
    let mut value = serde_json::to_value(&artifact).expect("serialize fixture artifact");
    value["checkpoints"][0]["point"]["anchor"]["public_root"]["candidates"] =
        serde_json::Value::Array(Vec::new());
    let invalid: TaskRunBranchCheckpointArtifactV1 =
        serde_json::from_value(value).expect("deserialize tampered fixture artifact");

    let error = invalid
        .validate(None)
        .expect_err("changed exact public root must be rejected");
    assert_eq!(
        error,
        format!(
            "run branch checkpoint 0 ({node_id}) exact state is invalid: durable exact scope checkpoint changed its public root at candidates.length"
        )
    );
}
