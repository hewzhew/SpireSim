//! One-turn action evidence under a frozen boundary-value teacher.
//!
//! Each legal root action is executed exactly, then expanded only until the
//! next player-turn boundary or terminal combat state.  A complete generation
//! surface may identify the best boundary successor under the frozen value
//! artifact.  Any truncated or mechanics-gapped surface remains
//! `BudgetUnknown`, even when an attractive successor was observed.

use std::collections::HashMap;
use std::path::PathBuf;

use clap::Args;
use serde::Serialize;
use serde_json::{json, Value};
use sts_combat_planner::{
    CombatDecisionRoot, CombatPlanningQuantum, CombatPolicyChoice, CompleteTurnOption,
    CompleteTurnOptionBoundary, SharedCombatActionPolicy, TurnOptionGenerationStatus,
    TurnOptionGeneratorConfig, TurnOptionGeneratorSession,
};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_action_imitation::{
    combat_action_imitation_policy_v1, concrete_combat_action_candidates_v1,
};
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::combat_guidance_bundle::{
    combat_value_prototype_rank_v1, typed_combat_value_features_v1, CombatGuidanceBundleV1,
    COMBAT_VALUE_FEATURE_SCHEMA,
};
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::sim::combat_action::combat_action_key;
use sts_oracle_runtime::state::core::{ClientInput, EngineState};

use super::canonical_launch::runtime_source_content_fingerprint;
use super::combat_replay_tools::replay_combat_inputs;
use super::combat_trace_view::combat_action_label;
use super::exact_turn_corridor::load_action_segments as load_combat_action_segments;
use super::{oracle_lab_runtime_identity, source_content_fingerprint};

const CORPUS_SCHEMA: &str = "ActionBoundaryEvidenceCorpusV1";

