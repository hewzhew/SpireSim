use serde::{Deserialize, Serialize};

use super::session::RunControlSession;
use super::trace_annotation::{
    CombatAutomationTrajectoryRecordV1, CombatAutomationTrajectorySource,
};
use super::transition_report::{ActionResult, ActionResultChange};
use super::view_model::build_run_control_view_model;

pub const RUN_COMBAT_RESOLUTION_SCHEMA_NAME: &str = "RunCombatResolution";
pub const RUN_COMBAT_RESOLUTION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunCombatResolutionKindV1 {
    CompleteVictory,
    TurnSegment,
    SmokeBombEscape,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunCombatResolutionBoundaryV1 {
    pub decision_step: u64,
    pub combat_sequence: u64,
    pub title: String,
    pub location: String,
    pub active_combat: bool,
}

impl RunCombatResolutionBoundaryV1 {
    pub(in crate::eval::run_control) fn capture(session: &RunControlSession) -> Self {
        let view = build_run_control_view_model(session);
        Self {
            decision_step: session.decision_step,
            combat_sequence: session.combat_sequence,
            title: view.header.title,
            location: view.header.location,
            active_combat: session.active_combat.is_some(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunCombatResolutionV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub kind: RunCombatResolutionKindV1,
    pub before: RunCombatResolutionBoundaryV1,
    pub trajectory: CombatAutomationTrajectoryRecordV1,
    pub result: ActionResult,
    pub after: RunCombatResolutionBoundaryV1,
}

impl RunCombatResolutionV1 {
    pub(in crate::eval::run_control) fn new(
        kind: RunCombatResolutionKindV1,
        before: RunCombatResolutionBoundaryV1,
        trajectory: CombatAutomationTrajectoryRecordV1,
        result: ActionResult,
        after: RunCombatResolutionBoundaryV1,
    ) -> Result<Self, String> {
        if !before.active_combat {
            return Err("combat resolution started without an active combat".to_string());
        }
        if trajectory.actions.is_empty() || trajectory.action_count != trajectory.actions.len() {
            return Err(
                "combat resolution requires a non-empty coherent action trajectory".to_string(),
            );
        }
        if after.decision_step != before.decision_step {
            return Err("combat resolution changed the run decision-step counter".to_string());
        }
        let combat_ended = result
            .changes
            .iter()
            .any(|change| matches!(change, ActionResultChange::CombatEnded));
        match kind {
            RunCombatResolutionKindV1::CompleteVictory => {
                if !matches!(
                    trajectory.source,
                    CombatAutomationTrajectorySource::SearchCombat
                        | CombatAutomationTrajectorySource::AtomicExactV2
                        | CombatAutomationTrajectorySource::TurnGraphPortfolioV1
                        | CombatAutomationTrajectorySource::OracleExactActions
                        | CombatAutomationTrajectorySource::CompleteLineSolver
                        | CombatAutomationTrajectorySource::TurnPlanRescue
                        | CombatAutomationTrajectorySource::TurnPoolRescue
                ) {
                    return Err(
                        "complete combat victory has an incompatible trajectory source".to_string(),
                    );
                }
                if !combat_ended {
                    return Err("complete combat victory did not report CombatEnded".to_string());
                }
            }
            RunCombatResolutionKindV1::TurnSegment => {
                if trajectory.source != CombatAutomationTrajectorySource::SearchCombatTurnSegment {
                    return Err(
                        "combat turn segment has an incompatible trajectory source".to_string()
                    );
                }
                if combat_ended || !after.active_combat {
                    return Err("combat turn segment must retain the active combat".to_string());
                }
            }
            RunCombatResolutionKindV1::SmokeBombEscape => {
                if trajectory.source
                    != CombatAutomationTrajectorySource::SearchCombatSmokeBombSurvival
                {
                    return Err(
                        "Smoke Bomb resolution has an incompatible trajectory source".to_string(),
                    );
                }
                if !combat_ended {
                    return Err("Smoke Bomb resolution did not report CombatEnded".to_string());
                }
            }
        }
        Ok(Self {
            schema_name: RUN_COMBAT_RESOLUTION_SCHEMA_NAME.to_string(),
            schema_version: RUN_COMBAT_RESOLUTION_SCHEMA_VERSION,
            kind,
            before,
            trajectory,
            result,
            after,
        })
    }
}
