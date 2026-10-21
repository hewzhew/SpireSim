use super::super::CombatSearchV2RolloutEstimateReport;
use super::types::RolloutNodeEstimate;

impl RolloutNodeEstimate {
    pub(in crate::ai::combat_search_v2) fn to_report(
        &self,
    ) -> Option<CombatSearchV2RolloutEstimateReport> {
        self.evaluated
            .then_some(CombatSearchV2RolloutEstimateReport {
                terminal: self.terminal,
                estimated: true,
                final_hp: self.final_hp,
                hp_loss: self.hp_loss,
                turns: self.turns,
                potions_used: self.potions_used,
                potions_discarded: self.potions_discarded,
                cards_played: self.cards_played,
                living_enemy_count: self.living_enemy_count,
                total_enemy_hp: self.total_enemy_hp,
                total_enemy_block: self.total_enemy_block,
                phase_adjusted_enemy_effort: self.phase_adjusted_enemy_effort,
                special_enemy_phase_count: self.special_enemy_phase_count,
                guardian_mode_shift_pending_count: self.guardian_mode_shift_pending_count,
                lagavulin_waking_count: self.lagavulin_waking_count,
                gremlin_nob_anger_amount_total: self.gremlin_nob_anger_amount_total,
                sentry_dazed_pressure_count: self.sentry_dazed_pressure_count,
                hexaghost_opening_pressure_count: self.hexaghost_opening_pressure_count,
                high_fanout_pending_choice: self.high_fanout_pending_choice,
                pending_choice_estimated_action_fanout: self.pending_choice_estimated_action_fanout,
                pending_choices_seen: self.pending_choices_seen,
                pending_choice_actions_simulated: self.pending_choice_actions_simulated,
                max_pending_choice_candidate_count: self.max_pending_choice_candidate_count,
                max_pending_choice_estimated_action_fanout: self
                    .max_pending_choice_estimated_action_fanout,
                last_pending_choice_kind: self.last_pending_choice_kind,
                stopped_on_high_fanout_pending_choice: self.stopped_on_high_fanout_pending_choice,
                survival_margin: self.survival_margin,
                actions_simulated: self.actions_simulated,
                action_preview: self.action_preview.clone(),
                truncated: self.truncated,
                stop_reason: self.stop_reason.label(),
                last_action_reason: self.last_action_reason,
            })
    }
}
