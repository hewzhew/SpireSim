use std::{fs, path::Path};

use serde::Serialize;
use serde_json::Value;
use sts_oracle_runtime::eval::combat_lab_v1::atomic_write_json;
use sts_oracle_runtime::eval::run_control::{
    exact_replay_run_progress_journal_v1, run_progress_journal_fingerprint_v1,
    ExactRunProgressReplayReportV1,
};
use sts_oracle_runtime::runtime::branch::{
    load_oracle_analysis_workspace_v1, save_oracle_analysis_workspace_v1,
    OracleAnalysisWorkspaceArtifactV1, OracleAnalysisWorkspaceV1,
};

use super::workspace_commands::encode;

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceCompactionReceiptV1<'a> {
    schema_name: &'static str,
    schema_version: u32,
    source_workspace: &'a Path,
    source_node_id: usize,
    source_node_count: usize,
    source_bytes: u64,
    output_workspace: &'a Path,
    output_node_id: usize,
    output_node_count: usize,
    output_bytes: u64,
    bytes_removed: u64,
    reduction_basis_points: u64,
    journal_entries: usize,
    journal_fingerprint: String,
    final_state_fingerprint: String,
    source_workspace_modified: bool,
    exact_roundtrip_verified: bool,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceRepackReceiptV1<'a> {
    schema_name: &'static str,
    schema_version: u32,
    source_workspace: &'a Path,
    source_bytes: u64,
    output_workspace: &'a Path,
    output_bytes: u64,
    bytes_removed: u64,
    reduction_basis_points: u64,
    node_count: usize,
    source_workspace_modified: bool,
    complete_tree_preserved: bool,
    artifact_write_verified: bool,
    workspace_restore_verified: bool,
}

struct VerifiedWorkspaceNodeV1 {
    node_id: usize,
    journal_entries: usize,
    journal_fingerprint: String,
    replay: ExactRunProgressReplayReportV1,
}

fn verify_workspace_node(
    analysis: &OracleAnalysisWorkspaceV1,
    node_id: usize,
) -> Result<VerifiedWorkspaceNodeV1, String> {
    let continuation = analysis.continuation(node_id)?;
    let expected = continuation.session.clone().into_session()?;
    let replay = exact_replay_run_progress_journal_v1(
        analysis.seed,
        analysis.ascension,
        &continuation.journal,
        &expected,
    )?;
    Ok(VerifiedWorkspaceNodeV1 {
        node_id,
        journal_entries: continuation.journal.len(),
        journal_fingerprint: run_progress_journal_fingerprint_v1(&continuation.journal),
        replay,
    })
}

fn require_same_exact_node(
    source: &VerifiedWorkspaceNodeV1,
    candidate: &VerifiedWorkspaceNodeV1,
    context: &str,
) -> Result<(), String> {
    if source.replay == candidate.replay
        && source.journal_fingerprint == candidate.journal_fingerprint
    {
        Ok(())
    } else {
        Err(format!("{context} failed exact-state verification"))
    }
}

