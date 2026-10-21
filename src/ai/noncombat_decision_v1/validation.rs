use std::collections::BTreeSet;

use super::types::{
    DataRoleV1, InformationClassV1, NonCombatDecisionRecordV1, PolicySelectionStatusV1,
    NONCOMBAT_DECISION_RECORD_SCHEMA_NAME, NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NonCombatDecisionRecordValidationErrorV1 {
    pub field: String,
    pub message: String,
}

impl NonCombatDecisionRecordValidationErrorV1 {
    fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

pub fn validate_noncombat_decision_record_v1(
    record: &NonCombatDecisionRecordV1,
) -> Result<(), Vec<NonCombatDecisionRecordValidationErrorV1>> {
    let mut errors = Vec::new();

    validate_schema(record, &mut errors);
    validate_information_boundary(record, &mut errors);
    let candidate_ids = validate_candidates(record, &mut errors);
    validate_evidence(record, &candidate_ids, &mut errors);
    validate_values(record, &candidate_ids, &mut errors);
    validate_selection(record, &candidate_ids, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn render_noncombat_decision_record_validation_errors(
    errors: &[NonCombatDecisionRecordValidationErrorV1],
) -> String {
    if errors.is_empty() {
        return "no validation errors".to_string();
    }
    errors
        .iter()
        .map(|error| format!("{}: {}", error.field, error.message))
        .collect::<Vec<_>>()
        .join("; ")
}

fn validate_schema(
    record: &NonCombatDecisionRecordV1,
    errors: &mut Vec<NonCombatDecisionRecordValidationErrorV1>,
) {
    if record.schema_name != NONCOMBAT_DECISION_RECORD_SCHEMA_NAME {
        errors.push(NonCombatDecisionRecordValidationErrorV1::new(
            "schema_name",
            format!(
                "expected {NONCOMBAT_DECISION_RECORD_SCHEMA_NAME}, got {}",
                record.schema_name
            ),
        ));
    }
    if record.schema_version != NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION {
        errors.push(NonCombatDecisionRecordValidationErrorV1::new(
            "schema_version",
            format!(
                "expected {NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION}, got {}",
                record.schema_version
            ),
        ));
    }
}

fn validate_information_boundary(
    record: &NonCombatDecisionRecordV1,
    errors: &mut Vec<NonCombatDecisionRecordValidationErrorV1>,
) {
    if record.information_boundary.hidden_simulator_state_used {
        errors.push(NonCombatDecisionRecordValidationErrorV1::new(
            "information_boundary.hidden_simulator_state_used",
            "non-combat decision records must remain hidden-free",
        ));
    }
    if !record
        .information_boundary
        .forbidden_inputs
        .contains(&InformationClassV1::HiddenSimulatorState)
    {
        errors.push(NonCombatDecisionRecordValidationErrorV1::new(
            "information_boundary.forbidden_inputs",
            "HiddenSimulatorState must be explicitly forbidden",
        ));
    }
    if record
        .information_boundary
        .allowed_inputs
        .contains(&InformationClassV1::HiddenSimulatorState)
    {
        errors.push(NonCombatDecisionRecordValidationErrorV1::new(
            "information_boundary.allowed_inputs",
            "HiddenSimulatorState must not be allowed",
        ));
    }
}

fn validate_candidates(
    record: &NonCombatDecisionRecordV1,
    errors: &mut Vec<NonCombatDecisionRecordValidationErrorV1>,
) -> BTreeSet<String> {
    let mut candidate_ids = BTreeSet::new();
    for (idx, candidate) in record.candidates.iter().enumerate() {
        if candidate.candidate_id.trim().is_empty() {
            errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                format!("candidates[{idx}].candidate_id"),
                "candidate id must not be empty",
            ));
        } else if !candidate_ids.insert(candidate.candidate_id.clone()) {
            errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                format!("candidates[{idx}].candidate_id"),
                format!("duplicate candidate id {}", candidate.candidate_id),
            ));
        }
        if candidate.site != record.site {
            errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                format!("candidates[{idx}].site"),
                "candidate site must match record site",
            ));
        }
        if candidate.label.trim().is_empty() {
            errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                format!("candidates[{idx}].label"),
                "candidate label must not be empty",
            ));
        }
        if candidate
            .information_classes
            .contains(&InformationClassV1::HiddenSimulatorState)
        {
            errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                format!("candidates[{idx}].information_classes"),
                "candidate evidence must not use hidden simulator state",
            ));
        }
    }
    candidate_ids
}

