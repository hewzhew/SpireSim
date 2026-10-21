use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::eval::source_identity::SourceIdentity;

use super::{
    combat_lab_cell_key_v1, derive_shuffle_seed_v1, CombatLabCellRecordV1, ResolvedCombatLabSpecV1,
    COMBAT_LAB_CELL_SCHEMA_VERSION,
};

pub const COMBAT_LAB_ARTIFACT_SCHEMA_VERSION: u32 = 1;

pub(super) type JournalSync = fn(&File) -> std::io::Result<()>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabManifestV1 {
    pub schema_version: u32,
    pub experiment_hash: String,
    pub resolved_spec: ResolvedCombatLabSpecV1,
    pub source_identity: SourceIdentity,
    pub environment: CombatLabEnvironmentV1,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatLabEnvironmentV1 {
    pub package_version: String,
    pub target_os: String,
    pub target_arch: String,
    #[serde(default)]
    pub cargo_profile: Option<String>,
    #[serde(default)]
    pub debug_assertions: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabCheckpointV1 {
    pub schema_version: u32,
    pub journal_digest: String,
    pub completed_cell_keys: BTreeSet<String>,
    pub next_sample_hint: u64,
}

pub struct CombatLabArtifactStoreV1 {
    root: PathBuf,
    manifest: CombatLabManifestV1,
    cells: Vec<CombatLabCellRecordV1>,
    completed_cell_keys: BTreeSet<String>,
    valid_journal_bytes: Vec<u8>,
    journal_sync: JournalSync,
    mutation_poisoned: bool,
}

impl CombatLabManifestV1 {
    pub fn from_resolved_v1(
        resolved_spec: ResolvedCombatLabSpecV1,
        source_identity: SourceIdentity,
        created_at_unix_ms: u64,
    ) -> Self {
        Self {
            schema_version: COMBAT_LAB_ARTIFACT_SCHEMA_VERSION,
            experiment_hash: resolved_spec.experiment_hash.clone(),
            resolved_spec,
            source_identity,
            environment: CombatLabEnvironmentV1 {
                package_version: env!("CARGO_PKG_VERSION").to_string(),
                target_os: std::env::consts::OS.to_string(),
                target_arch: std::env::consts::ARCH.to_string(),
                cargo_profile: Some(env!("STS_CARGO_PROFILE").to_string()),
                debug_assertions: Some(cfg!(debug_assertions)),
            },
            created_at_unix_ms,
        }
    }
}

impl CombatLabArtifactStoreV1 {
    pub fn create_or_resume(
        output_dir: &Path,
        expected_manifest: CombatLabManifestV1,
    ) -> Result<Self, String> {
        Self::create_or_resume_with_journal_sync(output_dir, expected_manifest, File::sync_data)
    }

    pub(super) fn create_or_resume_with_journal_sync(
        output_dir: &Path,
        expected_manifest: CombatLabManifestV1,
        journal_sync: JournalSync,
    ) -> Result<Self, String> {
        fs::create_dir_all(output_dir).map_err(|error| {
            format!(
                "failed to create combat laboratory artifact directory '{}': {error}",
                output_dir.display()
            )
        })?;
        let manifest_path = output_dir.join("manifest.json");
        let manifest = if manifest_path.exists() {
            let bytes = fs::read(&manifest_path).map_err(|error| {
                format!(
                    "failed to read combat laboratory manifest '{}': {error}",
                    manifest_path.display()
                )
            })?;
            let existing: CombatLabManifestV1 =
                serde_json::from_slice(&bytes).map_err(|error| {
                    format!(
                        "failed to parse combat laboratory manifest '{}': {error}",
                        manifest_path.display()
                    )
                })?;
            validate_resume_identity(&existing, &expected_manifest)?;
            existing
        } else {
            let orphan = ["cells.jsonl", "checkpoint.json", "summary.json"]
                .into_iter()
                .find(|file_name| output_dir.join(file_name).exists());
            if let Some(file_name) = orphan {
                return Err(format!(
                    "refusing to create combat laboratory manifest: orphan canonical artifact '{file_name}' exists without manifest.json"
                ));
            }
            atomic_write_json(&manifest_path, &expected_manifest)?;
            expected_manifest
        };

        let journal_path = output_dir.join("cells.jsonl");
        let journal_bytes = if journal_path.exists() {
            fs::read(&journal_path).map_err(|error| {
                format!(
                    "failed to read combat laboratory journal '{}': {error}",
                    journal_path.display()
                )
            })?
        } else {
            Vec::new()
        };
        let valid_len = journal_bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        if valid_len != journal_bytes.len() {
            truncate_partial_journal_tail(&journal_path, valid_len as u64)?;
        }
        let valid_journal_bytes = journal_bytes[..valid_len].to_vec();
        let cells = parse_complete_journal_lines(&valid_journal_bytes)?;
        let mut completed_cell_keys = BTreeSet::new();
        for cell in &cells {
            if !completed_cell_keys.insert(cell.cell_key.clone()) {
                return Err(format!(
                    "duplicate cell key '{}' in combat laboratory journal",
                    cell.cell_key
                ));
            }
        }

        repair_checkpoint_if_needed(
            output_dir,
            &manifest,
            &cells,
            &completed_cell_keys,
            &valid_journal_bytes,
        )?;

        Ok(Self {
            root: output_dir.to_path_buf(),
            manifest,
            cells,
            completed_cell_keys,
            valid_journal_bytes,
            journal_sync,
            mutation_poisoned: false,
        })
    }

    pub fn manifest(&self) -> &CombatLabManifestV1 {
        &self.manifest
    }

    pub fn cells(&self) -> &[CombatLabCellRecordV1] {
        &self.cells
    }

    pub fn contains_cell(&self, cell_key: &str) -> bool {
        self.completed_cell_keys.contains(cell_key)
    }

    pub fn append_cell(&mut self, cell: &CombatLabCellRecordV1) -> Result<(), String> {
        self.ensure_mutations_allowed()?;
        validate_cell_schema_version(cell)?;
        if self.contains_cell(&cell.cell_key) {
            return Err(format!(
                "duplicate cell key '{}': journal unchanged",
                cell.cell_key
            ));
        }
        let mut encoded = serde_json::to_vec(cell)
            .map_err(|error| format!("failed to serialize combat laboratory cell: {error}"))?;
        encoded.push(b'\n');
        let path = self.root.join("cells.jsonl");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| {
                format!(
                    "failed to open combat laboratory journal '{}': {error}",
                    path.display()
                )
            })?;
        if let Err(error) = file.write_all(&encoded) {
            self.mutation_poisoned = true;
            return Err(format!(
                "failed to append combat laboratory journal '{}': {error}",
                path.display()
            ));
        }
        if let Err(error) = (self.journal_sync)(&file) {
            self.mutation_poisoned = true;
            return Err(format!(
                "failed to sync combat laboratory journal '{}': {error}",
                path.display()
            ));
        }
        self.valid_journal_bytes.extend_from_slice(&encoded);
        self.completed_cell_keys.insert(cell.cell_key.clone());
        self.cells.push(cell.clone());
        Ok(())
    }

    pub fn checkpoint_sample_boundary(&self, next_sample_hint: u64) -> Result<(), String> {
        self.ensure_mutations_allowed()?;
        let derived_next_sample_hint =
            conservative_next_sample_hint(&self.manifest, &self.cells, &self.completed_cell_keys);
        if next_sample_hint != derived_next_sample_hint {
            return Err(format!(
                "combat laboratory checkpoint next_sample_hint {next_sample_hint} disagrees with journal-derived {derived_next_sample_hint}"
            ));
        }
        let checkpoint = CombatLabCheckpointV1 {
            schema_version: COMBAT_LAB_ARTIFACT_SCHEMA_VERSION,
            journal_digest: journal_digest(&self.valid_journal_bytes),
            completed_cell_keys: self.completed_cell_keys.clone(),
            next_sample_hint: derived_next_sample_hint,
        };
        atomic_write_json(&self.root.join("checkpoint.json"), &checkpoint)
    }

    pub fn write_summary<T: Serialize>(&self, summary: &T) -> Result<(), String> {
        self.ensure_mutations_allowed()?;
        atomic_write_json(&self.root.join("summary.json"), summary)
    }

    fn ensure_mutations_allowed(&self) -> Result<(), String> {
        if self.mutation_poisoned {
            return Err(
                "combat laboratory artifact store mutation refused after journal persistence failure; reopen required"
                    .to_string(),
            );
        }
        Ok(())
    }
}

fn validate_resume_identity(
    existing: &CombatLabManifestV1,
    expected: &CombatLabManifestV1,
) -> Result<(), String> {
    ensure_resume_field(
        "schema_version",
        &existing.schema_version,
        &expected.schema_version,
    )?;
    ensure_resume_field(
        "resolved_spec.scenario_hash",
        &existing.resolved_spec.scenario_hash,
        &expected.resolved_spec.scenario_hash,
    )?;
    ensure_resume_field(
        "resolved_spec.profiles.len",
        &existing.resolved_spec.profiles.len(),
        &expected.resolved_spec.profiles.len(),
    )?;
    for (index, (existing_profile, expected_profile)) in existing
        .resolved_spec
        .profiles
        .iter()
        .zip(&expected.resolved_spec.profiles)
        .enumerate()
    {
        ensure_resume_field(
            &format!("resolved_spec.profiles[{index}].profile_hash"),
            &existing_profile.profile_hash,
            &expected_profile.profile_hash,
        )?;
    }
    ensure_resume_field(
        "resolved_spec.budget_hash",
        &existing.resolved_spec.budget_hash,
        &expected.resolved_spec.budget_hash,
    )?;

    let existing_resolved = serde_json::to_value(&existing.resolved_spec).map_err(|error| {
        format!("failed to compare existing combat laboratory resolved spec: {error}")
    })?;
    let expected_resolved = serde_json::to_value(&expected.resolved_spec).map_err(|error| {
        format!("failed to compare expected combat laboratory resolved spec: {error}")
    })?;
    if let Some(field) =
        first_json_difference(&existing_resolved, &expected_resolved, "resolved_spec")
    {
        return Err(format!("combat laboratory resume mismatch at {field}"));
    }

    ensure_resume_field(
        "experiment_hash",
        &existing.experiment_hash,
        &expected.experiment_hash,
    )?;
    ensure_resume_field(
        "source_identity.git_commit",
        &existing.source_identity.git_commit,
        &expected.source_identity.git_commit,
    )?;
    ensure_resume_field(
        "source_identity.git_dirty",
        &existing.source_identity.git_dirty,
        &expected.source_identity.git_dirty,
    )?;
    ensure_resume_field(
        "environment.package_version",
        &existing.environment.package_version,
        &expected.environment.package_version,
    )?;
    ensure_resume_field(
        "environment.target_os",
        &existing.environment.target_os,
        &expected.environment.target_os,
    )?;
    ensure_resume_field(
        "environment.target_arch",
        &existing.environment.target_arch,
        &expected.environment.target_arch,
    )?;
    ensure_resume_field(
        "environment.cargo_profile",
        &existing.environment.cargo_profile,
        &expected.environment.cargo_profile,
    )?;
    ensure_resume_field(
        "environment.debug_assertions",
        &existing.environment.debug_assertions,
        &expected.environment.debug_assertions,
    )?;
    Ok(())
}

fn ensure_resume_field<T>(field: &str, existing: &T, expected: &T) -> Result<(), String>
where
    T: std::fmt::Debug + PartialEq,
{
    if existing == expected {
        return Ok(());
    }
    Err(format!(
        "combat laboratory resume mismatch at {field}: existing {existing:?}, expected {expected:?}"
    ))
}

fn first_json_difference(
    existing: &serde_json::Value,
    expected: &serde_json::Value,
    path: &str,
) -> Option<String> {
    match (existing, expected) {
        (serde_json::Value::Object(existing), serde_json::Value::Object(expected)) => {
            for key in existing.keys().chain(expected.keys()) {
                match (existing.get(key), expected.get(key)) {
                    (Some(existing), Some(expected)) => {
                        let field = format!("{path}.{key}");
                        if let Some(difference) = first_json_difference(existing, expected, &field)
                        {
                            return Some(difference);
                        }
                    }
                    _ => return Some(format!("{path}.{key}")),
                }
            }
            None
        }
        (serde_json::Value::Array(existing), serde_json::Value::Array(expected)) => {
            if existing.len() != expected.len() {
                return Some(format!("{path}.len"));
            }
            existing
                .iter()
                .zip(expected)
                .enumerate()
                .find_map(|(index, (existing, expected))| {
                    first_json_difference(existing, expected, &format!("{path}[{index}]"))
                })
        }
        _ => (existing != expected).then(|| path.to_string()),
    }
}

fn parse_complete_journal_lines(bytes: &[u8]) -> Result<Vec<CombatLabCellRecordV1>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    bytes[..bytes.len() - 1]
        .split(|byte| *byte == b'\n')
        .enumerate()
        .map(|(index, line)| {
            #[derive(Deserialize)]
            struct CellSchemaProbe {
                schema_version: u32,
            }
            let probe: CellSchemaProbe = serde_json::from_slice(line).map_err(|error| {
                format!(
                    "malformed combat laboratory journal entry at line {}: {error}",
                    index + 1
                )
            })?;
            validate_cell_schema_version_value(probe.schema_version).map_err(|error| {
                format!("combat laboratory journal line {}: {error}", index + 1)
            })?;
            let cell: CombatLabCellRecordV1 = serde_json::from_slice(line).map_err(|error| {
                format!(
                    "malformed combat laboratory journal entry at line {}: {error}",
                    index + 1
                )
            })?;
            validate_cell_schema_version(&cell).map_err(|error| {
                format!("combat laboratory journal line {}: {error}", index + 1)
            })?;
            Ok(cell)
        })
        .collect()
}

