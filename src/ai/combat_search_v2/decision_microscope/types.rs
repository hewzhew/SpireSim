use serde::Serialize;

use crate::state::core::ClientInput;

use super::super::{
    CombatSearchV2ActionFacts, CombatSearchV2FrontierValueReport, CombatSearchV2OutcomeReport,
    CombatSearchV2PhaseProfileReport, CombatSearchV2StateSummary, SearchTerminalLabel,
};

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DecisionMicroscopeReport {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub question: &'static str,
    pub behavioral_scope: &'static str,
    pub input_label: Option<String>,
    pub config: CombatSearchV2DecisionMicroscopeConfigReport,
    pub search_outcome: CombatSearchV2OutcomeReport,
    pub best_complete_summary: Option<CombatSearchV2DecisionTrajectorySummary>,
    pub best_win_summary: Option<CombatSearchV2DecisionTrajectorySummary>,
    pub selected_first_action: Option<CombatSearchV2DecisionSelectedAction>,
    pub initial_context: CombatSearchV2DecisionContext,
    pub candidate_count: usize,
    pub reported_candidate_limit: usize,
    pub candidates: Vec<CombatSearchV2DecisionCandidateReport>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DecisionMicroscopeConfigReport {
    pub max_nodes: usize,
    pub max_actions_per_line: usize,
    pub max_engine_steps_per_action: usize,
    pub wall_time_ms: Option<u128>,
    pub potion_policy: &'static str,
    pub max_potions_used: Option<u32>,
    pub rollout_policy: &'static str,
    pub rollout_max_evaluations: usize,
    pub rollout_max_actions: usize,
    pub rollout_beam_width: usize,
    pub phase_guard_policy: &'static str,
    pub setup_bias_policy: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DecisionTrajectorySummary {
    pub terminal: SearchTerminalLabel,
    pub estimated: bool,
    pub final_hp: i32,
    pub hp_loss: i32,
    pub turns: u32,
    pub potions_used: u32,
    pub potions_discarded: u32,
    pub cards_played: u32,
    pub action_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DecisionSelectedAction {
    pub action_id: usize,
    pub action_key: String,
    pub action_debug: String,
    pub action_role: &'static str,
    pub selection_source: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DecisionContext {
    pub state: CombatSearchV2StateSummary,
    pub phase_profile: CombatSearchV2PhaseProfileReport,
    pub frontier_value: CombatSearchV2FrontierValueReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DecisionCandidateReport {
    pub original_action_id: usize,
    pub ordered_index: usize,
    pub action_key: String,
    pub action_debug: String,
    pub action_role: &'static str,
    pub selected_by_best_complete: bool,
    pub input: ClientInput,
    pub action_facts: CombatSearchV2ActionFactsReport,
    pub one_step: CombatSearchV2DecisionOneStepReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2ActionFactsReport {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub evidence_policy: &'static str,
    pub consumer_boundary: &'static str,
    pub facts: CombatSearchV2ActionFacts,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DecisionOneStepReport {
    pub status: &'static str,
    pub engine_steps: usize,
    pub terminal: SearchTerminalLabel,
    pub transition: Option<String>,
    pub turn_branch_priority_hint: Option<i32>,
    pub player_hp: i32,
    pub player_block: i32,
    pub energy: u8,
    pub visible_incoming_damage: i32,
    pub visible_hp_loss_if_turn_ends: i32,
    pub survival_margin: i32,
    pub living_enemy_count: usize,
    pub total_enemy_hp: i32,
    pub total_enemy_block: i32,
    pub phase_adjusted_enemy_effort: i32,
    pub split_debt_hp: i32,
    pub guardian_mode_shift_pending_count: usize,
    pub lagavulin_waking_count: usize,
    pub gremlin_nob_anger_amount_total: i32,
    pub sentry_dazed_pressure_count: usize,
    pub hexaghost_opening_pressure_count: usize,
    pub pending_choice_present: bool,
    pub pending_choice_estimated_action_fanout: usize,
}
