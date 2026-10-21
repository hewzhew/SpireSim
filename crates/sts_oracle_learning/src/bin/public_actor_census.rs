#[path = "public_actor_census/run_capture.rs"]
mod run_capture;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use run_capture::{prepare_combat_run_capture_v1, write_combat_run_capture_v1};
use sts_oracle_learning::agent::task::{TaskBudgetV1, TaskObjectiveV1};
use sts_oracle_learning::ai::combat_witness_contract::{
    CombatWitnessPotionPolicyV1, CombatWitnessSatisfactionV1,
};
use sts_oracle_learning::content::monsters::factory::EncounterId;
use sts_oracle_learning::eval::combat_case::load_combat_case;
use sts_oracle_learning::eval::run_control::{
    evaluate_privileged_reference_actor_census_v9,
    evaluate_privileged_reference_actor_census_with_evidence_v9,
    evaluate_privileged_reference_actor_combat_case_census_v9,
    evaluate_privileged_reference_actor_combat_case_census_with_evidence_v9,
    evaluate_public_deterministic_actor_census_v9,
    evaluate_public_rule_search_actor_census_for_task_v9,
    evaluate_public_structured_actor_census_for_task_v9, CombatResearchPlanOptionsV1,
    CompleteTurnProposalBeamResearchOptionsV1, ExactBranchingTaskEvidenceRequestV1,
    ExactBranchingTaskExecutionEvidenceV1, PrivilegedReferenceEpisodeEvidenceRequestV1,
};

const USAGE: &str = "Usage:
  public_actor_census --seed <u64> [--seed <u64> ...]
      --actor <privileged-reference|rule-search|structured-bootstrap|legality-bootstrap>
      --output <fresh.json>
      [--ascension <0..20>] [--objective <act-three|heart>]
  public_actor_census --combat-case <production.case.json>
      --actor privileged-reference
      --output <fresh.json>
      [--combat-plan-work <usize>] [--combat-plan-wall-ms <u64>]
      [--combat-plan-quantum-work <usize>] [--combat-plan-quantum-ms <u64>]
      [--combat-plan-satisfaction <zero-loss|first-win>]
      [--combat-plan-potion-policy <never|all>]
      [--combat-plan-max-potions <u32|unlimited>]
      [--proposal-beam-width <usize>] [--proposal-beam-boundaries <usize>]
      [--proposal-beam-wall-ms <u64>] [--no-proposal-beam]
      [--atomic-challenger-nodes <usize>] [--atomic-challenger-wall-ms <u64>]
      [--no-atomic-challenger]
      [--capture-combat-case <EncounterId> <fresh.case.json>]
      [--capture-combat-run <fresh-directory>]
      [--capture-run-archive <fresh.json>]
      [--capture-run-checkpoints <fresh.bin>]
      [--run-checkpoint-max-bytes <usize>]
      [--max-world-transitions <u64>] [--max-reloads <u32>]

Runs the selected public actor from natural Ironclad entrances, or resumes
one exact production combat case through the privileged reference actor.
Ascension and objective options apply to rule-search and structured-bootstrap; the other
actors retain their A0 Act 3 task.
`--capture-combat-case` pauses at the matching turn-zero public root before
privileged support, then publishes both the paused receipt and canonical case.
The two bootstrap lanes measure legal coverage and termination only; they are
not combat continuations, incumbents, or capability baselines.
The output path must not already exist.";

const DEFAULT_RUN_CHECKPOINT_MAX_BYTES: usize = 64 * 1024 * 1024;

struct Arguments {
    seeds: Vec<u64>,
    combat_case: Option<PathBuf>,
    capture_combat_case: Option<(EncounterId, PathBuf)>,
    capture_combat_run: Option<PathBuf>,
    capture_run_archive: Option<PathBuf>,
    capture_run_checkpoints: Option<PathBuf>,
    run_checkpoint_max_bytes: usize,
    output: PathBuf,
    budget: TaskBudgetV1,
    actor: ActorLane,
    ascension_level: u8,
    objective: TaskObjectiveV1,
    combat_plan_options: CombatResearchPlanOptionsV1,
}

