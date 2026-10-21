use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::super::combat_evidence_manifest::{
    decode_combat_evidence_manifest, COMBAT_EVIDENCE_MANIFEST_FILE_SUFFIX,
};
use super::{
    display_path, ArtifactInventory, PairCandidate, ReplayExpectations, UnresolvedArtifact,
};

pub(super) fn collect_artifacts(root: &Path) -> Result<ArtifactInventory, String> {
    let mut inventory = ArtifactInventory::default();
    collect_artifacts_recursive(root, &mut inventory)?;
    inventory.manifest_files.sort();
    inventory.trace_files.sort();
    inventory.case_files.sort();
    inventory.action_files.sort();
    Ok(inventory)
}

fn collect_artifacts_recursive(
    directory: &Path,
    inventory: &mut ArtifactInventory,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("cannot scan '{}': {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            let name = entry.file_name();
            if name == ".git" || name == "target" {
                continue;
            }
            collect_artifacts_recursive(&path, inventory)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if is_trace_artifact_name(&name) {
            inventory.trace_files.push(path.clone());
        }
        if name.ends_with(COMBAT_EVIDENCE_MANIFEST_FILE_SUFFIX) {
            inventory.manifest_files.push(path.clone());
        }
        if name.ends_with(".case.json") || name.ends_with(".combat-case.json") {
            inventory.case_files.push(path.clone());
        }
        if name.ends_with(".actions.json") {
            inventory.action_files.push(path);
        }
    }
    Ok(())
}

pub(super) fn is_trace_artifact_name(name: &str) -> bool {
    name.ends_with("trace.json")
}

pub(super) fn read_json_value(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(
        &fs::read(path)
            .map_err(|error| format!("cannot read trace '{}': {error}", path.display()))?,
    )
    .map_err(|error| format!("cannot parse trace '{}': {error}", path.display()))
}

pub(super) fn declared_manifest_pairs(
    manifest_path: &Path,
    scan_root: &Path,
    current_dir: &Path,
) -> Result<Vec<PairCandidate>, String> {
    let bytes = fs::read(manifest_path).map_err(|error| {
        format!(
            "cannot read combat evidence manifest '{}': {error}",
            manifest_path.display()
        )
    })?;
    let manifest = decode_combat_evidence_manifest(manifest_path, &bytes)?;
    let manifest_relative_paths = manifest.uses_manifest_relative_paths();
    let manifest_root_exact_state_hash = manifest.root_exact_state_hash().to_string();
    let manifest_case_identity = manifest.case_identity.clone();
    let case_raw = manifest.case_path.to_string_lossy();
    let case_path = resolve_manifest_path(
        &case_raw,
        manifest_path,
        scan_root,
        current_dir,
        manifest_relative_paths,
    )
    .ok_or_else(|| format!("manifest case path is missing: {case_raw}"))?;
    let mut candidates = Vec::with_capacity(manifest.entries.len());
    for entry in manifest.entries {
        let mut action_paths = Vec::with_capacity(entry.action_paths.len());
        for raw_path in &entry.action_paths {
            let raw = raw_path.to_string_lossy();
            action_paths.push(
                resolve_manifest_path(
                    &raw,
                    manifest_path,
                    scan_root,
                    current_dir,
                    manifest_relative_paths,
                )
                .ok_or_else(|| format!("manifest action path is missing: {raw}"))?,
            );
        }
        let mut source_paths = BTreeSet::from([display_path(manifest_path)]);
        for raw_path in &entry.trace_paths {
            let raw = raw_path.to_string_lossy();
            let path = resolve_manifest_path(
                &raw,
                manifest_path,
                scan_root,
                current_dir,
                manifest_relative_paths,
            )
            .ok_or_else(|| format!("manifest trace path is missing: {raw}"))?;
            source_paths.insert(display_path(&path));
        }
        candidates.push(PairCandidate {
            case_path: case_path.clone(),
            action_paths,
            provenance: BTreeSet::from([if manifest_relative_paths {
                "typed_evidence_manifest_v2".to_string()
            } else {
                "typed_evidence_manifest_v1_legacy_paths".to_string()
            }]),
            source_paths,
            expectations: ReplayExpectations {
                case_identities: manifest_case_identity.clone().into_iter().collect(),
                root_exact_state_hashes: BTreeSet::from([manifest_root_exact_state_hash.clone()]),
                action_sequence_blake2b_512: BTreeSet::from([entry.action_sequence_blake2b_512]),
                supplied_action_counts: BTreeSet::from([entry.supplied_action_count]),
                final_terminals: vec![entry.expected_terminal],
                final_player_hps: entry.expected_final_player_hp.into_iter().collect(),
            },
        });
    }
    Ok(candidates)
}

