use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::eval::campfire_evaluation::CampfireEvaluationContext;
use crate::eval::campfire_survival_scenarios::{
    CampfireSurvivalScenarioGapRecord, CampfireSurvivalSubject,
};
use crate::eval::combat_lab_v1::{atomic_write_json, CombatLabEnvironmentV1};
use crate::eval::source_identity::SourceIdentity;

use super::{
    CampfireThreatPanelCellV1, ResolvedCampfireThreatPanelSpecV1,
    CAMPFIRE_THREAT_PANEL_CELL_SCHEMA_VERSION,
};

pub const CAMPFIRE_THREAT_PANEL_ARTIFACT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireThreatPanelManifestV1 {
    pub schema_version: u32,
    pub contract_hash: String,
    pub resolved_spec: ResolvedCampfireThreatPanelSpecV1,
    pub evaluation_context: CampfireEvaluationContext,
    pub subjects: Vec<CampfireSurvivalSubject>,
    pub candidate_gaps: Vec<CampfireSurvivalScenarioGapRecord>,
    pub source_identity: SourceIdentity,
    pub environment: CombatLabEnvironmentV1,
    pub created_at_unix_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireThreatPanelCheckpointV1 {
    pub schema_version: u32,
    pub journal_digest: String,
    pub completed_cell_keys: BTreeSet<String>,
    pub next_sample_hint: u64,
}

pub struct CampfireThreatPanelArtifactStoreV1 {
    root: PathBuf,
    manifest: CampfireThreatPanelManifestV1,
    cells: Vec<CampfireThreatPanelCellV1>,
    completed_cell_keys: BTreeSet<String>,
    valid_journal_bytes: Vec<u8>,
    mutation_poisoned: bool,
}

impl CampfireThreatPanelManifestV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        resolved_spec: ResolvedCampfireThreatPanelSpecV1,
        evaluation_context: CampfireEvaluationContext,
        subjects: Vec<CampfireSurvivalSubject>,
        candidate_gaps: Vec<CampfireSurvivalScenarioGapRecord>,
        source_identity: SourceIdentity,
        created_at_unix_ms: u64,
    ) -> Self {
        Self {
            schema_version: CAMPFIRE_THREAT_PANEL_ARTIFACT_SCHEMA_VERSION,
            contract_hash: resolved_spec.contract_hash.clone(),
            resolved_spec,
            evaluation_context,
            subjects,
            candidate_gaps,
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

impl CampfireThreatPanelArtifactStoreV1 {
    pub fn create_or_resume(
        output_dir: &Path,
        expected_manifest: CampfireThreatPanelManifestV1,
    ) -> Result<Self, String> {
        fs::create_dir_all(output_dir).map_err(|error| {
            format!(
                "failed to create Campfire threat panel artifact directory '{}': {error}",
                output_dir.display()
            )
        })?;
        let manifest_path = output_dir.join("manifest.json");
        let manifest = if manifest_path.exists() {
            let existing: CampfireThreatPanelManifestV1 =
                serde_json::from_slice(&fs::read(&manifest_path).map_err(|error| {
                    format!("failed to read '{}': {error}", manifest_path.display())
                })?)
                .map_err(|error| {
                    format!("failed to parse '{}': {error}", manifest_path.display())
                })?;
            validate_resume_identity(&existing, &expected_manifest)?;
            existing
        } else {
            let orphan = ["cells.jsonl", "checkpoint.json", "summary.json"]
                .into_iter()
                .find(|name| output_dir.join(name).exists());
            if let Some(orphan) = orphan {
                return Err(format!(
                    "refusing Campfire threat panel artifact with orphan '{orphan}' and no manifest.json"
                ));
            }
            atomic_write_json(&manifest_path, &expected_manifest)?;
            expected_manifest
        };

        let journal_path = output_dir.join("cells.jsonl");
        let bytes = if journal_path.exists() {
            fs::read(&journal_path)
                .map_err(|error| format!("failed to read '{}': {error}", journal_path.display()))?
        } else {
            Vec::new()
        };
        let valid_len = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        if valid_len != bytes.len() {
            truncate_partial_tail(&journal_path, valid_len as u64)?;
        }
        let valid_journal_bytes = bytes[..valid_len].to_vec();
        let cells = parse_cells(&valid_journal_bytes)?;
        let mut completed_cell_keys = BTreeSet::new();
        for cell in &cells {
            if cell.contract_hash != manifest.contract_hash {
                return Err(format!(
                    "Campfire threat panel cell '{}' has foreign contract hash",
                    cell.cell_key
                ));
            }
            if cell.context_fingerprint != manifest.evaluation_context.context_fingerprint {
                return Err(format!(
                    "Campfire threat panel cell '{}' has foreign evaluation context",
                    cell.cell_key
                ));
            }
            if !completed_cell_keys.insert(cell.cell_key.clone()) {
                return Err(format!(
                    "duplicate Campfire threat panel cell key '{}'",
                    cell.cell_key
                ));
            }
        }
        repair_checkpoint(
            output_dir,
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
            mutation_poisoned: false,
        })
    }

    pub fn manifest(&self) -> &CampfireThreatPanelManifestV1 {
        &self.manifest
    }

    pub fn cells(&self) -> &[CampfireThreatPanelCellV1] {
        &self.cells
    }

    pub fn contains_cell(&self, cell_key: &str) -> bool {
        self.completed_cell_keys.contains(cell_key)
    }

    pub fn append_cell(&mut self, cell: &CampfireThreatPanelCellV1) -> Result<(), String> {
        self.ensure_mutable()?;
        validate_cell_schema_version(cell)?;
        if self.contains_cell(&cell.cell_key) {
            return Err(format!(
                "duplicate Campfire threat panel cell key '{}'",
                cell.cell_key
            ));
        }
        let mut encoded = serde_json::to_vec(cell)
            .map_err(|error| format!("failed to serialize Campfire threat panel cell: {error}"))?;
        encoded.push(b'\n');
        let path = self.root.join("cells.jsonl");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| format!("failed to open '{}': {error}", path.display()))?;
        if let Err(error) = file.write_all(&encoded).and_then(|_| file.sync_data()) {
            self.mutation_poisoned = true;
            return Err(format!(
                "failed to durably append Campfire threat panel journal '{}': {error}",
                path.display()
            ));
        }
        self.valid_journal_bytes.extend_from_slice(&encoded);
        self.completed_cell_keys.insert(cell.cell_key.clone());
        self.cells.push(cell.clone());
        Ok(())
    }

    pub fn checkpoint_sample_boundary(&self, next_sample_hint: u64) -> Result<(), String> {
        self.ensure_mutable()?;
        // The journal is authoritative and the hint is only an accelerator.
        // Preserve a farther existing target when a caller requests a smaller
        // summary window over an already-extended experiment.
        let next_sample_hint = next_sample_hint.max(conservative_next_sample_hint(&self.cells));
        atomic_write_json(
            &self.root.join("checkpoint.json"),
            &CampfireThreatPanelCheckpointV1 {
                schema_version: CAMPFIRE_THREAT_PANEL_ARTIFACT_SCHEMA_VERSION,
                journal_digest: journal_digest(&self.valid_journal_bytes),
                completed_cell_keys: self.completed_cell_keys.clone(),
                next_sample_hint,
            },
        )
    }

    pub fn write_summary<T: Serialize>(&self, summary: &T) -> Result<(), String> {
        self.ensure_mutable()?;
        atomic_write_json(&self.root.join("summary.json"), summary)
    }

    fn ensure_mutable(&self) -> Result<(), String> {
        if self.mutation_poisoned {
            Err("Campfire threat panel store is poisoned after a persistence failure; reopen required"
                .to_string())
        } else {
            Ok(())
        }
    }
}

