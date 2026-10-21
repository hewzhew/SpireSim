use serde::{Deserialize, Serialize};

use super::session::{RunControlSession, RunProgressOutcome};
use super::trace_annotation::RunControlTraceAnnotationV1;
use super::transition_report::{render_action_result, ActionResult};
use super::view_model::{CandidateAction, DecisionCandidateKey};
use super::RunProgressStepV1;
use super::{build_decision_surface, render_run_control_state, RunDecisionAction};

pub const RUN_DECISION_TRANSACTION_SCHEMA_NAME: &str = "RunDecisionTransaction";
pub const RUN_DECISION_TRANSACTION_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunDecisionSelectionSourceV1 {
    ExplicitCandidate,
    OnlyVisibleCandidate,
    RoutinePolicy,
    RoutePolicy,
    OwnerPolicy,
    RewardPolicy,
    LearnedPolicy,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunDecisionCandidateSnapshotV1 {
    pub candidate_id: String,
    pub label: String,
    pub key: Option<DecisionCandidateKey>,
    pub action: CandidateAction,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunDecisionBoundaryV1 {
    pub decision_step: u64,
    pub title: String,
    pub location: String,
    pub candidates: Vec<RunDecisionCandidateSnapshotV1>,
}

impl RunDecisionBoundaryV1 {
    pub(in crate::eval::run_control) fn capture(session: &RunControlSession) -> Self {
        let surface = build_decision_surface(session);
        Self {
            decision_step: surface.view.header.step,
            title: surface.view.header.title,
            location: surface.view.header.location,
            candidates: surface
                .view
                .candidates
                .into_iter()
                .map(|candidate| RunDecisionCandidateSnapshotV1 {
                    candidate_id: candidate.id,
                    label: candidate.label,
                    key: candidate.key,
                    action: candidate.action,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunDecisionSelectionV1 {
    pub source: RunDecisionSelectionSourceV1,
    pub candidate_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunDecisionTransactionV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub before: RunDecisionBoundaryV1,
    pub selection: RunDecisionSelectionV1,
    pub action: RunDecisionAction,
    pub result: ActionResult,
    pub after: RunDecisionBoundaryV1,
    pub trace_annotations: Vec<RunControlTraceAnnotationV1>,
}

impl RunDecisionTransactionV1 {
    pub(in crate::eval::run_control) fn new(
        before: RunDecisionBoundaryV1,
        source: RunDecisionSelectionSourceV1,
        candidate_id: String,
        action: RunDecisionAction,
        result: ActionResult,
        after: RunDecisionBoundaryV1,
        trace_annotations: Vec<RunControlTraceAnnotationV1>,
    ) -> Result<Self, String> {
        let selected = before
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_id == candidate_id)
            .ok_or_else(|| "selected candidate is absent from the before boundary".to_string())?;
        let action_matches = match (&selected.action, &selected.key, &action) {
            (CandidateAction::Execute(expected), _, actual) => expected == actual,
            (
                CandidateAction::Parameterized { .. },
                Some(DecisionCandidateKey::SelectionSubmit { .. }),
                RunDecisionAction::Input(crate::state::core::ClientInput::SubmitSelection(_)),
            ) => true,
            _ => false,
        };
        if !action_matches {
            return Err("selected candidate action disagrees with the executed action".to_string());
        }
        if after.decision_step != before.decision_step.saturating_add(1) {
            return Err(
                "decision transaction did not advance exactly one decision step".to_string(),
            );
        }
        Ok(Self {
            schema_name: RUN_DECISION_TRANSACTION_SCHEMA_NAME.to_string(),
            schema_version: RUN_DECISION_TRANSACTION_SCHEMA_VERSION,
            before,
            selection: RunDecisionSelectionV1 {
                source,
                candidate_id,
            },
            action,
            result,
            after,
            trace_annotations,
        })
    }

    pub(in crate::eval::run_control) fn project_progress_outcome(
        &self,
        session: &RunControlSession,
    ) -> RunProgressOutcome {
        let mut report = render_action_result(&self.result);
        for annotation in &self.trace_annotations {
            if let RunControlTraceAnnotationV1::AutoCombatCapture {
                case_id,
                capture_path,
                benchmark_manifest_path,
                ..
            } = annotation
            {
                report.push_str(&format!(
                    "\nAuto-captured combat case `{case_id}` to {capture_path} and registered {benchmark_manifest_path}."
                ));
            }
        }
        RunProgressOutcome::action(
            format!("{report}\n{}", render_run_control_state(session)),
            self.result.clone(),
        )
        .with_trace_annotations(self.trace_annotations.clone())
        .with_progress_step(RunProgressStepV1::Decision(self.clone()))
    }
}
