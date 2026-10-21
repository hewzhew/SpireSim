//! Typed provenance contract for exact combat case/action evidence.

use std::fs;
use std::path::{Component, Path, PathBuf};

use blake2::{Blake2b512, Digest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::combat_case_context::{
    CombatCaseReplayCapabilityV2, CombatCaseReplayIdentityV2,
    COMBAT_CASE_REPLAY_IDENTITY_SCHEMA_NAME, COMBAT_CASE_REPLAY_IDENTITY_SCHEMA_VERSION,
};
use sts_oracle_runtime::sim::combat::CombatTerminal;
use sts_oracle_runtime::state::core::ClientInput;

use super::canonical_launch::{runtime_identity, runtime_source_content_fingerprint};

pub(super) const COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME: &str = "CombatEvidenceManifestV2";
const LEGACY_COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME: &str = "CombatEvidenceManifestV1";
pub(super) const COMBAT_EVIDENCE_MANIFEST_FILE_SUFFIX: &str = "combat-evidence-manifest.json";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CombatEvidenceProducerV1 {
    HistoricalCombatWitnessExport,
    LocalGraphSearch,
    PolicyDiscrepancySearch,
    PotionExpenditureAudit,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatEvidenceManifest {
    pub(super) schema_name: String,
    pub(super) schema_version: u32,
    pub(super) producer: CombatEvidenceProducerV1,
    pub(super) runtime: Value,
    pub(super) runtime_source_content_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) case_identity: Option<CombatCaseReplayIdentityV2>,
    #[serde(
        default,
        rename = "root_exact_state_hash",
        skip_serializing_if = "Option::is_none"
    )]
    legacy_root_exact_state_hash: Option<String>,
    pub(super) case_path: PathBuf,
    pub(super) entries: Vec<CombatEvidenceManifestEntryV1>,
}

impl CombatEvidenceManifest {
    pub(super) fn root_exact_state_hash(&self) -> &str {
        self.case_identity
            .as_ref()
            .map(|identity| identity.root_exact_state_hash.as_str())
            .or(self.legacy_root_exact_state_hash.as_deref())
            .expect("validated combat evidence manifest must carry a root identity")
    }

    pub(super) fn uses_manifest_relative_paths(&self) -> bool {
        self.schema_name == COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME && self.schema_version == 2
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatEvidenceManifestEntryV1 {
    pub(super) evidence_id: String,
    pub(super) action_paths: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) trace_paths: Vec<PathBuf>,
    pub(super) action_sequence_blake2b_512: String,
    pub(super) supplied_action_count: usize,
    pub(super) expected_terminal: CombatTerminal,
    pub(super) expected_final_player_hp: Option<i32>,
}

impl CombatEvidenceManifestEntryV1 {
    pub(super) fn from_actions(
        evidence_id: String,
        action_paths: Vec<PathBuf>,
        actions: &[ClientInput],
        expected_terminal: CombatTerminal,
        expected_final_player_hp: Option<i32>,
    ) -> Result<Self, String> {
        if evidence_id.trim().is_empty() {
            return Err("combat evidence manifest evidence_id must not be empty".to_string());
        }
        if action_paths.is_empty() {
            return Err(format!(
                "combat evidence manifest entry '{evidence_id}' has no action paths"
            ));
        }
        Ok(Self {
            evidence_id,
            action_paths,
            trace_paths: Vec::new(),
            action_sequence_blake2b_512: combat_action_sequence_hash(actions)?,
            supplied_action_count: actions.len(),
            expected_terminal,
            expected_final_player_hp,
        })
    }
}

pub(super) fn write_combat_evidence_manifest(
    path: &Path,
    producer: CombatEvidenceProducerV1,
    root_exact_state_hash: String,
    case_path: PathBuf,
    mut entries: Vec<CombatEvidenceManifestEntryV1>,
) -> Result<(), String> {
    if root_exact_state_hash.trim().is_empty() {
        return Err("combat evidence manifest exact root hash must not be empty".to_string());
    }
    if entries.is_empty() {
        return Err("combat evidence manifest must contain at least one entry".to_string());
    }
    let case = load_combat_case(&case_path)?;
    let case_identity = case.replay_identity_v2()?;
    if case_identity.root_exact_state_hash != root_exact_state_hash {
        return Err(format!(
            "combat evidence manifest root does not match its case: supplied {root_exact_state_hash}, case {}",
            case_identity.root_exact_state_hash
        ));
    }
    let manifest_directory = prepare_manifest_directory(path)?;
    let case_path = manifest_relative_existing_path(&manifest_directory, &case_path)?;
    for entry in &mut entries {
        entry.action_paths = entry
            .action_paths
            .iter()
            .map(|path| manifest_relative_existing_path(&manifest_directory, path))
            .collect::<Result<Vec<_>, _>>()?;
        entry.trace_paths = entry
            .trace_paths
            .iter()
            .map(|path| manifest_relative_existing_path(&manifest_directory, path))
            .collect::<Result<Vec<_>, _>>()?;
    }
    let manifest = CombatEvidenceManifest {
        schema_name: COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME.to_string(),
        schema_version: 2,
        producer,
        runtime: runtime_identity(),
        runtime_source_content_fingerprint: runtime_source_content_fingerprint()?,
        case_identity: Some(case_identity),
        legacy_root_exact_state_hash: None,
        case_path,
        entries,
    };
    validate_combat_evidence_manifest(&manifest)?;
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("cannot serialize combat evidence manifest: {error}"))?;
    fs::write(path, bytes).map_err(|error| {
        format!(
            "cannot write combat evidence manifest '{}': {error}",
            path.display()
        )
    })
}

