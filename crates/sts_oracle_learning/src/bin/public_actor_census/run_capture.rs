use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sts_oracle_learning::agent::task::TaskEpisodeOutcomeV1;
use sts_oracle_learning::content::monsters::factory::EncounterId;
use sts_oracle_learning::eval::run_control::{
    CombatBaselineOutcomeV1, ExactBranchingTaskExecutionEvidenceV1, PublicActorCensusV9,
};

use super::{json_payload, write_payload_new};

const COMBAT_RUN_CAPTURE_SCHEMA_V1: &str = "sts-combat-run-capture-v1";

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CombatRunCaptureManifestV1 {
    schema: String,
    source_receipt_file: String,
    seed: u64,
    episode_outcome: TaskEpisodeOutcomeV1,
    curriculum_eligible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    curriculum_ineligibility: Option<String>,
    captured_combat_entries: usize,
    committed_combat_count: usize,
    committed_combat_actions: usize,
    actor_installed_plans: usize,
    actor_plan_actions: usize,
    combats: Vec<CombatRunCaptureEntryV1>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CombatRunCaptureEntryV1 {
    ordinal: usize,
    public_snapshot_id: String,
    act: u8,
    floor: i32,
    encounter_id: Option<EncounterId>,
    root_exact_state_hash: String,
    run_session_fingerprint: String,
    action_count: usize,
    outcome: CombatBaselineOutcomeV1,
    case_file: String,
    actions_file: String,
}

struct CombatRunCaptureFileV1 {
    path: PathBuf,
    payload: Vec<u8>,
    description: &'static str,
}

pub(super) struct CombatRunCapturePayloadV1 {
    directory: PathBuf,
    files: Vec<CombatRunCaptureFileV1>,
    manifest_path: PathBuf,
    manifest_payload: Vec<u8>,
}

pub(super) fn prepare_combat_run_capture_v1(
    directory: &Path,
    receipt_path: &Path,
    census: &PublicActorCensusV9,
    execution: &ExactBranchingTaskExecutionEvidenceV1,
) -> Result<CombatRunCapturePayloadV1, String> {
    let [episode] = census.episodes.as_slice() else {
        return Err("combat-run capture requires exactly one episode".to_string());
    };
    let combats = &execution.committed_combats;
    let committed_combat_actions = combats
        .iter()
        .map(|combat| combat.actions.len())
        .sum::<usize>();
    let successful = census.summary.successes == 1;
    let complete_alignment = execution.captured_combat_entries.len() == combats.len()
        && combats.len() == episode.combat_research_plans_installed
        && committed_combat_actions == episode.combat_research_plan_actions;
    let curriculum_eligible = successful && complete_alignment;
    let curriculum_ineligibility = if curriculum_eligible {
        None
    } else if !successful {
        Some(format!("episode outcome is {:?}", episode.outcome))
    } else {
        Some(format!(
            "successful episode evidence is incomplete: captured_entries={} committed_combats={} installed_plans={} committed_actions={} actor_plan_actions={}",
            execution.captured_combat_entries.len(),
            combats.len(),
            episode.combat_research_plans_installed,
            committed_combat_actions,
            episode.combat_research_plan_actions
        ))
    };
    let receipt_file = receipt_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "combat-run receipt path has no UTF-8 file name".to_string())?
        .to_owned();
    let mut files = Vec::with_capacity(combats.len() * 2);
    let mut entries = Vec::with_capacity(combats.len());
    let mut seen_roots = BTreeSet::new();
    for (ordinal, combat) in combats.iter().enumerate() {
        if !seen_roots.insert(combat.root_public_snapshot_id.clone()) {
            return Err(format!(
                "combat-run capture repeats public root {}",
                combat.root_public_snapshot_id
            ));
        }
        if combat.case.core.source.seed != episode.seed {
            return Err(format!(
                "combat {ordinal} seed {} differs from episode seed {}",
                combat.case.core.source.seed, episode.seed
            ));
        }
        let identity = combat.case.replay_identity_v2()?;
        let run_session_fingerprint = identity
            .run_session_fingerprint
            .ok_or_else(|| format!("combat {ordinal} is not an exact production-state case"))?;
        let encounter_id = combat
            .case
            .production_context
            .as_ref()
            .and_then(|context| context.encounter_id);
        let case_file = format!("combat-{ordinal:03}.case.json");
        let actions_file = format!("combat-{ordinal:03}.actions.json");
        files.push(CombatRunCaptureFileV1 {
            path: directory.join(&case_file),
            payload: json_payload(&combat.case, "combat-run case")?,
            description: "combat-run case",
        });
        files.push(CombatRunCaptureFileV1 {
            path: directory.join(&actions_file),
            payload: json_payload(&combat.actions, "combat-run winning actions")?,
            description: "combat-run winning actions",
        });
        entries.push(CombatRunCaptureEntryV1 {
            ordinal,
            public_snapshot_id: combat.root_public_snapshot_id.clone(),
            act: combat.case.core.run.act,
            floor: combat.case.core.run.floor,
            encounter_id,
            root_exact_state_hash: identity.root_exact_state_hash,
            run_session_fingerprint,
            action_count: combat.actions.len(),
            outcome: combat.outcome.clone(),
            case_file,
            actions_file,
        });
    }
    let manifest = CombatRunCaptureManifestV1 {
        schema: COMBAT_RUN_CAPTURE_SCHEMA_V1.to_string(),
        source_receipt_file: receipt_file,
        seed: episode.seed,
        episode_outcome: episode.outcome.clone(),
        curriculum_eligible,
        curriculum_ineligibility,
        captured_combat_entries: execution.captured_combat_entries.len(),
        committed_combat_count: entries.len(),
        committed_combat_actions,
        actor_installed_plans: episode.combat_research_plans_installed,
        actor_plan_actions: episode.combat_research_plan_actions,
        combats: entries,
    };
    Ok(CombatRunCapturePayloadV1 {
        directory: directory.to_path_buf(),
        files,
        manifest_path: directory.join("manifest.json"),
        manifest_payload: json_payload(&manifest, "combat-run manifest")?,
    })
}

pub(super) fn write_combat_run_capture_v1(
    capture: &CombatRunCapturePayloadV1,
    receipt_path: &Path,
    receipt_payload: &[u8],
) -> Result<(), String> {
    if let Some(parent) = capture.directory.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create combat-run parent directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::create_dir(&capture.directory).map_err(|error| {
        format!(
            "failed to create fresh combat-run directory {}: {error}",
            capture.directory.display()
        )
    })?;
    for file in &capture.files {
        write_payload_new(&file.path, &file.payload, file.description)?;
    }
    write_payload_new(receipt_path, receipt_payload, "public actor census")?;
    write_payload_new(
        &capture.manifest_path,
        &capture.manifest_payload,
        "combat-run manifest",
    )
}
