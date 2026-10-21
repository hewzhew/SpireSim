//! Shadow-only racing across legal first actions at one exact combat root.
//!
//! Every materialized root action is executed before any candidate can be
//! deferred. Resumable complete-turn generators then receive equal work in
//! deterministic rounds. Between rounds, the least promising half is deferred
//! by the frozen boundary-value teacher; a deferred, unfinished candidate is
//! never reported as a refutation.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use clap::Args;
use serde::Serialize;
use serde_json::{json, Value};
use sts_combat_planner::{
    CombatDecisionRoot, CombatPlanningQuantum, CombatPolicyChoice, CompleteTurnOptionBoundary,
    TurnOptionGenerationStatus, TurnOptionGeneratorConfig, TurnOptionGeneratorSession,
};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_action_imitation::concrete_combat_action_candidates_v1;
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::combat_guidance_bundle::combat_value_prototype_policy_v1;
use sts_oracle_runtime::eval::combat_guidance_bundle::combat_value_prototype_rank_v1;
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::sim::combat_action::combat_action_key;
use sts_oracle_runtime::state::core::{ClientInput, EngineState};

use super::action_boundary_followup::{
    run_exact_boundary_followup, ExactBoundaryFollowupConfig, ExactBoundaryFollowupReport,
};
use super::combat_trace_view::{combat_action_label, readable_turn_option_action_labels};
use super::guidance_artifact_commands::load_value_prototype;

