use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

mod contract;
mod fs_support;
#[cfg(test)]
mod tests;

use crate::model::RunPlayHeadV1;
use crate::storage::read_bounded;
use contract::{validate_manifest_v1, validate_plan_v1, validate_short_text_v1};
use fs_support::{
    absolute_path_v1, append_jsonl_v1, canonical_direct_child_v1, canonical_directory_v1,
    canonical_file_outside_root_v1, create_jsonl_receipt_v1, directory_size_v1, path_text_v1,
    read_manifest_v1, reject_links_in_tree_v1, resolve_output_outside_root_v1,
    validate_workspace_shape_v1, write_json_new_v1,
};

const MANAGED_WORKSPACE_MANIFEST_SCHEMA_V1: &str = "sts-run-play-managed-workspace-v1";
const WORKSPACE_GC_SUMMARY_SCHEMA_V2: &str = "sts-run-play-workspace-gc-summary-v2";
const WORKSPACE_GC_PLAN_SCHEMA_V2: &str = "sts-run-play-workspace-gc-plan-v2";
const WORKSPACE_GC_EXECUTION_START_SCHEMA_V2: &str = "sts-run-play-workspace-gc-execution-start-v2";
const WORKSPACE_GC_EXECUTION_ITEM_SCHEMA_V1: &str = "sts-run-play-workspace-gc-execution-item-v1";
const WORKSPACE_GC_EXECUTION_COMPLETE_SCHEMA_V1: &str =
    "sts-run-play-workspace-gc-execution-complete-v1";
const MANAGED_WORKSPACE_MANIFEST_FILE_V1: &str = "managed-workspace.json";

