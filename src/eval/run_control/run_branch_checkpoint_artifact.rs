//! Opaque durable exact roots for complete-run solution exploration.
//!
//! The public run archive owns reachability and victory proof. This artifact
//! only binds private exact simulator restore state to already observed archive
//! nodes. Actor code may carry neither the payload nor a decoded checkpoint.

use std::collections::BTreeMap;
use std::io::{self, Cursor, Write};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::agent::task::{
    ArchivedNodeKnowledgeV1, CommittedPublicHistoryV1, CommittedPublicStepV1,
    CommittedScopeBoundaryV1, LegalKnowledgeLedgerV1, NodeAttemptStatusV1, NodeAttemptV1,
    NodeReloadTaskEnvironmentV1, ReloadPermissionV1, TaskActorViewV1, TaskAttemptOrdinalV1,
    TaskBudgetV1, TaskCommittedScopeExitV1, TaskPublicDecisionV1, TaskResearchRunBranchPointV1,
    TaskRunArchiveNodeV1, TaskRunSolutionArchiveV1, TaskScopeOrdinalV1, TaskSpecV1,
};
use crate::ai::planner_core::stable_planner_id;

use sts_oracle_learning_env::eval::run_control::{
    CombatResearchPlanOptionsV1, ExactBranchingTaskEvidenceRequestV1, ExactBranchingTaskWorldV1,
    RunControlSessionCheckpointV1, SimulatorExactScopeCheckpointIdentityV1,
    SimulatorExactScopeCheckpointV1,
};

pub const TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC: &[u8] = b"STS-TASK-RUN-BRANCH-CHECKPOINTS\0";
const TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_LEGACY_FORMAT_VERSION: u32 = 1;
const TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_SHARED_HISTORY_FORMAT_VERSION: u32 = 2;
pub const TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_FORMAT_VERSION: u32 = 3;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunBranchCheckpointIdentityV1 {
    pub archive_node_id: String,
    pub exact_state_id: String,
    pub lineage_id: String,
    pub root_public_snapshot_id: String,
}

