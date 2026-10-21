use crate::ai::deck_mutation_compiler_v1::{
    CompiledDeckMutationDecisionV1, DeckMutationPlanCandidateV1,
};

use super::types::{
    CandidateDescriptorV1, DataRoleV1, DecisionSiteKindV1, EvidenceBundleV1, EvidenceItemV1,
    EvidenceKindV1, InformationBoundaryV1, InformationClassV1, NonCombatDecisionRecordV1,
    PolicyProvenanceV1, PolicySelectionStatusV1, PolicySelectionV1, PublicActionPlanV1,
    ValueComponentV1, ValueEstimateV1, NONCOMBAT_DECISION_RECORD_SCHEMA_NAME,
    NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
};

impl CompiledDeckMutationDecisionV1 {
    pub fn to_noncombat_decision_record_v1(&self) -> NonCombatDecisionRecordV1 {
        let selected_candidate_id = self.selected_plan.as_ref().map(|plan| plan.plan_id.clone());
        let selection = PolicySelectionV1 {
            status: if selected_candidate_id.is_some() {
                PolicySelectionStatusV1::Selected
            } else if self.candidate_plans.is_empty() {
                PolicySelectionStatusV1::NoCandidates
            } else {
                PolicySelectionStatusV1::Stopped
            },
            selected_candidate_id,
            reason: deck_mutation_selection_reason(self),
            confidence: self
                .selected_plan
                .as_ref()
                .map(|plan| plan.confidence)
                .unwrap_or(0.0),
            selection_mode: "deck_mutation_compiler_v1".to_string(),
        };

        NonCombatDecisionRecordV1 {
            schema_name: NONCOMBAT_DECISION_RECORD_SCHEMA_NAME.to_string(),
            schema_version: NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
            site: DecisionSiteKindV1::RunChoice,
            data_role: DataRoleV1::BehaviorPolicyNotTeacher,
            information_boundary: InformationBoundaryV1::hidden_free(vec![
                InformationClassV1::PublicObservation,
                InformationClassV1::Belief,
            ]),
            provenance: PolicyProvenanceV1 {
                source_policy: "deck_mutation_compiler_v1".to_string(),
                source_schema_name: "CompiledDeckMutationDecisionV1".to_string(),
                source_schema_version: 1,
            },
            candidates: self
                .candidate_plans
                .iter()
                .map(deck_mutation_candidate_descriptor)
                .collect(),
            evidence: EvidenceBundleV1 {
                items: self
                    .candidate_plans
                    .iter()
                    .map(deck_mutation_evidence_item)
                    .collect(),
                assumptions: vec![
                    "deck mutation compiler owns target classification and execution approval"
                        .to_string(),
                    "allowed consumers specify whether a candidate may execute, branch, inspect, or replay"
                        .to_string(),
                    "deck mutation automation is a behavior policy, not a teacher label".to_string(),
                ],
                warnings: Vec::new(),
            },
            values: self
                .candidate_plans
                .iter()
                .map(deck_mutation_value_estimate)
                .collect(),
            selection,
        }
    }
}

fn deck_mutation_candidate_descriptor(plan: &DeckMutationPlanCandidateV1) -> CandidateDescriptorV1 {
    CandidateDescriptorV1 {
        candidate_id: plan.plan_id.clone(),
        site: DecisionSiteKindV1::RunChoice,
        label: plan.step.effect_label.clone(),
        action_plan: PublicActionPlanV1 {
            summary: plan.step.effect_label.clone(),
            command: Some(plan.step.command.clone()),
        },
        information_classes: vec![InformationClassV1::PublicObservation],
        uncertainty_notes: plan.risks.clone(),
    }
}

fn deck_mutation_evidence_item(plan: &DeckMutationPlanCandidateV1) -> EvidenceItemV1 {
    let mut components = vec![
        ValueComponentV1::new("score_hint", plan.score_hint as f32),
        ValueComponentV1::new("confidence", plan.confidence),
        ValueComponentV1::new("representative_count", plan.representative_count as f32),
        ValueComponentV1::new("suppressed_count", plan.suppressed_count as f32),
        ValueComponentV1::new(format!("role_{:?}", plan.role), 1.0),
    ];
    components.extend(
        plan.step
            .cards
            .iter()
            .map(|card| ValueComponentV1::new(format!("target_{:?}", card.target_class), 1.0)),
    );

    EvidenceItemV1 {
        kind: EvidenceKindV1::PolicyGate,
        candidate_id: Some(plan.plan_id.clone()),
        label: format!(
            "deck mutation role={:?} allowed execute={} branch={} inspect={}",
            plan.role,
            plan.allowed_consumers.execute_autopilot,
            plan.allowed_consumers.branch_active,
            plan.allowed_consumers.inspect
        ),
        information_class: InformationClassV1::Belief,
        components,
    }
}

fn deck_mutation_value_estimate(plan: &DeckMutationPlanCandidateV1) -> ValueEstimateV1 {
    ValueEstimateV1 {
        candidate_id: plan.plan_id.clone(),
        mean_utility: plan.score_hint as f32,
        risk_adjusted_utility: if plan.allowed_consumers.execute_autopilot {
            plan.score_hint as f32
        } else {
            plan.score_hint as f32 - 10_000.0
        },
        confidence: plan.confidence,
        components: vec![
            ValueComponentV1::new("score_hint", plan.score_hint as f32),
            ValueComponentV1::new(
                "execute_allowed",
                plan.allowed_consumers.execute_autopilot as u8 as f32,
            ),
            ValueComponentV1::new(
                "branch_allowed",
                plan.allowed_consumers.branch_active as u8 as f32,
            ),
        ],
        evidence_refs: Vec::new(),
    }
}

fn deck_mutation_selection_reason(decision: &CompiledDeckMutationDecisionV1) -> String {
    decision
        .selected_plan
        .as_ref()
        .map(|plan| plan.reasons.join("; "))
        .filter(|reason| !reason.is_empty())
        .unwrap_or_else(|| "deck mutation compiler did not approve an executable plan".to_string())
}
