use serde::Serialize;

use super::super::{CombatSearchV2ActionPreview, SearchTerminalLabel};

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2RolloutReport {
    pub policy: &'static str,
    pub behavioral_effect: &'static str,
    pub max_evaluations: usize,
    pub max_actions_per_rollout: usize,
    pub beam_width: usize,
    pub turn_beam_extension_budget: usize,
    pub turn_beam_extensions: u64,
    pub turn_beam_extension_budget_skips: u64,
    pub evaluations: u64,
    pub cache_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_inserts: u64,
    pub budget_skips: u64,
    pub max_evaluation_budget_skips: u64,
    pub deadline_budget_skips: u64,
    pub truncated_rollouts: u64,
    pub terminal_wins: u64,
    pub terminal_losses: u64,
    pub rollouts_with_pending_choice: u64,
    pub rollouts_stopped_on_high_fanout_pending_choice: u64,
    pub pending_choice_actions_simulated: u64,
    pub max_pending_choice_estimated_action_fanout: usize,
    pub performance: CombatSearchV2RolloutPerformanceReport,
    pub turn_beam_attribution: CombatSearchV2TurnBeamAttributionReport,
    pub best_frontier_estimate: Option<CombatSearchV2RolloutEstimateReport>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2RolloutPerformanceReport {
    pub cache_lookup_us: u128,
    pub policy_dispatch_us: u128,
    pub no_potion_iterations: u64,
    pub no_potion_phase_profile_us: u128,
    pub no_potion_legal_actions_us: u128,
    pub no_potion_choose_action_us: u128,
    pub no_potion_choose_ordering_us: u128,
    pub no_potion_probe_us: u128,
    pub no_potion_probe_score_calls: u64,
    pub no_potion_probe_actions_evaluated: u64,
    pub no_potion_probe_step_reuses: u64,
    pub no_potion_probe_engine_step_us: u128,
    pub no_potion_probe_phase_profile_us: u128,
    pub no_potion_probe_action_facts_us: u128,
    pub no_potion_engine_step_us: u128,
    pub no_potion_child_build_us: u128,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2TurnBeamAttributionReport {
    pub enabled: bool,
    pub calls: u64,
    pub conservative_anchor_present: u64,
    pub conservative_anchor_selected: u64,
    pub conservative_anchor_terminal_wins: u64,
    pub extension_calls: u64,
    pub turn_plan_calls: u64,
    pub turn_plan_inner_nodes_expanded: u64,
    pub turn_plan_inner_nodes_generated: u64,
    pub turn_plans_kept: u64,
    pub turn_plans_kept_by_bucket: Vec<CombatSearchV2TurnBeamBucketCountReport>,
    pub terminal_candidates_kept: u64,
    pub best_pv_len: usize,
    pub best_pv_terminal: Option<SearchTerminalLabel>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnBeamBucketCountReport {
    pub bucket: &'static str,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2RolloutEstimateReport {
    pub terminal: SearchTerminalLabel,
    pub estimated: bool,
    pub final_hp: i32,
    pub hp_loss: i32,
    pub turns: u32,
    pub potions_used: u32,
    pub potions_discarded: u32,
    pub cards_played: u32,
    pub living_enemy_count: usize,
    pub total_enemy_hp: i32,
    pub total_enemy_block: i32,
    pub phase_adjusted_enemy_effort: i32,
    pub special_enemy_phase_count: usize,
    pub guardian_mode_shift_pending_count: usize,
    pub lagavulin_waking_count: usize,
    pub gremlin_nob_anger_amount_total: i32,
    pub sentry_dazed_pressure_count: usize,
    pub hexaghost_opening_pressure_count: usize,
    pub high_fanout_pending_choice: bool,
    pub pending_choice_estimated_action_fanout: usize,
    pub pending_choices_seen: usize,
    pub pending_choice_actions_simulated: usize,
    pub max_pending_choice_candidate_count: usize,
    pub max_pending_choice_estimated_action_fanout: usize,
    pub last_pending_choice_kind: Option<&'static str>,
    pub stopped_on_high_fanout_pending_choice: bool,
    pub survival_margin: i32,
    pub actions_simulated: usize,
    pub action_preview: Vec<CombatSearchV2ActionPreview>,
    pub truncated: bool,
    pub stop_reason: &'static str,
    pub last_action_reason: Option<&'static str>,
}