pub(super) fn combat_evidence_manifest_path_for_actions(action_path: &Path) -> PathBuf {
    let file_name = action_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("actions.json");
    let stem = file_name
        .strip_suffix(".actions.json")
        .or_else(|| action_path.file_stem().and_then(|stem| stem.to_str()))
        .unwrap_or("actions");
    action_path.with_file_name(format!("{stem}.{COMBAT_EVIDENCE_MANIFEST_FILE_SUFFIX}"))
}

pub(super) fn decode_combat_evidence_manifest(
    path: &Path,
    bytes: &[u8],
) -> Result<CombatEvidenceManifest, String> {
    let manifest: CombatEvidenceManifest = serde_json::from_slice(bytes).map_err(|error| {
        format!(
            "cannot decode combat evidence manifest '{}': {error}",
            path.display()
        )
    })?;
    validate_combat_evidence_manifest(&manifest)?;
    Ok(manifest)
}

fn prepare_manifest_directory(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "cannot create combat evidence manifest directory '{}': {error}",
            parent.display()
        )
    })?;
    parent.canonicalize().map_err(|error| {
        format!(
            "cannot resolve combat evidence manifest directory '{}': {error}",
            parent.display()
        )
    })
}

fn manifest_relative_existing_path(base: &Path, target: &Path) -> Result<PathBuf, String> {
    let absolute = if target.is_absolute() {
        target.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(target)
    };
    let absolute = absolute.canonicalize().map_err(|error| {
        format!(
            "cannot resolve combat evidence artifact '{}': {error}",
            target.display()
        )
    })?;
    relative_path(base, &absolute)
}

fn relative_path(base: &Path, target: &Path) -> Result<PathBuf, String> {
    let base_components = base.components().collect::<Vec<_>>();
    let target_components = target.components().collect::<Vec<_>>();
    let common = base_components
        .iter()
        .zip(&target_components)
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return Err(format!(
            "combat evidence artifact '{}' cannot be expressed relative to manifest directory '{}'",
            target.display(),
            base.display()
        ));
    }
    let mut relative = PathBuf::new();
    for component in &base_components[common..] {
        match component {
            Component::Normal(_) => relative.push(".."),
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "manifest directory '{}' is not a normalized absolute path",
                    base.display()
                ));
            }
        }
    }
    for component in &target_components[common..] {
        relative.push(component.as_os_str());
    }
    if relative.as_os_str().is_empty() {
        return Err("combat evidence artifact path resolves to the manifest directory".to_string());
    }
    Ok(relative)
}

fn valid_manifest_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::ParentDir))
}