#[derive(Clone, Debug)]
pub struct PreparedDisposableTargetV1 {
    managed_root: PathBuf,
    target: PathBuf,
    workspace_name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedWorkspaceSourceV1 {
    pub session_path: String,
    pub session_id: String,
    pub head_revision: u64,
    pub archive_node_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedWorkspaceManifestV1 {
    pub schema: String,
    pub kind: String,
    pub workspace_name: String,
    pub target_session_id: String,
    pub source: ManagedWorkspaceSourceV1,
    pub created_unix_ms: u64,
    pub expected_cleanup_phase: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceGcTargetV1 {
    pub workspace_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub manifest: ManagedWorkspaceManifestV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceGcSummaryV1 {
    pub schema: String,
    pub managed_root: String,
    pub eligible: Vec<WorkspaceGcTargetV1>,
    pub eligible_bytes: u64,
    pub protected_focus: usize,
    pub unmanaged_directories: usize,
    pub invalid_directories: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceGcPlanV1 {
    pub schema: String,
    pub created_unix_ms: u64,
    pub managed_root: String,
    pub focus_session_at_plan: Option<String>,
    pub targets: Vec<WorkspaceGcTargetV1>,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
struct WorkspaceGcExecutionStartV1<'a> {
    schema: &'static str,
    started_unix_ms: u64,
    plan: String,
    managed_root: &'a str,
    focus_session_at_execution: Option<&'a str>,
    target_count: usize,
    total_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
struct WorkspaceGcExecutionItemV1<'a> {
    schema: &'static str,
    workspace_name: &'a str,
    path: &'a str,
    size_bytes: u64,
    outcome: &'a str,
    error: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize)]
struct WorkspaceGcExecutionCompleteV1 {
    schema: &'static str,
    completed_unix_ms: u64,
    deleted_count: usize,
    deleted_bytes: u64,
}

pub fn prepare_disposable_target_v1(
    managed_root: &Path,
    target: &Path,
) -> Result<PreparedDisposableTargetV1, String> {
    fs::create_dir_all(managed_root).map_err(|error| {
        format!(
            "failed to create managed run-play root {}: {error}",
            managed_root.display()
        )
    })?;
    let managed_root = canonical_directory_v1(managed_root, "managed run-play root")?;
    if target.exists() {
        return Err(format!(
            "managed run-play target already exists: {}",
            target.display()
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| "managed run-play target has no parent".to_string())?;
    let parent = canonical_directory_v1(parent, "managed run-play target parent")?;
    if parent != managed_root {
        return Err(format!(
            "disposable run-play target must be a direct child of {}",
            managed_root.display()
        ));
    }
    let workspace_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .ok_or_else(|| "managed run-play target has an invalid directory name".to_string())?
        .to_string();
    Ok(PreparedDisposableTargetV1 {
        target: managed_root.join(&workspace_name),
        managed_root,
        workspace_name,
    })
}

pub fn register_disposable_fork_v1(
    prepared: &PreparedDisposableTargetV1,
    source_path: &Path,
    source_head: &RunPlayHeadV1,
    target_head: &RunPlayHeadV1,
    expected_cleanup_phase: &str,
) -> Result<PathBuf, String> {
    validate_short_text_v1("expected cleanup phase", expected_cleanup_phase)?;
    let target = canonical_directory_v1(&prepared.target, "managed run-play target")?;
    if target.parent() != Some(prepared.managed_root.as_path()) {
        return Err("created disposable target escaped its managed root".to_string());
    }
    validate_workspace_shape_v1(&target)?;
    let source_path = canonical_directory_v1(source_path, "source run-play session")?;
    let manifest = ManagedWorkspaceManifestV1 {
        schema: MANAGED_WORKSPACE_MANIFEST_SCHEMA_V1.to_string(),
        kind: "disposable_fork".to_string(),
        workspace_name: prepared.workspace_name.clone(),
        target_session_id: target_head.session_id.clone(),
        source: ManagedWorkspaceSourceV1 {
            session_path: path_text_v1(&source_path),
            session_id: source_head.session_id.clone(),
            head_revision: source_head.head_revision,
            archive_node_id: target_head.position.archive_node_id().to_string(),
        },
        created_unix_ms: unix_ms_v1()?,
        expected_cleanup_phase: expected_cleanup_phase.trim().to_string(),
    };
    validate_manifest_v1(&manifest, &prepared.workspace_name)?;
    let path = target.join(MANAGED_WORKSPACE_MANIFEST_FILE_V1);
    write_json_new_v1(&path, &manifest, "managed workspace manifest")?;
    Ok(path)
}

/// Loads one explicitly named registered disposable fork without scanning or
/// mutating the rest of the managed root. Evidence readers use this boundary
/// so an arbitrary run-play directory cannot be presented as a managed probe.
pub fn load_registered_disposable_fork_v1(
    managed_root: &Path,
    target: &Path,
) -> Result<(PathBuf, ManagedWorkspaceManifestV1), String> {
    let managed_root = canonical_directory_v1(managed_root, "managed run-play root")?;
    let target_absolute = absolute_path_v1(target)?;
    reject_links_in_tree_v1(&target_absolute)?;
    let target = canonical_direct_child_v1(&managed_root, &target_absolute)?;
    validate_workspace_shape_v1(&target)?;
    let workspace_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "managed run-play target has an invalid directory name".to_string())?;
    let manifest = read_manifest_v1(&target.join(MANAGED_WORKSPACE_MANIFEST_FILE_V1))?;
    validate_manifest_v1(&manifest, workspace_name)?;
    Ok((target, manifest))
}

pub fn inspect_workspace_gc_v1(
    managed_root: &Path,
    focus_session: Option<&Path>,
) -> Result<WorkspaceGcSummaryV1, String> {
    if !managed_root.exists() {
        return Ok(WorkspaceGcSummaryV1 {
            schema: WORKSPACE_GC_SUMMARY_SCHEMA_V2.to_string(),
            managed_root: path_text_v1(&absolute_path_v1(managed_root)?),
            eligible: Vec::new(),
            eligible_bytes: 0,
            protected_focus: 0,
            unmanaged_directories: 0,
            invalid_directories: Vec::new(),
        });
    }
    let managed_root = canonical_directory_v1(managed_root, "managed run-play root")?;
    let focus_session = focus_session
        .map(|path| canonical_directory_v1(path, "focused run-play session"))
        .transpose()?;
    let mut entries = fs::read_dir(&managed_root)
        .map_err(|error| {
            format!(
                "failed to inspect managed run-play root {}: {error}",
                managed_root.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to inspect managed run-play entry: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());

    let mut eligible = Vec::new();
    let mut eligible_bytes = 0u64;
    let mut protected_focus = 0usize;
    let mut unmanaged_directories = 0usize;
    let mut invalid_directories = Vec::new();

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "failed to inspect managed run-play entry {}: {error}",
                path.display()
            )
        })?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        let manifest_path = path.join(MANAGED_WORKSPACE_MANIFEST_FILE_V1);
        if !manifest_path.is_file() {
            unmanaged_directories = unmanaged_directories.saturating_add(1);
            continue;
        }
        let workspace_name = entry.file_name().to_string_lossy().to_string();
        let candidate = (|| -> Result<WorkspaceGcTargetV1, String> {
            let canonical = canonical_direct_child_v1(&managed_root, &path)?;
            validate_workspace_shape_v1(&canonical)?;
            reject_links_in_tree_v1(&canonical)?;
            let manifest = read_manifest_v1(&manifest_path)?;
            validate_manifest_v1(&manifest, &workspace_name)?;
            let size_bytes = directory_size_v1(&canonical)?;
            Ok(WorkspaceGcTargetV1 {
                workspace_name: workspace_name.clone(),
                path: path_text_v1(&canonical),
                size_bytes,
                manifest,
            })
        })();
        let candidate = match candidate {
            Ok(candidate) => candidate,
            Err(error) => {
                invalid_directories.push(format!("{workspace_name}: {error}"));
                continue;
            }
        };
        let canonical = PathBuf::from(&candidate.path);
        if focus_session.as_ref() == Some(&canonical) {
            protected_focus = protected_focus.saturating_add(1);
            continue;
        }
        eligible_bytes = eligible_bytes.saturating_add(candidate.size_bytes);
        eligible.push(candidate);
    }

    Ok(WorkspaceGcSummaryV1 {
        schema: WORKSPACE_GC_SUMMARY_SCHEMA_V2.to_string(),
        managed_root: path_text_v1(&managed_root),
        eligible,
        eligible_bytes,
        protected_focus,
        unmanaged_directories,
        invalid_directories,
    })
}

pub fn write_workspace_gc_plan_v1(
    managed_root: &Path,
    focus_session: Option<&Path>,
    output: &Path,
) -> Result<WorkspaceGcPlanV1, String> {
    let summary = inspect_workspace_gc_v1(managed_root, focus_session)?;
    let root = PathBuf::from(&summary.managed_root);
    let output = resolve_output_outside_root_v1(&root, output)?;
    let focus_session_at_plan = focus_session
        .map(|path| canonical_directory_v1(path, "focused run-play session"))
        .transpose()?
        .map(|path| path_text_v1(&path));
    let plan = WorkspaceGcPlanV1 {
        schema: WORKSPACE_GC_PLAN_SCHEMA_V2.to_string(),
        created_unix_ms: unix_ms_v1()?,
        managed_root: summary.managed_root,
        focus_session_at_plan,
        targets: summary.eligible,
        total_bytes: summary.eligible_bytes,
    };
    validate_plan_v1(&plan)?;
    write_json_new_v1(&output, &plan, "workspace GC plan")?;
    Ok(plan)
}

pub fn execute_workspace_gc_plan_v1(
    managed_root: &Path,
    focus_session: Option<&Path>,
    plan_path: &Path,
    receipt_path: &Path,
) -> Result<(usize, u64), String> {
    let managed_root = canonical_directory_v1(managed_root, "managed run-play root")?;
    let plan_path = canonical_file_outside_root_v1(&managed_root, plan_path, "workspace GC plan")?;
    let payload = read_bounded(&plan_path, 8 * 1024 * 1024, "workspace GC plan")?;
    let plan: WorkspaceGcPlanV1 = serde_json::from_slice(&payload).map_err(|error| {
        format!(
            "failed to decode workspace GC plan {}: {error}",
            plan_path.display()
        )
    })?;
    validate_plan_v1(&plan)?;
    if path_text_v1(&managed_root) != plan.managed_root {
        return Err("workspace GC plan belongs to a different managed root".to_string());
    }
    let receipt_path = resolve_output_outside_root_v1(&managed_root, receipt_path)?;
    let focus_session = focus_session
        .map(|path| canonical_directory_v1(path, "focused run-play session"))
        .transpose()?;
    let focus_text = focus_session.as_ref().map(|path| path_text_v1(path));

    let mut seen = BTreeSet::new();
    for target in &plan.targets {
        let path = PathBuf::from(&target.path);
        let canonical = canonical_direct_child_v1(&managed_root, &path)?;
        if path_text_v1(&canonical) != target.path || !seen.insert(target.path.clone()) {
            return Err("workspace GC plan contains a moved or duplicate target".to_string());
        }
        if focus_session.as_ref() == Some(&canonical) {
            return Err(format!(
                "workspace GC target became focused after planning: {}",
                canonical.display()
            ));
        }
        validate_workspace_shape_v1(&canonical)?;
        reject_links_in_tree_v1(&canonical)?;
        let manifest = read_manifest_v1(&canonical.join(MANAGED_WORKSPACE_MANIFEST_FILE_V1))?;
        if manifest != target.manifest {
            return Err(format!(
                "workspace GC target manifest changed after planning: {}",
                canonical.display()
            ));
        }
    }

    let mut receipt = create_jsonl_receipt_v1(&receipt_path)?;
    append_jsonl_v1(
        &mut receipt,
        &WorkspaceGcExecutionStartV1 {
            schema: WORKSPACE_GC_EXECUTION_START_SCHEMA_V2,
            started_unix_ms: unix_ms_v1()?,
            plan: path_text_v1(&plan_path),
            managed_root: &plan.managed_root,
            focus_session_at_execution: focus_text.as_deref(),
            target_count: plan.targets.len(),
            total_bytes: plan.total_bytes,
        },
    )?;

    let mut deleted_count = 0usize;
    let mut deleted_bytes = 0u64;
    for target in &plan.targets {
        match fs::remove_dir_all(&target.path) {
            Ok(()) => {
                deleted_count = deleted_count.saturating_add(1);
                deleted_bytes = deleted_bytes.saturating_add(target.size_bytes);
                append_jsonl_v1(
                    &mut receipt,
                    &WorkspaceGcExecutionItemV1 {
                        schema: WORKSPACE_GC_EXECUTION_ITEM_SCHEMA_V1,
                        workspace_name: &target.workspace_name,
                        path: &target.path,
                        size_bytes: target.size_bytes,
                        outcome: "deleted",
                        error: None,
                    },
                )?;
            }
            Err(error) => {
                let message = error.to_string();
                append_jsonl_v1(
                    &mut receipt,
                    &WorkspaceGcExecutionItemV1 {
                        schema: WORKSPACE_GC_EXECUTION_ITEM_SCHEMA_V1,
                        workspace_name: &target.workspace_name,
                        path: &target.path,
                        size_bytes: target.size_bytes,
                        outcome: "failed",
                        error: Some(&message),
                    },
                )?;
                return Err(format!(
                    "failed to delete managed workspace {}: {error}; execution receipt={}",
                    target.path,
                    receipt_path.display()
                ));
            }
        }
    }
    append_jsonl_v1(
        &mut receipt,
        &WorkspaceGcExecutionCompleteV1 {
            schema: WORKSPACE_GC_EXECUTION_COMPLETE_SCHEMA_V1,
            completed_unix_ms: unix_ms_v1()?,
            deleted_count,
            deleted_bytes,
        },
    )?;
    Ok((deleted_count, deleted_bytes))
}

pub fn render_workspace_gc_summary_v1(summary: &WorkspaceGcSummaryV1) -> String {
    let eligible_names = summary
        .eligible
        .iter()
        .take(4)
        .map(|target| target.workspace_name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let suffix = if summary.eligible.len() > 4 {
        ",..."
    } else {
        ""
    };
    format!(
        "workspace_gc eligible={} bytes={} focus_protected={} unmanaged={} invalid={}{}{}\n",
        summary.eligible.len(),
        summary.eligible_bytes,
        summary.protected_focus,
        summary.unmanaged_directories,
        summary.invalid_directories.len(),
        if eligible_names.is_empty() {
            ""
        } else {
            " names=["
        },
        if eligible_names.is_empty() {
            String::new()
        } else {
            format!("{eligible_names}{suffix}]")
        }
    )
}

fn unix_ms_v1() -> Result<u64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock precedes Unix epoch: {error}"))?
        .as_millis();
    u64::try_from(millis).map_err(|_| "system time does not fit in u64 milliseconds".to_string())
}
