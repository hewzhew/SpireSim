use crate::ai::noncombat_decision_v1::{
    render_noncombat_decision_record_validation_errors, validate_noncombat_decision_record_v1,
    NonCombatDecisionRecordV1,
};

use super::trace_annotation::RunControlTraceAnnotationV1;

pub(super) fn noncombat_policy_annotation(
    policy_name: &str,
    record: NonCombatDecisionRecordV1,
) -> Result<RunControlTraceAnnotationV1, String> {
    validate_noncombat_policy_record(policy_name, &record)?;
    Ok(RunControlTraceAnnotationV1::NonCombatPolicyDecision { record })
}

pub(super) fn validate_noncombat_policy_record(
    policy_name: &str,
    record: &NonCombatDecisionRecordV1,
) -> Result<(), String> {
    validate_noncombat_decision_record_v1(record).map_err(|errors| {
        format!(
            "{policy_name} produced invalid NonCombatDecisionRecordV1: {}",
            render_noncombat_decision_record_validation_errors(&errors)
        )
    })
}
