use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::combat_evidence_export::{
    export_verified_combat_witness, CombatEvidenceManifestExport, VerifiedCombatWitnessExport,
};
use clap::Args;
use serde_json::json;
use sts_combat_planner::{
    CombatDecisionRoot, PolicyDiscrepancyConfig, PolicyDiscrepancyQuantum,
    PolicyDiscrepancySession, PolicyDiscrepancyTurnMacroConfig,
};
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::run_control::retained_combat_tactical_policy_v1;
use sts_oracle_runtime::sim::combat::EngineCombatStepper;

use super::combat_evidence_manifest::CombatEvidenceProducerV1;
use super::combat_policy_controls::load_action_imitation_policy;
use super::exact_turn_corridor::load_action_segments as load_combat_action_segments;
use super::{oracle_lab_runtime_identity, print_json};

#[derive(Debug, Args)]
pub(super) struct CombatCasePolicyDiscrepancyArgs {
    #[arg(long)]
    case: PathBuf,
    #[arg(long)]
    action_imitation_artifact: Option<PathBuf>,
    #[arg(long, default_value_t = 250_000)]
    max_transitions: usize,
    #[arg(long, default_value_t = 5_000)]
    wall_ms: u64,
    /// Optional production-like applied-transition grant per resume.
    /// Omit both quantum controls for one uninterrupted search allowance.
    #[arg(long, requires = "quantum_wall_ms")]
    quantum_transitions: Option<usize>,
    /// Optional production-like wall slice per resume.
    #[arg(long, requires = "quantum_transitions")]
    quantum_wall_ms: Option<u64>,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
    #[arg(long, default_value_t = 10_000)]
    uniform_exploration_ppm: u32,
    #[arg(long, default_value_t = 128)]
    max_greedy_actions_per_dive: usize,
    /// Lazily generate bounded complete-turn alternatives at player-turn
    /// boundaries. Zero keeps the pure atomic discrepancy control.
    #[arg(long, default_value_t = 0)]
    turn_macro_transitions: usize,
    #[arg(long, default_value_t = 8)]
    turn_macro_proposals_per_view: usize,
    /// Read-only exact combat states to inspect after the search.
    #[arg(long)]
    watch_case: Vec<PathBuf>,
    /// Replay one or more exact action segments and report their weighted
    /// discrepancy under the same runtime policy surface as the search.
    #[arg(long)]
    audit_actions: Vec<PathBuf>,
    #[arg(long)]
    export_witness_actions: Option<PathBuf>,
}

