use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    CommittedPublicHistoryV1, GameOutcomeV1, TaskActorEpisodeV1, TaskActorIdentityV1,
    TaskBudgetStateV1, TaskBudgetV1, TaskCommittedScopeExitV1, TaskEpisodeOutcomeV1,
    TaskPublicDecisionV1, TaskScopedPublicStepV1, TaskSpecV1,
};
use crate::ai::planner_core::stable_planner_id;

pub const TASK_RUN_SOLUTION_ARCHIVE_SCHEMA_V1: &str = "sts-task-run-solution-archive-v1";

/// Every node in the archive is replay-observed and therefore reachable.
/// A bounded miss or a committed defeat never changes `ReachableUnknown` into
/// a synthetic loss label. Only a complete task success can install a
/// `ProvenVictory` witness on its committed prefix.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "proof", rename_all = "snake_case")]
pub enum TaskRunArchiveNodeProofV1 {
    ReachableUnknown,
    ProvenVictory { first_proving_attempt_id: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunArchiveNodeV1 {
    pub node_id: String,
    pub seed: u64,
    pub parent_node_id: Option<String>,
    pub depth: usize,
    pub initial_public_decision: Option<TaskPublicDecisionV1>,
    pub incoming_step: Option<TaskScopedPublicStepV1>,
    pub proof: TaskRunArchiveNodeProofV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunArchiveAttemptV1 {
    pub attempt_id: String,
    pub seed: u64,
    pub actor: TaskActorIdentityV1,
    /// Identifies caller-owned planner, model, and compute configuration not
    /// represented by the task transition/reload budget alone.
    pub execution_manifest_id: String,
    pub requested_budget: TaskBudgetV1,
    pub remaining_budget: TaskBudgetStateV1,
    pub outcome: TaskEpisodeOutcomeV1,
    pub committed_history_id: String,
    /// Root first, followed by one node for every committed public step.
    pub node_path: Vec<String>,
    /// Leaves of replay-observed active-scope attempts extending the committed
    /// path. They remain reachable unknown and are never victory proof.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observed_frontier_node_ids: Vec<String>,
}

/// A seed-partitioned trie of complete-run committed behavior. It is an
/// evidence archive, not a value function: failed and censored attempts remain
/// useful reachability records but never create negative action labels.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRunSolutionArchiveV1 {
    pub schema: String,
    pub task: TaskSpecV1,
    pub nodes: Vec<TaskRunArchiveNodeV1>,
    pub attempts: Vec<TaskRunArchiveAttemptV1>,
}

impl TaskRunSolutionArchiveV1 {
    pub fn new(task: TaskSpecV1) -> Result<Self, String> {
        task.validate()?;
        if !task.is_complete_run_task() {
            return Err(
                "complete-run solution archive requires a natural or exact-run decision root"
                    .to_string(),
            );
        }
        Ok(Self {
            schema: TASK_RUN_SOLUTION_ARCHIVE_SCHEMA_V1.to_string(),
            task,
            nodes: Vec::new(),
            attempts: Vec::new(),
        })
    }

    /// Revalidates a durable archive before it is joined to private restore
    /// state. The archive intentionally does not retain full completed-scope
    /// ledgers, so this proves its stored trie, attempt identities, and victory
    /// references rather than replaying the source episodes.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != TASK_RUN_SOLUTION_ARCHIVE_SCHEMA_V1 {
            return Err("run solution archive schema is unsupported".to_string());
        }
        self.task.validate()?;
        if !self.task.is_complete_run_task() {
            return Err(
                "complete-run solution archive requires a natural or exact-run decision root"
                    .to_string(),
            );
        }

        let mut nodes = BTreeMap::new();
        for node in &self.nodes {
            if node.node_id.trim().is_empty() {
                return Err("run archive node id must not be empty".to_string());
            }
            if nodes.insert(node.node_id.as_str(), node).is_some() {
                return Err(format!("run archive repeats node {}", node.node_id));
            }
        }
        for node in &self.nodes {
            if node.depth == 0 {
                if node.parent_node_id.is_some() || node.incoming_step.is_some() {
                    return Err(format!(
                        "run archive root {} has parent or incoming step",
                        node.node_id
                    ));
                }
                let initial = node.initial_public_decision.as_ref().ok_or_else(|| {
                    format!("run archive root {} has no initial decision", node.node_id)
                })?;
                initial.validate()?;
                let expected = stable_planner_id(
                    "task_run_archive_root_v1",
                    &(node.seed, &self.task, initial),
                )?;
                if expected != node.node_id {
                    return Err(format!(
                        "run archive root {} has invalid stable identity",
                        node.node_id
                    ));
                }
                continue;
            }

            if node.initial_public_decision.is_some() {
                return Err(format!(
                    "non-root run archive node {} contains an initial decision",
                    node.node_id
                ));
            }
            let parent_id = node.parent_node_id.as_deref().ok_or_else(|| {
                format!("non-root run archive node {} has no parent", node.node_id)
            })?;
            let parent = nodes.get(parent_id).ok_or_else(|| {
                format!(
                    "run archive node {} has missing parent {parent_id}",
                    node.node_id
                )
            })?;
            if parent.seed != node.seed || parent.depth.checked_add(1) != Some(node.depth) {
                return Err(format!(
                    "run archive node {} disagrees with its parent depth or seed",
                    node.node_id
                ));
            }
            let incoming = node.incoming_step.as_ref().ok_or_else(|| {
                format!(
                    "non-root run archive node {} has no incoming step",
                    node.node_id
                )
            })?;
            incoming.step.source.validate()?;
            incoming
                .step
                .source
                .validate_choice(&incoming.step.choice)?;
            if incoming.site != incoming.step.source.site {
                return Err(format!(
                    "run archive node {} incoming site disagrees with its source",
                    node.node_id
                ));
            }
            let expected = stable_planner_id("task_run_archive_node_v1", &(parent_id, incoming))?;
            if expected != node.node_id {
                return Err(format!(
                    "run archive node {} has invalid stable identity",
                    node.node_id
                ));
            }
        }

        let mut attempts = BTreeMap::new();
        let mut replay_observed_nodes = BTreeSet::new();
        for attempt in &self.attempts {
            if attempts
                .insert(attempt.attempt_id.as_str(), attempt)
                .is_some()
            {
                return Err(format!(
                    "run archive repeats attempt {}",
                    attempt.attempt_id
                ));
            }
            attempt.actor.validate()?;
            attempt.requested_budget.validate()?;
            if attempt.execution_manifest_id.trim().is_empty()
                || attempt.committed_history_id.trim().is_empty()
            {
                return Err(format!(
                    "run archive attempt {} has an empty durable identity",
                    attempt.attempt_id
                ));
            }
            if attempt.remaining_budget.remaining_world_transitions
                > attempt.requested_budget.max_world_transitions
                || attempt.remaining_budget.remaining_reloads > attempt.requested_budget.max_reloads
            {
                return Err(format!(
                    "run archive attempt {} remaining budget exceeds its request",
                    attempt.attempt_id
                ));
            }
            if attempt.node_path.is_empty() {
                return Err(format!(
                    "run archive attempt {} has an empty node path",
                    attempt.attempt_id
                ));
            }
            let mut seen_path_nodes = BTreeSet::new();
            for (depth, node_id) in attempt.node_path.iter().enumerate() {
                if !seen_path_nodes.insert(node_id.as_str()) {
                    return Err(format!(
                        "run archive attempt {} repeats node {node_id}",
                        attempt.attempt_id
                    ));
                }
                replay_observed_nodes.insert(node_id.as_str());
                let node = nodes.get(node_id.as_str()).ok_or_else(|| {
                    format!(
                        "run archive attempt {} references missing node {node_id}",
                        attempt.attempt_id
                    )
                })?;
                let expected_parent = depth
                    .checked_sub(1)
                    .map(|index| attempt.node_path[index].as_str());
                if node.seed != attempt.seed
                    || node.depth != depth
                    || node.parent_node_id.as_deref() != expected_parent
                {
                    return Err(format!(
                        "run archive attempt {} has a discontinuous node path",
                        attempt.attempt_id
                    ));
                }
            }
            let committed_leaf = attempt
                .node_path
                .last()
                .expect("non-empty attempt path has a leaf");
            let mut previous_frontier: Option<&str> = None;
            for frontier_leaf in &attempt.observed_frontier_node_ids {
                if previous_frontier.is_some_and(|previous| previous >= frontier_leaf.as_str()) {
                    return Err(format!(
                        "run archive attempt {} frontiers are not in canonical order",
                        attempt.attempt_id
                    ));
                }
                previous_frontier = Some(frontier_leaf);
                if frontier_leaf == committed_leaf {
                    return Err(format!(
                        "run archive attempt {} records an empty active frontier",
                        attempt.attempt_id
                    ));
                }
                let mut cursor = nodes.get(frontier_leaf.as_str()).ok_or_else(|| {
                    format!(
                        "run archive attempt {} references missing frontier {frontier_leaf}",
                        attempt.attempt_id
                    )
                })?;
                loop {
                    if cursor.seed != attempt.seed {
                        return Err(format!(
                            "run archive attempt {} frontier belongs to another seed",
                            attempt.attempt_id
                        ));
                    }
                    replay_observed_nodes.insert(cursor.node_id.as_str());
                    let Some(parent_node_id) = cursor.parent_node_id.as_deref() else {
                        return Err(format!(
                            "run archive attempt {} frontier does not extend its committed path",
                            attempt.attempt_id
                        ));
                    };
                    if parent_node_id == committed_leaf {
                        break;
                    }
                    cursor = nodes.get(parent_node_id).ok_or_else(|| {
                        format!(
                            "run archive frontier {} has missing parent {parent_node_id}",
                            frontier_leaf
                        )
                    })?;
                }
            }
            let expected_attempt_id = task_run_archive_attempt_id_v1(
                attempt.seed,
                &self.task,
                &attempt.actor,
                attempt.execution_manifest_id.as_str(),
                attempt.requested_budget,
                attempt.remaining_budget,
                &attempt.outcome,
                attempt.committed_history_id.as_str(),
                &attempt.observed_frontier_node_ids,
            )?;
            if expected_attempt_id != attempt.attempt_id {
                return Err(format!(
                    "run archive attempt {} has invalid stable identity",
                    attempt.attempt_id
                ));
            }
            if matches!(
                attempt.outcome,
                TaskEpisodeOutcomeV1::Success {
                    game_outcome: GameOutcomeV1::Defeat
                }
            ) {
                return Err(format!(
                    "run archive attempt {} calls defeat a success",
                    attempt.attempt_id
                ));
            }
        }

        for node in &self.nodes {
            if !replay_observed_nodes.contains(node.node_id.as_str()) {
                return Err(format!(
                    "run archive node {} is not owned by a replay-observed attempt",
                    node.node_id
                ));
            }
        }

        for node in &self.nodes {
            let TaskRunArchiveNodeProofV1::ProvenVictory {
                first_proving_attempt_id,
            } = &node.proof
            else {
                continue;
            };
            let attempt = attempts
                .get(first_proving_attempt_id.as_str())
                .ok_or_else(|| {
                    format!(
                    "run archive node {} cites missing proving attempt {first_proving_attempt_id}",
                    node.node_id
                )
                })?;
            if !matches!(attempt.outcome, TaskEpisodeOutcomeV1::Success { .. })
                || !attempt.node_path.contains(&node.node_id)
            {
                return Err(format!(
                    "run archive node {} has invalid victory proof",
                    node.node_id
                ));
            }
        }
        for attempt in &self.attempts {
            if !matches!(attempt.outcome, TaskEpisodeOutcomeV1::Success { .. }) {
                continue;
            }
            if attempt.node_path.iter().any(|node_id| {
                !matches!(
                    nodes[node_id.as_str()].proof,
                    TaskRunArchiveNodeProofV1::ProvenVictory { .. }
                )
            }) {
                return Err(format!(
                    "successful run archive attempt {} has an unproven path",
                    attempt.attempt_id
                ));
            }
        }
        Ok(())
    }

