use serde::Serialize;

use super::enums::*;

#[derive(Clone, Debug, Serialize)]
pub struct StateAbstractionBoundarySpec {
    pub id: StateAbstractionBoundaryId,
    pub name: &'static str,
    pub scope: StateAbstractionBoundaryScope,
    pub soundness: StateAbstractionSoundnessLevel,
    pub allowed_consumers: Vec<StateAbstractionConsumer>,
    pub ignored_fields: Vec<&'static str>,
    pub reveal_gates: Vec<StateAbstractionRevealGate>,
    pub audit_required: bool,
    pub notes: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct StateAbstractionGateReport {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub policy: &'static str,
    pub registered_boundaries: Vec<StateAbstractionBoundarySpec>,
    pub case_count: usize,
    pub divergence_histogram: Vec<StateAbstractionHistogramEntry>,
    pub divergence_group_histogram: Vec<StateAbstractionHistogramEntry>,
    pub divergence_path_histogram: Vec<StateAbstractionHistogramEntry>,
    pub latent_debt_histogram: Vec<StateAbstractionHistogramEntry>,
    pub latent_debt_group_histogram: Vec<StateAbstractionHistogramEntry>,
    pub candidate_level_histogram: Vec<StateAbstractionHistogramEntry>,
    pub candidate_level_group_histogram: Vec<StateAbstractionHistogramEntry>,
    pub recommended_consumer_histogram: Vec<StateAbstractionHistogramEntry>,
    pub reveal_gate_histogram: Vec<StateAbstractionHistogramEntry>,
    pub reveal_gate_group_histogram: Vec<StateAbstractionHistogramEntry>,
    pub identity_audit: StateAbstractionIdentityAuditReport,
    pub cases: Vec<StateAbstractionCaseReport>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StateAbstractionHistogramEntry {
    pub key: &'static str,
    pub cases: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct StateAbstractionCaseReport {
    pub case_id: String,
    pub boundary_id: StateAbstractionBoundaryId,
    pub soundness: StateAbstractionSoundnessLevel,
    pub allowed_consumers: Vec<StateAbstractionConsumer>,
    pub divergence_kind: StateDivergenceKind,
    pub first_divergence_path: Option<&'static str>,
    pub public_observation_changed: Option<bool>,
    pub legal_actions_changed: Option<bool>,
    pub terminal_class_changed: Option<bool>,
    pub guessed_reveal_gate: StateAbstractionRevealGate,
    pub latent_debt_kind: StateAbstractionLatentDebtKind,
    pub candidate_level: StateAbstractionCandidateLevel,
    pub recommended_consumer: StateAbstractionConsumer,
    pub pruning_allowed: bool,
    pub exact_branch_removal_allowed: bool,
    pub same_effect_turn_sequence_groups: usize,
    pub order_sensitive_turn_sequence_groups: usize,
    pub turn_sequence_divergence_histogram: Vec<StateAbstractionCaseDivergenceCount>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StateAbstractionIdentityAuditReport {
    pub audit_policy: &'static str,
    pub behavioral_effect: &'static str,
    pub status: &'static str,
    pub candidate_cases: usize,
    pub candidate_groups: usize,
    pub proof_pruning_enabled: bool,
    pub exact_branch_removal_allowed: bool,
    pub blocked_reason: &'static str,
    pub required_checks: Vec<&'static str>,
    pub samples: Vec<StateAbstractionIdentityAuditSample>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StateAbstractionIdentityAuditSample {
    pub case_id: String,
    pub groups: usize,
    pub first_divergence_path: Option<&'static str>,
    pub guessed_reveal_gate: StateAbstractionRevealGate,
    pub required_next_check: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct StateAbstractionCaseDivergenceCount {
    pub kind: StateDivergenceKind,
    pub first_divergence_path: Option<&'static str>,
    pub guessed_reveal_gate: StateAbstractionRevealGate,
    pub groups: usize,
}

#[derive(Clone, Debug)]
pub struct StateAbstractionCaseInput<'a> {
    pub case_id: &'a str,
    pub same_effect_turn_sequence_groups: usize,
    pub order_sensitive_turn_sequence_groups: usize,
    pub turn_sequence_divergence_histogram: Vec<StateAbstractionDivergenceInput>,
}

#[derive(Clone, Debug)]
pub struct StateAbstractionDivergenceInput {
    pub kind: StateDivergenceKind,
    pub first_divergence_path: Option<&'static str>,
    pub guessed_reveal_gate: StateAbstractionRevealGate,
    pub groups: usize,
}
