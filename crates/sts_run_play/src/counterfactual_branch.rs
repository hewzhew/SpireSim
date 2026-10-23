use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use sts_oracle_learning::agent::task::{
    TaskDecisionSiteV1, TaskPublicCandidateV1, TaskPublicChoiceV1,
};
use sts_oracle_learning::eval::run_control::{
    ExactBranchingTaskEvidenceRequestV1, ExactRunPlaySessionV1, TaskRunBranchCheckpointArtifactV1,
    TaskRunBranchCheckpointIdentityV1,
};

use crate::agent_protocol::{project_agent_decision_v1, AgentProbeEndpointV1};
use crate::driver::run_play_path_v1;
use crate::model::{RunPlayBoundaryKindV1, RunPlayPositionV1, RunPlayPublicScalarsV1};
use crate::session_control::{
    load_session_control_v1, RunPlaySessionLifecycleV1, RunPlaySessionPurposeV1,
};
use crate::storage::{
    write_new_synced, RunPlayPathsV1, RunPlayWorkspaceV1, DEFAULT_ARTIFACT_MAX_BYTES,
};
use crate::workspace_lifecycle::{load_registered_disposable_fork_v1, ManagedWorkspaceManifestV1};

const COUNTERFACTUAL_BRANCH_EVIDENCE_SCHEMA_V1: &str =
    "sts-run-play-counterfactual-branch-evidence-v1";
const COUNTERFACTUAL_BRANCH_EVIDENCE_SCOPE_V1: &str = "counterfactual_reachability_only";
pub const DEFAULT_COUNTERFACTUAL_BRANCH_MAX_STEPS_V1: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterfactualBranchAuthorityV1 {
    pub execution_authority: bool,
    pub policy_authority: bool,
    pub training_label_authority: bool,
    pub victory_proof_authority: bool,
}

impl CounterfactualBranchAuthorityV1 {
    fn none_v1() -> Self {
        Self {
            execution_authority: false,
            policy_authority: false,
            training_label_authority: false,
            victory_proof_authority: false,
        }
    }

