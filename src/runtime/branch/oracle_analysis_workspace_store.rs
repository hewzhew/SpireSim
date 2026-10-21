use std::fs;
use std::path::Path;

use crate::eval::combat_lab_v1::atomic_write_json;

use super::oracle_analysis::OracleAnalysisWorkspaceV1;
use super::oracle_analysis_workspace_contract::{
    OracleAnalysisWorkspaceArtifactV1, OracleAnalysisWorkspaceSaveTimingV1,
};

pub fn save_oracle_analysis_workspace_v1(
    path: &Path,
    workspace: &OracleAnalysisWorkspaceV1,
) -> Result<(), String> {
    save_oracle_analysis_workspace_with_timing_v1(path, workspace).map(|_| ())
}

pub fn save_oracle_analysis_workspace_with_timing_v1(
    path: &Path,
    workspace: &OracleAnalysisWorkspaceV1,
) -> Result<OracleAnalysisWorkspaceSaveTimingV1, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create oracle analysis directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let checkpoint_started = std::time::Instant::now();
    let artifact = workspace.artifact()?;
    let checkpoint_elapsed_ms = elapsed_millis(checkpoint_started);
    let write_started = std::time::Instant::now();
    atomic_write_json(path, &artifact)?;
    Ok(OracleAnalysisWorkspaceSaveTimingV1 {
        checkpoint_elapsed_ms,
        write_elapsed_ms: elapsed_millis(write_started),
    })
}

pub fn load_oracle_analysis_workspace_v1(path: &Path) -> Result<OracleAnalysisWorkspaceV1, String> {
    OracleAnalysisWorkspaceV1::restore(load_oracle_analysis_workspace_artifact_v1(path)?)
}

pub(super) fn load_oracle_analysis_workspace_artifact_v1(
    path: &Path,
) -> Result<OracleAnalysisWorkspaceArtifactV1, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read '{}': {error}", path.display()))?;
    serde_json::from_slice::<OracleAnalysisWorkspaceArtifactV1>(&bytes)
        .map_err(|error| format!("failed to parse '{}': {error}", path.display()))
}

fn elapsed_millis(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