fn resolve_manifest_path(
    raw: &str,
    manifest_path: &Path,
    scan_root: &Path,
    current_dir: &Path,
    manifest_relative_paths: bool,
) -> Option<PathBuf> {
    if manifest_relative_paths {
        let raw_path = Path::new(raw);
        if raw_path.is_absolute() {
            return None;
        }
        return manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(raw_path)
            .canonicalize()
            .ok()
            .filter(|candidate| candidate.is_file());
    }
    resolve_declared_path(raw, manifest_path, scan_root, current_dir)
}

pub(super) fn declared_pair(
    trace_path: &Path,
    value: &Value,
    scan_root: &Path,
    current_dir: &Path,
) -> Result<Option<PairCandidate>, String> {
    let Some(case_raw) = value.get("case").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(action_values) = value.get("actions") else {
        return Ok(None);
    };
    let action_raw = match action_values {
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>(),
        Value::String(path) => vec![path.clone()],
        _ => Vec::new(),
    };
    if action_raw.is_empty() {
        return Ok(None);
    }
    let case_path = resolve_declared_path(case_raw, trace_path, scan_root, current_dir)
        .ok_or_else(|| format!("declared case path is missing: {case_raw}"))?;
    let mut action_paths = Vec::with_capacity(action_raw.len());
    for raw in action_raw {
        action_paths.push(
            resolve_declared_path(&raw, trace_path, scan_root, current_dir)
                .ok_or_else(|| format!("declared action path is missing: {raw}"))?,
        );
    }
    Ok(Some(PairCandidate {
        case_path,
        action_paths,
        provenance: BTreeSet::from(["trace_declared_exact_pair".to_string()]),
        source_paths: BTreeSet::from([display_path(trace_path)]),
        expectations: ReplayExpectations::default(),
    }))
}

fn resolve_declared_path(
    raw: &str,
    trace_path: &Path,
    scan_root: &Path,
    current_dir: &Path,
) -> Option<PathBuf> {
    let raw_path = PathBuf::from(raw);
    let mut candidates = Vec::new();
    if raw_path.is_absolute() {
        candidates.push(raw_path.clone());
    } else {
        candidates.push(current_dir.join(&raw_path));
        if let Some(parent) = scan_root.parent() {
            candidates.push(parent.join(&raw_path));
        }
        if let Some(parent) = trace_path.parent() {
            candidates.push(parent.join(&raw_path));
        }
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| candidate.canonicalize().ok())
}

pub(super) fn pair_key(case_path: &Path, action_paths: &[PathBuf]) -> String {
    let actions = action_paths
        .iter()
        .map(|path| display_path(path))
        .collect::<Vec<_>>()
        .join("|");
    format!("{}|{actions}", display_path(case_path))
}

pub(super) fn merge_pair(
    pairs: &mut BTreeMap<String, PairCandidate>,
    key: String,
    candidate: PairCandidate,
) {
    if let Some(existing) = pairs.get_mut(&key) {
        existing.provenance.extend(candidate.provenance);
        existing.source_paths.extend(candidate.source_paths);
        existing
            .expectations
            .case_identities
            .extend(candidate.expectations.case_identities);
        existing
            .expectations
            .root_exact_state_hashes
            .extend(candidate.expectations.root_exact_state_hashes);
        existing
            .expectations
            .action_sequence_blake2b_512
            .extend(candidate.expectations.action_sequence_blake2b_512);
        existing
            .expectations
            .supplied_action_counts
            .extend(candidate.expectations.supplied_action_counts);
        existing
            .expectations
            .final_terminals
            .extend(candidate.expectations.final_terminals);
        existing
            .expectations
            .final_player_hps
            .extend(candidate.expectations.final_player_hps);
    } else {
        pairs.insert(key, candidate);
    }
}

pub(super) fn report_unassociated_actions(
    inventory: &ArtifactInventory,
    referenced_actions: &BTreeSet<PathBuf>,
    unresolved: &mut Vec<UnresolvedArtifact>,
) -> Result<usize, String> {
    let mut count = 0usize;
    for action_path in &inventory.action_files {
        let action_path = action_path
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if referenced_actions.contains(&action_path) {
            continue;
        }
        count = count.saturating_add(1);
        unresolved.push(UnresolvedArtifact {
            path: display_path(&action_path),
            reason:
                "unassociated action sequence: no producer manifest or declared trace relationship"
                    .to_string(),
        });
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn undeclared_actions_remain_unknown_instead_of_using_directory_inference() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sts-combat-evidence-unassociated-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let action_path = root.join("same-stem.actions.json");
        let case_path = root.join("same-stem.case.json");
        fs::write(&action_path, b"[]").unwrap();
        fs::write(&case_path, b"{}").unwrap();
        let inventory = ArtifactInventory {
            action_files: vec![action_path],
            case_files: vec![case_path],
            ..ArtifactInventory::default()
        };
        let mut unresolved = Vec::new();

        let count =
            report_unassociated_actions(&inventory, &BTreeSet::new(), &mut unresolved).unwrap();

        assert_eq!(count, 1);
        assert_eq!(unresolved.len(), 1);
        assert!(unresolved[0].reason.contains("no producer manifest"));
        fs::remove_dir_all(root).unwrap();
    }
}
