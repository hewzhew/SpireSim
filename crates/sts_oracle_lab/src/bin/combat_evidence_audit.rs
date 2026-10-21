//! Batch indexing and typed replay for local combat evidence artifacts.
//!
//! This is a diagnostic surface. It discovers exact case/action relationships
//! declared by producer manifests or traces, records typed same-turn bypass
//! counterfactuals, and executes bounded query batches without changing search
//! or run policy. Undeclared action files remain explicit unknowns.

mod artifacts;
mod contract;
#[cfg(test)]
mod manifest_tests;
mod query;
mod replay;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sts_oracle_runtime::content::cards::{CardId, CardType};
use sts_oracle_runtime::eval::combat_case_context::CombatCaseReplayIdentityV2;
use sts_oracle_runtime::sim::combat::CombatPosition;
use sts_oracle_runtime::sim::combat::CombatTerminal;
use sts_oracle_runtime::state::core::ClientInput;

use self::artifacts::*;
use self::contract::*;
use self::query::*;
use self::replay::*;
use super::canonical_launch::{runtime_identity, runtime_source_content_fingerprint};

const SUMMARY_SCHEMA_NAME: &str = "CombatEvidenceAuditSummaryV4";
const EVIDENCE_SCHEMA_NAME: &str = "CombatEvidenceReplayV3";

