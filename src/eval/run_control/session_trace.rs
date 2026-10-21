use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ai::noncombat_decision_v1::NonCombatOutcomeAttachmentV1;
use crate::ai::planner_core::{LegalCandidateSet, PlannerObservation, PlannerOutcomeAttachment};

use super::planner_capture::validate_planner_trace_payloads;
use super::session::RunControlSession;
use super::trace_annotation::{
    validate_run_control_trace_annotations_v1, RunControlTraceAnnotationV1,
};
use super::transition_report::ActionResult;
use super::view_model::{CandidateResolution, DecisionCandidate};

pub const SESSION_TRACE_SCHEMA_NAME: &str = "SessionTraceV1";
pub const SESSION_TRACE_SCHEMA_VERSION: u32 = 16;

/// Typed schema-v16 trace data with data-only compatibility through v6.
///
/// The raw command fields are retained only because they are part of the
/// persisted schema. This module never parses or executes them.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTraceV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub label_role: String,
    pub trainable_as_action_label: bool,
    pub policy_quality_claim: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<SessionTraceLineageV1>,
    pub run_config: SessionTraceRunConfigV1,
    pub steps: Vec<SessionTraceStepV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundary_records: Vec<SessionTraceBoundaryRecordV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub noncombat_outcome_attachments: Vec<NonCombatOutcomeAttachmentV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planner_observations: Vec<PlannerObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planner_legal_candidate_sets: Vec<LegalCandidateSet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planner_outcome_attachments: Vec<PlannerOutcomeAttachment>,
    pub artifact_refs: Vec<SessionTraceArtifactRefV1>,
}

impl SessionTraceV1 {
    pub fn new(session: &RunControlSession) -> Self {
        Self {
            schema_name: SESSION_TRACE_SCHEMA_NAME.to_string(),
            schema_version: SESSION_TRACE_SCHEMA_VERSION,
            label_role: "diagnostic_not_teacher_label".to_string(),
            trainable_as_action_label: false,
            policy_quality_claim: false,
            lineage: None,
            run_config: SessionTraceRunConfigV1::from_session(session),
            steps: Vec::new(),
            boundary_records: Vec::new(),
            noncombat_outcome_attachments: Vec::new(),
            planner_observations: Vec::new(),
            planner_legal_candidate_sets: Vec::new(),
            planner_outcome_attachments: Vec::new(),
            artifact_refs: Vec::new(),
        }
    }
}

/// Loads historical SessionTrace payloads as data only.
///
/// Historical traces remain available to exporters without restoring the
/// retired command parser, recorder, or replay executor.
pub fn load_session_trace_v1(path: &Path) -> Result<SessionTraceV1, String> {
    let payload = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let trace = serde_json::from_str::<SessionTraceV1>(&payload).map_err(|err| err.to_string())?;
    validate_loaded_session_trace_v1(&trace)?;
    Ok(trace)
}

