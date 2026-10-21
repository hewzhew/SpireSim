use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnPlan {
    pub planning_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub root_states_observed: u64,
    pub total_plans: u64,
    pub max_plans_in_state: usize,
    pub total_inner_nodes_expanded: u64,
    pub total_inner_nodes_generated: u64,
    pub total_exact_state_skips: u64,
    pub total_truncated_children: u64,
    pub turn_plan_prior_scored_plans: u64,
    pub frontier_seeded_nodes: u64,
    pub frontier_seed_plans: Vec<CombatSearchV2DiagnosticsTurnPlanEntry>,
    pub bucket_counts: Vec<CombatSearchV2DiagnosticsTurnPlanCount>,
    pub stop_reason_counts: Vec<CombatSearchV2DiagnosticsTurnPlanCount>,
    pub samples: Vec<CombatSearchV2DiagnosticsTurnPlanSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnPlanCount {
    pub label: String,
    pub plans: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnPlanSample {
    pub observed_at_root_state: u64,
    pub plans: usize,
    pub inner_nodes_expanded: usize,
    pub inner_nodes_generated: usize,
    pub exact_state_skips: usize,
    pub truncated_children: usize,
    pub top_plans: Vec<CombatSearchV2DiagnosticsTurnPlanEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnPlanEntry {
    pub rank: usize,
    pub bucket: &'static str,
    pub stop_reason: &'static str,
    pub outcome_class: &'static str,
    pub survival_bucket: &'static str,
    pub progress_bucket: &'static str,
    pub action_count: usize,
    pub final_hp: i32,
    pub risk_margin: i32,
    pub enemy_progress: i32,
    pub first_action_key: Option<String>,
    pub action_keys_preview: Vec<String>,
}
