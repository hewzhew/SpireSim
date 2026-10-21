#[cfg(feature = "backend")]
use std::fs;
#[cfg(feature = "backend")]
use std::path::Path;
use std::path::PathBuf;

use clap::{ArgGroup, Parser};
#[cfg(feature = "backend")]
use sts_oracle_eval as sts_oracle_runtime;
#[cfg(feature = "backend")]
use sts_oracle_runtime::ai::combat_search_v2::{
    explain_combat_search_v2_initial_decision, CombatSearchV2ChildRolloutPolicy,
    CombatSearchV2PotionPolicy, CombatSearchV2RolloutPolicy, CombatSearchV2TurnPlanPolicy,
};
#[cfg(feature = "backend")]
use sts_oracle_runtime::eval::campfire_threat_panel::{
    run_campfire_threat_panel_v1, CampfireThreatPanelRunRequestV1,
};
#[cfg(feature = "backend")]
use sts_oracle_runtime::eval::combat_capture::load_combat_capture_v3;
#[cfg(feature = "backend")]
use sts_oracle_runtime::eval::combat_case_core::{load_combat_case_core_v2, CombatCaseCoreV2};
#[cfg(feature = "backend")]
use sts_oracle_runtime::eval::combat_lab_v1::{run_combat_lab_v1, CombatLabRunRequestV1};
#[cfg(feature = "backend")]
use sts_oracle_runtime::eval::combat_search_v2::{
    compare_combat_search_v2_rollout_policies, compare_combat_search_v2_turn_plan_policies,
    load_combat_root_action_prior_hints_jsonl_v0, load_combat_search_v2_benchmark,
    load_combat_search_v2_snapshot, load_combat_search_v2_start,
    load_combat_turn_plan_prior_hints_jsonl_v0, run_combat_resource_frontier_probe_v1,
    run_combat_root_proposal_priority_matrix_v1, run_combat_root_proposal_probe_v1,
    run_combat_search_guidance_lab_benchmark_v1, run_combat_search_guidance_lab_v1,
    run_combat_search_v2_benchmark, run_combat_search_v2_loaded_start,
    run_combat_turn_plan_guidance_lab_benchmark_v1, run_combat_turn_plan_guidance_lab_v1,
    CombatSearchV2LoadedStart, CombatSearchV2RunOptions,
};
#[cfg(feature = "backend")]
use sts_oracle_runtime::eval::fingerprint::{combat_state_fingerprint_v3, StateFingerprintV3};

#[derive(Parser, Debug)]
#[command(
    name = "combat_search_v2",
    about = "Combat Search V2 whole-combat runner over exact combat inputs",
    group(
        ArgGroup::new("input")
            .required(true)
            .multiple(false)
            .args([
                "start_spec",
                "combat_snapshot",
                "combat_case",
                "benchmark_spec",
                "lab_spec",
                "threat_panel_spec"
            ])
    )
)]
pub struct Args {
    #[arg(long)]
    start_spec: Option<PathBuf>,

    #[arg(long)]
    combat_snapshot: Option<PathBuf>,

    #[arg(long)]
    combat_case: Option<PathBuf>,

    #[arg(long)]
    benchmark_spec: Option<PathBuf>,

    #[arg(
        long,
        requires_all = ["lab_output", "lab_samples"],
        conflicts_with_all = [
            "max_nodes",
            "max_actions_per_line",
            "max_engine_steps_per_action",
            "wall_ms",
            "max_hp_loss",
            "potion_policy",
            "max_potions_used",
            "rollout_policy",
            "child_rollout_policy",
            "compare_rollout",
            "compare_turn_plan",
            "explain_case",
            "rollout_max_evaluations",
            "rollout_max_actions",
            "rollout_beam_width",
            "turn_plan_policy",
            "validate_only",
            "gate_only",
            "guidance_lab",
            "turn_plan_guidance_lab",
            "root_proposal_probe",
            "root_proposal_quantum_nodes",
            "resource_frontier_probe",
            "resource_frontier_quantum_nodes",
            "guidance_lab_max_cases",
            "probe_max_nodes",
            "probe_wall_ms",
            "turn_plan_probe_max_inner_nodes",
            "turn_plan_probe_max_end_states",
            "turn_plan_probe_per_bucket_limit",
            "root_action_prior_hints",
            "turn_plan_prior_hints",
            "compact",
            "output"
        ]
    )]
    lab_spec: Option<PathBuf>,

    #[arg(long, requires = "lab_spec")]
    lab_output: Option<PathBuf>,

    #[arg(long, requires = "lab_spec", value_parser = parse_nonzero_lab_samples)]
    lab_samples: Option<u64>,

    #[arg(
        long,
        requires_all = ["threat_panel_output", "threat_panel_samples"],
        conflicts_with_all = [
            "max_nodes",
            "max_actions_per_line",
            "max_engine_steps_per_action",
            "wall_ms",
            "max_hp_loss",
            "potion_policy",
            "max_potions_used",
            "rollout_policy",
            "child_rollout_policy",
            "compare_rollout",
            "compare_turn_plan",
            "explain_case",
            "rollout_max_evaluations",
            "rollout_max_actions",
            "rollout_beam_width",
            "turn_plan_policy",
            "validate_only",
            "gate_only",
            "guidance_lab",
            "turn_plan_guidance_lab",
            "root_proposal_probe",
            "root_proposal_quantum_nodes",
            "resource_frontier_probe",
            "resource_frontier_quantum_nodes",
            "guidance_lab_max_cases",
            "probe_max_nodes",
            "probe_wall_ms",
            "turn_plan_probe_max_inner_nodes",
            "turn_plan_probe_max_end_states",
            "turn_plan_probe_per_bucket_limit",
            "root_action_prior_hints",
            "turn_plan_prior_hints",
            "compact",
            "output"
        ]
    )]
    threat_panel_spec: Option<PathBuf>,

    #[arg(long, requires = "threat_panel_spec")]
    threat_panel_output: Option<PathBuf>,

    #[arg(
        long,
        requires = "threat_panel_spec",
        value_parser = parse_nonzero_lab_samples
    )]
    threat_panel_samples: Option<u64>,

    #[arg(long)]
    max_nodes: Option<usize>,

    #[arg(long)]
    max_actions_per_line: Option<usize>,

    #[arg(long)]
    max_engine_steps_per_action: Option<usize>,

    #[arg(long)]
    wall_ms: Option<u64>,

    #[arg(long)]
    max_hp_loss: Option<String>,

    #[arg(long, value_parser = validate_driver_potion_policy)]
    potion_policy: Option<String>,

    #[arg(long)]
    max_potions_used: Option<u32>,

    #[arg(long, value_parser = validate_rollout_policy)]
    rollout_policy: Option<String>,

    #[arg(long, value_parser = validate_child_rollout_policy)]
    child_rollout_policy: Option<String>,

    #[arg(long)]
    compare_rollout: Option<String>,

    #[arg(long)]
    compare_turn_plan: Option<String>,

    #[arg(long)]
    explain_case: Option<String>,

    #[arg(long)]
    rollout_max_evaluations: Option<usize>,

    #[arg(long)]
    rollout_max_actions: Option<usize>,

    #[arg(long)]
    rollout_beam_width: Option<usize>,

    #[arg(long, value_parser = validate_turn_plan_policy)]
    turn_plan_policy: Option<String>,

    #[arg(long)]
    validate_only: bool,

    #[arg(long)]
    gate_only: bool,

    #[arg(long)]
    guidance_lab: bool,

    #[arg(long)]
    turn_plan_guidance_lab: bool,

    #[arg(long)]
    root_proposal_probe: bool,

    /// Refine exact complete wins in independently serviced entry-resource
    /// lanes and merge only replay-verified nondominated outcomes.
    #[arg(long)]
    resource_frontier_probe: bool,

    #[arg(long, requires = "resource_frontier_probe")]
    resource_frontier_quantum_nodes: Option<usize>,

    #[arg(long, requires = "root_proposal_probe")]
    root_proposal_priority_matrix: bool,

    #[arg(long, requires = "root_proposal_probe")]
    root_proposal_quantum_nodes: Option<usize>,

    #[arg(long)]
    guidance_lab_max_cases: Option<usize>,

    #[arg(long)]
    probe_max_nodes: Option<usize>,

    #[arg(long)]
    probe_wall_ms: Option<u64>,

    #[arg(long)]
    turn_plan_probe_max_inner_nodes: Option<usize>,

    #[arg(long)]
    turn_plan_probe_max_end_states: Option<usize>,

    #[arg(long)]
    turn_plan_probe_per_bucket_limit: Option<usize>,

    #[arg(long)]
    root_action_prior_hints: Option<PathBuf>,

    #[arg(long)]
    turn_plan_prior_hints: Option<PathBuf>,

    #[arg(long)]
    compact: bool,

    #[arg(long)]
    output: Option<PathBuf>,
}