#[derive(Clone, Debug, Args)]
pub struct ActionBoundaryRootRaceArgs {
    /// Exact combat state whose legal first actions are raced.
    #[arg(long)]
    case: PathBuf,
    /// Frozen boundary-value teacher. This deliberately accepts the value
    /// artifact directly so an unrelated action-policy rebuild cannot block a
    /// read-only root-coverage audit.
    #[arg(long)]
    value_prototype: PathBuf,
    /// Additional deterministic generator work per surviving candidate in
    /// each round.
    #[arg(long, value_delimiter = ',', default_value = "64,256,1024")]
    round_work: Vec<usize>,
    /// Maximum concrete structured-selection inputs materialized at the root.
    #[arg(long, default_value_t = 256)]
    max_structured_alternatives: usize,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
    /// Number of final shadow candidates given equal exact continuation work.
    /// Zero disables continuation and leaves a root-only report.
    #[arg(long, default_value_t = 2)]
    followup_top: usize,
    /// Deterministic local-graph generation work per selected boundary.
    #[arg(long, default_value_t = 20_000)]
    followup_work: usize,
    #[arg(long, default_value_t = 32)]
    followup_max_turn_depth: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
struct CandidateObservation {
    evidence_kind: String,
    exact_terminal_win_hp: Option<i32>,
    best_boundary: Option<BoundaryObservation>,
    observed_boundaries: usize,
    observed_terminal_non_wins: usize,
    complete_surface: bool,
    proven_non_win: bool,
}

#[derive(Clone, Debug, Serialize)]
struct BoundaryObservation {
    exact_state_hash: String,
    player_turn: u32,
    player_hp: i32,
    action_count_after_root: usize,
    action_labels_after_root: Vec<String>,
    negative_log_policy: f64,
    value_target_available: bool,
    value_rank: Vec<i32>,
}

#[derive(Clone, Debug, Serialize)]
struct CandidateRoundService {
    round: usize,
    requested_work: usize,
    generation_work_before: usize,
    generation_work_after: usize,
    applied_action_transitions_after: usize,
    unique_successor_states_after: usize,
    retained_work_items_after: usize,
    status_after: String,
    gap_count_after: usize,
}

struct RaceCandidate {
    canonical_index: usize,
    input: ClientInput,
    label: String,
    action_key: String,
    base_policy_rank: usize,
    base_policy_probability: f64,
    root_transition_engine_steps: usize,
    immediate_exact_state_hash: String,
    immediate_position: Option<CombatPosition>,
    generator: Option<TurnOptionGeneratorSession>,
    direct_observation: Option<CandidateObservation>,
    last_generation_status: String,
    last_generation_complete: bool,
    last_gap_count: usize,
    service: Vec<CandidateRoundService>,
    selected_through_round: usize,
    deferred_after_round: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidateRankEvidence {
    tier: u8,
    exact_terminal_win_hp: i32,
    boundary_value_rank: Vec<i32>,
    boundary_hp: i32,
    base_policy_rank: Reverse<usize>,
    stable_index: Reverse<usize>,
}

impl Ord for CandidateRankEvidence {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (
            self.tier,
            self.exact_terminal_win_hp,
            &self.boundary_value_rank,
            self.boundary_hp,
            self.base_policy_rank,
            self.stable_index,
        )
            .cmp(&(
                other.tier,
                other.exact_terminal_win_hp,
                &other.boundary_value_rank,
                other.boundary_hp,
                other.base_policy_rank,
                other.stable_index,
            ))
    }
}

impl PartialOrd for CandidateRankEvidence {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub fn run(args: ActionBoundaryRootRaceArgs) -> Result<Value, String> {
    let started = Instant::now();
    validate_args(&args)?;
    let loaded = load_combat_case(&args.case)?;
    let root_position = loaded.core.position;
    if EngineCombatStepper.terminal(&root_position) != CombatTerminal::Unresolved {
        return Err("action-boundary root race requires a non-terminal combat case".to_string());
    }
    let value_artifact = load_value_prototype(&args.value_prototype)?;
    let value_targets = value_artifact.targets_by_turn();
    let policy = retained_combat_tactical_policy_v1();
    let inputs =
        concrete_combat_action_candidates_v1(&root_position, args.max_structured_alternatives);
    if inputs.is_empty() {
        return Err("action-boundary root race materialized no legal actions".to_string());
    }
    let choices = inputs
        .iter()
        .map(CombatPolicyChoice::Atomic)
        .collect::<Vec<_>>();
    let weights = normalized_weights(policy.weights(&root_position, &choices), inputs.len());
    let ranks = descending_ranks(&weights);
    let mut candidates = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            build_candidate(
                index,
                input,
                &root_position,
                ranks[index],
                weights[index],
                args.max_engine_steps_per_transition,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;

    let legal_surface = EngineCombatStepper.legal_action_surface(&root_position);
    let mut active = (0..candidates.len()).collect::<Vec<_>>();
    let mut round_reports = Vec::new();
    for (round_index, work) in args.round_work.iter().copied().enumerate() {
        let round = round_index + 1;
        let before = active.clone();
        for candidate_index in before.iter().copied() {
            service_candidate(
                &mut candidates[candidate_index],
                round,
                work,
                args.max_engine_steps_per_transition,
            );
        }
        let observations = candidates
            .iter()
            .map(|candidate| {
                observe_candidate(
                    candidate,
                    &value_targets,
                    args.max_engine_steps_per_transition,
                )
            })
            .collect::<Vec<_>>();
        let exact_win_found = before
            .iter()
            .any(|index| observations[*index].exact_terminal_win_hp.is_some());
        let keep = if round == args.round_work.len() {
            before.len()
        } else if exact_win_found {
            before
                .iter()
                .filter(|index| observations[**index].exact_terminal_win_hp.is_some())
                .count()
                .max(1)
        } else {
            before.len().div_ceil(2).max(1)
        };
        active = rank_candidate_indices(&before, &candidates, &observations)
            .into_iter()
            .take(keep)
            .collect();
        for candidate_index in before.iter().copied() {
            if active.contains(&candidate_index) {
                candidates[candidate_index].selected_through_round = round;
            } else if candidates[candidate_index].deferred_after_round.is_none() {
                candidates[candidate_index].deferred_after_round = Some(round);
            }
        }
        round_reports.push(json!({
            "round": round,
            "work_per_served_candidate": work,
            "candidates_before": before.len(),
            "resumable_generators_served": before.iter().filter(|index| {
                candidates[**index].service.last().is_some_and(|entry| entry.round == round)
            }).count(),
            "candidates_with_live_boundary": before.iter().filter(|index| {
                observations[**index].best_boundary.is_some()
            }).count(),
            "exact_terminal_wins": before.iter().filter(|index| {
                observations[**index].exact_terminal_win_hp.is_some()
            }).count(),
            "proven_non_wins": before.iter().filter(|index| observations[**index].proven_non_win).count(),
            "survivors_after": active.len(),
            "survivor_indices": active.clone(),
        }));
        if exact_win_found || round == args.round_work.len() {
            break;
        }
    }

    let observations = candidates
        .iter()
        .map(|candidate| {
            observe_candidate(
                candidate,
                &value_targets,
                args.max_engine_steps_per_transition,
            )
        })
        .collect::<Vec<_>>();
    let final_order = rank_candidate_indices(
        &(0..candidates.len()).collect::<Vec<_>>(),
        &candidates,
        &observations,
    );
    let followup_policy =
        combat_value_prototype_policy_v1(retained_combat_tactical_policy_v1(), &value_artifact);
    let mut followups = HashMap::<usize, ExactBoundaryFollowupReport>::new();
    for index in final_order.iter().copied().take(args.followup_top) {
        if observations[index].exact_terminal_win_hp.is_some() {
            continue;
        }
        let Some(position) = best_boundary_position(&candidates[index], &value_targets) else {
            continue;
        };
        let report = run_exact_boundary_followup(
            position,
            followup_policy.clone(),
            ExactBoundaryFollowupConfig {
                generation_work: args.followup_work,
                max_engine_steps_per_transition: args.max_engine_steps_per_transition,
                max_turn_depth: args.followup_max_turn_depth,
            },
        )?;
        followups.insert(index, report);
    }
    let candidate_reports = final_order
        .iter()
        .enumerate()
        .map(|(final_rank, index)| {
            let candidate = &candidates[*index];
            let observation = &observations[*index];
            json!({
                "final_shadow_rank": final_rank + 1,
                "canonical_index": candidate.canonical_index,
                "input": candidate.input,
                "label": candidate.label,
                "action_key": candidate.action_key,
                "base_policy_rank": candidate.base_policy_rank,
                "base_policy_probability": candidate.base_policy_probability,
                "root_transition_engine_steps": candidate.root_transition_engine_steps,
                "immediate_exact_state_hash": candidate.immediate_exact_state_hash,
                "selected_through_round": candidate.selected_through_round,
                "deferred_after_round": candidate.deferred_after_round,
                "disposition": disposition(candidate, observation),
                "observation": observation,
                "exact_followup": followups.get(index),
                "service": candidate.service,
            })
        })
        .collect::<Vec<_>>();
    let root_exact_state_hash =
        combat_exact_state_hash_v2(&root_position.engine, &root_position.combat);
    Ok(json!({
        "schema_name": "ActionBoundaryRootRaceReportV2",
        "schema_version": 2,
        "authority": {
            "candidate_selection": "shadow_only_frozen_boundary_value",
            "selected_boundary_followup": "exact_replay_authority_under_explicit_work",
        },
        "elapsed_ms": started.elapsed().as_millis(),
        "source_case": args.case,
        "value_prototype": args.value_prototype,
        "root_exact_state_hash": root_exact_state_hash,
        "surface": {
            "materialized_candidates": candidates.len(),
            "atomic_actions": legal_surface.atomic_actions.len(),
            "structured_family_count": legal_surface.selection_families.len(),
            "max_structured_alternatives": args.max_structured_alternatives,
            "complete": legal_surface.selection_families.is_empty(),
        },
        "config": {
            "round_work": args.round_work,
            "selection": "exact_win_then_frozen_boundary_value_then_base_root_prior",
            "survivor_schedule": "ceil_half_without_replacement",
            "generation_policy": "existing_combat_knowledge_only",
            "boundary_selection_scope": "one_frozen_value_best_next_turn_successor_per_first_action",
            "followup_scope": "exact_only_for_the_selected_boundary_not_every_observed_boundary",
            "unknown_contract": "deferred_unfinished_candidates_are_not_refutations",
            "max_engine_steps_per_transition": args.max_engine_steps_per_transition,
            "exact_followup": {
                "top_candidates": args.followup_top,
                "generation_work_per_candidate": args.followup_work,
                "max_turn_depth": args.followup_max_turn_depth,
                "deadline": "none",
            },
        },
        "rounds": round_reports,
        "candidates": candidate_reports,
    }))
}

fn validate_args(args: &ActionBoundaryRootRaceArgs) -> Result<(), String> {
    if args.round_work.is_empty()
        || args.round_work.iter().any(|work| *work == 0)
        || args.max_structured_alternatives == 0
        || args.max_engine_steps_per_transition == 0
        || args.followup_work == 0
        || args.followup_max_turn_depth == 0
    {
        return Err("action-boundary root race budgets must be positive".to_string());
    }
    Ok(())
}

fn best_boundary_position(
    candidate: &RaceCandidate,
    value_targets: &HashMap<u32, Vec<(i32, Vec<i32>)>>,
) -> Option<CombatPosition> {
    if candidate
        .direct_observation
        .as_ref()
        .and_then(|observation| observation.best_boundary.as_ref())
        .is_some()
    {
        return candidate.immediate_position.clone();
    }
    let generator = candidate.generator.as_ref()?;
    let option_index = best_boundary_option_index(generator, value_targets)?;
    Some(
        generator.completed_options()[option_index]
            .exact_successor()
            .clone(),
    )
}

fn best_boundary_option_index(
    generator: &TurnOptionGeneratorSession,
    value_targets: &HashMap<u32, Vec<(i32, Vec<i32>)>>,
) -> Option<usize> {
    let mut best_index = None;
    let mut best_rank = None;
    for (index, option) in generator.completed_options().iter().enumerate() {
        if option.boundary() != CompleteTurnOptionBoundary::NextPlayerTurn {
            continue;
        }
        let position = option.exact_successor();
        let turn = position.combat.turn.turn_count;
        let rank = (
            combat_value_prototype_rank_v1(value_targets, position, turn),
            position.combat.entities.player.current_hp,
            Reverse(option.exact_successor_hash().to_string()),
        );
        if best_rank.as_ref().is_none_or(|best| rank > *best) {
            best_rank = Some(rank);
            best_index = Some(index);
        }
    }
    best_index
}

fn build_candidate(
    canonical_index: usize,
    input: &ClientInput,
    root_position: &CombatPosition,
    base_policy_rank: usize,
    base_policy_probability: f64,
    max_engine_steps_per_transition: usize,
) -> Result<RaceCandidate, String> {
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
            "root-race candidate {canonical_index} did not reach a stable state"
        ));
    }
    let root_turn = root_position.combat.turn.turn_count;
    let immediate_position = step.position.clone();
    let direct_observation = match step.terminal {
        CombatTerminal::Win if !step.position.combat.runtime.combat_smoked => {
            Some(CandidateObservation {
                evidence_kind: "exact_terminal_win".to_string(),
                exact_terminal_win_hp: Some(step.position.combat.entities.player.current_hp),
                complete_surface: true,
                ..CandidateObservation::default()
            })
        }
        CombatTerminal::Win | CombatTerminal::Loss => Some(CandidateObservation {
            evidence_kind: "exact_non_win".to_string(),
            observed_terminal_non_wins: 1,
            complete_surface: true,
            proven_non_win: true,
            ..CandidateObservation::default()
        }),
        CombatTerminal::Unresolved if is_next_player_turn(root_turn, &step.position) => {
            Some(CandidateObservation {
                evidence_kind: "exact_boundary_successor".to_string(),
                best_boundary: Some(BoundaryObservation {
                    exact_state_hash: combat_exact_state_hash_v2(
                        &step.position.engine,
                        &step.position.combat,
                    ),
                    player_turn: step.position.combat.turn.turn_count,
                    player_hp: step.position.combat.entities.player.current_hp,
                    action_count_after_root: 0,
                    action_labels_after_root: Vec::new(),
                    negative_log_policy: 0.0,
                    value_target_available: false,
                    value_rank: Vec::new(),
                }),
                observed_boundaries: 1,
                complete_surface: true,
                ..CandidateObservation::default()
            })
        }
        CombatTerminal::Unresolved => None,
    };
    let generator = if direct_observation.is_none() {
        let decision_root = CombatDecisionRoot::new(step.position.clone())
            .map_err(|error| format!("invalid root-race successor: {error:?}"))?;
        Some(TurnOptionGeneratorSession::with_policy(
            decision_root,
            TurnOptionGeneratorConfig {
                max_engine_steps_per_transition,
                ..TurnOptionGeneratorConfig::default()
            },
            retained_combat_tactical_policy_v1(),
        ))
    } else {
        None
    };
    Ok(RaceCandidate {
        canonical_index,
        input: input.clone(),
        label: combat_action_label(root_position, input),
        action_key: combat_action_key(&root_position.combat, input),
        base_policy_rank,
        base_policy_probability,
        root_transition_engine_steps: step.engine_steps,
        immediate_exact_state_hash: combat_exact_state_hash_v2(
            &step.position.engine,
            &step.position.combat,
        ),
        immediate_position: Some(immediate_position),
        generator,
        direct_observation,
        last_generation_status: "not_started".to_string(),
        last_generation_complete: false,
        last_gap_count: 0,
        service: Vec::new(),
        selected_through_round: 0,
        deferred_after_round: None,
    })
}

