use super::super::rollout_pending_choice::RolloutPendingChoiceProgress;
use super::super::CombatSearchV2ActionPreview;
use super::super::{combat_search_phase_profile, living_enemy_count, terminal_label, SearchNode};
use super::types::{RolloutNodeEstimate, RolloutStopReason, ROLLOUT_ACTION_PREVIEW_LIMIT};

impl RolloutNodeEstimate {
    pub(in crate::ai::combat_search_v2) fn from_node(
        node: &SearchNode,
        actions_simulated: usize,
        stop_reason: RolloutStopReason,
        last_action_reason: Option<&'static str>,
        pending_choice_progress: RolloutPendingChoiceProgress,
    ) -> Self {
        let phase_profile = combat_search_phase_profile(&node.engine, &node.combat);
        Self {
            evaluated: true,
            terminal: terminal_label(&node.engine, &node.combat),
            final_hp: node.combat.entities.player.current_hp,
            persistent_run_value:
                super::super::external_payoff::materialized_persistent_payoff_score_v1(&node.combat),
            external_burden_count: super::super::outcome_score::external_burden_count(&node.combat),
            unrecovered_stolen_gold: super::super::external_payoff::unrecovered_stolen_gold(
                &node.combat,
            ),
            hp_loss: (node.initial_hp - node.combat.entities.player.current_hp).max(0),
            turns: node.combat.turn.turn_count,
            potions_used: node.potions_used,
            potions_discarded: node.potions_discarded,
            cards_played: node.cards_played,
            living_enemy_count: living_enemy_count(&node.combat),
            total_enemy_hp: phase_profile.enemy_phase.raw_living_enemy_hp,
            total_enemy_block: phase_profile.enemy_phase.raw_living_enemy_block,
            phase_adjusted_enemy_effort: phase_profile
                .enemy_phase
                .phase_adjusted_living_enemy_effort,
            special_enemy_phase_count: phase_profile.special_enemy_phase_count(),
            guardian_mode_shift_pending_count: phase_profile
                .enemy_mechanics
                .guardian_mode_shift_pending_count,
            lagavulin_waking_count: phase_profile.enemy_mechanics.lagavulin_waking_count,
            gremlin_nob_anger_amount_total: phase_profile
                .enemy_mechanics
                .gremlin_nob_anger_amount_total,
            sentry_dazed_pressure_count: phase_profile.enemy_mechanics.sentry_dazed_pressure_count,
            hexaghost_opening_pressure_count: phase_profile
                .enemy_mechanics
                .hexaghost_opening_pressure_count,
            high_fanout_pending_choice: phase_profile.pending_choice.high_fanout,
            pending_choice_estimated_action_fanout: phase_profile
                .pending_choice
                .estimated_action_fanout,
            pending_choices_seen: pending_choice_progress.pending_choices_seen,
            pending_choice_actions_simulated: pending_choice_progress
                .pending_choice_actions_simulated,
            max_pending_choice_candidate_count: pending_choice_progress
                .max_pending_choice_candidate_count,
            max_pending_choice_estimated_action_fanout: pending_choice_progress
                .max_pending_choice_estimated_action_fanout,
            last_pending_choice_kind: pending_choice_progress.last_pending_choice_kind_label(),
            stopped_on_high_fanout_pending_choice: pending_choice_progress
                .stopped_on_high_fanout_pending_choice,
            survival_margin: phase_profile.pressure.survival_margin,
            actions_simulated,
            total_actions: node.actions.len(),
            action_preview: node
                .actions
                .iter()
                .take(ROLLOUT_ACTION_PREVIEW_LIMIT)
                .map(|action| CombatSearchV2ActionPreview {
                    action_key: action.action_key.clone(),
                    input: action.input.clone(),
                })
                .collect(),
            truncated: stop_reason.is_truncated(),
            stop_reason,
            last_action_reason,
        }
    }
}
