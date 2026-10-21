use std::collections::{BTreeSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde::Serialize;
use sts_oracle_learning::eval::run_control::{
    CombatLearningRootBatchArtifactV1, CombatLearningRootContextV1, CombatLearningRootIdentityV1,
};
use sts_oracle_runtime::eval::combat_case::{
    load_combat_case, save_combat_case, CombatCase, CombatCaseGap, CombatCaseRngSummary,
    CombatCaseRunSummary, CombatCaseSource, CombatCaseWitnessBudgetV1,
};
use sts_oracle_runtime::eval::combat_case_context::{
    capture_combat_case_production_context_v2, restore_combat_case_production_session_v2,
    CombatCaseReplayIdentityV2,
};
use sts_oracle_runtime::eval::run_control::{
    build_decision_surface, BoundedRunDriveStopV1, BoundedRunDriver, BoundedRunStepControlV1,
    RunControlAutoStepOptions, RunControlConfig, RunControlSession, RunControlSessionCheckpointV1,
    RunDecisionAction, RunProgressStepV1,
};
use sts_oracle_runtime::runtime::branch::{
    load_oracle_run_continuation_v1, ORACLE_RUN_CONTINUATION_SCHEMA_NAME,
    ORACLE_RUN_CONTINUATION_SCHEMA_VERSION,
};
use sts_oracle_runtime::sim::combat::CombatTerminal;
use sts_oracle_runtime::state::core::ClientInput;

const SUMMARY_SCHEMA_NAME: &str = "CombatLearningRootExportSummary";
const SUMMARY_SCHEMA_VERSION: u32 = 1;
const COLLECTION_SUMMARY_SCHEMA_NAME: &str = "CombatLearningRootCollectionSummary";
const COLLECTION_SUMMARY_SCHEMA_VERSION: u32 = 2;
const RECOVERY_SUMMARY_SCHEMA_NAME: &str = "CombatLearningRecoveryRootSummary";
const RECOVERY_SUMMARY_SCHEMA_VERSION: u32 = 1;
const SEARCH_RECOVERY_SUMMARY_SCHEMA_NAME: &str = "CombatLearningSearchRecoveryRootSummary";
const SEARCH_RECOVERY_SUMMARY_SCHEMA_VERSION: u32 = 1;
const MERGE_SUMMARY_SCHEMA_NAME: &str = "CombatLearningRootMergeSummary";
const MERGE_SUMMARY_SCHEMA_VERSION: u32 = 2;
const CASE_SUMMARY_SCHEMA_NAME: &str = "CombatLearningRootCaseSummary";
const CASE_SUMMARY_SCHEMA_VERSION: u32 = 1;
const SELECT_SUMMARY_SCHEMA_NAME: &str = "CombatLearningRootSelectSummary";
const SELECT_SUMMARY_SCHEMA_VERSION: u32 = 1;
const MAX_COLLECTED_ROOTS: usize = 64;
const MAX_RECOVERY_ACTIONS: usize = 4_096;
const MAX_RECOVERY_ACTION_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Subcommand)]
pub(super) enum LearningRootCommand {
    /// Convert production continuations already at combat boundaries into one
    /// bounded opaque root batch.
    Export {
        #[arg(long, required = true)]
        continuation: Vec<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 16 * 1024 * 1024)]
        max_bytes: usize,
    },
    /// Run current production owners to the first combat boundary and emit one
    /// opaque batch without intermediate continuation JSON.
    Collect {
        #[arg(long, required = true)]
        seed: Vec<u64>,
        #[arg(long, default_value_t = 0)]
        ascension: u8,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 64)]
        max_progress_steps: usize,
        #[arg(long, default_value_t = 10_000)]
        wall_ms: u64,
        #[arg(long, default_value_t = 16 * 1024 * 1024)]
        max_bytes: usize,
    },
    /// Replay one exact production combat win and export a bounded
    /// terminal-nearest recovery-root batch without action labels.
    Recover {
        #[arg(long)]
        case: PathBuf,
        #[arg(long)]
        actions: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 8)]
        max_roots: usize,
        #[arg(long, default_value_t = 16 * 1024 * 1024)]
        max_bytes: usize,
    },
    /// Recover a verified exact-win successor directly from its opaque source
    /// root and Rust-owned search corpus, without case or action intermediates.
    RecoverSearch {
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        expected_roots: usize,
        #[arg(long, default_value_t = 0)]
        root_slot: usize,
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long)]
        candidate_ordinal: usize,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 8)]
        max_roots: usize,
        #[arg(long, default_value_t = 16 * 1024 * 1024)]
        max_bytes: usize,
    },
    /// Export one exact root from an opaque learning batch as a replayable
    /// production combat case for bounded search and witness construction.
    Case {
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        expected_roots: usize,
        #[arg(long, default_value_t = 0)]
        root_slot: usize,
        #[arg(long, default_value_t = 16 * 1024 * 1024)]
        max_bytes: usize,
    },
    /// Select an explicit ordered root subset from one canonical opaque batch.
    Select {
        #[arg(long)]
        artifact: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        expected_roots: usize,
        #[arg(long, required = true)]
        root_slot: Vec<usize>,
        #[arg(long, default_value_t = 16 * 1024 * 1024)]
        max_bytes: usize,
    },
    /// Merge canonical root artifacts into one bounded opaque batch.
    Merge {
        #[arg(long, required = true)]
        input: Vec<PathBuf>,
        /// Expected root count for each input, in input order. When omitted,
        /// every input must be a canonical single-root artifact.
        #[arg(long)]
        input_roots: Vec<usize>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 16 * 1024 * 1024)]
        max_bytes: usize,
    },
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatLearningRootExportSummaryV1 {
    schema_name: &'static str,
    schema_version: u32,
    output: PathBuf,
    payload_bytes: usize,
    roots: Vec<CombatLearningRootExportedRootV1>,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct CombatLearningRootExportedRootV1 {
    identity: CombatLearningRootIdentityV1,
    context: CombatLearningRootContextV1,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatLearningRootCollectionSummaryV2 {
    schema_name: &'static str,
    schema_version: u32,
    ascension: u8,
    total_applied_progress_steps: usize,
    output: PathBuf,
    payload_bytes: usize,
    roots: Vec<CombatLearningRootCollectedRootV2>,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct CombatLearningRootCollectedRootV2 {
    seed: u64,
    applied_progress_steps: usize,
    identity: CombatLearningRootIdentityV1,
    context: CombatLearningRootContextV1,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatLearningRecoveryRootSummaryV1 {
    schema_name: &'static str,
    schema_version: u32,
    case: PathBuf,
    actions: PathBuf,
    output: PathBuf,
    payload_bytes: usize,
    supplied_action_count: usize,
    max_roots: usize,
    final_hp: i32,
    roots: Vec<CombatLearningRecoveredRootV1>,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatLearningSearchRecoveryRootSummaryV1 {
    schema_name: &'static str,
    schema_version: u32,
    source_artifact: PathBuf,
    source_expected_roots: usize,
    source_root_slot: usize,
    source_identity: CombatLearningRootIdentityV1,
    source_corpus: PathBuf,
    source_candidate_ordinal: usize,
    output: PathBuf,
    payload_bytes: usize,
    supplied_action_count: usize,
    max_roots: usize,
    final_hp: i32,
    roots: Vec<CombatLearningRecoveredRootV1>,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct CombatLearningRecoveredRootV1 {
    actions_to_terminal: usize,
    identity: CombatLearningRootIdentityV1,
    context: CombatLearningRootContextV1,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatLearningRootMergeSummaryV2 {
    schema_name: &'static str,
    schema_version: u32,
    inputs: Vec<PathBuf>,
    input_root_counts: Vec<usize>,
    output: PathBuf,
    payload_bytes: usize,
    roots: Vec<CombatLearningRootExportedRootV1>,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatLearningRootCaseSummaryV1 {
    schema_name: &'static str,
    schema_version: u32,
    artifact: PathBuf,
    expected_roots: usize,
    root_slot: usize,
    output: PathBuf,
    identity: CombatLearningRootIdentityV1,
    context: CombatLearningRootContextV1,
    replay_identity: CombatCaseReplayIdentityV2,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatLearningRootSelectSummaryV1 {
    schema_name: &'static str,
    schema_version: u32,
    artifact: PathBuf,
    expected_roots: usize,
    root_slots: Vec<usize>,
    output: PathBuf,
    payload_bytes: usize,
    roots: Vec<CombatLearningRootExportedRootV1>,
}

enum CombatLearningRootCollectionStop {
    CombatBoundary,
    AutomationGap(String),
}

pub(super) fn run(command: LearningRootCommand) -> Result<(), String> {
    match command {
        LearningRootCommand::Export {
            continuation,
            output,
            max_bytes,
        } => super::print_json(&export(&continuation, &output, max_bytes)?),
        LearningRootCommand::Collect {
            seed,
            ascension,
            output,
            max_progress_steps,
            wall_ms,
            max_bytes,
        } => super::print_json(&collect(
            &seed,
            ascension,
            &output,
            max_progress_steps,
            wall_ms,
            max_bytes,
        )?),
        LearningRootCommand::Recover {
            case,
            actions,
            output,
            max_roots,
            max_bytes,
        } => super::print_json(&recover(&case, &actions, &output, max_roots, max_bytes)?),
        LearningRootCommand::RecoverSearch {
            artifact,
            expected_roots,
            root_slot,
            corpus,
            candidate_ordinal,
            output,
            max_roots,
            max_bytes,
        } => super::print_json(&recover_search(
            &artifact,
            expected_roots,
            root_slot,
            &corpus,
            candidate_ordinal,
            &output,
            max_roots,
            max_bytes,
        )?),
        LearningRootCommand::Case {
            artifact,
            output,
            expected_roots,
            root_slot,
            max_bytes,
        } => super::print_json(&export_case(
            &artifact,
            &output,
            expected_roots,
            root_slot,
            max_bytes,
        )?),
        LearningRootCommand::Select {
            artifact,
            output,
            expected_roots,
            root_slot,
            max_bytes,
        } => super::print_json(&select(
            &artifact,
            &output,
            expected_roots,
            &root_slot,
            max_bytes,
        )?),
        LearningRootCommand::Merge {
            input,
            input_roots,
            output,
            max_bytes,
        } => super::print_json(&merge(&input, &input_roots, &output, max_bytes)?),
    }
}

pub(super) fn export_case(
    artifact_path: &Path,
    output: &Path,
    expected_roots: usize,
    root_slot: usize,
    max_bytes: usize,
) -> Result<CombatLearningRootCaseSummaryV1, String> {
    require_fresh_output(output)?;
    if expected_roots == 0 || expected_roots > MAX_COLLECTED_ROOTS {
        return Err(format!(
            "learning root case expected_roots must be in 1..={MAX_COLLECTED_ROOTS}"
        ));
    }
    if root_slot >= expected_roots {
        return Err("learning root case root_slot is outside expected_roots".to_owned());
    }
    let payload = read_bounded_payload(artifact_path, max_bytes)?;
    let artifact = CombatLearningRootBatchArtifactV1::decode(&payload, expected_roots, max_bytes)?;
    let captured = artifact
        .roots()
        .get(root_slot)
        .ok_or_else(|| "learning root case root_slot is absent after validation".to_owned())?;
    let identity = captured.identity().clone();
    let context = *captured.context();
    let checkpoint = artifact
        .into_checkpoints()?
        .into_iter()
        .nth(root_slot)
        .ok_or_else(|| {
            "learning root case checkpoint slot is absent after validation".to_owned()
        })?;
    let session = checkpoint.into_session()?;
    let mut case = combat_case_from_session(&session)?;
    case.production_context = Some(capture_combat_case_production_context_v2(
        &case.core, &session,
    )?);
    let replay_identity = case.replay_identity_v2()?;
    if replay_identity.root_exact_state_hash != identity.exact_combat_state_hash {
        return Err("learning root case changed the exact combat state".to_owned());
    }
    save_combat_case(output, &case)?;
    Ok(CombatLearningRootCaseSummaryV1 {
        schema_name: CASE_SUMMARY_SCHEMA_NAME,
        schema_version: CASE_SUMMARY_SCHEMA_VERSION,
        artifact: artifact_path.to_path_buf(),
        expected_roots,
        root_slot,
        output: output.to_path_buf(),
        identity,
        context,
        replay_identity,
    })
}

pub(super) fn select(
    artifact_path: &Path,
    output: &Path,
    expected_roots: usize,
    root_slots: &[usize],
    max_bytes: usize,
) -> Result<CombatLearningRootSelectSummaryV1, String> {
    require_fresh_output(output)?;
    if expected_roots == 0 || expected_roots > MAX_COLLECTED_ROOTS {
        return Err(format!(
            "learning root select expected_roots must be in 1..={MAX_COLLECTED_ROOTS}"
        ));
    }
    if root_slots.is_empty() || root_slots.len() > MAX_COLLECTED_ROOTS {
        return Err(format!(
            "learning root select root_slot count must be in 1..={MAX_COLLECTED_ROOTS}"
        ));
    }
    let payload = read_bounded_payload(artifact_path, max_bytes)?;
    let artifact = CombatLearningRootBatchArtifactV1::decode(&payload, expected_roots, max_bytes)?;
    let selected = artifact.select_root_slots(root_slots.iter().copied())?;
    let roots = selected
        .roots()
        .iter()
        .map(|root| CombatLearningRootExportedRootV1 {
            identity: root.identity().clone(),
            context: *root.context(),
        })
        .collect();
    let payload = selected.encode(max_bytes)?;
    write_new_payload(output, &payload)?;
    Ok(CombatLearningRootSelectSummaryV1 {
        schema_name: SELECT_SUMMARY_SCHEMA_NAME,
        schema_version: SELECT_SUMMARY_SCHEMA_VERSION,
        artifact: artifact_path.to_path_buf(),
        expected_roots,
        root_slots: root_slots.to_vec(),
        output: output.to_path_buf(),
        payload_bytes: payload.len(),
        roots,
    })
}

fn read_bounded_payload(path: &Path, max_bytes: usize) -> Result<Vec<u8>, String> {
    let file =
        File::open(path).map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let byte_count = usize::try_from(
        file.metadata()
            .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?
            .len(),
    )
    .map_err(|_| format!("learning root input is too large: {}", path.display()))?;
    if byte_count == 0 || byte_count > max_bytes {
        return Err(format!(
            "learning root input violates its byte bound: {}",
            path.display()
        ));
    }
    let mut payload = Vec::with_capacity(byte_count);
    file.take(max_bytes as u64 + 1)
        .read_to_end(&mut payload)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if payload.len() != byte_count {
        return Err(format!(
            "learning root input changed while reading: {}",
            path.display()
        ));
    }
    Ok(payload)
}

fn combat_case_from_session(session: &RunControlSession) -> Result<CombatCase, String> {
    Ok(CombatCase::new(
        CombatCaseSource {
            seed: session.run_state.seed,
            ascension: session.run_state.ascension_level,
            generation: 0,
            branch_id: 0,
            parent_id: None,
        },
        CombatCaseGap {
            boundary: "learning_root_case_export".to_owned(),
            reason: "exact opaque learning root selected for bounded diagnosis".to_owned(),
            witness_budget: CombatCaseWitnessBudgetV1::NotRun,
        },
        CombatCaseRunSummary {
            act: session.run_state.act_num,
            floor: session.run_state.floor_num,
            hp: session.run_state.current_hp,
            max_hp: session.run_state.max_hp,
            gold: session.run_state.gold,
            deck_size: session.run_state.master_deck.len(),
            relic_count: session.run_state.relics.len(),
            potion_slots: session.run_state.potions.len(),
        },
        Vec::new(),
        None,
        Vec::new(),
        CombatCaseRngSummary::from_pool(&session.run_state.rng_pool),
        session.current_active_combat_position()?,
    ))
}

pub(super) fn merge(
    input_paths: &[PathBuf],
    input_root_counts: &[usize],
    output: &Path,
    max_bytes: usize,
) -> Result<CombatLearningRootMergeSummaryV2, String> {
    if input_paths.len() < 2 {
        return Err("learning root merge requires at least two inputs".to_owned());
    }
    let input_root_counts = if input_root_counts.is_empty() {
        vec![1; input_paths.len()]
    } else {
        if input_root_counts.len() != input_paths.len() {
            return Err(
                "learning root merge requires one input_roots count for every input".to_owned(),
            );
        }
        input_root_counts.to_vec()
    };
    let total_root_count = input_root_counts.iter().try_fold(0usize, |total, &count| {
        if count == 0 {
            return Err("learning root merge input_roots counts must be positive".to_owned());
        }
        total
            .checked_add(count)
            .ok_or_else(|| "learning root merge root count overflow".to_owned())
    })?;
    if total_root_count > MAX_COLLECTED_ROOTS {
        return Err(format!(
            "learning root merge accepts at most {MAX_COLLECTED_ROOTS} total roots"
        ));
    }
    require_fresh_output(output)?;
    let mut payloads = Vec::with_capacity(input_paths.len());
    for path in input_paths {
        payloads.push(read_bounded_payload(path, max_bytes)?);
    }
    let artifact = CombatLearningRootBatchArtifactV1::merge_canonical_payloads(
        payloads
            .iter()
            .map(Vec::as_slice)
            .zip(input_root_counts.iter().copied()),
        max_bytes,
    )?;
    let roots = artifact
        .roots()
        .iter()
        .map(|root| CombatLearningRootExportedRootV1 {
            identity: root.identity().clone(),
            context: *root.context(),
        })
        .collect();
    let payload = artifact.encode(max_bytes)?;
    write_new_payload(output, &payload)?;
    Ok(CombatLearningRootMergeSummaryV2 {
        schema_name: MERGE_SUMMARY_SCHEMA_NAME,
        schema_version: MERGE_SUMMARY_SCHEMA_VERSION,
        inputs: input_paths.to_vec(),
        input_root_counts,
        output: output.to_path_buf(),
        payload_bytes: payload.len(),
        roots,
    })
}

pub(super) fn export(
    continuation_paths: &[PathBuf],
    output: &Path,
    max_bytes: usize,
) -> Result<CombatLearningRootExportSummaryV1, String> {
    if continuation_paths.is_empty() {
        return Err("learning root export requires at least one continuation".to_owned());
    }
    require_fresh_output(output)?;

    let mut checkpoints = Vec::with_capacity(continuation_paths.len());
    for path in continuation_paths {
        let continuation = load_oracle_run_continuation_v1(path)?;
        if continuation.schema_name != ORACLE_RUN_CONTINUATION_SCHEMA_NAME
            || continuation.schema_version != ORACLE_RUN_CONTINUATION_SCHEMA_VERSION
        {
            return Err(format!(
                "unsupported oracle continuation schema in {}",
                path.display()
            ));
        }
        checkpoints.push(continuation.session);
    }

    let artifact = CombatLearningRootBatchArtifactV1::from_checkpoints(checkpoints)?;
    let roots = artifact
        .roots()
        .iter()
        .map(|root| CombatLearningRootExportedRootV1 {
            identity: root.identity().clone(),
            context: *root.context(),
        })
        .collect();
    let payload = artifact.encode(max_bytes)?;
    write_new_payload(output, &payload)?;

    Ok(CombatLearningRootExportSummaryV1 {
        schema_name: SUMMARY_SCHEMA_NAME,
        schema_version: SUMMARY_SCHEMA_VERSION,
        output: output.to_path_buf(),
        payload_bytes: payload.len(),
        roots,
    })
}

pub(super) fn collect(
    seeds: &[u64],
    ascension: u8,
    output: &Path,
    max_progress_steps: usize,
    wall_ms: u64,
    max_bytes: usize,
) -> Result<CombatLearningRootCollectionSummaryV2, String> {
    require_fresh_output(output)?;
    if ascension > 20 {
        return Err("learning root collection ascension must be at most 20".to_owned());
    }
    if seeds.is_empty() {
        return Err("learning root collection requires at least one seed".to_owned());
    }
    if seeds.len() > MAX_COLLECTED_ROOTS {
        return Err(format!(
            "learning root collection accepts at most {MAX_COLLECTED_ROOTS} seeds"
        ));
    }
    let distinct = seeds.iter().copied().collect::<BTreeSet<_>>();
    if distinct.len() != seeds.len() {
        return Err("learning root collection requires distinct seeds".to_owned());
    }

    let mut checkpoints = Vec::with_capacity(seeds.len());
    let mut collected = Vec::with_capacity(seeds.len());
    for &seed in seeds {
        let (checkpoint, applied_progress_steps) =
            collect_one(seed, ascension, max_progress_steps, wall_ms)
                .map_err(|error| format!("seed {seed}: {error}"))?;
        checkpoints.push(checkpoint);
        collected.push((seed, applied_progress_steps));
    }
    let artifact = CombatLearningRootBatchArtifactV1::from_checkpoints(checkpoints)?;
    let roots = artifact
        .roots()
        .iter()
        .zip(collected)
        .map(
            |(root, (seed, applied_progress_steps))| CombatLearningRootCollectedRootV2 {
                seed,
                applied_progress_steps,
                identity: root.identity().clone(),
                context: *root.context(),
            },
        )
        .collect::<Vec<_>>();
    let total_applied_progress_steps = roots.iter().map(|root| root.applied_progress_steps).sum();
    let payload = artifact.encode(max_bytes)?;
    write_new_payload(output, &payload)?;

    Ok(CombatLearningRootCollectionSummaryV2 {
        schema_name: COLLECTION_SUMMARY_SCHEMA_NAME,
        schema_version: COLLECTION_SUMMARY_SCHEMA_VERSION,
        ascension,
        total_applied_progress_steps,
        output: output.to_path_buf(),
        payload_bytes: payload.len(),
        roots,
    })
}

pub(super) fn recover(
    case_path: &Path,
    actions_path: &Path,
    output: &Path,
    max_roots: usize,
    max_bytes: usize,
) -> Result<CombatLearningRecoveryRootSummaryV1, String> {
    require_fresh_output(output)?;
    if max_roots == 0 || max_roots > MAX_COLLECTED_ROOTS {
        return Err(format!(
            "learning recovery max_roots must be in 1..={MAX_COLLECTED_ROOTS}"
        ));
    }
    let case = load_combat_case(case_path)?;
    let session =
        restore_combat_case_production_session_v2(&case.core, case.production_context.as_ref())?;
    let mut action_payload = Vec::new();
    File::open(actions_path)
        .map_err(|error| format!("failed to open {}: {error}", actions_path.display()))?
        .take(MAX_RECOVERY_ACTION_BYTES + 1)
        .read_to_end(&mut action_payload)
        .map_err(|error| format!("failed to read {}: {error}", actions_path.display()))?;
    if action_payload.is_empty() || action_payload.len() as u64 > MAX_RECOVERY_ACTION_BYTES {
        return Err(format!(
            "learning recovery action bytes must be in 1..={MAX_RECOVERY_ACTION_BYTES}"
        ));
    }
    let actions = serde_json::from_slice::<Vec<ClientInput>>(&action_payload)
        .map_err(|error| format!("failed to decode {}: {error}", actions_path.display()))?;
    if actions.is_empty() || actions.len() > MAX_RECOVERY_ACTIONS {
        return Err(format!(
            "learning recovery action count must be in 1..={MAX_RECOVERY_ACTIONS}"
        ));
    }

    let (checkpoints, final_hp) = replay_recovery_roots(session, &actions, max_roots)?;
    let artifact = CombatLearningRootBatchArtifactV1::from_checkpoints(checkpoints)?;
    let roots = artifact
        .roots()
        .iter()
        .enumerate()
        .map(|(index, root)| CombatLearningRecoveredRootV1 {
            actions_to_terminal: index + 1,
            identity: root.identity().clone(),
            context: *root.context(),
        })
        .collect();
    let payload = artifact.encode(max_bytes)?;
    write_new_payload(output, &payload)?;
    Ok(CombatLearningRecoveryRootSummaryV1 {
        schema_name: RECOVERY_SUMMARY_SCHEMA_NAME,
        schema_version: RECOVERY_SUMMARY_SCHEMA_VERSION,
        case: case_path.to_path_buf(),
        actions: actions_path.to_path_buf(),
        output: output.to_path_buf(),
        payload_bytes: payload.len(),
        supplied_action_count: actions.len(),
        max_roots,
        final_hp,
        roots,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn recover_search(
    artifact_path: &Path,
    expected_roots: usize,
    root_slot: usize,
    corpus_path: &Path,
    candidate_ordinal: usize,
    output: &Path,
    max_roots: usize,
    max_bytes: usize,
) -> Result<CombatLearningSearchRecoveryRootSummaryV1, String> {
    require_fresh_output(output)?;
    if expected_roots == 0 || expected_roots > MAX_COLLECTED_ROOTS {
        return Err(format!(
            "learning search recovery expected_roots must be in 1..={MAX_COLLECTED_ROOTS}"
        ));
    }
    if root_slot >= expected_roots {
        return Err("learning search recovery root_slot is outside expected_roots".to_owned());
    }
    if max_roots == 0 || max_roots > MAX_COLLECTED_ROOTS {
        return Err(format!(
            "learning search recovery max_roots must be in 1..={MAX_COLLECTED_ROOTS}"
        ));
    }
    let payload = read_bounded_payload(artifact_path, max_bytes)?;
    let artifact = CombatLearningRootBatchArtifactV1::decode(&payload, expected_roots, max_bytes)?;
    let source_identity = artifact
        .roots()
        .get(root_slot)
        .ok_or_else(|| "learning search recovery root slot is absent".to_owned())?
        .identity()
        .clone();
    let witness = super::action_successor_reanalysis::load_exact_win_candidate_witness_v2(
        corpus_path,
        candidate_ordinal,
        max_bytes,
    )?;
    if witness.source_expected_roots != expected_roots
        || witness.source_root_slot != root_slot
        || witness.source_root_id != source_identity.root_id
        || witness.source_exact_state_hash != source_identity.exact_combat_state_hash
    {
        return Err(
            "learning search recovery corpus does not match its exact source root".to_owned(),
        );
    }
    let checkpoint = artifact
        .into_checkpoints()?
        .into_iter()
        .nth(root_slot)
        .ok_or_else(|| "learning search recovery checkpoint slot is absent".to_owned())?;
    let supplied_action_count = witness.actions.len();
    let (checkpoints, final_hp) =
        replay_recovery_roots(checkpoint.into_session()?, &witness.actions, max_roots)?;
    if final_hp != witness.final_hp {
        return Err(format!(
            "learning search recovery final HP {final_hp} does not match corpus {}",
            witness.final_hp
        ));
    }
    let recovered = CombatLearningRootBatchArtifactV1::from_checkpoints(checkpoints)?;
    let roots = recovered
        .roots()
        .iter()
        .enumerate()
        .map(|(index, root)| CombatLearningRecoveredRootV1 {
            actions_to_terminal: index + 1,
            identity: root.identity().clone(),
            context: *root.context(),
        })
        .collect();
    let recovered_payload = recovered.encode(max_bytes)?;
    write_new_payload(output, &recovered_payload)?;
    Ok(CombatLearningSearchRecoveryRootSummaryV1 {
        schema_name: SEARCH_RECOVERY_SUMMARY_SCHEMA_NAME,
        schema_version: SEARCH_RECOVERY_SUMMARY_SCHEMA_VERSION,
        source_artifact: artifact_path.to_path_buf(),
        source_expected_roots: expected_roots,
        source_root_slot: root_slot,
        source_identity,
        source_corpus: corpus_path.to_path_buf(),
        source_candidate_ordinal: candidate_ordinal,
        output: output.to_path_buf(),
        payload_bytes: recovered_payload.len(),
        supplied_action_count,
        max_roots,
        final_hp,
        roots,
    })
}

fn replay_recovery_roots(
    mut session: RunControlSession,
    actions: &[ClientInput],
    root_count: usize,
) -> Result<(Vec<RunControlSessionCheckpointV1>, i32), String> {
    let previous_outcome_id = session
        .last_combat_baseline()
        .map(|outcome| outcome.case_id.clone());
    let mut retained = VecDeque::with_capacity(root_count);
    for (index, action) in actions.iter().enumerate() {
        if session.active_combat.is_none() {
            return Err(format!(
                "learning recovery combat terminated before action {index}"
            ));
        }
        if retained.len() == root_count {
            retained.pop_front();
        }
        retained.push_back(RunControlSessionCheckpointV1::from_session(&session));
        session
            .apply_decision_action(RunDecisionAction::Input(action.clone()))
            .map_err(|error| format!("learning recovery action {index} failed: {error}"))?;
    }
    if session.active_combat.is_some() {
        return Err("learning recovery actions did not terminate combat".to_owned());
    }
    let outcome = session
        .last_combat_baseline()
        .filter(|outcome| Some(&outcome.case_id) != previous_outcome_id.as_ref())
        .ok_or_else(|| "learning recovery combat produced no new typed outcome".to_owned())?;
    if outcome.terminal != CombatTerminal::Win {
        return Err(format!(
            "learning recovery actions terminated with {:?}, expected win",
            outcome.terminal
        ));
    }
    Ok((retained.into_iter().rev().collect(), outcome.final_hp))
}

fn collect_one(
    seed: u64,
    ascension: u8,
    max_progress_steps: usize,
    wall_ms: u64,
) -> Result<(RunControlSessionCheckpointV1, usize), String> {
    let mut session = RunControlSession::new(RunControlConfig {
        seed,
        ascension_level: ascension,
        ..RunControlConfig::default()
    });
    let drive = BoundedRunDriver::new(max_progress_steps, Some(wall_ms))?
        .drive_with(
            &mut session,
            |session, _context| match apply_neutral_noncombat_step(session)? {
                NeutralNoncombatStep::Applied(step) => Ok(BoundedRunStepControlV1::Continue {
                    progress_steps: vec![step],
                }),
                NeutralNoncombatStep::CombatBoundary => Ok(BoundedRunStepControlV1::Stop {
                    progress_steps: Vec::new(),
                    output: CombatLearningRootCollectionStop::CombatBoundary,
                }),
                NeutralNoncombatStep::AutomationGap(reason) => Ok(BoundedRunStepControlV1::Stop {
                    progress_steps: Vec::new(),
                    output: CombatLearningRootCollectionStop::AutomationGap(reason),
                }),
            },
        )
        .map_err(|error| error.message)?;
    let applied_progress_steps = drive.applied_progress_steps();
    match drive.stop {
        BoundedRunDriveStopV1::Step(CombatLearningRootCollectionStop::CombatBoundary) => {}
        BoundedRunDriveStopV1::Step(CombatLearningRootCollectionStop::AutomationGap(reason)) => {
            return Err(format!(
                "learning root collection did not reach a combat boundary: {reason}"
            ));
        }
        BoundedRunDriveStopV1::ProgressBudgetExhausted => {
            return Err(format!(
                "learning root collection did not reach a combat boundary: progress budget exhausted after {applied_progress_steps} steps"
            ));
        }
        BoundedRunDriveStopV1::WallDeadlineReached => {
            return Err(format!(
                "learning root collection did not reach a combat boundary: wall deadline reached after {applied_progress_steps} steps"
            ));
        }
        BoundedRunDriveStopV1::RunCompleted { victory } => {
            return Err(format!(
                "learning root collection did not reach a combat boundary: run completed with victory={victory}"
            ));
        }
    }
    Ok((
        RunControlSessionCheckpointV1::from_session(&session),
        applied_progress_steps,
    ))
}

enum NeutralNoncombatStep {
    Applied(RunProgressStepV1),
    CombatBoundary,
    AutomationGap(String),
}

fn apply_neutral_noncombat_step(
    session: &mut RunControlSession,
) -> Result<NeutralNoncombatStep, String> {
    if session.active_combat.is_some() {
        return Ok(NeutralNoncombatStep::CombatBoundary);
    }
    let automatic = session.apply_progress_step(RunControlAutoStepOptions::default())?;
    match automatic.progress_steps.as_slice() {
        [step @ (RunProgressStepV1::Decision(_)
        | RunProgressStepV1::ForcedTransition(_)
        | RunProgressStepV1::CombatResolution(_))] => {
            Ok(NeutralNoncombatStep::Applied(step.clone()))
        }
        [RunProgressStepV1::Stop(stop)] => {
            if session.active_combat.is_some() {
                return Ok(NeutralNoncombatStep::CombatBoundary);
            }
            let surface = build_decision_surface(session);
            let Some(candidate_id) = surface
                .view
                .candidates
                .iter()
                .find(|candidate| candidate.action.executable_action_ref().is_some())
                .map(|candidate| candidate.id.clone())
            else {
                return Ok(NeutralNoncombatStep::AutomationGap(stop.reason.clone()));
            };
            let applied = session.apply_candidate_id(&candidate_id)?;
            match applied.progress_steps.as_slice() {
                [step
                @ (RunProgressStepV1::Decision(_) | RunProgressStepV1::ForcedTransition(_))] => {
                    Ok(NeutralNoncombatStep::Applied(step.clone()))
                }
                _ => Err(
                    "neutral learning-root step did not produce exactly one non-combat mutation"
                        .to_string(),
                ),
            }
        }
        _ => {
            Err("neutral learning-root step produced neither one mutation nor one stop".to_string())
        }
    }
}

fn require_fresh_output(output: &Path) -> Result<(), String> {
    if output.exists() {
        return Err(format!(
            "learning root output already exists: {}",
            output.display()
        ));
    }
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| format!("learning root output has no parent: {}", output.display()))?;
    if !parent.is_dir() {
        return Err(format!(
            "learning root output parent does not exist: {}",
            parent.display()
        ));
    }
    Ok(())
}

fn write_new_payload(output: &Path, payload: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| format!("failed to create {}: {error}", output.display()))?;
    file.write_all(payload)
        .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
    file.sync_all()
        .map_err(|error| format!("failed to flush {}: {error}", output.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use sts_oracle_runtime::content::cards::CardId;
    use sts_oracle_runtime::content::monsters::exordium::jaw_worm::JawWorm;
    use sts_oracle_runtime::content::monsters::{EnemyId, MonsterBehavior};
    use sts_oracle_runtime::eval::combat_case::{
        save_combat_case, CombatCase, CombatCaseGap, CombatCaseRngSummary, CombatCaseRunSummary,
        CombatCaseSource,
    };
    use sts_oracle_runtime::eval::combat_case_context::capture_combat_case_production_context_v2;
    use sts_oracle_runtime::eval::run_control::{
        RunControlSession, RunControlSessionCheckpointV1, RunPolicyPriorIdentityV1,
        RunProgressJournalV1,
    };
    use sts_oracle_runtime::runtime::branch::{
        save_oracle_run_continuation_v1, OracleRunContinuationV1,
    };
    use sts_oracle_runtime::runtime::combat::CombatCard;
    use sts_oracle_runtime::state::core::{
        ActiveCombat, CombatContext, DiscoveryChoiceState, EngineState, PendingChoice,
        RoomCombatContext,
    };
    use sts_oracle_runtime::state::map::node::RoomType;

    use super::*;

    #[test]
    fn public_continuation_exports_one_bridge_decodable_root() {
        let root = unique_temp_dir("export");
        fs::create_dir(&root).expect("create test root");
        let continuation_path = root.join("source.continuation.json");
        let output_path = root.join("roots.bin");

        let mut session = RunControlSession::new(Default::default());
        let choice = PendingChoice::DiscoverySelect(DiscoveryChoiceState {
            cards: vec![CardId::Bash, CardId::FiendFire],
            colorless: false,
            card_type: None,
            amount: 1,
            can_skip: true,
        });
        session.engine_state = EngineState::PendingChoice(choice.clone());
        session.active_combat = Some(ActiveCombat::new(
            EngineState::PendingChoice(choice),
            combat_root_session(40)
                .active_combat
                .expect("combat fixture")
                .combat_state,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        save_oracle_run_continuation_v1(
            &continuation_path,
            &OracleRunContinuationV1 {
                schema_name: ORACLE_RUN_CONTINUATION_SCHEMA_NAME.to_owned(),
                schema_version: ORACLE_RUN_CONTINUATION_SCHEMA_VERSION,
                decision_prior: Some(RunPolicyPriorIdentityV1::UniformExactSurfaceV1),
                seed: 0,
                ascension: 0,
                journal: RunProgressJournalV1::default(),
                session: RunControlSessionCheckpointV1::from_session(&session),
                explorer_frontier: None,
            },
        )
        .expect("write current continuation");

        let summary = export(
            std::slice::from_ref(&continuation_path),
            &output_path,
            1024 * 1024,
        )
        .expect("export combat root");
        let payload = fs::read(&output_path).expect("read opaque root artifact");
        let artifact = CombatLearningRootBatchArtifactV1::decode(&payload, 1, 1024 * 1024)
            .expect("decode exported root");

        assert_eq!(summary.roots.len(), 1);
        assert_eq!(summary.payload_bytes, payload.len());
        assert_eq!(artifact.roots().len(), 1);
        assert!(export(
            std::slice::from_ref(&continuation_path),
            &output_path,
            1024 * 1024,
        )
        .is_err());

        fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn canonical_single_root_artifacts_merge_atomically() {
        let root = unique_temp_dir("merge");
        fs::create_dir(&root).expect("create test root");
        let first_path = root.join("first.bin");
        let second_path = root.join("second.bin");
        let output_path = root.join("merged.bin");
        let duplicate_output = root.join("duplicate.bin");
        for (path, monster_hp) in [(&first_path, 20), (&second_path, 21)] {
            let payload = CombatLearningRootBatchArtifactV1::from_checkpoints([
                RunControlSessionCheckpointV1::from_session(&combat_root_session(monster_hp)),
            ])
            .expect("capture one root")
            .encode(1024 * 1024)
            .expect("encode one root");
            fs::write(path, payload).expect("write one root");
        }

        let summary = merge(
            &[first_path.clone(), second_path.clone()],
            &[],
            &output_path,
            1024 * 1024,
        )
        .expect("merge distinct single roots");
        let payload = fs::read(&output_path).expect("read merged roots");
        let artifact = CombatLearningRootBatchArtifactV1::decode(&payload, 2, 1024 * 1024)
            .expect("decode merged roots");

        assert_eq!(summary.schema_name, MERGE_SUMMARY_SCHEMA_NAME);
        assert_eq!(summary.roots.len(), 2);
        assert_eq!(summary.payload_bytes, payload.len());
        assert_eq!(artifact.roots().len(), 2);
        assert!(merge(
            &[first_path.clone(), first_path],
            &[],
            &duplicate_output,
            1024 * 1024,
        )
        .is_err());
        assert!(!duplicate_output.exists());
        fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn canonical_root_batches_merge_only_at_explicit_widths() {
        let root = unique_temp_dir("merge-batches");
        fs::create_dir(&root).expect("create test root");
        let first_path = root.join("first.bin");
        let second_path = root.join("second.bin");
        let output_path = root.join("merged.bin");
        let rejected_path = root.join("rejected.bin");
        for (path, monster_hps) in [(&first_path, vec![20, 21]), (&second_path, vec![22])] {
            let payload = CombatLearningRootBatchArtifactV1::from_checkpoints(
                monster_hps.into_iter().map(|monster_hp| {
                    RunControlSessionCheckpointV1::from_session(&combat_root_session(monster_hp))
                }),
            )
            .expect("capture root batch")
            .encode(1024 * 1024)
            .expect("encode root batch");
            fs::write(path, payload).expect("write root batch");
        }

        let summary = merge(
            &[first_path.clone(), second_path.clone()],
            &[2, 1],
            &output_path,
            1024 * 1024,
        )
        .expect("merge declared root batches");
        let payload = fs::read(&output_path).expect("read merged roots");
        let artifact = CombatLearningRootBatchArtifactV1::decode(&payload, 3, 1024 * 1024)
            .expect("decode merged roots");

        assert_eq!(summary.schema_version, MERGE_SUMMARY_SCHEMA_VERSION);
        assert_eq!(summary.input_root_counts, vec![2, 1]);
        assert_eq!(artifact.roots().len(), 3);
        assert!(merge(
            &[first_path, second_path],
            &[1, 1],
            &rejected_path,
            1024 * 1024,
        )
        .is_err());
        assert!(!rejected_path.exists());
        fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn current_owner_run_collects_first_combat_as_an_opaque_root() {
        let root = unique_temp_dir("collect");
        fs::create_dir(&root).expect("create test root");
        let output_path = root.join("root.bin");

        let summary = collect(&[11, 12], 0, &output_path, 32, 10_000, 16 * 1024 * 1024)
            .expect("collect first production combat root");
        let payload = fs::read(&output_path).expect("read collected root artifact");
        let artifact = CombatLearningRootBatchArtifactV1::decode(&payload, 2, 16 * 1024 * 1024)
            .expect("decode collected root");

        assert_eq!(summary.schema_name, COLLECTION_SUMMARY_SCHEMA_NAME);
        assert_eq!(summary.ascension, 0);
        assert!(summary.total_applied_progress_steps > 0);
        assert_eq!(summary.payload_bytes, payload.len());
        assert_eq!(artifact.roots().len(), 2);
        assert_eq!(
            summary
                .roots
                .iter()
                .map(|root| root.seed)
                .collect::<Vec<_>>(),
            vec![11, 12]
        );
        for (artifact_root, summary_root) in artifact.roots().iter().zip(&summary.roots) {
            assert!(summary_root.applied_progress_steps > 0);
            assert_eq!(artifact_root.identity(), &summary_root.identity);
            assert_eq!(artifact_root.context(), &summary_root.context);
        }

        fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn opaque_batch_exports_one_exact_production_combat_case() {
        let root = unique_temp_dir("case");
        fs::create_dir(&root).expect("create test root");
        let artifact_path = root.join("roots.bin");
        let case_path = root.join("selected.case.json");
        let rejected_path = root.join("rejected.case.json");
        let first = combat_root_session(20);
        let second = combat_root_session(21);
        let expected_state = second
            .current_active_combat_position()
            .expect("second exact position");
        let expected_hash = sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
            &expected_state.engine,
            &expected_state.combat,
        );
        let payload = CombatLearningRootBatchArtifactV1::from_checkpoints([
            RunControlSessionCheckpointV1::from_session(&first),
            RunControlSessionCheckpointV1::from_session(&second),
        ])
        .expect("capture batch")
        .encode(1024 * 1024)
        .expect("encode batch");
        fs::write(&artifact_path, payload).expect("write batch");

        let summary = export_case(&artifact_path, &case_path, 2, 1, 1024 * 1024)
            .expect("export selected root case");
        let case = load_combat_case(&case_path).expect("load selected case");
        let restored =
            restore_combat_case_production_session_v2(&case.core, case.production_context.as_ref())
                .expect("restore exact production session");
        let restored_position = restored
            .current_active_combat_position()
            .expect("restored exact position");

        assert_eq!(summary.schema_name, CASE_SUMMARY_SCHEMA_NAME);
        assert_eq!(summary.root_slot, 1);
        assert_eq!(summary.identity.exact_combat_state_hash, expected_hash);
        assert_eq!(
            case.replay_identity_v2().expect("replay identity").capability,
            sts_oracle_runtime::eval::combat_case_context::CombatCaseReplayCapabilityV2::ExactProductionState
        );
        assert_eq!(restored_position, expected_state);
        assert!(export_case(&artifact_path, &rejected_path, 1, 0, 1024 * 1024).is_err());
        assert!(!rejected_path.exists());
        fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn opaque_batch_selects_ordered_exact_roots_atomically() {
        let root = unique_temp_dir("select");
        fs::create_dir(&root).expect("create test root");
        let artifact_path = root.join("roots.bin");
        let selected_path = root.join("selected.bin");
        let duplicate_path = root.join("duplicate.bin");
        let sessions = [
            combat_root_session(20),
            combat_root_session(21),
            combat_root_session(22),
        ];
        let payload = CombatLearningRootBatchArtifactV1::from_checkpoints(
            sessions
                .iter()
                .map(RunControlSessionCheckpointV1::from_session),
        )
        .expect("capture batch")
        .encode(1024 * 1024)
        .expect("encode batch");
        fs::write(&artifact_path, payload).expect("write batch");

        let summary =
            select(&artifact_path, &selected_path, 3, &[2, 0], 1024 * 1024).expect("select roots");
        let payload = fs::read(&selected_path).expect("read selected roots");
        let selected = CombatLearningRootBatchArtifactV1::decode(&payload, 2, 1024 * 1024)
            .expect("decode selected roots");
        let source_payload = fs::read(&artifact_path).expect("read source roots");
        let source = CombatLearningRootBatchArtifactV1::decode(&source_payload, 3, 1024 * 1024)
            .expect("decode source roots");

        assert_eq!(summary.schema_name, SELECT_SUMMARY_SCHEMA_NAME);
        assert_eq!(summary.root_slots, vec![2, 0]);
        assert_eq!(selected.roots()[0].identity(), source.roots()[2].identity());
        assert_eq!(selected.roots()[1].identity(), source.roots()[0].identity());
        assert!(select(&artifact_path, &duplicate_path, 3, &[1, 1], 1024 * 1024,).is_err());
        assert!(!duplicate_path.exists());
        fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn verified_win_replay_caps_and_orders_terminal_nearest_roots() {
        let actions = vec![
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(7),
            },
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(7),
            },
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(7),
            },
        ];

        let (checkpoints, final_hp) =
            replay_recovery_roots(combat_root_session(18), &actions, 2).expect("replay exact win");
        let remaining_hp = checkpoints
            .iter()
            .cloned()
            .map(|checkpoint| {
                checkpoint
                    .into_session()
                    .expect("restore retained root")
                    .active_combat
                    .expect("retained root remains in combat")
                    .combat_state
                    .entities
                    .monsters[0]
                    .current_hp
            })
            .collect::<Vec<_>>();
        let artifact = CombatLearningRootBatchArtifactV1::from_checkpoints(checkpoints)
            .expect("build terminal-nearest root batch");

        assert_eq!(final_hp, 80);
        assert_eq!(artifact.roots().len(), 2);
        assert_eq!(remaining_hp, vec![6, 12]);
        assert_ne!(
            artifact.roots()[0].identity(),
            artifact.roots()[1].identity()
        );
        assert!(
            replay_recovery_roots(combat_root_session(18), &actions[..2], 2)
                .expect_err("incomplete line must fail")
                .contains("did not terminate")
        );
    }

    #[test]
    fn production_case_and_win_actions_export_bridge_decodable_recovery_roots() {
        let root = unique_temp_dir("recover");
        fs::create_dir(&root).expect("create test root");
        let case_path = root.join("source.case.json");
        let actions_path = root.join("win.actions.json");
        let output_path = root.join("recovery.bin");
        let rejected_output = root.join("rejected.bin");
        let session = combat_root_session(12);
        let mut case = combat_case(&session);
        case.production_context = Some(
            capture_combat_case_production_context_v2(&case.core, &session)
                .expect("capture production context"),
        );
        save_combat_case(&case_path, &case).expect("save production case");
        let actions = vec![
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(7),
            },
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(7),
            },
        ];
        fs::write(
            &actions_path,
            serde_json::to_vec(&actions).expect("encode actions"),
        )
        .expect("save actions");

        let summary = recover(&case_path, &actions_path, &output_path, 8, 1024 * 1024)
            .expect("export recovery roots");
        let payload = fs::read(&output_path).expect("read recovery roots");
        let artifact = CombatLearningRootBatchArtifactV1::decode(&payload, 2, 1024 * 1024)
            .expect("decode recovery roots");

        assert_eq!(summary.supplied_action_count, 2);
        assert_eq!(summary.max_roots, 8);
        assert_eq!(summary.final_hp, 80);
        assert_eq!(summary.roots.len(), 2);
        assert_eq!(summary.roots[0].actions_to_terminal, 1);
        assert_eq!(summary.roots[1].actions_to_terminal, 2);
        assert_eq!(artifact.roots().len(), 2);

        fs::write(
            &actions_path,
            serde_json::to_vec(&actions[..1]).expect("encode incomplete actions"),
        )
        .expect("replace actions");
        assert!(recover(&case_path, &actions_path, &rejected_output, 8, 1024 * 1024,).is_err());
        assert!(!rejected_output.exists());
        fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn collection_budget_exhaustion_writes_no_artifact() {
        let root = unique_temp_dir("bounded");
        fs::create_dir(&root).expect("create test root");
        let output_path = root.join("root.bin");

        let error = collect(&[11], 0, &output_path, 1, 10_000, 16 * 1024 * 1024)
            .expect_err("one progress step must not reach combat");

        assert!(error.contains("did not reach a combat boundary"));
        assert!(!output_path.exists());
        fs::remove_dir_all(&root).expect("remove test root");
    }

    #[test]
    fn collection_rejects_duplicate_or_excessive_seed_batches_before_writing() {
        let root = unique_temp_dir("seed-bounds");
        fs::create_dir(&root).expect("create test root");
        let duplicate_output = root.join("duplicate.bin");
        let excessive_output = root.join("excessive.bin");

        assert!(collect(
            &[11, 11],
            0,
            &duplicate_output,
            32,
            10_000,
            16 * 1024 * 1024,
        )
        .is_err());
        assert!(collect(
            &(0..=MAX_COLLECTED_ROOTS as u64).collect::<Vec<_>>(),
            0,
            &excessive_output,
            32,
            10_000,
            16 * 1024 * 1024,
        )
        .is_err());
        assert!(!duplicate_output.exists());
        assert!(!excessive_output.exists());
        fs::remove_dir_all(&root).expect("remove test root");
    }

    fn combat_root_session(monster_hp: i32) -> RunControlSession {
        let mut session = RunControlSession::new(Default::default());
        let mut combat = sts_oracle_runtime::test_support::blank_test_combat();
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 51),
            CombatCard::for_scenario(CardId::Strike, 52),
            CombatCard::for_scenario(CardId::Strike, 53),
        ];
        let mut monster = sts_oracle_runtime::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        monster.current_hp = monster_hp;
        monster.max_hp = monster_hp;
        monster.set_planned_move_id(1);
        let plan = JawWorm::turn_plan(&combat, &monster);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters.push(monster);
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        session
    }

    fn combat_case(session: &RunControlSession) -> CombatCase {
        CombatCase::new(
            CombatCaseSource {
                seed: session.run_state.seed,
                ascension: session.run_state.ascension_level,
                generation: 0,
                branch_id: 0,
                parent_id: None,
            },
            CombatCaseGap {
                boundary: "learning recovery fixture".to_owned(),
                reason: "contract".to_owned(),
                witness_budget: CombatCaseWitnessBudgetV1::NotRun,
            },
            CombatCaseRunSummary {
                act: session.run_state.act_num,
                floor: session.run_state.floor_num,
                hp: session.run_state.current_hp,
                max_hp: session.run_state.max_hp,
                gold: session.run_state.gold,
                deck_size: session.run_state.master_deck.len(),
                relic_count: session.run_state.relics.len(),
                potion_slots: session.run_state.potions.len(),
            },
            Vec::new(),
            None,
            Vec::new(),
            CombatCaseRngSummary::from_pool(&session.run_state.rng_pool),
            session
                .current_active_combat_position()
                .expect("active combat position"),
        )
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "sts_learning_root_{label}_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        ))
    }
}
