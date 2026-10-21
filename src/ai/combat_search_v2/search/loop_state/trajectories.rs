use super::super::super::frontier::SearchNode;
use super::super::super::CombatSearchV2Config;
use super::SearchLoopState;

impl SearchLoopState {
    pub(in crate::ai::combat_search_v2::search) fn reportable_trajectories(
        &self,
    ) -> super::super::best_trajectories::SearchTrajectoryBook {
        let root_states = self.root_action_schedule_states();
        let current_round_complete = self
            .root_round_scheduler
            .current_comparison_complete(&root_states);
        let mut reportable = if self.accepted_complete_candidate
            || !self.root_round_scheduler.started()
            || self.root_round_scheduler.completed_rounds() > 0
            || current_round_complete
        {
            self.trajectories.clone()
        } else {
            self.completed_root_round_trajectories
                .clone()
                .unwrap_or_else(|| self.trajectories.empty_at_same_root())
        };
        // An incomplete root-comparison round limits ranking confidence; it
        // does not invalidate an exact terminal witness already replayed by
        // the search. Keep open/frontier observations gated by the completed
        // round, but never hide exact wins from the execution boundary.
        if self.trajectories.best_win.is_some() {
            reportable.best_complete = self.trajectories.best_complete.clone();
            reportable.best_win = self.trajectories.best_win.clone();
            reportable.win_candidates = self.trajectories.win_candidates.clone();
            reportable.win_frontier_revision = self.trajectories.win_frontier_revision;
        }
        reportable.best_frontier = self.trajectories.best_frontier.clone();
        reportable
    }

    pub(in crate::ai::combat_search_v2::search) fn remember_best_frontier(
        &mut self,
        node: &SearchNode,
    ) {
        self.trajectories.remember_best_frontier(node);
    }

    pub(in crate::ai::combat_search_v2::search) fn remember_complete(&mut self, node: SearchNode) {
        self.root_evidence.observe_exact_complete(&node);
        self.trajectories.remember_complete(node);
    }

    pub(in crate::ai::combat_search_v2::search) fn remember_win(
        &mut self,
        node: SearchNode,
        config: &CombatSearchV2Config,
    ) -> bool {
        let nodes_generated_at_discovery = self.stats.nodes_generated;
        self.remember_win_observed_at(node, config, nodes_generated_at_discovery, true)
    }

    pub(in crate::ai::combat_search_v2::search) fn remember_promoted_win_observed_at(
        &mut self,
        node: SearchNode,
        config: &CombatSearchV2Config,
        nodes_generated_at_discovery: u64,
    ) -> bool {
        let first_retained_path_observation = !self.trajectories.contains_exact_win_path(&node);
        self.remember_win_observed_at(
            node,
            config,
            nodes_generated_at_discovery,
            first_retained_path_observation,
        )
    }

    fn remember_win_observed_at(
        &mut self,
        node: SearchNode,
        config: &CombatSearchV2Config,
        nodes_generated_at_discovery: u64,
        count_observation: bool,
    ) -> bool {
        if count_observation {
            self.stats.terminal_wins = self.stats.terminal_wins.saturating_add(1);
            if self.stats.nodes_to_first_win.is_none() {
                self.stats.nodes_to_first_win = Some(nodes_generated_at_discovery);
            }
        }
        self.root_evidence.observe_exact_win(&node);
        self.trajectories
            .remember_win(node, config, self.initial_external_burden_count)
    }

    pub(in crate::ai::combat_search_v2::search) fn remember_loss(&mut self, node: SearchNode) {
        self.stats.terminal_losses = self.stats.terminal_losses.saturating_add(1);
        self.remember_complete(node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::combat_search_v2::frontier::RootLineageId;
    use crate::ai::combat_search_v2::search::root_round_scheduler::RootActionScheduleState;
    use crate::ai::combat_search_v2::{
        terminal_label, CombatSearchV2Satisfaction, SearchTerminalLabel,
    };
    use crate::state::core::{EngineState, RunResult};
    use crate::test_support::blank_test_combat;

    #[test]
    fn incomplete_root_round_keeps_exact_win_executable_without_claiming_acceptance() {
        let config = CombatSearchV2Config {
            satisfaction: CombatSearchV2Satisfaction::BudgetOrExhaustion,
            ..CombatSearchV2Config::default()
        };
        let mut state = SearchLoopState::new(
            &config,
            false,
            crate::sim::combat::CombatPosition::new(
                EngineState::CombatPlayerTurn,
                blank_test_combat(),
            ),
        );
        state.root_round_scheduler.activate(
            &[
                RootActionScheduleState {
                    id: RootLineageId(0),
                    expanded: 0,
                    has_work: true,
                },
                RootActionScheduleState {
                    id: RootLineageId(1),
                    expanded: 0,
                    has_work: true,
                },
            ],
            "test",
        );
        state.completed_root_round_trajectories = Some(state.trajectories.empty_at_same_root());
        let win = SearchNode::root(
            EngineState::GameOver(RunResult::Victory),
            blank_test_combat(),
        );

        assert!(!state.remember_win(win, &config));
        assert!(!state.accepted_complete_candidate);
        let reportable = state.reportable_trajectories();
        assert_eq!(
            reportable
                .best_win
                .as_ref()
                .map(|node| terminal_label(&node.engine, &node.combat)),
            Some(SearchTerminalLabel::Win)
        );
    }
}