    /// Produces the smallest conservative archive that still owns the public
    /// path from the natural entrance to `target_node_id`. All off-path
    /// exploration is intentionally discarded. Victory-bearing paths are
    /// rejected instead of silently weakening their proof provenance.
    pub fn retained_recovery_path_to_node_v1(&self, target_node_id: &str) -> Result<Self, String> {
        self.validate()?;
        let nodes = self
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let mut reversed_path = Vec::new();
        let mut cursor = nodes
            .get(target_node_id)
            .copied()
            .ok_or_else(|| format!("run archive has no target node {target_node_id}"))?;
        loop {
            reversed_path.push(cursor);
            let Some(parent_node_id) = cursor.parent_node_id.as_deref() else {
                break;
            };
            cursor = nodes
                .get(parent_node_id)
                .copied()
                .ok_or_else(|| format!("run archive target path misses parent {parent_node_id}"))?;
        }
        reversed_path.reverse();
        if reversed_path
            .iter()
            .any(|node| matches!(node.proof, TaskRunArchiveNodeProofV1::ProvenVictory { .. }))
        {
            return Err(
                "active-path compaction refuses to discard victory-proof provenance".to_string(),
            );
        }
        let target_path_ids = reversed_path
            .iter()
            .map(|node| node.node_id.as_str())
            .collect::<Vec<_>>();
        let source_attempt = self
            .attempts
            .iter()
            .filter(|attempt| {
                attempt.node_path.len() <= target_path_ids.len()
                    && attempt
                        .node_path
                        .iter()
                        .map(String::as_str)
                        .eq(target_path_ids.iter().copied().take(attempt.node_path.len()))
            })
            .filter(|attempt| {
                if attempt.node_path.len() == target_path_ids.len() {
                    return true;
                }
                attempt.observed_frontier_node_ids.iter().any(|frontier| {
                    archive_ancestor_path_contains_v1(&nodes, target_node_id, frontier)
                })
            })
            .max_by_key(|attempt| attempt.node_path.len())
            .ok_or_else(|| {
                format!(
                    "run archive has no single replay-observed attempt owning target {target_node_id}"
                )
            })?;
        let mut retained_attempt = source_attempt.clone();
        retained_attempt.observed_frontier_node_ids =
            if retained_attempt.node_path.len() == target_path_ids.len() {
                Vec::new()
            } else {
                vec![target_node_id.to_string()]
            };
        retained_attempt.attempt_id = task_run_archive_attempt_id_v1(
            retained_attempt.seed,
            &self.task,
            &retained_attempt.actor,
            retained_attempt.execution_manifest_id.as_str(),
            retained_attempt.requested_budget,
            retained_attempt.remaining_budget,
            &retained_attempt.outcome,
            retained_attempt.committed_history_id.as_str(),
            &retained_attempt.observed_frontier_node_ids,
        )?;
        let retained = Self {
            schema: self.schema.clone(),
            task: self.task.clone(),
            nodes: reversed_path.into_iter().cloned().collect(),
            attempts: vec![retained_attempt],
        };
        retained.validate()?;
        Ok(retained)
    }