fn validate_evidence(
    record: &NonCombatDecisionRecordV1,
    candidate_ids: &BTreeSet<String>,
    errors: &mut Vec<NonCombatDecisionRecordValidationErrorV1>,
) {
    for (idx, item) in record.evidence.items.iter().enumerate() {
        if let Some(candidate_id) = item.candidate_id.as_ref() {
            if !candidate_ids.contains(candidate_id) {
                errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                    format!("evidence.items[{idx}].candidate_id"),
                    format!("unknown candidate id {candidate_id}"),
                ));
            }
        }
        if item.information_class == InformationClassV1::HiddenSimulatorState {
            errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                format!("evidence.items[{idx}].information_class"),
                "evidence item must not use hidden simulator state",
            ));
        }
    }
}

fn validate_values(
    record: &NonCombatDecisionRecordV1,
    candidate_ids: &BTreeSet<String>,
    errors: &mut Vec<NonCombatDecisionRecordValidationErrorV1>,
) {
    for (idx, value) in record.values.iter().enumerate() {
        if !candidate_ids.contains(&value.candidate_id) {
            errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                format!("values[{idx}].candidate_id"),
                format!("unknown candidate id {}", value.candidate_id),
            ));
        }
        for evidence_ref in &value.evidence_refs {
            if *evidence_ref >= record.evidence.items.len() {
                errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                    format!("values[{idx}].evidence_refs"),
                    format!("evidence ref {evidence_ref} is out of range"),
                ));
            }
        }
    }
}

