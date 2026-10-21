use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use sts_oracle_learning::agent::task::{
    TaskActorIdentityV1, TaskBudgetStateV1, TaskBudgetV1, TaskEpisodeOutcomeV1, TaskPublicChoiceV1,
    TaskRunSolutionArchiveV1,
};
use sts_oracle_learning::content::monsters::factory::EncounterId;
use sts_oracle_learning::eval::run_control::{
    resume_privileged_reference_run_continuation_v2, task_run_branch_decision_inventory_v1,
    CombatResearchPlanOptionsV1, CompleteTurnProposalBeamResearchOptionsV1,
    ExactBranchingTaskEvidenceRequestV1, TaskRunBranchCheckpointArtifactV1,
    TaskRunForcedPublicDeviationV2,
};

const RECEIPT_SCHEMA_V3: &str = "sts-run-solution-branch-receipt-v3";
const DEFAULT_ARTIFACT_MAX_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_PLAN_OPTIONS_MAX_BYTES: usize = 1024 * 1024;

const USAGE: &str = "Usage:
  run_solution_branch inventory
      --archive <input.json>
      --checkpoints <input.bin>
      --output <fresh-inventory.json>
      [--artifact-max-bytes <usize>]

  run_solution_branch compact-checkpoints
      --archive <input.json>
      --checkpoints <input.bin>
      --output <fresh-checkpoints.bin>
      [--artifact-max-bytes <usize>]

  run_solution_branch [continue]
      --archive <input.json>
      --checkpoints <input.bin>
      --node <archive-node-id>
      --choice-ordinal <usize> [--choice-ordinal <usize> ...]
      --max-world-transitions <u64> [--max-reloads <u32>]
      --output <fresh-receipt.json>
      --output-archive <fresh-archive.json>
      --output-checkpoints <fresh-checkpoints.bin>
      [--capture-combat-case <EncounterId> <fresh.case.json>]
      [--plan-options <combat-plan-options.json>]
      [--artifact-max-bytes <usize>]

Restores the nearest durable checkpoint, exact-replays the archived public
prefix to --node, forces one previously unobserved public choice, and then
continues with the privileged reference actor. It never picks or enumerates
branches. Inventory exposes recoverable public decisions, replay distance,
candidate semantics, and observed public choices. Compact-checkpoints performs
a validated one-way V1-to-V2 sidecar rewrite without running the game. Every
output path must be fresh and distinct.";

enum ParsedArguments {
    Inventory(InventoryArguments),
    CompactCheckpoints(InventoryArguments),
    Continue(ContinueArguments),
}

struct InventoryArguments {
    archive: PathBuf,
    checkpoints: PathBuf,
    output: PathBuf,
    artifact_max_bytes: usize,
}

