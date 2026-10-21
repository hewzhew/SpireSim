use std::collections::BTreeMap;

use serde::Serialize;

use crate::state::core::ClientInput;

use super::{
    CombatSearchV2ActionFacts, CombatSearchV2ActionTrace, CombatSearchV2DecisionContext,
    CombatSearchV2StateSummary,
};

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeRootReport {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub question: &'static str,
    pub behavioral_scope: &'static str,
    pub input_label: Option<String>,
    pub config: CombatSearchV2TurnPlanProbeConfigReport,
    pub initial_context: CombatSearchV2DecisionContext,
    pub root_action_mask: CombatSearchV2TurnPlanProbeActionMaskReport,
    pub enumeration: CombatSearchV2TurnPlanProbeEnumerationReport,
    pub selection_audit: CombatSearchV2TurnPlanProbeSelectionAuditReport,
    pub candidates: Vec<CombatSearchV2TurnPlanProbeCandidateReport>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeConfigReport {
    pub max_inner_nodes: usize,
    pub max_end_states: usize,
    pub per_bucket_limit: usize,
    pub potion_policy: &'static str,
    pub allowed_potion_slots: Option<u64>,
    pub allow_potion_discard: Option<bool>,
    pub max_engine_steps_per_action: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeEnumerationReport {
    pub planning_policy: &'static str,
    pub plans: usize,
    pub preselection_plans: usize,
    pub preselection_first_action_count: usize,
    pub preselection_bucket_counts: BTreeMap<&'static str, usize>,
    pub selected_bucket_counts: BTreeMap<&'static str, usize>,
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub engine_steps: usize,
    pub exact_state_skips: usize,
    pub truncated_children: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeSelectionAuditReport {
    pub data_role: &'static str,
    pub behavioral_effect: &'static str,
    pub candidates: Vec<CombatSearchV2TurnPlanProbeCandidateSelectionAuditReport>,
    pub coverage_groups: Vec<CombatSearchV2TurnPlanProbeCoverageGroupAuditReport>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeCandidateSelectionAuditReport {
    pub preselection_rank: usize,
    pub selected_plan_index: Option<usize>,
    pub outcome: &'static str,
    pub drop_reason: Option<&'static str>,
    pub bucket: &'static str,
    pub action_keys: Vec<String>,
    pub coverage_key: CombatSearchV2TurnPlanProbeCoverageKeyReport,
    pub coverage_signature: CombatSearchV2TurnPlanProbeCoverageSignatureReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeCoverageGroupAuditReport {
    pub bucket: &'static str,
    pub coverage_key: CombatSearchV2TurnPlanProbeCoverageKeyReport,
    pub preselection_count: usize,
    pub selected_count: usize,
    pub bucket_cap_dropped_count: usize,
    pub max_end_states_dropped_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeCoverageKeyReport {
    pub damage: &'static str,
    pub block: &'static str,
    pub debuff: &'static str,
    pub setup: &'static str,
    pub resource: &'static str,
    pub risk: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeCoverageSignatureReport {
    pub action_count: usize,
    pub cards_played: usize,
    pub attacks_played: usize,
    pub skills_played: usize,
    pub powers_played: usize,
    pub potions_used: usize,
    pub damage_done: i32,
    pub block_gained_proxy: i32,
    pub enemy_vulnerable_added: i32,
    pub enemy_weak_added: i32,
    pub enemy_strength_down_added: i32,
    pub player_strength_gain: i32,
    pub player_temporary_strength_gain: i32,
    pub energy_spent_proxy: i32,
    pub hand_delta: i32,
    pub draw_delta: i32,
    pub discard_delta: i32,
    pub exhaust_delta: i32,
    pub queued_cards_delta: i32,
    pub player_hp_lost: i32,
    pub reactive_player_hp_loss: i32,
    pub reactive_forced_turn_end_actions: usize,
    pub pending_choice_steps: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeActionMaskReport {
    pub data_role: &'static str,
    pub availability: &'static str,
    pub complete_legal_mask: bool,
    pub legal_action_count: usize,
    pub candidate_eligible_action_count: usize,
    pub equivalence_representative_action_count: usize,
    pub preselection_first_action_count: usize,
    pub potion_policy: &'static str,
    pub legal_actions: Vec<CombatSearchV2TurnPlanProbeActionReport>,
    pub candidate_eligible_actions: Vec<CombatSearchV2TurnPlanProbeActionReport>,
    pub equivalence_representative_actions: Vec<CombatSearchV2TurnPlanProbeActionReport>,
    pub preselection_first_actions: Vec<CombatSearchV2TurnPlanProbeActionReport>,
    pub preselection_first_action_summaries:
        Vec<CombatSearchV2TurnPlanProbeFirstActionSummaryReport>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeFirstActionSummaryReport {
    pub action: CombatSearchV2TurnPlanProbeActionReport,
    pub plan_count: usize,
    pub bucket_counts: BTreeMap<&'static str, usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeActionReport {
    pub action_id: usize,
    pub action_key: String,
    pub action_debug: String,
    pub input: ClientInput,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeCandidateReport {
    pub plan_index: usize,
    pub bucket: &'static str,
    pub stop_reason: &'static str,
    pub outcome_class: &'static str,
    pub survival_bucket: &'static str,
    pub progress_bucket: &'static str,
    pub action_count: usize,
    pub first_action_key: Option<String>,
    pub action_keys: Vec<String>,
    pub actions: Vec<CombatSearchV2ActionTrace>,
    pub action_facts: Vec<CombatSearchV2ActionFacts>,
    pub steps: Vec<CombatSearchV2TurnPlanProbeStepReport>,
    pub eval_final_hp: i32,
    pub eval_risk_margin: i32,
    pub eval_enemy_progress: i32,
    pub end_state: CombatSearchV2StateSummary,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TurnPlanProbeStepReport {
    pub step_index: usize,
    pub action: CombatSearchV2ActionTrace,
    pub action_facts: CombatSearchV2ActionFacts,
    pub exact_state_hash_kind: &'static str,
    pub state_before_exact_state_hash: String,
    pub state_after_exact_state_hash: String,
    pub state_before: CombatSearchV2StateSummary,
    pub state_after: CombatSearchV2StateSummary,
    pub engine_steps: usize,
}