fn validate_loaded_session_trace_v1(trace: &SessionTraceV1) -> Result<(), String> {
    if trace.schema_name != SESSION_TRACE_SCHEMA_NAME {
        return Err(format!(
            "unsupported trace schema '{}', expected {SESSION_TRACE_SCHEMA_NAME}",
            trace.schema_name
        ));
    }
    if !(6..=SESSION_TRACE_SCHEMA_VERSION).contains(&trace.schema_version) {
        return Err(format!(
            "unsupported trace schema version {}, expected 6..={SESSION_TRACE_SCHEMA_VERSION}",
            trace.schema_version
        ));
    }
    for step in &trace.steps {
        validate_run_control_trace_annotations_v1(&step.annotations)
            .map_err(|error| format!("trace step {} {error}", step.step_index))?;
    }
    for boundary in &trace.boundary_records {
        validate_run_control_trace_annotations_v1(&boundary.annotations)
            .map_err(|error| format!("boundary record {} {error}", boundary.record_index))?;
    }
    validate_planner_trace_payloads(trace)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTraceLineageV1 {
    pub role: SessionTraceLineageRoleV1,
    pub parent_trace_path: String,
    pub parent_trace_hash: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTraceLineageRoleV1 {
    Continuation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTraceRunConfigV1 {
    pub seed: u64,
    pub ascension_level: u8,
    pub player_class: String,
    pub final_act: bool,
    pub reward_automation: SessionTraceRewardAutomationV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTraceRewardAutomationV1 {
    pub claim_gold: bool,
    pub claim_potion_with_empty_slot: bool,
    #[serde(default = "default_true")]
    pub claim_safe_relic_without_sapphire_key: bool,
}

impl SessionTraceRunConfigV1 {
    fn from_session(session: &RunControlSession) -> Self {
        Self {
            seed: session.run_state.seed,
            ascension_level: session.run_state.ascension_level,
            player_class: session.run_state.player_class.to_string(),
            final_act: session.run_state.is_final_act_available,
            reward_automation: SessionTraceRewardAutomationV1 {
                claim_gold: session.reward_automation.claim_gold,
                claim_potion_with_empty_slot: session
                    .reward_automation
                    .claim_potion_with_empty_slot,
                claim_safe_relic_without_sapphire_key: session
                    .reward_automation
                    .claim_safe_relic_without_sapphire_key,
            },
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTraceStepV1 {
    pub step_index: usize,
    #[serde(default)]
    pub step_source: SessionTraceStepSourceV1,
    pub raw_command_line: String,
    pub decision_step_before: u64,
    pub decision_step_after: u64,
    pub screen_title: String,
    pub decision_kind: String,
    pub before: SessionTraceBoundaryFingerprintV1,
    pub after: SessionTraceBoundaryFingerprintV1,
    pub visible_candidates: Vec<SessionTraceCandidateV1>,
    pub selected_candidate: Option<SessionTraceCandidateV1>,
    pub selection_resolution: SessionTraceSelectionResolution,
    pub annotations: Vec<RunControlTraceAnnotationV1>,
    pub action_result: ActionResult,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTraceBoundaryRecordV1 {
    pub record_index: usize,
    pub raw_command_line: String,
    pub decision_step: u64,
    pub screen_title: String,
    pub decision_kind: String,
    pub boundary: SessionTraceBoundaryFingerprintV1,
    pub annotations: Vec<RunControlTraceAnnotationV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[derive(Default)]
pub enum SessionTraceStepSourceV1 {
    #[default]
    ManualOrAutomation,
    ReplayVerified {
        source_trace_step_index: usize,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTraceBoundaryFingerprintV1 {
    pub decision_step: u64,
    pub engine_state: String,
    pub active_combat_engine_state: Option<String>,
    pub screen_title: String,
    pub decision_kind: String,
    pub decision_label: String,
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub boss: String,
    pub candidate_count: usize,
    pub candidate_set_hash: String,
    pub candidate_order_hash: String,
    pub combat: Option<SessionTraceCombatFingerprintV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTraceCombatFingerprintV1 {
    pub public_observation_hash: String,
    pub legal_candidate_set_hash: String,
    pub legal_candidate_order_hash: String,
    pub exact_state_hash: String,
    pub stable_outcome_hash: Option<String>,
    pub rng_boundary_status: crate::eval::fingerprint::RngFingerprintStatus,
    pub rng_boundary_stream_count: usize,
    pub rng_boundary_digest: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTraceCandidateV1 {
    pub id: String,
    pub label: String,
    /// Historical field name retained for schema compatibility.
    pub command: String,
    pub note: Option<String>,
    pub executable: bool,
    pub resolution: Option<CandidateResolution>,
}

impl From<DecisionCandidate> for SessionTraceCandidateV1 {
    fn from(candidate: DecisionCandidate) -> Self {
        Self {
            id: candidate.id,
            label: candidate.label,
            command: candidate.action.summary(),
            note: candidate.note,
            executable: candidate.action.executable_action().is_some(),
            resolution: candidate.resolution,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTraceSelectionResolution {
    ResolvedByVisibleId,
    ResolvedSingleVisibleCandidate,
    ResolvedByUniqueLabel,
    AmbiguousLabel,
    Unresolved,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SessionTraceArtifactRefV1 {
    pub raw_command_line: String,
    pub decision_step: u64,
    pub artifact_kind: SessionTraceArtifactKind,
    pub capture_path: Option<String>,
    pub baseline_path: Option<String>,
    pub search_evidence_path: Option<String>,
    pub benchmark_manifest_path: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTraceArtifactKind {
    CombatCaptureCase,
    CombatBaselineCase,
    CombatSearchEvidence,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_trace_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "sts-session-trace-{label}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn historical_trace_loader_round_trips_data_without_replay() {
        let session = RunControlSession::new(Default::default());
        let trace = SessionTraceV1::new(&session);
        let path = temp_trace_path("round-trip");
        fs::write(&path, serde_json::to_vec_pretty(&trace).unwrap()).unwrap();

        let loaded = load_session_trace_v1(&path).expect("historical trace loads");

        assert_eq!(loaded, trace);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn historical_trace_loader_rejects_wrong_schema() {
        let session = RunControlSession::new(Default::default());
        let mut trace = SessionTraceV1::new(&session);
        trace.schema_name = "NotSessionTrace".to_string();
        let path = temp_trace_path("wrong-schema");
        fs::write(&path, serde_json::to_vec_pretty(&trace).unwrap()).unwrap();

        let error = load_session_trace_v1(&path).expect_err("wrong schema must fail");

        assert!(error.contains("unsupported trace schema"));
        let _ = fs::remove_file(path);
    }
}