fn validate_cell_schema_version(cell: &CombatLabCellRecordV1) -> Result<(), String> {
    validate_cell_schema_version_value(cell.schema_version)
}

fn validate_cell_schema_version_value(schema_version: u32) -> Result<(), String> {
    if schema_version == COMBAT_LAB_CELL_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "unsupported combat laboratory cell schema_version {}; expected {} and legacy fingerprint cells cannot be resumed",
            schema_version, COMBAT_LAB_CELL_SCHEMA_VERSION
        ))
    }
}

fn truncate_partial_journal_tail(path: &Path, valid_len: u64) -> Result<(), String> {
    let file = OpenOptions::new().write(true).open(path).map_err(|error| {
        format!(
            "failed to open combat laboratory journal '{}' for tail repair: {error}",
            path.display()
        )
    })?;
    file.set_len(valid_len).map_err(|error| {
        format!(
            "failed to truncate partial combat laboratory journal tail '{}': {error}",
            path.display()
        )
    })?;
    file.sync_data().map_err(|error| {
        format!(
            "failed to sync repaired combat laboratory journal '{}': {error}",
            path.display()
        )
    })
}

fn repair_checkpoint_if_needed(
    output_dir: &Path,
    manifest: &CombatLabManifestV1,
    cells: &[CombatLabCellRecordV1],
    completed_cell_keys: &BTreeSet<String>,
    valid_journal_bytes: &[u8],
) -> Result<(), String> {
    let path = output_dir.join("checkpoint.json");
    let digest = journal_digest(valid_journal_bytes);
    let next_sample_hint = conservative_next_sample_hint(manifest, cells, completed_cell_keys);
    if !path.exists() {
        if cells.is_empty() {
            return Ok(());
        }
        let recovered = CombatLabCheckpointV1 {
            schema_version: COMBAT_LAB_ARTIFACT_SCHEMA_VERSION,
            journal_digest: digest,
            completed_cell_keys: completed_cell_keys.clone(),
            next_sample_hint,
        };
        return atomic_write_json(&path, &recovered);
    }
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "failed to read combat laboratory checkpoint '{}': {error}",
            path.display()
        )
    })?;
    let checkpoint = serde_json::from_slice::<CombatLabCheckpointV1>(&bytes).ok();
    let agrees = checkpoint.as_ref().is_some_and(|checkpoint| {
        checkpoint.schema_version == COMBAT_LAB_ARTIFACT_SCHEMA_VERSION
            && checkpoint.journal_digest == digest
            && checkpoint.completed_cell_keys == *completed_cell_keys
            && checkpoint.next_sample_hint == next_sample_hint
    });
    if agrees {
        return Ok(());
    }
    let repaired = CombatLabCheckpointV1 {
        schema_version: COMBAT_LAB_ARTIFACT_SCHEMA_VERSION,
        journal_digest: digest,
        completed_cell_keys: completed_cell_keys.clone(),
        next_sample_hint,
    };
    atomic_write_json(&path, &repaired)
}