    /// Produces a validated archive suitable for an independently writable
    /// fork at `target_node_id` without weakening victory evidence.
    ///
    /// Ordinary reachable-unknown paths use the compact recovery archive.
    /// Every node on a successful path owns `ProvenVictory` provenance, so a
    /// historical fork from such a path must retain the complete source
    /// archive instead of rewriting that proof into a smaller recovery-only
    /// attempt.
    pub fn retained_fork_archive_to_node_v1(&self, target_node_id: &str) -> Result<Self, String> {
        self.validate()?;
        let nodes = self
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let mut cursor = nodes
            .get(target_node_id)
            .copied()
            .ok_or_else(|| format!("run archive has no target node {target_node_id}"))?;
        loop {
            if matches!(
                cursor.proof,
                TaskRunArchiveNodeProofV1::ProvenVictory { .. }
            ) {
                return Ok(self.clone());
            }
            let Some(parent_node_id) = cursor.parent_node_id.as_deref() else {
                break;
            };
            cursor = nodes
                .get(parent_node_id)
                .copied()
                .ok_or_else(|| format!("run archive target path misses parent {parent_node_id}"))?;
        }
        self.retained_recovery_path_to_node_v1(target_node_id)
    }

    pub fn record_episode(
        &mut self,
        seed: u64,
        execution_manifest_id: impl Into<String>,
        requested_budget: TaskBudgetV1,
        episode: &TaskActorEpisodeV1,
    ) -> Result<String, String> {
        requested_budget.validate()?;
        if episode.final_view.task != self.task {
            return Err("run archive episode belongs to a different task".to_string());
        }
        if episode.final_view.budget.remaining_world_transitions
            > requested_budget.max_world_transitions
            || episode.final_view.budget.remaining_reloads > requested_budget.max_reloads
        {
            return Err("run archive episode remaining budget exceeds its request".to_string());
        }
        let execution_manifest_id = execution_manifest_id.into();
        if execution_manifest_id.trim().is_empty() {
            return Err("run archive execution manifest id must not be empty".to_string());
        }
        let history = &episode.final_view.committed_history;
        let initial_public_decision = history
            .steps
            .first()
            .map(|step| step.step.source.clone())
            .or_else(|| {
                episode
                    .final_view
                    .legal_knowledge
                    .completed_scopes
                    .first()
                    .and_then(|scope| scope.attempts.first())
                    .map(|attempt| attempt.root.clone())
            })
            .or_else(|| {
                episode
                    .final_view
                    .node_memory
                    .as_ref()
                    .and_then(|memory| memory.attempts.first())
                    .map(|attempt| attempt.root.clone())
            })
            .ok_or_else(|| {
                "run archive episode has no public root or committed prefix".to_string()
            })?;
        initial_public_decision.validate()?;
        validate_committed_episode(history, &episode.outcome)?;
        let committed_history_id = stable_planner_id("task_committed_run_history_v1", history)?;
        let node_path =
            committed_prefix_node_ids_v1(seed, &self.task, &initial_public_decision, history)?;
        let root_id = node_path[0].clone();
        let root = TaskRunArchiveNodeV1 {
            node_id: root_id.clone(),
            seed,
            parent_node_id: None,
            depth: 0,
            initial_public_decision: Some(initial_public_decision),
            incoming_step: None,
            proof: TaskRunArchiveNodeProofV1::ReachableUnknown,
        };
        self.insert_or_validate_node(root)?;

        let mut parent_id = root_id;
        for (index, step) in history.steps.iter().enumerate() {
            let node_id = node_path[index + 1].clone();
            let node = TaskRunArchiveNodeV1 {
                node_id: node_id.clone(),
                seed,
                parent_node_id: Some(parent_id),
                depth: index + 1,
                initial_public_decision: None,
                incoming_step: Some(step.clone().into()),
                proof: TaskRunArchiveNodeProofV1::ReachableUnknown,
            };
            self.insert_or_validate_node(node)?;
            parent_id = node_id;
        }

        let committed_leaf = node_path
            .last()
            .expect("non-empty committed path has a leaf")
            .clone();
        let committed_depth = node_path.len() - 1;
        let mut observed_frontier_node_ids = Vec::new();
        if let Some(memory) = &episode.final_view.node_memory {
            for attempt in &memory.attempts {
                if attempt.steps.is_empty() {
                    continue;
                }
                if attempt.steps[0].source != attempt.root {
                    return Err("run archive active attempt changed its public root".to_string());
                }
                let mut frontier_parent = committed_leaf.clone();
                for (offset, step) in attempt.steps.iter().enumerate() {
                    step.source.validate()?;
                    step.source.validate_choice(&step.choice)?;
                    let scoped = TaskScopedPublicStepV1 {
                        scope: memory.scope,
                        site: step.source.site,
                        step: step.clone(),
                    };
                    let node_id = stable_planner_id(
                        "task_run_archive_node_v1",
                        &(frontier_parent.as_str(), &scoped),
                    )?;
                    self.insert_or_validate_node(TaskRunArchiveNodeV1 {
                        node_id: node_id.clone(),
                        seed,
                        parent_node_id: Some(frontier_parent),
                        depth: committed_depth + offset + 1,
                        initial_public_decision: None,
                        incoming_step: Some(scoped),
                        proof: TaskRunArchiveNodeProofV1::ReachableUnknown,
                    })?;
                    frontier_parent = node_id;
                }
                observed_frontier_node_ids.push(frontier_parent);
            }
        }
        observed_frontier_node_ids.sort();
        observed_frontier_node_ids.dedup();
        let attempt_id = task_run_archive_attempt_id_v1(
            seed,
            &self.task,
            &episode.actor,
            execution_manifest_id.as_str(),
            requested_budget,
            episode.final_view.budget,
            &episode.outcome,
            committed_history_id.as_str(),
            &observed_frontier_node_ids,
        )?;
        if self
            .attempts
            .iter()
            .any(|attempt| attempt.attempt_id == attempt_id)
        {
            return Ok(attempt_id);
        }

        self.attempts.push(TaskRunArchiveAttemptV1 {
            attempt_id: attempt_id.clone(),
            seed,
            actor: episode.actor.clone(),
            execution_manifest_id,
            requested_budget,
            remaining_budget: episode.final_view.budget,
            outcome: episode.outcome.clone(),
            committed_history_id,
            node_path: node_path.clone(),
            observed_frontier_node_ids,
        });
        if matches!(episode.outcome, TaskEpisodeOutcomeV1::Success { .. }) {
            for node_id in node_path {
                let node = self
                    .nodes
                    .iter_mut()
                    .find(|node| node.node_id == node_id)
                    .expect("recorded archive path must resolve");
                if matches!(node.proof, TaskRunArchiveNodeProofV1::ReachableUnknown) {
                    node.proof = TaskRunArchiveNodeProofV1::ProvenVictory {
                        first_proving_attempt_id: attempt_id.clone(),
                    };
                }
            }
        }
        Ok(attempt_id)
    }

