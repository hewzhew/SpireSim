use serde::{Deserialize, Serialize};

use super::{RunCombatResolutionV1, RunDecisionTransactionV1, RunForcedTransitionV1};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "record")]
pub enum RunProgressStepV1 {
    Decision(RunDecisionTransactionV1),
    ForcedTransition(RunForcedTransitionV1),
    CombatResolution(RunCombatResolutionV1),
    Stop(RunControlAutoStopV1),
}

impl RunProgressStepV1 {
    pub fn as_decision(&self) -> Option<&RunDecisionTransactionV1> {
        match self {
            Self::Decision(transaction) => Some(transaction),
            _ => None,
        }
    }

    pub fn as_forced_transition(&self) -> Option<&RunForcedTransitionV1> {
        match self {
            Self::ForcedTransition(transition) => Some(transition),
            _ => None,
        }
    }

    pub fn as_combat_resolution(&self) -> Option<&RunCombatResolutionV1> {
        match self {
            Self::CombatResolution(resolution) => Some(resolution),
            _ => None,
        }
    }

    pub fn as_stop(&self) -> Option<&RunControlAutoStopV1> {
        match self {
            Self::Stop(stop) => Some(stop),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RunControlAutoStopV1 {
    pub kind: RunControlAutoStopKind,
    pub reason: String,
    pub applied_operations: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunControlAutoStopKind {
    HpLossGateRequired,
    CombatSearchNoCompleteWin,
    RoutePolicyNoMutation,
    RoutePolicyDeclined,
    AutoCandidateNotExecutable,
    HumanBoundary,
    CombatBoundary,
    ProgressBudgetExhausted,
    WallDeadlineReached,
    RunCompleted,
}