    fn has_any_authority_v1(&self) -> bool {
        self.execution_authority
            || self.policy_authority
            || self.training_label_authority
            || self.victory_proof_authority
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterfactualBranchRootV1 {
    pub session_path: String,
    pub session_id: String,
    pub head_revision: u64,
    pub state_revision: u64,
    pub archive_node_id: String,
    pub public_snapshot_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterfactualBranchEndpointIdentityV1 {
    pub session_path: String,
    pub session_id: String,
    pub head_revision: u64,
    pub state_revision: u64,
    pub archive_node_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterfactualBranchCaptureInvariantsV1 {
    pub registered_direct_child: bool,
    pub source_head_unchanged: bool,
    pub root_exact_checkpoint_identity_equal: bool,
    pub branch_endpoint_descends_from_root: bool,
    pub branch_parked: bool,
    pub branch_purpose_probe: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterfactualBranchStepV1 {
    pub from_archive_node_id: String,
    pub to_archive_node_id: String,
    pub depth: usize,
    pub source_public_snapshot_id: String,
    pub site: TaskDecisionSiteV1,
    pub before: RunPlayPublicScalarsV1,
    pub semantic_action: String,
    pub choice: TaskPublicChoiceV1,
    pub selected_candidate: TaskPublicCandidateV1,
    pub observed_boundary: RunPlayBoundaryKindV1,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterfactualBranchEvidenceV1 {
    pub schema: String,
    pub evidence_scope: String,
    pub authority: CounterfactualBranchAuthorityV1,
    pub source_root: CounterfactualBranchRootV1,
    pub branch_endpoint: CounterfactualBranchEndpointIdentityV1,
    pub capture_invariants: CounterfactualBranchCaptureInvariantsV1,
    pub steps: Vec<CounterfactualBranchStepV1>,
    pub endpoint: AgentProbeEndpointV1,
}

impl CounterfactualBranchEvidenceV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != COUNTERFACTUAL_BRANCH_EVIDENCE_SCHEMA_V1
            || self.evidence_scope != COUNTERFACTUAL_BRANCH_EVIDENCE_SCOPE_V1
        {
            return Err("counterfactual branch evidence identity is unsupported".to_string());
        }
        if self.authority.has_any_authority_v1() {
            return Err("counterfactual branch evidence must not carry authority".to_string());
        }
        if self.source_root.session_path.trim().is_empty()
            || self.source_root.session_id.trim().is_empty()
            || self.source_root.archive_node_id.trim().is_empty()
            || self.source_root.public_snapshot_id.trim().is_empty()
            || self.branch_endpoint.session_path.trim().is_empty()
            || self.branch_endpoint.session_id.trim().is_empty()
            || self.branch_endpoint.archive_node_id.trim().is_empty()
        {
            return Err("counterfactual branch evidence identity is incomplete".to_string());
        }
        let invariants = &self.capture_invariants;
        if !invariants.registered_direct_child
            || !invariants.source_head_unchanged
            || !invariants.root_exact_checkpoint_identity_equal
            || !invariants.branch_endpoint_descends_from_root
            || !invariants.branch_parked
            || !invariants.branch_purpose_probe
        {
            return Err(
                "counterfactual branch evidence has an unproved capture invariant".to_string(),
            );
        }
        if self.steps.is_empty() {
            return Err("counterfactual branch evidence requires a non-empty suffix".to_string());
        }
        let mut expected_from = self.source_root.archive_node_id.as_str();
        for step in &self.steps {
            if step.from_archive_node_id != expected_from
                || step.to_archive_node_id.trim().is_empty()
                || step.source_public_snapshot_id.trim().is_empty()
                || step.semantic_action.trim().is_empty()
            {
                return Err(
                    "counterfactual branch evidence contains a broken typed suffix".to_string(),
                );
            }
            expected_from = step.to_archive_node_id.as_str();
        }
        if expected_from != self.branch_endpoint.archive_node_id {
            return Err("counterfactual branch evidence suffix misses its endpoint".to_string());
        }
        Ok(())
    }
}

pub fn capture_counterfactual_branch_evidence_v1(
    target_session: &Path,
    managed_root: &Path,
    max_steps: usize,
) -> Result<CounterfactualBranchEvidenceV1, String> {
    if max_steps == 0 {
        return Err("counterfactual branch evidence requires a positive step bound".to_string());
    }
    let (canonical_target, manifest) =
        load_registered_disposable_fork_v1(managed_root, target_session)?;
    let target_paths = RunPlayPathsV1::new(&canonical_target);
    let target_control = load_session_control_v1(&target_paths)?;
    if target_control.lifecycle != RunPlaySessionLifecycleV1::Parked {
        return Err("counterfactual branch evidence requires a parked probe".to_string());
    }
    if target_control.purpose != RunPlaySessionPurposeV1::Probe {
        return Err("counterfactual branch evidence requires session purpose probe".to_string());
    }

    let canonical_source =
        fs::canonicalize(PathBuf::from(&manifest.source.session_path)).map_err(|error| {
            format!(
                "failed to resolve counterfactual branch source {}: {error}",
                manifest.source.session_path
            )
        })?;
    if canonical_source == canonical_target {
        return Err("counterfactual branch source and probe must be distinct".to_string());
    }
    let source_paths = RunPlayPathsV1::new(&canonical_source);
    let source_before = source_paths.load()?;
    validate_manifest_source_v1(&manifest, &source_before)?;
    let target = target_paths.load()?;
    if target.head.session_id != manifest.target_session_id {
        return Err("counterfactual branch probe session id mismatches its manifest".to_string());
    }

    let root_node_id = manifest.source.archive_node_id.as_str();
    let source_identity = checkpoint_identity_v1(&source_before, root_node_id)?;
    let target_identity = checkpoint_identity_v1(&target, root_node_id)?;
    if source_identity != target_identity {
        return Err(
            "counterfactual branch source and probe root checkpoint identities differ".to_string(),
        );
    }

    let branch_node_path = archive_node_path_v1(&target, target.head.position.archive_node_id())?;
    let root_index = branch_node_path
        .iter()
        .position(|node_id| node_id == root_node_id)
        .ok_or_else(|| {
            "counterfactual branch endpoint does not descend from its manifest root".to_string()
        })?;
    let suffix_node_ids = &branch_node_path[root_index + 1..];
    if suffix_node_ids.is_empty() {
        return Err("counterfactual branch probe has no active suffix".to_string());
    }
    if suffix_node_ids.len() > max_steps {
        return Err(format!(
            "counterfactual branch suffix has {} steps, exceeding the maximum of {max_steps}",
            suffix_node_ids.len()
        ));
    }

    let path_steps = run_play_path_v1(
        &target.state.archive,
        target.head.position.archive_node_id(),
    )?;
    let mut from_node_id = root_node_id.to_string();
    let mut steps = Vec::with_capacity(suffix_node_ids.len());
    for node_id in suffix_node_ids {
        let node = target
            .state
            .archive
            .node(node_id)
            .ok_or_else(|| format!("counterfactual branch archive misses node {node_id}"))?;
        let incoming = node.incoming_step.as_ref().ok_or_else(|| {
            format!("counterfactual branch node {node_id} has no incoming typed step")
        })?;
        let path_step = path_steps
            .iter()
            .find(|step| step.archive_node_id == *node_id)
            .ok_or_else(|| {
                format!("counterfactual branch path projection misses node {node_id}")
            })?;
        let semantic_action = path_step
            .semantic_action
            .as_deref()
            .filter(|action| !action.starts_with("<semantic-error:"))
            .ok_or_else(|| {
                format!("counterfactual branch node {node_id} has no valid semantic action")
            })?
            .to_string();
        steps.push(CounterfactualBranchStepV1 {
            from_archive_node_id: from_node_id,
            to_archive_node_id: node_id.clone(),
            depth: path_step.depth,
            source_public_snapshot_id: incoming.step.source.information.snapshot_id.clone(),
            site: path_step.site,
            before: path_step.before,
            semantic_action,
            choice: path_step.choice.clone(),
            selected_candidate: path_step.selected_candidate.clone(),
            observed_boundary: path_step.observed_boundary,
        });
        from_node_id = node_id.clone();
    }

    let endpoint = match &target.head.position {
        RunPlayPositionV1::Decision { .. } => {
            let decision = target
                .head
                .current_decision
                .as_ref()
                .ok_or_else(|| "counterfactual branch decision endpoint is missing".to_string())?
                .public_decision_v1()?;
            project_agent_decision_v1(&decision)?
        }
        RunPlayPositionV1::Terminal { outcome, .. } => AgentProbeEndpointV1::Outcome {
            outcome: outcome.clone(),
        },
    };

    let source_after = source_paths.load()?;
    validate_manifest_source_v1(&manifest, &source_after)?;
    if source_after.head.state_revision != source_before.head.state_revision {
        return Err("counterfactual branch source state changed during capture".to_string());
    }
    let root_public_snapshot_id = source_after
        .head
        .current_decision
        .as_ref()
        .ok_or_else(|| "counterfactual branch source root has no public decision".to_string())?
        .public_snapshot_id
        .clone();
    if root_public_snapshot_id != source_identity.root_public_snapshot_id {
        return Err(
            "counterfactual branch root public snapshot mismatches its checkpoint".to_string(),
        );
    }
    let target_after = target_paths.load()?;
    if target_after.head.head_revision != target.head.head_revision
        || target_after.head.state_revision != target.head.state_revision
        || target_after.head.position != target.head.position
    {
        return Err("counterfactual branch probe changed during capture".to_string());
    }
    let target_control_after = load_session_control_v1(&target_paths)?;
    if target_control_after.lifecycle != RunPlaySessionLifecycleV1::Parked
        || target_control_after.purpose != RunPlaySessionPurposeV1::Probe
    {
        return Err("counterfactual branch probe control changed during capture".to_string());
    }

    let artifact = CounterfactualBranchEvidenceV1 {
        schema: COUNTERFACTUAL_BRANCH_EVIDENCE_SCHEMA_V1.to_string(),
        evidence_scope: COUNTERFACTUAL_BRANCH_EVIDENCE_SCOPE_V1.to_string(),
        authority: CounterfactualBranchAuthorityV1::none_v1(),
        source_root: CounterfactualBranchRootV1 {
            session_path: canonical_source.to_string_lossy().to_string(),
            session_id: source_after.head.session_id.clone(),
            head_revision: source_after.head.head_revision,
            state_revision: source_after.head.state_revision,
            archive_node_id: root_node_id.to_string(),
            public_snapshot_id: root_public_snapshot_id,
        },
        branch_endpoint: CounterfactualBranchEndpointIdentityV1 {
            session_path: canonical_target.to_string_lossy().to_string(),
            session_id: target.head.session_id.clone(),
            head_revision: target.head.head_revision,
            state_revision: target.head.state_revision,
            archive_node_id: target.head.position.archive_node_id().to_string(),
        },
        capture_invariants: CounterfactualBranchCaptureInvariantsV1 {
            registered_direct_child: true,
            source_head_unchanged: true,
            root_exact_checkpoint_identity_equal: true,
            branch_endpoint_descends_from_root: true,
            branch_parked: true,
            branch_purpose_probe: true,
        },
        steps,
        endpoint,
    };
    artifact.validate()?;
    Ok(artifact)
}

pub fn write_counterfactual_branch_evidence_v1(
    output: &Path,
    managed_root: &Path,
    artifact: &CounterfactualBranchEvidenceV1,
) -> Result<(), String> {
    artifact.validate()?;
    let managed_root = fs::canonicalize(managed_root).map_err(|error| {
        format!(
            "failed to resolve managed run-play root {}: {error}",
            managed_root.display()
        )
    })?;
    let absolute_output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?
            .join(output)
    };
    if absolute_output
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(
            "counterfactual branch evidence output must not contain parent traversal".to_string(),
        );
    }
    if absolute_output.starts_with(&managed_root) {
        return Err(format!(
            "counterfactual branch evidence output must be outside managed root {}",
            managed_root.display()
        ));
    }
    let parent = absolute_output
        .parent()
        .ok_or_else(|| "counterfactual branch evidence output has no parent".to_string())?;
    let mut existing_ancestor = parent;
    loop {
        match fs::symlink_metadata(existing_ancestor) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                existing_ancestor = existing_ancestor.parent().ok_or_else(|| {
                    "counterfactual branch evidence output has no existing ancestor".to_string()
                })?;
            }
            Err(error) => {
                return Err(format!(
                    "failed to inspect counterfactual branch evidence directory {}: {error}",
                    existing_ancestor.display()
                ));
            }
        }
    }
    let canonical_ancestor = fs::canonicalize(existing_ancestor).map_err(|error| {
        format!(
            "failed to resolve counterfactual branch evidence ancestor {}: {error}",
            existing_ancestor.display()
        )
    })?;
    if canonical_ancestor.starts_with(&managed_root) {
        return Err(format!(
            "counterfactual branch evidence output must be outside managed root {}",
            managed_root.display()
        ));
    }
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create counterfactual branch evidence directory {}: {error}",
            parent.display()
        )
    })?;
    let parent = fs::canonicalize(parent).map_err(|error| {
        format!(
            "failed to resolve counterfactual branch evidence directory {}: {error}",
            parent.display()
        )
    })?;
    if parent.starts_with(&managed_root) {
        return Err(format!(
            "counterfactual branch evidence output must be outside managed root {}",
            managed_root.display()
        ));
    }
    let output = parent.join(
        absolute_output
            .file_name()
            .ok_or_else(|| "counterfactual branch evidence output has no file name".to_string())?,
    );
    let mut payload = serde_json::to_vec_pretty(artifact)
        .map_err(|error| format!("failed to encode counterfactual branch evidence: {error}"))?;
    payload.push(b'\n');
    write_new_synced(&output, &payload, "counterfactual branch evidence")
}

