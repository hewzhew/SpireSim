use super::super::super::frontier::SearchNode;
use super::super::super::{terminal_label, SearchTerminalLabel};
use super::SearchLoopState;

impl SearchLoopState {
    pub(in crate::ai::combat_search_v2::search) fn mark_deadline_hit(&mut self) {
        self.stats.deadline_hit = true;
        self.exhausted = true;
    }

    pub(in crate::ai::combat_search_v2::search) fn mark_node_budget_hit(&mut self) {
        self.stats.node_budget_hit = true;
        self.exhausted = true;
    }

    pub(in crate::ai::combat_search_v2::search) fn mark_action_prefix_budget_hit(&mut self) {
        self.stats.action_prefix_budget_hit = true;
        self.exhausted = true;
    }

    pub(in crate::ai::combat_search_v2::search) fn mark_action_surface_incomplete(&mut self) {
        self.stats.action_surface_incomplete = true;
    }

    pub(in crate::ai::combat_search_v2::search) fn mark_accepted_complete_candidate(&mut self) {
        self.accepted_complete_candidate = true;
    }

    pub(in crate::ai::combat_search_v2::search) fn record_node_expanded(
        &mut self,
        node: &SearchNode,
    ) {
        self.stats.nodes_expanded = self.stats.nodes_expanded.saturating_add(1);
        self.root_evidence.record_expanded(node);
    }

    pub(in crate::ai::combat_search_v2::search) fn record_node_generated(
        &mut self,
        node: &SearchNode,
    ) {
        self.stats.nodes_generated = self.stats.nodes_generated.saturating_add(1);
        self.root_evidence.record_generated(node);
    }

    pub(in crate::ai::combat_search_v2::search) fn record_turn_boundary_work(
        &mut self,
        source: &SearchNode,
        nodes_expanded: usize,
        nodes_generated: usize,
    ) {
        self.stats.nodes_expanded = self
            .stats
            .nodes_expanded
            .saturating_add(nodes_expanded as u64);
        self.stats.nodes_generated = self
            .stats
            .nodes_generated
            .saturating_add(nodes_generated as u64);
        self.root_evidence
            .record_bulk_work(source, nodes_expanded, nodes_generated);
    }

    pub(in crate::ai::combat_search_v2::search) fn record_first_generated_win_if_needed(
        &mut self,
        node: &SearchNode,
    ) {
        if self.stats.nodes_to_first_win.is_none()
            && terminal_label(&node.engine, &node.combat) == SearchTerminalLabel::Win
        {
            self.stats.nodes_to_first_win = Some(self.stats.nodes_generated);
        }
    }

    pub(in crate::ai::combat_search_v2::search) fn record_unresolved_leaf(
        &mut self,
        node: &SearchNode,
    ) {
        self.unresolved_leaf_count = self.unresolved_leaf_count.saturating_add(1);
        self.remember_best_frontier(node);
    }

    pub(in crate::ai::combat_search_v2::search) fn record_max_actions_cut(&mut self) {
        self.max_actions_cut_count = self.max_actions_cut_count.saturating_add(1);
    }

    pub(in crate::ai::combat_search_v2::search) fn record_engine_step_limit(&mut self) {
        self.engine_step_limit_count = self.engine_step_limit_count.saturating_add(1);
    }

    pub(in crate::ai::combat_search_v2::search) fn record_potion_budget_cut(&mut self) {
        self.potion_budget_cut_count = self.potion_budget_cut_count.saturating_add(1);
    }

    pub(in crate::ai::combat_search_v2::search) fn record_transposition_prune(&mut self) {
        self.stats.transposition_prunes = self.stats.transposition_prunes.saturating_add(1);
    }

    pub(in crate::ai::combat_search_v2::search) fn record_dominance_prune(&mut self) {
        self.stats.dominance_prunes = self.stats.dominance_prunes.saturating_add(1);
    }

    pub(in crate::ai::combat_search_v2::search) fn record_turn_local_dominance_prune(&mut self) {
        self.stats.turn_local_dominance_prunes =
            self.stats.turn_local_dominance_prunes.saturating_add(1);
        self.performance.turn_local_dominance_rollout_skips = self
            .performance
            .turn_local_dominance_rollout_skips
            .saturating_add(1);
    }
}