fn conservative_next_sample_hint(
    manifest: &CombatLabManifestV1,
    cells: &[CombatLabCellRecordV1],
    completed_cell_keys: &BTreeSet<String>,
) -> u64 {
    let largest_observed = cells
        .iter()
        .map(|cell| cell.sample_index)
        .max()
        .unwrap_or(0);
    let mut sample_index = 0_u64;
    while sample_index <= largest_observed {
        let shuffle_seed = derive_shuffle_seed_v1(&manifest.resolved_spec.schedule, sample_index);
        let sample_complete = manifest.resolved_spec.profiles.iter().all(|profile| {
            let key = combat_lab_cell_key_v1(
                &manifest.experiment_hash,
                sample_index,
                shuffle_seed,
                &profile.spec.id,
                &profile.profile_hash,
                &manifest.resolved_spec.budget_hash,
            );
            completed_cell_keys.contains(&key)
        });
        if !sample_complete {
            return sample_index;
        }
        sample_index = sample_index.saturating_add(1);
    }
    sample_index
}

fn journal_digest(bytes: &[u8]) -> String {
    use blake2::{Blake2b512, Digest};

    let mut hasher = Blake2b512::new();
    hasher.update(bytes);
    hasher.finalize()[..32]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Persist one JSON artifact through a synchronized sibling temporary file.
///
/// Branch persistence shares this primitive so every oracle artifact observes
/// the same replacement and cleanup contract.
pub fn atomic_write_json<T: Serialize>(destination: &Path, value: &T) -> Result<(), String> {
    let temporary = unique_sibling_temporary_path(destination);
    let result = (|| {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| {
                format!(
                    "failed to create temporary artifact '{}': {error}",
                    temporary.display()
                )
            })?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer(&mut writer, value).map_err(|error| {
            format!(
                "failed to serialize temporary artifact '{}': {error}",
                temporary.display()
            )
        })?;
        writer.flush().map_err(|error| {
            format!(
                "failed to flush temporary artifact '{}': {error}",
                temporary.display()
            )
        })?;
        writer.get_ref().sync_all().map_err(|error| {
            format!(
                "failed to sync temporary artifact '{}': {error}",
                temporary.display()
            )
        })?;
        drop(writer);
        replace_file(&temporary, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn unique_sibling_temporary_path(destination: &Path) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    destination.with_file_name(format!(
        ".{name}.tmp-{}-{nonce}-{sequence}",
        std::process::id()
    ))
}

#[cfg(unix)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|error| {
        format!(
            "failed to replace artifact '{}' with '{}': {error}",
            destination.display(),
            source.display()
        )
    })
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: Both buffers are owned, NUL-terminated UTF-16 paths and remain live for the call.
    let replaced = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        return Err(format!(
            "failed to replace artifact '{}' with '{}': {}",
            destination.display(),
            source.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
compile_error!("combat laboratory atomic artifact replacement requires Unix or Windows");