fn validate_manifest_source_v1(
    manifest: &ManagedWorkspaceManifestV1,
    source: &RunPlayWorkspaceV1,
) -> Result<(), String> {
    if source.head.session_id != manifest.source.session_id
        || source.head.head_revision != manifest.source.head_revision
        || source.head.position.archive_node_id() != manifest.source.archive_node_id
    {
        return Err("counterfactual branch source no longer matches its manifest root".to_string());
    }
    Ok(())
}

fn checkpoint_identity_v1(
    workspace: &RunPlayWorkspaceV1,
    archive_node_id: &str,
) -> Result<TaskRunBranchCheckpointIdentityV1, String> {
    let checkpoints = TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive(
        &workspace.state.checkpoint_payload,
        &workspace.state.archive,
        DEFAULT_ARTIFACT_MAX_BYTES,
    )?;
    if let Some(identity) = checkpoints
        .identities()
        .find(|identity| identity.archive_node_id == archive_node_id)
        .cloned()
    {
        return Ok(identity);
    }

    // Older or append-only workspaces may retain a public root that is exact
    // and replayable without having compacted a checkpoint at that particular
    // node. Counterfactual capture is read-only, so reconstruct the same root
    // through the production exact owner and materialize its identity only in
    // process. The workspace is never published or rewritten here.
    let (exact, _) = ExactRunPlaySessionV1::open_in_validated_session_profiled_v1(
        workspace.state.archive.clone(),
        checkpoints,
        archive_node_id,
        workspace.head.default_budget,
        workspace.head.combat_plan_options.clone(),
        ExactBranchingTaskEvidenceRequestV1::default(),
    )
    .map_err(|error| {
        format!(
            "counterfactual branch root {archive_node_id} has no durable checkpoint and could not be restored exactly: {error}"
        )
    })?;
    let prepared = exact.prepare_current_checkpoint_v1().map_err(|error| {
        format!(
            "counterfactual branch root {archive_node_id} could not materialize an exact checkpoint identity: {error}"
        )
    })?;
    let identity = prepared
        .checkpoints()
        .identities()
        .find(|identity| identity.archive_node_id == archive_node_id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "counterfactual branch root {archive_node_id} exact restore produced no checkpoint identity"
            )
        })?;
    Ok(identity)
}