fn validate_selection(
    record: &NonCombatDecisionRecordV1,
    candidate_ids: &BTreeSet<String>,
    errors: &mut Vec<NonCombatDecisionRecordValidationErrorV1>,
) {
    match record.selection.status {
        PolicySelectionStatusV1::Selected => {
            let Some(candidate_id) = record.selection.selected_candidate_id.as_ref() else {
                errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                    "selection.selected_candidate_id",
                    "selected records must name a selected candidate",
                ));
                return;
            };
            if !candidate_ids.contains(candidate_id) {
                errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                    "selection.selected_candidate_id",
                    format!("unknown candidate id {candidate_id}"),
                ));
            }
            if record.data_role == DataRoleV1::HumanBoundaryNotTeacher {
                errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                    "selection.status",
                    "human boundary records must stop instead of selecting an action",
                ));
            }
        }
        PolicySelectionStatusV1::Stopped | PolicySelectionStatusV1::NoCandidates => {
            if record.selection.selected_candidate_id.is_some() {
                errors.push(NonCombatDecisionRecordValidationErrorV1::new(
                    "selection.selected_candidate_id",
                    "stopped/no-candidate records must not name a selected candidate",
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::noncombat_decision_v1::types::{
        CandidateDescriptorV1, DecisionSiteKindV1, EvidenceBundleV1, EvidenceItemV1,
        EvidenceKindV1, InformationBoundaryV1, PolicyProvenanceV1, PolicySelectionV1,
        PublicActionPlanV1, ValueEstimateV1,
    };

    #[test]
    fn noncombat_record_validator_accepts_minimal_human_boundary_record() {
        let record = minimal_record(DataRoleV1::HumanBoundaryNotTeacher);

        validate_noncombat_decision_record_v1(&record).expect("record should be valid");
    }

    #[test]
    fn noncombat_record_validator_rejects_hidden_simulator_state() {
        let mut record = minimal_record(DataRoleV1::HumanBoundaryNotTeacher);
        record.information_boundary.hidden_simulator_state_used = true;

        let err = validate_noncombat_decision_record_v1(&record)
            .expect_err("hidden simulator state must be rejected");

        assert!(err
            .iter()
            .any(|error| error.field == "information_boundary.hidden_simulator_state_used"));
    }

    #[test]
    fn noncombat_record_validator_rejects_unknown_candidate_refs() {
        let mut record = minimal_record(DataRoleV1::BehaviorPolicyNotTeacher);
        record.selection.status = PolicySelectionStatusV1::Selected;
        record.selection.selected_candidate_id = Some("missing".to_string());
        record.values[0].candidate_id = "missing".to_string();
        record.evidence.items[0].candidate_id = Some("missing".to_string());

        let err = validate_noncombat_decision_record_v1(&record)
            .expect_err("unknown candidate references must be rejected");

        assert!(err
            .iter()
            .any(|error| error.field == "selection.selected_candidate_id"));
        assert!(err
            .iter()
            .any(|error| error.field == "values[0].candidate_id"));
        assert!(err
            .iter()
            .any(|error| error.field == "evidence.items[0].candidate_id"));
    }

    #[test]
    fn noncombat_record_validator_rejects_human_boundary_selected_action() {
        let mut record = minimal_record(DataRoleV1::HumanBoundaryNotTeacher);
        record.selection.status = PolicySelectionStatusV1::Selected;
        record.selection.selected_candidate_id = Some("shop:leave".to_string());

        let err = validate_noncombat_decision_record_v1(&record)
            .expect_err("human boundary records must not select actions");

        assert!(err.iter().any(|error| error.field == "selection.status"));
    }

    fn minimal_record(data_role: DataRoleV1) -> NonCombatDecisionRecordV1 {
        NonCombatDecisionRecordV1 {
            schema_name: NONCOMBAT_DECISION_RECORD_SCHEMA_NAME.to_string(),
            schema_version: NONCOMBAT_DECISION_RECORD_SCHEMA_VERSION,
            site: DecisionSiteKindV1::Shop,
            data_role,
            information_boundary: InformationBoundaryV1::hidden_free(vec![
                InformationClassV1::PublicObservation,
            ]),
            provenance: PolicyProvenanceV1 {
                source_policy: "test_policy".to_string(),
                source_schema_name: "TestPolicy".to_string(),
                source_schema_version: 1,
            },
            candidates: vec![CandidateDescriptorV1 {
                candidate_id: "shop:leave".to_string(),
                site: DecisionSiteKindV1::Shop,
                label: "Leave shop".to_string(),
                action_plan: PublicActionPlanV1 {
                    summary: "leave shop".to_string(),
                    command: Some("leave".to_string()),
                },
                information_classes: vec![InformationClassV1::PublicObservation],
                uncertainty_notes: Vec::new(),
            }],
            evidence: EvidenceBundleV1 {
                items: vec![EvidenceItemV1 {
                    kind: EvidenceKindV1::CandidateFacts,
                    candidate_id: Some("shop:leave".to_string()),
                    label: "Leave shop".to_string(),
                    information_class: InformationClassV1::PublicObservation,
                    components: Vec::new(),
                }],
                assumptions: Vec::new(),
                warnings: Vec::new(),
            },
            values: vec![ValueEstimateV1 {
                candidate_id: "shop:leave".to_string(),
                mean_utility: 0.0,
                risk_adjusted_utility: 0.0,
                confidence: 0.0,
                components: Vec::new(),
                evidence_refs: vec![0],
            }],
            selection: PolicySelectionV1 {
                status: match data_role {
                    DataRoleV1::BehaviorPolicyNotTeacher => PolicySelectionStatusV1::Selected,
                    DataRoleV1::HumanBoundaryNotTeacher => PolicySelectionStatusV1::Stopped,
                },
                selected_candidate_id: match data_role {
                    DataRoleV1::BehaviorPolicyNotTeacher => Some("shop:leave".to_string()),
                    DataRoleV1::HumanBoundaryNotTeacher => None,
                },
                reason: "test".to_string(),
                confidence: 0.0,
                selection_mode: "test".to_string(),
            },
        }
    }
}
