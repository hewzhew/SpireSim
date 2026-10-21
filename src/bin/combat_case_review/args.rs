use std::path::PathBuf;

use clap::Parser;
use sts_oracle_runtime::ai::combat_search_v2::{
    CombatSearchExpansionPluginId, CombatSearchRolloutPluginId, CombatSearchTurnPlanPluginId,
};

#[derive(Parser)]
pub(super) struct Args {
    #[arg(long)]
    pub(super) case: PathBuf,
    #[arg(long)]
    pub(super) ladder: bool,
    #[arg(
        long,
        help = "Replay one bounded complete line through ordinary and clean-only run-control adjudication"
    )]
    pub(super) adjudicate: bool,
    #[arg(long, default_value_t = 200_000)]
    pub(super) fast_nodes: usize,
    #[arg(long, default_value_t = 2_000)]
    pub(super) fast_ms: u64,
    #[arg(long, default_value_t = 800_000)]
    pub(super) slow_nodes: usize,
    #[arg(long, default_value_t = 8_000)]
    pub(super) slow_ms: u64,
    #[arg(long, default_value_t = 3)]
    pub(super) diagnostic_potion_max: u32,
    #[arg(long)]
    pub(super) write_review: Option<PathBuf>,
    #[arg(long)]
    pub(super) compact: bool,
    #[arg(long, default_value_t = 12)]
    pub(super) action_preview_limit: usize,
    #[arg(long)]
    pub(super) replay_focus: bool,
    #[arg(long)]
    pub(super) immediate_child_rollout: bool,
    #[arg(long, hide = true)]
    pub(super) lazy_child_rollout: bool,
    #[arg(long)]
    pub(super) disable_rollout: bool,
    #[arg(
        long,
        value_parser = parse_rollout_plugin,
        conflicts_with = "disable_rollout",
        help = "Override the review rollout policy: adaptive|conservative|phase-aware|turn-beam|disabled"
    )]
    pub(super) rollout_policy: Option<CombatSearchRolloutPluginId>,
    #[arg(
        long,
        value_parser = parse_turn_plan_plugin,
        help = "Override the review turn-plan policy: disabled|diagnostic|root-frontier|turn-boundary|tactical-boundary"
    )]
    pub(super) turn_plan_policy: Option<CombatSearchTurnPlanPluginId>,
    #[arg(
        long,
        value_parser = parse_expansion_plugin,
        help = "Override search topology: atomic|hierarchical-turn-boundary"
    )]
    pub(super) expansion_policy: Option<CombatSearchExpansionPluginId>,
    #[arg(long)]
    pub(super) rollout_max_actions: Option<usize>,
    #[arg(long)]
    pub(super) rollout_max_evaluations: Option<usize>,
    #[arg(long)]
    pub(super) line_lab: bool,
    #[arg(long, default_value_t = 30_000)]
    pub(super) line_lab_ms: u64,
    #[arg(long, default_value_t = 8)]
    pub(super) line_lab_cuts: usize,
    #[arg(long)]
    pub(super) quality_lanes: bool,
    #[arg(long)]
    pub(super) frozen_panel_lanes: bool,
    #[arg(long)]
    pub(super) quality_lane_total_nodes: Option<usize>,
    #[arg(long)]
    pub(super) quality_lane_total_ms: Option<u64>,
    #[arg(long)]
    pub(super) counterfactual_hp_probe: bool,
    #[arg(long, default_value = "real,half,full")]
    pub(super) counterfactual_hp_levels: String,
    #[arg(
        long,
        help = "Run an exact bounded turn-pool probe for the Awakened One opening"
    )]
    pub(super) awakened_opening_probe: bool,
    #[arg(long, default_value_t = 5_000)]
    pub(super) awakened_opening_probe_ms: u64,
    #[arg(long, default_value_t = 4)]
    pub(super) awakened_opening_probe_turns: usize,
    #[arg(
        long,
        help = "Run all-Power free-play, Feel-No-Pain-only, and optimistic-preinstalled combat counterfactuals"
    )]
    pub(super) power_setup_counterfactual: bool,
    #[arg(
        long,
        requires = "power_setup_counterfactual",
        help = "Restrict the Power setup counterfactual to the optimistic-preinstalled calibration variant"
    )]
    pub(super) power_setup_optimistic_only: bool,
}

fn parse_rollout_plugin(value: &str) -> Result<CombatSearchRolloutPluginId, String> {
    match value.to_ascii_lowercase().as_str() {
        "adaptive"
        | "adaptive-no-potion"
        | "adaptive_no_potion"
        | "enemy-mechanics-adaptive-no-potion"
        | "enemy_mechanics_adaptive_no_potion" => {
            Ok(CombatSearchRolloutPluginId::EnemyMechanicsAdaptiveNoPotion)
        }
        "conservative" | "conservative-no-potion" | "conservative_no_potion" => {
            Ok(CombatSearchRolloutPluginId::ConservativeNoPotion)
        }
        "phase-aware" | "phase_aware" | "phase-aware-no-potion" | "phase_aware_no_potion" => {
            Ok(CombatSearchRolloutPluginId::PhaseAwareNoPotion)
        }
        "turn-beam" | "turn_beam" | "turn-beam-no-potion" | "turn_beam_no_potion" => {
            Ok(CombatSearchRolloutPluginId::TurnBeamNoPotion)
        }
        "disabled" | "off" | "none" => Ok(CombatSearchRolloutPluginId::Disabled),
        _ => Err(format!(
            "invalid rollout policy '{value}', expected adaptive|conservative|phase-aware|turn-beam|disabled"
        )),
    }
}