struct ContinueArguments {
    archive: PathBuf,
    checkpoints: PathBuf,
    archive_node_id: String,
    choice_ordinals: Vec<usize>,
    budget: TaskBudgetV1,
    output: PathBuf,
    output_archive: PathBuf,
    output_checkpoints: PathBuf,
    capture_combat_case: Option<(EncounterId, PathBuf)>,
    plan_options: Option<PathBuf>,
    artifact_max_bytes: usize,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RunSolutionBranchReceiptV3 {
    schema: String,
    seed: u64,
    forced_deviation: TaskRunForcedPublicDeviationV2,
    actor: TaskActorIdentityV1,
    outcome: TaskEpisodeOutcomeV1,
    unsupported_reason: Option<String>,
    requested_budget: TaskBudgetV1,
    remaining_budget: TaskBudgetStateV1,
    execution_manifest_id: String,
    attempt_id: String,
    source_archive_nodes: usize,
    source_archive_attempts: usize,
    source_checkpoints: usize,
    merged_archive_nodes: usize,
    merged_archive_attempts: usize,
    merged_checkpoints: usize,
    committed_public_steps: usize,
    committed_scopes: usize,
    observed_frontier_count: usize,
    strategic_support_count: usize,
    combat_research_plans_installed: usize,
    combat_research_plans_unavailable: usize,
    combat_research_plan_actions: usize,
    combat_research_plan_attempts: usize,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let Some(arguments) = parse_arguments()? else {
        println!("{USAGE}");
        return Ok(());
    };
    match arguments {
        ParsedArguments::Inventory(arguments) => run_inventory(arguments),
        ParsedArguments::CompactCheckpoints(arguments) => run_compact_checkpoints(arguments),
        ParsedArguments::Continue(arguments) => run_continuation(arguments),
    }
}

fn run_compact_checkpoints(arguments: InventoryArguments) -> Result<(), String> {
    if arguments.output.exists() {
        return Err(format!(
            "output already exists: {}",
            arguments.output.display()
        ));
    }
    if arguments.output == arguments.archive || arguments.output == arguments.checkpoints {
        return Err("checkpoint output must not overwrite an input artifact".to_string());
    }
    let source_bytes = fs::metadata(&arguments.checkpoints)
        .map_err(|error| {
            format!(
                "failed to inspect run branch checkpoints {}: {error}",
                arguments.checkpoints.display()
            )
        })?
        .len();
    let (_archive, checkpoints) = load_artifacts(
        &arguments.archive,
        &arguments.checkpoints,
        arguments.artifact_max_bytes,
    )?;
    let output = checkpoints.encode(arguments.artifact_max_bytes)?;
    create_parent_directory(&arguments.output)?;
    write_payload_new(
        &arguments.output,
        &output,
        "compacted run branch checkpoints",
    )?;
    println!(
        "run-branch-checkpoints checkpoints={} bytes={}->{} output={}",
        checkpoints.checkpoint_count(),
        source_bytes,
        output.len(),
        arguments.output.display()
    );
    Ok(())
}

fn run_inventory(arguments: InventoryArguments) -> Result<(), String> {
    if arguments.output.exists() {
        return Err(format!(
            "output already exists: {}",
            arguments.output.display()
        ));
    }
    if arguments.output == arguments.archive || arguments.output == arguments.checkpoints {
        return Err("inventory output must not overwrite an input artifact".to_string());
    }
    let (archive, checkpoints) = load_artifacts(
        &arguments.archive,
        &arguments.checkpoints,
        arguments.artifact_max_bytes,
    )?;
    let inventory = task_run_branch_decision_inventory_v1(&archive, &checkpoints)?;
    let output = json_payload(&inventory, "run branch decision inventory")?;
    if output.len() > arguments.artifact_max_bytes {
        return Err("run branch decision inventory exceeds --artifact-max-bytes".to_string());
    }
    create_parent_directory(&arguments.output)?;
    write_payload_new(&arguments.output, &output, "run branch decision inventory")?;
    println!(
        "run-branch-inventory decisions={} output={}",
        inventory.decisions.len(),
        arguments.output.display()
    );
    Ok(())
}

fn run_continuation(arguments: ContinueArguments) -> Result<(), String> {
    validate_paths(&arguments)?;

    let (archive, checkpoints) = load_artifacts(
        &arguments.archive,
        &arguments.checkpoints,
        arguments.artifact_max_bytes,
    )?;
    let plan_options = match &arguments.plan_options {
        Some(path) => {
            let payload =
                read_bounded(path, DEFAULT_PLAN_OPTIONS_MAX_BYTES, "combat plan options")?;
            serde_json::from_slice::<CombatResearchPlanOptionsV1>(&payload).map_err(|error| {
                format!(
                    "failed to decode combat plan options {}: {error}",
                    path.display()
                )
            })?
        }
        None => CombatResearchPlanOptionsV1 {
            complete_turn_proposal_beam: Some(CompleteTurnProposalBeamResearchOptionsV1::default()),
            ..CombatResearchPlanOptionsV1::default()
        },
    };

    let source_archive_nodes = archive.nodes.len();
    let source_archive_attempts = archive.attempts.len();
    let source_checkpoints = checkpoints.checkpoint_count();
    let evidence_request = ExactBranchingTaskEvidenceRequestV1 {
        combat_entry_encounter: arguments
            .capture_combat_case
            .as_ref()
            .map(|(encounter_id, _)| *encounter_id),
        ..ExactBranchingTaskEvidenceRequestV1::default()
    };
    let result = resume_privileged_reference_run_continuation_v2(
        &archive,
        &checkpoints,
        &arguments.archive_node_id,
        TaskPublicChoiceV1 {
            ordinal_path: arguments.choice_ordinals,
        },
        arguments.budget,
        plan_options,
        evidence_request,
    )?;

    let captured_combat_case = match (
        arguments.capture_combat_case.as_ref(),
        result.execution.captured_combat_case.as_ref(),
    ) {
        (Some((_, path)), None) => {
            return Err(format!(
                "continuation never reached requested combat encounter for {}",
                path.display()
            ));
        }
        (Some((_, path)), Some(case)) => {
            Some((path, json_payload(case, "captured production combat case")?))
        }
        (None, _) => None,
    };

    let recorded_attempt = result
        .archive
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_id == result.attempt_id)
        .expect("continuation result records its attempt");
    let receipt = RunSolutionBranchReceiptV3 {
        schema: RECEIPT_SCHEMA_V3.to_string(),
        seed: result.seed,
        forced_deviation: result.forced_deviation.clone(),
        actor: result.episode.actor.clone(),
        outcome: result.episode.outcome.clone(),
        unsupported_reason: result.episode.unsupported_reason.clone(),
        requested_budget: arguments.budget,
        remaining_budget: result.episode.final_view.budget,
        execution_manifest_id: result.execution_manifest_id.clone(),
        attempt_id: result.attempt_id.clone(),
        source_archive_nodes,
        source_archive_attempts,
        source_checkpoints,
        merged_archive_nodes: result.archive.nodes.len(),
        merged_archive_attempts: result.archive.attempts.len(),
        merged_checkpoints: result.checkpoints.checkpoint_count(),
        committed_public_steps: result.episode.final_view.committed_history.steps.len(),
        committed_scopes: result
            .episode
            .final_view
            .committed_history
            .completed_scopes
            .len(),
        observed_frontier_count: recorded_attempt.observed_frontier_node_ids.len(),
        strategic_support_count: result.strategic_support.len(),
        combat_research_plans_installed: result.combat_research_plans_installed,
        combat_research_plans_unavailable: result.combat_research_plans_unavailable,
        combat_research_plan_actions: result.combat_research_plan_actions,
        combat_research_plan_attempts: result.combat_research_plan_attempts.len(),
    };
    let archive_output = compact_json_payload(&result.archive, "merged run solution archive")?;
    if archive_output.len() > arguments.artifact_max_bytes {
        return Err("merged run solution archive exceeds --artifact-max-bytes".to_string());
    }
    let checkpoint_output = result.checkpoints.encode(arguments.artifact_max_bytes)?;
    let receipt_output = json_payload(&receipt, "run solution branch receipt")?;

