use super::types::{
    ActionExpansionDiagnosticsCollector, ActionExpansionGroupObservation, ActionExpansionSummary,
};

impl ActionExpansionDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn observe(&mut self, summary: &ActionExpansionSummary) {
        self.states_observed = self.states_observed.saturating_add(1);
        self.total_atomic_actions = self
            .total_atomic_actions
            .saturating_add(summary.action_count as u64);
        self.total_fanout_groups = self
            .total_fanout_groups
            .saturating_add(summary.group_count as u64);
        self.fanout_groups_max = self.fanout_groups_max.max(summary.group_count);

        for group in &summary.groups {
            self.max_group_size = self.max_group_size.max(group.action_count);
            let count = self.kind_counts.entry(group.key.kind).or_default();
            count.atomic_actions = count
                .atomic_actions
                .saturating_add(group.action_count as u64);
            count.fanout_groups = count.fanout_groups.saturating_add(1);
            count.max_group_size = count.max_group_size.max(group.action_count);
            self.remember_largest_group(ActionExpansionGroupObservation {
                observed_at_state_query: self.states_observed,
                key: group.key.clone(),
                action_count: group.action_count,
            });
        }
    }
}