fn archive_node_path_v1(
    workspace: &RunPlayWorkspaceV1,
    endpoint: &str,
) -> Result<Vec<String>, String> {
    let mut path = Vec::new();
    let mut cursor = Some(endpoint);
    while let Some(node_id) = cursor {
        let node = workspace
            .state
            .archive
            .node(node_id)
            .ok_or_else(|| format!("counterfactual branch path misses node {node_id}"))?;
        path.push(node.node_id.clone());
        cursor = node.parent_node_id.as_deref();
    }
    path.reverse();
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_learning::agent::task::TaskBudgetV1;
    use sts_oracle_learning::content::events::neow::NeowEligibilityV1;
    use sts_oracle_learning::eval::run_control::{
        CombatResearchPlanOptionsV1, CompleteTurnProposalBeamResearchOptionsV1,
    };

    use crate::driver::{create_natural_session_v1, fork_session_v1, RunPlayControllerV1};
    use crate::selector::parse_action_selector_v1;
    use crate::session_control::{describe_session_v1, park_session_v1, unpark_session_v1};
    use crate::workspace_lifecycle::{prepare_disposable_target_v1, register_disposable_fork_v1};

    struct TestSandboxV1(PathBuf);

    impl Drop for TestSandboxV1 {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fresh_test_root_v1(label: &str) -> TestSandboxV1 {
        TestSandboxV1(std::env::temp_dir().join(format!(
            "sts_run_play_counterfactual_branch_{label}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        )))
    }

    #[test]
    fn parked_probe_captures_a_read_only_exact_root_bound_suffix() {
        let sandbox = fresh_test_root_v1("capture");
        let source = sandbox.0.join("source");
        let managed_root = sandbox.0.join("managed");
        let target = managed_root.join("probe");
        let source_paths = RunPlayPathsV1::new(&source);
        let source_workspace = create_natural_session_v1(
            &source_paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 4,
                ascension_level: 20,
                neow_eligibility: NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 4096,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1 {
                complete_turn_proposal_beam: Some(
                    CompleteTurnProposalBeamResearchOptionsV1::default(),
                ),
                ..CombatResearchPlanOptionsV1::default()
            },
        )
        .expect("source");
        assert_eq!(
            source_workspace.head.evidence_identity,
            Some(crate::model::RunPlayEvidenceIdentityV1::current_v1()),
            "a fresh natural root carries the current evidence contract"
        );
        let prepared =
            prepare_disposable_target_v1(&managed_root, &target).expect("prepared target");
        let target_paths = RunPlayPathsV1::new(&target);
        let target_workspace =
            fork_session_v1(source_paths.clone(), &target_paths, None).expect("fork");
        assert_eq!(
            target_workspace.head.evidence_identity, source_workspace.head.evidence_identity,
            "a fork inherits its source evidence contract instead of being re-blessed"
        );
        register_disposable_fork_v1(
            &prepared,
            &source,
            &source_workspace.head,
            &target_workspace.head,
            "after evidence capture",
        )
        .expect("manifest");
        let selector = parse_action_selector_v1(
            "event:Neow:1",
            "counterfactual test",
            sts_presentation::Locale::En,
        )
        .expect("selector");
        let mut controller = RunPlayControllerV1::open(target_paths.clone()).expect("controller");
        controller
            .choose_action_v1(&selector, true)
            .expect("Neow's Lament branch");
        let leave = parse_action_selector_v1(
            "event:Neow:0",
            "counterfactual test",
            sts_presentation::Locale::En,
        )
        .expect("leave");
        controller
            .choose_action_v1(&leave, true)
            .expect("explicit completed-Neow Leave");
        park_session_v1(&target_paths, "probe complete".to_string()).expect("park");

        let wrong_purpose = capture_counterfactual_branch_evidence_v1(
            &target,
            &managed_root,
            DEFAULT_COUNTERFACTUAL_BRANCH_MAX_STEPS_V1,
        )
        .expect_err("unknown-purpose workspace must not become evidence");
        assert!(wrong_purpose.contains("purpose probe"));

        describe_session_v1(&target_paths, RunPlaySessionPurposeV1::Probe, None)
            .expect("describe probe");
        let artifact = capture_counterfactual_branch_evidence_v1(
            &target,
            &managed_root,
            DEFAULT_COUNTERFACTUAL_BRANCH_MAX_STEPS_V1,
        )
        .expect("capture");
        assert_eq!(artifact.evidence_scope, "counterfactual_reachability_only");
        assert!(!artifact.authority.has_any_authority_v1());
        assert_eq!(artifact.source_root.head_revision, 0);
        assert_eq!(artifact.steps.len(), 2);
        assert_eq!(artifact.steps[0].semantic_action, "event:Neow:1");
        assert_eq!(artifact.steps[1].semantic_action, "event:Neow:0");
        assert_eq!(
            artifact.steps[0].from_archive_node_id,
            artifact.source_root.archive_node_id
        );
        assert_eq!(
            artifact.steps[1].to_archive_node_id,
            artifact.branch_endpoint.archive_node_id
        );
        assert!(matches!(
            artifact.endpoint,
            AgentProbeEndpointV1::Decision {
                site: TaskDecisionSiteV1::Map,
                ..
            }
        ));
        assert_eq!(
            source_paths.load_head().expect("source head").head_revision,
            0
        );

        let managed_output_parent = managed_root.join("must-not-create");
        let managed_output = managed_output_parent.join("must-not-write.json");
        let managed_output_error =
            write_counterfactual_branch_evidence_v1(&managed_output, &managed_root, &artifact)
                .expect_err("evidence must not be written into the managed root");
        assert!(managed_output_error.contains("outside managed root"));
        assert!(!managed_output.exists());
        assert!(!managed_output_parent.exists());

        unpark_session_v1(&target_paths, "continue probing".to_string()).expect("unpark");
        let open_probe = capture_counterfactual_branch_evidence_v1(
            &target,
            &managed_root,
            DEFAULT_COUNTERFACTUAL_BRANCH_MAX_STEPS_V1,
        )
        .expect_err("open probe must not become closed evidence");
        assert!(open_probe.contains("parked probe"));
    }

    #[test]
    fn replayable_source_root_is_materialized_read_only_for_capture() {
        let sandbox = fresh_test_root_v1("replayable_source_root");
        let source = sandbox.0.join("source");
        let managed_root = sandbox.0.join("managed");
        let target = managed_root.join("probe");
        let source_paths = RunPlayPathsV1::new(&source);
        create_natural_session_v1(
            &source_paths,
            sts_oracle_learning::eval::run_control::RunControlConfig {
                seed: 9,
                ascension_level: 20,
                neow_eligibility: NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            TaskBudgetV1 {
                max_world_transitions: 4096,
                max_reloads: 0,
            },
            CombatResearchPlanOptionsV1 {
                complete_turn_proposal_beam: Some(
                    CompleteTurnProposalBeamResearchOptionsV1::default(),
                ),
                ..CombatResearchPlanOptionsV1::default()
            },
        )
        .expect("source");
        let mut source_controller =
            RunPlayControllerV1::open(source_paths.clone()).expect("source controller");
        let proceed = parse_action_selector_v1(
            "event:Neow:0",
            "advance source root",
            sts_presentation::Locale::En,
        )
        .expect("selector");
        source_controller
            .choose_action_v1(&proceed, true)
            .expect("advance to replayable source root");
        drop(source_controller);

        let source_before = source_paths.load().expect("source before fork");
        let source_root = source_before.head.position.archive_node_id().to_string();
        let durable_source_checkpoints =
            TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive(
                &source_before.state.checkpoint_payload,
                &source_before.state.archive,
                DEFAULT_ARTIFACT_MAX_BYTES,
            )
            .expect("source checkpoints");
        assert!(
            !durable_source_checkpoints
                .identities()
                .any(|identity| identity.archive_node_id == source_root),
            "regression fixture must begin at a replayable public-only root"
        );

        let prepared =
            prepare_disposable_target_v1(&managed_root, &target).expect("prepared target");
        let target_paths = RunPlayPathsV1::new(&target);
        let target_workspace =
            fork_session_v1(source_paths.clone(), &target_paths, None).expect("fork");
        let target_checkpoints = TaskRunBranchCheckpointArtifactV1::decode_workspace_for_archive(
            &target_workspace.state.checkpoint_payload,
            &target_workspace.state.archive,
            DEFAULT_ARTIFACT_MAX_BYTES,
        )
        .expect("target checkpoints");
        assert!(
            target_checkpoints
                .identities()
                .any(|identity| identity.archive_node_id == source_root),
            "fresh forks must persist their exact root"
        );
        register_disposable_fork_v1(
            &prepared,
            &source,
            &source_before.head,
            &target_workspace.head,
            "after evidence capture",
        )
        .expect("manifest");

        let suffix_choice = target_workspace
            .head
            .current_decision
            .as_ref()
            .expect("target decision")
            .candidates
            .iter()
            .find_map(|candidate| candidate.direct_choice.clone())
            .expect("direct suffix choice");
        let mut target_controller =
            RunPlayControllerV1::open(target_paths.clone()).expect("target controller");
        target_controller
            .choose(suffix_choice, None, true)
            .expect("advance probe");
        drop(target_controller);
        describe_session_v1(&target_paths, RunPlaySessionPurposeV1::Probe, None)
            .expect("describe probe");
        park_session_v1(&target_paths, "probe complete".to_string()).expect("park");

        let artifact = capture_counterfactual_branch_evidence_v1(
            &target,
            &managed_root,
            DEFAULT_COUNTERFACTUAL_BRANCH_MAX_STEPS_V1,
        )
        .expect("capture from replayable source root");
        assert_eq!(artifact.source_root.archive_node_id, source_root);
        assert!(
            artifact
                .capture_invariants
                .root_exact_checkpoint_identity_equal
        );
        assert_eq!(
            source_paths.load_head().expect("source head").head_revision,
            source_before.head.head_revision,
            "read-only materialization must not publish into the source"
        );
    }
}