#[derive(Clone, Copy)]
enum ActorLane {
    PrivilegedReference,
    RuleSearch,
    StructuredBootstrap,
    LegalityBootstrap,
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
    if arguments.output.exists() {
        return Err(format!(
            "output already exists: {}",
            arguments.output.display()
        ));
    }
    if let Some((_, path)) = &arguments.capture_combat_case {
        if path.exists() {
            return Err(format!("capture output already exists: {}", path.display()));
        }
        if path == &arguments.output {
            return Err("receipt and combat case require different output paths".to_string());
        }
    }
    if let Some(directory) = &arguments.capture_combat_run {
        if directory.exists() {
            return Err(format!(
                "combat-run capture directory already exists: {}",
                directory.display()
            ));
        }
        if arguments.output.parent() != Some(directory.as_path()) {
            return Err(
                "--output must be a direct child of --capture-combat-run so the corpus is self-contained"
                    .to_string(),
            );
        }
    }
    if let Some(path) = &arguments.capture_run_archive {
        if path.exists() {
            return Err(format!(
                "run archive output already exists: {}",
                path.display()
            ));
        }
        if path == &arguments.output {
            return Err("receipt and run archive require different output paths".to_string());
        }
    }
    if let Some(path) = &arguments.capture_run_checkpoints {
        if path.exists() {
            return Err(format!(
                "run checkpoint output already exists: {}",
                path.display()
            ));
        }
        if path == &arguments.output
            || arguments.capture_run_archive.as_ref() == Some(path)
            || arguments
                .capture_combat_case
                .as_ref()
                .is_some_and(|(_, case_path)| case_path == path)
        {
            return Err(
                "receipt, run archive, combat case, and run checkpoints require distinct output paths"
                    .to_string(),
            );
        }
    }
    let (census, execution, run_solution_archive, run_branch_checkpoints) =
        match (arguments.actor, arguments.combat_case.as_ref()) {
            (ActorLane::PrivilegedReference, Some(path)) => {
                let case = load_combat_case(path)?;
                if let Some((encounter_id, _)) = arguments.capture_combat_case.as_ref() {
                    let result =
                        evaluate_privileged_reference_actor_combat_case_census_with_evidence_v9(
                            &case,
                            arguments.budget,
                            arguments.combat_plan_options,
                            PrivilegedReferenceEpisodeEvidenceRequestV1 {
                                execution: ExactBranchingTaskEvidenceRequestV1 {
                                    combat_entry_encounter: Some(*encounter_id),
                                    ..ExactBranchingTaskEvidenceRequestV1::default()
                                },
                                pause_at_requested_combat_entry: true,
                                ..PrivilegedReferenceEpisodeEvidenceRequestV1::default()
                            },
                        )?;
                    (result.census, result.execution, None, None)
                } else {
                    let census = evaluate_privileged_reference_actor_combat_case_census_v9(
                        &case,
                        arguments.budget,
                        arguments.combat_plan_options,
                    )?;
                    (
                        census,
                        ExactBranchingTaskExecutionEvidenceV1::default(),
                        None,
                        None,
                    )
                }
            }
            (ActorLane::PrivilegedReference, None) => {
                if arguments.capture_combat_case.is_some()
                    || arguments.capture_combat_run.is_some()
                    || arguments.capture_run_archive.is_some()
                    || arguments.capture_run_checkpoints.is_some()
                {
                    let encounter_id = arguments
                        .capture_combat_case
                        .as_ref()
                        .map(|(encounter_id, _)| *encounter_id);
                    let result = evaluate_privileged_reference_actor_census_with_evidence_v9(
                        arguments.seeds[0],
                        arguments.budget,
                        arguments.combat_plan_options,
                        PrivilegedReferenceEpisodeEvidenceRequestV1 {
                            execution: ExactBranchingTaskEvidenceRequestV1 {
                                combat_entry_encounter: encounter_id,
                                capture_all_combat_entries: arguments.capture_combat_run.is_some(),
                            },
                            capture_all_run_branch_points: arguments
                                .capture_run_checkpoints
                                .is_some(),
                            pause_at_requested_combat_entry: encounter_id.is_some(),
                        },
                    )?;
                    (
                        result.census,
                        result.execution,
                        result.run_solution_archive,
                        result.run_branch_checkpoints,
                    )
                } else {
                    let census = evaluate_privileged_reference_actor_census_v9(
                        &arguments.seeds,
                        arguments.budget,
                        arguments.combat_plan_options,
                    )?;
                    (
                        census,
                        ExactBranchingTaskExecutionEvidenceV1::default(),
                        None,
                        None,
                    )
                }
            }
            (ActorLane::StructuredBootstrap, None) => {
                let census = evaluate_public_structured_actor_census_for_task_v9(
                    &arguments.seeds,
                    arguments.ascension_level,
                    arguments.objective,
                    arguments.budget,
                )?;
                (
                    census,
                    ExactBranchingTaskExecutionEvidenceV1::default(),
                    None,
                    None,
                )
            }
            (ActorLane::RuleSearch, None) => {
                let census = evaluate_public_rule_search_actor_census_for_task_v9(
                    &arguments.seeds,
                    arguments.ascension_level,
                    arguments.objective,
                    arguments.budget,
                )?;
                (
                    census,
                    ExactBranchingTaskExecutionEvidenceV1::default(),
                    None,
                    None,
                )
            }
            (ActorLane::LegalityBootstrap, None) => {
                let census = evaluate_public_deterministic_actor_census_v9(
                    &arguments.seeds,
                    arguments.budget,
                )?;
                (
                    census,
                    ExactBranchingTaskExecutionEvidenceV1::default(),
                    None,
                    None,
                )
            }
            (_, Some(_)) => unreachable!("combat-case argument validation restricts the actor"),
        };
    let requested_case = match (
        &arguments.capture_combat_case,
        execution.captured_combat_case.clone(),
    ) {
        (Some((_, path)), None) => {
            return Err(format!(
                "episode never reached requested combat encounter for {}",
                path.display()
            ))
        }
        (Some((_, path)), Some(case)) => Some((path, case)),
        (None, _) => None,
    };
    let combat_run_payloads = arguments
        .capture_combat_run
        .as_ref()
        .map(|directory| {
            prepare_combat_run_capture_v1(directory, &arguments.output, &census, &execution)
        })
        .transpose()?;
    let census_payload = json_payload(&census, "public actor census")?;
    let run_archive_payload = match (
        arguments.capture_run_archive.as_ref(),
        run_solution_archive.as_ref(),
    ) {
        (Some(path), None) => {
            return Err(format!(
                "episode produced no committed run prefix for {}",
                path.display()
            ))
        }
        (Some(_), Some(archive)) => Some(json_payload(archive, "run solution archive")?),
        (None, _) => None,
    };
    let run_checkpoint_payload = match (
        arguments.capture_run_checkpoints.as_ref(),
        run_branch_checkpoints.as_ref(),
    ) {
        (Some(path), None) => {
            return Err(format!(
                "episode produced no private run checkpoints for {}",
                path.display()
            ))
        }
        (Some(_), Some(artifact)) => Some(
            artifact
                .encode(arguments.run_checkpoint_max_bytes)
                .map_err(|error| format!("failed to encode run branch checkpoints: {error}"))?,
        ),
        (None, _) => None,
    };
    let case_payload = requested_case
        .as_ref()
        .map(|(_, case)| json_payload(case, "combat case"))
        .transpose()?;
    if let Some(payloads) = combat_run_payloads.as_ref() {
        write_combat_run_capture_v1(payloads, &arguments.output, &census_payload)?;
    } else {
        create_parent_directory(&arguments.output)?;
        write_payload_new(&arguments.output, &census_payload, "public actor census")?;
    }
    if let Some((path, _)) = requested_case.as_ref() {
        create_parent_directory(path)?;
    }
    if let (Some((path, _)), Some(payload)) = (requested_case, case_payload) {
        write_payload_new(path, &payload, "combat case")?;
    }
    if let (Some(path), Some(payload)) = (
        arguments.capture_run_archive.as_ref(),
        run_archive_payload.as_ref(),
    ) {
        create_parent_directory(path)?;
        write_payload_new(path, payload, "run solution archive")?;
    }
    if let (Some(path), Some(payload)) = (
        arguments.capture_run_checkpoints.as_ref(),
        run_checkpoint_payload.as_ref(),
    ) {
        create_parent_directory(path)?;
        write_payload_new(path, payload, "run branch checkpoints")?;
    }