fn service_candidate(
    candidate: &mut RaceCandidate,
    round: usize,
    requested_work: usize,
    max_engine_steps_per_transition: usize,
) {
    let Some(generator) = candidate.generator.as_mut() else {
        return;
    };
    if generator.is_finished() {
        return;
    }
    let before = generator.counters().generation_work;
    let report = generator.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum::deterministic(
            requested_work,
            requested_work.saturating_mul(max_engine_steps_per_transition),
        ),
    );
    candidate.last_generation_status = format!("{:?}", report.status);
    candidate.last_generation_complete = report.status == TurnOptionGenerationStatus::Complete;
    candidate.last_gap_count = report.gaps.len();
    candidate.service.push(CandidateRoundService {
        round,
        requested_work,
        generation_work_before: before,
        generation_work_after: report.after.generation_work,
        applied_action_transitions_after: report.after_diagnostics.applied_action_transitions,
        unique_successor_states_after: report.after_diagnostics.unique_successor_states,
        retained_work_items_after: report.retained_work_items,
        status_after: candidate.last_generation_status.clone(),
        gap_count_after: candidate.last_gap_count,
    });
}

fn observe_candidate(
    candidate: &RaceCandidate,
    value_targets: &HashMap<u32, Vec<(i32, Vec<i32>)>>,
    max_engine_steps_per_transition: usize,
) -> CandidateObservation {
    if let Some(mut observation) = candidate.direct_observation.clone() {
        if let Some(boundary) = observation.best_boundary.as_mut() {
            let position = candidate
                .immediate_position
                .as_ref()
                .expect("runtime candidates retain their exact immediate position");
            let turn = position.combat.turn.turn_count;
            boundary.value_target_available = value_targets.contains_key(&turn);
            boundary.value_rank = combat_value_prototype_rank_v1(value_targets, position, turn);
        }
        return observation;
    }
    let Some(generator) = candidate.generator.as_ref() else {
        return CandidateObservation::default();
    };
    let mut exact_terminal_win_hp = None;
    let mut terminal_non_wins = 0usize;
    let mut best_boundary_index = None;
    let mut best_boundary_rank = None;
    let mut observed_boundaries = 0usize;
    for (option_index, option) in generator.completed_options().iter().enumerate() {
        match option.boundary() {
            CompleteTurnOptionBoundary::TerminalWin
                if !option.exact_successor().combat.runtime.combat_smoked =>
            {
                exact_terminal_win_hp = exact_terminal_win_hp.max(Some(
                    option.exact_successor().combat.entities.player.current_hp,
                ));
            }
            CompleteTurnOptionBoundary::NextPlayerTurn => {
                let position = option.exact_successor();
                let turn = position.combat.turn.turn_count;
                observed_boundaries = observed_boundaries.saturating_add(1);
                let value_rank = combat_value_prototype_rank_v1(value_targets, position, turn);
                let rank = (
                    value_rank,
                    position.combat.entities.player.current_hp,
                    Reverse(option.exact_successor_hash().to_string()),
                );
                if best_boundary_rank.as_ref().is_none_or(|best| rank > *best) {
                    best_boundary_rank = Some(rank);
                    best_boundary_index = Some(option_index);
                }
            }
            CompleteTurnOptionBoundary::TerminalWin
            | CompleteTurnOptionBoundary::TerminalLoss
            | CompleteTurnOptionBoundary::Escape => {
                terminal_non_wins = terminal_non_wins.saturating_add(1);
            }
        }
    }
    let complete_surface = generator.is_finished()
        && candidate.last_gap_count == 0
        && candidate.last_generation_complete;
    let best_boundary = best_boundary_index.map(|option_index| {
        let option = &generator.completed_options()[option_index];
        let position = option.exact_successor();
        let turn = position.combat.turn.turn_count;
        BoundaryObservation {
            exact_state_hash: option.exact_successor_hash().to_string(),
            player_turn: turn,
            player_hp: position.combat.entities.player.current_hp,
            action_count_after_root: option.actions().len(),
            action_labels_after_root: readable_turn_option_action_labels(
                candidate
                    .immediate_position
                    .as_ref()
                    .expect("runtime candidates retain their exact immediate position"),
                option.actions(),
                max_engine_steps_per_transition,
            )
            .unwrap_or_default(),
            negative_log_policy: option.negative_log_policy(),
            value_target_available: value_targets.contains_key(&turn),
            value_rank: combat_value_prototype_rank_v1(value_targets, position, turn),
        }
    });
    let proven_non_win =
        complete_surface && exact_terminal_win_hp.is_none() && best_boundary.is_none();
    let evidence_kind = if exact_terminal_win_hp.is_some() {
        "exact_terminal_win"
    } else if best_boundary.is_some() && complete_surface {
        "exact_boundary_successor"
    } else if proven_non_win {
        "exact_non_win"
    } else {
        "budget_unknown"
    };
    CandidateObservation {
        evidence_kind: evidence_kind.to_string(),
        exact_terminal_win_hp,
        best_boundary,
        observed_boundaries,
        observed_terminal_non_wins: terminal_non_wins,
        complete_surface,
        proven_non_win,
    }
}

