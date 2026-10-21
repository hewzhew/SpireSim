use super::super::*;
use super::types::PendingChoiceObservation;
use super::{PendingChoiceDiagnosticsCollector, PendingChoiceProfile};

impl PendingChoiceDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn observe(
        &mut self,
        profile: Option<&PendingChoiceProfile>,
    ) {
        self.states_observed = self.states_observed.saturating_add(1);
        let Some(profile) = profile else {
            return;
        };

        self.pending_choice_states = self.pending_choice_states.saturating_add(1);
        self.max_candidate_count = self.max_candidate_count.max(profile.candidate_count);
        if profile.search_risk == "high_fanout_pending_choice" {
            self.high_fanout_states = self.high_fanout_states.saturating_add(1);
        }

        let count = self.kind_counts.entry(profile.kind).or_default();
        count.states = count.states.saturating_add(1);
        count.max_candidate_count = count.max_candidate_count.max(profile.candidate_count);
        count.max_estimated_action_fanout = count
            .max_estimated_action_fanout
            .max(profile.estimated_action_fanout);
        self.remember_largest_pending_choice(PendingChoiceObservation {
            observed_at_state_query: self.states_observed,
            profile: profile.clone(),
        });
    }

    pub(in crate::ai::combat_search_v2) fn observe_ordering(
        &mut self,
        profile: Option<&PendingChoiceProfile>,
        ordering: &ActionOrderingSummary,
    ) {
        if profile.is_none() {
            return;
        }

        self.expanded_pending_choice_states = self.expanded_pending_choice_states.saturating_add(1);
        self.legal_actions_from_pending_choice = self
            .legal_actions_from_pending_choice
            .saturating_add(ordering.action_count() as u64);
        self.max_legal_actions_from_pending_choice = self
            .max_legal_actions_from_pending_choice
            .max(ordering.action_count());

        for (role, count) in ordering.role_counts() {
            let mutable = self.ordering_role_counts.entry(role).or_default();
            mutable.actions = mutable.actions.saturating_add(count as u64);
        }
        if let Some(first_role) = ordering.first_role() {
            let mutable = self.ordering_role_counts.entry(first_role).or_default();
            mutable.first_actions = mutable.first_actions.saturating_add(1);
        }
    }

    pub(in crate::ai::combat_search_v2) fn observe_child_transition(
        &mut self,
        profile: Option<&PendingChoiceProfile>,
        truncated: bool,
        child_engine: &EngineState,
    ) {
        if profile.is_none() {
            return;
        }

        if truncated {
            self.truncated_children = self.truncated_children.saturating_add(1);
        } else if matches!(child_engine, EngineState::PendingChoice(_)) {
            self.still_pending_children = self.still_pending_children.saturating_add(1);
        } else {
            self.resolved_children = self.resolved_children.saturating_add(1);
        }
    }
}