    pub fn node(&self, node_id: &str) -> Option<&TaskRunArchiveNodeV1> {
        self.nodes.iter().find(|node| node.node_id == node_id)
    }

    /// Resolves a replay-observed committed prefix to its stable archive node.
    /// `current_root` is used only for a root-only prefix; later prefixes carry
    /// their natural entrance in the first committed step. The current root is
    /// still validated so a private checkpoint cannot smuggle malformed public
    /// data through this join.
    pub fn node_id_for_committed_prefix(
        &self,
        seed: u64,
        current_root: &TaskPublicDecisionV1,
        history: &CommittedPublicHistoryV1,
    ) -> Result<String, String> {
        current_root.validate()?;
        validate_committed_history(history)?;
        let initial_public_decision = history
            .steps
            .first()
            .map(|step| step.step.source.clone())
            .unwrap_or_else(|| current_root.clone());
        let node_path =
            committed_prefix_node_ids_v1(seed, &self.task, &initial_public_decision, history)?;
        for (index, node_id) in node_path.iter().enumerate() {
            let node = self.node(node_id).ok_or_else(|| {
                format!("run archive does not contain committed prefix node {node_id}")
            })?;
            let expected_parent = index
                .checked_sub(1)
                .map(|parent| node_path[parent].as_str());
            if node.seed != seed
                || node.depth != index
                || node.parent_node_id.as_deref() != expected_parent
            {
                return Err(format!(
                    "run archive node {node_id} disagrees with its committed prefix"
                ));
            }
            if index == 0 {
                if node.initial_public_decision.as_ref() != Some(&initial_public_decision)
                    || node.incoming_step.is_some()
                {
                    return Err(format!(
                        "run archive root {node_id} disagrees with its natural entrance"
                    ));
                }
            } else if node.initial_public_decision.is_some()
                || node.incoming_step.as_ref() != Some(&history.steps[index - 1].clone().into())
            {
                return Err(format!(
                    "run archive node {node_id} disagrees with committed step {}",
                    index - 1
                ));
            }
        }
        node_path
            .last()
            .cloned()
            .ok_or_else(|| "run archive committed prefix produced no root node".to_string())
    }

