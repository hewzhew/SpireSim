use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnPrefix {
    pub tracking_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub states_observed: u64,
    pub non_empty_prefix_states: u64,
    pub empty_prefix_states: u64,
    pub avg_prefix_length: f64,
    pub max_prefix_length: usize,
    pub max_legal_actions_after_non_empty_prefix: usize,
    pub total_cards_played_in_prefix: u64,
    pub total_potions_used_in_prefix: u64,
    pub total_potions_discarded_in_prefix: u64,
    pub total_other_actions_in_prefix: u64,
    pub prefix_length_counts: Vec<CombatSearchV2DiagnosticsTurnPrefixLengthCount>,
    pub prefix_kind_counts: Vec<CombatSearchV2DiagnosticsTurnPrefixKindCount>,
    pub largest_prefix_fanouts: Vec<CombatSearchV2DiagnosticsTurnPrefixFanoutSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnPrefixLengthCount {
    pub prefix_length: usize,
    pub states: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnPrefixKindCount {
    pub kind: String,
    pub states: u64,
    pub legal_actions_total: u64,
    pub max_prefix_length: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnPrefixFanoutSample {
    pub observed_at_state_query: u64,
    pub prefix_length: usize,
    pub kind: String,
    pub cards_played: usize,
    pub potions_used: usize,
    pub potions_discarded: usize,
    pub other_actions: usize,
    pub legal_actions: usize,
    pub signature_preview: String,
    pub signature_truncated: bool,
}