#[derive(Debug, Args)]
pub(super) struct CombatEvidenceAuditArgs {
    /// Artifact tree to scan. The command never mutates source artifacts.
    #[arg(long, default_value = ".oracle-lab")]
    root: PathBuf,
    /// Fresh ignored directory for evidence artifacts. Defaults to a unique report directory.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Removed compatibility switch. Directory inference is no longer an
    /// evidence source; producers must declare the case/action relationship.
    #[arg(long, hide = true)]
    replay_untraced: bool,
    /// Optional CombatEvidenceQueryBatchV1 JSON file; use '-' to read one batch from stdin.
    #[arg(long)]
    query_batch: Option<PathBuf>,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CardObservation {
    id: CardId,
    lineage: sts_oracle_runtime::runtime::combat::CardLineageId,
    #[serde(default)]
    upgrades: u8,
    #[serde(default)]
    cost_for_turn: Option<u8>,
    #[serde(default)]
    free_to_play_once: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PlayerObservation {
    hp: i32,
    block: i32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct MonsterObservation {
    id: usize,
    #[serde(default)]
    hp: i32,
    #[serde(default)]
    max_hp: i32,
    #[serde(default)]
    block: i32,
    #[serde(default)]
    slot: u8,
    #[serde(default)]
    is_dying: bool,
    #[serde(default)]
    half_dead: bool,
    #[serde(default)]
    is_escaped: bool,
}

impl MonsterObservation {
    fn terminal_like(&self) -> bool {
        self.hp <= 0 || self.is_dying || self.half_dead || self.is_escaped
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct StateObservation {
    turn: u32,
    energy: i32,
    player: PlayerObservation,
    #[serde(default)]
    hand: Vec<CardObservation>,
    #[serde(default)]
    monsters: Vec<MonsterObservation>,
}

impl StateObservation {
    fn monster(&self, target: usize) -> Option<&MonsterObservation> {
        self.monsters.iter().find(|monster| monster.id == target)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ActionObservation {
    index: usize,
    input: ClientInput,
    card: Option<CardObservation>,
    card_type: Option<CardType>,
    before: StateObservation,
    after: StateObservation,
    terminal_after: CombatTerminal,
    #[serde(default)]
    previous_card_bypass: Option<PreviousCardBypassObservation>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct EvidenceRecord {
    schema_name: String,
    schema_version: u32,
    record_id: String,
    root_exact_state_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    case_identity: Option<CombatCaseReplayIdentityV2>,
    action_sequence_blake2b_512: String,
    provenance: BTreeSet<String>,
    source_paths: BTreeSet<String>,
    case_path: Option<String>,
    action_paths: Vec<String>,
    replay_exact: bool,
    supplied_action_count: usize,
    consumed_action_count: usize,
    final_terminal: CombatTerminal,
    final_player_hp: i32,
    actions: Vec<ActionObservation>,
    fiend_fire_observations: Vec<FiendFireObservation>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FiendFireObservation {
    record_id: String,
    root_exact_state_hash: String,
    turn: u32,
    previous_action_index: Option<usize>,
    fiend_fire_action_index: usize,
    previous_card: Option<CardId>,
    previous_card_type: Option<CardType>,
    target_id: Option<usize>,
    target_before_previous: Option<MonsterObservation>,
    target_after_previous: Option<MonsterObservation>,
    target_before_fiend_fire: Option<MonsterObservation>,
    target_after_fiend_fire: Option<MonsterObservation>,
    immediate_fiend_fire: PreviousCardBypassObservation,
    full_line_terminal: CombatTerminal,
    classification: FiendFireClassification,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum PreviousCardBypassStatus {
    Applied,
    NoPreviousCardBoundary,
    MissingCardIdentity,
    NotCardPlay,
    CardNotInPreviousHand,
    IllegalAtPreviousBoundary,
    TransitionLimited,
    TraceOnlyUnavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PreviousCardBypassObservation {
    previous_action_index: Option<usize>,
    status: PreviousCardBypassStatus,
    terminal_after: Option<CombatTerminal>,
    after: Option<StateObservation>,
}

impl PreviousCardBypassObservation {
    fn target_after(&self, target: usize) -> Option<&MonsterObservation> {
        self.after.as_ref()?.monster(target)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum FiendFireClassification {
    NoPreviousCard,
    PreviousCardNotAttack,
    FiendFireHasNoTarget,
    MissingPreviousTargetState,
    NoPositiveBlockBeforePreviousAttack,
    PreviousAttackDidNotReduceTargetBlock,
    FiendFireNotTerminalLike,
    ImmediateFiendFireAlreadyTerminalLike,
    ConfirmedBlockConversionWindow,
    LocalBlockConversionWithoutCompleteWin,
    ObservedBlockConversionCandidate,
    BlockConversionCounterfactualUnknown,
}

#[derive(Clone, Debug)]
struct ReplayFrame {
    before_position: CombatPosition,
    observation: ActionObservation,
}

#[derive(Clone, Debug)]
struct PairCandidate {
    case_path: PathBuf,
    action_paths: Vec<PathBuf>,
    provenance: BTreeSet<String>,
    source_paths: BTreeSet<String>,
    expectations: ReplayExpectations,
}

#[derive(Clone, Debug, Default)]
struct ReplayExpectations {
    case_identities: BTreeSet<CombatCaseReplayIdentityV2>,
    root_exact_state_hashes: BTreeSet<String>,
    action_sequence_blake2b_512: BTreeSet<String>,
    supplied_action_counts: BTreeSet<usize>,
    final_terminals: Vec<CombatTerminal>,
    final_player_hps: BTreeSet<i32>,
}

#[derive(Clone, Debug, Serialize)]
struct UnresolvedArtifact {
    path: String,
    reason: String,
}

#[derive(Debug, Default)]
struct ArtifactInventory {
    manifest_files: Vec<PathBuf>,
    trace_files: Vec<PathBuf>,
    case_files: Vec<PathBuf>,
    action_files: Vec<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct ContractTrace {
    root_exact_state_hash: String,
    actions: Vec<ContractTraceAction>,
}

#[derive(Debug, Deserialize)]
struct ContractTraceAction {
    index: usize,
    input: ClientInput,
    #[serde(default)]
    subject: Option<ContractTraceSubject>,
    before: StateObservation,
    after: StateObservation,
}

#[derive(Debug, Deserialize)]
struct ContractTraceSubject {
    #[serde(default)]
    card: Option<CardObservation>,
}

#[derive(Debug, Serialize)]
pub(super) struct CombatEvidenceAuditSummary {
    schema_name: &'static str,
    schema_version: u32,
    contract: Value,
    runtime: Value,
    runtime_source_content_fingerprint: String,
    scan_root: String,
    output_directory: String,
    trace_files: usize,
    manifest_files: usize,
    case_files: usize,
    action_files: usize,
    trace_schema_counts: BTreeMap<String, usize>,
    non_replay_trace_files: usize,
    declared_replay_pairs: usize,
    manifest_replay_pairs: usize,
    unassociated_action_files: usize,
    exact_replays: usize,
    replay_failures: usize,
    contract_trace_records: usize,
    deduplicated_evidence_records: usize,
    fiend_fire_plays: usize,
    fiend_fire_classifications: BTreeMap<FiendFireClassification, usize>,
    confirmed_independent_root_count: usize,
    unresolved_artifact_count: usize,
    unresolved_reason_counts: BTreeMap<String, usize>,
    query_results: Option<CombatEvidenceQueryBatchSummary>,
    output_files: Vec<String>,
}

pub(super) fn run(args: CombatEvidenceAuditArgs) -> Result<CombatEvidenceAuditSummary, String> {
    if args.replay_untraced {
        return Err(
            "--replay-untraced is no longer supported; export a producer manifest for each exact case/action relationship"
                .to_string(),
        );
    }
    let output = args.output.unwrap_or_else(default_output_directory);
    if output.exists() {
        return Err(format!(
            "output directory already exists; choose a fresh path: {}",
            output.display()
        ));
    }
    let query_batch = args
        .query_batch
        .as_deref()
        .map(read_query_batch)
        .transpose()?;
    let scan_root = args.root.canonicalize().map_err(|error| {
        format!(
            "cannot resolve scan root '{}': {error}",
            args.root.display()
        )
    })?;
    let current_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let inventory = collect_artifacts(&scan_root)?;
    let mut unresolved = Vec::new();
    let mut schema_counts = BTreeMap::<String, usize>::new();
    let mut pair_candidates = BTreeMap::<String, PairCandidate>::new();
    let mut contract_records = Vec::new();
    let mut referenced_actions = BTreeSet::<PathBuf>::new();
    let mut declared_pair_keys = BTreeSet::new();
    let mut manifest_pair_keys = BTreeSet::new();
    let mut non_replay_trace_files = 0usize;

    for manifest_path in &inventory.manifest_files {
        match declared_manifest_pairs(manifest_path, &scan_root, &current_dir) {
            Ok(candidates) => {
                for candidate in candidates {
                    for action_path in &candidate.action_paths {
                        referenced_actions.insert(action_path.clone());
                    }
                    let key = pair_key(&candidate.case_path, &candidate.action_paths);
                    manifest_pair_keys.insert(key.clone());
                    merge_pair(&mut pair_candidates, key, candidate);
                }
            }
            Err(error) => unresolved.push(UnresolvedArtifact {
                path: display_path(manifest_path),
                reason: error,
            }),
        }
    }

    for trace_path in &inventory.trace_files {
        let value = match read_json_value(trace_path) {
            Ok(value) => value,
            Err(error) => {
                unresolved.push(UnresolvedArtifact {
                    path: display_path(trace_path),
                    reason: error,
                });
                continue;
            }
        };
        let schema = value
            .get("schema_name")
            .and_then(Value::as_str)
            .unwrap_or("<missing>")
            .to_string();
        *schema_counts.entry(schema.clone()).or_default() += 1;
        if schema == "CombatContractWitnessTraceV1" {
            match contract_record(trace_path, value) {
                Ok(record) => contract_records.push(record),
                Err(error) => unresolved.push(UnresolvedArtifact {
                    path: display_path(trace_path),
                    reason: error,
                }),
            }
            continue;
        }
        match declared_pair(trace_path, &value, &scan_root, &current_dir) {
            Ok(Some(candidate)) => {
                for action_path in &candidate.action_paths {
                    referenced_actions.insert(action_path.clone());
                }
                let key = pair_key(&candidate.case_path, &candidate.action_paths);
                declared_pair_keys.insert(key.clone());
                merge_pair(&mut pair_candidates, key, candidate);
            }
            Ok(None) => {
                non_replay_trace_files = non_replay_trace_files.saturating_add(1);
            }
            Err(error) => unresolved.push(UnresolvedArtifact {
                path: display_path(trace_path),
                reason: error,
            }),
        }
    }

    let unassociated_action_files =
        report_unassociated_actions(&inventory, &referenced_actions, &mut unresolved)?;

    let mut replay_failures = 0usize;
    let mut records = contract_records;
    for candidate in pair_candidates.values() {
        match replay_pair(candidate, args.max_engine_steps_per_transition) {
            Ok(record) => records.push(record),
            Err(error) => {
                replay_failures = replay_failures.saturating_add(1);
                unresolved.push(UnresolvedArtifact {
                    path: candidate
                        .source_paths
                        .iter()
                        .next()
                        .cloned()
                        .unwrap_or_else(|| display_path(&candidate.case_path)),
                    reason: error,
                });
            }
        }
    }

    let exact_replays = records.iter().filter(|record| record.replay_exact).count();
    let contract_trace_records = records.iter().filter(|record| !record.replay_exact).count();
    let records = deduplicate_records(records);
    let windows = records
        .iter()
        .flat_map(|record| record.fiend_fire_observations.iter().cloned())
        .collect::<Vec<_>>();
    let mut classifications = BTreeMap::<FiendFireClassification, usize>::new();
    for window in &windows {
        *classifications.entry(window.classification).or_default() += 1;
    }
    let confirmed_roots = windows
        .iter()
        .filter(|window| {
            window.classification == FiendFireClassification::ConfirmedBlockConversionWindow
        })
        .map(|window| window.root_exact_state_hash.clone())
        .collect::<BTreeSet<_>>();
    let mut unresolved_reason_counts = BTreeMap::<String, usize>::new();
    for artifact in &unresolved {
        *unresolved_reason_counts
            .entry(unresolved_category(&artifact.reason).to_string())
            .or_default() += 1;
    }

    fs::create_dir_all(&output).map_err(|error| {
        format!(
            "cannot create output directory '{}': {error}",
            output.display()
        )
    })?;
    let evidence_path = output.join("evidence.jsonl");
    write_jsonl(&evidence_path, &records)?;
    let windows_path = output.join("fiend-fire-windows.json");
    write_json(&windows_path, &windows)?;
    let unresolved_path = output.join("unresolved.json");
    write_json(&unresolved_path, &unresolved)?;
    let summary_path = output.join("summary.json");
    let mut query_output_paths = Vec::new();
    let query_results = if let Some(batch) = query_batch.as_ref() {
        let batch_path = output.join("query-batch.json");
        write_json(&batch_path, batch)?;
        let results = execute_query_batch(batch, &records)?;
        let results_path = output.join("query-results.json");
        write_json(&results_path, &results)?;
        query_output_paths.push(batch_path);
        query_output_paths.push(results_path);
        Some(results.summary())
    } else {
        None
    };

    let mut output_files = vec![
        display_path(&evidence_path),
        display_path(&windows_path),
        display_path(&unresolved_path),
        display_path(&summary_path),
    ];
    for path in &query_output_paths {
        output_files.push(display_path(path));
    }

    let summary = CombatEvidenceAuditSummary {
        schema_name: SUMMARY_SCHEMA_NAME,
        schema_version: 4,
        contract: json!({
            "classification": "diagnostic",
            "search": false,
            "policy_mutation": false,
            "ranking": false,
            "source_artifact_mutation": false,
            "missing_or_failed_replay": "explicit_unknown",
            "manifest_identity": "replay_revalidated",
        }),
        runtime: runtime_identity(),
        runtime_source_content_fingerprint: runtime_source_content_fingerprint()?,
        scan_root: display_path(&scan_root),
        output_directory: display_path(&output),
        trace_files: inventory.trace_files.len(),
        manifest_files: inventory.manifest_files.len(),
        case_files: inventory.case_files.len(),
        action_files: inventory.action_files.len(),
        trace_schema_counts: schema_counts,
        non_replay_trace_files,
        declared_replay_pairs: declared_pair_keys.len(),
        manifest_replay_pairs: manifest_pair_keys.len(),
        unassociated_action_files,
        exact_replays,
        replay_failures,
        contract_trace_records,
        deduplicated_evidence_records: records.len(),
        fiend_fire_plays: windows.len(),
        fiend_fire_classifications: classifications,
        confirmed_independent_root_count: confirmed_roots.len(),
        unresolved_artifact_count: unresolved.len(),
        unresolved_reason_counts,
        query_results,
        output_files,
    };
    write_json(&summary_path, &summary)?;
    Ok(summary)
}

fn default_output_directory() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    PathBuf::from(format!(
        ".oracle-lab/reports/combat-evidence-audit-{suffix}"
    ))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let file = File::create(path)
        .map_err(|error| format!("cannot create '{}': {error}", path.display()))?;
    serde_json::to_writer_pretty(BufWriter::new(file), value)
        .map_err(|error| format!("cannot write '{}': {error}", path.display()))
}

fn write_jsonl(path: &Path, values: &[EvidenceRecord]) -> Result<(), String> {
    let file = File::create(path)
        .map_err(|error| format!("cannot create '{}': {error}", path.display()))?;
    let mut writer = BufWriter::new(file);
    for value in values {
        serde_json::to_writer(&mut writer, value)
            .map_err(|error| format!("cannot write '{}': {error}", path.display()))?;
        writer
            .write_all(b"\n")
            .map_err(|error| format!("cannot write '{}': {error}", path.display()))?;
    }
    Ok(())
}

fn display_path(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let text = absolute.to_string_lossy();
    text.strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .replace('\\', "/")
}

fn unresolved_category(reason: &str) -> &'static str {
    if reason.starts_with("unassociated action sequence") {
        "unassociated_action"
    } else if reason.starts_with("ambiguous action sequence") {
        "ambiguous_action_pairing"
    } else if reason.contains("declared case path is missing")
        || reason.contains("declared action path is missing")
    {
        "missing_declared_path"
    } else if reason.contains("manifest case path is missing")
        || reason.contains("manifest action path is missing")
        || reason.contains("manifest trace path is missing")
    {
        "missing_manifest_path"
    } else if reason.contains("pair replay") {
        "replay_rejected"
    } else if reason.contains("cannot parse") || reason.contains("cannot decode") {
        "parse_or_schema_error"
    } else if reason.contains("combat evidence manifest") {
        "invalid_manifest"
    } else {
        "other"
    }
}