    fn insert_or_validate_node(&mut self, node: TaskRunArchiveNodeV1) -> Result<(), String> {
        let Some(existing) = self
            .nodes
            .iter()
            .find(|existing| existing.node_id == node.node_id)
        else {
            self.nodes.push(node);
            return Ok(());
        };
        if existing.seed != node.seed
            || existing.parent_node_id != node.parent_node_id
            || existing.depth != node.depth
            || existing.initial_public_decision != node.initial_public_decision
            || existing.incoming_step != node.incoming_step
        {
            return Err(format!(
                "run archive node id {} collided with different content",
                node.node_id
            ));
        }
        Ok(())
    }
}

fn archive_ancestor_path_contains_v1(
    nodes: &BTreeMap<&str, &TaskRunArchiveNodeV1>,
    ancestor_node_id: &str,
    descendant_node_id: &str,
) -> bool {
    let mut cursor = nodes.get(descendant_node_id).copied();
    while let Some(node) = cursor {
        if node.node_id == ancestor_node_id {
            return true;
        }
        cursor = node
            .parent_node_id
            .as_deref()
            .and_then(|parent| nodes.get(parent).copied());
    }
    false
}

fn task_run_archive_attempt_id_v1(
    seed: u64,
    task: &TaskSpecV1,
    actor: &TaskActorIdentityV1,
    execution_manifest_id: &str,
    requested_budget: TaskBudgetV1,
    remaining_budget: TaskBudgetStateV1,
    outcome: &TaskEpisodeOutcomeV1,
    committed_history_id: &str,
    observed_frontier_node_ids: &[String],
) -> Result<String, String> {
    if observed_frontier_node_ids.is_empty() {
        return stable_planner_id(
            "task_run_archive_attempt_v1",
            &(
                seed,
                task,
                actor,
                execution_manifest_id,
                requested_budget,
                remaining_budget,
                outcome,
                committed_history_id,
            ),
        );
    }
    stable_planner_id(
        "task_run_archive_attempt_v2",
        &(
            seed,
            task,
            actor,
            execution_manifest_id,
            requested_budget,
            remaining_budget,
            outcome,
            committed_history_id,
            observed_frontier_node_ids,
        ),
    )
}