    create_parent_directory(&arguments.output_archive)?;
    create_parent_directory(&arguments.output_checkpoints)?;
    create_parent_directory(&arguments.output)?;
    if let Some((path, _)) = captured_combat_case.as_ref() {
        create_parent_directory(path)?;
    }
    write_payload_new(
        &arguments.output_archive,
        &archive_output,
        "merged run solution archive",
    )?;
    write_payload_new(
        &arguments.output_checkpoints,
        &checkpoint_output,
        "merged run branch checkpoints",
    )?;
    write_payload_new(
        &arguments.output,
        &receipt_output,
        "run solution branch receipt",
    )?;
    if let Some((path, payload)) = captured_combat_case {
        write_payload_new(path, &payload, "captured production combat case")?;
    }
    println!(
        "run-solution-branch outcome={:?} nodes={}->{} checkpoints={}->{} receipt={}",
        receipt.outcome,
        receipt.source_archive_nodes,
        receipt.merged_archive_nodes,
        receipt.source_checkpoints,
        receipt.merged_checkpoints,
        arguments.output.display()
    );
    Ok(())
}

fn load_artifacts(
    archive_path: &Path,
    checkpoint_path: &Path,
    max_bytes: usize,
) -> Result<(TaskRunSolutionArchiveV1, TaskRunBranchCheckpointArtifactV1), String> {
    let archive_payload = read_bounded(archive_path, max_bytes, "run solution archive")?;
    let archive: TaskRunSolutionArchiveV1 =
        serde_json::from_slice(&archive_payload).map_err(|error| {
            format!(
                "failed to decode run solution archive {}: {error}",
                archive_path.display()
            )
        })?;
    let checkpoint_payload = read_bounded(checkpoint_path, max_bytes, "run branch checkpoints")?;
    let checkpoints = TaskRunBranchCheckpointArtifactV1::decode_for_archive(
        &checkpoint_payload,
        &archive,
        max_bytes,
    )?;
    Ok((archive, checkpoints))
}