#[derive(Debug, Args)]
pub(super) struct ActionBoundaryEvidenceArgs {
    /// Exact combat case at the beginning of a verified terminal witness.
    #[arg(long)]
    case: PathBuf,
    /// One or more consecutive exact action segments forming that witness.
    #[arg(long, required = true)]
    actions: Vec<PathBuf>,
    /// Number of witness actions replayed before auditing the next action.
    #[arg(long)]
    through: usize,
    /// Frozen action/value bundle used for proposal order and boundary value.
    #[arg(long)]
    guidance_bundle: PathBuf,
    /// Destination for the typed offline evidence corpus.
    #[arg(long)]
    output: PathBuf,
    /// Deterministic complete-turn generation work per root action.
    #[arg(long, default_value_t = 5_000)]
    generation_work_per_candidate: usize,
    /// Maximum independent action continuations evaluated concurrently.
    #[arg(long, default_value_t = 4)]
    candidate_jobs: usize,
    /// Maximum concrete structured-selection inputs materialized at the root.
    #[arg(long, default_value_t = 256)]
    max_structured_alternatives: usize,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

#[derive(Debug, Args)]
pub(super) struct ActionBoundaryEvidenceBatchArgs {
    /// Queue produced by `build-action-reanalysis-queue`.
    #[arg(long)]
    queue: PathBuf,
    /// Frozen action/value bundle shared by every boundary audit.
    #[arg(long)]
    guidance_bundle: PathBuf,
    /// Directory receiving one typed boundary corpus per selected state.
    #[arg(long)]
    output_dir: PathBuf,
    /// Compact report listing every generated corpus.
    #[arg(long)]
    report: PathBuf,
    /// Number of highest-priority queue items to inspect.
    #[arg(long, default_value_t = 24)]
    take: usize,
    /// Number of higher-priority queue items already processed.
    #[arg(long, default_value_t = 0)]
    skip: usize,
    #[arg(long, default_value_t = 5_000)]
    generation_work_per_candidate: usize,
    #[arg(long, default_value_t = 4)]
    candidate_jobs: usize,
    #[arg(long, default_value_t = 256)]
    max_structured_alternatives: usize,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

#[derive(Clone, Debug, Serialize)]
struct BoundaryGenerationEvidence {
    status: String,
    generation_work: usize,
    engine_steps: usize,
    completed_options: usize,
    applied_action_transitions: usize,
    unique_successor_states: usize,
    retained_work_items: usize,
    gap_count: usize,
    complete_surface: bool,
}

impl BoundaryGenerationEvidence {
    fn direct() -> Self {
        Self {
            status: "direct_exact_transition".to_string(),
            generation_work: 0,
            engine_steps: 0,
            completed_options: 1,
            applied_action_transitions: 0,
            unique_successor_states: 1,
            retained_work_items: 0,
            gap_count: 0,
            complete_surface: true,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct BoundaryValueObservation {
    exact_state_hash: String,
    player_turn: u32,
    player_hp: i32,
    action_suffix_count: usize,
    action_suffix: Vec<ClientInput>,
    negative_log_policy: Option<f64>,
    value_target_available: bool,
    value_rank: Vec<i32>,
    value_features: Vec<i32>,
    known_witness_boundary: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ActionBoundaryEvidence {
    ExactTerminalWin {
        source: String,
        final_hp: i32,
        continuation_actions: Vec<ClientInput>,
        generation: BoundaryGenerationEvidence,
    },
    ExactBoundarySuccessor {
        successor: BoundaryValueObservation,
        observed_boundary_count: usize,
        terminal_non_win_count: usize,
        generation: BoundaryGenerationEvidence,
    },
    ExactNonWin {
        boundary: String,
        terminal_non_win_count: usize,
        generation: BoundaryGenerationEvidence,
    },
    BudgetUnknown {
        best_observed_successor: Option<BoundaryValueObservation>,
        observed_boundary_count: usize,
        terminal_non_win_count: usize,
        generation: BoundaryGenerationEvidence,
    },
}

impl ActionBoundaryEvidence {
    fn kind(&self) -> &'static str {
        match self {
            Self::ExactTerminalWin { .. } => "exact_terminal_win",
            Self::ExactBoundarySuccessor { .. } => "exact_boundary_successor",
            Self::ExactNonWin { .. } => "exact_non_win",
            Self::BudgetUnknown { .. } => "budget_unknown",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct ActionBoundaryCandidate {
    canonical_index: usize,
    input: ClientInput,
    label: String,
    action_key: String,
    known_witness_action: bool,
    base_policy_rank: usize,
    base_policy_probability: f64,
    guided_policy_rank: usize,
    guided_policy_probability: f64,
    transition_engine_steps: usize,
    immediate_exact_successor_hash: String,
    evidence: ActionBoundaryEvidence,
}

#[derive(Clone)]
struct PolicyDistribution {
    ranks: Vec<usize>,
    probabilities: Vec<f64>,
}

pub(super) fn build(args: ActionBoundaryEvidenceArgs) -> Result<Value, String> {
    validate_args(&args)?;
    let case = load_combat_case(&args.case)?;
    let witness_actions = load_combat_action_segments(&args.actions)?;
    if args.through >= witness_actions.len() {
        return Err(format!(
            "--through {} must select one of the {} verified witness actions",
            args.through,
            witness_actions.len()
        ));
    }
    let final_position = replay_combat_inputs(
        case.core.position.clone(),
        &witness_actions,
        args.max_engine_steps_per_transition,
    )?;
    if EngineCombatStepper.terminal(&final_position) != CombatTerminal::Win
        || final_position.combat.runtime.combat_smoked
    {
        return Err("action-boundary source is not an exact non-smoke victory".to_string());
    }
    let root_position = replay_combat_inputs(
        case.core.position,
        &witness_actions[..args.through],
        args.max_engine_steps_per_transition,
    )?;
    if EngineCombatStepper.terminal(&root_position) != CombatTerminal::Unresolved {
        return Err("action-boundary audit root is already terminal".to_string());
    }
    let known_witness_action = witness_actions[args.through].clone();
    let known_boundary_actions = witness_actions_to_next_boundary(
        &root_position,
        &witness_actions[args.through..],
        args.max_engine_steps_per_transition,
    )?;

    let bundle = CombatGuidanceBundleV1::load(&args.guidance_bundle)?;
    let value_targets = bundle.boundary_value.targets_by_turn();
    let guided_policy = combat_action_imitation_policy_v1(
        retained_combat_tactical_policy_v1(),
        bundle.action_imitation.clone(),
    )?;
    let mut inputs =
        concrete_combat_action_candidates_v1(&root_position, args.max_structured_alternatives);
    if !inputs.contains(&known_witness_action) {
        inputs.push(known_witness_action.clone());
    }
    if inputs.is_empty() {
        return Err("action-boundary root has no materialized legal actions".to_string());
    }
    let base_distribution = policy_distribution(
        retained_combat_tactical_policy_v1(),
        &root_position,
        &inputs,
    );
    let guided_distribution = policy_distribution(guided_policy.clone(), &root_position, &inputs);

    let jobs = args.candidate_jobs.min(inputs.len()).max(1);
    let chunk_len = inputs.len().div_ceil(jobs);
    let batches = std::thread::scope(|scope| {
        let handles = inputs
            .chunks(chunk_len)
            .enumerate()
            .map(|(chunk_index, chunk)| {
                let root_position = &root_position;
                let known_witness_action = &known_witness_action;
                let known_boundary_actions = &known_boundary_actions;
                let value_targets = &value_targets;
                let guided_policy = guided_policy.clone();
                let base_distribution = &base_distribution;
                let guided_distribution = &guided_distribution;
                let args = &args;
                scope.spawn(move || {
                    chunk
                        .iter()
                        .enumerate()
                        .map(|(offset, input)| {
                            let canonical_index = chunk_index * chunk_len + offset;
                            build_candidate(
                                canonical_index,
                                input,
                                root_position,
                                known_witness_action,
                                known_boundary_actions,
                                value_targets,
                                guided_policy.clone(),
                                base_distribution.ranks[canonical_index],
                                base_distribution.probabilities[canonical_index],
                                guided_distribution.ranks[canonical_index],
                                guided_distribution.probabilities[canonical_index],
                                args.generation_work_per_candidate,
                                args.max_engine_steps_per_transition,
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| "action-boundary candidate worker panicked".to_string())
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    let mut candidates = batches
        .into_iter()
        .flatten()
        .collect::<Result<Vec<_>, String>>()?;
    candidates.sort_by_key(|candidate| candidate.canonical_index);

    let legal_surface = EngineCombatStepper.legal_action_surface(&root_position);
    let mut evidence_counts = std::collections::BTreeMap::<&'static str, usize>::new();
    for candidate in &candidates {
        *evidence_counts
            .entry(candidate.evidence.kind())
            .or_default() += 1;
    }
    let corpus = json!({
        "schema_name": CORPUS_SCHEMA,
        "schema_version": 1,
        "runtime": oracle_lab_runtime_identity(),
        "runtime_source_content_fingerprint": runtime_source_content_fingerprint()?,
        "source_case": args.case,
        "source_actions": args.actions,
        "through": args.through,
        "root_exact_state_hash": combat_exact_state_hash_v2(
            &root_position.engine,
            &root_position.combat,
        ),
        "root_position": root_position,
        "known_witness_action": known_witness_action,
        "known_witness_final_hp": final_position.combat.entities.player.current_hp,
        "guidance_bundle": args.guidance_bundle,
        "guidance_bundle_content_fingerprint": source_content_fingerprint(
            &std::env::current_dir().map_err(|error| error.to_string())?,
            std::slice::from_ref(&args.guidance_bundle),
        )?,
        "value_feature_schema": COMBAT_VALUE_FEATURE_SCHEMA,
        "surface": {
            "materialized_candidates": candidates.len(),
            "atomic_actions": legal_surface.atomic_actions.len(),
            "structured_family_count": legal_surface.selection_families.len(),
            "max_structured_alternatives": args.max_structured_alternatives,
            "complete": legal_surface.selection_families.is_empty(),
        },
        "config": {
            "generation_work_per_candidate": args.generation_work_per_candidate,
            "candidate_jobs": jobs,
            "generation_policy": "bundled_action_imitation_over_existing_policy",
            "boundary_teacher": "frozen_bundled_value_prototype",
            "max_engine_steps_per_transition": args.max_engine_steps_per_transition,
        },
        "evidence_counts": evidence_counts,
        "candidates": candidates,
    });
    if let Some(parent) = args.output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        &args.output,
        serde_json::to_vec_pretty(&corpus).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "schema_name": "ActionBoundaryEvidenceBuildReportV1",
        "schema_version": 1,
        "output": args.output,
        "root_exact_state_hash": corpus["root_exact_state_hash"],
        "known_witness_action": corpus["known_witness_action"],
        "surface": corpus["surface"],
        "evidence_counts": corpus["evidence_counts"],
    }))
}

pub(super) fn build_batch(args: ActionBoundaryEvidenceBatchArgs) -> Result<Value, String> {
    if args.take == 0 {
        return Err("--take must be positive".to_string());
    }
    let queue = super::action_reanalysis_queue::load_queue(&args.queue)?;
    let selected = queue
        .queue
        .iter()
        .skip(args.skip)
        .take(args.take)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(format!(
            "action-boundary batch selected no states from {}",
            args.queue.display()
        ));
    }
    std::fs::create_dir_all(&args.output_dir).map_err(|error| error.to_string())?;
    let mut generated = Vec::with_capacity(selected.len());
    for item in selected {
        let hash_prefix = item
            .exact_state_hash
            .get(..12)
            .unwrap_or(&item.exact_state_hash);
        let output = args.output_dir.join(format!(
            "boundary-{:03}-{hash_prefix}.json",
            item.queue_rank
        ));
        let result = build(ActionBoundaryEvidenceArgs {
            case: item.source_case.clone(),
            actions: item.source_actions.clone(),
            through: item.through,
            guidance_bundle: args.guidance_bundle.clone(),
            output: output.clone(),
            generation_work_per_candidate: args.generation_work_per_candidate,
            candidate_jobs: args.candidate_jobs,
            max_structured_alternatives: args.max_structured_alternatives,
            max_engine_steps_per_transition: args.max_engine_steps_per_transition,
        })?;
        generated.push(json!({
            "queue_rank": item.queue_rank,
            "demonstration_id": item.demonstration_id,
            "through": item.through,
            "source_exact_state_hash": item.exact_state_hash,
            "output": output,
            "result": result,
        }));
    }
    let report = json!({
        "schema_name": "ActionBoundaryEvidenceBatchReportV1",
        "schema_version": 1,
        "queue": args.queue,
        "guidance_bundle": args.guidance_bundle,
        "skip": args.skip,
        "take": args.take,
        "generated_count": generated.len(),
        "generated": generated,
    });
    if let Some(parent) = args.report.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        &args.report,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(report)
}

fn validate_args(args: &ActionBoundaryEvidenceArgs) -> Result<(), String> {
    if args.generation_work_per_candidate == 0
        || args.candidate_jobs == 0
        || args.max_structured_alternatives == 0
        || args.max_engine_steps_per_transition == 0
    {
        return Err("action-boundary evidence budgets must be positive".to_string());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_candidate(
    canonical_index: usize,
    input: &ClientInput,
    root_position: &CombatPosition,
    known_witness_action: &ClientInput,
    known_boundary_actions: &[ClientInput],
    value_targets: &HashMap<u32, Vec<(i32, Vec<i32>)>>,
    guided_policy: SharedCombatActionPolicy,
    base_policy_rank: usize,
    base_policy_probability: f64,
    guided_policy_rank: usize,
    guided_policy_probability: f64,
    generation_work: usize,
    max_engine_steps_per_transition: usize,
) -> Result<ActionBoundaryCandidate, String> {
    let step = EngineCombatStepper.apply_to_stable(
        root_position,
        input.clone(),
        CombatStepLimits {
            max_engine_steps: max_engine_steps_per_transition,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return Err(format!(
            "action-boundary candidate {canonical_index} did not reach a stable state"
        ));
    }
    let evidence = match step.terminal {
        CombatTerminal::Win if !step.position.combat.runtime.combat_smoked => {
            ActionBoundaryEvidence::ExactTerminalWin {
                source: "immediate_exact_transition".to_string(),
                final_hp: step.position.combat.entities.player.current_hp,
                continuation_actions: Vec::new(),
                generation: BoundaryGenerationEvidence::direct(),
            }
        }
        CombatTerminal::Win => ActionBoundaryEvidence::ExactNonWin {
            boundary: "smoke_escape".to_string(),
            terminal_non_win_count: 1,
            generation: BoundaryGenerationEvidence::direct(),
        },
        CombatTerminal::Loss => ActionBoundaryEvidence::ExactNonWin {
            boundary: "terminal_loss".to_string(),
            terminal_non_win_count: 1,
            generation: BoundaryGenerationEvidence::direct(),
        },
        CombatTerminal::Unresolved
            if is_next_player_turn(root_position.combat.turn.turn_count, &step.position) =>
        {
            ActionBoundaryEvidence::ExactBoundarySuccessor {
                successor: boundary_observation(
                    &step.position,
                    Vec::new(),
                    None,
                    value_targets,
                    input == known_witness_action,
                ),
                observed_boundary_count: 1,
                terminal_non_win_count: 0,
                generation: BoundaryGenerationEvidence::direct(),
            }
        }
        CombatTerminal::Unresolved => evaluate_mid_turn_successors(
            &step.position,
            input == known_witness_action,
            known_boundary_actions.get(1..).unwrap_or_default(),
            value_targets,
            guided_policy,
            generation_work,
            max_engine_steps_per_transition,
        )?,
    };

    Ok(ActionBoundaryCandidate {
        canonical_index,
        input: input.clone(),
        label: combat_action_label(root_position, input),
        action_key: combat_action_key(&root_position.combat, input),
        known_witness_action: input == known_witness_action,
        base_policy_rank,
        base_policy_probability,
        guided_policy_rank,
        guided_policy_probability,
        transition_engine_steps: step.engine_steps,
        immediate_exact_successor_hash: combat_exact_state_hash_v2(
            &step.position.engine,
            &step.position.combat,
        ),
        evidence,
    })
}

fn evaluate_mid_turn_successors(
    position: &CombatPosition,
    is_known_witness_action: bool,
    known_boundary_suffix: &[ClientInput],
    value_targets: &HashMap<u32, Vec<(i32, Vec<i32>)>>,
    guided_policy: SharedCombatActionPolicy,
    generation_work: usize,
    max_engine_steps_per_transition: usize,
) -> Result<ActionBoundaryEvidence, String> {
    let root = CombatDecisionRoot::new(position.clone())
        .map_err(|error| format!("invalid action-boundary successor root: {error:?}"))?;
    let mut generator = TurnOptionGeneratorSession::with_policy(
        root,
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition,
            ..TurnOptionGeneratorConfig::default()
        },
        guided_policy,
    );
    let report = generator.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum::deterministic(
            generation_work,
            generation_work.saturating_mul(max_engine_steps_per_transition),
        ),
    );
    let complete_surface =
        matches!(report.status, TurnOptionGenerationStatus::Complete) && report.gaps.is_empty();
    let generation = BoundaryGenerationEvidence {
        status: format!("{:?}", report.status),
        generation_work: report.after.generation_work,
        engine_steps: report.after.engine_steps,
        completed_options: report.total_completed_options,
        applied_action_transitions: report.after_diagnostics.applied_action_transitions,
        unique_successor_states: report.after_diagnostics.unique_successor_states,
        retained_work_items: report.retained_work_items,
        gap_count: report.gaps.len(),
        complete_surface,
    };

    let mut terminal_wins = Vec::<(i32, Vec<ClientInput>)>::new();
    let mut terminal_non_win_count = 0usize;
    let mut boundaries = Vec::<BoundaryValueObservation>::new();
    for option in generator.completed_options() {
        match option.boundary() {
            CompleteTurnOptionBoundary::TerminalWin => {
                let replayed = replay_combat_inputs(
                    position.clone(),
                    &option_inputs(option),
                    max_engine_steps_per_transition,
                )?;
                if EngineCombatStepper.terminal(&replayed) == CombatTerminal::Win
                    && !replayed.combat.runtime.combat_smoked
                {
                    terminal_wins.push((
                        replayed.combat.entities.player.current_hp,
                        option_inputs(option),
                    ));
                }
            }
            CompleteTurnOptionBoundary::NextPlayerTurn => boundaries.push(boundary_observation(
                option.exact_successor(),
                option_inputs(option),
                Some(option.negative_log_policy()),
                value_targets,
                false,
            )),
            CompleteTurnOptionBoundary::TerminalLoss | CompleteTurnOptionBoundary::Escape => {
                terminal_non_win_count = terminal_non_win_count.saturating_add(1);
            }
        }
    }
    if is_known_witness_action && !known_boundary_suffix.is_empty() {
        let replayed = replay_combat_inputs(
            position.clone(),
            known_boundary_suffix,
            max_engine_steps_per_transition,
        )?;
        match EngineCombatStepper.terminal(&replayed) {
            CombatTerminal::Win if !replayed.combat.runtime.combat_smoked => {
                terminal_wins.push((
                    replayed.combat.entities.player.current_hp,
                    known_boundary_suffix.to_vec(),
                ));
            }
            CombatTerminal::Unresolved
                if is_next_player_turn(position.combat.turn.turn_count, &replayed) =>
            {
                let hash = combat_exact_state_hash_v2(&replayed.engine, &replayed.combat);
                if let Some(existing) = boundaries
                    .iter_mut()
                    .find(|boundary| boundary.exact_state_hash == hash)
                {
                    existing.known_witness_boundary = true;
                } else {
                    boundaries.push(boundary_observation(
                        &replayed,
                        known_boundary_suffix.to_vec(),
                        None,
                        value_targets,
                        true,
                    ));
                }
            }
            CombatTerminal::Win | CombatTerminal::Loss | CombatTerminal::Unresolved => {}
        }
    }
    if let Some((final_hp, continuation_actions)) = terminal_wins
        .into_iter()
        .max_by_key(|(final_hp, actions)| (*final_hp, std::cmp::Reverse(actions.len())))
    {
        return Ok(ActionBoundaryEvidence::ExactTerminalWin {
            source: "exact_current_turn_continuation".to_string(),
            final_hp,
            continuation_actions,
            generation,
        });
    }
    boundaries.sort_by(|left, right| {
        right
            .value_rank
            .cmp(&left.value_rank)
            .then_with(|| left.exact_state_hash.cmp(&right.exact_state_hash))
    });
    let observed_boundary_count = boundaries.len();
    let best_observed_successor = boundaries.into_iter().next();
    if complete_surface {
        if let Some(successor) = best_observed_successor {
            Ok(ActionBoundaryEvidence::ExactBoundarySuccessor {
                successor,
                observed_boundary_count,
                terminal_non_win_count,
                generation,
            })
        } else {
            Ok(ActionBoundaryEvidence::ExactNonWin {
                boundary: "complete_current_turn_has_no_live_successor".to_string(),
                terminal_non_win_count,
                generation,
            })
        }
    } else {
        Ok(ActionBoundaryEvidence::BudgetUnknown {
            best_observed_successor,
            observed_boundary_count,
            terminal_non_win_count,
            generation,
        })
    }
}

fn boundary_observation(
    position: &CombatPosition,
    action_suffix: Vec<ClientInput>,
    negative_log_policy: Option<f64>,
    value_targets: &HashMap<u32, Vec<(i32, Vec<i32>)>>,
    known_witness_boundary: bool,
) -> BoundaryValueObservation {
    let player_turn = position.combat.turn.turn_count;
    BoundaryValueObservation {
        exact_state_hash: combat_exact_state_hash_v2(&position.engine, &position.combat),
        player_turn,
        player_hp: position.combat.entities.player.current_hp,
        action_suffix_count: action_suffix.len(),
        action_suffix,
        negative_log_policy,
        value_target_available: value_targets.contains_key(&player_turn),
        value_rank: combat_value_prototype_rank_v1(value_targets, position, player_turn),
        value_features: typed_combat_value_features_v1(position),
        known_witness_boundary,
    }
}

fn option_inputs(option: &CompleteTurnOption) -> Vec<ClientInput> {
    option
        .actions()
        .iter()
        .map(|action| action.input.clone())
        .collect()
}

fn is_next_player_turn(root_turn: u32, position: &CombatPosition) -> bool {
    position.combat.turn.turn_count > root_turn
        && matches!(position.engine, EngineState::CombatPlayerTurn)
}

fn witness_actions_to_next_boundary(
    root: &CombatPosition,
    actions: &[ClientInput],
    max_engine_steps_per_transition: usize,
) -> Result<Vec<ClientInput>, String> {
    let root_turn = root.combat.turn.turn_count;
    let mut position = root.clone();
    let mut prefix = Vec::new();
    for input in actions {
        position = replay_combat_inputs(
            position,
            std::slice::from_ref(input),
            max_engine_steps_per_transition,
        )?;
        prefix.push(input.clone());
        if EngineCombatStepper.terminal(&position) != CombatTerminal::Unresolved
            || is_next_player_turn(root_turn, &position)
        {
            return Ok(prefix);
        }
    }
    Err("verified witness has no next player-turn or terminal boundary".to_string())
}

fn policy_distribution(
    policy: SharedCombatActionPolicy,
    position: &CombatPosition,
    inputs: &[ClientInput],
) -> PolicyDistribution {
    let choices = inputs
        .iter()
        .map(CombatPolicyChoice::Atomic)
        .collect::<Vec<_>>();
    let raw = policy.weights(position, &choices);
    let weights = if raw.len() == inputs.len() {
        raw.into_iter()
            .map(|weight| {
                if weight.is_finite() && weight > 0.0 {
                    weight
                } else {
                    1.0
                }
            })
            .collect::<Vec<_>>()
    } else {
        vec![1.0; inputs.len()]
    };
    let total = weights.iter().sum::<f64>();
    let uniform = 1.0 / inputs.len() as f64;
    PolicyDistribution {
        ranks: weights
            .iter()
            .map(|weight| 1 + weights.iter().filter(|other| **other > *weight).count())
            .collect(),
        probabilities: weights
            .iter()
            .map(|weight| 0.95 * (*weight / total) + 0.05 * uniform)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_surface_never_claims_exact_boundary_authority() {
        let evidence = ActionBoundaryEvidence::BudgetUnknown {
            best_observed_successor: None,
            observed_boundary_count: 0,
            terminal_non_win_count: 0,
            generation: BoundaryGenerationEvidence {
                status: "Partial(GenerationWorkBudget)".to_string(),
                generation_work: 5,
                engine_steps: 7,
                completed_options: 0,
                applied_action_transitions: 2,
                unique_successor_states: 2,
                retained_work_items: 1,
                gap_count: 0,
                complete_surface: false,
            },
        };

        assert_eq!(evidence.kind(), "budget_unknown");
    }

    #[test]
    fn policy_distribution_is_normalized_and_ranked() {
        struct FixedPolicy;
        impl sts_combat_planner::CombatActionPolicy for FixedPolicy {
            fn weights(
                &self,
                _position: &CombatPosition,
                choices: &[CombatPolicyChoice<'_>],
            ) -> Vec<f64> {
                (1..=choices.len()).map(|value| value as f64).collect()
            }
        }
        let position = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            sts_oracle_runtime::test_support::blank_test_combat(),
        );
        let inputs = vec![ClientInput::EndTurn, ClientInput::EndTurn];
        let distribution =
            policy_distribution(std::sync::Arc::new(FixedPolicy), &position, &inputs);

        assert_eq!(distribution.ranks, vec![2, 1]);
        assert!((distribution.probabilities.iter().sum::<f64>() - 1.0).abs() < 1e-9);
    }
}
