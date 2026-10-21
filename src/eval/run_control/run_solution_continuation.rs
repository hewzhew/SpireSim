//! One explicit complete-run continuation from a durable archive prefix.
//!
//! This owner does not choose a branch or schedule work. The caller names one
//! replay-observed decision node and one previously unobserved public choice.
//! The exact execution shell restores the nearest checkpoint ancestor, replays
//! the archived public prefix, forces the choice with explicit provenance, and
//! then returns control to the existing privileged reference actor. Only the
//! resulting complete task episode may change archive proof.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::agent::task::{
    run_task_privileged_support_actor_episode_capturing_fresh_scopes_v1,
    run_task_privileged_support_actor_episode_v1, CommittedPublicStepV1,
    NodeReloadTaskEnvironmentV1, TaskActorBoundaryV1, TaskActorEpisodeV1, TaskActorIdentityV1,
    TaskActorIntentV1, TaskActorV1, TaskActorViewV1, TaskBudgetV1, TaskCommittedScopeExitV1,
    TaskDecisionSiteV1, TaskEpisodeOutcomeV1, TaskObservedBoundaryV1,
    TaskPrivilegedStrategicSupportV1, TaskPrivilegedSupportActorV1,
    TaskPrivilegedSupportProvenanceV1, TaskPrivilegedSupportV1, TaskPublicCandidateV1,
    TaskPublicChoiceV1, TaskPublicDecisionV1, TaskPublicObservationV1, TaskResearchPlanAttemptV1,
    TaskResearchPlanV1, TaskRunArchiveNodeProofV1, TaskRunSolutionArchiveV1,
    TaskScopedPublicStepV1, TaskSpecV1, TaskStrategicActionV1,
};
use crate::ai::planner_core::stable_planner_id;
use crate::eval::combat_case::CombatCase;

use super::{
    ironclad_privileged_reference_environment_v1, privileged_reference_task_actor_v1,
    CombatResearchPlanAttemptReceiptV1, CombatResearchPlanOptionsV1, CombatResearchPortfolioV1,
    CombatResearchProgressEventV1, ExactBossRelicPolicyAuditV1,
    ExactBranchingTaskEvidenceRequestV1, ExactBranchingTaskExecutionEvidenceV1,
    ExactBranchingTaskWorldV1, ExactCampfirePolicyAuditV1, ExactCardRewardPolicyAuditV1,
    ExactRoutePolicyAuditV1, ExactRunChoicePolicyAuditV1, ExactShopPolicyAuditV1,
    PotionRewardPolicyAuditV1, PrivilegedReferenceTaskActorV1, TaskRunBranchCheckpointArtifactV1,
};

pub const FORCED_RUN_CONTINUATION_ACTOR_NAME_V2: &str =
    "recovered_public_prefix_deviation_then_privileged_reference";
pub const FORCED_RUN_CONTINUATION_ACTOR_REVISION_V2: u32 = 2;
pub const CONTROLLED_RUN_CONTINUATION_ACTOR_NAME_V1: &str =
    "recovered_public_prefix_deviation_then_combat_only_until_control_yield";
pub const CONTROLLED_RUN_CONTINUATION_ACTOR_REVISION_V1: u32 = 1;
pub const MANUAL_RUN_CONTINUATION_ACTOR_NAME_V1: &str =
    "recovered_public_prefix_deviation_then_public_decision_yield";
pub const MANUAL_RUN_CONTINUATION_ACTOR_REVISION_V1: u32 = 1;
pub const TASK_RUN_BRANCH_DECISION_INVENTORY_SCHEMA_V1: &str =
    "sts-task-run-branch-decision-inventory-v1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunBranchCandidateInventoryV1 {
    pub candidate_ordinal: usize,
    pub candidate: TaskPublicCandidateV1,
    pub observed_choices: Vec<TaskPublicChoiceV1>,
    pub direct_choice: Option<TaskPublicChoiceV1>,
    pub direct_choice_observed: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunBranchDecisionInventoryV1 {
    pub target_archive_node_id: String,
    pub target_archive_depth: usize,
    pub recovery_checkpoint_node_id: String,
    pub recovery_checkpoint_depth: usize,
    pub replayed_public_step_count: usize,
    pub proof: TaskRunArchiveNodeProofV1,
    pub public_snapshot_id: String,
    pub site: TaskDecisionSiteV1,
    pub observation: TaskPublicObservationV1,
    pub observed_immediate_choices: Vec<TaskPublicChoiceV1>,
    pub candidates: Vec<TaskRunBranchCandidateInventoryV1>,
    pub unobserved_direct_choice_count: usize,
}