fn parse_arguments() -> Result<Option<ParsedArguments>, String> {
    let mut values = std::env::args().skip(1).collect::<Vec<_>>();
    if values.is_empty()
        || values
            .iter()
            .any(|value| matches!(value.as_str(), "-h" | "--help"))
    {
        return Ok(None);
    }
    let mode = values.first().map(String::as_str);
    let inventory = mode == Some("inventory");
    let compact_checkpoints = mode == Some("compact-checkpoints");
    if inventory || compact_checkpoints || mode == Some("continue") {
        values.remove(0);
    }
    if inventory || compact_checkpoints {
        let arguments = parse_inventory_arguments(values.into_iter())?;
        return Ok(Some(if inventory {
            ParsedArguments::Inventory(arguments)
        } else {
            ParsedArguments::CompactCheckpoints(arguments)
        }));
    }
    parse_continue_arguments(values.into_iter())
        .map(ParsedArguments::Continue)
        .map(Some)
}

fn parse_inventory_arguments(
    mut args: std::vec::IntoIter<String>,
) -> Result<InventoryArguments, String> {
    let mut archive = None;
    let mut checkpoints = None;
    let mut output = None;
    let mut artifact_max_bytes = DEFAULT_ARTIFACT_MAX_BYTES;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--archive" => set_path_once(&mut archive, "--archive", args.next())?,
            "--checkpoints" => set_path_once(&mut checkpoints, "--checkpoints", args.next())?,
            "--output" => set_path_once(&mut output, "--output", args.next())?,
            "--artifact-max-bytes" => {
                artifact_max_bytes = parse_value("--artifact-max-bytes", args.next())?
            }
            unknown => return Err(format!("unknown inventory argument {unknown}")),
        }
    }
    if artifact_max_bytes == 0 {
        return Err("--artifact-max-bytes must be positive".to_string());
    }
    Ok(InventoryArguments {
        archive: archive.ok_or_else(|| "--archive is required".to_string())?,
        checkpoints: checkpoints.ok_or_else(|| "--checkpoints is required".to_string())?,
        output: output.ok_or_else(|| "--output is required".to_string())?,
        artifact_max_bytes,
    })
}

fn parse_continue_arguments(
    mut args: std::vec::IntoIter<String>,
) -> Result<ContinueArguments, String> {
    let mut archive = None;
    let mut checkpoints = None;
    let mut archive_node_id = None;
    let mut choice_ordinals = Vec::new();
    let mut max_world_transitions = None;
    let mut max_reloads = 0u32;
    let mut output = None;
    let mut output_archive = None;
    let mut output_checkpoints = None;
    let mut capture_combat_case = None;
    let mut plan_options = None;
    let mut artifact_max_bytes = DEFAULT_ARTIFACT_MAX_BYTES;

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--archive" => set_path_once(&mut archive, "--archive", args.next())?,
            "--checkpoints" => set_path_once(&mut checkpoints, "--checkpoints", args.next())?,
            "--node" => {
                let value = required_value("--node", args.next())?;
                if archive_node_id.replace(value).is_some() {
                    return Err("--node may be specified only once".to_string());
                }
            }
            "--choice-ordinal" => {
                choice_ordinals.push(parse_value("--choice-ordinal", args.next())?)
            }
            "--max-world-transitions" => {
                let value = parse_value("--max-world-transitions", args.next())?;
                if max_world_transitions.replace(value).is_some() {
                    return Err("--max-world-transitions may be specified only once".to_string());
                }
            }
            "--max-reloads" => max_reloads = parse_value("--max-reloads", args.next())?,
            "--output" => set_path_once(&mut output, "--output", args.next())?,
            "--output-archive" => {
                set_path_once(&mut output_archive, "--output-archive", args.next())?
            }
            "--output-checkpoints" => {
                set_path_once(&mut output_checkpoints, "--output-checkpoints", args.next())?
            }
            "--capture-combat-case" => {
                let encounter_value = required_value("--capture-combat-case", args.next())?;
                let encounter_id = serde_json::from_value(serde_json::Value::String(
                    encounter_value.clone(),
                ))
                .map_err(|error| {
                    format!("invalid --capture-combat-case EncounterId {encounter_value}: {error}")
                })?;
                let path = PathBuf::from(required_value("--capture-combat-case", args.next())?);
                if capture_combat_case.replace((encounter_id, path)).is_some() {
                    return Err("--capture-combat-case may be specified only once".to_string());
                }
            }
            "--plan-options" => set_path_once(&mut plan_options, "--plan-options", args.next())?,
            "--artifact-max-bytes" => {
                artifact_max_bytes = parse_value("--artifact-max-bytes", args.next())?
            }
            unknown => return Err(format!("unknown argument {unknown}")),
        }
    }
    if choice_ordinals.is_empty() {
        return Err("at least one --choice-ordinal is required".to_string());
    }
    let budget = TaskBudgetV1 {
        max_world_transitions: max_world_transitions
            .ok_or_else(|| "--max-world-transitions is required".to_string())?,
        max_reloads,
    };
    budget.validate()?;
    if artifact_max_bytes == 0 {
        return Err("--artifact-max-bytes must be positive".to_string());
    }
    Ok(ContinueArguments {
        archive: archive.ok_or_else(|| "--archive is required".to_string())?,
        checkpoints: checkpoints.ok_or_else(|| "--checkpoints is required".to_string())?,
        archive_node_id: archive_node_id.ok_or_else(|| "--node is required".to_string())?,
        choice_ordinals,
        budget,
        output: output.ok_or_else(|| "--output is required".to_string())?,
        output_archive: output_archive.ok_or_else(|| "--output-archive is required".to_string())?,
        output_checkpoints: output_checkpoints
            .ok_or_else(|| "--output-checkpoints is required".to_string())?,
        capture_combat_case,
        plan_options,
        artifact_max_bytes,
    })
}