fn parse_turn_plan_plugin(value: &str) -> Result<CombatSearchTurnPlanPluginId, String> {
    match value.to_ascii_lowercase().as_str() {
        "disabled" | "off" | "none" => Ok(CombatSearchTurnPlanPluginId::Disabled),
        "diagnostic" | "diagnostic-only" | "diagnostic_only" => {
            Ok(CombatSearchTurnPlanPluginId::DiagnosticOnly)
        }
        "root-frontier" | "root_frontier" | "root-frontier-seed" | "root_frontier_seed" => {
            Ok(CombatSearchTurnPlanPluginId::RootFrontierSeed)
        }
        "turn-boundary"
        | "turn_boundary"
        | "turn-boundary-frontier-seed"
        | "turn_boundary_frontier_seed" => {
            Ok(CombatSearchTurnPlanPluginId::TurnBoundaryFrontierSeed)
        }
        "tactical-boundary"
        | "tactical_boundary"
        | "tactical-enemy-turn-boundary-frontier-seed"
        | "tactical_enemy_turn_boundary_frontier_seed" => {
            Ok(CombatSearchTurnPlanPluginId::TacticalEnemyTurnBoundaryFrontierSeed)
        }
        _ => Err(format!(
            "invalid turn-plan policy '{value}', expected disabled|diagnostic|root-frontier|turn-boundary|tactical-boundary"
        )),
    }
}

fn parse_expansion_plugin(value: &str) -> Result<CombatSearchExpansionPluginId, String> {
    match value.to_ascii_lowercase().as_str() {
        "atomic" | "atomic-actions" | "atomic_actions" => {
            Ok(CombatSearchExpansionPluginId::AtomicActions)
        }
        "hierarchical"
        | "hierarchical-turn-boundary"
        | "hierarchical_turn_boundary"
        | "turn-boundary-macro"
        | "turn_boundary_macro" => Ok(CombatSearchExpansionPluginId::HierarchicalTurnBoundary),
        _ => Err(format!(
            "invalid expansion policy '{value}', expected atomic|hierarchical-turn-boundary"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjudicate_flag_parses() {
        let args =
            Args::try_parse_from(["combat_case_review", "--case", "case.json", "--adjudicate"])
                .expect("parse adjudicate flag");
        assert!(args.adjudicate);
    }

    #[test]
    fn rollout_policy_flag_parses_turn_beam() {
        let args = Args::try_parse_from([
            "combat_case_review",
            "--case",
            "case.json",
            "--rollout-policy",
            "turn-beam",
        ])
        .expect("parse rollout policy");

        assert_eq!(
            args.rollout_policy,
            Some(CombatSearchRolloutPluginId::TurnBeamNoPotion)
        );
    }

    #[test]
    fn turn_plan_policy_flag_parses_root_frontier() {
        let args = Args::try_parse_from([
            "combat_case_review",
            "--case",
            "case.json",
            "--turn-plan-policy",
            "root-frontier",
        ])
        .expect("parse turn-plan policy");

        assert_eq!(
            args.turn_plan_policy,
            Some(CombatSearchTurnPlanPluginId::RootFrontierSeed)
        );
    }

    #[test]
    fn expansion_policy_flag_parses_hierarchical_turn_boundary() {
        let args = Args::try_parse_from([
            "combat_case_review",
            "--case",
            "case.json",
            "--expansion-policy",
            "hierarchical-turn-boundary",
        ])
        .expect("parse expansion policy");

        assert_eq!(
            args.expansion_policy,
            Some(CombatSearchExpansionPluginId::HierarchicalTurnBoundary)
        );
    }

    #[test]
    fn awakened_opening_probe_flag_and_bounds_parse() {
        let args = Args::try_parse_from([
            "combat_case_review",
            "--case",
            "case.json",
            "--awakened-opening-probe",
            "--awakened-opening-probe-ms",
            "1234",
            "--awakened-opening-probe-turns",
            "3",
        ])
        .expect("parse Awakened One opening probe");

        assert!(args.awakened_opening_probe);
        assert_eq!(args.awakened_opening_probe_ms, 1234);
        assert_eq!(args.awakened_opening_probe_turns, 3);
    }

    #[test]
    fn power_setup_counterfactual_flag_parses() {
        let args = Args::try_parse_from([
            "combat_case_review",
            "--case",
            "case.json",
            "--power-setup-counterfactual",
        ])
        .expect("parse Power setup counterfactual");

        assert!(args.power_setup_counterfactual);
    }

    #[test]
    fn power_setup_optimistic_only_flag_parses() {
        let args = Args::try_parse_from([
            "combat_case_review",
            "--case",
            "case.json",
            "--power-setup-counterfactual",
            "--power-setup-optimistic-only",
        ])
        .expect("parse optimistic-only Power setup probe");

        assert!(args.power_setup_counterfactual);
        assert!(args.power_setup_optimistic_only);
    }
}
