use serde::Serialize;

use super::super::super::state_abstraction::{
    StateAbstractionBoundaryId, StateAbstractionConsumer, StateAbstractionSoundnessLevel,
};
#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsExpansion {
    pub grouping_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub states_observed: u64,
    pub total_atomic_actions: u64,
    pub total_fanout_groups: u64,
    pub fanout_groups_avg: f64,
    pub fanout_groups_max: usize,
    pub max_group_size: usize,
    pub action_kind_counts: Vec<CombatSearchV2DiagnosticsActionKindCount>,
    pub largest_groups: Vec<CombatSearchV2DiagnosticsActionGroupSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsActionKindCount {
    pub kind: String,
    pub atomic_actions: u64,
    pub fanout_groups: u64,
    pub max_group_size: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsActionGroupSample {
    pub observed_at_state_query: u64,
    pub kind: String,
    pub group_key: String,
    pub atomic_actions: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTargetFanout {
    pub grouping_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub states_observed: u64,
    pub targeted_actions_total: u64,
    pub target_fanout_groups_total: u64,
    pub multi_target_fanout_groups: u64,
    pub avg_targets_per_group: f64,
    pub max_targets_per_group: usize,
    pub lethal_target_groups: u64,
    pub unique_lethal_target_groups: u64,
    pub uniform_damage_groups: u64,
    pub max_target_hp_span: i32,
    pub group_kind_counts: Vec<CombatSearchV2DiagnosticsTargetFanoutKindCount>,
    pub largest_target_fanouts: Vec<CombatSearchV2DiagnosticsTargetFanoutSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTargetFanoutKindCount {
    pub kind: String,
    pub groups: u64,
    pub actions: u64,
    pub multi_target_groups: u64,
    pub lethal_target_groups: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTargetFanoutSample {
    pub observed_at_state_query: u64,
    pub kind: String,
    pub source_key: String,
    pub target_count: usize,
    pub lethal_targets: usize,
    pub min_target_hp_with_block: i32,
    pub max_target_hp_with_block: i32,
    pub target_hp_span: i32,
    pub min_damage_hint: i32,
    pub max_damage_hint: i32,
    pub first_action_key: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsEquivalence {
    pub equivalence_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub states_observed: u64,
    pub states_compressed: u64,
    pub atomic_actions_in: u64,
    pub representative_actions_out: u64,
    pub actions_removed: u64,
    pub removed_action_ratio: f64,
    pub max_group_size: usize,
    pub group_kind_counts: Vec<CombatSearchV2DiagnosticsEquivalenceKindCount>,
    pub largest_groups: Vec<CombatSearchV2DiagnosticsEquivalenceGroupSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsEquivalenceKindCount {
    pub kind: String,
    pub boundary_id: StateAbstractionBoundaryId,
    pub soundness: StateAbstractionSoundnessLevel,
    pub allowed_consumers: Vec<StateAbstractionConsumer>,
    pub groups: u64,
    pub actions_in: u64,
    pub actions_removed: u64,
    pub max_group_size: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsEquivalenceGroupSample {
    pub observed_at_state_query: u64,
    pub kind: String,
    pub boundary_id: StateAbstractionBoundaryId,
    pub soundness: StateAbstractionSoundnessLevel,
    pub allowed_consumers: Vec<StateAbstractionConsumer>,
    pub equivalence_key: String,
    pub representative_original_action_id: usize,
    pub removed_original_action_ids: Vec<usize>,
    pub group_size: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsOrdering {
    pub ordering_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub states_observed: u64,
    pub states_reordered: u64,
    pub reordered_state_ratio: f64,
    pub total_actions_observed: u64,
    pub action_effect_actions: u64,
    pub attack_retaliation_actions: u64,
    pub attack_retaliation_trigger_count_hint: u64,
    pub attack_retaliation_raw_player_damage_hint: i64,
    pub attack_retaliation_player_block_loss_hint: i64,
    pub attack_retaliation_player_hp_loss_hint: i64,
    pub max_attack_retaliation_player_hp_loss_hint: i32,
    pub phase_action_hint_actions: u64,
    pub root_action_prior_scored_states: u64,
    pub root_action_prior_scored_actions: u64,
    pub max_position_shift: usize,
    pub avg_position_shift: f64,
    pub action_role_counts: Vec<CombatSearchV2DiagnosticsActionRoleCount>,
    pub largest_reorders: Vec<CombatSearchV2DiagnosticsOrderingSample>,
    pub action_effect_samples: Vec<CombatSearchV2DiagnosticsActionEffectSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsActionRoleCount {
    pub role: String,
    pub actions: u64,
    pub first_actions: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsOrderingSample {
    pub observed_at_state_query: u64,
    pub action_count: usize,
    pub max_position_shift: usize,
    pub first_role: String,
    pub first_original_action_id: usize,
    pub first_action_key: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsActionEffectSample {
    pub observed_at_state_query: u64,
    pub original_action_id: usize,
    pub ordered_index: usize,
    pub role: String,
    pub action_key: String,
    pub direct: CombatSearchV2DiagnosticsActionEffectDirect,
    pub reactive: CombatSearchV2DiagnosticsActionEffectReactive,
    pub access: CombatSearchV2DiagnosticsActionEffectAccess,
    pub derived: CombatSearchV2DiagnosticsActionEffectDerived,
    pub phase: CombatSearchV2DiagnosticsActionPhase,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsActionPhase {
    pub awakened_one_strength_transition: bool,
    pub temporary_strength_down: i32,
    pub convertible_positive_strength: i32,
    pub remaining_damage_upper_bound: i32,
    pub phase_one_hp_with_block: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsActionEffectDirect {
    pub persistent_enemy_strength_down: i32,
    pub temporary_enemy_strength_down: i32,
    pub visible_attack_mitigation_hint: i32,
    pub enemy_weak: i32,
    pub enemy_vulnerable: i32,
    pub enemy_strength_gain: i32,
    pub visible_attack_pressure_hint: i32,
    pub player_strength_gain: i32,
    pub player_temporary_strength_gain: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsActionEffectReactive {
    pub player_hp_loss: i32,
    pub attack_retaliation_trigger_count_hint: usize,
    pub attack_retaliation_raw_player_damage_hint: i32,
    pub attack_retaliation_player_block_loss_hint: i32,
    pub attack_retaliation_player_hp_loss_hint: i32,
    pub player_block: i32,
    pub enemy_damage: i32,
    pub bad_draw_cards: i32,
    pub forced_turn_end: bool,
    pub enemy_strength_gain: i32,
    pub visible_attack_pressure_hint: i32,
    pub enemy_weak: i32,
    pub enemy_vulnerable: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsActionEffectAccess {
    pub declared_draw_cards: i32,
    pub conditional_draw_cards: i32,
    pub total_draw_cards: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsActionEffectDerived {
    pub mitigation_score: i32,
    pub reactive_risk_score: i32,
    pub enemy_scaling_risk_score: i32,
    pub net_mitigation_score: i32,
}
