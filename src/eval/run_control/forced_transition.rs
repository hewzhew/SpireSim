use serde::{Deserialize, Serialize};

use super::decision_transaction::RunDecisionBoundaryV1;
use super::render_run_control_state;
use super::session::{RunControlSession, RunProgressOutcome};
use super::transition_report::{render_action_result, ActionResult};
use super::RunProgressStepV1;

pub const RUN_FORCED_TRANSITION_SCHEMA_NAME: &str = "RunForcedTransition";
pub const RUN_FORCED_TRANSITION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunForcedTransitionKindV1 {
    EmptyCampfireExit,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunForcedTransitionV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub kind: RunForcedTransitionKindV1,
    pub before: RunDecisionBoundaryV1,
    pub result: ActionResult,
    pub after: RunDecisionBoundaryV1,
}

impl RunForcedTransitionV1 {
    pub(in crate::eval::run_control) fn new(
        kind: RunForcedTransitionKindV1,
        before: RunDecisionBoundaryV1,
        result: ActionResult,
        after: RunDecisionBoundaryV1,
    ) -> Result<Self, String> {
        if !before.candidates.is_empty() {
            return Err(
                "forced transition started at a boundary with legal candidates".to_string(),
            );
        }
        if after.decision_step != before.decision_step {
            return Err("forced transition changed the decision-step counter".to_string());
        }
        Ok(Self {
            schema_name: RUN_FORCED_TRANSITION_SCHEMA_NAME.to_string(),
            schema_version: RUN_FORCED_TRANSITION_SCHEMA_VERSION,
            kind,
            before,
            result,
            after,
        })
    }

    pub(in crate::eval::run_control) fn project_progress_outcome(
        &self,
        session: &RunControlSession,
    ) -> RunProgressOutcome {
        RunProgressOutcome::action(
            format!(
                "{}\n{}",
                render_action_result(&self.result),
                render_run_control_state(session)
            ),
            self.result.clone(),
        )
        .with_progress_step(RunProgressStepV1::ForcedTransition(self.clone()))
    }
}
