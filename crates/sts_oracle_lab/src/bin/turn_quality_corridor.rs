use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;

use clap::Args;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use sts_oracle_runtime::ai::combat_search_v2::{
    enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices,
    CombatSearchV2Config, CombatSearchV2PotionPolicy,
};
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_replay_tools::save_combat_inputs;
use super::turn_audits::single_potion_slot_mask;

mod frontier;
pub(super) use frontier::{inspect_frontier, TurnQualityFrontierArgs};

const CHECKPOINT_SCHEMA_NAME: &str = "OracleTurnQualityCorridorCheckpointV1";
const CHECKPOINT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Args)]
pub(super) struct TurnQualityCorridorArgs {
    #[arg(long)]
    case: PathBuf,
    /// Absolute HP floor retained at every unresolved player-turn boundary.
    #[arg(long)]
    min_boundary_player_hp: i32,
    /// Absolute HP floor required after exact victory effects have resolved.
    #[arg(long)]
    min_terminal_player_hp: i32,
    /// Maximum number of complete player turns explored from the root.
    #[arg(long, default_value_t = 3)]
    max_turns: usize,
    /// Open exactly one zero-based potion identity slot for the whole corridor.
    #[arg(long)]
    potion_slot: Option<usize>,
    #[arg(long, default_value_t = 20_000)]
    max_inner_nodes_per_state: usize,
    #[arg(long, default_value_t = 8_192)]
    max_end_states_per_state: usize,
    #[arg(long, default_value_t = 8_192)]
    per_bucket_limit: usize,
    #[arg(long, default_value_t = 8_192)]
    max_frontier_states: usize,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
    /// Save the exact root-to-win action list when a quality win is found.
    #[arg(long)]
    export_witness_actions: Option<PathBuf>,
    /// Resume one exact compressed machine checkpoint produced by this command.
    #[arg(long)]
    checkpoint_in: Option<PathBuf>,
    /// Save the retained exact frontier as a compressed machine checkpoint.
    #[arg(long)]
    checkpoint_out: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct TurnQualityCorridorReportV1 {
    schema_name: &'static str,
    schema_version: u32,
    behavioral_scope: &'static str,
    case: PathBuf,
    root_exact_state_hash: String,
    initial_player_hp: i32,
    min_boundary_player_hp: i32,
    min_terminal_player_hp: i32,
    settings: TurnQualityCorridorSettingsV1,
    status: TurnQualityCorridorStatusV1,
    censoring_reasons: Vec<TurnQualityCorridorCensorReasonV1>,
    turns: Vec<TurnQualityCorridorTurnReportV1>,
    witness: Option<TurnQualityCorridorWitnessV1>,
    exported_witness_actions: Option<PathBuf>,
    resumed_from_depth: usize,
    checkpoint_in: Option<PathBuf>,
    checkpoint_out: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct TurnQualityCorridorSettingsV1 {
    max_turns: usize,
    potion_slot: Option<usize>,
    allowed_potion_slots: Option<u64>,
    max_inner_nodes_per_state: usize,
    max_end_states_per_state: usize,
    per_bucket_limit: usize,
    max_frontier_states: usize,
    max_engine_steps_per_transition: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TurnQualityCorridorStatusV1 {
    TerminalQualityWinFound,
    BoundaryFrontierExhausted,
    CensoredWithoutBoundaryFrontier,
    TurnLimitReached,
    CensoredAtTurnLimit,
    RootBelowBoundaryFloor,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum TurnQualityCorridorCensorReasonV1 {
    InnerNodeCapReached,
    EndStateSelectionDroppedPlans,
    TruncatedTransition,
    IncompleteRootActionMask,
    NonTurnBoundaryCandidate,
    FrontierStateCapReached,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TurnQualityCorridorTurnReportV1 {
    depth: usize,
    input_states: usize,
    minimum_input_player_turn: u32,
    maximum_input_player_turn: u32,
    preselection_plans: usize,
    selected_plans: usize,
    inner_nodes_expanded: usize,
    inner_nodes_generated: usize,
    exact_state_skips: usize,
    below_boundary_hp_plans: usize,
    below_terminal_hp_wins: usize,
    non_turn_boundary_candidates: usize,
    quality_wins: usize,
    quality_successors_before_dedup: usize,
    unique_quality_successors_before_cap: usize,
    duplicate_quality_successors: usize,
    retained_quality_successors: usize,
    frontier_was_capped: bool,
}

#[derive(Clone, Debug, Serialize)]
struct TurnQualityCorridorWitnessV1 {
    final_player_hp: i32,
    action_count: usize,
    terminal_exact_state_hash: String,
}

#[derive(Clone)]
struct FrontierState {
    exact_state_hash: String,
    position: CombatPosition,
    actions: Vec<ClientInput>,
}

struct CorridorAnalysis {
    status: TurnQualityCorridorStatusV1,
    censoring_reasons: BTreeSet<TurnQualityCorridorCensorReasonV1>,
    turns: Vec<TurnQualityCorridorTurnReportV1>,
    witness: Option<FrontierState>,
    frontier: Vec<FrontierState>,
    next_depth: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TurnQualityCorridorCheckpointV1 {
    schema_name: String,
    schema_version: u32,
    root_exact_state_hash: String,
    min_boundary_player_hp: i32,
    min_terminal_player_hp: i32,
    settings: TurnQualityCorridorSettingsV1,
    next_depth: usize,
    censoring_reasons: BTreeSet<TurnQualityCorridorCensorReasonV1>,
    turns: Vec<TurnQualityCorridorTurnReportV1>,
    frontier: Vec<FrontierStateCheckpointV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FrontierStateCheckpointV1 {
    exact_state_hash: String,
    position: CombatPosition,
    actions: Vec<ClientInput>,
}

pub(super) fn run(args: TurnQualityCorridorArgs) -> Result<TurnQualityCorridorReportV1, String> {
    let TurnQualityCorridorArgs {
        case,
        min_boundary_player_hp,
        min_terminal_player_hp,
        max_turns,
        potion_slot,
        max_inner_nodes_per_state,
        max_end_states_per_state,
        per_bucket_limit,
        max_frontier_states,
        max_engine_steps_per_transition,
        export_witness_actions,
        checkpoint_in,
        checkpoint_out,
    } = args;
    if max_turns == 0
        || max_inner_nodes_per_state == 0
        || max_end_states_per_state == 0
        || per_bucket_limit == 0
        || max_frontier_states == 0
        || max_engine_steps_per_transition == 0
    {
        return Err("turn-quality-corridor limits must all be positive".to_string());
    }
    let allowed_potion_slots = potion_slot.map(single_potion_slot_mask).transpose()?;
    let settings = TurnQualityCorridorSettingsV1 {
        max_turns,
        potion_slot,
        allowed_potion_slots,
        max_inner_nodes_per_state,
        max_end_states_per_state,
        per_bucket_limit,
        max_frontier_states,
        max_engine_steps_per_transition,
    };
    let position = load_combat_case(&case)?.core.position;
    let root_exact_state_hash = combat_exact_state_hash_v2(&position.engine, &position.combat);
    let initial_player_hp = position.combat.entities.player.current_hp;
    let (resumed_from_depth, analysis) = if let Some(path) = checkpoint_in.as_ref() {
        let checkpoint = load_checkpoint(path)?;
        validate_checkpoint(
            &checkpoint,
            &root_exact_state_hash,
            min_boundary_player_hp,
            min_terminal_player_hp,
            &settings,
        )?;
        let resumed_from_depth = checkpoint.next_depth;
        let frontier = checkpoint
            .frontier
            .into_iter()
            .map(|state| FrontierState {
                exact_state_hash: state.exact_state_hash,
                position: state.position,
                actions: state.actions,
            })
            .collect();
        (
            resumed_from_depth,
            analyze_frontier(
                frontier,
                resumed_from_depth,
                min_boundary_player_hp,
                min_terminal_player_hp,
                &settings,
                checkpoint.turns,
                checkpoint.censoring_reasons,
            ),
        )
    } else {
        (
            0,
            analyze(
                position,
                min_boundary_player_hp,
                min_terminal_player_hp,
                &settings,
            ),
        )
    };
    let exported_witness_actions = match (analysis.witness.as_ref(), export_witness_actions) {
        (Some(witness), Some(path)) => {
            save_combat_inputs(&path, witness.actions.clone())?;
            Some(path)
        }
        _ => None,
    };
    let witness = analysis
        .witness
        .as_ref()
        .map(|witness| TurnQualityCorridorWitnessV1 {
            final_player_hp: witness.position.combat.entities.player.current_hp,
            action_count: witness.actions.len(),
            terminal_exact_state_hash: witness.exact_state_hash.clone(),
        });
    let checkpoint_out = match (checkpoint_out, analysis.witness.is_none()) {
        (Some(path), true) if !analysis.frontier.is_empty() => {
            let checkpoint = checkpoint_from_analysis(
                &root_exact_state_hash,
                min_boundary_player_hp,
                min_terminal_player_hp,
                &settings,
                &analysis,
            );
            save_checkpoint(&path, &checkpoint)?;
            Some(path)
        }
        _ => None,
    };
    Ok(TurnQualityCorridorReportV1 {
        schema_name: "OracleTurnQualityCorridorV1",
        schema_version: 1,
        behavioral_scope: "read_only_exact_turn_enumeration_no_policy_change",
        case,
        root_exact_state_hash,
        initial_player_hp,
        min_boundary_player_hp,
        min_terminal_player_hp,
        settings,
        status: analysis.status,
        censoring_reasons: analysis.censoring_reasons.into_iter().collect(),
        turns: analysis.turns,
        witness,
        exported_witness_actions,
        resumed_from_depth,
        checkpoint_in,
        checkpoint_out,
    })
}

fn analyze(
    position: CombatPosition,
    min_boundary_player_hp: i32,
    min_terminal_player_hp: i32,
    settings: &TurnQualityCorridorSettingsV1,
) -> CorridorAnalysis {
    if position.combat.entities.player.current_hp < min_boundary_player_hp {
        return CorridorAnalysis {
            status: TurnQualityCorridorStatusV1::RootBelowBoundaryFloor,
            censoring_reasons: BTreeSet::new(),
            turns: Vec::new(),
            witness: None,
            frontier: Vec::new(),
            next_depth: 0,
        };
    }
    let root_hash = combat_exact_state_hash_v2(&position.engine, &position.combat);
    let frontier = vec![FrontierState {
        exact_state_hash: root_hash,
        position,
        actions: Vec::new(),
    }];
    analyze_frontier(
        frontier,
        0,
        min_boundary_player_hp,
        min_terminal_player_hp,
        settings,
        Vec::new(),
        BTreeSet::new(),
    )
}

fn analyze_frontier(
    mut frontier: Vec<FrontierState>,
    start_depth: usize,
    min_boundary_player_hp: i32,
    min_terminal_player_hp: i32,
    settings: &TurnQualityCorridorSettingsV1,
    mut turns: Vec<TurnQualityCorridorTurnReportV1>,
    mut censoring_reasons: BTreeSet<TurnQualityCorridorCensorReasonV1>,
) -> CorridorAnalysis {
    for depth in start_depth..settings.max_turns {
        let minimum_input_player_turn = frontier
            .iter()
            .map(|state| state.position.combat.turn.turn_count)
            .min()
            .unwrap_or_default();
        let maximum_input_player_turn = frontier
            .iter()
            .map(|state| state.position.combat.turn.turn_count)
            .max()
            .unwrap_or_default();
        let input_states = frontier.len();
        let mut next = BTreeMap::<String, FrontierState>::new();
        let mut best_witness: Option<FrontierState> = None;
        let mut turn = TurnQualityCorridorTurnReportV1 {
            depth,
            input_states,
            minimum_input_player_turn,
            maximum_input_player_turn,
            preselection_plans: 0,
            selected_plans: 0,
            inner_nodes_expanded: 0,
            inner_nodes_generated: 0,
            exact_state_skips: 0,
            below_boundary_hp_plans: 0,
            below_terminal_hp_wins: 0,
            non_turn_boundary_candidates: 0,
            quality_wins: 0,
            quality_successors_before_dedup: 0,
            unique_quality_successors_before_cap: 0,
            duplicate_quality_successors: 0,
            retained_quality_successors: 0,
            frontier_was_capped: false,
        };

        for state in frontier {
            let config = probe_config(settings);
            let enumeration =
                enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices(
                    &state.position.engine,
                    &state.position.combat,
                    &config,
                );
            let report = &enumeration.report;
            turn.preselection_plans = turn
                .preselection_plans
                .saturating_add(report.enumeration.preselection_plans);
            turn.selected_plans = turn.selected_plans.saturating_add(report.enumeration.plans);
            turn.inner_nodes_expanded = turn
                .inner_nodes_expanded
                .saturating_add(report.enumeration.nodes_expanded);
            turn.inner_nodes_generated = turn
                .inner_nodes_generated
                .saturating_add(report.enumeration.nodes_generated);
            turn.exact_state_skips = turn
                .exact_state_skips
                .saturating_add(report.enumeration.exact_state_skips);
            if report.enumeration.nodes_expanded >= settings.max_inner_nodes_per_state {
                censoring_reasons.insert(TurnQualityCorridorCensorReasonV1::InnerNodeCapReached);
            }
            if report.enumeration.plans < report.enumeration.preselection_plans {
                censoring_reasons
                    .insert(TurnQualityCorridorCensorReasonV1::EndStateSelectionDroppedPlans);
            }
            if report.enumeration.truncated_children > 0 {
                censoring_reasons.insert(TurnQualityCorridorCensorReasonV1::TruncatedTransition);
            }
            if !report.root_action_mask.complete_legal_mask {
                censoring_reasons
                    .insert(TurnQualityCorridorCensorReasonV1::IncompleteRootActionMask);
            }

            for candidate in enumeration.candidates {
                let final_hp = candidate.position.combat.entities.player.current_hp;
                let mut actions = state.actions.clone();
                actions.extend(
                    candidate
                        .report
                        .actions
                        .iter()
                        .map(|action| action.input.clone()),
                );
                let exact_state_hash = combat_exact_state_hash_v2(
                    &candidate.position.engine,
                    &candidate.position.combat,
                );
                let successor = FrontierState {
                    exact_state_hash,
                    position: candidate.position,
                    actions,
                };
                match EngineCombatStepper.terminal(&successor.position) {
                    CombatTerminal::Win if final_hp >= min_terminal_player_hp => {
                        turn.quality_wins = turn.quality_wins.saturating_add(1);
                        if best_witness.as_ref().is_none_or(|current| {
                            compare_witness(&successor, current) == Ordering::Greater
                        }) {
                            best_witness = Some(successor);
                        }
                    }
                    CombatTerminal::Win => {
                        turn.below_terminal_hp_wins = turn.below_terminal_hp_wins.saturating_add(1);
                    }
                    CombatTerminal::Unresolved if final_hp < min_boundary_player_hp => {
                        turn.below_boundary_hp_plans =
                            turn.below_boundary_hp_plans.saturating_add(1);
                    }
                    CombatTerminal::Unresolved
                        if successor.position.combat.turn.turn_count
                            > state.position.combat.turn.turn_count =>
                    {
                        turn.quality_successors_before_dedup =
                            turn.quality_successors_before_dedup.saturating_add(1);
                        match next.entry(successor.exact_state_hash.clone()) {
                            std::collections::btree_map::Entry::Vacant(entry) => {
                                entry.insert(successor);
                            }
                            std::collections::btree_map::Entry::Occupied(mut entry) => {
                                turn.duplicate_quality_successors =
                                    turn.duplicate_quality_successors.saturating_add(1);
                                if successor.actions.len() < entry.get().actions.len() {
                                    entry.insert(successor);
                                }
                            }
                        }
                    }
                    CombatTerminal::Unresolved => {
                        turn.non_turn_boundary_candidates =
                            turn.non_turn_boundary_candidates.saturating_add(1);
                        censoring_reasons
                            .insert(TurnQualityCorridorCensorReasonV1::NonTurnBoundaryCandidate);
                    }
                    CombatTerminal::Loss => {
                        turn.below_boundary_hp_plans =
                            turn.below_boundary_hp_plans.saturating_add(1);
                    }
                }
            }
        }

        turn.unique_quality_successors_before_cap = next.len();
        if let Some(witness) = best_witness {
            turns.push(turn);
            return CorridorAnalysis {
                status: TurnQualityCorridorStatusV1::TerminalQualityWinFound,
                censoring_reasons,
                turns,
                witness: Some(witness),
                frontier: Vec::new(),
                next_depth: depth.saturating_add(1),
            };
        }

        let mut next = next.into_values().collect::<Vec<_>>();
        next.sort_by(|left, right| {
            right
                .position
                .combat
                .entities
                .player
                .current_hp
                .cmp(&left.position.combat.entities.player.current_hp)
                .then_with(|| left.actions.len().cmp(&right.actions.len()))
                .then_with(|| left.exact_state_hash.cmp(&right.exact_state_hash))
        });
        if next.len() > settings.max_frontier_states {
            next.truncate(settings.max_frontier_states);
            turn.frontier_was_capped = true;
            censoring_reasons.insert(TurnQualityCorridorCensorReasonV1::FrontierStateCapReached);
        }
        turn.retained_quality_successors = next.len();
        turns.push(turn);
        if next.is_empty() {
            return CorridorAnalysis {
                status: if censoring_reasons.is_empty() {
                    TurnQualityCorridorStatusV1::BoundaryFrontierExhausted
                } else {
                    TurnQualityCorridorStatusV1::CensoredWithoutBoundaryFrontier
                },
                censoring_reasons,
                turns,
                witness: None,
                frontier: Vec::new(),
                next_depth: depth.saturating_add(1),
            };
        }
        frontier = next;
    }

    CorridorAnalysis {
        status: if censoring_reasons.is_empty() {
            TurnQualityCorridorStatusV1::TurnLimitReached
        } else {
            TurnQualityCorridorStatusV1::CensoredAtTurnLimit
        },
        censoring_reasons,
        turns,
        witness: None,
        frontier,
        next_depth: settings.max_turns,
    }
}

fn checkpoint_from_analysis(
    root_exact_state_hash: &str,
    min_boundary_player_hp: i32,
    min_terminal_player_hp: i32,
    settings: &TurnQualityCorridorSettingsV1,
    analysis: &CorridorAnalysis,
) -> TurnQualityCorridorCheckpointV1 {
    TurnQualityCorridorCheckpointV1 {
        schema_name: CHECKPOINT_SCHEMA_NAME.to_string(),
        schema_version: CHECKPOINT_SCHEMA_VERSION,
        root_exact_state_hash: root_exact_state_hash.to_string(),
        min_boundary_player_hp,
        min_terminal_player_hp,
        settings: settings.clone(),
        next_depth: analysis.next_depth,
        censoring_reasons: analysis.censoring_reasons.clone(),
        turns: analysis.turns.clone(),
        frontier: analysis
            .frontier
            .iter()
            .map(|state| FrontierStateCheckpointV1 {
                exact_state_hash: state.exact_state_hash.clone(),
                position: state.position.clone(),
                actions: state.actions.clone(),
            })
            .collect(),
    }
}

fn save_checkpoint(
    path: &PathBuf,
    checkpoint: &TurnQualityCorridorCheckpointV1,
) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "checkpoint output already exists: {}",
            path.display()
        ));
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let file = File::create(path).map_err(|error| error.to_string())?;
    let writer = BufWriter::new(file);
    let mut encoder = GzEncoder::new(writer, Compression::new(3));
    serde_json::to_writer(&mut encoder, checkpoint).map_err(|error| error.to_string())?;
    let mut writer = encoder.finish().map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

fn load_checkpoint(path: &PathBuf) -> Result<TurnQualityCorridorCheckpointV1, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let reader = BufReader::new(file);
    let decoder = GzDecoder::new(reader);
    serde_json::from_reader(decoder).map_err(|error| {
        format!(
            "invalid turn-quality-corridor checkpoint {}: {error}",
            path.display()
        )
    })
}

fn validate_checkpoint(
    checkpoint: &TurnQualityCorridorCheckpointV1,
    root_exact_state_hash: &str,
    min_boundary_player_hp: i32,
    min_terminal_player_hp: i32,
    settings: &TurnQualityCorridorSettingsV1,
) -> Result<(), String> {
    if checkpoint.schema_name != CHECKPOINT_SCHEMA_NAME
        || checkpoint.schema_version != CHECKPOINT_SCHEMA_VERSION
    {
        return Err("unsupported turn-quality-corridor checkpoint schema".to_string());
    }
    if checkpoint.root_exact_state_hash != root_exact_state_hash {
        return Err("checkpoint exact root identity does not match --case".to_string());
    }
    if checkpoint.min_boundary_player_hp != min_boundary_player_hp
        || checkpoint.min_terminal_player_hp != min_terminal_player_hp
    {
        return Err("checkpoint HP floors do not match the requested corridor".to_string());
    }
    if !checkpoint_settings_match(&checkpoint.settings, settings) {
        return Err("checkpoint enumeration settings do not match the request".to_string());
    }
    if checkpoint.next_depth > settings.max_turns {
        return Err(format!(
            "checkpoint depth {} exceeds requested --max-turns {}",
            checkpoint.next_depth, settings.max_turns
        ));
    }
    if checkpoint.frontier.is_empty() {
        return Err("checkpoint has no resumable frontier".to_string());
    }
    if checkpoint.turns.len() != checkpoint.next_depth {
        return Err("checkpoint turn history does not match its resume depth".to_string());
    }
    for (index, state) in checkpoint.frontier.iter().enumerate() {
        let observed = combat_exact_state_hash_v2(&state.position.engine, &state.position.combat);
        if observed != state.exact_state_hash {
            return Err(format!(
                "checkpoint frontier state {index} failed exact-state validation"
            ));
        }
    }
    Ok(())
}

fn checkpoint_settings_match(
    checkpoint: &TurnQualityCorridorSettingsV1,
    requested: &TurnQualityCorridorSettingsV1,
) -> bool {
    checkpoint.potion_slot == requested.potion_slot
        && checkpoint.allowed_potion_slots == requested.allowed_potion_slots
        && checkpoint.max_inner_nodes_per_state == requested.max_inner_nodes_per_state
        && checkpoint.max_end_states_per_state == requested.max_end_states_per_state
        && checkpoint.per_bucket_limit == requested.per_bucket_limit
        && checkpoint.max_frontier_states == requested.max_frontier_states
        && checkpoint.max_engine_steps_per_transition == requested.max_engine_steps_per_transition
}

fn probe_config(settings: &TurnQualityCorridorSettingsV1) -> CombatSearchV2Config {
    let mut config = CombatSearchV2Config::default();
    config.max_engine_steps_per_action = settings.max_engine_steps_per_transition;
    config.turn_plan_probe_max_inner_nodes = Some(settings.max_inner_nodes_per_state);
    config.turn_plan_probe_max_end_states = Some(settings.max_end_states_per_state);
    config.turn_plan_probe_per_bucket_limit = Some(settings.per_bucket_limit);
    config.input_label = Some("oracle_lab_turn_quality_corridor".to_string());
    if let Some(mask) = settings.allowed_potion_slots {
        config.potion_policy = CombatSearchV2PotionPolicy::All;
        config.max_potions_used = Some(1);
        config.allowed_potion_slots = Some(mask);
        config.allow_potion_discard = Some(false);
    }
    config
}

fn compare_witness(left: &FrontierState, right: &FrontierState) -> Ordering {
    left.position
        .combat
        .entities
        .player
        .current_hp
        .cmp(&right.position.combat.entities.player.current_hp)
        .then_with(|| right.actions.len().cmp(&left.actions.len()))
        .then_with(|| right.exact_state_hash.cmp(&left.exact_state_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_runtime::content::cards::CardId;
    use sts_oracle_runtime::content::monsters::EnemyId;
    use sts_oracle_runtime::runtime::combat::CombatCard;
    use sts_oracle_runtime::state::core::EngineState;
    use sts_oracle_runtime::test_support::{blank_test_combat, test_monster};

    #[test]
    fn corridor_finds_an_immediate_exact_quality_win() {
        let mut combat = blank_test_combat();
        let mut monster = test_monster(EnemyId::JawWorm);
        monster.current_hp = 1;
        monster.set_planned_move_id(1);
        combat.entities.monsters = vec![monster];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 41)];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let settings = TurnQualityCorridorSettingsV1 {
            max_turns: 1,
            potion_slot: None,
            allowed_potion_slots: None,
            max_inner_nodes_per_state: 128,
            max_end_states_per_state: 128,
            per_bucket_limit: 128,
            max_frontier_states: 128,
            max_engine_steps_per_transition: 250,
        };

        let analysis = analyze(position, 1, 1, &settings);

        assert_eq!(
            analysis.status,
            TurnQualityCorridorStatusV1::TerminalQualityWinFound
        );
        assert!(analysis.witness.is_some());
    }

    #[test]
    fn compressed_checkpoint_round_trips_exact_frontier_identity() {
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, blank_test_combat());
        let root_exact_state_hash = combat_exact_state_hash_v2(&position.engine, &position.combat);
        let settings = TurnQualityCorridorSettingsV1 {
            max_turns: 1,
            potion_slot: None,
            allowed_potion_slots: None,
            max_inner_nodes_per_state: 128,
            max_end_states_per_state: 128,
            per_bucket_limit: 128,
            max_frontier_states: 128,
            max_engine_steps_per_transition: 250,
        };
        let checkpoint = TurnQualityCorridorCheckpointV1 {
            schema_name: CHECKPOINT_SCHEMA_NAME.to_string(),
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            root_exact_state_hash: root_exact_state_hash.clone(),
            min_boundary_player_hp: 1,
            min_terminal_player_hp: 1,
            settings: settings.clone(),
            next_depth: 0,
            censoring_reasons: BTreeSet::new(),
            turns: Vec::new(),
            frontier: vec![FrontierStateCheckpointV1 {
                exact_state_hash: root_exact_state_hash.clone(),
                position,
                actions: Vec::new(),
            }],
        };
        let mut encoder = GzEncoder::new(Vec::new(), Compression::new(3));
        serde_json::to_writer(&mut encoder, &checkpoint).expect("serialize checkpoint");
        let bytes = encoder.finish().expect("finish checkpoint compression");
        let decoded: TurnQualityCorridorCheckpointV1 =
            serde_json::from_reader(GzDecoder::new(bytes.as_slice()))
                .expect("deserialize checkpoint");
        let mut resumed_settings = settings;
        resumed_settings.max_turns = 2;

        validate_checkpoint(&decoded, &root_exact_state_hash, 1, 1, &resumed_settings)
            .expect("validate exact checkpoint");
    }
}