pub(super) fn run(args: CombatCasePolicyDiscrepancyArgs) -> Result<(), String> {
    let CombatCasePolicyDiscrepancyArgs {
        case,
        action_imitation_artifact,
        max_transitions,
        wall_ms,
        quantum_transitions,
        quantum_wall_ms,
        max_engine_steps_per_transition,
        uniform_exploration_ppm,
        max_greedy_actions_per_dive,
        turn_macro_transitions,
        turn_macro_proposals_per_view,
        watch_case,
        audit_actions,
        export_witness_actions,
    } = args;
    let command_started = Instant::now();
    let case_path = case.clone();
    let case = load_combat_case(&case)?;
    let root_position = case.core.position;
    let root = CombatDecisionRoot::new(root_position.clone())
        .map_err(|error| format!("invalid combat case root: {error:?}"))?;
    let initial_hp = root.position().combat.entities.player.current_hp;
    let watched_positions = watch_case
        .iter()
        .map(|path| load_combat_case(path).map(|case| (path.clone(), case.core.position)))
        .collect::<Result<Vec<_>, _>>()?;
    let policy = action_imitation_artifact
        .as_deref()
        .map(|path| load_action_imitation_policy(path, retained_combat_tactical_policy_v1()))
        .transpose()?
        .unwrap_or_else(retained_combat_tactical_policy_v1);
    let search_config = PolicyDiscrepancyConfig {
        max_engine_steps_per_transition,
        uniform_exploration_ppm,
        max_greedy_actions_per_dive,
        turn_macro: (turn_macro_transitions > 0).then_some(PolicyDiscrepancyTurnMacroConfig {
            max_applied_transitions: turn_macro_transitions,
            proposals_per_view: turn_macro_proposals_per_view,
            ..PolicyDiscrepancyTurnMacroConfig::default()
        }),
        max_potions_used: None,
        allow_potion_discard: true,
        allowed_potion_slots: None,
    };
    let trajectory_audit = if audit_actions.is_empty() {
        None
    } else {
        let inputs = load_combat_action_segments(&audit_actions)?;
        let audit_root = CombatDecisionRoot::new(root_position.clone())
            .map_err(|error| format!("invalid trajectory audit root: {error:?}"))?;
        let mut audit =
            PolicyDiscrepancySession::with_policy(audit_root, search_config, policy.clone());
        Some(audit.audit_trajectory(&EngineCombatStepper, &inputs)?)
    };
    let mut search = PolicyDiscrepancySession::with_policy(root, search_config, policy);
    let started = Instant::now();
    let overall_deadline = started + Duration::from_millis(wall_ms);
    let report = if let (Some(quantum_transitions), Some(quantum_wall_ms)) =
        (quantum_transitions, quantum_wall_ms)
    {
        if quantum_transitions == 0 || quantum_wall_ms == 0 {
            return Err("policy-discrepancy quantum controls must be positive".to_string());
        }
        loop {
            let used = search.counters().applied_action_transitions;
            let remaining = max_transitions.saturating_sub(used);
            let grant = remaining.min(quantum_transitions);
            let now = Instant::now();
            let slice_deadline =
                (now + Duration::from_millis(quantum_wall_ms)).min(overall_deadline);
            let report = search.advance(
                &EngineCombatStepper,
                PolicyDiscrepancyQuantum {
                    additional_applied_transitions: grant,
                    additional_engine_steps: grant.saturating_mul(max_engine_steps_per_transition),
                    deadline: Some(slice_deadline),
                },
            );
            let terminal = !matches!(
                report.status,
                sts_combat_planner::PolicyDiscrepancyStatus::Partial(_)
            );
            if terminal
                || report.after.applied_action_transitions >= max_transitions
                || Instant::now() >= overall_deadline
            {
                break report;
            }
        }
    } else {
        search.advance(
            &EngineCombatStepper,
            PolicyDiscrepancyQuantum {
                additional_applied_transitions: max_transitions,
                additional_engine_steps: max_transitions
                    .saturating_mul(max_engine_steps_per_transition),
                deadline: Some(overall_deadline),
            },
        )
    };
    let elapsed = started.elapsed();
    let watched = watched_positions
        .iter()
        .map(|(path, position)| {
            let diagnostic = search.state_diagnostic(position);
            json!({
                "case": path,
                "exact_state_hash": diagnostic.exact_state_hash,
                "discovered": diagnostic.discovered,
                "best_discrepancy": diagnostic.best_discrepancy,
                "policy_dive_services": diagnostic.policy_dive_services,
                "selected_by_turn_macro": diagnostic.selected_by_turn_macro,
                "turn_macro_scheduled": diagnostic.turn_macro_scheduled,
            })
        })
        .collect::<Vec<_>>();
    let (exported_witness_actions, exported_witness_manifest) =
        match (export_witness_actions.as_ref(), report.witness.as_ref()) {
            (Some(path), Some(witness)) => {
                let actions = witness
                    .actions
                    .iter()
                    .map(|action| action.input.clone())
                    .collect::<Vec<_>>();
                let manifest = export_verified_combat_witness(VerifiedCombatWitnessExport {
                    root_position: &root_position,
                    action_output: path,
                    actions: &actions,
                    expected_final_position: &witness.final_position,
                    max_engine_steps_per_transition,
                    manifest: Some(CombatEvidenceManifestExport {
                        producer: CombatEvidenceProducerV1::PolicyDiscrepancySearch,
                        case_path: &case_path,
                        evidence_id: "policy_discrepancy_complete_win",
                    }),
                })?
                .ok_or_else(|| {
                    "policy-discrepancy witness export unexpectedly omitted its manifest"
                        .to_string()
                })?;
                (Some(path.clone()), Some(manifest))
            }
            _ => (None, None),
        };
    print_json(&json!({
        "schema_name": "OracleCombatCasePolicyDiscrepancyV2",
        "schema_version": 2,
        "case": case_path,
        "runtime": oracle_lab_runtime_identity(),
        "mode": {
            "search": "policy_discrepancy_complete_trajectories",
            "state_guides": turn_macro_transitions > 0,
            "complete_turn_generator": turn_macro_transitions > 0,
            "lazy_turn_macro_proposals": turn_macro_transitions > 0,
            "v2_donor": false,
            "action_imitation_artifact": action_imitation_artifact,
        },
        "status": format!("{:?}", report.status),
        "timing_ms": {
            "setup": started.duration_since(command_started).as_millis(),
            "search": elapsed.as_millis(),
            "total_before_print": command_started.elapsed().as_millis(),
        },
        "budget": {
            "max_transitions": max_transitions,
            "wall_ms": wall_ms,
            "quantum_transitions": quantum_transitions,
            "quantum_wall_ms": quantum_wall_ms,
            "max_engine_steps_per_transition": max_engine_steps_per_transition,
            "max_greedy_actions_per_dive": max_greedy_actions_per_dive,
            "turn_macro_transitions": turn_macro_transitions,
            "turn_macro_proposals_per_view": turn_macro_proposals_per_view,
        },
        "work": {
            "policy_dives": report.after.policy_dives,
            "applied_action_transitions": report.after.applied_action_transitions,
            "engine_steps": report.after.engine_steps,
            "exact_states": report.after.exact_states,
            "queued_discrepancies": report.after.queued_discrepancies,
            "structured_inputs_materialized": report.after.structured_inputs_materialized,
            "duplicate_or_dominated_states": report.after.duplicate_or_dominated_states,
            "unsupported_stable_boundaries": report.after.unsupported_stable_boundaries,
            "transition_step_limit_gaps": report.after.transition_step_limit_gaps,
            "greedy_depth_limit_hits": report.after.greedy_depth_limit_hits,
            "turn_macro_generations": report.after.turn_macro_generations,
            "turn_macro_partial_generations": report.after.turn_macro_partial_generations,
            "turn_macro_deadline_retries": report.after.turn_macro_deadline_retries,
            "turn_macro_applied_transitions": report.after.turn_macro_applied_transitions,
            "turn_macro_options_generated": report.after.turn_macro_options_generated,
            "turn_macro_options_enqueued": report.after.turn_macro_options_enqueued,
        },
        "frontier": {
            "entries": report.frontier_entries,
            "best_queued_priority": report.best_queued_priority,
            "best_queued_discrepancy": report.best_queued_discrepancy,
        },
        "watched": watched,
        "trajectory_audit": trajectory_audit.as_ref().map(|audit| json!({
            "source_action_count": audit.source_action_count,
            "non_greedy_action_count": audit.non_greedy_action_count,
            "total_weighted_discrepancy": audit.total_weighted_discrepancy,
            "terminal": format!("{:?}", audit.terminal),
            "deviations": audit.deviations.iter().map(|deviation| json!({
                "action_index": deviation.action_index,
                "player_turn": deviation.player_turn,
                "demonstrated_input": deviation.demonstrated_input,
                "greedy_input": deviation.greedy_input,
                "demonstrated_probability": deviation.demonstrated_probability,
                "greedy_probability": deviation.greedy_probability,
                "discrepancy_increment": deviation.discrepancy_increment,
                "cumulative_discrepancy": deviation.cumulative_discrepancy,
                "demonstrated_was_lazy": deviation.demonstrated_was_lazy,
            })).collect::<Vec<_>>(),
        })),
        "exported_witness_actions": exported_witness_actions,
        "exported_witness_manifest": exported_witness_manifest,
        "witness": report.witness.as_ref().map(|witness| json!({
            "final_hp": witness.final_position.combat.entities.player.current_hp,
            "hp_loss": initial_hp.saturating_sub(
                witness.final_position.combat.entities.player.current_hp,
            ),
            "action_count": witness.actions.len(),
            "weighted_discrepancy": witness.negative_log_policy,
            "replay_engine_steps": witness.replay_engine_steps,
    })),
    }))
}