impl TaskRunBranchDecisionInventoryV1 {
    /// Reconstructs the canonical deployable decision whose compact inventory
    /// payload is cached by run-play. The stable snapshot check prevents a
    /// diagnostic consumer from accidentally pairing candidates with a
    /// different observation while still avoiding exact checkpoint recovery.
    pub fn public_decision_v1(&self) -> Result<TaskPublicDecisionV1, String> {
        let decision = TaskPublicDecisionV1::new(
            self.site,
            self.observation.clone(),
            self.candidates
                .iter()
                .map(|candidate| candidate.candidate.clone())
                .collect(),
        )?;
        if decision.information.snapshot_id != self.public_snapshot_id {
            return Err(
                "run branch decision inventory public snapshot identity mismatched".to_string(),
            );
        }
        Ok(decision)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunBranchDecisionInventoryArtifactV1 {
    pub schema: String,
    pub task: TaskSpecV1,
    pub decisions: Vec<TaskRunBranchDecisionInventoryV1>,
}

#[derive(Clone)]
struct TaskRunBranchRecoveryPlanV1 {
    recovery_checkpoint_node_id: String,
    recovery_checkpoint_depth: usize,
    target_archive_node_id: String,
    target_archive_depth: usize,
    target_root: TaskPublicDecisionV1,
    replay_steps: Vec<CommittedPublicStepV1>,
}

/// Projects every publicly identified decision reachable by exact replay from
/// a retained checkpoint. The collection remains privileged exploration
/// metadata and is never placed in an actor view; exact identities and
/// checkpoint bytes are omitted.
pub fn task_run_branch_decision_inventory_v1(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
) -> Result<TaskRunBranchDecisionInventoryArtifactV1, String> {
    let recoveries = recoverable_run_branch_decisions_v1(archive, checkpoints)?;
    let mut children_by_parent =
        BTreeMap::<&str, Vec<&crate::agent::task::TaskRunArchiveNodeV1>>::new();
    for child in archive
        .nodes
        .iter()
        .filter(|node| node.parent_node_id.is_some())
    {
        children_by_parent
            .entry(child.parent_node_id.as_deref().expect("filtered child"))
            .or_default()
            .push(child);
    }
    let decisions = recoveries
        .into_iter()
        .map(|recovery| {
            let observed = observed_immediate_choices_from_children_v1(
                children_by_parent
                    .get(recovery.target_archive_node_id.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                &recovery.target_root,
            )?;
            decision_inventory_from_recovery_v1(archive, recovery, observed)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TaskRunBranchDecisionInventoryArtifactV1 {
        schema: TASK_RUN_BRANCH_DECISION_INVENTORY_SCHEMA_V1.to_string(),
        task: archive.task.clone(),
        decisions,
    })
}

/// Projects one named recoverable public decision. Interactive controllers use
/// this targeted operation so `show` and `choose` do not materialize a census
/// of every historical run boundary.
pub fn task_run_branch_decision_v1(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
    target_archive_node_id: &str,
) -> Result<TaskRunBranchDecisionInventoryV1, String> {
    let recovery =
        recoverable_run_branch_decision_v1(archive, checkpoints, target_archive_node_id)?;
    let observed = observed_immediate_choices_v1(
        archive,
        &recovery.target_archive_node_id,
        &recovery.target_root,
    )?;
    decision_inventory_from_recovery_v1(archive, recovery, observed)
}

/// Targeted projection for an already validated in-memory run-play session.
/// This skips only the global archive/checkpoint audit paid when the hot
/// session was opened; the named recovery chain and public decision identity
/// are still checked on every call.
#[doc(hidden)]
pub fn task_run_branch_decision_in_validated_session_v1(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
    target_archive_node_id: &str,
) -> Result<TaskRunBranchDecisionInventoryV1, String> {
    let recovery = recoverable_run_branch_decision_in_validated_session_v1(
        archive,
        checkpoints,
        target_archive_node_id,
    )?;
    let observed = observed_immediate_choices_v1(
        archive,
        &recovery.target_archive_node_id,
        &recovery.target_root,
    )?;
    decision_inventory_from_recovery_v1(archive, recovery, observed)
}

fn decision_inventory_from_recovery_v1(
    archive: &TaskRunSolutionArchiveV1,
    recovery: TaskRunBranchRecoveryPlanV1,
    mut observed_immediate_choices: Vec<TaskPublicChoiceV1>,
) -> Result<TaskRunBranchDecisionInventoryV1, String> {
    let node = archive
        .node(&recovery.target_archive_node_id)
        .expect("recovery target was resolved from the archive");
    observed_immediate_choices.sort_by(|left, right| left.ordinal_path.cmp(&right.ordinal_path));
    observed_immediate_choices.dedup();

    let candidates = recovery
        .target_root
        .candidates
        .iter()
        .cloned()
        .enumerate()
        .map(|(candidate_ordinal, candidate)| {
            let mut observed_choices = observed_immediate_choices
                .iter()
                .filter(|choice| choice.ordinal_path.first() == Some(&candidate_ordinal))
                .cloned()
                .collect::<Vec<_>>();
            observed_choices.sort_by(|left, right| left.ordinal_path.cmp(&right.ordinal_path));
            let direct_choice = (!matches!(
                &candidate,
                TaskPublicCandidateV1::CombatSelectionFamily { .. }
                    | TaskPublicCandidateV1::Strategic {
                        action: TaskStrategicActionV1::BeginRunCardSelection { .. }
                    }
            ))
            .then(|| TaskPublicChoiceV1::one(candidate_ordinal));
            let direct_choice_observed = direct_choice
                .as_ref()
                .is_some_and(|choice| observed_choices.contains(choice));
            TaskRunBranchCandidateInventoryV1 {
                candidate_ordinal,
                candidate,
                observed_choices,
                direct_choice,
                direct_choice_observed,
            }
        })
        .collect::<Vec<_>>();
    let unobserved_direct_choice_count = candidates
        .iter()
        .filter(|candidate| candidate.direct_choice.is_some() && !candidate.direct_choice_observed)
        .count();
    Ok(TaskRunBranchDecisionInventoryV1 {
        target_archive_node_id: recovery.target_archive_node_id,
        target_archive_depth: recovery.target_archive_depth,
        recovery_checkpoint_node_id: recovery.recovery_checkpoint_node_id,
        recovery_checkpoint_depth: recovery.recovery_checkpoint_depth,
        replayed_public_step_count: recovery.replay_steps.len(),
        proof: node.proof.clone(),
        public_snapshot_id: recovery.target_root.information.snapshot_id,
        site: recovery.target_root.site,
        observation: recovery.target_root.observation,
        observed_immediate_choices,
        candidates,
        unobserved_direct_choice_count,
    })
}

fn recoverable_run_branch_decision_v1(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
    target_archive_node_id: &str,
) -> Result<TaskRunBranchRecoveryPlanV1, String> {
    checkpoints.validate_for_archive(archive)?;
    recoverable_run_branch_decision_in_validated_session_v1(
        archive,
        checkpoints,
        target_archive_node_id,
    )
}

/// Joins immutable archive/checkpoint evidence across an explicit public
/// observation schema migration. Runtime restoration remains directional;
/// this relation is symmetric only because either artifact may contain the
/// older projection while both refer to the same already-recorded decision.
fn replay_join_compatible_v2(left: &TaskPublicDecisionV1, right: &TaskPublicDecisionV1) -> bool {
    left == right || left.replay_compatible_v2(right) || right.replay_compatible_v2(left)
}

fn recoverable_run_branch_decision_in_validated_session_v1(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
    target_archive_node_id: &str,
) -> Result<TaskRunBranchRecoveryPlanV1, String> {
    let seed = checkpoints.seed();
    let nodes = archive
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let target_node = nodes
        .get(target_archive_node_id)
        .copied()
        .ok_or_else(|| format!("run branch target node {target_archive_node_id} is missing"))?;
    if target_node.seed != seed {
        return Err(format!(
            "run branch target node seed {} disagrees with checkpoint seed {seed}",
            target_node.seed
        ));
    }
    let target_root =
        public_decision_at_archive_node_v1(archive, checkpoints, target_archive_node_id)?;
    let checkpoint_ids = checkpoints
        .identities()
        .map(|identity| identity.archive_node_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut cursor = target_node;
    let mut reversed_steps = Vec::new();
    while !checkpoint_ids.contains(cursor.node_id.as_str()) {
        let incoming = cursor.incoming_step.clone().ok_or_else(|| {
            format!("run branch target {target_archive_node_id} has no checkpoint ancestor")
        })?;
        reversed_steps.push(incoming.into());
        let parent_node_id = cursor.parent_node_id.as_deref().ok_or_else(|| {
            format!(
                "run branch target {target_archive_node_id} reached a root without a checkpoint"
            )
        })?;
        cursor = nodes
            .get(parent_node_id)
            .copied()
            .ok_or_else(|| format!("run branch recovery has missing ancestor {parent_node_id}"))?;
    }
    reversed_steps.reverse();
    let checkpoint_root = checkpoints
        .public_root_for_archive_node_unchecked(&cursor.node_id)
        .ok_or_else(|| format!("checkpoint ancestor {} has no public root", cursor.node_id))?;
    let first_replay_source = reversed_steps
        .first()
        .map(|step: &CommittedPublicStepV1| &step.step.source)
        .unwrap_or(&target_root);
    if !replay_join_compatible_v2(checkpoint_root, first_replay_source) {
        return Err(format!(
            "run branch recovery from checkpoint {} disagrees at its first public source",
            cursor.node_id
        ));
    }
    validate_recorded_replay_chain_v1(&reversed_steps, &target_root)?;
    Ok(TaskRunBranchRecoveryPlanV1 {
        recovery_checkpoint_node_id: cursor.node_id.clone(),
        recovery_checkpoint_depth: cursor.depth,
        target_archive_node_id: target_archive_node_id.to_string(),
        target_archive_depth: target_node.depth,
        target_root,
        replay_steps: reversed_steps,
    })
}

fn public_decision_at_archive_node_v1(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
    target_archive_node_id: &str,
) -> Result<TaskPublicDecisionV1, String> {
    let node = archive
        .node(target_archive_node_id)
        .ok_or_else(|| format!("run branch target node {target_archive_node_id} is missing"))?;
    let mut roots = Vec::new();
    if let Some(root) = checkpoints.public_root_for_archive_node_unchecked(target_archive_node_id) {
        roots.push(root);
    }
    if let Some(root) = node.initial_public_decision.as_ref() {
        roots.push(root);
    }
    if let Some(incoming) = node.incoming_step.as_ref() {
        match &incoming.step.observed_boundary {
            TaskObservedBoundaryV1::Decision { decision }
            | TaskObservedBoundaryV1::CommittedNextScope { decision } => roots.push(decision),
            _ => {}
        }
    }
    roots.extend(
        archive
            .nodes
            .iter()
            .filter(|child| child.parent_node_id.as_deref() == Some(target_archive_node_id))
            .filter_map(|child| child.incoming_step.as_ref())
            .map(|incoming| &incoming.step.source),
    );
    let Some(first) = roots.first().copied() else {
        return Err(format!(
            "archive node {target_archive_node_id} has no observed public decision"
        ));
    };
    if roots
        .iter()
        .any(|root| !replay_join_compatible_v2(first, root))
    {
        return Err(format!(
            "archive node {target_archive_node_id} has conflicting public decision identities"
        ));
    }
    Ok(first.clone())
}

fn recoverable_run_branch_decisions_v1(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
) -> Result<Vec<TaskRunBranchRecoveryPlanV1>, String> {
    archive.validate()?;
    let public_checkpoints = checkpoints.public_roots_for_archive(archive)?;
    let seed = checkpoints.seed();
    let checkpoint_roots = public_checkpoints
        .into_iter()
        .map(|checkpoint| (checkpoint.archive_node_id, checkpoint.root))
        .collect::<BTreeMap<_, _>>();
    let nodes = archive
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<BTreeMap<_, _>>();

    // A checkpoint root identifies its decision directly. Every other public
    // decision is identified by the common source stored on its observed
    // outgoing edges. Conflicting sources for one public prefix are ambiguous
    // and therefore not safe to recover from this artifact pair.
    let mut target_roots = checkpoint_roots.clone();
    for child in archive.nodes.iter().filter(|node| node.seed == seed) {
        let Some(parent_node_id) = child.parent_node_id.as_ref() else {
            continue;
        };
        let incoming = child.incoming_step.as_ref().ok_or_else(|| {
            format!(
                "run archive child {} has no incoming public step",
                child.node_id
            )
        })?;
        match target_roots.get(parent_node_id) {
            Some(existing) if !replay_join_compatible_v2(existing, &incoming.step.source) => {
                return Err(format!(
                    "run archive node {parent_node_id} has conflicting public decision sources"
                ));
            }
            Some(_) => {}
            None => {
                target_roots.insert(parent_node_id.clone(), incoming.step.source.clone());
            }
        }
    }
    for node in archive.nodes.iter().filter(|node| node.seed == seed) {
        let Some(incoming) = node.incoming_step.as_ref() else {
            continue;
        };
        let decision = match &incoming.step.observed_boundary {
            TaskObservedBoundaryV1::Decision { decision }
            | TaskObservedBoundaryV1::CommittedNextScope { decision } => decision,
            _ => continue,
        };
        match target_roots.get(&node.node_id) {
            Some(existing) if !replay_join_compatible_v2(existing, decision) => {
                return Err(format!(
                    "run archive node {} has conflicting observed successor decisions",
                    node.node_id
                ));
            }
            Some(_) => {}
            None => {
                target_roots.insert(node.node_id.clone(), decision.clone());
            }
        }
    }

    let mut recoveries = Vec::with_capacity(target_roots.len());
    for (target_archive_node_id, target_root) in target_roots {
        let target_node = nodes
            .get(target_archive_node_id.as_str())
            .ok_or_else(|| format!("run branch target node {target_archive_node_id} is missing"))?;
        let mut cursor = *target_node;
        let mut reversed_steps: Vec<CommittedPublicStepV1> = Vec::new();
        while !checkpoint_roots.contains_key(&cursor.node_id) {
            let incoming = cursor.incoming_step.clone().ok_or_else(|| {
                format!(
                    "run branch target {} has no checkpoint ancestor",
                    target_archive_node_id
                )
            })?;
            reversed_steps.push(incoming.into());
            let parent_node_id = cursor.parent_node_id.as_deref().ok_or_else(|| {
                format!(
                    "run branch target {} reached a root without a checkpoint",
                    target_archive_node_id
                )
            })?;
            cursor = *nodes.get(parent_node_id).ok_or_else(|| {
                format!("run branch recovery has missing ancestor {parent_node_id}")
            })?;
        }
        reversed_steps.reverse();
        let checkpoint_root = &checkpoint_roots[&cursor.node_id];
        let first_replay_source = reversed_steps
            .first()
            .map(|step| &step.step.source)
            .unwrap_or(&target_root);
        if !replay_join_compatible_v2(checkpoint_root, first_replay_source) {
            return Err(format!(
                "run branch recovery from checkpoint {} disagrees at its first public source",
                cursor.node_id
            ));
        }
        validate_recorded_replay_chain_v1(&reversed_steps, &target_root)?;
        recoveries.push(TaskRunBranchRecoveryPlanV1 {
            recovery_checkpoint_node_id: cursor.node_id.clone(),
            recovery_checkpoint_depth: cursor.depth,
            target_archive_node_id,
            target_archive_depth: target_node.depth,
            target_root,
            replay_steps: reversed_steps,
        });
    }
    recoveries.sort_by(|left, right| {
        left.target_archive_depth
            .cmp(&right.target_archive_depth)
            .then_with(|| {
                left.target_archive_node_id
                    .cmp(&right.target_archive_node_id)
            })
    });
    Ok(recoveries)
}

fn validate_recorded_replay_chain_v1(
    steps: &[CommittedPublicStepV1],
    target_root: &TaskPublicDecisionV1,
) -> Result<(), String> {
    for (index, step) in steps.iter().enumerate() {
        let next_source = steps
            .get(index + 1)
            .map(|next| &next.step.source)
            .unwrap_or(target_root);
        match &step.step.observed_boundary {
            TaskObservedBoundaryV1::Decision { decision }
            | TaskObservedBoundaryV1::CommittedNextScope { decision }
                if !replay_join_compatible_v2(decision, next_source) =>
            {
                return Err(format!(
                    "run branch replay step {index} disagrees with its next public source"
                ));
            }
            TaskObservedBoundaryV1::CommittedGameTerminal { .. } => {
                return Err("run branch replay path continues after a game terminal".to_string());
            }
            _ => {}
        }
    }
    Ok(())
}

fn observed_immediate_choices_v1(
    archive: &TaskRunSolutionArchiveV1,
    archive_node_id: &str,
    root: &TaskPublicDecisionV1,
) -> Result<Vec<TaskPublicChoiceV1>, String> {
    let children = archive
        .nodes
        .iter()
        .filter(|child| child.parent_node_id.as_deref() == Some(archive_node_id))
        .collect::<Vec<_>>();
    observed_immediate_choices_from_children_v1(&children, root)
}

fn observed_immediate_choices_from_children_v1(
    children: &[&crate::agent::task::TaskRunArchiveNodeV1],
    root: &TaskPublicDecisionV1,
) -> Result<Vec<TaskPublicChoiceV1>, String> {
    children
        .iter()
        .copied()
        .map(|child| {
            let incoming = child.incoming_step.as_ref().ok_or_else(|| {
                format!(
                    "run archive child {} has no incoming public step",
                    child.node_id
                )
            })?;
            if !replay_join_compatible_v2(root, &incoming.step.source) {
                return Err(format!(
                    "run archive child {} disagrees with target public root",
                    child.node_id
                ));
            }
            Ok(incoming.step.choice.clone())
        })
        .collect()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunForcedPublicDeviationV2 {
    pub recovery_checkpoint_node_id: String,
    pub target_archive_node_id: String,
    pub replayed_public_step_count: usize,
    pub target_public_snapshot_id: String,
    pub choice: TaskPublicChoiceV1,
}

pub struct PrivilegedReferenceRunContinuationV2 {
    pub seed: u64,
    pub forced_deviation: TaskRunForcedPublicDeviationV2,
    pub execution_manifest_id: String,
    pub attempt_id: String,
    pub episode: TaskActorEpisodeV1,
    pub execution: ExactBranchingTaskExecutionEvidenceV1,
    pub strategic_support: Vec<TaskPrivilegedStrategicSupportV1>,
    pub combat_research_plans_installed: usize,
    pub combat_research_plans_unavailable: usize,
    pub combat_research_plan_actions: usize,
    pub combat_research_plan_attempts: Vec<CombatResearchPlanAttemptReceiptV1>,
    pub archive: TaskRunSolutionArchiveV1,
    pub checkpoints: TaskRunBranchCheckpointArtifactV1,
}

/// Resolves the public decision at which a paused continuation yielded back
/// to its controller. The identity is reconstructed from the just-recorded
/// attempt's committed leaf plus the final view's current active-scope path,
/// then checked against the exact final public decision. Publicly identical
/// decisions may legitimately recur after actions such as closing and
/// reopening a card reward, so observation equality alone is not an identity.
pub fn task_run_paused_decision_node_id_v1(
    archive: &TaskRunSolutionArchiveV1,
    attempt_id: &str,
    episode: &TaskActorEpisodeV1,
) -> Result<String, String> {
    if !matches!(
        episode.outcome,
        crate::agent::task::TaskEpisodeOutcomeV1::Paused { .. }
    ) {
        return Err("run continuation did not yield a paused episode".to_string());
    }
    let TaskActorBoundaryV1::Decision { decision } = &episode.final_view.boundary else {
        return Err("paused run continuation has no public decision boundary".to_string());
    };
    let attempt = archive
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_id == attempt_id)
        .ok_or_else(|| format!("paused run continuation attempt {attempt_id} is missing"))?;
    let committed_leaf = attempt
        .node_path
        .last()
        .ok_or_else(|| "paused run continuation attempt has no committed leaf".to_string())?;
    let mut resolved_node_id = committed_leaf.clone();
    if let Some(memory) = episode.final_view.node_memory.as_ref() {
        let current_attempt = memory
            .attempts
            .last()
            .ok_or_else(|| "paused run continuation has empty active-scope memory".to_string())?;
        for step in &current_attempt.steps {
            let scoped = TaskScopedPublicStepV1 {
                scope: memory.scope,
                site: step.source.site,
                step: step.clone(),
            };
            resolved_node_id = stable_planner_id(
                "task_run_archive_node_v1",
                &(resolved_node_id.as_str(), &scoped),
            )?;
        }
        if !current_attempt.steps.is_empty()
            && !attempt
                .observed_frontier_node_ids
                .contains(&resolved_node_id)
        {
            return Err(
                "paused active-scope path is missing from its recorded archive frontier"
                    .to_string(),
            );
        }
    }
    let node = archive.node(&resolved_node_id).ok_or_else(|| {
        format!("paused run continuation node {resolved_node_id} is missing from the archive")
    })?;
    let observed = node.initial_public_decision.as_ref().or_else(|| {
        node.incoming_step
            .as_ref()
            .and_then(|incoming| match &incoming.step.observed_boundary {
                TaskObservedBoundaryV1::Decision { decision }
                | TaskObservedBoundaryV1::CommittedNextScope { decision } => Some(decision),
                TaskObservedBoundaryV1::CommitReady
                | TaskObservedBoundaryV1::ExploredScopeExit { .. }
                | TaskObservedBoundaryV1::CommittedGameTerminal { .. } => None,
            })
    });
    if observed != Some(decision) {
        return Err(
            "paused archive path does not end at the exact final public decision".to_string(),
        );
    }
    Ok(resolved_node_id)
}

#[derive(Clone, Copy)]
enum RunContinuationControlV1 {
    Complete,
    YieldAtStrategicDecision,
    YieldAtPublicDecision,
}

struct RecoveredPrefixPrivilegedActorV2 {
    replay_steps: Vec<CommittedPublicStepV1>,
    replay_cursor: usize,
    pending_replay_boundary: Option<TaskObservedBoundaryV1>,
    target_root: TaskPublicDecisionV1,
    forced_choice: Option<TaskPublicChoiceV1>,
    forced_choice_submitted: bool,
    recovery_error: Option<String>,
    control: RunContinuationControlV1,
    continuation: PrivilegedReferenceTaskActorV1,
}

impl RecoveredPrefixPrivilegedActorV2 {
    fn new(
        replay_steps: Vec<CommittedPublicStepV1>,
        target_root: TaskPublicDecisionV1,
        forced_choice: Option<TaskPublicChoiceV1>,
        control: RunContinuationControlV1,
    ) -> Self {
        Self {
            replay_steps,
            replay_cursor: 0,
            pending_replay_boundary: None,
            target_root,
            forced_choice,
            forced_choice_submitted: false,
            recovery_error: None,
            control,
            continuation: privileged_reference_task_actor_v1(),
        }
    }

    fn recovery_intent(
        &mut self,
        view: &TaskActorViewV1,
    ) -> Result<Option<TaskActorIntentV1>, String> {
        if let Some(expected) = self.pending_replay_boundary.take() {
            match expected {
                TaskObservedBoundaryV1::Decision { decision }
                | TaskObservedBoundaryV1::CommittedNextScope { decision } => {
                    require_decision_boundary_v1(view, &decision, "replayed successor")?;
                }
                TaskObservedBoundaryV1::CommitReady => {
                    if !matches!(view.boundary, TaskActorBoundaryV1::CommitReady) {
                        return Err(
                            "run branch replay did not reproduce its commit-ready boundary"
                                .to_string(),
                        );
                    }
                    return Ok(Some(TaskActorIntentV1::CommitScope));
                }
                TaskObservedBoundaryV1::ExploredScopeExit { exit } => {
                    if !matches!(
                        &view.boundary,
                        TaskActorBoundaryV1::ExploredScopeExit { exit: observed } if observed == &exit
                    ) {
                        return Err(
                            "run branch replay did not reproduce its explored scope exit"
                                .to_string(),
                        );
                    }
                    return Ok(Some(TaskActorIntentV1::CommitScope));
                }
                TaskObservedBoundaryV1::CommittedGameTerminal { .. } => {
                    return Err(
                        "run branch replay reached a terminal before its target decision"
                            .to_string(),
                    );
                }
            }
        }

        if let Some(recorded) = self.replay_steps.get(self.replay_cursor).cloned() {
            require_decision_boundary_v1(view, &recorded.step.source, "replayed source")?;
            let observed_scope = view.node_memory.as_ref().map(|memory| memory.scope);
            if observed_scope != Some(recorded.scope) {
                return Err(format!(
                    "run branch replay scope mismatch at public step {}",
                    self.replay_cursor
                ));
            }
            self.replay_cursor += 1;
            self.pending_replay_boundary = Some(recorded.step.observed_boundary);
            return Ok(Some(TaskActorIntentV1::Act {
                choice: recorded.step.choice,
            }));
        }

        require_decision_boundary_v1(view, &self.target_root, "forced-deviation target")?;
        if let Some(choice) = self.forced_choice.take() {
            self.forced_choice_submitted = true;
            return Ok(Some(TaskActorIntentV1::Act { choice }));
        }
        self.forced_choice_submitted = true;
        Ok(Some(TaskActorIntentV1::YieldControl))
    }

    fn require_completed_recovery(&self) -> Result<(), String> {
        if let Some(error) = &self.recovery_error {
            return Err(error.clone());
        }
        if self.pending_replay_boundary.is_some()
            || self.replay_cursor != self.replay_steps.len()
            || !self.forced_choice_submitted
        {
            return Err(
                "run branch episode ended before prefix recovery and forced deviation completed"
                    .to_string(),
            );
        }
        Ok(())
    }

    fn should_yield(&self, view: &TaskActorViewV1) -> bool {
        let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return false;
        };
        match self.control {
            RunContinuationControlV1::Complete => false,
            RunContinuationControlV1::YieldAtStrategicDecision => {
                decision.site != TaskDecisionSiteV1::Combat
            }
            RunContinuationControlV1::YieldAtPublicDecision => true,
        }
    }
}

fn require_decision_boundary_v1(
    view: &TaskActorViewV1,
    expected: &TaskPublicDecisionV1,
    description: &str,
) -> Result<(), String> {
    match &view.boundary {
        TaskActorBoundaryV1::Decision { decision } if expected.replay_compatible_v2(decision) => {
            Ok(())
        }
        TaskActorBoundaryV1::Decision { decision } => Err(format!(
            "run branch {description} public decision identity mismatched at {}",
            expected
                .replay_incompatibility_v2(decision)
                .unwrap_or_else(|| "unknown component".to_string())
        )),
        _ => Err(format!(
            "run branch {description} was not a public decision boundary"
        )),
    }
}

impl TaskActorV1 for RecoveredPrefixPrivilegedActorV2 {
    fn identity(&self) -> TaskActorIdentityV1 {
        continuation_actor_identity_v1(self.control)
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        if !self.forced_choice_submitted {
            return match self.recovery_intent(view) {
                Ok(Some(intent)) => intent,
                Ok(None) => {
                    self.recovery_error =
                        Some("run branch recovery produced no forced-deviation intent".to_string());
                    TaskActorIntentV1::UnsupportedDecisionSurface {
                        reason: self.recovery_error.clone().expect("error was installed"),
                    }
                }
                Err(error) => {
                    self.recovery_error = Some(error.clone());
                    TaskActorIntentV1::UnsupportedDecisionSurface { reason: error }
                }
            };
        }
        if self.should_yield(view) {
            return TaskActorIntentV1::YieldControl;
        }
        self.continuation.next_intent(view)
    }
}

impl TaskPrivilegedSupportActorV1 for RecoveredPrefixPrivilegedActorV2 {
    fn needs_privileged_support(&mut self, view: &TaskActorViewV1) -> bool {
        self.forced_choice_submitted
            && self.recovery_error.is_none()
            && !self.should_yield(view)
            && self.continuation.needs_privileged_support(view)
    }

    fn receive_privileged_support(
        &mut self,
        view: &TaskActorViewV1,
        support: TaskPrivilegedSupportV1,
    ) -> Result<(), String> {
        if !self.forced_choice_submitted {
            return Err(
                "run continuation received support before its forced deviation".to_string(),
            );
        }
        self.continuation.receive_privileged_support(view, support)
    }
}

fn continuation_actor_identity_v1(control: RunContinuationControlV1) -> TaskActorIdentityV1 {
    match control {
        RunContinuationControlV1::Complete => TaskActorIdentityV1::privileged_support_assisted(
            FORCED_RUN_CONTINUATION_ACTOR_NAME_V2,
            FORCED_RUN_CONTINUATION_ACTOR_REVISION_V2,
        ),
        RunContinuationControlV1::YieldAtStrategicDecision => {
            TaskActorIdentityV1::privileged_support_assisted(
                CONTROLLED_RUN_CONTINUATION_ACTOR_NAME_V1,
                CONTROLLED_RUN_CONTINUATION_ACTOR_REVISION_V1,
            )
        }
        RunContinuationControlV1::YieldAtPublicDecision => {
            TaskActorIdentityV1::privileged_support_assisted(
                MANUAL_RUN_CONTINUATION_ACTOR_NAME_V1,
                MANUAL_RUN_CONTINUATION_ACTOR_REVISION_V1,
            )
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactRunPlayAdvanceV1 {
    NextPublicDecision,
    NextStrategicDecision,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExactRunPlayStepProfileV1 {
    pub total: Duration,
    pub setup: Duration,
    pub actor_episode: Duration,
    pub validation: Duration,
    pub archive_record: Duration,
    pub checkpoint_merge: Duration,
    pub endpoint_projection: Duration,
}

pub struct ExactRunPlayStepV1 {
    pub attempt_id: String,
    pub episode: TaskActorEpisodeV1,
    pub current_archive_node_id: Option<String>,
    pub archive_nodes_added: usize,
    pub archive_attempts_added: usize,
    pub checkpoints_added: usize,
    pub profile: ExactRunPlayStepProfileV1,
}

/// One non-publishing action observed while comparing a candidate line from a
/// hot run-play root. Both boundaries remain public task payloads; the private
/// execution fork never enters the durable archive or checkpoint sidecar.
pub struct ExactRunPlayProbeStepV1 {
    pub source_decision: TaskPublicDecisionV1,
    /// Runtime correspondence captured on the same non-publishing fork. These
    /// ids stay private; diagnostic deltas use them instead of transient slots.
    pub monster_entities_before: Vec<(u8, crate::EntityId)>,
    pub monster_entities_after: Vec<(u8, crate::EntityId)>,
    pub choice: TaskPublicChoiceV1,
    pub selected_candidate: TaskPublicCandidateV1,
    pub action_result: Option<super::RunActionResultV1>,
    pub outcome: TaskEpisodeOutcomeV1,
    pub next_decision: Option<TaskPublicDecisionV1>,
    pub elapsed: Duration,
}

/// A bounded read-only exact line. The fork cost is kept separate from action
/// execution so the interactive workbench can distinguish simulator work from
/// durable publication overhead without serializing performance data.
pub struct ExactRunPlayProbeLineV1 {
    pub steps: Vec<ExactRunPlayProbeStepV1>,
    pub fork_elapsed: Duration,
    pub total_elapsed: Duration,
}

/// User inspection retains expected resolution failures and terminal prefixes.
/// This is not proof that every requested action was simulated.
pub struct ExactRunPlayInspectionV1 {
    pub line: ExactRunPlayProbeLineV1,
    pub requested_steps: usize,
    pub stop: ExactRunPlayInspectionStopV1,
}

pub enum ExactRunPlayInspectionStopV1 {
    Complete,
    Terminal {
        unapplied_steps: usize,
    },
    Unresolved {
        requested_step: usize,
        error: String,
        decision: Box<TaskPublicDecisionV1>,
    },
}

/// Opaque in-memory recovery anchor for one exact run-play decision. It owns
/// no archive or checkpoint artifact; the durable session remains the source
/// of reachability evidence. A controller may use the anchor only while it
/// keeps the corresponding validated archive in memory.
pub struct ExactRunPlayHotSnapshotV1 {
    environment: NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>,
    archive_node_id: String,
    public_root: TaskPublicDecisionV1,
}

/// A hot exact full-run controller. Prefix restore and checkpoint decoding are
/// paid once at `open`; subsequent choices advance the same private world and
/// append typed archive evidence without rebuilding a global recovery census.
pub struct ExactRunPlaySessionV1 {
    seed: u64,
    requested_budget: TaskBudgetV1,
    environment: NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>,
    archive: TaskRunSolutionArchiveV1,
    checkpoints: TaskRunBranchCheckpointArtifactV1,
    current_archive_node_id: Option<String>,
    current_root: Option<TaskPublicDecisionV1>,
}

/// Process-local phase timings for opening one exact run-play decision. These
/// diagnostics are never serialized, exposed to an actor, or used to alter an
/// execution path.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExactRunPlayOpenProfileV1 {
    pub artifact_validation: Duration,
    pub recovery_resolution: Duration,
    pub environment_construction: Duration,
    pub checkpoint_restore: Duration,
    pub replay_episode: Duration,
    pub final_validation: Duration,
    pub replayed_public_steps: usize,
}

/// Transactionally prepared durable recovery point for the current run-play
/// decision.  The owning controller publishes the matching archive/checkpoint
/// state before installing this private execution state in its hot session.
pub struct ExactRunPlayPreparedCheckpointV1 {
    environment: NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>,
    checkpoints: TaskRunBranchCheckpointArtifactV1,
    current_decision: TaskRunBranchDecisionInventoryV1,
    archive_node_id: String,
    public_root: TaskPublicDecisionV1,
}

impl ExactRunPlayPreparedCheckpointV1 {
    pub fn checkpoints(&self) -> &TaskRunBranchCheckpointArtifactV1 {
        &self.checkpoints
    }

    pub fn current_decision(&self) -> &TaskRunBranchDecisionInventoryV1 {
        &self.current_decision
    }
}

/// A complete public action line prepared on an isolated exact session.  The
/// live session is unchanged until its controller durably publishes the
/// matching archive/checkpoint state and installs this bundle.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExactRunPlayPreparedLineProfileV1 {
    pub total: Duration,
    pub initial_fork: Duration,
    pub selector_resolution: Duration,
    pub step_total: Duration,
    pub step_setup: Duration,
    pub actor_episode: Duration,
    pub step_validation: Duration,
    pub archive_record: Duration,
    pub checkpoint_merge: Duration,
    pub endpoint_projection: Duration,
    pub final_checkpoint: Duration,
}

pub struct ExactRunPlayPreparedLineV1 {
    session: ExactRunPlaySessionV1,
    current_decision: Option<TaskRunBranchDecisionInventoryV1>,
    terminal_outcome: Option<TaskEpisodeOutcomeV1>,
    from_archive_node_id: String,
    to_archive_node_id: String,
    choices: Vec<TaskPublicChoiceV1>,
    archive_nodes_before: usize,
    archive_attempts_before: usize,
    checkpoints_before: usize,
    profile: ExactRunPlayPreparedLineProfileV1,
}

impl ExactRunPlayPreparedLineV1 {
    pub fn archive(&self) -> &TaskRunSolutionArchiveV1 {
        self.session.archive()
    }

    pub fn checkpoints(&self) -> &TaskRunBranchCheckpointArtifactV1 {
        self.session.checkpoints()
    }

    pub fn current_decision(&self) -> Option<&TaskRunBranchDecisionInventoryV1> {
        self.current_decision.as_ref()
    }

    pub fn terminal_outcome(&self) -> Option<&TaskEpisodeOutcomeV1> {
        self.terminal_outcome.as_ref()
    }

    /// Captures the prepared endpoint for the trusted run-play acceptance
    /// owner without exposing private simulator state to the public protocol.
    #[doc(hidden)]
    pub fn current_run_decision_checkpoint_for_run_play_v1(
        &self,
    ) -> Result<super::RunControlSessionCheckpointV1, String> {
        self.session.current_run_decision_checkpoint_v1()
    }

    /// Returns the prepared endpoint only to the trusted run-play acceptance
    /// owner. It is never serialized into the public protocol.
    #[doc(hidden)]
    pub fn current_combat_position_for_run_play_v1(
        &self,
    ) -> Result<crate::sim::combat::CombatPosition, String> {
        self.session.current_combat_position_for_run_play_v1()
    }

    pub fn source_archive_node_id(&self) -> &str {
        &self.from_archive_node_id
    }

    pub fn to_archive_node_id(&self) -> &str {
        &self.to_archive_node_id
    }

    pub fn choices(&self) -> &[TaskPublicChoiceV1] {
        &self.choices
    }

    pub fn archive_nodes_before(&self) -> usize {
        self.archive_nodes_before
    }

    pub fn archive_attempts_before(&self) -> usize {
        self.archive_attempts_before
    }

    pub fn checkpoints_before(&self) -> usize {
        self.checkpoints_before
    }

    pub fn profile(&self) -> ExactRunPlayPreparedLineProfileV1 {
        self.profile
    }
}

impl ExactRunPlaySessionV1 {
    pub fn open(
        archive: TaskRunSolutionArchiveV1,
        checkpoints: TaskRunBranchCheckpointArtifactV1,
        target_archive_node_id: &str,
        budget: TaskBudgetV1,
        plan_options: CombatResearchPlanOptionsV1,
        evidence_request: ExactBranchingTaskEvidenceRequestV1,
    ) -> Result<Self, String> {
        Self::open_profiled_v1(
            archive,
            checkpoints,
            target_archive_node_id,
            budget,
            plan_options,
            evidence_request,
        )
        .map(|(session, _profile)| session)
    }

    #[doc(hidden)]
    pub fn open_profiled_v1(
        archive: TaskRunSolutionArchiveV1,
        checkpoints: TaskRunBranchCheckpointArtifactV1,
        target_archive_node_id: &str,
        budget: TaskBudgetV1,
        plan_options: CombatResearchPlanOptionsV1,
        evidence_request: ExactBranchingTaskEvidenceRequestV1,
    ) -> Result<(Self, ExactRunPlayOpenProfileV1), String> {
        Self::open_profiled_impl_v1(
            archive,
            checkpoints,
            target_archive_node_id,
            budget,
            plan_options,
            evidence_request,
            true,
        )
    }

    /// Opens a session after an immutable workspace owner already validated
    /// the complete archive/checkpoint join during typed decoding.
    #[doc(hidden)]
    pub fn open_in_validated_session_profiled_v1(
        archive: TaskRunSolutionArchiveV1,
        checkpoints: TaskRunBranchCheckpointArtifactV1,
        target_archive_node_id: &str,
        budget: TaskBudgetV1,
        plan_options: CombatResearchPlanOptionsV1,
        evidence_request: ExactBranchingTaskEvidenceRequestV1,
    ) -> Result<(Self, ExactRunPlayOpenProfileV1), String> {
        Self::open_profiled_impl_v1(
            archive,
            checkpoints,
            target_archive_node_id,
            budget,
            plan_options,
            evidence_request,
            false,
        )
    }

    fn open_profiled_impl_v1(
        archive: TaskRunSolutionArchiveV1,
        checkpoints: TaskRunBranchCheckpointArtifactV1,
        target_archive_node_id: &str,
        budget: TaskBudgetV1,
        plan_options: CombatResearchPlanOptionsV1,
        evidence_request: ExactBranchingTaskEvidenceRequestV1,
        validate_artifact: bool,
    ) -> Result<(Self, ExactRunPlayOpenProfileV1), String> {
        let mut profile = ExactRunPlayOpenProfileV1::default();
        budget.validate()?;
        plan_options.validate()?;
        if validate_artifact {
            let phase = Instant::now();
            checkpoints.validate_for_archive(&archive)?;
            profile.artifact_validation = phase.elapsed();
        }
        let seed = checkpoints.seed();
        let phase = Instant::now();
        let recovery = recoverable_run_branch_decision_in_validated_session_v1(
            &archive,
            &checkpoints,
            target_archive_node_id,
        )?;
        profile.recovery_resolution = phase.elapsed();
        profile.replayed_public_steps = recovery.replay_steps.len();
        let phase = Instant::now();
        let mut environment = match &archive.task.initial_condition {
            crate::agent::task::TaskInitialConditionV1::NaturalNewGameEntrance
            | crate::agent::task::TaskInitialConditionV1::ExactRunDecision { .. } => checkpoints
                .exact_run_environment_for_restore_v1(
                    &recovery.recovery_checkpoint_node_id,
                    budget,
                    plan_options,
                    evidence_request,
                )?,
            crate::agent::task::TaskInitialConditionV1::ExactProductionCombatCase { .. } => {
                return Err(
                    "exact run-play cannot open a combat-case-only task as a complete run"
                        .to_string(),
                );
            }
        };
        profile.environment_construction = phase.elapsed();
        let phase = Instant::now();
        checkpoints.restore_into_environment_in_validated_session_v1(
            &recovery.recovery_checkpoint_node_id,
            &mut environment,
            budget,
        )?;
        profile.checkpoint_restore = phase.elapsed();
        let mut actor = RecoveredPrefixPrivilegedActorV2::new(
            recovery.replay_steps,
            recovery.target_root.clone(),
            None,
            RunContinuationControlV1::YieldAtPublicDecision,
        );
        let phase = Instant::now();
        let episode = run_task_privileged_support_actor_episode_v1(&mut environment, &mut actor)
            .map_err(|error| error.to_string())?;
        actor.require_completed_recovery()?;
        profile.replay_episode = phase.elapsed();
        let phase = Instant::now();
        if !matches!(episode.outcome, TaskEpisodeOutcomeV1::Paused { .. }) {
            return Err("exact run-play open did not yield at its target decision".to_string());
        }
        let TaskActorBoundaryV1::Decision { decision } = &episode.final_view.boundary else {
            return Err("exact run-play open has no target public decision".to_string());
        };
        if !recovery.target_root.replay_compatible_v2(decision) {
            return Err("exact run-play open restored a different public decision".to_string());
        }
        let current_root = decision.clone();
        let session = Self {
            seed,
            requested_budget: budget,
            environment,
            archive,
            checkpoints,
            current_archive_node_id: Some(target_archive_node_id.to_string()),
            current_root: Some(current_root),
        };
        profile.final_validation = phase.elapsed();
        Ok((session, profile))
    }

    pub fn archive(&self) -> &TaskRunSolutionArchiveV1 {
        &self.archive
    }

    pub fn checkpoints(&self) -> &TaskRunBranchCheckpointArtifactV1 {
        &self.checkpoints
    }

    /// Returns the exact artifacts after an owner has used `open` as a
    /// publication gate. Durable workspaces can therefore prove that their
    /// configured controller restores the root before writing the first file,
    /// without cloning private checkpoint state.
    #[doc(hidden)]
    pub fn into_archive_and_checkpoints_v1(
        self,
    ) -> (TaskRunSolutionArchiveV1, TaskRunBranchCheckpointArtifactV1) {
        (self.archive, self.checkpoints)
    }

    pub fn current_archive_node_id(&self) -> Option<&str> {
        self.current_archive_node_id.as_deref()
    }

    pub fn current_public_decision(&self) -> Option<&TaskPublicDecisionV1> {
        self.current_root.as_ref()
    }

    /// Captures the exact simulator/RNG state at the current public decision
    /// without retaining this task's public history or observation root.
    #[doc(hidden)]
    pub fn current_run_decision_checkpoint_v1(
        &self,
    ) -> Result<super::RunControlSessionCheckpointV1, String> {
        if self.current_root.is_none() || self.current_archive_node_id.is_none() {
            return Err("exact run-play reroot requires a public decision".to_string());
        }
        self.environment
            .inspect_private_research_execution_v1(|world| {
                world.current_run_decision_checkpoint_v1()
            })
    }

    /// Reads the exact strategic owner's projected public choice without
    /// mutating the live run-play world.  The support is generated on a
    /// process-local fork and therefore cannot consume budget, append an
    /// attempt, or move the controller head.
    pub fn current_privileged_strategic_support_v1(
        &self,
    ) -> Result<TaskPrivilegedStrategicSupportV1, String> {
        let current = self
            .current_root
            .as_ref()
            .ok_or_else(|| "exact run-play advice requires a public decision".to_string())?;
        if current.site == TaskDecisionSiteV1::Combat {
            return Err("exact run-play strategic advice is unavailable in combat".to_string());
        }
        let mut fork = self
            .environment
            .fork_private_research_execution_v1(|world| world.fork_for_hot_run_play_v1())?;
        let support = match fork.prepare_privileged_support()? {
            TaskPrivilegedSupportV1::StrategicChoice { support } => support,
            TaskPrivilegedSupportV1::CombatResearchPlan { .. } => {
                return Err(
                    "exact run-play received combat support at a strategic decision".to_string(),
                );
            }
        };
        support.validate(&fork.actor_view())?;
        if support.root_public_snapshot_id != current.information.snapshot_id {
            return Err("exact run-play advice belongs to another public root".to_string());
        }
        Ok(support)
    }

    /// Runs the configured combat-research support on an isolated fork of the
    /// current exact root and returns its complete lane/candidate attempt. The
    /// live run-play environment remains unchanged.
    /// Runs combat research on an isolated fork with an optional process-local
    /// effort override. The override never mutates the durable session or its
    /// configured default; it only changes the portfolio used for this one
    /// read-only attempt.
    pub fn current_combat_research_attempt_with_options_v1(
        &self,
        options: Option<CombatResearchPlanOptionsV1>,
    ) -> Result<TaskResearchPlanAttemptV1, String> {
        let current = self.current_root.as_ref().ok_or_else(|| {
            "exact run-play combat research requires a public decision".to_string()
        })?;
        if current.site != TaskDecisionSiteV1::Combat {
            return Err("exact run-play combat research requires a combat decision".to_string());
        }
        let portfolio = self.current_combat_research_portfolio_with_options_v1(options)?;
        portfolio.attempt.validate()?;
        Ok(portfolio.attempt)
    }

    /// Runs terminal-witness and finite typed-milestone projection from the
    /// same bounded resident portfolio on one isolated exact fork.
    pub fn current_combat_research_portfolio_with_options_v1(
        &self,
        options: Option<CombatResearchPlanOptionsV1>,
    ) -> Result<CombatResearchPortfolioV1, String> {
        self.current_combat_research_portfolio_with_options_and_progress_v1(
            options,
            None,
            &mut |_| {},
        )
    }

    pub fn current_combat_research_portfolio_with_options_and_progress_v1(
        &self,
        options: Option<CombatResearchPlanOptionsV1>,
        deadline: Option<Instant>,
        progress: &mut dyn FnMut(CombatResearchProgressEventV1),
    ) -> Result<CombatResearchPortfolioV1, String> {
        let current = self.current_root.as_ref().ok_or_else(|| {
            "exact run-play combat research requires a public decision".to_string()
        })?;
        if current.site != TaskDecisionSiteV1::Combat {
            return Err("exact run-play combat research requires a combat decision".to_string());
        }
        let mut portfolio = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                portfolio = Some(
                    world.combat_research_portfolio_for_run_play_with_progress_v1(
                        options.as_ref(),
                        deadline,
                        progress,
                    )?,
                );
                world.fork_for_hot_run_play_v1()
            })?;
        let portfolio = portfolio
            .ok_or_else(|| "exact run-play combat research produced no portfolio".to_string())?;
        portfolio.attempt.validate()?;
        Ok(portfolio)
    }

    /// Clones the current exact combat position for a trusted process-local
    /// run-play diagnostic. The returned value is never part of a public task
    /// payload or durable controller artifact.
    #[doc(hidden)]
    pub fn current_combat_position_for_run_play_v1(
        &self,
    ) -> Result<crate::sim::combat::CombatPosition, String> {
        let current = self.current_root.as_ref().ok_or_else(|| {
            "exact run-play combat position requires a public decision".to_string()
        })?;
        if current.site != TaskDecisionSiteV1::Combat {
            return Err("exact run-play combat position requires a combat decision".to_string());
        }
        let mut position = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                position = Some(world.current_combat_position_for_run_play_v1()?);
                world.fork_for_hot_run_play_v1()
            })?;
        position.ok_or_else(|| "exact run-play combat position produced no result".to_string())
    }

    /// Converts one cached exact complete-turn proposal into public replay
    /// choices on an isolated fork. The live session remains unchanged until
    /// the run-play controller prepares and publishes those choices.
    #[doc(hidden)]
    pub fn current_combat_turn_plan_for_run_play_v1(
        &self,
        inputs: &[crate::state::core::ClientInput],
        provenance: TaskPrivilegedSupportProvenanceV1,
    ) -> Result<TaskResearchPlanV1, String> {
        let current = self
            .current_root
            .as_ref()
            .ok_or_else(|| "exact run-play combat turn requires a public decision".to_string())?;
        if current.site != TaskDecisionSiteV1::Combat {
            return Err("exact run-play combat turn requires a combat decision".to_string());
        }
        let mut plan = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                plan = Some(
                    world.public_combat_turn_plan_for_run_play_v1(inputs, provenance.clone())?,
                );
                world.fork_for_hot_run_play_v1()
            })?;
        plan.ok_or_else(|| "exact run-play combat turn produced no public plan".to_string())
    }