fn validate_paths(arguments: &ContinueArguments) -> Result<(), String> {
    let mut outputs = vec![
        &arguments.output,
        &arguments.output_archive,
        &arguments.output_checkpoints,
    ];
    if let Some((_, path)) = arguments.capture_combat_case.as_ref() {
        outputs.push(path);
    }
    for (index, path) in outputs.iter().enumerate() {
        if path.exists() {
            return Err(format!("output already exists: {}", path.display()));
        }
        if outputs[..index].contains(path) {
            return Err("output paths must be distinct".to_string());
        }
        if path == &&arguments.archive || path == &&arguments.checkpoints {
            return Err("output paths must not overwrite input artifacts".to_string());
        }
    }
    Ok(())
}

fn read_bounded(path: &Path, max_bytes: usize, description: &str) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "failed to inspect {description} {}: {error}",
            path.display()
        )
    })?;
    if metadata.len() > max_bytes as u64 {
        return Err(format!(
            "{description} exceeds its caller-provided byte limit"
        ));
    }
    fs::read(path)
        .map_err(|error| format!("failed to read {description} {}: {error}", path.display()))
}

fn json_payload<T: Serialize>(value: &T, description: &str) -> Result<Vec<u8>, String> {
    let mut payload = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode {description}: {error}"))?;
    payload.push(b'\n');
    Ok(payload)
}

fn compact_json_payload<T: Serialize>(value: &T, description: &str) -> Result<Vec<u8>, String> {
    let mut payload = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode {description}: {error}"))?;
    payload.push(b'\n');
    Ok(payload)
}

fn create_parent_directory(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create output directory {}: {error}",
                parent.display()
            )
        })?;
    }
    Ok(())
}

fn write_payload_new(path: &Path, payload: &[u8], description: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create {description} {}: {error}", path.display()))?;
    file.write_all(payload)
        .and_then(|_| file.flush())
        .map_err(|error| format!("failed to finish {description} {}: {error}", path.display()))
}

fn set_path_once(
    slot: &mut Option<PathBuf>,
    name: &str,
    value: Option<String>,
) -> Result<(), String> {
    let path = PathBuf::from(required_value(name, value)?);
    if slot.replace(path).is_some() {
        return Err(format!("{name} may be specified only once"));
    }
    Ok(())
}

fn required_value(name: &str, value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| format!("{name} requires a value"))
}

fn parse_value<T>(name: &str, value: Option<String>) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let value = required_value(name, value)?;
    value
        .parse::<T>()
        .map_err(|error| format!("invalid {name} value {value}: {error}"))
}