fn validate_committed_episode(
    history: &CommittedPublicHistoryV1,
    outcome: &TaskEpisodeOutcomeV1,
) -> Result<(), String> {
    validate_committed_history(history)?;
    if let TaskEpisodeOutcomeV1::Success { game_outcome } = outcome {
        if !matches!(
            game_outcome,
            GameOutcomeV1::ActThreeVictory | GameOutcomeV1::HeartVictory
        ) {
            return Err("successful run archive episode has no victory terminal".to_string());
        }
        let Some(last_scope) = history.completed_scopes.last() else {
            return Err(
                "successful run archive episode has no committed terminal scope".to_string(),
            );
        };
        if !matches!(
            last_scope.exit,
            TaskCommittedScopeExitV1::GameTerminal {
                game_outcome: observed
            } if observed == *game_outcome
        ) {
            return Err(
                "successful run archive episode terminal disagrees with committed history"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn validate_committed_history(history: &CommittedPublicHistoryV1) -> Result<(), String> {
    for step in &history.steps {
        step.step.source.validate()?;
        step.step.source.validate_choice(&step.step.choice)?;
        if step.site != step.step.source.site {
            return Err("committed run step site disagrees with its source".to_string());
        }
    }
    Ok(())
}

fn committed_prefix_node_ids_v1(
    seed: u64,
    task: &TaskSpecV1,
    initial_public_decision: &TaskPublicDecisionV1,
    history: &CommittedPublicHistoryV1,
) -> Result<Vec<String>, String> {
    initial_public_decision.validate()?;
    validate_committed_history(history)?;
    let root_id = stable_planner_id(
        "task_run_archive_root_v1",
        &(seed, task, initial_public_decision),
    )?;
    let mut node_path = Vec::with_capacity(history.steps.len() + 1);
    node_path.push(root_id.clone());
    let mut parent_id = root_id;
    for step in &history.steps {
        let scoped: TaskScopedPublicStepV1 = step.clone().into();
        let node_id =
            stable_planner_id("task_run_archive_node_v1", &(parent_id.as_str(), &scoped))?;
        node_path.push(node_id.clone());
        parent_id = node_id;
    }
    Ok(node_path)
}