pub(super) fn repack_workspace(workspace: &Path, output: &Path) -> Result<Value, String> {
    if output.exists() {
        return Err(format!(
            "oracle repack workspace output already exists: '{}'",
            output.display()
        ));
    }
    let source_bytes = fs::metadata(workspace)
        .map_err(|error| format!("failed to inspect '{}': {error}", workspace.display()))?
        .len();
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let node_count = analysis.session.tree().nodes.len();
    let expected_artifact = analysis.artifact()?;
    let expected_bytes = serde_json::to_vec(&expected_artifact)
        .map_err(|error| format!("failed to encode pooled workspace checkpoint: {error}"))?;

    atomic_write_json(output, &expected_artifact)?;
    let written_bytes = fs::read(output)
        .map_err(|error| format!("failed to read '{}': {error}", output.display()))?;
    if expected_bytes != written_bytes {
        return Err("written oracle repack workspace failed artifact write verification".into());
    }
    let written_artifact =
        serde_json::from_slice::<OracleAnalysisWorkspaceArtifactV1>(&written_bytes)
            .map_err(|error| format!("failed to parse written pooled workspace: {error}"))?;
    let restored = OracleAnalysisWorkspaceV1::restore(written_artifact)?;
    let source_tree = serde_json::to_vec(&analysis.session.tree())
        .map_err(|error| format!("failed to encode source workspace tree: {error}"))?;
    let restored_tree = serde_json::to_vec(&restored.session.tree())
        .map_err(|error| format!("failed to encode restored workspace tree: {error}"))?;
    if source_tree != restored_tree {
        return Err("written oracle repack workspace changed the variation tree".into());
    }

    let output_bytes = fs::metadata(output)
        .map_err(|error| format!("failed to inspect '{}': {error}", output.display()))?
        .len();
    let bytes_removed = source_bytes.saturating_sub(output_bytes);
    let reduction_basis_points = if source_bytes == 0 {
        0
    } else {
        bytes_removed.saturating_mul(10_000) / source_bytes
    };
    encode(WorkspaceRepackReceiptV1 {
        schema_name: "OracleAnalysisWorkspaceRepackReceipt",
        schema_version: 1,
        source_workspace: workspace,
        source_bytes,
        output_workspace: output,
        output_bytes,
        bytes_removed,
        reduction_basis_points,
        node_count,
        source_workspace_modified: false,
        complete_tree_preserved: true,
        artifact_write_verified: true,
        workspace_restore_verified: true,
    })
}

pub(super) fn compact_workspace(
    workspace: &Path,
    node: Option<usize>,
    output: &Path,
) -> Result<Value, String> {
    if output.exists() {
        return Err(format!(
            "oracle compact workspace output already exists: '{}'",
            output.display()
        ));
    }
    let source_bytes = fs::metadata(workspace)
        .map_err(|error| format!("failed to inspect '{}': {error}", workspace.display()))?
        .len();
    let analysis = load_oracle_analysis_workspace_v1(workspace)?;
    let source_node_id = node.unwrap_or_else(|| analysis.session.cursor_node_id());
    let source_node_count = analysis.session.tree().nodes.len();
    let source = verify_workspace_node(&analysis, source_node_id)?;

    let compact = analysis.compact_from_node(source_node_id)?;
    let output_node_id = compact.session.cursor_node_id();
    let output_node_count = compact.session.tree().nodes.len();
    let in_memory = verify_workspace_node(&compact, output_node_id)?;
    require_same_exact_node(&source, &in_memory, "oracle compact workspace")?;

    save_oracle_analysis_workspace_v1(output, &compact)?;
    let restored = load_oracle_analysis_workspace_v1(output)?;
    let restored_node_id = restored.session.cursor_node_id();
    let restored = verify_workspace_node(&restored, restored_node_id)?;
    require_same_exact_node(&source, &restored, "written oracle compact workspace")?;

    let output_bytes = fs::metadata(output)
        .map_err(|error| format!("failed to inspect '{}': {error}", output.display()))?
        .len();
    let bytes_removed = source_bytes.saturating_sub(output_bytes);
    let reduction_basis_points = if source_bytes == 0 {
        0
    } else {
        bytes_removed.saturating_mul(10_000) / source_bytes
    };
    encode(WorkspaceCompactionReceiptV1 {
        schema_name: "OracleAnalysisWorkspaceCompactionReceipt",
        schema_version: 1,
        source_workspace: workspace,
        source_node_id,
        source_node_count,
        source_bytes,
        output_workspace: output,
        output_node_id: restored.node_id,
        output_node_count,
        output_bytes,
        bytes_removed,
        reduction_basis_points,
        journal_entries: restored.journal_entries,
        journal_fingerprint: restored.journal_fingerprint,
        final_state_fingerprint: restored.replay.final_fingerprint,
        source_workspace_modified: false,
        exact_roundtrip_verified: true,
    })
}
