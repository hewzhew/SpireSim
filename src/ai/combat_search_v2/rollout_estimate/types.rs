use super::super::{CombatSearchV2ActionPreview, SearchTerminalLabel};

pub(in crate::ai::combat_search_v2) const ROLLOUT_ACTION_PREVIEW_LIMIT: usize = 96;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct RolloutNodeEstimate {
    pub(in crate::ai::combat_search_v2) evaluated: bool,
    pub(in crate::ai::combat_search_v2) terminal: SearchTerminalLabel,
    pub(in crate::ai::combat_search_v2) final_hp: i32,
    pub(in crate::ai::combat_search_v2) persistent_run_value: i32,
    pub(in crate::ai::combat_search_v2) external_burden_count: i32,
    pub(in crate::ai::combat_search_v2) unrecovered_stolen_gold: i32,
    pub(in crate::ai::combat_search_v2) hp_loss: i32,
    pub(in crate::ai::combat_search_v2) turns: u32,
    pub(in crate::ai::combat_search_v2) potions_used: u32,
    pub(in crate::ai::combat_search_v2) potions_discarded: u32,
    pub(in crate::ai::combat_search_v2) cards_played: u32,
    pub(in crate::ai::combat_search_v2) living_enemy_count: usize,
    pub(in crate::ai::combat_search_v2) total_enemy_hp: i32,
    pub(in crate::ai::combat_search_v2) total_enemy_block: i32,
    pub(in crate::ai::combat_search_v2) phase_adjusted_enemy_effort: i32,
    pub(in crate::ai::combat_search_v2) special_enemy_phase_count: usize,
    pub(in crate::ai::combat_search_v2) guardian_mode_shift_pending_count: usize,
    pub(in crate::ai::combat_search_v2) lagavulin_waking_count: usize,
    pub(in crate::ai::combat_search_v2) gremlin_nob_anger_amount_total: i32,
    pub(in crate::ai::combat_search_v2) sentry_dazed_pressure_count: usize,
    pub(in crate::ai::combat_search_v2) hexaghost_opening_pressure_count: usize,
    pub(in crate::ai::combat_search_v2) high_fanout_pending_choice: bool,
    pub(in crate::ai::combat_search_v2) pending_choice_estimated_action_fanout: usize,
    pub(in crate::ai::combat_search_v2) pending_choices_seen: usize,
    pub(in crate::ai::combat_search_v2) pending_choice_actions_simulated: usize,
    pub(in crate::ai::combat_search_v2) max_pending_choice_candidate_count: usize,
    pub(in crate::ai::combat_search_v2) max_pending_choice_estimated_action_fanout: usize,
    pub(in crate::ai::combat_search_v2) last_pending_choice_kind: Option<&'static str>,
    pub(in crate::ai::combat_search_v2) stopped_on_high_fanout_pending_choice: bool,
    pub(in crate::ai::combat_search_v2) survival_margin: i32,
    pub(in crate::ai::combat_search_v2) actions_simulated: usize,
    pub(in crate::ai::combat_search_v2) total_actions: usize,
    pub(in crate::ai::combat_search_v2) action_preview: Vec<CombatSearchV2ActionPreview>,
    pub(in crate::ai::combat_search_v2) truncated: bool,
    pub(in crate::ai::combat_search_v2) stop_reason: RolloutStopReason,
    pub(in crate::ai::combat_search_v2) last_action_reason: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) enum RolloutStopReason {
    NotEvaluated,
    TerminalState,
    MaxActions,
    Deadline,
    NoLegalActions,
    PolicyDeclined,
    StructuredPendingChoice,
    EngineStepLimit,
    HighFanoutPendingChoice,
}

impl RolloutNodeEstimate {
    pub(in crate::ai::combat_search_v2) fn is_evaluated(&self) -> bool {
        self.evaluated
    }

    pub(in crate::ai::combat_search_v2) fn unevaluated() -> Self {
        Self {
            evaluated: false,
            terminal: SearchTerminalLabel::Unresolved,
            final_hp: 0,
            persistent_run_value: 0,
            external_burden_count: 0,
            unrecovered_stolen_gold: 0,
            hp_loss: 0,
            turns: 0,
            potions_used: 0,
            potions_discarded: 0,
            cards_played: 0,
            living_enemy_count: 0,
            total_enemy_hp: 0,
            total_enemy_block: 0,
            phase_adjusted_enemy_effort: 0,
            special_enemy_phase_count: 0,
            guardian_mode_shift_pending_count: 0,
            lagavulin_waking_count: 0,
            gremlin_nob_anger_amount_total: 0,
            sentry_dazed_pressure_count: 0,
            hexaghost_opening_pressure_count: 0,
            high_fanout_pending_choice: false,
            pending_choice_estimated_action_fanout: 0,
            pending_choices_seen: 0,
            pending_choice_actions_simulated: 0,
            max_pending_choice_candidate_count: 0,
            max_pending_choice_estimated_action_fanout: 0,
            last_pending_choice_kind: None,
            stopped_on_high_fanout_pending_choice: false,
            survival_margin: 0,
            actions_simulated: 0,
            total_actions: 0,
            action_preview: Vec::new(),
            truncated: false,
            stop_reason: RolloutStopReason::NotEvaluated,
            last_action_reason: None,
        }
    }

    pub(in crate::ai::combat_search_v2) fn is_replayable_terminal_win(&self) -> bool {
        self.evaluated
            && self.terminal == SearchTerminalLabel::Win
            && self.stop_reason == RolloutStopReason::TerminalState
            && !self.truncated
            && self.total_actions == self.action_preview.len()
            && self.total_actions <= ROLLOUT_ACTION_PREVIEW_LIMIT
    }

    pub(in crate::ai::combat_search_v2) fn is_replayable_terminal_win_without_new_external_burden(
        &self,
        initial_external_burden_count: i32,
    ) -> bool {
        self.is_replayable_terminal_win()
            && self.external_burden_count <= initial_external_burden_count
    }
}

#[cfg(test)]
mod tests {
    use crate::state::core::ClientInput;

    use super::*;

    #[test]
    fn replayable_terminal_win_requires_the_complete_action_preview() {
        let mut estimate = RolloutNodeEstimate::unevaluated();
        estimate.evaluated = true;
        estimate.terminal = SearchTerminalLabel::Win;
        estimate.stop_reason = RolloutStopReason::TerminalState;
        estimate.total_actions = ROLLOUT_ACTION_PREVIEW_LIMIT + 1;
        estimate.action_preview = (0..ROLLOUT_ACTION_PREVIEW_LIMIT)
            .map(|index| CombatSearchV2ActionPreview {
                action_key: format!("action/{index}"),
                input: ClientInput::EndTurn,
            })
            .collect();

        assert!(!estimate.is_replayable_terminal_win());

        estimate.total_actions = estimate.action_preview.len();
        assert!(estimate.is_replayable_terminal_win());
    }
}
