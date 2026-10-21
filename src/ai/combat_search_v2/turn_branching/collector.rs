use super::types::{TurnBranchingDiagnosticsCollector, TurnBranchingStateObservation};

impl TurnBranchingDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn observe(
        &mut self,
        observation: &TurnBranchingStateObservation,
    ) {
        self.states_observed = self.states_observed.saturating_add(1);
        self.total_legal_actions = self
            .total_legal_actions
            .saturating_add(observation.legal_actions as u64);
        self.total_generated_children = self
            .total_generated_children
            .saturating_add(observation.generated_children as u64);
        self.same_turn_children = self
            .same_turn_children
            .saturating_add(observation.same_turn_children as u64);
        self.next_turn_children = self
            .next_turn_children
            .saturating_add(observation.next_turn_children as u64);
        self.pending_choice_children = self
            .pending_choice_children
            .saturating_add(observation.pending_choice_children as u64);
        self.terminal_children = self
            .terminal_children
            .saturating_add(observation.terminal_children as u64);
        self.other_children = self
            .other_children
            .saturating_add(observation.other_children as u64);
        self.end_turn_children = self
            .end_turn_children
            .saturating_add(observation.end_turn_children as u64);

        for (key, count) in &observation.transition_counts {
            *self.transition_counts.entry(*key).or_insert(0) += *count as u64;
        }
        self.remember_largest_turn_fanout(observation);
    }
}
