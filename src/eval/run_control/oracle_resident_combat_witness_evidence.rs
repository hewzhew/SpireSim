use crate::ai::combat_witness_contract::CombatWitnessEngineV1;
use sts_combat_planner::{
    CombatGuideServiceBias, OracleCombatDeepStateSnapshot, OracleCombatWitnessDiscoverySource,
    OracleCombatWitnessStateProgressSnapshot, TurnOptionAction,
};

use super::oracle_combat_witness_contract::OracleCombatWitnessCandidateDispositionV1;

/// Read-only evidence emitted by one resident combat job.
///
/// Queue sizes and planner snapshots are observations only. This contract
/// carries no live search session, frontier entry, or mutation authority.
#[derive(Clone, Debug)]
pub struct OracleResidentCombatWitnessEvidenceV1 {
    pub witness_engine: CombatWitnessEngineV1,
    pub root_exact_state_hash: String,
    pub guide_service_bias: Option<CombatGuideServiceBias>,
    /// Work charged by earlier search attempts whose frontier was not
    /// serialized and therefore is not present in the current session.
    pub historical_generation_work: u64,
    /// Work represented by the currently resident search frontier.
    pub current_search_generation_work: u64,
    /// Historical plus current work. This is accounting, not resumable depth.
    pub generation_work: u64,
    pub local_generation_work: u64,
    pub discrepancy_generation_work: u64,
    pub engine_steps: usize,
    pub exact_states: usize,
    pub local_exact_states: usize,
    pub discrepancy_exact_states: usize,
    pub applied_action_transitions: usize,
    pub unique_successor_states: usize,
    pub duplicate_exact_successors: usize,
    pub completed_turn_options: usize,
    /// The optional bounded complete-turn seed is distinct from the resident
    /// lazy turn generator and from encounter-authored plan prefixes.
    pub public_plan_turn_proposal_enabled: bool,
    pub public_plan_turn_proposal_attempts: usize,
    pub public_plan_turn_proposal_applied_transitions: usize,
    pub public_plan_turn_proposal_completed_options: usize,
    pub public_plan_turn_proposal_retained_options: usize,
    pub public_plan_turn_proposal_grounding_failures: usize,
    pub retained_state_work: usize,
    pub local_retained_state_work: usize,
    pub discrepancy_retained_state_work: usize,
    pub queued_anchor_entries: usize,
    pub queued_guided_entries: Vec<usize>,
    pub root_state: Option<OracleCombatWitnessStateProgressSnapshot>,
    pub max_player_turn: u32,
    pub deepest_survival_state: Option<OracleCombatDeepStateSnapshot>,
    pub deepest_progress_state: Option<OracleCombatDeepStateSnapshot>,
    pub deepest_survival_actions: Vec<TurnOptionAction>,
    pub deepest_progress_actions: Vec<TurnOptionAction>,
    pub recent_turn_survival_envelope: Vec<OracleCombatDeepStateSnapshot>,
    pub max_path_atomic_depth: usize,
    pub max_completed_turn_options_at_state: usize,
    pub generation_gap_count: usize,
    pub pending_witness_replay: bool,
    pub plan_prefix_proposals: usize,
    pub plan_prefix_proposed_turns: usize,
    pub plan_prefix_proposed_actions: usize,
    pub plan_prefix_proposal_rejections: usize,
    pub local_candidate_final_hp: Option<i32>,
    pub local_candidate_action_count: Option<usize>,
    pub local_candidate_potions_used: Option<u32>,
    pub local_candidate_potion_slots: Option<u64>,
    pub local_candidate_satisfies_satisfaction: Option<bool>,
    pub local_candidate_disposition: Option<OracleCombatWitnessCandidateDispositionV1>,
    pub incumbent_discovery_source: Option<OracleCombatWitnessDiscoverySource>,
    pub incumbent_final_hp: Option<i32>,
    pub incumbent_hp_loss: Option<i32>,
    pub incumbent_action_count: Option<usize>,
    pub incumbent_potions_used: Option<u32>,
    pub incumbent_potion_slots: Option<u64>,
    pub incumbent_satisfies_satisfaction: Option<bool>,
    pub incumbent_ends_quality_refinement: Option<bool>,
    pub potion_spend_requires_satisfaction: bool,
    pub incumbent_revision: u64,
    pub quanta_since_incumbent_improvement: usize,
    pub last_quantum_generation_work: usize,
    pub last_quantum_engine_steps: usize,
    pub last_status: Option<String>,
}