pub(super) fn combat_action_sequence_hash(actions: &[ClientInput]) -> Result<String, String> {
    let bytes = serde_json::to_vec(actions).map_err(|error| error.to_string())?;
    let mut digest = Blake2b512::new();
    digest.update(bytes);
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn validate_combat_evidence_manifest(manifest: &CombatEvidenceManifest) -> Result<(), String> {
    let is_v2 = manifest.schema_name == COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME
        && manifest.schema_version == 2;
    let is_legacy_v1 = manifest.schema_name == LEGACY_COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME
        && manifest.schema_version == 1;
    if !is_v2 && !is_legacy_v1 {
        return Err(format!(
            "expected {COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME} schema_version 2 or legacy {LEGACY_COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME} schema_version 1, got {} schema_version {}",
            manifest.schema_name, manifest.schema_version
        ));
    }
    if is_v2 {
        if manifest.legacy_root_exact_state_hash.is_some() {
            return Err("V2 combat evidence manifest must use case_identity".to_string());
        }
        let identity = manifest
            .case_identity
            .as_ref()
            .ok_or_else(|| "V2 combat evidence manifest lacks case_identity".to_string())?;
        validate_case_identity(identity)?;
        if !valid_manifest_relative_path(&manifest.case_path) {
            return Err(
                "V2 combat evidence manifest case path must be manifest-relative".to_string(),
            );
        }
    } else {
        if manifest.case_identity.is_some() {
            return Err(
                "legacy V1 combat evidence manifest cannot declare case_identity".to_string(),
            );
        }
        if manifest
            .legacy_root_exact_state_hash
            .as_deref()
            .is_none_or(|hash| hash.trim().is_empty())
        {
            return Err("combat evidence manifest exact root hash must not be empty".to_string());
        }
    }
    if !valid_hex_digest(&manifest.runtime_source_content_fingerprint, 128) {
        return Err("combat evidence manifest runtime fingerprint is invalid".to_string());
    }
    if manifest.case_path.as_os_str().is_empty() {
        return Err("combat evidence manifest case path must not be empty".to_string());
    }
    if manifest.entries.is_empty() {
        return Err("combat evidence manifest must contain at least one entry".to_string());
    }
    for entry in &manifest.entries {
        if entry.evidence_id.trim().is_empty() {
            return Err("combat evidence manifest evidence_id must not be empty".to_string());
        }
        if entry.action_paths.is_empty() {
            return Err(format!(
                "combat evidence manifest entry '{}' has no action paths",
                entry.evidence_id
            ));
        }
        if entry
            .action_paths
            .iter()
            .any(|path| path.as_os_str().is_empty())
        {
            return Err(format!(
                "combat evidence manifest entry '{}' has an empty action path",
                entry.evidence_id
            ));
        }
        if entry
            .trace_paths
            .iter()
            .any(|path| path.as_os_str().is_empty())
        {
            return Err(format!(
                "combat evidence manifest entry '{}' has an empty trace path",
                entry.evidence_id
            ));
        }
        if is_v2
            && entry
                .action_paths
                .iter()
                .chain(&entry.trace_paths)
                .any(|path| !valid_manifest_relative_path(path))
        {
            return Err(format!(
                "V2 combat evidence manifest entry '{}' paths must be manifest-relative",
                entry.evidence_id
            ));
        }
        if !valid_hex_digest(&entry.action_sequence_blake2b_512, 128) {
            return Err(format!(
                "combat evidence manifest entry '{}' has an invalid action hash",
                entry.evidence_id
            ));
        }
    }
    Ok(())
}

fn validate_case_identity(identity: &CombatCaseReplayIdentityV2) -> Result<(), String> {
    if identity.schema_name != COMBAT_CASE_REPLAY_IDENTITY_SCHEMA_NAME
        || identity.schema_version != COMBAT_CASE_REPLAY_IDENTITY_SCHEMA_VERSION
    {
        return Err("combat evidence manifest has an unsupported case identity".to_string());
    }
    if !valid_hex_digest(&identity.root_exact_state_hash, 64) {
        return Err("combat evidence manifest exact root hash is invalid".to_string());
    }
    let valid_optional_digest = |value: &Option<String>| {
        value
            .as_deref()
            .is_none_or(|fingerprint| valid_hex_digest(fingerprint, 64))
    };
    if !valid_optional_digest(&identity.run_session_fingerprint)
        || !valid_optional_digest(&identity.owner_policy_fingerprint)
    {
        return Err("combat evidence manifest case identity fingerprint is invalid".to_string());
    }
    let valid_capability = match identity.capability {
        CombatCaseReplayCapabilityV2::IsolatedProjection => {
            identity.run_session_fingerprint.is_none()
                && identity.owner_policy_fingerprint.is_none()
        }
        CombatCaseReplayCapabilityV2::ExactProductionState => {
            identity.run_session_fingerprint.is_some()
                && identity.owner_policy_fingerprint.is_none()
        }
        CombatCaseReplayCapabilityV2::ExactProductionOwner => {
            identity.run_session_fingerprint.is_some()
                && identity.owner_policy_fingerprint.is_some()
        }
    };
    if !valid_capability {
        return Err(
            "combat evidence manifest case identity contradicts its capability".to_string(),
        );
    }
    Ok(())
}

fn valid_hex_digest(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        combat_action_sequence_hash, combat_evidence_manifest_path_for_actions,
        decode_combat_evidence_manifest, COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME,
        LEGACY_COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME,
    };

    #[test]
    fn manifest_decode_rejects_invalid_action_identity() {
        let runtime_fingerprint = "0".repeat(128);
        let value = serde_json::json!({
            "schema_name": LEGACY_COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME,
            "schema_version": 1,
            "producer": "potion_expenditure_audit",
            "runtime": {},
            "runtime_source_content_fingerprint": runtime_fingerprint,
            "root_exact_state_hash": "root",
            "case_path": "case.json",
            "entries": [{
                "evidence_id": "lane",
                "action_paths": ["lane.actions.json"],
                "action_sequence_blake2b_512": "not-a-hash",
                "supplied_action_count": 0,
                "expected_terminal": "win",
                "expected_final_player_hp": 1
            }]
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        let error = decode_combat_evidence_manifest(Path::new("manifest.json"), &bytes)
            .expect_err("invalid hash must be rejected");
        assert!(error.contains("invalid action hash"));
    }

    #[test]
    fn v2_manifest_rejects_non_relative_artifact_paths() {
        let absolute_case = std::env::current_dir().unwrap().join("case.json");
        let value = serde_json::json!({
            "schema_name": COMBAT_EVIDENCE_MANIFEST_SCHEMA_NAME,
            "schema_version": 2,
            "producer": "potion_expenditure_audit",
            "runtime": {},
            "runtime_source_content_fingerprint": "0".repeat(128),
            "case_identity": {
                "schema_name": "CombatCaseReplayIdentityV2",
                "schema_version": 2,
                "capability": "isolated_projection",
                "root_exact_state_hash": "0".repeat(64)
            },
            "case_path": absolute_case,
            "entries": [{
                "evidence_id": "lane",
                "action_paths": ["lane.actions.json"],
                "action_sequence_blake2b_512": "0".repeat(128),
                "supplied_action_count": 0,
                "expected_terminal": "win",
                "expected_final_player_hp": 1
            }]
        });
        let error = decode_combat_evidence_manifest(
            Path::new("manifest.json"),
            &serde_json::to_vec(&value).unwrap(),
        )
        .expect_err("V2 paths must resolve from the manifest only");
        assert!(error.contains("manifest-relative"), "{error}");
    }

    #[test]
    fn action_hash_is_stable_for_typed_empty_sequence() {
        let left = combat_action_sequence_hash(&[]).unwrap();
        let right = combat_action_sequence_hash(&[]).unwrap();
        assert_eq!(left, right);
        assert_eq!(left.len(), 128);
    }

    #[test]
    fn action_manifest_path_preserves_distinct_action_stem() {
        let path = combat_evidence_manifest_path_for_actions(Path::new("root/win.actions.json"));
        assert_eq!(path, Path::new("root/win.combat-evidence-manifest.json"));
    }
}