    /// Projects a terminal exact combat line through mandatory run handoffs on
    /// an isolated fork, leaving the live run-play session unchanged.
    #[doc(hidden)]
    pub fn current_combat_replay_outcome_for_run_play_v1(
        &self,
        inputs: &[crate::state::core::ClientInput],
        provenance: TaskPrivilegedSupportProvenanceV1,
    ) -> Result<crate::agent::task::TaskResearchPlanOutcomeV1, String> {
        let current = self
            .current_root
            .as_ref()
            .ok_or_else(|| "exact run-play combat replay requires a public decision".to_string())?;
        if current.site != TaskDecisionSiteV1::Combat {
            return Err("exact run-play combat replay requires a combat decision".to_string());
        }
        let mut outcome = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                outcome = Some(
                    world
                        .public_combat_replay_outcome_for_run_play_v1(inputs, provenance.clone())?,
                );
                world.fork_for_hot_run_play_v1()
            })?;
        outcome.ok_or_else(|| "exact run-play combat replay produced no outcome".to_string())
    }

    #[doc(hidden)]
    pub fn current_combat_replay_plan_and_outcome_for_run_play_v1(
        &self,
        inputs: &[crate::state::core::ClientInput],
        provenance: TaskPrivilegedSupportProvenanceV1,
    ) -> Result<
        (
            crate::agent::task::TaskResearchPlanV1,
            crate::agent::task::TaskResearchPlanOutcomeV1,
        ),
        String,
    > {
        let current = self
            .current_root
            .as_ref()
            .ok_or_else(|| "exact run-play combat replay requires a public decision".to_string())?;
        if current.site != TaskDecisionSiteV1::Combat {
            return Err("exact run-play combat replay requires a combat decision".to_string());
        }
        let mut projected = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                projected = Some(world.public_combat_replay_plan_and_outcome_for_run_play_v1(
                    inputs,
                    provenance.clone(),
                )?);
                world.fork_for_hot_run_play_v1()
            })?;
        projected.ok_or_else(|| "exact run-play combat replay produced no projection".to_string())
    }

    /// Captures the current exact combat root as a canonical production case
    /// on a private fork, leaving the hot run-play session unchanged.
    #[doc(hidden)]
    pub fn current_combat_case_for_run_play_v1(&self) -> Result<CombatCase, String> {
        let current = self
            .current_root
            .as_ref()
            .ok_or_else(|| "exact run-play combat case requires a public decision".to_string())?;
        if current.site != TaskDecisionSiteV1::Combat {
            return Err("exact run-play combat case requires a combat decision".to_string());
        }
        let mut case = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                case = Some(world.current_combat_case_for_run_play_v1()?);
                world.fork_for_hot_run_play_v1()
            })?;
        case.ok_or_else(|| "exact run-play combat case produced no result".to_string())
    }

    /// Reads the existing typed route-policy audit without exposing the
    /// private exact world. The live environment remains unchanged.
    pub fn current_exact_route_policy_audit_v1(&self) -> Result<ExactRoutePolicyAuditV1, String> {
        let current = self
            .current_root
            .as_ref()
            .ok_or_else(|| "exact run-play route audit requires a public decision".to_string())?;
        if current.site != TaskDecisionSiteV1::Map {
            return Err("exact run-play route audit requires a map decision".to_string());
        }
        let mut audit = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                audit = Some(world.exact_route_policy_audit_for_run_play_v1()?);
                world.fork_for_hot_run_play_v1()
            })?;
        audit.ok_or_else(|| "exact run-play route audit produced no result".to_string())
    }

    /// Reads the value-free public path family below every currently legal map
    /// entry. Unlike the route-policy audit, this is available before a Map
    /// choice exists (notably at Neow) and carries no owner rank or prior.
    pub fn current_public_route_entry_paths_v1(
        &self,
        path_budget: usize,
    ) -> Result<Vec<crate::ai::route_window_facts::RouteEntryPathSummaryV1>, String> {
        self.current_root.as_ref().ok_or_else(|| {
            "exact run-play route-entry paths require a public decision".to_string()
        })?;
        let mut entries = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                entries = Some(world.public_route_entry_paths_for_run_play_v1(path_budget)?);
                world.fork_for_hot_run_play_v1()
            })?;
        entries.ok_or_else(|| "exact run-play route-entry paths produced no result".to_string())
    }

    /// Reads the existing typed card-reward audit without exposing the
    /// private exact world. The live environment remains unchanged.
    pub fn current_exact_card_reward_policy_audit_v1(
        &self,
    ) -> Result<ExactCardRewardPolicyAuditV1, String> {
        let current = self.current_root.as_ref().ok_or_else(|| {
            "exact run-play card-reward audit requires a public decision".to_string()
        })?;
        if current.site != TaskDecisionSiteV1::CardReward {
            return Err(
                "exact run-play card-reward audit requires a card reward decision".to_string(),
            );
        }
        let mut audit = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                audit = Some(world.exact_card_reward_policy_audit_for_run_play_v1()?);
                world.fork_for_hot_run_play_v1()
            })?;
        audit.ok_or_else(|| "exact run-play card-reward audit produced no result".to_string())
    }

    /// Reads the shared direct-card acquisition audit at a compatible Event
    /// surface such as The Library without moving the exact session.
    pub fn current_exact_direct_card_acquisition_policy_audit_v1(
        &self,
    ) -> Result<ExactCardRewardPolicyAuditV1, String> {
        let current = self.current_root.as_ref().ok_or_else(|| {
            "exact run-play direct-card audit requires a public decision".to_string()
        })?;
        if current.site != TaskDecisionSiteV1::Event {
            return Err("exact run-play direct-card audit requires an event decision".to_string());
        }
        let mut audit = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                audit = Some(world.exact_direct_card_acquisition_policy_audit_for_run_play_v1()?);
                world.fork_for_hot_run_play_v1()
            })?;
        audit.ok_or_else(|| "exact run-play direct-card audit produced no result".to_string())
    }

    /// Reads the existing typed campfire audit without exposing the private
    /// exact world. The live environment remains unchanged.
    pub fn current_exact_campfire_policy_audit_v1(
        &self,
    ) -> Result<ExactCampfirePolicyAuditV1, String> {
        let current = self.current_root.as_ref().ok_or_else(|| {
            "exact run-play campfire audit requires a public decision".to_string()
        })?;
        if current.site != TaskDecisionSiteV1::Campfire {
            return Err("exact run-play campfire audit requires a campfire decision".to_string());
        }
        let mut audit = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                audit = Some(world.exact_campfire_policy_audit_for_run_play_v1()?);
                world.fork_for_hot_run_play_v1()
            })?;
        audit.ok_or_else(|| "exact run-play campfire audit produced no result".to_string())
    }

    /// Reads the existing typed boss-relic audit without exposing the private
    /// exact world. The live environment remains unchanged.
    pub fn current_exact_boss_relic_policy_audit_v1(
        &self,
    ) -> Result<ExactBossRelicPolicyAuditV1, String> {
        let current = self.current_root.as_ref().ok_or_else(|| {
            "exact run-play boss-relic audit requires a public decision".to_string()
        })?;
        if current.site != TaskDecisionSiteV1::BossRelic {
            return Err(
                "exact run-play boss-relic audit requires a boss relic decision".to_string(),
            );
        }
        let mut audit = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                audit = Some(world.exact_boss_relic_policy_audit_for_run_play_v1()?);
                world.fork_for_hot_run_play_v1()
            })?;
        audit.ok_or_else(|| "exact run-play boss-relic audit produced no result".to_string())
    }

    /// Reads the existing typed shop audit without exposing the private exact
    /// world. The live environment remains unchanged.
    pub fn current_exact_shop_policy_audit_v1(&self) -> Result<ExactShopPolicyAuditV1, String> {
        let current = self
            .current_root
            .as_ref()
            .ok_or_else(|| "exact run-play shop audit requires a public decision".to_string())?;
        if current.site != TaskDecisionSiteV1::Shop {
            return Err("exact run-play shop audit requires a shop decision".to_string());
        }
        let mut audit = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                audit = Some(world.exact_shop_policy_audit_for_run_play_v1()?);
                world.fork_for_hot_run_play_v1()
            })?;
        audit.ok_or_else(|| "exact run-play shop audit produced no result".to_string())
    }

    /// Reads the typed run-choice owner audit without exposing the private
    /// exact world. The live environment remains unchanged.
    pub fn current_exact_run_choice_policy_audit_v1(
        &self,
    ) -> Result<ExactRunChoicePolicyAuditV1, String> {
        let current = self.current_root.as_ref().ok_or_else(|| {
            "exact run-play run-choice audit requires a public decision".to_string()
        })?;
        if current.site != TaskDecisionSiteV1::RunChoice {
            return Err(
                "exact run-play run-choice audit requires a run choice decision".to_string(),
            );
        }
        let mut audit = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                audit = Some(world.exact_run_choice_policy_audit_for_run_play_v1()?);
                world.fork_for_hot_run_play_v1()
            })?;
        audit.ok_or_else(|| "exact run-play run-choice audit produced no result".to_string())
    }

    /// Reads the typed full-belt potion exchange audit without mutating the
    /// reward screen or exposing private state.
    pub fn current_potion_reward_policy_audit_v1(
        &self,
    ) -> Result<Option<PotionRewardPolicyAuditV1>, String> {
        let current = self.current_root.as_ref().ok_or_else(|| {
            "exact run-play potion-reward audit requires a public decision".to_string()
        })?;
        if current.site != TaskDecisionSiteV1::Reward {
            return Err(
                "exact run-play potion-reward audit requires a reward decision".to_string(),
            );
        }
        let mut audit = None;
        let _fork = self
            .environment
            .fork_private_research_execution_v1(|world| {
                audit = Some(world.potion_reward_policy_audit_for_run_play_v1()?);
                world.fork_for_hot_run_play_v1()
            })?;
        audit.ok_or_else(|| "exact run-play potion-reward audit produced no result".to_string())
    }

    /// Captures the current private execution state for bounded, process-local
    /// interactive navigation. Durable recovery still comes from the archive
    /// and checkpoint artifact after a process restart.
    pub fn hot_snapshot_v1(&self) -> Result<ExactRunPlayHotSnapshotV1, String> {
        let archive_node_id = self
            .current_archive_node_id
            .clone()
            .ok_or_else(|| "exact run-play hot snapshot requires a public decision".to_string())?;
        let public_root = self
            .current_root
            .clone()
            .ok_or_else(|| "exact run-play hot snapshot has no public root".to_string())?;
        Ok(ExactRunPlayHotSnapshotV1 {
            environment: self
                .environment
                .fork_private_research_execution_v1(|world| world.fork_for_hot_run_play_v1())?,
            archive_node_id,
            public_root,
        })
    }

    /// Restores one process-local decision anchor while preserving the latest
    /// append-only archive and checkpoint set accumulated by this session.
    pub fn restore_hot_snapshot_v1(
        &mut self,
        snapshot: &ExactRunPlayHotSnapshotV1,
    ) -> Result<(), String> {
        let restored = snapshot.environment.actor_view();
        let TaskActorBoundaryV1::Decision { decision } = &restored.boundary else {
            return Err("exact run-play hot snapshot is not at a public decision".to_string());
        };
        if !snapshot.public_root.replay_compatible_v2(decision) {
            return Err("exact run-play hot snapshot changed its public decision".to_string());
        }
        let inventory = task_run_branch_decision_in_validated_session_v1(
            &self.archive,
            &self.checkpoints,
            &snapshot.archive_node_id,
        )?;
        if !inventory
            .public_decision_v1()?
            .replay_compatible_v2(&snapshot.public_root)
        {
            return Err(
                "exact run-play hot snapshot disagrees with the retained archive node".to_string(),
            );
        }
        self.environment = snapshot
            .environment
            .fork_private_research_execution_v1(|world| world.fork_for_hot_run_play_v1())?;
        self.current_archive_node_id = Some(snapshot.archive_node_id.clone());
        self.current_root = Some(snapshot.public_root.clone());
        Ok(())
    }

    /// Rebases the selected exact prefix at the current public decision and
    /// prepares the repository's existing typed branch checkpoint.  The
    /// operation runs on a private fork so a failed commit, identity check, or
    /// durable publication cannot mutate the live run-play world.
    pub fn prepare_current_checkpoint_v1(
        &self,
    ) -> Result<ExactRunPlayPreparedCheckpointV1, String> {
        let archive_node_id = self
            .current_archive_node_id
            .clone()
            .ok_or_else(|| "exact run-play checkpoint requires a public decision".to_string())?;
        let public_root = self
            .current_root
            .clone()
            .ok_or_else(|| "exact run-play checkpoint has no public root".to_string())?;
        if self
            .checkpoints
            .identities()
            .any(|identity| identity.archive_node_id == archive_node_id)
        {
            return Err("exact run-play decision already has a durable checkpoint".to_string());
        }
        let mut environment = self
            .environment
            .fork_private_research_execution_v1(|world| world.fork_for_hot_run_play_v1())?;
        let committed = environment
            .commit_current_attempt()
            .map_err(|error| format!("exact run-play checkpoint commit failed: {error}"))?;
        let TaskActorBoundaryV1::Decision { decision } = &committed.boundary else {
            return Err(
                "exact run-play checkpoint commit did not produce a public decision".to_string(),
            );
        };
        if !public_root.replay_compatible_v2(decision) {
            return Err("exact run-play checkpoint commit changed its public decision".to_string());
        }
        let point = environment
            .capture_research_run_branch_point_v1()?
            .map_anchor(|anchor| anchor.durable_checkpoint_v1());
        let checkpoints = self.checkpoints.clone().merged_with_branch_points(
            &self.archive,
            self.seed,
            [point],
        )?;
        if !checkpoints
            .identities()
            .any(|identity| identity.archive_node_id == archive_node_id)
        {
            return Err(
                "exact run-play checkpoint did not bind the current archive node".to_string(),
            );
        }
        let current_decision = task_run_branch_decision_in_validated_session_v1(
            &self.archive,
            &checkpoints,
            &archive_node_id,
        )?;
        Ok(ExactRunPlayPreparedCheckpointV1 {
            environment,
            checkpoints,
            current_decision,
            archive_node_id,
            public_root,
        })
    }

    /// Installs a recovery point only after the controller has durably
    /// published its checkpoint artifact.  Preparation already validated the
    /// public and archive joins, so installation cannot fail halfway through.
    pub fn install_prepared_checkpoint_v1(&mut self, prepared: ExactRunPlayPreparedCheckpointV1) {
        self.environment = prepared.environment;
        self.checkpoints = prepared.checkpoints;
        self.current_archive_node_id = Some(prepared.archive_node_id);
        self.current_root = Some(prepared.public_root);
    }

    /// Executes a complete public line in a private session, records the same
    /// typed archive evidence as ordinary exact choices, and guarantees a
    /// replay-free durable checkpoint at the final public decision, or a
    /// replay-observed committed game terminal. Any invalid choice, premature
    /// terminal, or checkpoint failure drops the private candidate and leaves
    /// `self` untouched.
    pub fn prepare_line_v1(
        &self,
        choices: &[TaskPublicChoiceV1],
    ) -> Result<ExactRunPlayPreparedLineV1, String> {
        if choices.is_empty() {
            return Err("exact run-play prepared line must not be empty".to_string());
        }
        self.prepare_line_resolving_v1(choices.len(), &mut |index, _decision| {
            Ok(choices[index].clone())
        })
    }

    /// Executes a bounded line while resolving each requested action against
    /// the public decision produced by the preceding action. This is the
    /// semantic-action entry point: callers must not pre-resolve a dynamic
    /// combat line into ordinals from its initial root.
    pub fn prepare_line_resolving_v1(
        &self,
        step_count: usize,
        resolve: &mut dyn FnMut(usize, &TaskPublicDecisionV1) -> Result<TaskPublicChoiceV1, String>,
    ) -> Result<ExactRunPlayPreparedLineV1, String> {
        if step_count == 0 {
            return Err("exact run-play prepared line must not be empty".to_string());
        }
        let total = Instant::now();
        let mut profile = ExactRunPlayPreparedLineProfileV1::default();
        let from_archive_node_id = self
            .current_archive_node_id
            .clone()
            .ok_or_else(|| "exact run-play prepared line requires a public decision".to_string())?;
        let archive_nodes_before = self.archive.nodes.len();
        let archive_attempts_before = self.archive.attempts.len();
        let checkpoints_before = self.checkpoints.checkpoint_count();
        let phase = Instant::now();
        let mut candidate = Self {
            seed: self.seed,
            requested_budget: self.requested_budget,
            environment: self
                .environment
                .fork_private_research_execution_v1(|world| world.fork_for_hot_run_play_v1())?,
            archive: self.archive.clone(),
            checkpoints: self.checkpoints.clone(),
            current_archive_node_id: self.current_archive_node_id.clone(),
            current_root: self.current_root.clone(),
        };
        profile.initial_fork = phase.elapsed();
        let mut choices = Vec::with_capacity(step_count);
        let mut final_episode = None;
        for index in 0..step_count {
            let phase = Instant::now();
            let choice = resolve(
                index,
                candidate.current_root.as_ref().ok_or_else(|| {
                    format!(
                        "exact run-play prepared line has no public decision at step {}",
                        index + 1
                    )
                })?,
            )
            .map_err(|error| {
                format!(
                    "exact run-play prepared line could not resolve step {}: {error}",
                    index + 1
                )
            })?;
            profile.selector_resolution += phase.elapsed();
            let step_total = Instant::now();
            let phase = Instant::now();
            let current_root = candidate.current_root.clone().ok_or_else(|| {
                format!(
                    "exact run-play prepared line has no public decision at step {}",
                    index + 1
                )
            })?;
            current_root.validate_choice(&choice).map_err(|error| {
                format!(
                    "exact run-play prepared line step {} failed: {error}",
                    index + 1
                )
            })?;
            let mut actor = RecoveredPrefixPrivilegedActorV2::new(
                Vec::new(),
                current_root.clone(),
                Some(choice.clone()),
                RunContinuationControlV1::YieldAtPublicDecision,
            );
            profile.step_setup += phase.elapsed();
            let phase = Instant::now();
            let episode = run_task_privileged_support_actor_episode_v1(
                &mut candidate.environment,
                &mut actor,
            )
            .map_err(|error| {
                format!(
                    "exact run-play prepared line step {} failed: {error}",
                    index + 1
                )
            })?;
            profile.actor_episode += phase.elapsed();
            let phase = Instant::now();
            actor.require_completed_recovery()?;
            require_forced_choice_observed_v1(&episode, 0, &current_root, &choice)?;
            profile.step_validation += phase.elapsed();
            let phase = Instant::now();
            candidate.current_root = match (&episode.outcome, &episode.final_view.boundary) {
                (
                    TaskEpisodeOutcomeV1::Paused { .. },
                    TaskActorBoundaryV1::Decision { decision },
                ) => Some(decision.clone()),
                (TaskEpisodeOutcomeV1::Paused { .. }, _) => {
                    return Err(format!(
                        "exact run-play prepared line step {} paused without a public decision",
                        index + 1
                    ));
                }
                _ => None,
            };
            profile.endpoint_projection += phase.elapsed();
            profile.step_total += step_total.elapsed();
            final_episode = Some(episode);
            choices.push(choice);
            if index + 1 < step_count && candidate.current_root.is_none() {
                return Err(format!(
                    "exact run-play prepared line ended after step {} before all {} choices were applied during private preparation; the entire line was rejected and no actions were committed",
                    index + 1,
                    step_count
                ));
            }
        }
        let final_episode = final_episode
            .ok_or_else(|| "exact run-play prepared line has no final episode".to_string())?;
        let execution_manifest_id = stable_planner_id(
            "hot_exact_run_play_line_manifest_v1",
            &(
                &final_episode.actor,
                &candidate.archive.task,
                candidate.seed,
                from_archive_node_id.as_str(),
                &choices,
                candidate.requested_budget,
            ),
        )?;
        let phase = Instant::now();
        let final_attempt_id = candidate.archive.record_episode(
            candidate.seed,
            execution_manifest_id,
            candidate.requested_budget,
            &final_episode,
        )?;
        profile.archive_record += phase.elapsed();
        let phase = Instant::now();
        let to_archive_node_id = match &final_episode.outcome {
            TaskEpisodeOutcomeV1::Paused { .. } => task_run_paused_decision_node_id_v1(
                &candidate.archive,
                &final_attempt_id,
                &final_episode,
            )?,
            TaskEpisodeOutcomeV1::Success { .. }
            | TaskEpisodeOutcomeV1::Failure {
                game_outcome: Some(_),
                ..
            } => candidate
                .archive
                .attempts
                .iter()
                .find(|attempt| attempt.attempt_id == final_attempt_id)
                .and_then(|attempt| attempt.node_path.last())
                .cloned()
                .ok_or_else(|| {
                    "exact run-play prepared line terminal attempt has no archive leaf".to_string()
                })?,
            TaskEpisodeOutcomeV1::Failure { .. } => {
                return Err(
                    "exact run-play prepared line failed without an archive terminal".to_string(),
                );
            }
        };
        candidate.current_archive_node_id = Some(to_archive_node_id.clone());
        profile.endpoint_projection += phase.elapsed();
        let (current_decision, terminal_outcome) = if candidate.current_root.is_some() {
            if !matches!(final_episode.outcome, TaskEpisodeOutcomeV1::Paused { .. }) {
                return Err(
                    "exact run-play prepared line has a public decision after a terminal outcome"
                        .to_string(),
                );
            }
            if !candidate
                .checkpoints
                .identities()
                .any(|identity| identity.archive_node_id == to_archive_node_id)
            {
                let phase = Instant::now();
                let checkpoint = candidate.prepare_current_checkpoint_v1()?;
                candidate.install_prepared_checkpoint_v1(checkpoint);
                profile.final_checkpoint += phase.elapsed();
            }
            let current_decision = task_run_branch_decision_in_validated_session_v1(
                &candidate.archive,
                &candidate.checkpoints,
                &to_archive_node_id,
            )?;
            if current_decision.replayed_public_step_count != 0 {
                return Err(
                    "exact run-play prepared line final checkpoint still requires replay"
                        .to_string(),
                );
            }
            (Some(current_decision), None)
        } else {
            let episode_game_outcome = match &final_episode.outcome {
                TaskEpisodeOutcomeV1::Success { game_outcome }
                | TaskEpisodeOutcomeV1::Failure {
                    game_outcome: Some(game_outcome),
                    ..
                } => game_outcome,
                TaskEpisodeOutcomeV1::Paused { .. } => {
                    return Err(
                        "exact run-play prepared line paused without a public decision".to_string(),
                    );
                }
                TaskEpisodeOutcomeV1::Failure {
                    game_outcome: None, ..
                } => {
                    return Err(
                        "exact run-play prepared line failed without an archive terminal"
                            .to_string(),
                    );
                }
            };
            let committed_game_outcome = match final_episode
                .final_view
                .committed_history
                .completed_scopes
                .last()
                .map(|scope| &scope.exit)
            {
                Some(TaskCommittedScopeExitV1::GameTerminal { game_outcome }) => game_outcome,
                _ => {
                    return Err(
                        "exact run-play prepared line terminal outcome lacks a committed terminal scope"
                            .to_string(),
                    );
                }
            };
            if committed_game_outcome != episode_game_outcome {
                return Err(
                    "exact run-play prepared line episode and committed terminal outcomes disagree"
                        .to_string(),
                );
            }
            let final_attempt = candidate
                .archive
                .attempts
                .iter()
                .find(|attempt| attempt.attempt_id == final_attempt_id)
                .ok_or_else(|| {
                    "exact run-play prepared line final archive attempt is missing".to_string()
                })?;
            if final_attempt.outcome != final_episode.outcome
                || final_attempt.node_path.last() != Some(&to_archive_node_id)
            {
                return Err(
                    "exact run-play prepared line episode and archive attempt disagree".to_string(),
                );
            }
            (None, Some(final_episode.outcome))
        };
        profile.total = total.elapsed();
        Ok(ExactRunPlayPreparedLineV1 {
            session: candidate,
            current_decision,
            terminal_outcome,
            from_archive_node_id,
            to_archive_node_id,
            choices,
            archive_nodes_before,
            archive_attempts_before,
            checkpoints_before,
            profile,
        })
    }

    /// Installs an already-published prepared line without another replay or
    /// fallible archive projection.
    pub fn install_prepared_line_v1(&mut self, prepared: ExactRunPlayPreparedLineV1) {
        *self = prepared.session;
    }

    pub fn choose(
        &mut self,
        choice: TaskPublicChoiceV1,
        advance: ExactRunPlayAdvanceV1,
    ) -> Result<ExactRunPlayStepV1, String> {
        // Ordinary interactive choices remain transactional. The isolated
        // candidate may mutate in place because it is dropped on any error;
        // only a fully validated step replaces the live session.
        let mut candidate = Self {
            seed: self.seed,
            requested_budget: self.requested_budget,
            environment: self
                .environment
                .fork_private_research_execution_v1(|world| world.fork_for_hot_run_play_v1())?,
            archive: self.archive.clone(),
            checkpoints: self.checkpoints.clone(),
            current_archive_node_id: self.current_archive_node_id.clone(),
            current_root: self.current_root.clone(),
        };
        let step = candidate.choose_in_place_private_v1(choice, advance)?;
        *self = candidate;
        Ok(step)
    }

    /// Advances an already-isolated exact session without cloning its growing
    /// archive again. Callers must own the surrounding transaction and drop
    /// this session if any later check fails.
    fn choose_in_place_private_v1(
        &mut self,
        choice: TaskPublicChoiceV1,
        advance: ExactRunPlayAdvanceV1,
    ) -> Result<ExactRunPlayStepV1, String> {
        let total = Instant::now();
        let setup_phase = Instant::now();
        let current_node_id = self
            .current_archive_node_id
            .clone()
            .ok_or_else(|| "exact run-play session is not at a public decision".to_string())?;
        let current_root = self
            .current_root
            .clone()
            .ok_or_else(|| "exact run-play session has no current public root".to_string())?;
        current_root.validate_choice(&choice)?;
        let control = match advance {
            ExactRunPlayAdvanceV1::NextPublicDecision => {
                RunContinuationControlV1::YieldAtPublicDecision
            }
            ExactRunPlayAdvanceV1::NextStrategicDecision => {
                RunContinuationControlV1::YieldAtStrategicDecision
            }
        };
        let actor_identity = continuation_actor_identity_v1(control);
        let execution_manifest_id = stable_planner_id(
            "hot_exact_run_play_step_manifest_v1",
            &(
                &actor_identity,
                &self.archive.task,
                self.seed,
                current_node_id.as_str(),
                &choice,
                advance,
                self.requested_budget,
            ),
        )?;
        let mut actor = RecoveredPrefixPrivilegedActorV2::new(
            Vec::new(),
            current_root.clone(),
            Some(choice.clone()),
            control,
        );
        let setup = setup_phase.elapsed();
        let phase = Instant::now();
        let (episode, branch_points) =
            run_task_privileged_support_actor_episode_capturing_fresh_scopes_v1(
                &mut self.environment,
                &mut actor,
                |point| Ok::<_, String>(point.map_anchor(|anchor| anchor.durable_checkpoint_v1())),
            )
            .map_err(|error| error.to_string())?;
        let actor_episode = phase.elapsed();
        let phase = Instant::now();
        actor.require_completed_recovery()?;
        require_forced_choice_observed_v1(&episode, 0, &current_root, &choice)?;
        let validation = phase.elapsed();

        let previous_nodes = self.archive.nodes.len();
        let previous_attempts = self.archive.attempts.len();
        let previous_checkpoints = self.checkpoints.checkpoint_count();
        let phase = Instant::now();
        let attempt_id = self.archive.record_episode(
            self.seed,
            execution_manifest_id,
            self.requested_budget,
            &episode,
        )?;
        let archive_record = phase.elapsed();
        let phase = Instant::now();
        if !branch_points.is_empty() {
            self.checkpoints = self.checkpoints.clone().merged_with_branch_points(
                &self.archive,
                self.seed,
                branch_points,
            )?;
        }
        let checkpoint_merge = phase.elapsed();
        let phase = Instant::now();
        let current_archive_node_id = match &episode.outcome {
            TaskEpisodeOutcomeV1::Paused { .. } => Some(task_run_paused_decision_node_id_v1(
                &self.archive,
                &attempt_id,
                &episode,
            )?),
            TaskEpisodeOutcomeV1::Success { .. }
            | TaskEpisodeOutcomeV1::Failure {
                game_outcome: Some(_),
                ..
            } => self
                .archive
                .attempts
                .iter()
                .find(|attempt| attempt.attempt_id == attempt_id)
                .and_then(|attempt| attempt.node_path.last())
                .cloned(),
            TaskEpisodeOutcomeV1::Failure { .. } => None,
        };
        let current_root = match &episode.final_view.boundary {
            TaskActorBoundaryV1::Decision { decision }
                if matches!(episode.outcome, TaskEpisodeOutcomeV1::Paused { .. }) =>
            {
                Some(decision.clone())
            }
            _ => None,
        };
        self.current_archive_node_id = current_archive_node_id.clone();
        self.current_root = current_root;
        let endpoint_projection = phase.elapsed();
        Ok(ExactRunPlayStepV1 {
            attempt_id,
            episode,
            current_archive_node_id,
            archive_nodes_added: self.archive.nodes.len().saturating_sub(previous_nodes),
            archive_attempts_added: self
                .archive
                .attempts
                .len()
                .saturating_sub(previous_attempts),
            checkpoints_added: self
                .checkpoints
                .checkpoint_count()
                .saturating_sub(previous_checkpoints),
            profile: ExactRunPlayStepProfileV1 {
                total: total.elapsed(),
                setup,
                actor_episode,
                validation,
                archive_record,
                checkpoint_merge,
                endpoint_projection,
            },
        })
    }

    /// Executes a caller-supplied public line on one private in-memory fork and
    /// returns its typed public successors without changing this session's
    /// exact world, archive, checkpoints, or durable controller head.
    pub fn probe_line_v1(
        &self,
        choices: &[TaskPublicChoiceV1],
    ) -> Result<ExactRunPlayProbeLineV1, String> {
        if choices.is_empty() {
            return Err("exact run-play probe line must not be empty".to_string());
        }
        self.probe_line_resolving_v1(choices.len(), &mut |index, _decision| {
            Ok(choices[index].clone())
        })
    }

    /// Probes one private exact fork while resolving every action against the
    /// latest public root. This keeps semantic action identities stable even
    /// when the candidate surface is re-numbered after each card play.
    pub fn probe_line_resolving_v1(
        &self,
        step_count: usize,
        resolve: &mut dyn FnMut(usize, &TaskPublicDecisionV1) -> Result<TaskPublicChoiceV1, String>,
    ) -> Result<ExactRunPlayProbeLineV1, String> {
        let inspection = self.inspect_line_resolving_v1(step_count, resolve)?;
        match inspection.stop {
            ExactRunPlayInspectionStopV1::Unresolved { requested_step, error, .. } => Err(format!(
                "exact run-play probe could not resolve step {requested_step}: {error}"
            )),
            ExactRunPlayInspectionStopV1::Terminal { unapplied_steps } if unapplied_steps > 0 => Err(format!(
                "exact run-play probe line ended after step {} before all {step_count} choices were applied",
                inspection.line.steps.len(),
            )),
            _ => Ok(inspection.line),
        }
    }

    pub fn inspect_line_resolving_v1(
        &self,
        step_count: usize,
        resolve: &mut dyn FnMut(usize, &TaskPublicDecisionV1) -> Result<TaskPublicChoiceV1, String>,
    ) -> Result<ExactRunPlayInspectionV1, String> {
        if step_count == 0 {
            return Err("exact run-play probe line must not be empty".to_string());
        }
        let total = Instant::now();
        let phase = Instant::now();
        let mut environment = self
            .environment
            .fork_private_research_execution_v1(|world| world.fork_for_hot_run_play_v1())?;
        let fork_elapsed = phase.elapsed();
        let mut current_root = self
            .current_root
            .clone()
            .ok_or_else(|| "exact run-play probe has no current public root".to_string())?;
        let mut steps = Vec::with_capacity(step_count);
        let mut stop = ExactRunPlayInspectionStopV1::Complete;
        for index in 0..step_count {
            let choice = match resolve(index, &current_root) {
                Ok(choice) => choice,
                Err(error) => {
                    stop = ExactRunPlayInspectionStopV1::Unresolved {
                        requested_step: index + 1,
                        error,
                        decision: Box::new(current_root.clone()),
                    };
                    break;
                }
            };
            current_root.validate_choice(&choice)?;
            let selected_candidate = current_root.candidates[choice.ordinal_path[0]].clone();
            let source_decision = current_root.clone();
            let monster_entities_before =
                environment.inspect_private_research_execution_v1(|world| {
                    Ok(world.combat_monster_entities_for_run_play_v1())
                })?;
            let mut actor = RecoveredPrefixPrivilegedActorV2::new(
                Vec::new(),
                current_root.clone(),
                Some(choice.clone()),
                RunContinuationControlV1::YieldAtPublicDecision,
            );
            let phase = Instant::now();
            let episode =
                run_task_privileged_support_actor_episode_v1(&mut environment, &mut actor)
                    .map_err(|error| error.to_string())?;
            actor.require_completed_recovery()?;
            require_forced_choice_observed_v1(&episode, 0, &current_root, &choice)?;
            let (action_result, monster_entities_after) = environment
                .inspect_private_research_execution_v1(|world| {
                    Ok((
                        world.last_action_result_for_run_play_v1(),
                        world.combat_monster_entities_for_run_play_v1(),
                    ))
                })?;
            let next_decision = match (&episode.outcome, &episode.final_view.boundary) {
                (
                    TaskEpisodeOutcomeV1::Paused { .. },
                    TaskActorBoundaryV1::Decision { decision },
                ) => Some(decision.clone()),
                (TaskEpisodeOutcomeV1::Paused { .. }, _) => {
                    return Err(format!(
                        "exact run-play probe step {} paused without a public decision",
                        index + 1
                    ));
                }
                _ => None,
            };
            let outcome = episode.outcome.clone();
            let elapsed = phase.elapsed();
            steps.push(ExactRunPlayProbeStepV1 {
                source_decision,
                monster_entities_before,
                monster_entities_after,
                choice,
                selected_candidate,
                action_result,
                outcome,
                next_decision: next_decision.clone(),
                elapsed,
            });
            match next_decision {
                Some(decision) => current_root = decision,
                None => {
                    stop = ExactRunPlayInspectionStopV1::Terminal {
                        unapplied_steps: step_count - index - 1,
                    };
                    break;
                }
            }
        }
        Ok(ExactRunPlayInspectionV1 {
            line: ExactRunPlayProbeLineV1 {
                steps,
                fork_elapsed,
                total_elapsed: total.elapsed(),
            },
            requested_steps: step_count,
            stop,
        })
    }
}

