use serde::Serialize;

use super::super::super::super::state_abstraction::{
    StateAbstractionRevealGate, StateDivergenceKind,
};

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnSequence {
    pub grouping_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub states_observed: u64,
    pub non_empty_prefix_states: u64,
    pub grouped_prefix_states: u64,
    pub unordered_sequence_groups: usize,
    pub groups_with_order_variants: usize,
    pub same_effect_order_variant_groups: usize,
    pub order_sensitive_groups: usize,
    pub max_ordered_variants_per_group: usize,
    pub max_effect_variants_per_group: usize,
    pub max_prefix_length: usize,
    pub max_legal_actions_after_prefix: usize,
    pub order_sensitive_divergence_histogram:
        Vec<CombatSearchV2DiagnosticsTurnSequenceDivergenceCount>,
    pub discard_order_shadow_audit: CombatSearchV2DiagnosticsDiscardOrderShadowAudit,
    pub largest_groups: Vec<CombatSearchV2DiagnosticsTurnSequenceGroupSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsDiscardOrderShadowAudit {
    pub audit_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub candidate_groups: usize,
    pub candidate_states: u64,
    pub static_immediate_safe_groups: usize,
    pub static_immediate_safe_states: u64,
    pub exact_rollout_verified_groups: usize,
    pub proof_pruning_enabled: bool,
    pub reveal_gate: StateAbstractionRevealGate,
    pub one_step_exact_policy: &'static str,
    pub one_step_exact_stored_group_limit: usize,
    pub one_step_exact_sample_limit_groups: usize,
    pub one_step_exact_sample_limit_actions_per_group: usize,
    pub one_step_exact_checked_groups: usize,
    pub one_step_exact_sample_verified_groups: usize,
    pub one_step_exact_blocked_groups: usize,
    pub one_step_exact_checked_actions: usize,
    pub one_step_exact_verified_actions: usize,
    pub one_step_exact_blocked_actions: usize,
    pub sample_limit: usize,
    pub samples: Vec<CombatSearchV2DiagnosticsDiscardOrderShadowAuditSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsDiscardOrderShadowAuditSample {
    pub origin_key: String,
    pub unordered_key_preview: String,
    pub states: u64,
    pub max_prefix_length: usize,
    pub ordered_variants: usize,
    pub effect_variants: usize,
    pub max_legal_actions: usize,
    pub first_divergence_path: Option<&'static str>,
    pub reveal_gate: StateAbstractionRevealGate,
    pub one_step_exact_status: &'static str,
    pub one_step_exact_checked_actions: usize,
    pub one_step_exact_verified_actions: usize,
    pub one_step_exact_blocked_actions: usize,
    pub one_step_exact_blocking_action_key: Option<String>,
    pub one_step_exact_blocking_divergence_kind: Option<StateDivergenceKind>,
    pub one_step_exact_blocking_path: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnSequenceDivergenceCount {
    pub kind: StateDivergenceKind,
    pub first_divergence_path: Option<&'static str>,
    pub guessed_reveal_gate: StateAbstractionRevealGate,
    pub groups: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2DiagnosticsTurnSequenceGroupSample {
    pub group_class: String,
    pub origin_key: String,
    pub unordered_key_preview: String,
    pub states: u64,
    pub max_prefix_length: usize,
    pub ordered_variants: usize,
    pub effect_variants: usize,
    pub max_legal_actions: usize,
    pub divergence_kind: StateDivergenceKind,
    pub first_divergence_path: Option<&'static str>,
    pub guessed_reveal_gate: StateAbstractionRevealGate,
    pub ordered_samples: Vec<String>,
}
