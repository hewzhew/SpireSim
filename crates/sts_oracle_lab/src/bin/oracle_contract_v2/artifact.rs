use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use serde_json::Value;

use super::{CombatContractArtifactV2, ARTIFACT_SCHEMA, ARTIFACT_SCHEMA_VERSION};

pub(super) struct ArtifactDirectoryReservation {
    pub(super) staging_path: PathBuf,
    pub(super) final_path: PathBuf,
}

pub(super) fn reserve_artifact_directory(
    root_hash: &str,
) -> Result<ArtifactDirectoryReservation, String> {
    let root = PathBuf::from(env!("STS_REPOSITORY_ROOT"))
        .join(".oracle-lab")
        .join("v2")
        .join("contracts");
    fs::create_dir_all(&root).map_err(|error| {
        format!(
            "failed to create V2 artifact root '{}': {error}",
            root.display()
        )
    })?;
    let unix_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| format!("system clock precedes Unix epoch: {error}"))?
        .as_millis();
    let hash_prefix = &root_hash[..root_hash.len().min(12)];
    for suffix in 0..100_u32 {
        let name = if suffix == 0 {
            format!("combat-{unix_ms}-{hash_prefix}")
        } else {
            format!("combat-{unix_ms}-{hash_prefix}-{suffix}")
        };
        let final_path = root.join(&name);
        if final_path.exists() {
            continue;
        }
        let staging_path = root.join(format!("{name}.pending"));
        match fs::create_dir(&staging_path) {
            Ok(()) => {
                return Ok(ArtifactDirectoryReservation {
                    staging_path,
                    final_path,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to reserve V2 artifact directory '{}': {error}",
                    staging_path.display()
                ));
            }
        }
    }
    Err("failed to reserve a unique V2 artifact directory".to_owned())
}

pub(super) fn write_json_create_new(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create '{}': {error}", path.display()))?;
    serde_json::to_writer(file, value)
        .map_err(|error| format!("failed to encode '{}': {error}", path.display()))
}

pub(super) fn load_artifact(path: &Path) -> Result<CombatContractArtifactV2, String> {
    let manifest = if path.is_dir() {
        path.join("manifest.json")
    } else {
        path.to_path_buf()
    };
    let bytes = fs::read(&manifest).map_err(|error| {
        format!(
            "failed to read V2 artifact '{}': {error}",
            manifest.display()
        )
    })?;
    parse_artifact(&manifest, &bytes)
}

fn parse_artifact(manifest: &Path, bytes: &[u8]) -> Result<CombatContractArtifactV2, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("invalid artifact JSON '{}': {error}", manifest.display()))?;
    let schema_name = value.get("schema_name").and_then(Value::as_str);
    let schema_version = value.get("schema_version").and_then(Value::as_u64);
    if schema_name != Some(ARTIFACT_SCHEMA)
        || schema_version != Some(u64::from(ARTIFACT_SCHEMA_VERSION))
    {
        return Err(format!(
            "unsupported artifact '{}'; V2 commands do not parse legacy reports",
            manifest.display()
        ));
    }
    serde_json::from_value(value)
        .map_err(|error| format!("invalid V2 artifact '{}': {error}", manifest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_rejects_legacy_report_before_typed_deserialization() {
        let error = parse_artifact(
            Path::new("legacy-report.json"),
            br#"{"schema_version":1,"request":{"case":11}}"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            "unsupported artifact 'legacy-report.json'; V2 commands do not parse legacy reports"
        );
    }

    #[test]
    fn loader_rejects_the_previous_manifest_without_search_state_index() {
        let error = parse_artifact(
            Path::new("v2-manifest.json"),
            br#"{"schema_name":"OracleCombatContractArtifactV2","schema_version":3}"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            "unsupported artifact 'v2-manifest.json'; V2 commands do not parse legacy reports"
        );
    }

    #[test]
    fn loader_rejects_schema_four_without_plan_prefix_accounting() {
        let error = parse_artifact(
            Path::new("v2-manifest.json"),
            br#"{"schema_name":"OracleCombatContractArtifactV2","schema_version":4}"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            "unsupported artifact 'v2-manifest.json'; V2 commands do not parse legacy reports"
        );
    }

    #[test]
    fn loader_rejects_schema_five_without_preferred_service_accounting() {
        let error = parse_artifact(
            Path::new("v2-manifest.json"),
            br#"{"schema_name":"OracleCombatContractArtifactV2","schema_version":5}"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            "unsupported artifact 'v2-manifest.json'; V2 commands do not parse legacy reports"
        );
    }

    #[test]
    fn loader_rejects_schema_six_without_independent_proposal_queues() {
        let error = parse_artifact(
            Path::new("v2-manifest.json"),
            br#"{"schema_name":"OracleCombatContractArtifactV2","schema_version":6}"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            "unsupported artifact 'v2-manifest.json'; V2 commands do not parse legacy reports"
        );
    }

    #[test]
    fn loader_rejects_schema_seven_without_per_state_proposal_attribution() {
        let error = parse_artifact(
            Path::new("v2-manifest.json"),
            br#"{"schema_name":"OracleCombatContractArtifactV2","schema_version":7}"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            "unsupported artifact 'v2-manifest.json'; V2 commands do not parse legacy reports"
        );
    }

    #[test]
    fn loader_rejects_schema_eight_without_proposal_service_scope() {
        let error = parse_artifact(
            Path::new("v2-manifest.json"),
            br#"{"schema_name":"OracleCombatContractArtifactV2","schema_version":8}"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            "unsupported artifact 'v2-manifest.json'; V2 commands do not parse legacy reports"
        );
    }

    #[test]
    fn loader_rejects_schema_nine_without_composed_prefix_contracts() {
        let error = parse_artifact(
            Path::new("v2-manifest.json"),
            br#"{"schema_name":"OracleCombatContractArtifactV2","schema_version":9}"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            "unsupported artifact 'v2-manifest.json'; V2 commands do not parse legacy reports"
        );
    }
}