    let transitions = census
        .episodes
        .iter()
        .map(|episode| episode.budget.world_transitions_used)
        .sum::<u64>();
    let structured_steps = census
        .episodes
        .iter()
        .map(|episode| episode.combat_plan_events.steps_issued)
        .sum::<u64>();
    let wall_millis = census
        .episodes
        .iter()
        .map(|episode| episode.wall_time_millis)
        .sum::<u64>();
    let research_plans = census
        .episodes
        .iter()
        .map(|episode| episode.combat_research_plans_installed)
        .sum::<usize>();
    let research_actions = census
        .episodes
        .iter()
        .map(|episode| episode.combat_research_plan_actions)
        .sum::<usize>();
    let research_misses = census
        .episodes
        .iter()
        .map(|episode| episode.combat_research_plans_unavailable)
        .sum::<usize>();
    println!(
        "public-actor-census episodes={} successes={} committed_defeats={} budget_failures={} unsupported={} protocol_failures={} transitions={} structured_steps={} combat_plans={} combat_misses={} combat_plan_actions={} wall_ms={} output={}",
        census.summary.episodes,
        census.summary.successes,
        census.summary.committed_defeats,
        census.summary.retry_budget_exhausted + census.summary.transition_budget_exhausted,
        census.summary.unsupported_decision_surface,
        census.summary.mechanics_or_protocol_errors,
        transitions,
        structured_steps,
        research_plans,
        research_misses,
        research_actions,
        wall_millis,
        arguments.output.display(),
    );
    if let Some(directory) = arguments.capture_combat_run.as_ref() {
        let committed_actions = execution
            .committed_combats
            .iter()
            .map(|combat| combat.actions.len())
            .sum::<usize>();
        let eligible = census.summary.successes == 1
            && census.episodes.len() == 1
            && execution.captured_combat_entries.len() == execution.committed_combats.len()
            && execution.committed_combats.len() == research_plans
            && committed_actions == research_actions;
        println!(
            "combat-run-capture captured_entries={} committed_wins={} committed_actions={} curriculum_eligible={} directory={}",
            execution.captured_combat_entries.len(),
            execution.committed_combats.len(),
            committed_actions,
            eligible,
            directory.display(),
        );
    }
    if let (Some(path), Some(archive)) = (
        arguments.capture_run_archive.as_ref(),
        run_solution_archive.as_ref(),
    ) {
        let proven_nodes =
            archive
                .nodes
                .iter()
                .filter(|node| {
                    matches!(
                    node.proof,
                    sts_oracle_learning::agent::task::TaskRunArchiveNodeProofV1::ProvenVictory {
                        ..
                    }
                )
                })
                .count();
        println!(
            "run-solution-archive attempts={} nodes={} proven_victory_nodes={} output={}",
            archive.attempts.len(),
            archive.nodes.len(),
            proven_nodes,
            path.display(),
        );
    }
    if let (Some(path), Some(checkpoints), Some(payload)) = (
        arguments.capture_run_checkpoints.as_ref(),
        run_branch_checkpoints.as_ref(),
        run_checkpoint_payload.as_ref(),
    ) {
        println!(
            "run-branch-checkpoints roots={} bytes={} output={}",
            checkpoints.checkpoint_count(),
            payload.len(),
            path.display(),
        );
    }
    Ok(())
}