/// Restores the nearest durable fresh-scope ancestor, exact-replays the
/// archived public prefix to one caller-selected decision, executes one public
/// deviation, and lets the real privileged reference actor finish the task.
///
/// The supplied choice must be legal at the recovered target root and must not
/// equal any already archived immediate child. The input archive and sidecar
/// are borrowed; successful execution returns new merged values so callers can
/// publish to fresh paths without overwriting their source evidence.
pub fn resume_privileged_reference_run_continuation_v2(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
    target_archive_node_id: &str,
    forced_choice: TaskPublicChoiceV1,
    continuation_budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
) -> Result<PrivilegedReferenceRunContinuationV2, String> {
    resume_privileged_reference_run_continuation_with_control_v1(
        archive,
        checkpoints,
        target_archive_node_id,
        forced_choice,
        continuation_budget,
        plan_options,
        evidence_request,
        RunContinuationControlV1::Complete,
    )
}

/// Executes one caller-selected public choice, permits the privileged
/// reference actor to finish any combat that follows, and yields before the
/// first subsequent strategic decision. No strategic fallback is allowed to
/// choose on behalf of the controller.
pub fn resume_run_until_next_strategic_decision_v1(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
    target_archive_node_id: &str,
    forced_choice: TaskPublicChoiceV1,
    continuation_budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
) -> Result<PrivilegedReferenceRunContinuationV2, String> {
    resume_privileged_reference_run_continuation_with_control_v1(
        archive,
        checkpoints,
        target_archive_node_id,
        forced_choice,
        continuation_budget,
        plan_options,
        evidence_request,
        RunContinuationControlV1::YieldAtStrategicDecision,
    )
}

