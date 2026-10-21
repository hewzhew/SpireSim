use std::time::{Duration, Instant};

use crate::ai::combat_search_v2::CombatSearchV2Config;
use crate::sim::combat::{CombatPosition, EngineCombatStepper};

use super::combat_candidate_line::{CombatCandidateLine, CombatCandidateLineSource};
use super::combat_complete_line_repair::{repair_line_if_useful, LineRepairBudget};
use super::combat_complete_line_search::{
    line_search_from, reindex_actions, LineSearchConfig, LineSearchSeed, LINE_BEAM,
};

pub(super) struct CompleteLineSolverOutcome {
    pub line: CombatCandidateLine,
    base_hp_loss: i32,
    base_action_count: usize,
    final_hp_loss: i32,
    final_action_count: usize,
    repair_hp_loss_saved: i32,
    repair_action_count_delta: isize,
    base_node_budget: usize,
    base_ms_budget: u64,
    repair_node_budget_per_cut: usize,
    repair_ms_budget_per_cut: u64,
    repair_cut_budget: usize,
    base_stop_reason: &'static str,
    last_repair_stop_reason: Option<&'static str>,
    repair_attempts: usize,
    repair_wins: usize,
    repair_improvements: usize,
    base_nodes_expanded: usize,
    base_nodes_generated: usize,
    repair_nodes_expanded: usize,
    repair_nodes_generated: usize,
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub elapsed_ms: u128,
}

impl CompleteLineSolverOutcome {
    pub(super) fn transition_summary(&self) -> String {
        format!(
            "complete_line_solver actions={}/{} delta={} hp_loss={}/{} saved={} budget=base:{}/{}ms repair:{}x{}/{}ms stops={}/{} nodes={} generated={} base_nodes={}/{} repair_nodes={}/{} repair={}/{}/{} elapsed_ms={}",
            self.final_action_count,
            self.base_action_count,
            self.repair_action_count_delta,
            self.final_hp_loss,
            self.base_hp_loss,
            self.repair_hp_loss_saved,
            self.base_node_budget,
            self.base_ms_budget,
            self.repair_cut_budget,
            self.repair_node_budget_per_cut,
            self.repair_ms_budget_per_cut,
            self.base_stop_reason,
            self.last_repair_stop_reason.unwrap_or("none"),
            self.nodes_expanded,
            self.nodes_generated,
            self.base_nodes_expanded,
            self.base_nodes_generated,
            self.repair_nodes_expanded,
            self.repair_nodes_generated,
            self.repair_attempts,
            self.repair_wins,
            self.repair_improvements,
            self.elapsed_ms
        )
    }
}

#[derive(Clone, Copy)]
struct CompleteLineSolverBudget {
    base_nodes: usize,
    base_ms: u64,
    repair: LineRepairBudget,
}

impl CompleteLineSolverBudget {
    fn from_search_config(config: &CombatSearchV2Config) -> Self {
        Self {
            base_nodes: config.max_nodes,
            base_ms: config
                .wall_time
                .unwrap_or_else(|| Duration::from_millis(2_000))
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
            repair: LineRepairBudget::default_budget(),
        }
    }

    fn base_search(self, max_actions: usize) -> LineSearchConfig {
        LineSearchConfig {
            nodes: self.base_nodes,
            ms: self.base_ms,
            beam: LINE_BEAM,
            max_actions,
        }
    }
}

pub(super) fn try_solve_complete_line(
    start: &CombatPosition,
    config: &CombatSearchV2Config,
) -> Option<CompleteLineSolverOutcome> {
    let started = Instant::now();
    let stepper = EngineCombatStepper;
    let initial_hp = start.combat.entities.player.current_hp;
    let budget = CompleteLineSolverBudget::from_search_config(config);
    let run = line_search_from(
        start.clone(),
        initial_hp,
        budget.base_search(config.max_actions_per_line),
        config,
        &stepper,
        LineSearchSeed::root(),
    );
    let base = run.best_win?;
    let base_hp_loss = (initial_hp - base.position.combat.entities.player.current_hp).max(0);
    let base_action_count = base.actions.len();
    let (best, repair) =
        repair_line_if_useful(start, base, initial_hp, budget.repair, config, &stepper);
    let final_hp_loss = (initial_hp - best.position.combat.entities.player.current_hp).max(0);
    let final_action_count = best.actions.len();
    Some(CompleteLineSolverOutcome {
        line: CombatCandidateLine::from_position(
            CombatCandidateLineSource::CompleteLineSolver,
            reindex_actions(best.actions),
            initial_hp,
            &best.position,
        ),
        base_hp_loss,
        base_action_count,
        final_hp_loss,
        final_action_count,
        repair_hp_loss_saved: base_hp_loss - final_hp_loss,
        repair_action_count_delta: final_action_count as isize - base_action_count as isize,
        base_node_budget: budget.base_nodes,
        base_ms_budget: budget.base_ms,
        repair_node_budget_per_cut: budget.repair.nodes_per_cut,
        repair_ms_budget_per_cut: budget.repair.ms_per_cut,
        repair_cut_budget: budget.repair.cuts,
        base_stop_reason: run.stop_reason.label(),
        last_repair_stop_reason: repair.last_stop_reason.map(|reason| reason.label()),
        repair_attempts: repair.attempts,
        repair_wins: repair.wins,
        repair_improvements: repair.improvements,
        base_nodes_expanded: run.nodes_expanded,
        base_nodes_generated: run.nodes_generated,
        repair_nodes_expanded: repair.nodes_expanded,
        repair_nodes_generated: repair.nodes_generated,
        nodes_expanded: run.nodes_expanded + repair.nodes_expanded,
        nodes_generated: run.nodes_generated + repair.nodes_generated,
        elapsed_ms: started.elapsed().as_millis(),
    })
}
