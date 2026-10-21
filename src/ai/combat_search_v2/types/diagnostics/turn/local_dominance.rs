use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnLocalDominance {
    pub pruning_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub parent_states_observed: u64,
    pub enabled_parent_states: u64,
    pub eligible_child_states: u64,
    pub accepted_child_states: u64,
    pub pruned_child_states: u64,
    pub prune_ratio: f64,
    pub max_parent_dominance_buckets: usize,
    pub max_parent_resource_vectors: usize,
    pub max_bucket_width: usize,
    pub largest_parent_samples: Vec<CombatSearchV2DiagnosticsTurnLocalDominanceSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnLocalDominanceSample {
    pub observed_at_parent_state: u64,
    pub parent_turn_count: u32,
    pub legal_actions: usize,
    pub eligible_child_states: usize,
    pub accepted_child_states: usize,
    pub pruned_child_states: usize,
    pub dominance_buckets: usize,
    pub resource_vectors: usize,
    pub max_bucket_width: usize,
}
