use crate::{
    ComparisonStatusV1, DomainEvidenceV1, ExecutionInputsV1, LockstepModeV1,
    NaturalRunRngCallComparisonV1, ParityDomainV1, ParitySnapshotV1, ProducerIdentityV1,
    SemanticActionV1, SnapshotComparisonV1, TransitionJournalEntryV1, TransitionOutcomeV1,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const LOCKSTEP_JOURNAL_SCHEMA_V2: &str = "sts-lockstep-journal-v2";
pub const LOCKSTEP_TRANSITION_SCHEMA_V4: &str = "sts-lockstep-transition-v4";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct LockstepJournalHeaderV2 {
    pub schema: String,
    pub session_id: String,
    pub mode: LockstepModeV1,
    pub java_initial: SnapshotReceiptV2,
    pub rust_initial: SnapshotReceiptV2,
    pub initial_comparison: ComparisonReceiptV2,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SnapshotReceiptV2 {
    observation_id: String,
    boundary_kind: String,
    producer: ProducerIdentityV1,
    contract_schema: String,
    domains: Vec<SnapshotDomainReceiptV2>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct SnapshotDomainReceiptV2 {
    domain: ParityDomainV1,
    critical: bool,
    schema: Option<String>,
    digest: Option<String>,
    unobserved_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ComparisonReceiptV2 {
    status: ComparisonStatusV1,
    contract_exact: bool,
    contract_reason: Option<String>,
    boundary_exact: bool,
    boundary_reason: Option<String>,
    domains: Vec<ComparisonDomainReceiptV2>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ComparisonDomainReceiptV2 {
    domain: ParityDomainV1,
    status: ComparisonStatusV1,
    critical: bool,
    reason: Option<String>,
    java_digest: Option<String>,
    rust_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredTransitionJournalEntryV4 {
    schema: String,
    index: u64,
    action: SemanticActionV1,
    java_after: Option<SnapshotReceiptV2>,
    rust_after: Option<SnapshotReceiptV2>,
    comparison: Option<ComparisonReceiptV2>,
    rng_call_comparison: Option<NaturalRunRngCallComparisonV1>,
    execution_inputs: Option<ExecutionInputsV1>,
    outcome: TransitionOutcomeV1,
}

#[derive(Debug)]
pub struct LockstepJournalStoreV1 {
    directory: PathBuf,
    next_step: u64,
}

impl LockstepJournalStoreV1 {
    pub fn create_new(
        directory: &Path,
        session_id: impl Into<String>,
        mode: LockstepModeV1,
        java_initial: ParitySnapshotV1,
        rust_initial: ParitySnapshotV1,
        initial_comparison: SnapshotComparisonV1,
    ) -> Result<Self, String> {
        let session_id = session_id.into();
        if session_id.is_empty() {
            return Err("lockstep session ID is empty".to_owned());
        }
        if directory.exists() {
            if !directory.is_dir() {
                return Err("lockstep journal path is not a directory".to_owned());
            }
        } else {
            fs::create_dir(directory)
                .map_err(|error| format!("failed to create lockstep journal directory: {error}"))?;
        }
        fs::create_dir(directory.join("steps"))
            .map_err(|error| format!("failed to create lockstep step directory: {error}"))?;
        let header = LockstepJournalHeaderV2 {
            schema: LOCKSTEP_JOURNAL_SCHEMA_V2.to_owned(),
            session_id,
            mode,
            java_initial: snapshot_receipt(&java_initial),
            rust_initial: snapshot_receipt(&rust_initial),
            initial_comparison: comparison_receipt(&initial_comparison),
        };
        write_json_create_new(&directory.join("session.json"), &header)?;
        Ok(Self {
            directory: directory.to_path_buf(),
            next_step: 0,
        })
    }

    pub fn append(&mut self, entry: &TransitionJournalEntryV1) -> Result<PathBuf, String> {
        let _timing = crate::timing_span_v1("journal.append");
        if entry.index != self.next_step {
            return Err(format!(
                "lockstep journal expected step {}, received {}",
                self.next_step, entry.index
            ));
        }
        let path = self
            .directory
            .join("steps")
            .join(format!("{:08}.json", entry.index));
        let stored = StoredTransitionJournalEntryV4 {
            schema: LOCKSTEP_TRANSITION_SCHEMA_V4.to_owned(),
            index: entry.index,
            action: entry.action.clone(),
            java_after: entry.java_after.as_ref().map(snapshot_receipt),
            rust_after: entry.rust_after.as_ref().map(snapshot_receipt),
            comparison: entry.comparison.as_ref().map(comparison_receipt),
            rng_call_comparison: entry.rng_call_comparison.clone(),
            execution_inputs: entry.execution_inputs.clone(),
            outcome: entry.outcome.clone(),
        };
        write_json_create_new(&path, &stored)?;
        self.next_step = self
            .next_step
            .checked_add(1)
            .ok_or_else(|| "lockstep journal step overflowed u64".to_owned())?;
        Ok(path)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

fn snapshot_receipt(snapshot: &ParitySnapshotV1) -> SnapshotReceiptV2 {
    let domains = snapshot
        .domains
        .iter()
        .map(|(domain, observation)| {
            let (schema, digest, unobserved_reason) = match &observation.evidence {
                DomainEvidenceV1::Observed { schema, value } => (
                    Some(schema.clone()),
                    Some(canonical_json_digest(value)),
                    None,
                ),
                DomainEvidenceV1::Unobserved { reason } => (None, None, Some(reason.clone())),
            };
            SnapshotDomainReceiptV2 {
                domain: *domain,
                critical: snapshot.contract.critical(*domain).unwrap_or(false),
                schema,
                digest,
                unobserved_reason,
            }
        })
        .collect();
    SnapshotReceiptV2 {
        observation_id: snapshot.observation_id.clone(),
        boundary_kind: snapshot.boundary_kind.clone(),
        producer: snapshot.producer.clone(),
        contract_schema: snapshot.contract.schema.clone(),
        domains,
    }
}

fn comparison_receipt(comparison: &SnapshotComparisonV1) -> ComparisonReceiptV2 {
    ComparisonReceiptV2 {
        status: comparison.status,
        contract_exact: comparison.contract_exact,
        contract_reason: comparison.contract_reason.clone(),
        boundary_exact: comparison.boundary_exact,
        boundary_reason: comparison.boundary_reason.clone(),
        domains: comparison
            .domains
            .iter()
            .map(|domain| ComparisonDomainReceiptV2 {
                domain: domain.domain,
                status: domain.status,
                critical: domain.critical,
                reason: domain.reason.clone(),
                java_digest: domain.java.as_ref().map(canonical_json_digest),
                rust_digest: domain.rust.as_ref().map(canonical_json_digest),
            })
            .collect(),
    }
}

/// Hash canonical JSON tokens rather than serde's object insertion order, so
/// equal endpoint values always produce the same receipt.
fn canonical_json_digest(value: &Value) -> String {
    let mut hasher = Sha256::new();
    update_canonical_json_digest(&mut hasher, value);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn update_canonical_json_digest(hasher: &mut Sha256, value: &Value) {
    match value {
        Value::Null => hasher.update(b"null"),
        Value::Bool(value) => {
            hasher.update(if *value { &b"true"[..] } else { &b"false"[..] });
        }
        Value::Number(value) => hasher.update(value.to_string().as_bytes()),
        Value::String(value) => hasher.update(
            serde_json::to_vec(value).expect("a JSON string always has a canonical encoding"),
        ),
        Value::Array(values) => {
            hasher.update(b"[");
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    hasher.update(b",");
                }
                update_canonical_json_digest(hasher, value);
            }
            hasher.update(b"]");
        }
        Value::Object(values) => {
            hasher.update(b"{");
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort();
            for (index, key) in keys.into_iter().enumerate() {
                if index != 0 {
                    hasher.update(b",");
                }
                hasher.update(
                    serde_json::to_vec(key)
                        .expect("a JSON object key always has a canonical encoding"),
                );
                hasher.update(b":");
                update_canonical_json_digest(
                    hasher,
                    values.get(key).expect("the key came from this JSON object"),
                );
            }
            hasher.update(b"}");
        }
    }
}

fn write_json_create_new<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode lockstep journal JSON: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create lockstep journal file: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist lockstep journal file: {error}"))
}