/// Executes exactly one caller-selected public choice and yields at the next
/// public decision, including an atomic combat decision. This is the manual
/// full-run play primitive; it never asks a continuation policy to choose the
/// following action.
pub fn resume_run_until_next_public_decision_v1(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
    target_archive_node_id: &str,
    forced_choice: TaskPublicChoiceV1,
    continuation_budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
) -> Result<PrivilegedReferenceRunContinuationV2, String> {
    resume_privileged_reference_run_continuation_with_control_v1(
        archive,
        checkpoints,
        target_archive_node_id,
        forced_choice,
        continuation_budget,
        plan_options,
        evidence_request,
        RunContinuationControlV1::YieldAtPublicDecision,
    )
}

fn resume_privileged_reference_run_continuation_with_control_v1(
    archive: &TaskRunSolutionArchiveV1,
    checkpoints: &TaskRunBranchCheckpointArtifactV1,
    target_archive_node_id: &str,
    forced_choice: TaskPublicChoiceV1,
    continuation_budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
    control: RunContinuationControlV1,
) -> Result<PrivilegedReferenceRunContinuationV2, String> {
    continuation_budget.validate()?;
    plan_options.validate()?;
    let seed = checkpoints.seed();
    let recovery =
        recoverable_run_branch_decision_v1(archive, checkpoints, target_archive_node_id)?;
    let archive_node = archive
        .node(target_archive_node_id)
        .expect("recovery target resolved from the archive");
    if archive_node.seed != seed {
        return Err(format!(
            "run continuation archive node seed {} disagrees with checkpoint seed {seed}",
            archive_node.seed
        ));
    }

    let mut environment = ironclad_privileged_reference_environment_v1(
        seed,
        archive.task.ascension_level,
        continuation_budget,
        plan_options.clone(),
        evidence_request,
    )?;
    let restored = checkpoints.restore_into_environment(
        archive,
        &recovery.recovery_checkpoint_node_id,
        &mut environment,
        continuation_budget,
    )?;
    let TaskActorBoundaryV1::Decision { decision } = &restored.boundary else {
        return Err("run continuation checkpoint did not restore a public decision".to_string());
    };
    let restored_prefix_steps = restored.committed_history.steps.len();
    if !replay_join_compatible_v2(&recovery.target_root, decision)
        && recovery.replay_steps.is_empty()
    {
        return Err("run continuation checkpoint root disagrees with its target".to_string());
    }
    recovery.target_root.validate_choice(&forced_choice)?;
    require_unobserved_immediate_choice_v1(
        archive,
        target_archive_node_id,
        &recovery.target_root,
        &forced_choice,
    )?;
    let forced_deviation = TaskRunForcedPublicDeviationV2 {
        recovery_checkpoint_node_id: recovery.recovery_checkpoint_node_id.clone(),
        target_archive_node_id: target_archive_node_id.to_string(),
        replayed_public_step_count: recovery.replay_steps.len(),
        target_public_snapshot_id: recovery.target_root.information.snapshot_id.clone(),
        choice: forced_choice.clone(),
    };
    let actor_identity = continuation_actor_identity_v1(control);
    let execution_manifest_id = stable_planner_id(
        "forced_privileged_run_continuation_manifest_v2",
        &(
            &actor_identity,
            &archive.task,
            seed,
            &forced_deviation,
            continuation_budget,
            &plan_options,
        ),
    )?;

    let mut actor = RecoveredPrefixPrivilegedActorV2::new(
        recovery.replay_steps,
        recovery.target_root.clone(),
        Some(forced_choice),
        control,
    );
    let (episode, branch_points) =
        run_task_privileged_support_actor_episode_capturing_fresh_scopes_v1(
            &mut environment,
            &mut actor,
            |point| Ok::<_, String>(point.map_anchor(|anchor| anchor.durable_checkpoint_v1())),
        )
        .map_err(|error| error.to_string())?;
    actor.require_completed_recovery()?;
    require_forced_choice_observed_v1(
        &episode,
        restored_prefix_steps,
        &recovery.target_root,
        &forced_deviation.choice,
    )?;

    let mut merged_archive = archive.clone();
    let attempt_id = merged_archive.record_episode(
        seed,
        execution_manifest_id.clone(),
        continuation_budget,
        &episode,
    )?;
    let merged_checkpoints =
        checkpoints
            .clone()
            .merged_with_branch_points(&merged_archive, seed, branch_points)?;
    let execution = environment
        .take_execution_evidence()
        .materialize_committed_combats(&episode.final_view.committed_history)?;

    Ok(PrivilegedReferenceRunContinuationV2 {
        seed,
        forced_deviation,
        execution_manifest_id,
        attempt_id,
        strategic_support: actor.continuation.strategic_support().to_vec(),
        combat_research_plans_installed: actor.continuation.installed_plans(),
        combat_research_plans_unavailable: actor.continuation.unavailable_plans(),
        combat_research_plan_actions: actor.continuation.planned_actions(),
        combat_research_plan_attempts: actor.continuation.attempts().to_vec(),
        episode,
        execution,
        archive: merged_archive,
        checkpoints: merged_checkpoints,
    })
}