fn rank_candidate_indices(
    indices: &[usize],
    candidates: &[RaceCandidate],
    observations: &[CandidateObservation],
) -> Vec<usize> {
    let mut ranked = indices.to_vec();
    ranked.sort_by_key(|index| Reverse(rank_evidence(&candidates[*index], &observations[*index])));
    ranked
}

fn rank_evidence(
    candidate: &RaceCandidate,
    observation: &CandidateObservation,
) -> CandidateRankEvidence {
    let tier = if observation.exact_terminal_win_hp.is_some() {
        4
    } else if observation.best_boundary.is_some() {
        3
    } else if observation.proven_non_win {
        0
    } else {
        2
    };
    CandidateRankEvidence {
        tier,
        exact_terminal_win_hp: observation.exact_terminal_win_hp.unwrap_or(i32::MIN),
        boundary_value_rank: observation
            .best_boundary
            .as_ref()
            .map(|boundary| boundary.value_rank.clone())
            .unwrap_or_default(),
        boundary_hp: observation
            .best_boundary
            .as_ref()
            .map(|boundary| boundary.player_hp)
            .unwrap_or(i32::MIN),
        base_policy_rank: Reverse(candidate.base_policy_rank),
        stable_index: Reverse(candidate.canonical_index),
    }
}

fn disposition(candidate: &RaceCandidate, observation: &CandidateObservation) -> &'static str {
    if observation.exact_terminal_win_hp.is_some() {
        "exact_terminal_win"
    } else if observation.proven_non_win {
        "exact_non_win"
    } else if candidate.deferred_after_round.is_some() {
        "deferred_not_refuted"
    } else {
        "survived_shadow_race"
    }
}