fn parse_arguments() -> Result<Option<Arguments>, String> {
    let mut args = std::env::args().skip(1);
    let mut seeds = Vec::new();
    let mut combat_case = None;
    let mut capture_combat_case = None;
    let mut capture_combat_run = None;
    let mut capture_run_archive = None;
    let mut capture_run_checkpoints = None;
    let mut run_checkpoint_max_bytes = DEFAULT_RUN_CHECKPOINT_MAX_BYTES;
    let mut output = None;
    let mut max_world_transitions = 2_048u64;
    let mut max_reloads = 0u32;
    let mut actor = ActorLane::StructuredBootstrap;
    let mut ascension_level = 0u8;
    let mut objective = TaskObjectiveV1::ActThreeVictory;
    let mut combat_plan_options = CombatResearchPlanOptionsV1 {
        complete_turn_proposal_beam: Some(CompleteTurnProposalBeamResearchOptionsV1::default()),
        ..CombatResearchPlanOptionsV1::default()
    };
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => return Ok(None),
            "--seed" => seeds.push(parse_value("--seed", args.next())?),
            "--combat-case" => {
                let path = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--combat-case requires a path".to_string())?,
                );
                if combat_case.replace(path).is_some() {
                    return Err("--combat-case may be specified only once".to_string());
                }
            }
            "--output" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--output requires a path".to_string())?;
                if output.replace(PathBuf::from(value)).is_some() {
                    return Err("--output may be specified only once".to_string());
                }
            }
            "--max-world-transitions" => {
                max_world_transitions = parse_value("--max-world-transitions", args.next())?
            }
            "--max-reloads" => max_reloads = parse_value("--max-reloads", args.next())?,
            "--ascension" => ascension_level = parse_value("--ascension", args.next())?,
            "--objective" => {
                objective = match args.next().as_deref() {
                    Some("act-three") => TaskObjectiveV1::ActThreeVictory,
                    Some("heart") => TaskObjectiveV1::HeartVictory,
                    Some(value) => return Err(format!("unknown --objective value {value}")),
                    None => return Err("--objective requires act-three or heart".to_string()),
                }
            }
            "--combat-plan-work" => {
                combat_plan_options.max_generation_work =
                    parse_value("--combat-plan-work", args.next())?
            }
            "--combat-plan-wall-ms" => {
                combat_plan_options.wall_ms = parse_value("--combat-plan-wall-ms", args.next())?
            }
            "--combat-plan-quantum-work" => {
                combat_plan_options.quantum_generation_work =
                    parse_value("--combat-plan-quantum-work", args.next())?
            }
            "--combat-plan-quantum-ms" => {
                combat_plan_options.quantum_wall_ms =
                    parse_value("--combat-plan-quantum-ms", args.next())?
            }
            "--combat-plan-satisfaction" => {
                combat_plan_options.satisfaction = match args.next().as_deref() {
                    Some("zero-loss") => CombatWitnessSatisfactionV1::ZeroLossOrBudget,
                    Some("first-win") => CombatWitnessSatisfactionV1::FirstCompleteWin,
                    Some(value) => {
                        return Err(format!("unknown --combat-plan-satisfaction value {value}"))
                    }
                    None => {
                        return Err("--combat-plan-satisfaction requires zero-loss or first-win"
                            .to_string())
                    }
                }
            }
            "--combat-plan-potion-policy" => {
                combat_plan_options.potion_policy = match args.next().as_deref() {
                    Some("never") => CombatWitnessPotionPolicyV1::Never,
                    Some("all") => CombatWitnessPotionPolicyV1::All,
                    Some(value) => {
                        return Err(format!("unknown --combat-plan-potion-policy value {value}"))
                    }
                    None => {
                        return Err("--combat-plan-potion-policy requires never or all".to_string())
                    }
                }
            }
            "--combat-plan-max-potions" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--combat-plan-max-potions requires a value".to_string())?;
                combat_plan_options.max_potions_used = match value.as_str() {
                    "unlimited" => None,
                    _ => Some(value.parse::<u32>().map_err(|error| {
                        format!("invalid --combat-plan-max-potions value {value}: {error}")
                    })?),
                };
            }
            "--proposal-beam-width" => {
                let beam = combat_plan_options
                    .complete_turn_proposal_beam
                    .as_mut()
                    .ok_or_else(|| {
                        "--proposal-beam-width conflicts with --no-proposal-beam".to_string()
                    })?;
                beam.beam_width = parse_value("--proposal-beam-width", args.next())?;
            }
            "--proposal-beam-boundaries" => {
                let beam = combat_plan_options
                    .complete_turn_proposal_beam
                    .as_mut()
                    .ok_or_else(|| {
                        "--proposal-beam-boundaries conflicts with --no-proposal-beam".to_string()
                    })?;
                beam.max_expanded_boundaries =
                    parse_value("--proposal-beam-boundaries", args.next())?;
            }
            "--proposal-beam-wall-ms" => {
                let beam = combat_plan_options
                    .complete_turn_proposal_beam
                    .as_mut()
                    .ok_or_else(|| {
                        "--proposal-beam-wall-ms conflicts with --no-proposal-beam".to_string()
                    })?;
                beam.wall_ms = parse_value("--proposal-beam-wall-ms", args.next())?;
            }
            "--no-proposal-beam" => {
                combat_plan_options.complete_turn_proposal_beam = None;
            }
            "--atomic-challenger-nodes" => {
                let challenger =
                    combat_plan_options
                        .atomic_challenger
                        .as_mut()
                        .ok_or_else(|| {
                            "--atomic-challenger-nodes conflicts with --no-atomic-challenger"
                                .to_string()
                        })?;
                challenger.max_nodes = parse_value("--atomic-challenger-nodes", args.next())?;
            }
            "--atomic-challenger-wall-ms" => {
                let challenger =
                    combat_plan_options
                        .atomic_challenger
                        .as_mut()
                        .ok_or_else(|| {
                            "--atomic-challenger-wall-ms conflicts with --no-atomic-challenger"
                                .to_string()
                        })?;
                challenger.wall_ms = parse_value("--atomic-challenger-wall-ms", args.next())?;
            }
            "--no-atomic-challenger" => {
                combat_plan_options.atomic_challenger = None;
            }
            "--capture-combat-case" => {
                let encounter_value = args
                    .next()
                    .ok_or_else(|| "--capture-combat-case requires an EncounterId".to_string())?;
                let encounter_id =
                    serde_json::from_value(serde_json::Value::String(encounter_value.clone()))
                        .map_err(|error| {
                            format!(
                        "invalid --capture-combat-case EncounterId {encounter_value}: {error}"
                    )
                        })?;
                let path = PathBuf::from(args.next().ok_or_else(|| {
                    "--capture-combat-case requires a fresh output path".to_string()
                })?);
                if capture_combat_case.replace((encounter_id, path)).is_some() {
                    return Err("--capture-combat-case may be specified only once".to_string());
                }
            }
            "--capture-combat-run" => {
                let path = PathBuf::from(args.next().ok_or_else(|| {
                    "--capture-combat-run requires a fresh directory".to_string()
                })?);
                if capture_combat_run.replace(path).is_some() {
                    return Err("--capture-combat-run may be specified only once".to_string());
                }
            }
            "--capture-run-archive" => {
                let path = PathBuf::from(args.next().ok_or_else(|| {
                    "--capture-run-archive requires a fresh output path".to_string()
                })?);
                if capture_run_archive.replace(path).is_some() {
                    return Err("--capture-run-archive may be specified only once".to_string());
                }
            }
            "--capture-run-checkpoints" => {
                let path = PathBuf::from(args.next().ok_or_else(|| {
                    "--capture-run-checkpoints requires a fresh output path".to_string()
                })?);
                if capture_run_checkpoints.replace(path).is_some() {
                    return Err(
                        "--capture-run-checkpoints may be specified only once".to_string(),
                    );
                }
            }
            "--run-checkpoint-max-bytes" => {
                run_checkpoint_max_bytes =
                    parse_value("--run-checkpoint-max-bytes", args.next())?;
            }
            "--actor" => actor = match args.next().as_deref() {
                Some("privileged-reference") => ActorLane::PrivilegedReference,
                Some("rule-search") => ActorLane::RuleSearch,
                Some("structured-bootstrap") => ActorLane::StructuredBootstrap,
                Some("legality-bootstrap") => ActorLane::LegalityBootstrap,
                Some(value) => return Err(format!("unknown --actor value {value}")),
                None => return Err(
                    "--actor requires privileged-reference, rule-search, structured-bootstrap, or legality-bootstrap"
                        .to_string(),
                ),
            },
            unknown => return Err(format!("unknown argument {unknown}")),
        }
    }
    if seeds.is_empty() == combat_case.is_none() {
        return Err("specify either one or more --seed values or one --combat-case".to_string());
    }
    if combat_case.is_some() && !matches!(actor, ActorLane::PrivilegedReference) {
        return Err("--combat-case requires --actor privileged-reference".to_string());
    }
    if ascension_level > 20 {
        return Err("--ascension must be in 0..=20".to_string());
    }
    if (ascension_level != 0 || objective != TaskObjectiveV1::ActThreeVictory)
        && !matches!(
            actor,
            ActorLane::StructuredBootstrap | ActorLane::RuleSearch
        )
    {
        return Err(
            "--ascension and --objective require --actor rule-search or structured-bootstrap"
                .to_string(),
        );
    }
    if capture_combat_case.is_some()
        && (!matches!(actor, ActorLane::PrivilegedReference)
            || (combat_case.is_none() && seeds.len() != 1))
    {
        return Err(
            "--capture-combat-case requires --actor privileged-reference with one --seed or one --combat-case"
                .to_string(),
        );
    }
    if capture_combat_run.is_some()
        && (!matches!(actor, ActorLane::PrivilegedReference)
            || combat_case.is_some()
            || seeds.len() != 1)
    {
        return Err(
            "--capture-combat-run requires --actor privileged-reference with exactly one --seed"
                .to_string(),
        );
    }
    if capture_combat_run.is_some() && capture_combat_case.is_some() {
        return Err(
            "--capture-combat-run and --capture-combat-case are separate evidence requests"
                .to_string(),
        );
    }
    if capture_run_archive.is_some()
        && (!matches!(actor, ActorLane::PrivilegedReference)
            || combat_case.is_some()
            || seeds.len() != 1)
    {
        return Err(
            "--capture-run-archive requires --actor privileged-reference with exactly one natural --seed"
                .to_string(),
        );
    }
    if capture_run_checkpoints.is_some() && capture_run_archive.is_none() {
        return Err(
            "--capture-run-checkpoints requires --capture-run-archive so every private root is bound to public evidence"
                .to_string(),
        );
    }
    combat_plan_options.validate()?;
    let output = output.ok_or_else(|| "--output is required".to_string())?;
    Ok(Some(Arguments {
        seeds,
        combat_case,
        capture_combat_case,
        capture_combat_run,
        capture_run_archive,
        capture_run_checkpoints,
        run_checkpoint_max_bytes,
        output,
        budget: TaskBudgetV1 {
            max_world_transitions,
            max_reloads,
        },
        actor,
        ascension_level,
        objective,
        combat_plan_options,
    }))
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

fn json_payload<T: serde::Serialize>(value: &T, description: &str) -> Result<Vec<u8>, String> {
    let mut payload = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode {description}: {error}"))?;
    payload.push(b'\n');
    Ok(payload)
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

fn parse_value<T>(name: &str, value: Option<String>) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let value = value.ok_or_else(|| format!("{name} requires a value"))?;
    value
        .parse::<T>()
        .map_err(|error| format!("invalid {name} value {value}: {error}"))
}
