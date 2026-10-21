use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsPendingChoice {
    pub profiling_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub rollout_contract_policy: &'static str,
    pub rollout_contract_behavioral_effect: &'static str,
    pub states_observed: u64,
    pub pending_choice_states: u64,
    pub expanded_pending_choice_states: u64,
    pub high_fanout_states: u64,
    pub max_candidate_count: usize,
    pub legal_actions_from_pending_choice: u64,
    pub max_legal_actions_from_pending_choice: usize,
    pub resolved_children: u64,
    pub still_pending_children: u64,
    pub truncated_children: u64,
    pub kind_counts: Vec<CombatSearchV2DiagnosticsPendingChoiceKindCount>,
    pub ordering_role_counts: Vec<CombatSearchV2DiagnosticsPendingChoiceOrderingRoleCount>,
    pub largest_pending_choices: Vec<CombatSearchV2DiagnosticsPendingChoiceSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsPendingChoiceKindCount {
    pub kind: String,
    pub states: u64,
    pub max_candidate_count: usize,
    pub max_estimated_action_fanout: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsPendingChoiceOrderingRoleCount {
    pub role: String,
    pub actions: u64,
    pub first_actions: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsPendingChoiceSample {
    pub observed_at_state_query: u64,
    pub kind: String,
    pub reason: Option<String>,
    pub source_pile: Option<String>,
    pub candidate_count: usize,
    pub estimated_action_fanout: usize,
    pub min_cards: usize,
    pub max_cards: usize,
    pub can_cancel: bool,
    pub fanout_class: &'static str,
    pub search_risk: &'static str,
}