/// Public projection of a restorable private branch point. It intentionally
/// omits exact state, lineage, checkpoint bytes, and task-kernel history.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunBranchCheckpointPublicRootV1 {
    pub archive_node_id: String,
    pub root: TaskPublicDecisionV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskRunBranchCheckpointEntryV1 {
    identity: TaskRunBranchCheckpointIdentityV1,
    point: TaskResearchRunBranchPointV1<SimulatorExactScopeCheckpointV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunBranchCheckpointArtifactV1 {
    task: TaskSpecV1,
    seed: u64,
    checkpoints: Vec<TaskRunBranchCheckpointEntryV1>,
}

/// Process-local decode timings for one checkpoint artifact. This diagnostic
/// is not part of the artifact schema or any actor-visible surface.
#[derive(Clone, Copy, Debug, Default)]
pub struct TaskRunBranchCheckpointDecodeProfileV1 {
    pub header: Duration,
    pub deserialize: Duration,
    pub runtime_materialization: Duration,
    pub validation: Duration,
    pub validation_preamble: Duration,
    pub validation_checkpoint_entries: Duration,
    pub validation_archive_joins: Duration,
    pub canonicality: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
struct TaskRunBranchCheckpointValidationProfileV1 {
    preamble: Duration,
    checkpoint_entries: Duration,
    archive_joins: Duration,
}

#[derive(Clone, Copy, Debug)]
enum TaskRunBranchArchiveJoinValidationV1 {
    PayloadHistory,
    ArchiveMaterializedHistory,
}

/// On-disk V2 stores each completed task scope once. Checkpoints reference the
/// shared records and recover public committed steps from the archive trie,
/// avoiding a full prefix copy at every fresh scope root.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskRunBranchCompletedScopeRecordV2 {
    record_id: String,
    committed_scope: CommittedScopeBoundaryV1,
    knowledge: ArchivedNodeKnowledgeV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskRunBranchCheckpointEntryV2 {
    identity: TaskRunBranchCheckpointIdentityV1,
    checkpoint: SimulatorExactScopeCheckpointV1,
    scope_ordinal: TaskScopeOrdinalV1,
    reload_permission: ReloadPermissionV1,
    next_scope_ordinal: u64,
    completed_scope_record_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskRunBranchCheckpointDiskArtifactV2 {
    task: TaskSpecV1,
    seed: u64,
    completed_scopes: Vec<TaskRunBranchCompletedScopeRecordV2>,
    checkpoints: Vec<TaskRunBranchCheckpointEntryV2>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum TaskRunBranchScopeKnowledgeV3 {
    ArchiveSelected { attempt: TaskAttemptOrdinalV1 },
    Expanded { knowledge: ArchivedNodeKnowledgeV1 },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskRunBranchCompletedScopeRecordV3 {
    record_id: String,
    committed_scope: CommittedScopeBoundaryV1,
    knowledge: TaskRunBranchScopeKnowledgeV3,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskRunBranchCheckpointDiskArtifactV3 {
    task: TaskSpecV1,
    seed: u64,
    completed_scopes: Vec<TaskRunBranchCompletedScopeRecordV3>,
    checkpoints: Vec<TaskRunBranchCheckpointEntryV2>,
}

impl TaskRunBranchCheckpointArtifactV1 {
    /// Constructs a task-matching exact environment from a retained root.
    /// Both natural tasks and exact-decision tasks retain their original task
    /// and character; restore does not create an unrelated placeholder game.
    #[doc(hidden)]
    pub fn exact_run_environment_for_restore_v1(
        &self,
        archive_node_id: &str,
        continuation_budget: TaskBudgetV1,
        plan_options: CombatResearchPlanOptionsV1,
        evidence_request: ExactBranchingTaskEvidenceRequestV1,
    ) -> Result<NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>, String> {
        let entry = self
            .checkpoints
            .iter()
            .find(|entry| entry.identity.archive_node_id == archive_node_id)
            .ok_or_else(|| {
                format!("run branch checkpoint has no archive node {archive_node_id}")
            })?;
        let world = ExactBranchingTaskWorldV1::from_durable_scope_checkpoint_v1(
            entry.point.private_anchor().clone(),
        )?
        .with_combat_research_plans(plan_options)?
        .with_execution_evidence_request(evidence_request)?;
        NodeReloadTaskEnvironmentV1::new(world, self.task.clone(), continuation_budget)
    }

    pub fn from_branch_points(
        archive: &TaskRunSolutionArchiveV1,
        seed: u64,
        points: impl IntoIterator<Item = TaskResearchRunBranchPointV1<SimulatorExactScopeCheckpointV1>>,
    ) -> Result<Self, String> {
        let mut checkpoints = points
            .into_iter()
            .map(|point| checkpoint_entry_v1(archive, seed, point))
            .collect::<Result<Vec<_>, String>>()?;
        checkpoints.sort_by(|left, right| {
            left.identity
                .archive_node_id
                .cmp(&right.identity.archive_node_id)
        });
        let artifact = Self {
            task: archive.task.clone(),
            seed,
            checkpoints,
        };
        artifact.validate(Some(archive))?;
        Ok(artifact)
    }

    /// Adds newly observed fresh roots after a resumed continuation. Existing
    /// roots remain authoritative. Re-observing the same archive node with the
    /// same exact identity is idempotent; binding that node to another exact
    /// state or lineage is rejected instead of silently choosing one.
    pub fn merged_with_branch_points(
        mut self,
        archive: &TaskRunSolutionArchiveV1,
        seed: u64,
        points: impl IntoIterator<Item = TaskResearchRunBranchPointV1<SimulatorExactScopeCheckpointV1>>,
    ) -> Result<Self, String> {
        self.validate(Some(archive))?;
        if seed != self.seed {
            return Err(format!(
                "run branch checkpoint merge seed {seed} disagrees with artifact seed {}",
                self.seed
            ));
        }
        self.checkpoints.extend(
            points
                .into_iter()
                .map(|point| checkpoint_entry_v1(archive, seed, point))
                .collect::<Result<Vec<_>, String>>()?,
        );
        self.checkpoints.sort_by(|left, right| {
            left.identity
                .archive_node_id
                .cmp(&right.identity.archive_node_id)
        });

        let mut merged: Vec<TaskRunBranchCheckpointEntryV1> =
            Vec::with_capacity(self.checkpoints.len());
        for entry in self.checkpoints {
            let Some(existing) = merged.last() else {
                merged.push(entry);
                continue;
            };
            if existing.identity.archive_node_id != entry.identity.archive_node_id {
                merged.push(entry);
                continue;
            }
            if existing.identity != entry.identity {
                return Err(format!(
                    "run branch checkpoint archive node {} maps to multiple exact identities",
                    entry.identity.archive_node_id
                ));
            }
            // The existing point remains canonical. Fresh node memory is not
            // allowed to turn a repeated exact identity into a second root.
        }
        self.checkpoints = merged;
        self.validate(Some(archive))?;
        Ok(self)
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    /// Drops checkpoints whose archive nodes were deliberately removed by a
    /// conservative archive projection. Private identities are never changed;
    /// every retained entry is revalidated against the projected archive.
    #[doc(hidden)]
    pub fn retained_for_archive_v1(
        &self,
        archive: &TaskRunSolutionArchiveV1,
    ) -> Result<Self, String> {
        self.validate(None)?;
        if self.task != archive.task {
            return Err("run branch checkpoint retention task mismatch".to_string());
        }
        let retained = Self {
            task: self.task.clone(),
            seed: self.seed,
            checkpoints: self
                .checkpoints
                .iter()
                .filter(|entry| archive.node(&entry.identity.archive_node_id).is_some())
                .cloned()
                .collect(),
        };
        if retained.checkpoints.is_empty() {
            return Err("projected archive retained no exact checkpoint".to_string());
        }
        retained.validate(Some(archive))?;
        Ok(retained)
    }

    pub fn identities(&self) -> impl Iterator<Item = &TaskRunBranchCheckpointIdentityV1> {
        self.checkpoints.iter().map(|entry| &entry.identity)
    }

    pub fn public_roots_for_archive(
        &self,
        archive: &TaskRunSolutionArchiveV1,
    ) -> Result<Vec<TaskRunBranchCheckpointPublicRootV1>, String> {
        self.validate(Some(archive))?;
        Ok(self
            .checkpoints
            .iter()
            .map(|entry| TaskRunBranchCheckpointPublicRootV1 {
                archive_node_id: entry.identity.archive_node_id.clone(),
                root: entry.point.root().clone(),
            })
            .collect())
    }

    /// Returns one public fresh-scope root without cloning every retained
    /// checkpoint projection. This is the targeted lookup used by interactive
    /// run research; callers that need a census should use
    /// [`Self::public_roots_for_archive`].
    pub fn public_root_for_archive_node<'a>(
        &'a self,
        archive: &TaskRunSolutionArchiveV1,
        archive_node_id: &str,
    ) -> Result<Option<&'a TaskPublicDecisionV1>, String> {
        self.validate(Some(archive))?;
        Ok(self.public_root_for_archive_node_unchecked(archive_node_id))
    }

    #[doc(hidden)]
    pub fn validate_for_archive(&self, archive: &TaskRunSolutionArchiveV1) -> Result<(), String> {
        self.validate(Some(archive))
    }

    /// Targeted companion to [`Self::validate_for_archive`]. Internal owners
    /// use it only after validating the complete artifact/archive pair once.
    #[doc(hidden)]
    pub fn public_root_for_archive_node_unchecked(
        &self,
        archive_node_id: &str,
    ) -> Option<&TaskPublicDecisionV1> {
        self.checkpoints
            .iter()
            .find(|entry| entry.identity.archive_node_id == archive_node_id)
            .map(|entry| entry.point.root())
    }

    pub fn encode(&self, max_bytes: usize) -> Result<Vec<u8>, String> {
        self.validate(None)?;
        encode_artifact_v3(self, max_bytes)
    }

    pub fn decode_for_archive(
        payload: &[u8],
        archive: &TaskRunSolutionArchiveV1,
        max_bytes: usize,
    ) -> Result<Self, String> {
        Self::decode_for_archive_with_canonicality(payload, archive, max_bytes, true)
    }

    /// Decodes a checkpoint payload already admitted into an immutable
    /// run-play state pack. Structural and archive joins are still validated;
    /// only the expensive byte-for-byte canonical re-encoding check is
    /// omitted because it was performed at import/publication time.
    #[doc(hidden)]
    pub fn decode_workspace_for_archive(
        payload: &[u8],
        archive: &TaskRunSolutionArchiveV1,
        max_bytes: usize,
    ) -> Result<Self, String> {
        Self::decode_for_archive_with_canonicality(payload, archive, max_bytes, false)
    }

    /// Read one exact simulator checkpoint for a *new* reroot when a changed
    /// public observation prevents normal archive replay. This deliberately
    /// does not bless the old archive or all of its checkpoints: it requires
    /// an exact checkpoint at the selected node and verifies its stored state
    /// and lineage identities before returning only that simulator state.
    #[doc(hidden)]
    pub fn selected_exact_state_for_reroot_v1(
        payload: &[u8],
        archive: &TaskRunSolutionArchiveV1,
        archive_node_id: &str,
        max_bytes: usize,
    ) -> Result<(RunControlSessionCheckpointV1, TaskPublicDecisionV1), String> {
        if payload.len() > max_bytes {
            return Err(
                "run branch checkpoint artifact exceeds its caller-provided byte limit".into(),
            );
        }
        let header_bytes = artifact_header_bytes();
        if payload.len() < header_bytes
            || &payload[..TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC.len()]
                != TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC
        {
            return Err("run branch checkpoint artifact header is invalid".into());
        }
        let version_start = TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC.len();
        let version = u32::from_be_bytes(
            payload[version_start..header_bytes]
                .try_into()
                .map_err(|_| "run branch checkpoint artifact version is truncated")?,
        );
        if version != TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_FORMAT_VERSION {
            return Err(format!(
                "exact-state reroot requires V3 checkpoint format; stored format is V{version}"
            ));
        }
        archive.validate()?;
        let mut decoder = rmp_serde::Deserializer::new(Cursor::new(&payload[header_bytes..]));
        let disk = TaskRunBranchCheckpointDiskArtifactV3::deserialize(&mut decoder)
            .map_err(|error| format!("cannot decode run branch checkpoint artifact V3: {error}"))?;
        require_decoder_consumed_v1(decoder.position(), payload.len() - header_bytes)?;
        if disk.task != archive.task {
            return Err("run branch checkpoint and archive tasks differ".into());
        }
        if archive.node(archive_node_id).is_none() {
            return Err(format!(
                "run archive does not contain selected node {archive_node_id}"
            ));
        }
        let mut matching = disk
            .checkpoints
            .iter()
            .filter(|entry| entry.identity.archive_node_id == archive_node_id);
        let entry = matching.next().ok_or_else(|| {
            format!("selected node {archive_node_id} has no exact checkpoint for reroot")
        })?;
        if matching.next().is_some() {
            return Err(format!(
                "selected node {archive_node_id} has duplicate exact checkpoints"
            ));
        }
        let (checkpoint, exact_state_id, lineage_id) =
            entry.checkpoint.exact_state_for_reroot_v1()?;
        if exact_state_id != entry.identity.exact_state_id
            || lineage_id != entry.identity.lineage_id
            || entry.checkpoint.public_root_v1().information.snapshot_id
                != entry.identity.root_public_snapshot_id
            || checkpoint.clone().into_session()?.run_state.seed != disk.seed
        {
            return Err(format!(
                "selected node {archive_node_id} exact checkpoint identity is invalid"
            ));
        }
        Ok((checkpoint, entry.checkpoint.public_root_v1().clone()))
    }

    #[doc(hidden)]
    pub fn decode_workspace_for_archive_profiled_v1(
        payload: &[u8],
        archive: &TaskRunSolutionArchiveV1,
        max_bytes: usize,
    ) -> Result<(Self, TaskRunBranchCheckpointDecodeProfileV1), String> {
        Self::decode_for_archive_with_canonicality_profiled_v1(payload, archive, max_bytes, false)
    }

    fn decode_for_archive_with_canonicality(
        payload: &[u8],
        archive: &TaskRunSolutionArchiveV1,
        max_bytes: usize,
        require_canonical_encoding: bool,
    ) -> Result<Self, String> {
        Self::decode_for_archive_with_canonicality_profiled_v1(
            payload,
            archive,
            max_bytes,
            require_canonical_encoding,
        )
        .map(|(artifact, _profile)| artifact)
    }

    fn decode_for_archive_with_canonicality_profiled_v1(
        payload: &[u8],
        archive: &TaskRunSolutionArchiveV1,
        max_bytes: usize,
        require_canonical_encoding: bool,
    ) -> Result<(Self, TaskRunBranchCheckpointDecodeProfileV1), String> {
        let mut profile = TaskRunBranchCheckpointDecodeProfileV1::default();
        let phase = Instant::now();
        if payload.len() > max_bytes {
            return Err(
                "run branch checkpoint artifact exceeds its caller-provided byte limit".to_string(),
            );
        }
        let header_bytes = artifact_header_bytes();
        if payload.len() < header_bytes {
            return Err("run branch checkpoint artifact ended before its header".to_string());
        }
        if &payload[..TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC.len()]
            != TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC
        {
            return Err("run branch checkpoint artifact magic is invalid".to_string());
        }
        let version_start = TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC.len();
        let encoded_version = u32::from_be_bytes(
            payload[version_start..header_bytes]
                .try_into()
                .map_err(|_| "run branch checkpoint artifact version is truncated")?,
        );
        profile.header = phase.elapsed();
        match encoded_version {
            TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_LEGACY_FORMAT_VERSION => {
                let phase = Instant::now();
                let mut decoder =
                    rmp_serde::Deserializer::new(Cursor::new(&payload[header_bytes..]));
                let artifact = Self::deserialize(&mut decoder).map_err(|error| {
                    format!("cannot decode legacy run branch checkpoint artifact: {error}")
                })?;
                require_decoder_consumed_v1(decoder.position(), payload.len() - header_bytes)?;
                profile.deserialize = phase.elapsed();
                let phase = Instant::now();
                let validation = artifact.validate_profiled_v1(
                    Some(archive),
                    TaskRunBranchArchiveJoinValidationV1::PayloadHistory,
                )?;
                profile.validation = phase.elapsed();
                profile.validation_preamble = validation.preamble;
                profile.validation_checkpoint_entries = validation.checkpoint_entries;
                profile.validation_archive_joins = validation.archive_joins;
                let phase = Instant::now();
                if require_canonical_encoding
                    && encode_legacy_artifact_v1(&artifact, max_bytes)? != payload
                {
                    return Err(
                        "legacy run branch checkpoint artifact encoding is not canonical"
                            .to_string(),
                    );
                }
                profile.canonicality = phase.elapsed();
                Ok((artifact, profile))
            }
            TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_SHARED_HISTORY_FORMAT_VERSION => {
                let phase = Instant::now();
                let mut decoder =
                    rmp_serde::Deserializer::new(Cursor::new(&payload[header_bytes..]));
                let disk = TaskRunBranchCheckpointDiskArtifactV2::deserialize(&mut decoder)
                    .map_err(|error| {
                        format!("cannot decode run branch checkpoint artifact V2: {error}")
                    })?;
                require_decoder_consumed_v1(decoder.position(), payload.len() - header_bytes)?;
                profile.deserialize = phase.elapsed();
                let phase = Instant::now();
                let artifact = disk.into_runtime_artifact_v1(archive)?;
                profile.runtime_materialization = phase.elapsed();
                let phase = Instant::now();
                // V2 committed steps were just reconstructed by following the
                // claimed node's validated archive parent chain. Recomputing
                // the same stable prefix for every checkpoint proves only the
                // already-validated archive again; it adds no independent
                // binding to the private exact checkpoint.
                let validation = artifact.validate_profiled_v1(
                    Some(archive),
                    TaskRunBranchArchiveJoinValidationV1::ArchiveMaterializedHistory,
                )?;
                profile.validation = phase.elapsed();
                profile.validation_preamble = validation.preamble;
                profile.validation_checkpoint_entries = validation.checkpoint_entries;
                profile.validation_archive_joins = validation.archive_joins;
                let phase = Instant::now();
                if require_canonical_encoding
                    && encode_artifact_v2(&artifact, max_bytes)? != payload
                {
                    return Err(
                        "run branch checkpoint artifact V2 encoding is not canonical".to_string(),
                    );
                }
                profile.canonicality = phase.elapsed();
                Ok((artifact, profile))
            }
            TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_FORMAT_VERSION => {
                let phase = Instant::now();
                let mut decoder =
                    rmp_serde::Deserializer::new(Cursor::new(&payload[header_bytes..]));
                let disk = TaskRunBranchCheckpointDiskArtifactV3::deserialize(&mut decoder)
                    .map_err(|error| {
                        format!("cannot decode run branch checkpoint artifact V3: {error}")
                    })?;
                require_decoder_consumed_v1(decoder.position(), payload.len() - header_bytes)?;
                profile.deserialize = phase.elapsed();
                let phase = Instant::now();
                let artifact = disk.into_runtime_artifact_v1(archive)?;
                profile.runtime_materialization = phase.elapsed();
                let phase = Instant::now();
                let validation = artifact.validate_profiled_v1(
                    Some(archive),
                    TaskRunBranchArchiveJoinValidationV1::ArchiveMaterializedHistory,
                )?;
                profile.validation = phase.elapsed();
                profile.validation_preamble = validation.preamble;
                profile.validation_checkpoint_entries = validation.checkpoint_entries;
                profile.validation_archive_joins = validation.archive_joins;
                let phase = Instant::now();
                if require_canonical_encoding
                    && encode_artifact_v3(&artifact, max_bytes)? != payload
                {
                    return Err(
                        "run branch checkpoint artifact V3 encoding is not canonical".to_string(),
                    );
                }
                profile.canonicality = phase.elapsed();
                Ok((artifact, profile))
            }
            _ => Err("run branch checkpoint artifact format version is unsupported".to_string()),
        }
    }

    /// Restores one private checkpoint into a caller-configured exact world.
    /// The caller owns the new continuation budget and any privileged support
    /// configuration; the artifact supplies neither policy nor compute budget.
    pub fn restore_into_environment(
        &self,
        archive: &TaskRunSolutionArchiveV1,
        archive_node_id: &str,
        environment: &mut NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>,
        continuation_budget: TaskBudgetV1,
    ) -> Result<TaskActorViewV1, String> {
        self.validate(Some(archive))?;
        self.restore_into_environment_in_validated_session_v1(
            archive_node_id,
            environment,
            continuation_budget,
        )
    }

    /// Targeted restore after the owning immutable workspace has already
    /// validated the complete archive/artifact join. Exact root identity is
    /// still checked by the task environment during restoration.
    #[doc(hidden)]
    pub fn restore_into_environment_in_validated_session_v1(
        &self,
        archive_node_id: &str,
        environment: &mut NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>,
        continuation_budget: TaskBudgetV1,
    ) -> Result<TaskActorViewV1, String> {
        let entry = self
            .checkpoints
            .iter()
            .find(|entry| entry.identity.archive_node_id == archive_node_id)
            .ok_or_else(|| {
                format!("run branch checkpoint has no archive node {archive_node_id}")
            })?;
        let point = entry
            .point
            .clone()
            .map_anchor(SimulatorExactScopeCheckpointV1::into_runtime_root_v1);
        environment.restore_research_run_branch_point_v1(&point, continuation_budget)
    }

    fn validate(&self, archive: Option<&TaskRunSolutionArchiveV1>) -> Result<(), String> {
        self.validate_profiled_v1(
            archive,
            TaskRunBranchArchiveJoinValidationV1::PayloadHistory,
        )
        .map(|_| ())
    }

    fn validate_profiled_v1(
        &self,
        archive: Option<&TaskRunSolutionArchiveV1>,
        archive_join_validation: TaskRunBranchArchiveJoinValidationV1,
    ) -> Result<TaskRunBranchCheckpointValidationProfileV1, String> {
        let mut profile = TaskRunBranchCheckpointValidationProfileV1::default();
        let phase = Instant::now();
        self.task.validate()?;
        if !self.task.is_complete_run_task() {
            return Err("run branch checkpoint artifact requires a complete-run task".to_string());
        }
        if self.checkpoints.is_empty() {
            return Err(
                "run branch checkpoint artifact must contain at least one root".to_string(),
            );
        }
        if let Some(archive) = archive {
            archive.validate()?;
            if archive.task != self.task {
                return Err("run branch checkpoint artifact and archive tasks differ".to_string());
            }
        }
        profile.preamble = phase.elapsed();

        let mut previous_node_id: Option<&str> = None;
        for (index, entry) in self.checkpoints.iter().enumerate() {
            let phase = Instant::now();
            let node_id = entry.identity.archive_node_id.as_str();
            if previous_node_id.is_some_and(|previous| previous >= node_id) {
                return Err(
                    "run branch checkpoint entries are not in canonical node-id order".to_string(),
                );
            }
            previous_node_id = Some(node_id);
            if entry.point.task() != &self.task {
                return Err(format!(
                    "run branch checkpoint {index} belongs to a different task"
                ));
            }
            validate_fresh_scope_public_join(&entry.point)?;
            entry.point.root().validate()?;
            let private = validate_private_scope_checkpoint(entry.point.private_anchor()).map_err(
                |error| {
                    format!(
                        "run branch checkpoint {index} ({node_id}) exact state is invalid: {error}"
                    )
                },
            )?;
            if private.seed != self.seed
                || private.public_root != *entry.point.root()
                || private.exact_state_id != entry.identity.exact_state_id
                || private.lineage_id != entry.identity.lineage_id
                || entry.point.root().information.snapshot_id
                    != entry.identity.root_public_snapshot_id
            {
                return Err(format!(
                    "run branch checkpoint {index} identity disagrees with its exact state"
                ));
            }
            profile.checkpoint_entries += phase.elapsed();
            if let (Some(archive), TaskRunBranchArchiveJoinValidationV1::PayloadHistory) =
                (archive, archive_join_validation)
            {
                let phase = Instant::now();
                let resolved = archive.node_id_for_committed_prefix(
                    self.seed,
                    entry.point.root(),
                    entry.point.committed_history(),
                )?;
                if resolved != entry.identity.archive_node_id {
                    return Err(format!(
                        "run branch checkpoint {index} maps to archive node {resolved}, not {}",
                        entry.identity.archive_node_id
                    ));
                }
                profile.archive_joins += phase.elapsed();
            }
        }
        Ok(profile)
    }
}

impl TaskRunBranchCheckpointDiskArtifactV2 {
    fn from_runtime_artifact_v1(
        artifact: &TaskRunBranchCheckpointArtifactV1,
    ) -> Result<Self, String> {
        let mut completed_scopes = BTreeMap::<String, TaskRunBranchCompletedScopeRecordV2>::new();
        let mut checkpoints = Vec::with_capacity(artifact.checkpoints.len());
        for (index, entry) in artifact.checkpoints.iter().enumerate() {
            let history = entry.point.committed_history();
            let knowledge = entry.point.legal_knowledge();
            if history.completed_scopes.len() != knowledge.completed_scopes.len() {
                return Err(format!(
                    "run branch checkpoint {index} has misaligned completed scope ledgers"
                ));
            }
            let mut completed_scope_record_ids = Vec::with_capacity(history.completed_scopes.len());
            for (committed_scope, scope_knowledge) in history
                .completed_scopes
                .iter()
                .zip(&knowledge.completed_scopes)
            {
                validate_completed_scope_record_v2(committed_scope, scope_knowledge)?;
                let record_id = completed_scope_record_id_v2(committed_scope, scope_knowledge)?;
                let record = TaskRunBranchCompletedScopeRecordV2 {
                    record_id: record_id.clone(),
                    committed_scope: committed_scope.clone(),
                    knowledge: scope_knowledge.clone(),
                };
                if let Some(existing) = completed_scopes.insert(record_id.clone(), record.clone()) {
                    if existing != record {
                        return Err(format!(
                            "run branch completed scope record id {record_id} collided"
                        ));
                    }
                }
                completed_scope_record_ids.push(record_id);
            }
            checkpoints.push(TaskRunBranchCheckpointEntryV2 {
                identity: entry.identity.clone(),
                checkpoint: entry
                    .point
                    .private_anchor()
                    .clone()
                    .normalized_for_durable_branch_v1(),
                scope_ordinal: entry.point.scope_ordinal(),
                reload_permission: entry.point.reload_permission().clone(),
                next_scope_ordinal: entry.point.next_scope_ordinal(),
                completed_scope_record_ids,
            });
        }
        Ok(Self {
            task: artifact.task.clone(),
            seed: artifact.seed,
            completed_scopes: completed_scopes.into_values().collect(),
            checkpoints,
        })
    }

    fn into_runtime_artifact_v1(
        self,
        archive: &TaskRunSolutionArchiveV1,
    ) -> Result<TaskRunBranchCheckpointArtifactV1, String> {
        let archive_nodes = archive_node_index_v1(archive);
        self.into_runtime_artifact_with_index_v1(archive, &archive_nodes)
    }

    fn into_runtime_artifact_with_index_v1(
        self,
        archive: &TaskRunSolutionArchiveV1,
        archive_nodes: &BTreeMap<&str, &TaskRunArchiveNodeV1>,
    ) -> Result<TaskRunBranchCheckpointArtifactV1, String> {
        if self.task != archive.task {
            return Err("run branch checkpoint V2 and archive tasks differ".to_string());
        }
        let mut scope_records = BTreeMap::new();
        let mut previous_record_id: Option<&str> = None;
        for record in &self.completed_scopes {
            if previous_record_id.is_some_and(|previous| previous >= record.record_id.as_str()) {
                return Err(
                    "run branch checkpoint V2 scope records are not in canonical id order"
                        .to_string(),
                );
            }
            previous_record_id = Some(&record.record_id);
            validate_completed_scope_record_v2(&record.committed_scope, &record.knowledge)?;
            let expected =
                completed_scope_record_id_v2(&record.committed_scope, &record.knowledge)?;
            if expected != record.record_id {
                return Err(format!(
                    "run branch checkpoint V2 scope record {} has invalid identity",
                    record.record_id
                ));
            }
            scope_records.insert(record.record_id.clone(), record.clone());
        }

        let mut checkpoints = Vec::with_capacity(self.checkpoints.len());
        let mut previous_node_id: Option<String> = None;
        for entry in self.checkpoints {
            if previous_node_id
                .as_deref()
                .is_some_and(|previous| previous >= entry.identity.archive_node_id.as_str())
            {
                return Err(
                    "run branch checkpoint V2 entries are not in canonical node-id order"
                        .to_string(),
                );
            }
            previous_node_id = Some(entry.identity.archive_node_id.clone());
            let steps = committed_steps_for_archive_node_v1(
                archive_nodes,
                self.seed,
                &entry.identity.archive_node_id,
            )?;
            let mut committed_scopes = Vec::with_capacity(entry.completed_scope_record_ids.len());
            let mut knowledge_scopes = Vec::with_capacity(entry.completed_scope_record_ids.len());
            for record_id in &entry.completed_scope_record_ids {
                let record = scope_records.get(record_id).ok_or_else(|| {
                    format!(
                        "run branch checkpoint V2 entry {} references missing scope record {record_id}",
                        entry.identity.archive_node_id
                    )
                })?;
                committed_scopes.push(record.committed_scope.clone());
                knowledge_scopes.push(record.knowledge.clone());
            }
            let root = entry.checkpoint.public_root_v1().clone();
            let point = TaskResearchRunBranchPointV1::from_durable_parts_v1(
                self.task.clone(),
                CommittedPublicHistoryV1 {
                    steps,
                    completed_scopes: committed_scopes,
                },
                LegalKnowledgeLedgerV1 {
                    completed_scopes: knowledge_scopes,
                },
                entry.scope_ordinal,
                entry.checkpoint,
                entry.reload_permission,
                root,
                entry.next_scope_ordinal,
            )?;
            checkpoints.push(TaskRunBranchCheckpointEntryV1 {
                identity: entry.identity,
                point,
            });
        }
        Ok(TaskRunBranchCheckpointArtifactV1 {
            task: self.task,
            seed: self.seed,
            checkpoints,
        })
    }
}

impl TaskRunBranchCheckpointDiskArtifactV3 {
    fn from_runtime_artifact_v1(
        artifact: &TaskRunBranchCheckpointArtifactV1,
    ) -> Result<Self, String> {
        let disk_v2 = TaskRunBranchCheckpointDiskArtifactV2::from_runtime_artifact_v1(artifact)?;
        let mut archive_selected = BTreeMap::<String, bool>::new();
        for entry in &artifact.checkpoints {
            let history = entry.point.committed_history();
            for (committed_scope, knowledge) in history
                .completed_scopes
                .iter()
                .zip(&entry.point.legal_knowledge().completed_scopes)
            {
                let record_id = completed_scope_record_id_v2(committed_scope, knowledge)?;
                let reconstructible =
                    archive_selected_attempt_v3(committed_scope, knowledge, history).is_some();
                archive_selected
                    .entry(record_id)
                    .and_modify(|eligible| *eligible &= reconstructible)
                    .or_insert(reconstructible);
            }
        }
        let completed_scopes = disk_v2
            .completed_scopes
            .into_iter()
            .map(|record| {
                let compact = archive_selected
                    .get(&record.record_id)
                    .copied()
                    .unwrap_or(false);
                let knowledge = if compact {
                    let attempt = record
                        .knowledge
                        .attempts
                        .first()
                        .expect("archive-selected scope has one validated attempt")
                        .attempt;
                    TaskRunBranchScopeKnowledgeV3::ArchiveSelected { attempt }
                } else {
                    TaskRunBranchScopeKnowledgeV3::Expanded {
                        knowledge: record.knowledge,
                    }
                };
                TaskRunBranchCompletedScopeRecordV3 {
                    record_id: record.record_id,
                    committed_scope: record.committed_scope,
                    knowledge,
                }
            })
            .collect();
        Ok(Self {
            task: disk_v2.task,
            seed: disk_v2.seed,
            completed_scopes,
            checkpoints: disk_v2.checkpoints,
        })
    }

    fn into_runtime_artifact_v1(
        self,
        archive: &TaskRunSolutionArchiveV1,
    ) -> Result<TaskRunBranchCheckpointArtifactV1, String> {
        let Self {
            task,
            seed,
            completed_scopes,
            checkpoints: disk_checkpoints,
        } = self;
        if task != archive.task {
            return Err("run branch checkpoint V3 and archive tasks differ".to_string());
        }
        let archive_nodes = archive_node_index_v1(archive);
        let mut scope_records = BTreeMap::new();
        let mut previous_record_id: Option<&str> = None;
        for record in &completed_scopes {
            if previous_record_id.is_some_and(|previous| previous >= record.record_id.as_str()) {
                return Err(
                    "run branch checkpoint V3 scope records are not in canonical id order"
                        .to_string(),
                );
            }
            previous_record_id = Some(&record.record_id);
            if let TaskRunBranchScopeKnowledgeV3::Expanded { knowledge } = &record.knowledge {
                validate_scope_record_identity_v3(record, knowledge)?;
            }
            scope_records.insert(record.record_id.as_str(), record);
        }

        let mut materialized_knowledge = BTreeMap::<&str, ArchivedNodeKnowledgeV1>::new();
        let mut checkpoints = Vec::with_capacity(disk_checkpoints.len());
        let mut previous_node_id: Option<&str> = None;
        for entry in &disk_checkpoints {
            let node_id = entry.identity.archive_node_id.as_str();
            if previous_node_id.is_some_and(|previous| previous >= node_id) {
                return Err(
                    "run branch checkpoint V3 entries are not in canonical node-id order"
                        .to_string(),
                );
            }
            previous_node_id = Some(node_id);
            let steps = committed_steps_for_archive_node_v1(&archive_nodes, seed, node_id)?;
            let mut scope_boundaries = Vec::with_capacity(entry.completed_scope_record_ids.len());
            let mut knowledge_scopes = Vec::with_capacity(entry.completed_scope_record_ids.len());
            for record_id in &entry.completed_scope_record_ids {
                let record = scope_records.get(record_id.as_str()).ok_or_else(|| {
                    format!(
                        "run branch checkpoint V3 entry {} references missing scope record {record_id}",
                        entry.identity.archive_node_id
                    )
                })?;
                let knowledge = match &record.knowledge {
                    TaskRunBranchScopeKnowledgeV3::ArchiveSelected { attempt } => {
                        let reconstructed = archive_selected_knowledge_v3(
                            &record.committed_scope,
                            *attempt,
                            &steps,
                        )?;
                        if let Some(previous) = materialized_knowledge.get(record_id.as_str()) {
                            if previous != &reconstructed {
                                return Err(format!(
                                    "run branch checkpoint V3 scope record {record_id} reconstructs differently across archive paths"
                                ));
                            }
                        } else {
                            validate_scope_record_identity_v3(record, &reconstructed)?;
                            materialized_knowledge
                                .insert(record.record_id.as_str(), reconstructed.clone());
                        }
                        reconstructed
                    }
                    TaskRunBranchScopeKnowledgeV3::Expanded { knowledge } => {
                        if let Some(previous) = materialized_knowledge.get(record_id.as_str()) {
                            previous.clone()
                        } else {
                            validate_scope_record_identity_v3(record, knowledge)?;
                            materialized_knowledge
                                .insert(record.record_id.as_str(), knowledge.clone());
                            knowledge.clone()
                        }
                    }
                };
                scope_boundaries.push(record.committed_scope.clone());
                knowledge_scopes.push(knowledge);
            }
            let root = entry.checkpoint.public_root_v1().clone();
            let point = TaskResearchRunBranchPointV1::from_durable_parts_v1(
                task.clone(),
                CommittedPublicHistoryV1 {
                    steps,
                    completed_scopes: scope_boundaries,
                },
                LegalKnowledgeLedgerV1 {
                    completed_scopes: knowledge_scopes,
                },
                entry.scope_ordinal,
                entry.checkpoint.clone(),
                entry.reload_permission.clone(),
                root,
                entry.next_scope_ordinal,
            )?;
            checkpoints.push(TaskRunBranchCheckpointEntryV1 {
                identity: entry.identity.clone(),
                point,
            });
        }
        if materialized_knowledge.len() != scope_records.len() {
            return Err(
                "run branch checkpoint V3 contains an unreferenced scope record".to_string(),
            );
        }
        Ok(TaskRunBranchCheckpointArtifactV1 {
            task,
            seed,
            checkpoints,
        })
    }
}

fn validate_scope_record_identity_v3(
    record: &TaskRunBranchCompletedScopeRecordV3,
    knowledge: &ArchivedNodeKnowledgeV1,
) -> Result<(), String> {
    validate_completed_scope_record_v2(&record.committed_scope, knowledge)?;
    let expected = completed_scope_record_id_v2(&record.committed_scope, knowledge)?;
    if expected != record.record_id {
        return Err(format!(
            "run branch checkpoint V3 scope record {} has invalid identity",
            record.record_id
        ));
    }
    Ok(())
}

fn archive_selected_attempt_v3(
    committed_scope: &CommittedScopeBoundaryV1,
    knowledge: &ArchivedNodeKnowledgeV1,
    history: &CommittedPublicHistoryV1,
) -> Option<TaskAttemptOrdinalV1> {
    let [attempt] = knowledge.attempts.as_slice() else {
        return None;
    };
    if attempt.status != NodeAttemptStatusV1::SelectedAndCommitted {
        return None;
    }
    let committed_steps = history
        .steps
        .iter()
        .filter(|step| step.scope == committed_scope.scope)
        .collect::<Vec<_>>();
    let first = committed_steps.first()?;
    if first.site != committed_scope.entry_site
        || attempt.root != first.step.source
        || attempt.steps.len() != committed_steps.len()
        || !attempt
            .steps
            .iter()
            .zip(&committed_steps)
            .all(|(attempt_step, committed_step)| attempt_step == &committed_step.step)
    {
        return None;
    }
    Some(attempt.attempt)
}

fn archive_selected_knowledge_v3(
    committed_scope: &CommittedScopeBoundaryV1,
    attempt: TaskAttemptOrdinalV1,
    history_steps: &[CommittedPublicStepV1],
) -> Result<ArchivedNodeKnowledgeV1, String> {
    let committed_steps = history_steps
        .iter()
        .filter(|step| step.scope == committed_scope.scope)
        .collect::<Vec<_>>();
    let first = committed_steps.first().ok_or_else(|| {
        format!(
            "run branch checkpoint V3 archive-selected scope {} has no committed steps",
            committed_scope.scope.0
        )
    })?;
    if first.site != committed_scope.entry_site {
        return Err(format!(
            "run branch checkpoint V3 archive-selected scope {} has a different entry site",
            committed_scope.scope.0
        ));
    }
    Ok(ArchivedNodeKnowledgeV1 {
        scope: committed_scope.scope,
        entry_site: committed_scope.entry_site,
        attempts: vec![NodeAttemptV1 {
            attempt,
            root: first.step.source.clone(),
            steps: committed_steps
                .into_iter()
                .map(|step| step.step.clone())
                .collect(),
            status: NodeAttemptStatusV1::SelectedAndCommitted,
        }],
    })
}

fn completed_scope_record_id_v2(
    committed_scope: &CommittedScopeBoundaryV1,
    knowledge: &ArchivedNodeKnowledgeV1,
) -> Result<String, String> {
    stable_planner_id(
        "task_run_branch_completed_scope_record_v2",
        &(committed_scope, knowledge),
    )
}

fn validate_completed_scope_record_v2(
    committed_scope: &CommittedScopeBoundaryV1,
    knowledge: &ArchivedNodeKnowledgeV1,
) -> Result<(), String> {
    if committed_scope.scope != knowledge.scope
        || committed_scope.entry_site != knowledge.entry_site
    {
        return Err("run branch completed scope and knowledge identities differ".to_string());
    }
    Ok(())
}

fn committed_steps_for_archive_node_v1(
    nodes: &BTreeMap<&str, &TaskRunArchiveNodeV1>,
    seed: u64,
    archive_node_id: &str,
) -> Result<Vec<CommittedPublicStepV1>, String> {
    let target = nodes.get(archive_node_id).ok_or_else(|| {
        format!("run branch checkpoint V2 references missing archive node {archive_node_id}")
    })?;
    if target.seed != seed {
        return Err(format!(
            "run branch checkpoint V2 archive node {archive_node_id} belongs to another seed"
        ));
    }
    let expected_depth = target.depth;
    let mut cursor = *target;
    let mut reversed = Vec::with_capacity(expected_depth);
    while let Some(parent_node_id) = cursor.parent_node_id.as_deref() {
        reversed.push(
            cursor
                .incoming_step
                .clone()
                .ok_or_else(|| {
                    format!(
                        "run branch archive node {} has no incoming public step",
                        cursor.node_id
                    )
                })?
                .into(),
        );
        cursor = *nodes.get(parent_node_id).ok_or_else(|| {
            format!(
                "run branch archive node {} has missing parent {parent_node_id}",
                cursor.node_id
            )
        })?;
    }
    reversed.reverse();
    if reversed.len() != expected_depth {
        return Err(format!(
            "run branch checkpoint V2 archive node {archive_node_id} has discontinuous depth"
        ));
    }
    Ok(reversed)
}

fn archive_node_index_v1(
    archive: &TaskRunSolutionArchiveV1,
) -> BTreeMap<&str, &TaskRunArchiveNodeV1> {
    archive
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect()
}

fn checkpoint_entry_v1(
    archive: &TaskRunSolutionArchiveV1,
    seed: u64,
    point: TaskResearchRunBranchPointV1<SimulatorExactScopeCheckpointV1>,
) -> Result<TaskRunBranchCheckpointEntryV1, String> {
    if point.task() != &archive.task {
        return Err("run branch checkpoint belongs to a different archive task".to_string());
    }
    validate_fresh_scope_public_join(&point)?;
    let private = validate_private_scope_checkpoint(point.private_anchor())?;
    if point.root() != &private.public_root {
        return Err("run branch checkpoint public root disagrees with its exact state".to_string());
    }
    if private.seed != seed {
        return Err(format!(
            "run branch checkpoint seed {} disagrees with artifact seed {seed}",
            private.seed
        ));
    }
    let archive_node_id =
        archive.node_id_for_committed_prefix(seed, point.root(), point.committed_history())?;
    Ok(TaskRunBranchCheckpointEntryV1 {
        identity: TaskRunBranchCheckpointIdentityV1 {
            archive_node_id,
            exact_state_id: private.exact_state_id,
            lineage_id: private.lineage_id,
            root_public_snapshot_id: point.root().information.snapshot_id.clone(),
        },
        point,
    })
}

fn validate_fresh_scope_public_join(
    point: &TaskResearchRunBranchPointV1<SimulatorExactScopeCheckpointV1>,
) -> Result<(), String> {
    let history = point.committed_history();
    if history.steps.is_empty() {
        if history.completed_scopes.is_empty() {
            return Ok(());
        }
        return Err(
            "root-only run branch checkpoint contains completed committed scopes".to_string(),
        );
    }
    let last_step = history
        .steps
        .last()
        .expect("non-empty committed history has a last step");
    let last_scope = history.completed_scopes.last().ok_or_else(|| {
        "non-root run branch checkpoint has no completed committed scope".to_string()
    })?;
    if last_step.scope != last_scope.scope {
        return Err(
            "run branch checkpoint last step and completed scope ordinals differ".to_string(),
        );
    }
    match &last_scope.exit {
        TaskCommittedScopeExitV1::NextScope { decision } if decision == point.root() => Ok(()),
        TaskCommittedScopeExitV1::NextScope { .. } => Err(
            "run branch checkpoint root disagrees with its committed scope successor".to_string(),
        ),
        TaskCommittedScopeExitV1::GameTerminal { .. } => {
            Err("run branch checkpoint cannot follow a committed game terminal".to_string())
        }
    }
}

fn validate_private_scope_checkpoint(
    checkpoint: &SimulatorExactScopeCheckpointV1,
) -> Result<SimulatorExactScopeCheckpointIdentityV1, String> {
    checkpoint.validated_identity_v1()
}

struct BoundedArtifactWriter {
    bytes: Vec<u8>,
    max_bytes: usize,
    failure: Option<String>,
}

impl BoundedArtifactWriter {
    fn new(max_bytes: usize, format_version: u32) -> Result<Self, String> {
        let header_bytes = artifact_header_bytes();
        if max_bytes < header_bytes {
            return Err(
                "run branch checkpoint artifact byte limit is smaller than its header".to_string(),
            );
        }
        let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
        bytes.extend_from_slice(TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC);
        bytes.extend_from_slice(&format_version.to_be_bytes());
        Ok(Self {
            bytes,
            max_bytes,
            failure: None,
        })
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedArtifactWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(next) = self.bytes.len().checked_add(buffer.len()) else {
            let message = "run branch checkpoint artifact byte count overflow".to_string();
            self.failure = Some(message.clone());
            return Err(io::Error::other(message));
        };
        if next > self.max_bytes {
            let message =
                "run branch checkpoint artifact exceeds its caller-provided byte limit".to_string();
            self.failure = Some(message.clone());
            return Err(io::Error::other(message));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn artifact_header_bytes() -> usize {
    TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_MAGIC.len() + std::mem::size_of::<u32>()
}

fn encode_artifact_v2(
    artifact: &TaskRunBranchCheckpointArtifactV1,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let disk = TaskRunBranchCheckpointDiskArtifactV2::from_runtime_artifact_v1(artifact)?;
    let mut writer = BoundedArtifactWriter::new(
        max_bytes,
        TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_SHARED_HISTORY_FORMAT_VERSION,
    )?;
    if let Err(error) = rmp_serde::encode::write_named(&mut writer, &disk) {
        return Err(writer.failure.unwrap_or_else(|| {
            format!("cannot encode run branch checkpoint artifact V2: {error}")
        }));
    }
    Ok(writer.finish())
}

fn encode_artifact_v3(
    artifact: &TaskRunBranchCheckpointArtifactV1,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let disk = TaskRunBranchCheckpointDiskArtifactV3::from_runtime_artifact_v1(artifact)?;
    let mut writer = BoundedArtifactWriter::new(
        max_bytes,
        TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_FORMAT_VERSION,
    )?;
    if let Err(error) = rmp_serde::encode::write_named(&mut writer, &disk) {
        return Err(writer.failure.unwrap_or_else(|| {
            format!("cannot encode run branch checkpoint artifact V3: {error}")
        }));
    }
    Ok(writer.finish())
}

fn encode_legacy_artifact_v1(
    artifact: &TaskRunBranchCheckpointArtifactV1,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut writer = BoundedArtifactWriter::new(
        max_bytes,
        TASK_RUN_BRANCH_CHECKPOINT_ARTIFACT_LEGACY_FORMAT_VERSION,
    )?;
    if let Err(error) = rmp_serde::encode::write_named(&mut writer, artifact) {
        return Err(writer.failure.unwrap_or_else(|| {
            format!("cannot encode legacy run branch checkpoint artifact: {error}")
        }));
    }
    Ok(writer.finish())
}

fn require_decoder_consumed_v1(position: u64, expected_bytes: usize) -> Result<(), String> {
    if usize::try_from(position).ok() != Some(expected_bytes) {
        return Err("run branch checkpoint artifact contains trailing bytes".to_string());
    }
    Ok(())
}

#[cfg(test)]
#[path = "run_branch_checkpoint_artifact/tests.rs"]
mod tests;
