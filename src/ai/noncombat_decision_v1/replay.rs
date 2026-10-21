use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::types::{DecisionSiteKindV1, NonCombatDecisionRecordV1, PolicySelectionStatusV1};
use super::validation::{
    render_noncombat_decision_record_validation_errors, validate_noncombat_decision_record_v1,
};

pub const NONCOMBAT_DECISION_REPLAY_SCHEMA_NAME: &str = "NonCombatDecisionReplayReportV1";
pub const NONCOMBAT_DECISION_REPLAY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NonCombatDecisionReplayReportV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub label_role: String,
    pub trainable_as_action_label: bool,
    pub policy_quality_claim: bool,
    pub site: DecisionSiteKindV1,
    pub old_source_policy: String,
    pub new_source_policy: String,
    pub candidate_set: NonCombatReplayCandidateSetV1,
    pub selection_changed: bool,
    pub old_selection_status: PolicySelectionStatusV1,
    pub new_selection_status: PolicySelectionStatusV1,
    pub old_selected_candidate_id: Option<String>,
    pub new_selected_candidate_id: Option<String>,
    pub value_deltas: Vec<NonCombatReplayValueDeltaV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NonCombatReplayCandidateSetV1 {
    pub status: NonCombatReplayCandidateSetStatusV1,
    pub old_order_hash: String,
    pub new_order_hash: String,
    pub added_candidate_ids: Vec<String>,
    pub removed_candidate_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NonCombatReplayCandidateSetStatusV1 {
    Unchanged,
    OrderChanged,
    SetChanged,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NonCombatReplayValueDeltaV1 {
    pub candidate_id: String,
    pub old_mean_utility: f32,
    pub new_mean_utility: f32,
    pub mean_utility_delta: f32,
    pub old_risk_adjusted_utility: f32,
    pub new_risk_adjusted_utility: f32,
    pub risk_adjusted_utility_delta: f32,
    pub old_confidence: f32,
    pub new_confidence: f32,
    pub confidence_delta: f32,
}

pub fn compare_noncombat_decision_records_v1(
    old_record: &NonCombatDecisionRecordV1,
    new_record: &NonCombatDecisionRecordV1,
) -> Result<NonCombatDecisionReplayReportV1, String> {
    validate_record("old_record", old_record)?;
    validate_record("new_record", new_record)?;
    if old_record.site != new_record.site {
        return Err(format!(
            "cannot compare decision records from different sites: old={:?} new={:?}",
            old_record.site, new_record.site
        ));
    }

    let candidate_set = compare_candidate_sets(old_record, new_record);
    let selection_changed = old_record.selection.status != new_record.selection.status
        || old_record.selection.selected_candidate_id != new_record.selection.selected_candidate_id
        || old_record.selection.reason != new_record.selection.reason;

    Ok(NonCombatDecisionReplayReportV1 {
        schema_name: NONCOMBAT_DECISION_REPLAY_SCHEMA_NAME.to_string(),
        schema_version: NONCOMBAT_DECISION_REPLAY_SCHEMA_VERSION,
        label_role: "diagnostic_not_teacher_label".to_string(),
        trainable_as_action_label: false,
        policy_quality_claim: false,
        site: old_record.site,
        old_source_policy: old_record.provenance.source_policy.clone(),
        new_source_policy: new_record.provenance.source_policy.clone(),
        candidate_set,
        selection_changed,
        old_selection_status: old_record.selection.status,
        new_selection_status: new_record.selection.status,
        old_selected_candidate_id: old_record.selection.selected_candidate_id.clone(),
        new_selected_candidate_id: new_record.selection.selected_candidate_id.clone(),
        value_deltas: compare_values(old_record, new_record),
    })
}

fn validate_record(label: &str, record: &NonCombatDecisionRecordV1) -> Result<(), String> {
    validate_noncombat_decision_record_v1(record).map_err(|errors| {
        format!(
            "{label} is not a valid NonCombatDecisionRecordV1: {}",
            render_noncombat_decision_record_validation_errors(&errors)
        )
    })
}

fn compare_candidate_sets(
    old_record: &NonCombatDecisionRecordV1,
    new_record: &NonCombatDecisionRecordV1,
) -> NonCombatReplayCandidateSetV1 {
    let old_ids = old_record
        .candidates
        .iter()
        .map(|candidate| candidate.candidate_id.clone())
        .collect::<Vec<_>>();
    let new_ids = new_record
        .candidates
        .iter()
        .map(|candidate| candidate.candidate_id.clone())
        .collect::<Vec<_>>();
    let old_set = old_ids.iter().cloned().collect::<BTreeSet<_>>();
    let new_set = new_ids.iter().cloned().collect::<BTreeSet<_>>();

    let status = if old_set != new_set {
        NonCombatReplayCandidateSetStatusV1::SetChanged
    } else if old_ids != new_ids {
        NonCombatReplayCandidateSetStatusV1::OrderChanged
    } else {
        NonCombatReplayCandidateSetStatusV1::Unchanged
    };

    NonCombatReplayCandidateSetV1 {
        status,
        old_order_hash: ordered_id_hash(&old_ids),
        new_order_hash: ordered_id_hash(&new_ids),
        added_candidate_ids: new_set.difference(&old_set).cloned().collect(),
        removed_candidate_ids: old_set.difference(&new_set).cloned().collect(),
    }
}

fn compare_values(
    old_record: &NonCombatDecisionRecordV1,
    new_record: &NonCombatDecisionRecordV1,
) -> Vec<NonCombatReplayValueDeltaV1> {
    let new_by_id = new_record
        .values
        .iter()
        .map(|value| (value.candidate_id.as_str(), value))
        .collect::<BTreeMap<_, _>>();

    old_record
        .values
        .iter()
        .filter_map(|old| {
            let new = new_by_id.get(old.candidate_id.as_str())?;
            Some(NonCombatReplayValueDeltaV1 {
                candidate_id: old.candidate_id.clone(),
                old_mean_utility: old.mean_utility,
                new_mean_utility: new.mean_utility,
                mean_utility_delta: new.mean_utility - old.mean_utility,
                old_risk_adjusted_utility: old.risk_adjusted_utility,
                new_risk_adjusted_utility: new.risk_adjusted_utility,
                risk_adjusted_utility_delta: new.risk_adjusted_utility - old.risk_adjusted_utility,
                old_confidence: old.confidence,
                new_confidence: new.confidence,
                confidence_delta: new.confidence - old.confidence,
            })
        })
        .collect()
}

fn ordered_id_hash(ids: &[String]) -> String {
    let mut bytes = Vec::new();
    for id in ids {
        bytes.extend_from_slice(id.as_bytes());
        bytes.push(0);
    }
    super::hash::hash_bytes(&bytes)
}
