use super::super::*;
use super::types::PendingChoiceObservation;
use super::PendingChoiceDiagnosticsCollector;

const LARGEST_PENDING_CHOICE_SAMPLE_LIMIT: usize = 8;

impl PendingChoiceDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn finish(&self) -> CombatSearchV2DiagnosticsPendingChoice {
        CombatSearchV2DiagnosticsPendingChoice {
            profiling_policy: "typed_pending_choice_profile_no_prune_no_auto_resolution",
            behavioral_effect: "diagnostic_only_search_expansion_unchanged",
            rollout_contract_policy:
                "search_expands_legal_pending_choice_actions_and_exact_replays_selected_child",
            rollout_contract_behavioral_effect:
                "diagnostic_only_no_prune_no_auto_resolution_no_terminal_claim",
            states_observed: self.states_observed,
            pending_choice_states: self.pending_choice_states,
            expanded_pending_choice_states: self.expanded_pending_choice_states,
            high_fanout_states: self.high_fanout_states,
            max_candidate_count: self.max_candidate_count,
            legal_actions_from_pending_choice: self.legal_actions_from_pending_choice,
            max_legal_actions_from_pending_choice: self.max_legal_actions_from_pending_choice,
            resolved_children: self.resolved_children,
            still_pending_children: self.still_pending_children,
            truncated_children: self.truncated_children,
            kind_counts: self.kind_count_reports(),
            ordering_role_counts: self.ordering_role_count_reports(),
            largest_pending_choices: self.largest_pending_choice_reports(),
            notes: vec![
                "pending choice profile only classifies choice boundaries; it does not resolve or prune them",
                "large grid/hand/scry choices are search-risk signals, not evidence that any branch is safe to drop",
                "future compression must prove selection equivalence or order-insensitivity before pruning",
                "pending choice rollout contract metrics count exact child transitions after legal choice inputs",
                "ordering roles are child-generation order hints only; they never suppress candidate choices",
            ],
        }
    }

    pub(super) fn remember_largest_pending_choice(
        &mut self,
        observation: PendingChoiceObservation,
    ) {
        if observation.profile.candidate_count <= 1 {
            return;
        }
        self.largest_pending_choices.push(observation);
        self.largest_pending_choices.sort_by(|left, right| {
            right
                .profile
                .candidate_count
                .cmp(&left.profile.candidate_count)
                .then_with(|| left.profile.kind.cmp(right.profile.kind))
                .then_with(|| {
                    left.observed_at_state_query
                        .cmp(&right.observed_at_state_query)
                })
        });
        self.largest_pending_choices
            .truncate(LARGEST_PENDING_CHOICE_SAMPLE_LIMIT);
    }

    fn kind_count_reports(&self) -> Vec<CombatSearchV2DiagnosticsPendingChoiceKindCount> {
        self.kind_counts
            .iter()
            .map(
                |(kind, count)| CombatSearchV2DiagnosticsPendingChoiceKindCount {
                    kind: (*kind).to_string(),
                    states: count.states,
                    max_candidate_count: count.max_candidate_count,
                    max_estimated_action_fanout: count.max_estimated_action_fanout,
                },
            )
            .collect()
    }

    fn ordering_role_count_reports(
        &self,
    ) -> Vec<CombatSearchV2DiagnosticsPendingChoiceOrderingRoleCount> {
        self.ordering_role_counts
            .iter()
            .map(
                |(role, count)| CombatSearchV2DiagnosticsPendingChoiceOrderingRoleCount {
                    role: role.label().to_string(),
                    actions: count.actions,
                    first_actions: count.first_actions,
                },
            )
            .collect()
    }

    fn largest_pending_choice_reports(&self) -> Vec<CombatSearchV2DiagnosticsPendingChoiceSample> {
        self.largest_pending_choices
            .iter()
            .map(|observation| {
                let profile = &observation.profile;
                CombatSearchV2DiagnosticsPendingChoiceSample {
                    observed_at_state_query: observation.observed_at_state_query,
                    kind: profile.kind.to_string(),
                    reason: profile.reason.clone(),
                    source_pile: profile.source_pile.clone(),
                    candidate_count: profile.candidate_count,
                    estimated_action_fanout: profile.estimated_action_fanout,
                    min_cards: profile.min_cards,
                    max_cards: profile.max_cards,
                    can_cancel: profile.can_cancel,
                    fanout_class: profile.fanout_class,
                    search_risk: profile.search_risk,
                }
            })
            .collect()
    }
}
