use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct StrategicAuditReport {
    pub strategic_kernel_bypass_count: usize,
    pub candidate_without_delta_count: usize,
    pub branch_without_signature_count: usize,
    pub candidate_count: usize,
    pub delta_count: usize,
    pub notes: Vec<String>,
}

pub fn audit_delta_coverage(candidate_count: usize, delta_count: usize) -> StrategicAuditReport {
    let candidate_without_delta_count = candidate_count.saturating_sub(delta_count);
    let mut notes = Vec::new();
    if candidate_without_delta_count > 0 {
        notes.push(format!(
            "{candidate_without_delta_count} candidate(s) did not produce CandidateDelta"
        ));
    }
    StrategicAuditReport {
        strategic_kernel_bypass_count: 0,
        candidate_without_delta_count,
        branch_without_signature_count: 0,
        candidate_count,
        delta_count,
        notes,
    }
}
