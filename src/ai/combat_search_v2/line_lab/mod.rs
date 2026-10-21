use std::time::Duration;

mod cuts;
mod repair;
mod replay;
mod types;

pub use types::{CombatLineLabReport, CombatLineLabTurnPoolLineSummary};

use crate::sim::combat::CombatPosition;

use super::{
    run_combat_search_v2, turn_pool_rescue::run_combat_turn_pool_rescue_report_v0,
    CombatSearchV2Config, CombatSearchV2TrajectoryReport,
};

pub fn run_combat_line_lab_v0(
    start: &CombatPosition,
    mut config: CombatSearchV2Config,
    total_budget_ms: u64,
    max_cuts: usize,
) -> CombatLineLabReport {
    let baseline_ms = (total_budget_ms / 2).max(500);
    let repair_budget_ms = total_budget_ms.saturating_sub(baseline_ms).max(500);
    config.wall_time = Some(Duration::from_millis(baseline_ms));
    let baseline_report = run_combat_search_v2(&start.engine, &start.combat, config.clone());
    let Some(parent) = baseline_report.best_complete_trajectory.as_ref() else {
        return CombatLineLabReport {
            schema: "combat_line_lab_v1",
            baseline: None,
            cuts: Vec::new(),
            best_repair: None,
            turn_pool: Some(run_combat_turn_pool_rescue_report_v0(
                start,
                repair_budget_ms,
                Some(&config),
            )),
        };
    };
    run_combat_line_lab_from_parent_v0(start, parent, config, repair_budget_ms, max_cuts)
}

pub fn run_combat_line_lab_from_parent_v0(
    start: &CombatPosition,
    parent: &CombatSearchV2TrajectoryReport,
    config: CombatSearchV2Config,
    repair_budget_ms: u64,
    max_cuts: usize,
) -> CombatLineLabReport {
    let suffix_budget_ms = (repair_budget_ms / 2).max(500);
    let turn_pool_budget_ms = repair_budget_ms.saturating_sub(suffix_budget_ms).max(500);
    let baseline = replay::line_summary("baseline_best_complete", parent);
    let cut_points = cuts::collect_cut_points(start, parent, max_cuts);
    let per_cut_ms = cuts::per_cut_budget_ms(suffix_budget_ms, cut_points.len());
    let cuts = cut_points
        .into_iter()
        .map(|cut| repair::repair_cut(start, parent, cut, &config, per_cut_ms))
        .collect::<Vec<_>>();
    let best_repair = cuts
        .iter()
        .filter(|cut| cut.failed_reason.is_none())
        .max_by_key(|cut| repair::repair_rank(cut))
        .cloned();

    CombatLineLabReport {
        schema: "combat_line_lab_v1",
        baseline: Some(baseline),
        cuts,
        best_repair,
        turn_pool: Some(run_combat_turn_pool_rescue_report_v0(
            start,
            turn_pool_budget_ms,
            Some(&config),
        )),
    }
}