fn validate_resume_identity(
    existing: &CampfireThreatPanelManifestV1,
    expected: &CampfireThreatPanelManifestV1,
) -> Result<(), String> {
    let comparable = |manifest: &CampfireThreatPanelManifestV1| {
        serde_json::json!({
            "schema_version": manifest.schema_version,
            "contract_hash": manifest.contract_hash,
            "resolved_spec": manifest.resolved_spec,
            "evaluation_context": manifest.evaluation_context,
            "subjects": manifest.subjects,
            "candidate_gaps": manifest.candidate_gaps,
            "source_identity": manifest.source_identity,
            "environment": manifest.environment,
        })
    };
    if comparable(existing) == comparable(expected) {
        Ok(())
    } else {
        Err("Campfire threat panel resume identity does not match manifest".to_string())
    }
}

fn parse_cells(bytes: &[u8]) -> Result<Vec<CampfireThreatPanelCellV1>, String> {
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
                    "malformed Campfire threat panel journal entry at line {}: {error}",
                    index + 1
                )
            })?;
            validate_cell_schema_version_value(probe.schema_version).map_err(|error| {
                format!("Campfire threat panel journal line {}: {error}", index + 1)
            })?;
            let cell: CampfireThreatPanelCellV1 =
                serde_json::from_slice(line).map_err(|error| {
                    format!(
                        "malformed Campfire threat panel journal entry at line {}: {error}",
                        index + 1
                    )
                })?;
            validate_cell_schema_version(&cell).map_err(|error| {
                format!("Campfire threat panel journal line {}: {error}", index + 1)
            })?;
            Ok(cell)
        })
        .collect()
}

