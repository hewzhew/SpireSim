use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::ManagedWorkspaceManifestV1;
use crate::storage::read_bounded;

pub(super) fn read_manifest_v1(path: &Path) -> Result<ManagedWorkspaceManifestV1, String> {
    let payload = read_bounded(path, 64 * 1024, "managed workspace manifest")?;
    serde_json::from_slice(&payload).map_err(|error| {
        format!(
            "failed to decode managed workspace manifest {}: {error}",
            path.display()
        )
    })
}

pub(super) fn validate_workspace_shape_v1(path: &Path) -> Result<(), String> {
    for child in ["heads", "states", "deltas"] {
        if !path.join(child).is_dir() {
            return Err(format!("managed workspace is missing {child}/"));
        }
    }
    Ok(())
}

pub(super) fn reject_links_in_tree_v1(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err("managed workspace contains a filesystem link".to_string());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to inspect workspace entry: {error}"))?;
        reject_links_in_tree_v1(&entry.path())?;
    }
    Ok(())
}

pub(super) fn directory_size_v1(path: &Path) -> Result<u64, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err("managed workspace contains a filesystem link".to_string());
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut bytes = 0u64;
    for entry in fs::read_dir(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to inspect workspace entry: {error}"))?;
        bytes = bytes.saturating_add(directory_size_v1(&entry.path())?);
    }
    Ok(bytes)
}

pub(super) fn canonical_direct_child_v1(root: &Path, candidate: &Path) -> Result<PathBuf, String> {
    let canonical = canonical_directory_v1(candidate, "managed workspace")?;
    if canonical.parent() != Some(root) {
        return Err(format!(
            "managed workspace is not a direct child of {}",
            root.display()
        ));
    }
    Ok(canonical)
}

pub(super) fn canonical_directory_v1(path: &Path, description: &str) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        format!(
            "failed to resolve {description} {}: {error}",
            path.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "{description} is not a directory: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

pub(super) fn resolve_output_outside_root_v1(
    root: &Path,
    output: &Path,
) -> Result<PathBuf, String> {
    let absolute = absolute_path_v1(output)?;
    if absolute.starts_with(root) {
        return Err(format!(
            "workspace GC receipt must be outside managed root {}",
            root.display()
        ));
    }
    let parent = absolute
        .parent()
        .ok_or_else(|| "workspace GC receipt has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create workspace GC receipt directory {}: {error}",
            parent.display()
        )
    })?;
    let parent = canonical_directory_v1(parent, "workspace GC receipt directory")?;
    let name = absolute
        .file_name()
        .ok_or_else(|| "workspace GC receipt has no file name".to_string())?;
    let output = parent.join(name);
    if output.starts_with(root) {
        return Err(format!(
            "workspace GC receipt must be outside managed root {}",
            root.display()
        ));
    }
    Ok(output)
}

pub(super) fn canonical_file_outside_root_v1(
    root: &Path,
    path: &Path,
    description: &str,
) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        format!(
            "failed to resolve {description} {}: {error}",
            path.display()
        )
    })?;
    if !canonical.is_file() {
        return Err(format!(
            "{description} is not a file: {}",
            canonical.display()
        ));
    }
    if canonical.starts_with(root) {
        return Err(format!(
            "{description} must be outside managed root {}",
            root.display()
        ));
    }
    Ok(canonical)
}

pub(super) fn absolute_path_v1(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(|error| format!("failed to resolve current directory: {error}"))
    }
}

pub(super) fn path_text_v1(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub(super) fn write_json_new_v1<T: Serialize>(
    path: &Path,
    value: &T,
    description: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create {description} directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let payload = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode {description}: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create {description} {}: {error}", path.display()))?;
    file.write_all(&payload)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("failed to write {description} {}: {error}", path.display()))
}

pub(super) fn create_jsonl_receipt_v1(path: &Path) -> Result<File, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create workspace GC receipt directory {}: {error}",
                parent.display()
            )
        })?;
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            format!(
                "failed to create workspace GC execution receipt {}: {error}",
                path.display()
            )
        })
}

pub(super) fn append_jsonl_v1<T: Serialize>(file: &mut File, value: &T) -> Result<(), String> {
    let payload = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode workspace GC execution receipt: {error}"))?;
    file.write_all(&payload)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_data())
        .map_err(|error| format!("failed to append workspace GC execution receipt: {error}"))
}
