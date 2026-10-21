use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Args;
use serde_json::json;
use sts_combat_planner::{
    generate_depth_beam_turn_options, CombatDecisionRoot, DepthBeamTurnBudget, DepthBeamTurnConfig,
    TurnOptionGeneratorConfig,
};
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::EngineCombatStepper;

use super::combat_policy_controls::load_action_imitation_policy;
use super::print_json;

#[derive(Debug, Args)]
pub(super) struct DepthBeamTurnAuditArgs {
    #[arg(long)]
    case: PathBuf,
    /// Lab-only typed semantic action-order artifact. The artifact may
    /// reorder legal actions but cannot remove them or claim an outcome.
    #[arg(long)]
    action_imitation_artifact: Option<PathBuf>,
    #[arg(long, default_value_t = 20_000)]
    max_applied_transitions: usize,
    #[arg(long, default_value_t = 5_000)]
    wall_ms: u64,
    #[arg(long, default_value_t = 32)]
    partial_beam_width: usize,
    #[arg(long, default_value_t = 6)]
    retained_per_view: usize,
    #[arg(long, default_value_t = 32)]
    max_atomic_depth: usize,
    #[arg(long, default_value_t = 256)]
    max_structured_members_per_family: usize,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
    #[arg(long)]
    watch_exact_state_hash: Vec<String>,
    #[arg(long, default_value_t = 64)]
    limit: usize,
}

pub(super) fn run_turn(args: DepthBeamTurnAuditArgs) -> Result<(), String> {
    let DepthBeamTurnAuditArgs {
        case,
        action_imitation_artifact,
        max_applied_transitions,
        wall_ms,
        partial_beam_width,
        retained_per_view,
        max_atomic_depth,
        max_structured_members_per_family,
        max_engine_steps_per_transition,
        watch_exact_state_hash,
        limit,
    } = args;
    let case = load_combat_case(&case)?;
    let root = CombatDecisionRoot::new(case.core.position)
        .map_err(|error| format!("invalid combat case root: {error:?}"))?;
    let policy = action_imitation_artifact
        .as_deref()
        .map(|path| load_action_imitation_policy(path, retained_combat_tactical_policy_v1()))
        .transpose()?
        .unwrap_or_else(retained_combat_tactical_policy_v1);
    let report = generate_depth_beam_turn_options(
        root,
        DepthBeamTurnConfig {
            generator: TurnOptionGeneratorConfig {
                max_engine_steps_per_transition,
                ..TurnOptionGeneratorConfig::default()
            },
            max_potion_expenditures: None,
            partial_beam_width,
            retained_per_view,
            max_atomic_depth,
            max_structured_members_per_family,
        },
        DepthBeamTurnBudget {
            max_applied_transitions,
            max_engine_steps: max_applied_transitions
                .saturating_mul(max_engine_steps_per_transition.max(1)),
            deadline: Some(Instant::now() + Duration::from_millis(wall_ms)),
        },
        policy.clone(),
        &EngineCombatStepper,
    );
    let option_json = |option: &sts_combat_planner::CompleteTurnOption| {
        json!({
            "exact_successor_hash": option.exact_successor_hash(),
            "boundary": format!("{:?}", option.boundary()),
            "action_count": option.actions().len(),
            "negative_log_policy": option.negative_log_policy(),
            "final_hp": option.exact_successor().combat.entities.player.current_hp,
            "state_guides": policy.state_guides(option.exact_successor()).into_iter().map(|guide| json!({
                "lane": guide.lane.value(),
                "components": guide.rank.components(),
            })).collect::<Vec<_>>(),
            "actions": option.actions().iter().map(|action| json!({
                "input": action.input,
                "expected_successor_hash": action.expected_successor_hash,
            })).collect::<Vec<_>>(),
        })
    };
    let watched = report
        .options
        .iter()
        .filter(|option| {
            watch_exact_state_hash
                .iter()
                .any(|hash| hash == option.exact_successor_hash())
        })
        .map(option_json)
        .collect::<Vec<_>>();
    let options = report
        .options
        .iter()
        .take(limit)
        .map(option_json)
        .collect::<Vec<_>>();
    print_json(&json!({
        "schema_name": "OracleDepthBeamTurnAuditV1",
        "schema_version": 1,
        "behavioral_scope": "read_only_no_search_seeding",
        "status": format!("{:?}", report.status),
        "config": {
            "max_applied_transitions": max_applied_transitions,
            "wall_ms": wall_ms,
            "partial_beam_width": partial_beam_width,
            "retained_per_view": retained_per_view,
            "max_atomic_depth": max_atomic_depth,
            "max_structured_members_per_family": max_structured_members_per_family,
            "max_engine_steps_per_transition": max_engine_steps_per_transition,
            "action_imitation_artifact": action_imitation_artifact,
        },
        "counters": {
            "expanded_partial_states": report.counters.expanded_partial_states,
            "applied_transitions": report.counters.applied_transitions,
            "engine_steps": report.counters.engine_steps,
            "unique_partial_states": report.counters.unique_partial_states,
            "duplicate_exact_successors": report.counters.duplicate_exact_successors,
            "completed_turn_options": report.counters.completed_turn_options,
            "retained_partial_states": report.counters.retained_partial_states,
            "pruned_partial_states": report.counters.pruned_partial_states,
            "maximum_atomic_depth": report.counters.maximum_atomic_depth,
            "truncated_structured_families": report.counters.truncated_structured_families,
        },
        "gap_count": report.gaps.len(),
        "watched": watched,
        "layers": report.layers.iter().map(|layer| json!({
            "atomic_depth": layer.atomic_depth,
            "expanded_partial_states": layer.expanded_partial_states,
            "generated_unique_partial_states": layer.generated_unique_partial_states,
            "retained_partial_states": layer.retained_partial_states,
            "retained_exact_state_hashes": layer.retained_exact_state_hashes,
            "new_completed_turn_options": layer.new_completed_turn_options,
        })).collect::<Vec<_>>(),
        "options": options,
    }))
}