fn require_unobserved_immediate_choice_v1(
    archive: &TaskRunSolutionArchiveV1,
    archive_node_id: &str,
    root: &TaskPublicDecisionV1,
    choice: &TaskPublicChoiceV1,
) -> Result<(), String> {
    for child in archive
        .nodes
        .iter()
        .filter(|node| node.parent_node_id.as_deref() == Some(archive_node_id))
    {
        let incoming = child.incoming_step.as_ref().ok_or_else(|| {
            format!(
                "run archive child {} has no incoming public step",
                child.node_id
            )
        })?;
        if !replay_join_compatible_v2(root, &incoming.step.source) {
            return Err(format!(
                "run archive child {} disagrees with restored public root",
                child.node_id
            ));
        }
        if incoming.step.choice == *choice {
            return Err(format!(
                "run continuation choice is already observed below archive node {archive_node_id}"
            ));
        }
    }
    Ok(())
}

fn require_forced_choice_observed_v1(
    episode: &TaskActorEpisodeV1,
    restored_prefix_steps: usize,
    root: &TaskPublicDecisionV1,
    choice: &TaskPublicChoiceV1,
) -> Result<(), String> {
    let matches = |step: &crate::agent::task::TaskPublicStepV1| {
        step.source.replay_compatible_v2(root) && step.choice == *choice
    };
    if episode
        .final_view
        .committed_history
        .steps
        .iter()
        .skip(restored_prefix_steps)
        .any(|step| matches(&step.step))
        || episode
            .final_view
            .node_memory
            .iter()
            .flat_map(|memory| &memory.attempts)
            .flat_map(|attempt| &attempt.steps)
            .any(matches)
    {
        return Ok(());
    }
    let observed_steps = episode
        .final_view
        .committed_history
        .steps
        .iter()
        .skip(restored_prefix_steps)
        .filter(|step| step.step.source.replay_compatible_v2(root))
        .map(|step| {
            (
                step.step.source.site,
                step.step.choice.ordinal_path.clone(),
                step.step.source.replay_compatible_v2(root),
            )
        })
        .chain(
            episode
                .final_view
                .node_memory
                .iter()
                .flat_map(|memory| &memory.attempts)
                .flat_map(|attempt| &attempt.steps)
                .filter(|step| step.source.replay_compatible_v2(root))
                .map(|step| {
                    (
                        step.source.site,
                        step.choice.ordinal_path.clone(),
                        step.source.replay_compatible_v2(root),
                    )
                }),
        )
        .take(8)
        .collect::<Vec<_>>();
    Err(format!(
        "forced run continuation choice was not executed at its restored root; outcome={:?}; observed_steps={observed_steps:?}",
        episode.outcome
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::task::{
        run_task_privileged_support_actor_episode_v1, TaskActorViewV1, TaskEpisodeOutcomeV1,
    };

    struct UnsupportedPrivilegedActor;

    impl TaskActorV1 for UnsupportedPrivilegedActor {
        fn identity(&self) -> TaskActorIdentityV1 {
            TaskActorIdentityV1::privileged_support_assisted("continuation-fixture", 1)
        }

        fn next_intent(&mut self, _view: &TaskActorViewV1) -> TaskActorIntentV1 {
            TaskActorIntentV1::UnsupportedDecisionSurface {
                reason: "capture root only".to_string(),
            }
        }
    }

    impl TaskPrivilegedSupportActorV1 for UnsupportedPrivilegedActor {
        fn needs_privileged_support(&mut self, _view: &TaskActorViewV1) -> bool {
            false
        }

        fn receive_privileged_support(
            &mut self,
            _view: &TaskActorViewV1,
            _support: TaskPrivilegedSupportV1,
        ) -> Result<(), String> {
            Err("fixture never requests privileged support".to_string())
        }
    }

    fn root_fixture(
        seed: u64,
    ) -> (
        TaskRunSolutionArchiveV1,
        TaskRunBranchCheckpointArtifactV1,
        CombatResearchPlanOptionsV1,
    ) {
        let budget = TaskBudgetV1 {
            max_world_transitions: 1,
            max_reloads: 0,
        };
        let plan_options = CombatResearchPlanOptionsV1::default();
        let mut environment = ironclad_privileged_reference_environment_v1(
            seed,
            0,
            budget,
            plan_options.clone(),
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("privileged exact environment");
        let point = environment
            .capture_research_run_branch_point_v1()
            .expect("fresh root")
            .map_anchor(|anchor| anchor.durable_checkpoint_v1());
        let episode = run_task_privileged_support_actor_episode_v1(
            &mut environment,
            &mut UnsupportedPrivilegedActor,
        )
        .expect("root-only episode");
        let mut archive =
            TaskRunSolutionArchiveV1::new(episode.final_view.task.clone()).expect("run archive");
        archive
            .record_episode(seed, "continuation-fixture", budget, &episode)
            .expect("record fixture");
        let checkpoints =
            TaskRunBranchCheckpointArtifactV1::from_branch_points(&archive, seed, [point])
                .expect("checkpoint artifact");
        (archive, checkpoints, plan_options)
    }

    #[test]
    fn resumed_continuation_records_one_new_public_branch_as_unknown() {
        let (archive, checkpoints, plan_options) = root_fixture(0);
        let root_node_id = archive.nodes[0].node_id.clone();
        let result = resume_privileged_reference_run_continuation_v2(
            &archive,
            &checkpoints,
            &root_node_id,
            TaskPublicChoiceV1::one(0),
            TaskBudgetV1 {
                max_world_transitions: 12,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("resume one public deviation");

        assert_eq!(result.archive.attempts.len(), 2);
        assert!(result.archive.nodes.len() > 1);
        assert_eq!(
            result.episode.actor.actor_name,
            FORCED_RUN_CONTINUATION_ACTOR_NAME_V2
        );
        assert!(matches!(
            result.episode.outcome,
            TaskEpisodeOutcomeV1::Failure { .. }
        ));
        assert!(result.archive.nodes.iter().all(|node| matches!(
            node.proof,
            crate::agent::task::TaskRunArchiveNodeProofV1::ReachableUnknown
        )));
        let payload = result
            .checkpoints
            .encode(16 * 1024 * 1024)
            .expect("encode shared-history checkpoints");
        let decoded = TaskRunBranchCheckpointArtifactV1::decode_for_archive(
            &payload,
            &result.archive,
            16 * 1024 * 1024,
        )
        .expect("decode shared-history checkpoints");
        assert_eq!(
            decoded.checkpoint_count(),
            result.checkpoints.checkpoint_count()
        );
        let inventory = task_run_branch_decision_inventory_v1(&result.archive, &decoded)
            .expect("public branch inventory");
        let restored_root = inventory
            .decisions
            .iter()
            .find(|decision| decision.target_archive_node_id == root_node_id)
            .expect("source root remains restorable");
        assert_eq!(
            restored_root.observed_immediate_choices,
            [TaskPublicChoiceV1::one(0)]
        );
        assert!(restored_root.candidates[0].direct_choice_observed);
        assert!(inventory.decisions.iter().any(|decision| {
            decision.target_archive_depth > decision.recovery_checkpoint_depth
                && decision.replayed_public_step_count
                    == decision.target_archive_depth - decision.recovery_checkpoint_depth
        }));

        let error = resume_privileged_reference_run_continuation_v2(
            &result.archive,
            &decoded,
            &root_node_id,
            TaskPublicChoiceV1::one(0),
            TaskBudgetV1 {
                max_world_transitions: 2,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .err()
        .expect("observed branch must be rejected");
        assert!(error.contains("already observed"));
    }

    #[test]
    fn controlled_continuation_yields_at_the_next_strategic_decision() {
        let (archive, checkpoints, plan_options) = root_fixture(0);
        let root_node_id = archive.nodes[0].node_id.clone();
        let result = resume_run_until_next_strategic_decision_v1(
            &archive,
            &checkpoints,
            &root_node_id,
            TaskPublicChoiceV1::one(0),
            TaskBudgetV1 {
                max_world_transitions: 24,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("controlled continuation");

        assert!(matches!(
            result.episode.outcome,
            TaskEpisodeOutcomeV1::Paused {
                reason: crate::agent::task::TaskEpisodePauseV1::PublicDecisionControlYield {
                    site
                }
            } if site != TaskDecisionSiteV1::Combat
        ));
        let paused_node_id = task_run_paused_decision_node_id_v1(
            &result.archive,
            &result.attempt_id,
            &result.episode,
        )
        .expect("paused decision node");
        let inventory = task_run_branch_decision_inventory_v1(&result.archive, &result.checkpoints)
            .expect("paused inventory");
        assert!(inventory
            .decisions
            .iter()
            .any(|decision| decision.target_archive_node_id == paused_node_id));
        assert!(result.archive.nodes.iter().all(|node| matches!(
            node.proof,
            crate::agent::task::TaskRunArchiveNodeProofV1::ReachableUnknown
        )));
    }

    #[test]
    fn hot_exact_run_play_session_advances_without_reopening_its_checkpoint() {
        let (archive, checkpoints, plan_options) = root_fixture(0);
        let root_node_id = archive.nodes[0].node_id.clone();
        let mut session = ExactRunPlaySessionV1::open(
            archive,
            checkpoints,
            &root_node_id,
            TaskBudgetV1 {
                max_world_transitions: 24,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("hot exact run-play session");
        let root_snapshot = session.hot_snapshot_v1().expect("hot root snapshot");

        let step = session
            .choose(
                TaskPublicChoiceV1::one(0),
                ExactRunPlayAdvanceV1::NextPublicDecision,
            )
            .expect("one hot exact step");

        assert!(matches!(
            step.episode.outcome,
            TaskEpisodeOutcomeV1::Paused { .. }
        ));
        assert!(step.current_archive_node_id.is_some());
        assert!(step.archive_attempts_added >= 1);
        assert_eq!(
            session.current_archive_node_id(),
            step.current_archive_node_id.as_deref()
        );
        let archive_nodes_after_step = session.archive().nodes.len();
        session
            .restore_hot_snapshot_v1(&root_snapshot)
            .expect("restore hot root snapshot");
        assert_eq!(
            session.current_archive_node_id(),
            Some(root_node_id.as_str())
        );
        assert_eq!(session.archive().nodes.len(), archive_nodes_after_step);
    }

    #[test]
    fn exact_run_play_prepared_line_is_private_until_installed() {
        let (archive, checkpoints, plan_options) = root_fixture(0);
        let root_node_id = archive.nodes[0].node_id.clone();
        let mut session = ExactRunPlaySessionV1::open(
            archive,
            checkpoints,
            &root_node_id,
            TaskBudgetV1 {
                max_world_transitions: 24,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("hot exact run-play session");
        let nodes_before = session.archive().nodes.len();
        let attempts_before = session.archive().attempts.len();
        let checkpoints_before = session.checkpoints().checkpoint_count();

        let prepared = session
            .prepare_line_v1(&[TaskPublicChoiceV1::one(0)])
            .expect("prepare one exact public action");

        assert_eq!(
            session.current_archive_node_id(),
            Some(root_node_id.as_str())
        );
        assert_eq!(session.archive().nodes.len(), nodes_before);
        assert_eq!(session.archive().attempts.len(), attempts_before);
        assert_eq!(session.checkpoints().checkpoint_count(), checkpoints_before);
        assert_eq!(prepared.source_archive_node_id(), root_node_id);
        assert_ne!(prepared.to_archive_node_id(), root_node_id);
        assert!(prepared.archive().nodes.len() > nodes_before);
        assert!(prepared.archive().attempts.len() > attempts_before);
        assert_eq!(
            prepared
                .current_decision()
                .expect("prepared decision endpoint")
                .replayed_public_step_count,
            0
        );
        assert!(prepared.terminal_outcome().is_none());
        assert!(prepared
            .checkpoints()
            .identities()
            .any(|identity| identity.archive_node_id == prepared.to_archive_node_id()));

        let final_node_id = prepared.to_archive_node_id().to_string();
        session.install_prepared_line_v1(prepared);
        assert_eq!(
            session.current_archive_node_id(),
            Some(final_node_id.as_str())
        );
        let recovery = recoverable_run_branch_decision_v1(
            session.archive(),
            session.checkpoints(),
            &final_node_id,
        )
        .expect("recover installed prepared line");
        assert!(recovery.replay_steps.is_empty());
    }

    #[test]
    fn exact_run_play_prepared_line_records_one_atomic_archive_attempt() {
        let (archive, checkpoints, plan_options) = root_fixture(0);
        let root_node_id = archive.nodes[0].node_id.clone();
        let session = ExactRunPlaySessionV1::open(
            archive,
            checkpoints,
            &root_node_id,
            TaskBudgetV1 {
                max_world_transitions: 24,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("hot exact run-play session");
        let attempts_before = session.archive().attempts.len();

        let prepared = session
            .prepare_line_resolving_v1(2, &mut |_index, decision| {
                let choice = TaskPublicChoiceV1::one(0);
                decision.validate_choice(&choice)?;
                Ok(choice)
            })
            .expect("prepare two exact public actions");

        assert_eq!(prepared.choices().len(), 2);
        assert_eq!(prepared.archive().attempts.len(), attempts_before + 1);
        assert_eq!(session.archive().attempts.len(), attempts_before);
    }

    #[test]
    fn exact_run_play_failed_prepared_line_leaves_session_unchanged() {
        let (archive, checkpoints, plan_options) = root_fixture(0);
        let root_node_id = archive.nodes[0].node_id.clone();
        let session = ExactRunPlaySessionV1::open(
            archive,
            checkpoints,
            &root_node_id,
            TaskBudgetV1 {
                max_world_transitions: 24,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("hot exact run-play session");
        let nodes_before = session.archive().nodes.len();
        let attempts_before = session.archive().attempts.len();
        let checkpoints_before = session.checkpoints().checkpoint_count();

        let error = session
            .prepare_line_v1(&[
                TaskPublicChoiceV1::one(0),
                TaskPublicChoiceV1::one(usize::MAX),
            ])
            .err()
            .expect("invalid second choice must reject the private line");

        assert!(error.contains("step 2 failed"));
        assert_eq!(
            session.current_archive_node_id(),
            Some(root_node_id.as_str())
        );
        assert_eq!(session.archive().nodes.len(), nodes_before);
        assert_eq!(session.archive().attempts.len(), attempts_before);
        assert_eq!(session.checkpoints().checkpoint_count(), checkpoints_before);
    }

    #[test]
    fn exact_inspection_does_not_convert_invalid_engine_choices_to_partial_success() {
        let (archive, checkpoints, plan_options) = root_fixture(0);
        let root_node_id = archive.nodes[0].node_id.clone();
        let session = ExactRunPlaySessionV1::open(
            archive,
            checkpoints,
            &root_node_id,
            TaskBudgetV1 {
                max_world_transitions: 24,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .unwrap();
        let snapshot = session.hot_snapshot_v1().unwrap();
        let nodes = session.archive().nodes.len();
        let attempts = session.archive().attempts.len();
        let result = session
            .inspect_line_resolving_v1(1, &mut |_, _| Ok(TaskPublicChoiceV1::one(usize::MAX)));
        assert!(result.is_err());
        let after = session.hot_snapshot_v1().unwrap();
        assert_eq!(after.archive_node_id, snapshot.archive_node_id);
        assert_eq!(after.public_root, snapshot.public_root);
        assert_eq!(session.archive().nodes.len(), nodes);
        assert_eq!(session.archive().attempts.len(), attempts);
    }

    #[test]
    fn exact_run_play_checkpoint_rebases_current_decision_without_replay() {
        let (archive, checkpoints, plan_options) = root_fixture(0);
        let root_node_id = archive.nodes[0].node_id.clone();
        let mut session = ExactRunPlaySessionV1::open(
            archive,
            checkpoints,
            &root_node_id,
            TaskBudgetV1 {
                max_world_transitions: 24,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("hot exact run-play session");
        let initial_checkpoint_count = session.checkpoints().checkpoint_count();
        session
            .choose(
                TaskPublicChoiceV1::one(0),
                ExactRunPlayAdvanceV1::NextPublicDecision,
            )
            .expect("one exact step");
        let current_node_id = session
            .current_archive_node_id()
            .expect("current public decision")
            .to_string();

        let prepared = session
            .prepare_current_checkpoint_v1()
            .expect("prepare current exact checkpoint");
        assert_eq!(
            prepared.checkpoints().checkpoint_count(),
            initial_checkpoint_count + 1
        );
        assert_eq!(prepared.current_decision().replayed_public_step_count, 0);
        session.install_prepared_checkpoint_v1(prepared);

        let recovery = recoverable_run_branch_decision_v1(
            session.archive(),
            session.checkpoints(),
            &current_node_id,
        )
        .expect("recover current checkpoint");
        assert_eq!(recovery.recovery_checkpoint_node_id, current_node_id);
        assert!(recovery.replay_steps.is_empty());
    }

    #[test]
    fn resumed_continuation_rejects_choice_outside_restored_surface() {
        let (archive, checkpoints, plan_options) = root_fixture(1);
        let error = resume_privileged_reference_run_continuation_v2(
            &archive,
            &checkpoints,
            &archive.nodes[0].node_id,
            TaskPublicChoiceV1::one(usize::MAX),
            TaskBudgetV1 {
                max_world_transitions: 2,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .err()
        .expect("invalid forced choice must fail");
        assert!(error.contains("outside candidate surface"));
        assert_eq!(archive.attempts.len(), 1);
        assert_eq!(checkpoints.checkpoint_count(), 1);
    }

    #[test]
    fn resumed_continuation_replays_public_prefix_before_forcing_an_internal_decision() {
        let (archive, checkpoints, plan_options) = root_fixture(0);
        let root_node_id = archive.nodes[0].node_id.clone();
        let first = resume_privileged_reference_run_continuation_v2(
            &archive,
            &checkpoints,
            &root_node_id,
            TaskPublicChoiceV1::one(0),
            TaskBudgetV1 {
                max_world_transitions: 12,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("record a replayable public prefix");
        let inventory = task_run_branch_decision_inventory_v1(&first.archive, &first.checkpoints)
            .expect("recoverable decisions");
        let target = inventory
            .decisions
            .iter()
            .filter(|decision| decision.replayed_public_step_count > 0)
            .find_map(|decision| {
                decision
                    .candidates
                    .iter()
                    .find(|candidate| {
                        candidate.direct_choice.is_some() && !candidate.direct_choice_observed
                    })
                    .and_then(|candidate| {
                        candidate
                            .direct_choice
                            .clone()
                            .map(|choice| (decision, choice))
                    })
            })
            .expect("fixture exposes an internal unobserved choice");
        let resumed = resume_privileged_reference_run_continuation_v2(
            &first.archive,
            &first.checkpoints,
            &target.0.target_archive_node_id,
            target.1.clone(),
            TaskBudgetV1 {
                max_world_transitions: 8,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("replay prefix and force internal choice");

        assert_eq!(
            resumed.forced_deviation.recovery_checkpoint_node_id,
            target.0.recovery_checkpoint_node_id
        );
        assert_eq!(
            resumed.forced_deviation.target_archive_node_id,
            target.0.target_archive_node_id
        );
        assert_eq!(
            resumed.forced_deviation.replayed_public_step_count,
            target.0.replayed_public_step_count
        );
        assert_eq!(resumed.forced_deviation.choice, target.1);
        assert_eq!(resumed.archive.attempts.len(), 3);
    }

    #[test]
    fn budget_stopped_active_scope_successor_remains_a_recoverable_unknown_decision() {
        let (archive, checkpoints, plan_options) = root_fixture(0);
        let root_node_id = archive.nodes[0].node_id.clone();
        let result = resume_privileged_reference_run_continuation_v2(
            &archive,
            &checkpoints,
            &root_node_id,
            TaskPublicChoiceV1::one(0),
            TaskBudgetV1 {
                max_world_transitions: 1,
                max_reloads: 0,
            },
            plan_options,
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("record budget-stopped active prefix");
        let attempt = result
            .archive
            .attempts
            .iter()
            .find(|attempt| attempt.attempt_id == result.attempt_id)
            .expect("recorded attempt");
        let [frontier_leaf] = attempt.observed_frontier_node_ids.as_slice() else {
            panic!("one-step active prefix should retain one frontier leaf");
        };
        assert!(matches!(
            result
                .archive
                .node(frontier_leaf)
                .expect("frontier leaf")
                .proof,
            TaskRunArchiveNodeProofV1::ReachableUnknown
        ));
        let inventory = task_run_branch_decision_inventory_v1(&result.archive, &result.checkpoints)
            .expect("frontier inventory");
        let recovered = inventory
            .decisions
            .iter()
            .find(|decision| decision.target_archive_node_id == *frontier_leaf)
            .expect("frontier successor is recoverable");
        assert_eq!(recovered.recovery_checkpoint_depth, 0);
        assert_eq!(recovered.replayed_public_step_count, 1);
        assert!(matches!(
            recovered.proof,
            TaskRunArchiveNodeProofV1::ReachableUnknown
        ));
        let choice = recovered
            .candidates
            .iter()
            .find(|candidate| {
                candidate.direct_choice.is_some() && !candidate.direct_choice_observed
            })
            .and_then(|candidate| candidate.direct_choice.clone())
            .expect("frontier successor has an unobserved direct choice");
        let resumed = resume_privileged_reference_run_continuation_v2(
            &result.archive,
            &result.checkpoints,
            frontier_leaf,
            choice.clone(),
            TaskBudgetV1 {
                max_world_transitions: 2,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1::default(),
            ExactBranchingTaskEvidenceRequestV1::default(),
        )
        .expect("recover active frontier and force its successor");
        assert_eq!(
            resumed.forced_deviation.target_archive_node_id,
            *frontier_leaf
        );
        assert_eq!(resumed.forced_deviation.replayed_public_step_count, 1);
        assert_eq!(resumed.forced_deviation.choice, choice);
    }
}
