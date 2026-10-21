use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

const ARTIFACT_SCHEMA_V1: &str = "sts-workbench-artifact-v1";
const INSPECT_RESPONSE_SCHEMA_V1: &str = "sts-workbench-inspect-response-v1";
const ARTIFACT_TYPES: [&str; 5] = [
    "paired_root_action_observation",
    "paired_root_search_detail",
    "paired_root_search_witness",
    "paired_root_preview_detail",
    "paired_root_commit_detail",
];

#[derive(Debug, Deserialize)]
struct ArtifactEnvelopeV1 {
    schema: String,
    handle: String,
    artifact_type: String,
    document_sha256: String,
    document: Value,
}

pub fn publish_artifact(
    repository: &Path,
    artifact_type: &str,
    document: &Value,
) -> Result<String, String> {
    let canonical_document = canonical_json(document)?;
    let mut handle_hasher = Sha256::new();
    handle_hasher.update(artifact_type.as_bytes());
    handle_hasher.update([0]);
    handle_hasher.update(&canonical_document);
    let handle = format!("workbench-artifact-v1-{:x}", handle_hasher.finalize());
    let document_sha256 = format!("{:x}", Sha256::digest(&canonical_document));
    let envelope = json!({
        "schema": ARTIFACT_SCHEMA_V1,
        "handle": handle,
        "artifact_type": artifact_type,
        "document_sha256": document_sha256,
        "document": document,
    });
    let encoded = canonical_json(&envelope)?;
    let directory = repository
        .join(".oracle-lab")
        .join("workbench-service")
        .join("artifacts");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("failed to create workbench artifact directory: {error}"))?;
    let path = directory.join(format!("{handle}.json"));
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => file
            .write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("failed to publish workbench artifact: {error}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(&path)
                .map_err(|error| format!("failed to verify existing artifact: {error}"))?;
            if existing != encoded {
                return Err(format!("workbench artifact collision: {handle}"));
            }
        }
        Err(error) => return Err(format!("failed to create workbench artifact: {error}")),
    }
    Ok(handle)
}

pub fn inspect_artifact(repository: &Path, handle: &str) -> Result<Value, String> {
    if !content_id_has_shape(handle, "workbench-artifact-v1") {
        return Err("invalid workbench artifact handle".to_owned());
    }
    let directory = repository
        .join(".oracle-lab")
        .join("workbench-service")
        .join("artifacts");
    let directory = fs::canonicalize(&directory)
        .map_err(|error| format!("failed to resolve workbench artifact directory: {error}"))?;
    let requested_path = directory.join(format!("{handle}.json"));
    let path = match fs::canonicalize(&requested_path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("workbench artifact does not exist: {handle}"));
        }
        Err(error) => {
            return Err(format!(
                "failed to resolve workbench artifact {handle}: {error}"
            ));
        }
    };
    if path.parent() != Some(directory.as_path()) || !path.is_file() {
        return Err(format!("workbench artifact does not exist: {handle}"));
    }
    let encoded = fs::read(&path)
        .map_err(|error| format!("failed to read workbench artifact {handle}: {error}"))?;
    let envelope: ArtifactEnvelopeV1 = serde_json::from_slice(&encoded)
        .map_err(|error| format!("invalid workbench artifact envelope {handle}: {error}"))?;
    if envelope.schema != ARTIFACT_SCHEMA_V1
        || envelope.handle != handle
        || !ARTIFACT_TYPES.contains(&envelope.artifact_type.as_str())
        || !envelope.document.is_object()
    {
        return Err(format!("invalid workbench artifact envelope: {handle}"));
    }
    let canonical_document = canonical_json(&envelope.document)?;
    let document_sha256 = format!("{:x}", Sha256::digest(&canonical_document));
    if document_sha256 != envelope.document_sha256 {
        return Err(format!("workbench artifact content hash changed: {handle}"));
    }
    let mut handle_hasher = Sha256::new();
    handle_hasher.update(envelope.artifact_type.as_bytes());
    handle_hasher.update([0]);
    handle_hasher.update(&canonical_document);
    let expected_handle = format!("workbench-artifact-v1-{:x}", handle_hasher.finalize());
    if expected_handle != handle {
        return Err(format!("workbench artifact handle changed: {handle}"));
    }
    Ok(json!({
        "schema": INSPECT_RESPONSE_SCHEMA_V1,
        "handle": handle,
        "artifact_type": envelope.artifact_type,
        "document": envelope.document
    }))
}

fn canonical_json(value: &Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| format!("failed to encode canonical JSON: {error}"))
}

fn content_id_has_shape(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|tail| tail.strip_prefix('-'))
        .is_some_and(is_sha256)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
