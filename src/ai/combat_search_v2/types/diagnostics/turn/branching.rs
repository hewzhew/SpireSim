use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnBranching {
    pub organization_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub states_observed: u64,
    pub total_legal_actions: u64,
    pub total_generated_children: u64,
    pub generated_children_per_state: f64,
    pub same_turn_children: u64,
    pub next_turn_children: u64,
    pub pending_choice_children: u64,
    pub terminal_children: u64,
    pub other_children: u64,
    pub end_turn_children: u64,
    pub same_turn_child_ratio: f64,
    pub next_turn_child_ratio: f64,
    pub transition_counts: Vec<CombatSearchV2DiagnosticsTurnTransitionCount>,
    pub largest_turn_fanouts: Vec<CombatSearchV2DiagnosticsTurnFanoutSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnTransitionCount {
    pub action_kind: String,
    pub transition_kind: String,
    pub children: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnFanoutSample {
    pub parent_turn_count: u32,
    pub parent_energy: u8,
    pub legal_actions: usize,
    pub generated_children: usize,
    pub same_turn_children: usize,
    pub next_turn_children: usize,
    pub pending_choice_children: usize,
    pub terminal_children: usize,
    pub end_turn_children: usize,
}
