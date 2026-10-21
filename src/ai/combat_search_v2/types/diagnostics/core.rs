use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTables {
    pub exact_keys: usize,
    pub exact_resource_vectors: usize,
    pub dominance_buckets: usize,
    pub dominance_resource_vectors: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsBranching {
    pub states_queried: u64,
    pub states_with_legal_actions: u64,
    pub legal_actions_total: u64,
    pub legal_actions_avg: f64,
    pub legal_actions_max: usize,
    pub nodes_generated_per_expanded: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsPruning {
    pub transposition_prunes: u64,
    pub dominance_prunes: u64,
    pub turn_local_dominance_prunes: u64,
    pub terminal_wins: u64,
    pub terminal_losses: u64,
    pub unresolved_leaf_count: u64,
    pub max_actions_cut_count: u64,
    pub engine_step_limit_count: u64,
    pub potion_budget_cut_count: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsFrontier {
    pub remaining_work_items: usize,
    pub sample_limit: usize,
    pub sampled_states: usize,
}
