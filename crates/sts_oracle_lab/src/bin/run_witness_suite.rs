use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sts_oracle_runtime::eval::run_control::{
    exact_replay_run_progress_journal_v1, ExactRunProgressReplayReportV1,
};
use sts_oracle_runtime::runtime::branch::load_oracle_run_continuation_v1;

const MANIFEST_SCHEMA_NAME: &str = "OracleRunWitnessSuiteManifestV1";
const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Args)]
pub struct RunWitnessSuiteArgs {
    /// Compact manifest whose paths are resolved relative to the manifest.
    #[arg(long)]
    pub manifest: PathBuf,
    /// List the manifest's typed witness inventory without loading or
    /// replaying continuation payloads.
    #[arg(long)]
    pub list: bool,
    /// Replay only the exact manifest witness with this id.
    #[arg(long)]
    pub witness: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunWitnessSuiteManifestV1 {
    schema_name: String,
    schema_version: u32,
    witnesses: Vec<RunWitnessSuiteEntryV1>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunWitnessSuiteEntryV1 {
    id: String,
    continuation: PathBuf,
    expected: ExactRunProgressReplayReportV1,
}

#[derive(Debug, Serialize)]
struct RunWitnessSuiteReportV1 {
    schema_name: &'static str,
    schema_version: u32,
    manifest: PathBuf,
    exact_witnesses_verified: usize,
    witnesses: Vec<RunWitnessSuiteWitnessReportV1>,
}

#[derive(Debug, Serialize)]
struct RunWitnessSuiteWitnessReportV1 {
    id: String,
    continuation: PathBuf,
    replay: ExactRunProgressReplayReportV1,
}

#[derive(Debug, Serialize)]
struct RunWitnessSuiteInventoryV1 {
    schema_name: &'static str,
    schema_version: u32,
    manifest: PathBuf,
    witness_count: usize,
    witnesses: Vec<RunWitnessSuiteInventoryEntryV1>,
}

#[derive(Debug, Serialize)]
struct RunWitnessSuiteInventoryEntryV1 {
    id: String,
    continuation: PathBuf,
    expected: ExactRunProgressReplayReportV1,
}

pub fn verify_run_witness_suite(args: RunWitnessSuiteArgs) -> Result<Value, String> {
    let bytes = fs::read(&args.manifest)
        .map_err(|error| format!("failed to read {}: {error}", args.manifest.display()))?;
    let mut manifest = serde_json::from_slice::<RunWitnessSuiteManifestV1>(&bytes)
        .map_err(|error| format!("failed to parse {}: {error}", args.manifest.display()))?;
    if manifest.schema_name != MANIFEST_SCHEMA_NAME
        || manifest.schema_version != MANIFEST_SCHEMA_VERSION
    {
        return Err(format!(
            "unsupported run-witness suite schema '{}'/{}",
            manifest.schema_name, manifest.schema_version
        ));
    }
    if manifest.witnesses.is_empty() {
        return Err("run-witness suite manifest contains no witnesses".to_string());
    }

    let root = args.manifest.parent().unwrap_or_else(|| Path::new("."));
    if let Some(witness_id) = args.witness.as_deref() {
        let available = manifest
            .witnesses
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();
        manifest.witnesses.retain(|entry| entry.id == witness_id);
        if manifest.witnesses.is_empty() {
            return Err(format!(
                "run-witness suite has no witness id '{witness_id}'; available ids: [{}]",
                available.join(", ")
            ));
        }
    }
    if args.list {
        let witnesses = manifest
            .witnesses
            .into_iter()
            .map(|entry| RunWitnessSuiteInventoryEntryV1 {
                id: entry.id,
                continuation: resolve_continuation_path(root, &entry.continuation),
                expected: entry.expected,
            })
            .collect::<Vec<_>>();
        return serde_json::to_value(RunWitnessSuiteInventoryV1 {
            schema_name: "OracleRunWitnessSuiteInventoryV1",
            schema_version: 1,
            manifest: args.manifest,
            witness_count: witnesses.len(),
            witnesses,
        })
        .map_err(|error| format!("failed to encode run-witness suite inventory: {error}"));
    }

    let mut reports = Vec::with_capacity(manifest.witnesses.len());
    for entry in manifest.witnesses {
        let continuation_path = resolve_continuation_path(root, &entry.continuation);
        let continuation = load_oracle_run_continuation_v1(&continuation_path)?;
        let expected_final = continuation.session.into_session()?;
        let replay = exact_replay_run_progress_journal_v1(
            continuation.seed,
            continuation.ascension,
            &continuation.journal,
            &expected_final,
        )?;
        if replay != entry.expected {
            return Err(format!(
                "run-witness '{}' replayed exactly but no longer identifies the manifest witness:\nexpected {}\nactual   {}",
                entry.id,
                serde_json::to_string(&entry.expected)
                    .map_err(|error| format!("failed to encode expected replay: {error}"))?,
                serde_json::to_string(&replay)
                    .map_err(|error| format!("failed to encode actual replay: {error}"))?,
            ));
        }

        reports.push(RunWitnessSuiteWitnessReportV1 {
            id: entry.id,
            continuation: continuation_path,
            replay,
        });
    }

    serde_json::to_value(RunWitnessSuiteReportV1 {
        schema_name: "OracleRunWitnessSuiteReportV1",
        schema_version: 1,
        manifest: args.manifest,
        exact_witnesses_verified: reports.len(),
        witnesses: reports,
    })
    .map_err(|error| format!("failed to encode run-witness suite report: {error}"))
}

fn resolve_continuation_path(root: &Path, continuation: &Path) -> PathBuf {
    if continuation.is_absolute() {
        continuation.to_path_buf()
    } else {
        root.join(continuation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_returns_typed_inventory_without_loading_continuations() {
        let directory =
            std::env::temp_dir().join(format!("sts-run-witness-suite-list-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("create temporary suite directory");
        let manifest = directory.join("suite.json");
        fs::write(
            &manifest,
            br#"{
                "schema_name":"OracleRunWitnessSuiteManifestV1",
                "schema_version":1,
                "witnesses":[{
                    "id":"seed006",
                    "continuation":"missing-but-not-loaded.json",
                    "expected":{
                        "seed":6,
                        "ascension":0,
                        "journal_entries":12,
                        "decisions":4,
                        "forced_transitions":3,
                        "combat_resolutions":5,
                        "combat_actions":42,
                        "final_fingerprint":"abc",
                        "act":3,
                        "floor":56,
                        "current_hp":10,
                        "max_hp":80,
                        "engine_state":"GameOver"
                    }
                }]
            }"#,
        )
        .expect("write temporary suite manifest");

        let inventory = verify_run_witness_suite(RunWitnessSuiteArgs {
            manifest: manifest.clone(),
            list: true,
            witness: Some("seed006".to_owned()),
        })
        .expect("list should not load the missing continuation");

        assert_eq!(inventory["schema_name"], "OracleRunWitnessSuiteInventoryV1");
        assert_eq!(inventory["witness_count"], 1);
        assert_eq!(inventory["witnesses"][0]["id"], "seed006");
        assert_eq!(
            inventory["witnesses"][0]["continuation"],
            directory
                .join("missing-but-not-loaded.json")
                .to_string_lossy()
                .as_ref()
        );
        fs::remove_dir_all(directory).expect("remove temporary suite directory");
    }
}
