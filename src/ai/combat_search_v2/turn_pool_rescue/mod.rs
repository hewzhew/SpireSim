mod engine;
mod ranking;
mod types;

pub use types::{
    CombatTurnPoolOpeningLineReport, CombatTurnPoolOpeningReport, CombatTurnPoolRescueLineSummary,
    CombatTurnPoolRescueReport, CombatTurnPoolRescueWin,
};

use crate::sim::combat::CombatPosition;

use super::{CombatSearchV2Config, SearchTerminalLabel};

pub fn find_combat_turn_pool_rescue_win_v0(
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    budget_ms: u64,
) -> Option<CombatTurnPoolRescueWin> {
    let run = engine::run_turn_pool_nodes_v0(start, budget_ms, Some(config));
    let best = run
        .lanes
        .into_iter()
        .filter(|candidate| candidate.node.terminal == SearchTerminalLabel::Win)
        .max_by_key(|candidate| {
            ranking::turn_pool_summary_rank(&ranking::turn_pool_summary(
                candidate.lane,
                &candidate.node,
            ))
        })?;
    let summary = ranking::turn_pool_summary(best.lane, &best.node);
    Some(CombatTurnPoolRescueWin {
        summary,
        actions: best.node.actions,
        nodes_expanded: run.nodes_expanded,
        nodes_generated: run.nodes_generated,
        deadline_hit: run.deadline_hit,
    })
}

pub fn run_combat_turn_pool_rescue_report_v0(
    start: &CombatPosition,
    budget_ms: u64,
    config: Option<&CombatSearchV2Config>,
) -> CombatTurnPoolRescueReport {
    let run = engine::run_turn_pool_nodes_v0(start, budget_ms, config);
    let lanes = run
        .lanes
        .iter()
        .map(|candidate| ranking::turn_pool_summary(candidate.lane, &candidate.node))
        .collect::<Vec<_>>();
    let best = lanes
        .iter()
        .max_by_key(|line| ranking::turn_pool_summary_rank(line))
        .cloned();
    CombatTurnPoolRescueReport {
        schema: "combat_turn_pool_rescue_v0",
        lanes,
        best,
        nodes_expanded: run.nodes_expanded,
        deadline_hit: run.deadline_hit,
    }
}

pub fn run_combat_turn_pool_opening_report_v0(
    start: &CombatPosition,
    budget_ms: u64,
    max_turns: usize,
    config: Option<&CombatSearchV2Config>,
) -> CombatTurnPoolOpeningReport {
    let run = engine::run_turn_pool_opening_nodes_v0(start, budget_ms, max_turns, config);
    let nodes_expanded = run.nodes_expanded;
    let nodes_generated = run.nodes_generated;
    let deadline_hit = run.deadline_hit;
    let lanes = run
        .lanes
        .iter()
        .map(|candidate| ranking::turn_pool_opening_line_report(candidate.lane, &candidate.node))
        .collect::<Vec<_>>();
    let best_cultist_cleanup = lanes
        .iter()
        .max_by_key(|line| ranking::opening_cleanup_rank(line))
        .cloned();
    CombatTurnPoolOpeningReport {
        schema: "combat_turn_pool_opening_probe_v0",
        max_turns: max_turns.max(1),
        lanes,
        best_cultist_cleanup,
        nodes_expanded,
        nodes_generated,
        deadline_hit,
    }
}