fn validate_cell_schema_version(cell: &CampfireThreatPanelCellV1) -> Result<(), String> {
    validate_cell_schema_version_value(cell.schema_version)
}

fn validate_cell_schema_version_value(schema_version: u32) -> Result<(), String> {
    if schema_version == CAMPFIRE_THREAT_PANEL_CELL_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(format!(
            "unsupported Campfire threat panel cell schema_version {}; expected {} and legacy fingerprint cells cannot be resumed",
            schema_version, CAMPFIRE_THREAT_PANEL_CELL_SCHEMA_VERSION
        ))
    }
}

fn truncate_partial_tail(path: &Path, valid_len: u64) -> Result<(), String> {
    let file = OpenOptions::new().write(true).open(path).map_err(|error| {
        format!(
            "failed to open '{}' for tail repair: {error}",
            path.display()
        )
    })?;
    file.set_len(valid_len)
        .and_then(|_| file.sync_data())
        .map_err(|error| {
            format!(
                "failed to repair journal tail '{}': {error}",
                path.display()
            )
        })
}

fn repair_checkpoint(
    output_dir: &Path,
    cells: &[CampfireThreatPanelCellV1],
    keys: &BTreeSet<String>,
    bytes: &[u8],
) -> Result<(), String> {
    let path = output_dir.join("checkpoint.json");
    let expected = CampfireThreatPanelCheckpointV1 {
        schema_version: CAMPFIRE_THREAT_PANEL_ARTIFACT_SCHEMA_VERSION,
        journal_digest: journal_digest(bytes),
        completed_cell_keys: keys.clone(),
        next_sample_hint: conservative_next_sample_hint(cells),
    };
    let agrees = path.exists()
        && fs::read(&path)
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<CampfireThreatPanelCheckpointV1>(&bytes).ok()
            })
            .is_some_and(|checkpoint| {
                checkpoint.schema_version == expected.schema_version
                    && checkpoint.journal_digest == expected.journal_digest
                    && checkpoint.completed_cell_keys == expected.completed_cell_keys
                    && checkpoint.next_sample_hint == expected.next_sample_hint
            });
    if agrees || (!path.exists() && cells.is_empty()) {
        Ok(())
    } else {
        atomic_write_json(&path, &expected)
    }
}

fn conservative_next_sample_hint(cells: &[CampfireThreatPanelCellV1]) -> u64 {
    cells
        .iter()
        .map(|cell| cell.sample_index)
        .max()
        .map_or(0, |sample| sample.saturating_add(1))
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
