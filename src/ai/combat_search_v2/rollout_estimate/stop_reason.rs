use super::types::RolloutStopReason;

impl RolloutStopReason {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            RolloutStopReason::NotEvaluated => "not_evaluated",
            RolloutStopReason::TerminalState => "terminal_state",
            RolloutStopReason::MaxActions => "max_actions",
            RolloutStopReason::Deadline => "deadline",
            RolloutStopReason::NoLegalActions => "no_legal_actions",
            RolloutStopReason::PolicyDeclined => "policy_declined",
            RolloutStopReason::StructuredPendingChoice => "structured_pending_choice",
            RolloutStopReason::EngineStepLimit => "engine_step_limit",
            RolloutStopReason::HighFanoutPendingChoice => "high_fanout_pending_choice",
        }
    }

    pub(in crate::ai::combat_search_v2) fn is_truncated(self) -> bool {
        !matches!(
            self,
            RolloutStopReason::NotEvaluated | RolloutStopReason::TerminalState
        )
    }
}