fn parse_nonzero_lab_samples(value: &str) -> Result<u64, String> {
    let samples = value
        .parse::<u64>()
        .map_err(|_| format!("invalid combat laboratory sample target '{value}'"))?;
    if samples == 0 {
        return Err("combat laboratory sample target must be nonzero".to_string());
    }
    Ok(samples)
}

fn validate_rollout_policy(value: &str) -> Result<String, String> {
    match value.to_ascii_lowercase().as_str() {
        "disabled"
        | "off"
        | "none"
        | "adaptive"
        | "adaptive-no-potion"
        | "adaptive_no_potion"
        | "enemy-mechanics-adaptive-no-potion"
        | "enemy_mechanics_adaptive_no_potion"
        | "conservative"
        | "conservative-no-potion"
        | "conservative_no_potion"
        | "no_potion"
        | "phase-aware"
        | "phase_aware"
        | "phase-aware-no-potion"
        | "phase_aware_no_potion"
        | "turn-beam"
        | "turn_beam"
        | "turn-beam-no-potion"
        | "turn_beam_no_potion" => Ok(value.to_owned()),
        _ => Err(format!(
            "invalid rollout policy '{value}', expected disabled|enemy_mechanics_adaptive_no_potion|conservative_no_potion|phase_aware_no_potion|turn_beam_no_potion"
        )),
    }
}

fn validate_child_rollout_policy(value: &str) -> Result<String, String> {
    match value.to_ascii_lowercase().as_str() {
        "immediate" | "eager" | "lazy" | "lazy-on-pop" | "lazy_on_pop" => Ok(value.to_owned()),
        _ => Err(format!(
            "invalid child rollout policy '{value}', expected immediate|lazy_on_pop"
        )),
    }
}

fn validate_driver_potion_policy(value: &str) -> Result<String, String> {
    match value.to_ascii_lowercase().as_str() {
        "never"
        | "all"
        | "all_legal_potion_actions"
        | "semantic"
        | "semantic-budgeted"
        | "semantic_budgeted"
        | "semantic_budgeted_potion_actions"
        | "auto"
        | "high_stakes"
        | "high-stakes"
        | "high_stakes_semantic" => Ok(value.to_owned()),
        _ => Err(format!(
            "invalid potion policy '{value}', expected never|all|semantic|auto"
        )),
    }
}

fn validate_turn_plan_policy(value: &str) -> Result<String, String> {
    match value.to_ascii_lowercase().as_str() {
        "disabled"
        | "disable"
        | "none"
        | "off"
        | "diagnostic"
        | "diagnostic-only"
        | "diagnostic_only"
        | "root-seed"
        | "root_seed"
        | "root-frontier-seed"
        | "root_frontier_seed"
        | "seed"
        | "turn-boundary-frontier-seed"
        | "turn_boundary_frontier_seed"
        | "turn-boundary-seed"
        | "turn_boundary_seed"
        | "tactical-enemy-turn-boundary-frontier-seed"
        | "tactical_enemy_turn_boundary_frontier_seed"
        | "tactical-turn-boundary-seed"
        | "tactical_turn_boundary_seed"
        | "tactical-seed"
        | "tactical_seed"
        | "support-enemy-turn-boundary-frontier-seed"
        | "support_enemy_turn_boundary_frontier_seed"
        | "support-turn-boundary-seed"
        | "support_turn_boundary_seed"
        | "support-seed"
        | "support_seed" => Ok(value.to_owned()),
        _ => Err(format!(
            "invalid turn plan policy '{value}', expected disabled|diagnostic_only|root_frontier_seed|turn_boundary_frontier_seed|tactical_enemy_turn_boundary_frontier_seed"
        )),
    }
}

/// Parses the lightweight command contract first, then builds and launches the
/// optimized worker only for a real search request. Clap exits before this
/// function reaches Cargo for `--help` and syntax errors.
pub fn launch_backend_from_env() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let argv = std::iter::once(std::ffi::OsString::from("combat_search_v2"))
        .chain(arguments.iter().cloned());
    let _ = Args::parse_from(argv);

    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("combat-search driver crate must live below the repository root")
        .to_path_buf();
    let status = std::process::Command::new("cargo")
        .current_dir(repository)
        .args([
            "run",
            "--quiet",
            "--locked",
            "--release",
            "-p",
            "sts_combat_search_driver",
            "--features",
            "backend",
            "--bin",
            "combat_search_v2_worker",
            "--",
        ])
        .args(arguments)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("combat search worker exited with {status}").into())
    }
}

#[cfg(feature = "backend")]
mod backend {
    use super::*;

    fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(spec_path) = args.threat_panel_spec.as_ref() {
            let report = run_campfire_threat_panel_v1(&CampfireThreatPanelRunRequestV1 {
                experiment_spec_path: spec_path.clone(),
                output_dir: args
                    .threat_panel_output
                    .clone()
                    .expect("clap requires --threat-panel-output"),
                requested_samples: args
                    .threat_panel_samples
                    .expect("clap requires --threat-panel-samples"),
            })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "output_dir": report.output_dir,
                    "requested_samples": report.requested_samples,
                    "cells_present": report.cells_present,
                    "cells_appended": report.cells_appended,
                    "halted_on_replay_error": report.halted_on_replay_error,
                    "completed_cells_in_summary": report.summary.completed_cells,
                    "direction_reversals": report.summary.reversals.len(),
                }))?
            );
            return Ok(());
        }
        if let Some(lab_spec_path) = args.lab_spec.as_ref() {
            let report = run_combat_lab_v1(&CombatLabRunRequestV1 {
                lab_spec_path: lab_spec_path.clone(),
                output_dir: args
                    .lab_output
                    .clone()
                    .expect("clap requires --lab-output with --lab-spec"),
                requested_samples: args
                    .lab_samples
                    .expect("clap requires --lab-samples with --lab-spec"),
            })?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        if args.gate_only && args.benchmark_spec.is_none() {
            return Err("--gate-only requires --benchmark-spec".into());
        }
        if args.gate_only && args.validate_only {
            return Err("--gate-only cannot be used with --validate-only".into());
        }
        if args.guidance_lab && args.turn_plan_guidance_lab {
            return Err("--guidance-lab cannot be combined with --turn-plan-guidance-lab".into());
        }
        if args.root_proposal_probe
            && (args.guidance_lab
                || args.turn_plan_guidance_lab
                || args.validate_only
                || args.gate_only
                || args.compare_rollout.is_some()
                || args.compare_turn_plan.is_some()
                || args.explain_case.is_some()
                || args.compact)
        {
            return Err(
                "--root-proposal-probe cannot be combined with other analysis modes".into(),
            );
        }
        if args.resource_frontier_probe
            && (args.root_proposal_probe
                || args.guidance_lab
                || args.turn_plan_guidance_lab
                || args.validate_only
                || args.gate_only
                || args.compare_rollout.is_some()
                || args.compare_turn_plan.is_some()
                || args.explain_case.is_some()
                || args.compact)
        {
            return Err(
                "--resource-frontier-probe cannot be combined with other analysis modes".into(),
            );
        }
        if args.resource_frontier_probe && args.benchmark_spec.is_some() {
            return Err(
                "--resource-frontier-probe requires one exact combat input, not a benchmark".into(),
            );
        }
        if args.resource_frontier_probe && args.max_hp_loss.is_some() {
            return Err(
                "--resource-frontier-probe owns satisfaction and cannot use --max-hp-loss".into(),
            );
        }
        if args.root_proposal_probe && args.benchmark_spec.is_some() {
            return Err(
                "--root-proposal-probe requires one exact combat input, not a benchmark".into(),
            );
        }
        if args.root_proposal_probe && args.max_hp_loss.is_some() {
            return Err(
                "--root-proposal-probe owns satisfaction and cannot use --max-hp-loss".into(),
            );
        }
        if args.compact && !args.turn_plan_guidance_lab {
            return Err("--compact currently requires --turn-plan-guidance-lab".into());
        }
        if (args.guidance_lab || args.turn_plan_guidance_lab) && args.validate_only {
            return Err("--guidance-lab modes cannot be used with --validate-only".into());
        }
        if (args.guidance_lab || args.turn_plan_guidance_lab) && args.gate_only {
            return Err("--guidance-lab modes cannot be used with --gate-only".into());
        }
        if !args.guidance_lab
            && !args.turn_plan_guidance_lab
            && (args.guidance_lab_max_cases.is_some()
                || args.probe_max_nodes.is_some()
                || args.probe_wall_ms.is_some()
                || args.turn_plan_probe_max_inner_nodes.is_some()
                || args.turn_plan_probe_max_end_states.is_some()
                || args.turn_plan_probe_per_bucket_limit.is_some())
        {
            return Err(
            "--guidance-lab-max-cases, --probe-*, and --turn-plan-probe-* require a guidance lab mode"
                .into(),
        );
        }
        if (args.guidance_lab || args.turn_plan_guidance_lab)
            && (args.compare_rollout.is_some()
                || args.compare_turn_plan.is_some()
                || args.explain_case.is_some())
        {
            return Err(
                "--guidance-lab modes cannot be combined with compare/explain modes".into(),
            );
        }
        if args.compare_rollout.is_some() && args.benchmark_spec.is_none() {
            return Err("--compare-rollout requires --benchmark-spec".into());
        }
        if args.compare_rollout.is_some() && args.gate_only {
            return Err("--compare-rollout cannot be used with --gate-only".into());
        }
        if args.compare_rollout.is_some() && args.rollout_policy.is_some() {
            return Err("--compare-rollout cannot be combined with --rollout-policy".into());
        }
        if args.compare_turn_plan.is_some() && args.benchmark_spec.is_none() {
            return Err("--compare-turn-plan requires --benchmark-spec".into());
        }
        if args.compare_turn_plan.is_some() && args.gate_only {
            return Err("--compare-turn-plan cannot be used with --gate-only".into());
        }
        if args.compare_turn_plan.is_some() && args.turn_plan_policy.is_some() {
            return Err("--compare-turn-plan cannot be combined with --turn-plan-policy".into());
        }
        if args.compare_turn_plan.is_some() && args.compare_rollout.is_some() {
            return Err("--compare-turn-plan cannot be combined with --compare-rollout".into());
        }
        if args.explain_case.is_some() && args.benchmark_spec.is_none() {
            return Err("--explain-case requires --benchmark-spec".into());
        }
        if args.explain_case.is_some() && args.compare_rollout.is_some() {
            return Err("--explain-case cannot be combined with --compare-rollout".into());
        }
        if args.explain_case.is_some() && args.compare_turn_plan.is_some() {
            return Err("--explain-case cannot be combined with --compare-turn-plan".into());
        }
        if args.explain_case.is_some() && args.gate_only {
            return Err("--explain-case cannot be combined with --gate-only".into());
        }
        if args.explain_case.is_some() && args.validate_only {
            return Err("--explain-case cannot be combined with --validate-only".into());
        }
        if args.validate_only {
            let payload = validate_input_payload(&args)?;
            write_or_print(args.output.as_ref(), &payload)?;
            return Ok(());
        }
        let satisfaction = match args.max_hp_loss.as_deref() {
            Some(value) => parse_max_hp_loss(value)?,
            None => None,
        }
        .map(sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2Satisfaction::HpLossAtMost);

        let parsed_potion_policy = args
            .potion_policy
            .as_deref()
            .map(parse_driver_potion_policy)
            .transpose()?;
        let rollout_policy = args
            .rollout_policy
            .as_deref()
            .map(parse_rollout_policy)
            .transpose()?;
        let child_rollout_policy = args
            .child_rollout_policy
            .as_deref()
            .map(parse_child_rollout_policy)
            .transpose()?;
        let turn_plan_policy = args
            .turn_plan_policy
            .as_deref()
            .map(parse_turn_plan_policy)
            .transpose()?;
        let (potion_policy, high_stakes_semantic_potions) = match parsed_potion_policy {
            Some(DriverPotionPolicy::Search(policy)) => (Some(policy), false),
            Some(DriverPotionPolicy::HighStakesAuto) => (None, true),
            None => (None, false),
        };
        let root_action_prior = args
            .root_action_prior_hints
            .as_ref()
            .map(|path| load_combat_root_action_prior_hints_jsonl_v0(path))
            .transpose()?;
        let turn_plan_prior = args
            .turn_plan_prior_hints
            .as_ref()
            .map(|path| load_combat_turn_plan_prior_hints_jsonl_v0(path))
            .transpose()?;

        let options = CombatSearchV2RunOptions {
            max_nodes: args.max_nodes,
            max_actions_per_line: args.max_actions_per_line,
            max_engine_steps_per_action: args.max_engine_steps_per_action,
            wall_ms: args.wall_ms,
            satisfaction,
            potion_policy,
            max_potions_used: args.max_potions_used,
            allowed_potion_slots: None,
            high_stakes_semantic_potions,
            rollout_policy,
            child_rollout_policy,
            rollout_max_evaluations: args.rollout_max_evaluations,
            rollout_max_actions: args.rollout_max_actions,
            rollout_beam_width: args.rollout_beam_width,
            turn_plan_policy,
            turn_plan_probe_max_inner_nodes: args.turn_plan_probe_max_inner_nodes,
            turn_plan_probe_max_end_states: args.turn_plan_probe_max_end_states,
            turn_plan_probe_per_bucket_limit: args.turn_plan_probe_per_bucket_limit,
            root_action_prior,
            turn_plan_prior,
        };
        let payload = if let Some(path) = args.benchmark_spec.as_ref() {
            let loaded = load_combat_search_v2_benchmark(path)?;
            if args.guidance_lab || args.turn_plan_guidance_lab {
                let mut child_options = options.clone();
                if args.probe_max_nodes.is_some() {
                    child_options.max_nodes = args.probe_max_nodes;
                }
                if args.probe_wall_ms.is_some() {
                    child_options.wall_ms = args.probe_wall_ms;
                }
                if args.turn_plan_guidance_lab {
                    let report = run_combat_turn_plan_guidance_lab_benchmark_v1(
                        &loaded,
                        options,
                        child_options,
                        args.guidance_lab_max_cases,
                    );
                    turn_plan_guidance_payload(&report, args.compact)?
                } else {
                    let report = run_combat_search_guidance_lab_benchmark_v1(
                        &loaded,
                        options,
                        child_options,
                        args.guidance_lab_max_cases,
                    );
                    serde_json::to_string_pretty(&report)?
                }
            } else if let Some(compare) = args.compare_rollout.as_deref() {
                let (left, right) = parse_rollout_policy_pair(compare)?;
                let run = compare_combat_search_v2_rollout_policies(&loaded, options, left, right);
                serde_json::to_string_pretty(&run)?
            } else if let Some(compare) = args.compare_turn_plan.as_deref() {
                let (left, right) = parse_turn_plan_policy_pair(compare)?;
                let run =
                    compare_combat_search_v2_turn_plan_policies(&loaded, options, left, right);
                serde_json::to_string_pretty(&run)?
            } else if let Some(case_id) = args.explain_case.as_deref() {
                let case = loaded
                    .cases
                    .iter()
                    .find(|case| case.id == case_id)
                    .ok_or_else(|| format!("benchmark case '{case_id}' not found"))?;
                let decision = explain_combat_search_v2_initial_decision(
                    &case.start.position.engine,
                    &case.start.position.combat,
                    options.to_search_config_for_position(
                        case.start.label.clone(),
                        &case.start.position,
                    ),
                );
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema_name": "CombatSearchV2BenchmarkDecisionMicroscopeReport",
                    "schema_version": 1,
                    "benchmark_name": loaded.name.clone(),
                    "case_id": case.id.clone(),
                    "input_kind": case.input.kind,
                    "input_path": case.input.path.display().to_string(),
                    "baseline": case.baseline.clone(),
                    "baseline_path": case.baseline_path.as_ref().map(|path| path.display().to_string()),
                    "decision": decision,
                    "notes": [
                        "explain-case is diagnostic-only and does not write artifacts",
                        "use it to inspect the first selected action before changing search policy"
                    ],
                }))?
            } else {
                let run = run_combat_search_v2_benchmark(&loaded, options);
                if args.gate_only {
                    serde_json::to_string_pretty(&serde_json::json!({
                        "schema_name": "CombatSearchV2BenchmarkGateOnlyReport",
                        "schema_version": 1,
                        "benchmark_name": run.benchmark_name,
                        "case_count": run.case_count,
                        "summary": run.summary,
                        "gate": run.gate,
                    }))?
                } else {
                    serde_json::to_string_pretty(&run)?
                }
            }
        } else {
            let loaded = if let Some(path) = args.combat_snapshot.as_ref() {
                load_combat_search_v2_snapshot(path)?
            } else if let Some(path) = args.combat_case.as_ref() {
                let case = load_combat_case_core(path)?;
                let position = case.position;
                let fingerprints = combat_state_fingerprint_v3(&position);
                CombatSearchV2LoadedStart {
                    label: format!("combat_case:{}", path.display()),
                    position,
                    artifact_trust_level: None,
                    fingerprints: Some(fingerprints),
                }
            } else {
                let path = args
                    .start_spec
                    .as_ref()
                    .expect("clap requires exactly one input");
                load_combat_search_v2_start(path)?
            };
            if args.resource_frontier_probe {
                let report = run_combat_resource_frontier_probe_v1(
                    &loaded,
                    options,
                    args.resource_frontier_quantum_nodes.unwrap_or(256),
                )?;
                serde_json::to_string_pretty(&report)?
            } else if args.root_proposal_probe {
                if args.root_proposal_priority_matrix {
                    let report = run_combat_root_proposal_priority_matrix_v1(
                        &loaded,
                        options,
                        args.root_proposal_quantum_nodes.unwrap_or(1),
                    )?;
                    serde_json::to_string_pretty(&report)?
                } else {
                    let report = run_combat_root_proposal_probe_v1(
                        &loaded,
                        options,
                        args.root_proposal_quantum_nodes.unwrap_or(1),
                    )?;
                    serde_json::to_string_pretty(&report)?
                }
            } else if args.guidance_lab || args.turn_plan_guidance_lab {
                let mut child_options = options.clone();
                if args.probe_max_nodes.is_some() {
                    child_options.max_nodes = args.probe_max_nodes;
                }
                if args.probe_wall_ms.is_some() {
                    child_options.wall_ms = args.probe_wall_ms;
                }
                if args.turn_plan_guidance_lab {
                    let report =
                        run_combat_turn_plan_guidance_lab_v1(&loaded, options, child_options);
                    turn_plan_guidance_payload(&report, args.compact)?
                } else {
                    let report = run_combat_search_guidance_lab_v1(&loaded, options, child_options);
                    serde_json::to_string_pretty(&report)?
                }
            } else {
                let run = run_combat_search_v2_loaded_start(&loaded, options);
                serde_json::to_string_pretty(&run.search_report)?
            }
        };
        write_or_print(args.output.as_ref(), &payload)?;
        Ok(())
    }

    fn load_combat_case_core(path: &Path) -> Result<CombatCaseCoreV2, Box<dyn std::error::Error>> {
        load_combat_case_core_v2(path).map_err(Into::into)
    }

    /// Runs the standalone compatibility binary with clap's native exit behavior.
    pub fn run_from_env() -> Result<(), Box<dyn std::error::Error>> {
        run(Args::parse())
    }

    fn parse_rollout_policy_pair(
        value: &str,
    ) -> Result<(CombatSearchV2RolloutPolicy, CombatSearchV2RolloutPolicy), String> {
        let mut parts = value.split(',').map(str::trim);
        let left = parts
            .next()
            .filter(|part| !part.is_empty())
            .ok_or_else(|| "compare-rollout requires LEFT,RIGHT".to_string())
            .and_then(parse_rollout_policy)?;
        let right = parts
            .next()
            .filter(|part| !part.is_empty())
            .ok_or_else(|| "compare-rollout requires LEFT,RIGHT".to_string())
            .and_then(parse_rollout_policy)?;
        if parts.next().is_some() {
            return Err(
                "compare-rollout requires exactly two comma-separated policies".to_string(),
            );
        }
        Ok((left, right))
    }

    fn parse_turn_plan_policy_pair(
        value: &str,
    ) -> Result<(CombatSearchV2TurnPlanPolicy, CombatSearchV2TurnPlanPolicy), String> {
        let mut parts = value.split(',').map(str::trim);
        let left = parts
            .next()
            .filter(|part| !part.is_empty())
            .ok_or_else(|| "compare-turn-plan requires LEFT,RIGHT".to_string())
            .and_then(parse_turn_plan_policy)?;
        let right = parts
            .next()
            .filter(|part| !part.is_empty())
            .ok_or_else(|| "compare-turn-plan requires LEFT,RIGHT".to_string())
            .and_then(parse_turn_plan_policy)?;
        if parts.next().is_some() {
            return Err(
                "compare-turn-plan requires exactly two comma-separated policies".to_string(),
            );
        }
        Ok((left, right))
    }

    fn parse_rollout_policy(value: &str) -> Result<CombatSearchV2RolloutPolicy, String> {
        match value.to_ascii_lowercase().as_str() {
        "disabled" | "off" | "none" => Ok(CombatSearchV2RolloutPolicy::Disabled),
        "adaptive"
        | "adaptive-no-potion"
        | "adaptive_no_potion"
        | "enemy-mechanics-adaptive-no-potion"
        | "enemy_mechanics_adaptive_no_potion" => {
            Ok(CombatSearchV2RolloutPolicy::EnemyMechanicsAdaptiveNoPotion)
        }
        "conservative" | "conservative-no-potion" | "conservative_no_potion" | "no_potion" => {
            Ok(CombatSearchV2RolloutPolicy::ConservativeNoPotion)
        }
        "phase-aware" | "phase_aware" | "phase-aware-no-potion" | "phase_aware_no_potion" => {
            Ok(CombatSearchV2RolloutPolicy::PhaseAwareNoPotion)
        }
        "turn-beam" | "turn_beam" | "turn-beam-no-potion" | "turn_beam_no_potion" => {
            Ok(CombatSearchV2RolloutPolicy::TurnBeamNoPotion)
        }
        _ => Err(format!(
            "invalid rollout policy '{value}', expected disabled|enemy_mechanics_adaptive_no_potion|conservative_no_potion|phase_aware_no_potion|turn_beam_no_potion"
        )),
    }
    }

    fn parse_child_rollout_policy(value: &str) -> Result<CombatSearchV2ChildRolloutPolicy, String> {
        match value.to_ascii_lowercase().as_str() {
            "immediate" | "eager" => Ok(CombatSearchV2ChildRolloutPolicy::Immediate),
            "lazy" | "lazy-on-pop" | "lazy_on_pop" => {
                Ok(CombatSearchV2ChildRolloutPolicy::LazyOnPop)
            }
            _ => Err(format!(
                "invalid child rollout policy '{value}', expected immediate|lazy_on_pop"
            )),
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum DriverPotionPolicy {
        Search(CombatSearchV2PotionPolicy),
        HighStakesAuto,
    }

    fn parse_driver_potion_policy(value: &str) -> Result<DriverPotionPolicy, String> {
        match value.to_ascii_lowercase().as_str() {
            "never" => Ok(DriverPotionPolicy::Search(
                CombatSearchV2PotionPolicy::Never,
            )),
            "all" | "all_legal_potion_actions" => {
                Ok(DriverPotionPolicy::Search(CombatSearchV2PotionPolicy::All))
            }
            "semantic"
            | "semantic-budgeted"
            | "semantic_budgeted"
            | "semantic_budgeted_potion_actions" => Ok(DriverPotionPolicy::Search(
                CombatSearchV2PotionPolicy::SemanticBudgeted,
            )),
            "auto" | "high_stakes" | "high-stakes" | "high_stakes_semantic" => {
                Ok(DriverPotionPolicy::HighStakesAuto)
            }
            _ => Err(format!(
                "invalid potion policy '{value}', expected never|all|semantic|auto"
            )),
        }
    }

    fn parse_max_hp_loss(value: &str) -> Result<Option<u32>, String> {
        match value.to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" => Ok(None),
            _ => value.parse::<u32>().map(Some).map_err(|_| {
                format!("invalid max hp loss '{value}', expected a non-negative integer or off")
            }),
        }
    }

    fn parse_turn_plan_policy(value: &str) -> Result<CombatSearchV2TurnPlanPolicy, String> {
        match value.to_ascii_lowercase().as_str() {
        "disabled" | "disable" | "none" | "off" => Ok(CombatSearchV2TurnPlanPolicy::Disabled),
        "diagnostic" | "diagnostic-only" | "diagnostic_only" => {
            Ok(CombatSearchV2TurnPlanPolicy::DiagnosticOnly)
        }
        "root-seed" | "root_seed" | "root-frontier-seed" | "root_frontier_seed" | "seed" => {
            Ok(CombatSearchV2TurnPlanPolicy::RootFrontierSeed)
        }
        "turn-boundary-frontier-seed"
        | "turn_boundary_frontier_seed"
        | "turn-boundary-seed"
        | "turn_boundary_seed" => Ok(CombatSearchV2TurnPlanPolicy::TurnBoundaryFrontierSeed),
        "tactical-enemy-turn-boundary-frontier-seed"
        | "tactical_enemy_turn_boundary_frontier_seed"
        | "tactical-turn-boundary-seed"
        | "tactical_turn_boundary_seed"
        | "tactical-seed"
        | "tactical_seed"
        | "support-enemy-turn-boundary-frontier-seed"
        | "support_enemy_turn_boundary_frontier_seed"
        | "support-turn-boundary-seed"
        | "support_turn_boundary_seed"
        | "support-seed"
        | "support_seed" => {
            Ok(CombatSearchV2TurnPlanPolicy::TacticalEnemyTurnBoundaryFrontierSeed)
        }
        _ => Err(format!(
            "invalid turn plan policy '{value}', expected disabled|diagnostic_only|root_frontier_seed|turn_boundary_frontier_seed|tactical_enemy_turn_boundary_frontier_seed"
        )),
    }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn threat_panel_mode_parser_accepts_complete_request() {
            let args = Args::try_parse_from([
                "combat_search_v2_driver",
                "--threat-panel-spec",
                "panel.json",
                "--threat-panel-output",
                "artifacts/runs/panel-test",
                "--threat-panel-samples",
                "3",
            ])
            .expect("complete threat panel request should parse");

            assert_eq!(args.threat_panel_spec, Some(PathBuf::from("panel.json")));
            assert_eq!(
                args.threat_panel_output,
                Some(PathBuf::from("artifacts/runs/panel-test"))
            );
            assert_eq!(args.threat_panel_samples, Some(3));
        }

        #[test]
        fn threat_panel_mode_rejects_incomplete_or_mixed_requests() {
            for argv in [
                vec!["driver", "--threat-panel-spec", "panel.json"],
                vec![
                    "driver",
                    "--threat-panel-spec",
                    "panel.json",
                    "--threat-panel-output",
                    "artifacts/runs/panel-test",
                ],
                vec![
                    "driver",
                    "--threat-panel-spec",
                    "panel.json",
                    "--threat-panel-output",
                    "artifacts/runs/panel-test",
                    "--threat-panel-samples",
                    "0",
                ],
                vec![
                    "driver",
                    "--threat-panel-spec",
                    "panel.json",
                    "--threat-panel-output",
                    "artifacts/runs/panel-test",
                    "--threat-panel-samples",
                    "1",
                    "--lab-spec",
                    "lab.json",
                    "--lab-output",
                    "artifacts/runs/lab-test",
                    "--lab-samples",
                    "1",
                ],
                vec![
                    "driver",
                    "--threat-panel-spec",
                    "panel.json",
                    "--threat-panel-output",
                    "artifacts/runs/panel-test",
                    "--threat-panel-samples",
                    "1",
                    "--wall-ms",
                    "10",
                ],
            ] {
                assert!(Args::try_parse_from(argv).is_err());
            }
        }

        #[test]
        fn lab_mode_parser_accepts_complete_request() {
            let args = Args::try_parse_from([
                "combat_search_v2_driver",
                "--lab-spec",
                "lab.json",
                "--lab-output",
                "artifacts/runs/lab-test",
                "--lab-samples",
                "8",
            ])
            .expect("complete lab request should parse");

            assert_eq!(args.lab_spec, Some(PathBuf::from("lab.json")));
            assert_eq!(
                args.lab_output,
                Some(PathBuf::from("artifacts/runs/lab-test"))
            );
            assert_eq!(args.lab_samples, Some(8));
        }

        #[test]
        fn lab_mode_parser_rejects_missing_arguments_and_zero_target() {
            for argv in [
                vec!["driver", "--lab-spec", "lab.json"],
                vec![
                    "driver",
                    "--lab-spec",
                    "lab.json",
                    "--lab-output",
                    "artifacts/runs/lab-test",
                ],
                vec!["driver", "--lab-spec", "lab.json", "--lab-samples", "8"],
                vec![
                    "driver",
                    "--lab-spec",
                    "lab.json",
                    "--lab-output",
                    "artifacts/runs/lab-test",
                    "--lab-samples",
                    "0",
                ],
            ] {
                assert!(Args::try_parse_from(argv).is_err());
            }
        }

        #[test]
        fn lab_mode_parser_rejects_every_legacy_input_or_override() {
            let conflicts = [
                ("--start-spec", Some("start.json")),
                ("--combat-snapshot", Some("snapshot.json")),
                ("--benchmark-spec", Some("benchmark.json")),
                ("--max-nodes", Some("1")),
                ("--max-actions-per-line", Some("1")),
                ("--max-engine-steps-per-action", Some("1")),
                ("--wall-ms", Some("1")),
                ("--max-hp-loss", Some("1")),
                ("--potion-policy", Some("semantic")),
                ("--max-potions-used", Some("1")),
                ("--rollout-policy", Some("enemy_mechanics_adaptive")),
                ("--child-rollout-policy", Some("lazy_on_pop")),
                ("--compare-rollout", Some("none,enemy_mechanics_adaptive")),
                ("--compare-turn-plan", Some("disabled,diagnostic_only")),
                ("--explain-case", Some("case")),
                ("--rollout-max-evaluations", Some("1")),
                ("--rollout-max-actions", Some("1")),
                ("--rollout-beam-width", Some("1")),
                ("--turn-plan-policy", Some("disabled")),
                ("--validate-only", None),
                ("--gate-only", None),
                ("--guidance-lab", None),
                ("--turn-plan-guidance-lab", None),
                ("--guidance-lab-max-cases", Some("1")),
                ("--probe-max-nodes", Some("1")),
                ("--probe-wall-ms", Some("1")),
                ("--turn-plan-probe-max-inner-nodes", Some("1")),
                ("--turn-plan-probe-max-end-states", Some("1")),
                ("--turn-plan-probe-per-bucket-limit", Some("1")),
                ("--root-action-prior-hints", Some("root.jsonl")),
                ("--turn-plan-prior-hints", Some("turn.jsonl")),
                ("--compact", None),
                ("--output", Some("report.json")),
            ];

            for (flag, value) in conflicts {
                let mut argv = vec![
                    "driver",
                    "--lab-spec",
                    "lab.json",
                    "--lab-output",
                    "artifacts/runs/lab-test",
                    "--lab-samples",
                    "8",
                    flag,
                ];
                if let Some(value) = value {
                    argv.push(value);
                }
                assert!(
                    Args::try_parse_from(argv).is_err(),
                    "lab mode accepted conflicting flag {flag}"
                );
            }
        }

        #[test]
        fn lab_mode_rejects_output_outside_artifact_root_before_writes() {
            let output = std::env::temp_dir()
                .join(format!("combat-lab-outside-root-{}", std::process::id()));
            if output.exists() {
                fs::remove_dir_all(&output).expect("remove stale outside output");
            }
            let args = Args::try_parse_from([
                std::ffi::OsString::from("driver"),
                std::ffi::OsString::from("--lab-spec"),
                std::ffi::OsString::from("missing-lab.json"),
                std::ffi::OsString::from("--lab-output"),
                output.as_os_str().to_os_string(),
                std::ffi::OsString::from("--lab-samples"),
                std::ffi::OsString::from("1"),
            ])
            .expect("syntactically valid lab request");

            let error = run(args).expect_err("outside artifact output must fail");

            assert!(
                error.to_string().contains("must be a descendant"),
                "{error}"
            );
            assert!(!output.exists());
        }

        #[test]
        fn parse_turn_plan_policy_pair_accepts_diagnostic_and_seed() {
            assert_eq!(
                parse_turn_plan_policy_pair("diagnostic_only,root_frontier_seed")
                    .expect("pair should parse"),
                (
                    CombatSearchV2TurnPlanPolicy::DiagnosticOnly,
                    CombatSearchV2TurnPlanPolicy::RootFrontierSeed
                )
            );
        }

        #[test]
        fn parse_turn_plan_policy_accepts_disabled() {
            assert_eq!(
                parse_turn_plan_policy("disabled").expect("policy should parse"),
                CombatSearchV2TurnPlanPolicy::Disabled
            );
            assert_eq!(
                parse_turn_plan_policy("off").expect("policy should parse"),
                CombatSearchV2TurnPlanPolicy::Disabled
            );
        }

        #[test]
        fn parse_turn_plan_policy_accepts_turn_boundary_seed() {
            assert_eq!(
                parse_turn_plan_policy("turn_boundary_frontier_seed").expect("policy should parse"),
                CombatSearchV2TurnPlanPolicy::TurnBoundaryFrontierSeed
            );
        }

        #[test]
        fn parse_turn_plan_policy_accepts_tactical_enemy_seed() {
            assert_eq!(
                parse_turn_plan_policy("tactical_enemy_turn_boundary_frontier_seed")
                    .expect("policy should parse"),
                CombatSearchV2TurnPlanPolicy::TacticalEnemyTurnBoundaryFrontierSeed
            );
        }

        #[test]
        fn parse_turn_plan_policy_keeps_support_enemy_alias() {
            assert_eq!(
                parse_turn_plan_policy("support_enemy_turn_boundary_frontier_seed")
                    .expect("policy should parse"),
                CombatSearchV2TurnPlanPolicy::TacticalEnemyTurnBoundaryFrontierSeed
            );
        }

        #[test]
        fn parse_rollout_policy_accepts_turn_beam_no_potion() {
            assert_eq!(
                parse_rollout_policy("turn_beam_no_potion").expect("policy should parse"),
                CombatSearchV2RolloutPolicy::TurnBeamNoPotion
            );
        }

        #[test]
        fn parse_rollout_policy_accepts_adaptive_no_potion() {
            assert_eq!(
                parse_rollout_policy("enemy_mechanics_adaptive_no_potion")
                    .expect("policy should parse"),
                CombatSearchV2RolloutPolicy::EnemyMechanicsAdaptiveNoPotion
            );
        }

        #[test]
        fn parse_driver_potion_policy_accepts_auto_high_stakes() {
            assert_eq!(
                parse_driver_potion_policy("auto").expect("policy should parse"),
                DriverPotionPolicy::HighStakesAuto
            );
        }

        #[test]
        fn parse_driver_potion_policy_keeps_explicit_semantic_policy() {
            assert_eq!(
                parse_driver_potion_policy("semantic").expect("policy should parse"),
                DriverPotionPolicy::Search(CombatSearchV2PotionPolicy::SemanticBudgeted)
            );
        }

        #[test]
        fn parse_max_hp_loss_accepts_integer_and_off() {
            assert_eq!(
                parse_max_hp_loss("8").expect("hp loss should parse"),
                Some(8)
            );
            assert_eq!(parse_max_hp_loss("off").expect("off should parse"), None);
        }

        #[test]
        fn compact_turn_plan_guidance_report_keeps_harness_verdict_and_divergence() {
            let full: serde_json::Value = serde_json::from_str(
                r#"{
            "schema_name": "CombatTurnPlanGuidanceLabBenchmarkV1Report",
            "schema_version": 6,
            "benchmark_name": "smoke",
            "summary": {
                "cases_run": 1,
                "candidate_count": 6,
                "cases_best_target_not_first_plan": 1,
                "cases_guided_prefix_better_than_baseline": 0,
                "cases_guided_prefix_tied_with_baseline": 0,
                "cases_guided_prefix_worse_than_baseline": 1,
                "cases_without_guided_prefix_baseline_comparison": 0,
                "cases_guided_prefix_better_than_budgeted_root": 1,
                "cases_guided_prefix_tied_with_budgeted_root": 0,
                "cases_guided_prefix_worse_than_budgeted_root": 0,
                "cases_without_guided_prefix_budgeted_root_comparison": 0
            },
            "cases": [{
                "id": "case-a",
                "input_kind": "start_spec",
                "input_path": "case.start.json",
                "lab": {
                    "candidates": [{
                        "plan": {
                            "plan_index": 0,
                            "bucket": "progress",
                            "stop_reason": "next_turn",
                            "first_action_key": "defend",
                            "action_keys": ["defend", "bash", "end"]
                        },
                        "target": {
                            "source": "bounded_child_search_best_complete",
                            "terminal": "win",
                            "complete_win": true,
                            "final_hp": 73,
                            "child_search_hp_loss": 7,
                            "nodes_expanded": 40
                        },
                        "tactical": {
                            "action_count": 3,
                            "cards_played": 2,
                            "potions_used": 0,
                            "attacks_played": 1,
                            "skills_played": 1,
                            "powers_played": 0,
                            "damage_done": 9,
                            "block_gained_proxy": 5,
                            "player_hp_lost": 0,
                            "energy_spent_proxy": 3,
                            "draw_delta": 0,
                            "discard_delta": 2,
                            "exhaust_delta": 0
                        }
                    }],
                    "summary": {
                        "candidate_count": 6,
                        "best_target_plan_index": 4,
                        "first_plan_rank_by_target": 6,
                        "baseline_vs_best_guided_prefix": {
                            "verdict": "guided_worse",
                            "verdict_basis": "turns",
                            "guided_prefix_selection_basis": "root_composed_objective",
                            "reference_turn_prefix_candidate_coverage": {
                                "candidate_count": 1,
                                "reference_prefix_action_count": 3,
                                "reference_prefix_action_keys": ["defend", "bash", "end"],
                                "exact_match_plan_index": null,
                                "longest_prefix_match_plan_index": 0,
                                "longest_prefix_match_action_count": 1
                            },
                            "delta_guided_minus_baseline": {
                                "final_hp_delta": 0,
                                "hp_loss_delta": 0,
                                "turn_delta": 1
                            },
                            "action_sequence_alignment": {
                                "common_prefix_action_count": 1,
                                "baseline_next_action_key": "bash",
                                "guided_next_action_key": "strike",
                                "first_divergence_kind": "diverged"
                            },
                            "baseline": {
                                "terminal": "win",
                                "complete_win": true,
                                "final_hp": 73,
                                "hp_loss": 7,
                                "turns": 5,
                                "potions_used": 0,
                                "cards_played": 16,
                                "action_count": 21,
                                "first_action_key": "defend",
                                "nodes_expanded": 170
                            },
                            "best_guided_prefix": {
                                "plan_index": 4,
                                "first_action_key": "defend",
                                "terminal": "win",
                                "complete_win": true,
                                "final_hp": 73,
                                "hp_loss": 7,
                                "turns": 6,
                                "potions_used": 0,
                                "cards_played": 15,
                                "action_count": 20,
                                "nodes_expanded": 60,
                                "tactical": {
                                    "action_count": 4,
                                    "cards_played": 3,
                                    "potions_used": 0,
                                    "attacks_played": 2,
                                    "skills_played": 1,
                                    "powers_played": 0,
                                    "damage_done": 12,
                                    "block_gained_proxy": 5,
                                    "player_hp_lost": 6,
                                    "energy_spent_proxy": 3,
                                    "draw_delta": -5,
                                    "discard_delta": 5,
                                    "exhaust_delta": 0
                                }
                            }
                        },
                        "budgeted_root_vs_best_guided_prefix": {
                            "verdict": "guided_better",
                            "verdict_basis": "nodes_expanded",
                            "guided_prefix_selection_basis": "root_composed_objective",
                            "reference_turn_prefix_candidate_coverage": {
                                "candidate_count": 1,
                                "reference_prefix_action_count": 3,
                                "reference_prefix_action_keys": ["defend", "bash", "end"],
                                "exact_match_plan_index": null,
                                "longest_prefix_match_plan_index": 0,
                                "longest_prefix_match_action_count": 1
                            },
                            "delta_guided_minus_baseline": {
                                "final_hp_delta": 0,
                                "hp_loss_delta": 0,
                                "turn_delta": 0,
                                "nodes_expanded_delta": -40
                            },
                            "action_sequence_alignment": {
                                "common_prefix_action_count": 1,
                                "baseline_next_action_key": "bash",
                                "guided_next_action_key": "strike",
                                "first_divergence_kind": "diverged"
                            },
                            "baseline": {
                                "terminal": "win",
                                "complete_win": true,
                                "final_hp": 73,
                                "hp_loss": 7,
                                "turns": 6,
                                "potions_used": 0,
                                "cards_played": 15,
                                "action_count": 20,
                                "first_action_key": "defend",
                                "nodes_expanded": 100
                            },
                            "best_guided_prefix": {
                                "plan_index": 4,
                                "first_action_key": "defend",
                                "terminal": "win",
                                "complete_win": true,
                                "final_hp": 73,
                                "hp_loss": 7,
                                "turns": 6,
                                "potions_used": 0,
                                "cards_played": 15,
                                "action_count": 20,
                                "nodes_expanded": 60,
                                "tactical": {
                                    "action_count": 4,
                                    "cards_played": 3,
                                    "potions_used": 0,
                                    "attacks_played": 2,
                                    "skills_played": 1,
                                    "powers_played": 0,
                                    "damage_done": 12,
                                    "block_gained_proxy": 5,
                                    "player_hp_lost": 6,
                                    "energy_spent_proxy": 3,
                                    "draw_delta": -5,
                                    "discard_delta": 5,
                                    "exhaust_delta": 0
                                }
                            }
                        }
                    }
                }
            }]
        }"#,
            )
            .expect("test json should parse");

            let compact = compact_turn_plan_guidance_report(&full);

            assert_eq!(
                compact["schema_name"],
                "CombatTurnPlanGuidanceHarnessCompactReport"
            );
            assert_eq!(
                compact["summary"]["cases_guided_prefix_worse_than_baseline"],
                1
            );
            assert_eq!(
                compact["summary"]["cases_guided_prefix_better_than_budgeted_root"],
                1
            );
            let comparison = &compact["cases"][0]["summary"]["baseline_vs_best_guided_prefix"];
            assert_eq!(comparison["verdict"], "guided_worse");
            assert_eq!(comparison["verdict_basis"], "turns");
            assert_eq!(
                comparison["guided_prefix_selection_basis"],
                "root_composed_objective"
            );
            assert_eq!(
                comparison["reference_turn_prefix_candidate_coverage"]["exact_match_plan_index"],
                serde_json::Value::Null
            );
            assert_eq!(
                comparison["reference_turn_prefix_candidate_coverage"]
                    ["longest_prefix_match_action_count"],
                1
            );
            assert_eq!(comparison["alignment"]["baseline_next_action_key"], "bash");
            assert_eq!(comparison["alignment"]["guided_next_action_key"], "strike");
            assert_eq!(
                comparison["best_guided_prefix"]["tactical"]["block_gained_proxy"],
                5
            );
            assert_eq!(
                compact["cases"][0]["candidate_prefixes"][0]["action_keys_preview"][1],
                "bash"
            );
            let budgeted = &compact["cases"][0]["summary"]["budgeted_root_vs_best_guided_prefix"];
            assert_eq!(budgeted["verdict"], "guided_better");
            assert_eq!(budgeted["verdict_basis"], "nodes_expanded");
        }
    }

    fn validate_input_payload(args: &Args) -> Result<String, Box<dyn std::error::Error>> {
        let payload = if let Some(path) = args.combat_snapshot.as_ref() {
            let capture = load_combat_capture_v3(path)?;
            serde_json::json!({
                "schema_name": "CombatSearchV2InputValidationReport",
                "schema_version": 2,
                "status": "valid",
                "input_kind": "combat_snapshot",
                "input_path": path.display().to_string(),
                "trust_level": capture.trust_level,
                "provenance": capture.provenance,
                "fingerprints": compact_fingerprint_report(&capture.fingerprints),
                "legal_action_surface": {
                    "atomic_actions": capture.legal_action_surface.atomic_action_count,
                    "action_families": capture.legal_action_surface.action_family_count,
                    "legal_input_language_digest": capture.legal_action_surface.legal_input_language_digest,
                    "enumeration_domain_digest": capture.legal_action_surface.enumeration_domain_digest,
                },
                "summary": capture.summary,
            })
        } else if let Some(path) = args.combat_case.as_ref() {
            let case = load_combat_case_core(path)?;
            let fingerprints = combat_state_fingerprint_v3(&case.position);
            serde_json::json!({
                "schema_name": "CombatSearchV2InputValidationReport",
                "schema_version": 2,
                "status": "valid",
                "input_kind": "combat_case",
                "input_path": path.display().to_string(),
                "source": case.source,
                "gap": case.gap,
                "fingerprints": compact_fingerprint_report(&fingerprints),
                "position": {
                    "engine": format!("{:?}", case.position.engine),
                    "hp": case.position.combat.entities.player.current_hp,
                    "turn": case.position.combat.turn.turn_count,
                    "enemy_count": case.position.combat.entities.monsters.len(),
                },
            })
        } else if let Some(path) = args.benchmark_spec.as_ref() {
            let benchmark = load_combat_search_v2_benchmark(path)?;
            let cases = benchmark
            .cases
            .iter()
            .map(|case| {
                serde_json::json!({
                    "id": case.id,
                    "input_kind": case.input.kind,
                    "input_path": case.input.path.display().to_string(),
                    "trust_level": case.start.artifact_trust_level,
                    "fingerprints": case.start.fingerprints.as_ref().map(compact_fingerprint_report),
                    "expected_fingerprints": case.expected_fingerprints.clone(),
                })
            })
            .collect::<Vec<_>>();
            serde_json::json!({
                "schema_name": "CombatSearchV2InputValidationReport",
                "schema_version": 2,
                "status": "valid",
                "input_kind": "benchmark_spec",
                "input_path": path.display().to_string(),
                "benchmark_name": benchmark.name,
                "min_trust_level": benchmark.min_trust_level,
                "case_count": cases.len(),
                "cases": cases,
            })
        } else {
            let path = args
                .start_spec
                .as_ref()
                .expect("clap requires exactly one input");
            let start = load_combat_search_v2_start(path)?;
            serde_json::json!({
                "schema_name": "CombatSearchV2InputValidationReport",
                "schema_version": 2,
                "status": "valid",
                "input_kind": "start_spec",
                "input_path": path.display().to_string(),
                "label": start.label,
                "artifact_trust_level": start.artifact_trust_level,
                "fingerprints": start.fingerprints.as_ref().map(compact_fingerprint_report),
            })
        };
        Ok(serde_json::to_string_pretty(&payload)?)
    }

    fn turn_plan_guidance_payload<T: serde::Serialize>(
        report: &T,
        compact: bool,
    ) -> Result<String, serde_json::Error> {
        if compact {
            let value = serde_json::to_value(report)?;
            serde_json::to_string_pretty(&compact_turn_plan_guidance_report(&value))
        } else {
            serde_json::to_string_pretty(report)
        }
    }

    fn compact_turn_plan_guidance_report(report: &serde_json::Value) -> serde_json::Value {
        if let Some(cases) = report.get("cases").and_then(|value| value.as_array()) {
            let compact_cases = cases
                .iter()
                .map(compact_turn_plan_guidance_case)
                .collect::<Vec<_>>();
            serde_json::json!({
                "schema_name": "CombatTurnPlanGuidanceHarnessCompactReport",
                "schema_version": 6,
                "source_schema_name": report.get("schema_name"),
                "source_schema_version": report.get("schema_version"),
                "benchmark_name": report.get("benchmark_name"),
                "summary": report.get("summary").map(compact_turn_plan_guidance_summary),
                "cases": compact_cases,
            })
        } else {
            serde_json::json!({
                "schema_name": "CombatTurnPlanGuidanceHarnessCompactReport",
                "schema_version": 6,
                "source_schema_name": report.get("schema_name"),
                "source_schema_version": report.get("schema_version"),
                "summary": report.get("summary").map(compact_turn_plan_guidance_lab_summary),
                "candidate_prefixes": report
                    .get("candidates")
                    .map(compact_turn_plan_guidance_candidates),
            })
        }
    }

    fn compact_turn_plan_guidance_case(case: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "id": case.get("id"),
            "input_kind": case.get("input_kind"),
            "input_path": case.get("input_path"),
            "summary": case
                .get("lab")
                .and_then(|lab| lab.get("summary"))
                .map(compact_turn_plan_guidance_lab_summary),
            "candidate_prefixes": case
                .get("lab")
                .and_then(|lab| lab.get("candidates"))
                .map(compact_turn_plan_guidance_candidates),
        })
    }

    fn compact_turn_plan_guidance_summary(summary: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "cases_run": summary.get("cases_run"),
            "candidate_count": summary.get("candidate_count"),
            "cases_best_target_not_first_plan": summary.get("cases_best_target_not_first_plan"),
            "cases_guided_prefix_better_than_baseline": summary.get("cases_guided_prefix_better_than_baseline"),
            "cases_guided_prefix_tied_with_baseline": summary.get("cases_guided_prefix_tied_with_baseline"),
            "cases_guided_prefix_worse_than_baseline": summary.get("cases_guided_prefix_worse_than_baseline"),
            "cases_without_guided_prefix_baseline_comparison": summary.get("cases_without_guided_prefix_baseline_comparison"),
            "cases_guided_prefix_better_than_budgeted_root": summary.get("cases_guided_prefix_better_than_budgeted_root"),
            "cases_guided_prefix_tied_with_budgeted_root": summary.get("cases_guided_prefix_tied_with_budgeted_root"),
            "cases_guided_prefix_worse_than_budgeted_root": summary.get("cases_guided_prefix_worse_than_budgeted_root"),
            "cases_without_guided_prefix_budgeted_root_comparison": summary.get("cases_without_guided_prefix_budgeted_root_comparison"),
        })
    }

    fn compact_turn_plan_guidance_lab_summary(summary: &serde_json::Value) -> serde_json::Value {
        let comparison = summary.get("baseline_vs_best_guided_prefix");
        let budgeted_comparison = summary.get("budgeted_root_vs_best_guided_prefix");
        serde_json::json!({
            "candidate_count": summary.get("candidate_count"),
            "best_target_plan_index": summary.get("best_target_plan_index"),
            "first_plan_rank_by_target": summary.get("first_plan_rank_by_target"),
            "baseline_vs_best_guided_prefix": comparison.map(compact_turn_plan_guidance_comparison),
            "budgeted_root_vs_best_guided_prefix": budgeted_comparison.map(compact_turn_plan_guidance_comparison),
        })
    }

    fn compact_turn_plan_guidance_candidates(candidates: &serde_json::Value) -> serde_json::Value {
        let compact = candidates
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .take(16)
                    .map(compact_turn_plan_guidance_candidate)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        serde_json::json!(compact)
    }

    fn compact_turn_plan_guidance_candidate(candidate: &serde_json::Value) -> serde_json::Value {
        let plan = candidate.get("plan");
        let target = candidate.get("target");
        serde_json::json!({
            "plan_index": plan.and_then(|plan| plan.get("plan_index")),
            "bucket": plan.and_then(|plan| plan.get("bucket")),
            "stop_reason": plan.and_then(|plan| plan.get("stop_reason")),
            "first_action_key": plan.and_then(|plan| plan.get("first_action_key")),
            "action_keys_preview": plan
                .and_then(|plan| plan.get("action_keys"))
                .and_then(|keys| keys.as_array())
                .map(|keys| keys.iter().take(8).cloned().collect::<Vec<_>>()),
            "target": target.map(compact_guidance_target),
            "tactical": candidate.get("tactical").map(compact_guidance_tactical_trace),
        })
    }

    fn compact_guidance_target(target: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "source": target.get("source"),
            "terminal": target.get("terminal"),
            "complete_win": target.get("complete_win"),
            "final_hp": target.get("final_hp"),
            "child_search_hp_loss": target.get("child_search_hp_loss"),
            "nodes_expanded": target.get("nodes_expanded"),
        })
    }

    fn compact_turn_plan_guidance_comparison(comparison: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "verdict": comparison.get("verdict"),
            "verdict_basis": comparison.get("verdict_basis"),
            "guided_prefix_selection_basis": comparison.get("guided_prefix_selection_basis"),
            "reference_turn_prefix_candidate_coverage": comparison.get("reference_turn_prefix_candidate_coverage"),
            "delta_guided_minus_baseline": comparison.get("delta_guided_minus_baseline"),
            "alignment": comparison.get("action_sequence_alignment"),
            "baseline": comparison.get("baseline").map(compact_guidance_search_snapshot),
            "best_guided_prefix": comparison
                .get("best_guided_prefix")
                .map(compact_guidance_plan_snapshot),
        })
    }

    fn compact_guidance_search_snapshot(snapshot: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "terminal": snapshot.get("terminal"),
            "complete_win": snapshot.get("complete_win"),
            "final_hp": snapshot.get("final_hp"),
            "hp_loss": snapshot.get("hp_loss"),
            "turns": snapshot.get("turns"),
            "potions_used": snapshot.get("potions_used"),
            "cards_played": snapshot.get("cards_played"),
            "action_count": snapshot.get("action_count"),
            "first_action_key": snapshot.get("first_action_key"),
            "nodes_expanded": snapshot.get("nodes_expanded"),
        })
    }

    fn compact_guidance_plan_snapshot(snapshot: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "plan_index": snapshot.get("plan_index"),
            "first_action_key": snapshot.get("first_action_key"),
            "terminal": snapshot.get("terminal"),
            "complete_win": snapshot.get("complete_win"),
            "final_hp": snapshot.get("final_hp"),
            "hp_loss": snapshot.get("hp_loss"),
            "turns": snapshot.get("turns"),
            "potions_used": snapshot.get("potions_used"),
            "cards_played": snapshot.get("cards_played"),
            "action_count": snapshot.get("action_count"),
            "nodes_expanded": snapshot.get("nodes_expanded"),
            "tactical": snapshot.get("tactical").map(compact_guidance_tactical_trace),
        })
    }

    fn compact_guidance_tactical_trace(tactical: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "action_count": tactical.get("action_count"),
            "cards_played": tactical.get("cards_played"),
            "potions_used": tactical.get("potions_used"),
            "attacks_played": tactical.get("attacks_played"),
            "skills_played": tactical.get("skills_played"),
            "powers_played": tactical.get("powers_played"),
            "damage_done": tactical.get("damage_done"),
            "block_gained_proxy": tactical.get("block_gained_proxy"),
            "visible_attack_mitigation_hint": tactical.get("visible_attack_mitigation_hint"),
            "enemy_debuff_pressure_hint": tactical.get("enemy_debuff_pressure_hint"),
            "player_strength_gain": tactical.get("player_strength_gain"),
            "player_temporary_strength_gain": tactical.get("player_temporary_strength_gain"),
            "player_hp_lost": tactical.get("player_hp_lost"),
            "energy_spent_proxy": tactical.get("energy_spent_proxy"),
            "draw_delta": tactical.get("draw_delta"),
            "discard_delta": tactical.get("discard_delta"),
            "exhaust_delta": tactical.get("exhaust_delta"),
        })
    }

    fn compact_fingerprint_report(fingerprints: &StateFingerprintV3) -> serde_json::Value {
        serde_json::json!({
            "boundary": fingerprints.boundary,
            "public_observation_hash": fingerprints.public_observation_hash,
            "legal_input_language_hash": fingerprints.legal_input_language_hash,
            "action_enumeration_domain_hash": fingerprints.action_enumeration_domain_hash,
            "exact_state_hash": fingerprints.exact_state_hash,
            "stable_outcome_hash": fingerprints.stable_outcome_hash,
            "rng_boundary": {
                "status": fingerprints.rng_boundary.status,
                "stream_count": fingerprints.rng_boundary.stream_count,
                "digest": fingerprints.rng_boundary.digest,
            }
        })
    }

    fn write_or_print(path: Option<&PathBuf>, payload: &str) -> Result<(), std::io::Error> {
        if let Some(path) = path {
            ensure_parent_dir(path)?;
            fs::write(path, payload)
        } else {
            println!("{payload}");
            Ok(())
        }
    }

    fn ensure_parent_dir(path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

#[cfg(feature = "backend")]
pub use backend::run_from_env;

#[cfg(test)]
mod frontend_tests {
    use super::*;

    #[test]
    fn help_does_not_require_a_backend_or_input_spec() {
        let error = Args::try_parse_from(["combat_search_v2", "--help"])
            .expect_err("clap represents help as an early successful exit");
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn complete_request_parses_without_the_backend_feature() {
        let args = Args::try_parse_from([
            "combat_search_v2",
            "--combat-case",
            "fight.case.json",
            "--max-nodes",
            "4096",
            "--rollout-policy",
            "adaptive",
        ])
        .expect("frontend should validate the complete command shape");
        assert_eq!(args.combat_case, Some(PathBuf::from("fight.case.json")));
        assert_eq!(args.max_nodes, Some(4096));
        assert_eq!(args.rollout_policy.as_deref(), Some("adaptive"));
    }

    #[test]
    fn invalid_policy_is_rejected_before_the_backend_feature() {
        let error = Args::try_parse_from([
            "combat_search_v2",
            "--combat-case",
            "fight.case.json",
            "--rollout-policy",
            "mystery",
        ])
        .expect_err("frontend must own CLI value validation");
        assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
    }
}
