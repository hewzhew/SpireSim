use super::super::super::rollout_pending_choice::RolloutPendingChoiceProgress;
use super::super::super::*;

#[derive(Clone)]
pub(super) struct TurnBeamState {
    pub(super) node: SearchNode,
    pub(super) progress: RolloutPendingChoiceProgress,
    pub(super) last_action_reason: Option<&'static str>,
    pub(super) estimate_override: Option<RolloutNodeEstimate>,
    pub(super) stalled_stop_reason: Option<RolloutStopReason>,
}