fn normalized_weights(raw: Vec<f64>, count: usize) -> Vec<f64> {
    let mut weights = if raw.len() == count {
        raw.into_iter()
            .map(|weight| {
                if weight.is_finite() && weight > 0.0 {
                    weight
                } else {
                    0.0
                }
            })
            .collect::<Vec<_>>()
    } else {
        vec![1.0; count]
    };
    let total = weights.iter().sum::<f64>();
    if total > 0.0 {
        for weight in &mut weights {
            *weight /= total;
        }
    } else {
        weights.fill(1.0 / count as f64);
    }
    weights
}

fn descending_ranks(weights: &[f64]) -> Vec<usize> {
    let mut order = (0..weights.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        weights[*right]
            .total_cmp(&weights[*left])
            .then_with(|| left.cmp(right))
    });
    let mut ranks = vec![0; weights.len()];
    for (rank, index) in order.into_iter().enumerate() {
        ranks[index] = rank + 1;
    }
    ranks
}

fn is_next_player_turn(root_turn: u32, position: &CombatPosition) -> bool {
    position.combat.turn.turn_count > root_turn
        && matches!(position.engine, EngineState::CombatPlayerTurn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(index: usize, base_rank: usize) -> RaceCandidate {
        RaceCandidate {
            canonical_index: index,
            input: ClientInput::EndTurn,
            label: format!("candidate-{index}"),
            action_key: format!("candidate-{index}"),
            base_policy_rank: base_rank,
            base_policy_probability: 0.1,
            root_transition_engine_steps: 0,
            immediate_exact_state_hash: format!("state-{index}"),
            immediate_position: None,
            generator: None,
            direct_observation: None,
            last_generation_status: "not_started".to_string(),
            last_generation_complete: false,
            last_gap_count: 0,
            service: Vec::new(),
            selected_through_round: 0,
            deferred_after_round: None,
        }
    }

    #[test]
    fn unknown_is_ranked_above_exact_refutation() {
        let candidates = vec![candidate(0, 2), candidate(1, 1)];
        let observations = vec![
            CandidateObservation {
                evidence_kind: "budget_unknown".to_string(),
                ..CandidateObservation::default()
            },
            CandidateObservation {
                evidence_kind: "exact_non_win".to_string(),
                complete_surface: true,
                proven_non_win: true,
                ..CandidateObservation::default()
            },
        ];
        assert_eq!(
            rank_candidate_indices(&[0, 1], &candidates, &observations),
            vec![0, 1]
        );
        assert_eq!(
            disposition(&candidates[0], &observations[0]),
            "survived_shadow_race"
        );
    }

    #[test]
    fn deferred_unknown_is_not_relabelled_as_non_win() {
        let mut candidate = candidate(0, 1);
        candidate.deferred_after_round = Some(1);
        let observation = CandidateObservation {
            evidence_kind: "budget_unknown".to_string(),
            ..CandidateObservation::default()
        };
        assert_eq!(
            disposition(&candidate, &observation),
            "deferred_not_refuted"
        );
        assert!(!observation.proven_non_win);
    }

    #[test]
    fn exact_win_outranks_boundary_and_unknown() {
        let candidates = vec![candidate(0, 1), candidate(1, 2), candidate(2, 3)];
        let observations = vec![
            CandidateObservation {
                evidence_kind: "budget_unknown".to_string(),
                ..CandidateObservation::default()
            },
            CandidateObservation {
                evidence_kind: "exact_boundary_successor".to_string(),
                best_boundary: Some(BoundaryObservation {
                    exact_state_hash: "boundary".to_string(),
                    player_turn: 2,
                    player_hp: 30,
                    action_count_after_root: 1,
                    action_labels_after_root: Vec::new(),
                    negative_log_policy: 1.0,
                    value_target_available: true,
                    value_rank: vec![4, 2],
                }),
                ..CandidateObservation::default()
            },
            CandidateObservation {
                evidence_kind: "exact_terminal_win".to_string(),
                exact_terminal_win_hp: Some(7),
                complete_surface: true,
                ..CandidateObservation::default()
            },
        ];
        assert_eq!(
            rank_candidate_indices(&[0, 1, 2], &candidates, &observations),
            vec![2, 1, 0]
        );
    }
}
