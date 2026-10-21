use super::super::action_priority::ActionOrderingRole;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct PendingChoiceProfile {
    pub(super) kind: &'static str,
    pub(super) reason: Option<String>,
    pub(super) source_pile: Option<String>,
    pub(super) candidate_count: usize,
    pub(super) estimated_action_fanout: usize,
    pub(super) min_cards: usize,
    pub(super) max_cards: usize,
    pub(super) can_cancel: bool,
    pub(super) fanout_class: &'static str,
    pub(super) search_risk: &'static str,
}

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct PendingChoiceDiagnosticsCollector {
    pub(super) states_observed: u64,
    pub(super) pending_choice_states: u64,
    pub(super) expanded_pending_choice_states: u64,
    pub(super) high_fanout_states: u64,
    pub(super) max_candidate_count: usize,
    pub(super) legal_actions_from_pending_choice: u64,
    pub(super) max_legal_actions_from_pending_choice: usize,
    pub(super) resolved_children: u64,
    pub(super) still_pending_children: u64,
    pub(super) truncated_children: u64,
    pub(super) kind_counts: BTreeMap<&'static str, MutablePendingChoiceKindCount>,
    pub(super) ordering_role_counts:
        BTreeMap<ActionOrderingRole, MutablePendingChoiceOrderingRoleCount>,
    pub(super) largest_pending_choices: Vec<PendingChoiceObservation>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct MutablePendingChoiceKindCount {
    pub(super) states: u64,
    pub(super) max_candidate_count: usize,
    pub(super) max_estimated_action_fanout: usize,
}

#[derive(Clone, Debug, Default)]
pub(super) struct MutablePendingChoiceOrderingRoleCount {
    pub(super) actions: u64,
    pub(super) first_actions: u64,
}

#[derive(Clone, Debug)]
pub(super) struct PendingChoiceObservation {
    pub(super) observed_at_state_query: u64,
    pub(super) profile: PendingChoiceProfile,
}
