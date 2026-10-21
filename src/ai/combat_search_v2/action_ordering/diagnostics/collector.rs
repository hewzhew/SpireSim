use std::collections::BTreeMap;

use super::super::super::action_priority::ActionOrderingRole;
use super::super::ActionOrderingSummary;
use super::samples::{
    ActionOrderingActionEffectObservation, ActionOrderingObservation, MutableOrderingRoleCount,
    ACTION_EFFECT_SAMPLE_LIMIT, LARGEST_REORDER_SAMPLE_LIMIT,
};

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct ActionOrderingDiagnosticsCollector {
    pub(super) states_observed: u64,
    pub(super) states_reordered: u64,
    pub(super) total_actions_observed: u64,
    pub(super) total_position_shift: u64,
    pub(super) max_position_shift: usize,
    pub(super) role_counts: BTreeMap<ActionOrderingRole, MutableOrderingRoleCount>,
    pub(super) largest_reorders: Vec<ActionOrderingObservation>,
    pub(super) action_effect_actions: u64,
    pub(super) attack_retaliation_actions: u64,
    pub(super) attack_retaliation_trigger_count_hint: u64,
    pub(super) attack_retaliation_raw_player_damage_hint: i64,
    pub(super) attack_retaliation_player_block_loss_hint: i64,
    pub(super) attack_retaliation_player_hp_loss_hint: i64,
    pub(super) max_attack_retaliation_player_hp_loss_hint: i32,
    pub(super) phase_action_hint_actions: u64,
    pub(super) root_action_prior_scored_states: u64,
    pub(super) root_action_prior_scored_actions: u64,
    pub(super) action_effect_samples: Vec<ActionOrderingActionEffectObservation>,
}

impl ActionOrderingDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn observe(&mut self, summary: &ActionOrderingSummary) {
        self.states_observed = self.states_observed.saturating_add(1);
        self.total_actions_observed = self
            .total_actions_observed
            .saturating_add(summary.action_count as u64);
        self.total_position_shift = self
            .total_position_shift
            .saturating_add(summary.max_position_shift as u64);
        self.max_position_shift = self.max_position_shift.max(summary.max_position_shift);
        if summary.max_position_shift > 0 {
            self.states_reordered = self.states_reordered.saturating_add(1);
        }
        self.phase_action_hint_actions = self
            .phase_action_hint_actions
            .saturating_add(summary.phase_signal_actions as u64);
        if summary.root_action_prior_scored_actions > 0 {
            self.root_action_prior_scored_states =
                self.root_action_prior_scored_states.saturating_add(1);
        }
        self.root_action_prior_scored_actions = self
            .root_action_prior_scored_actions
            .saturating_add(summary.root_action_prior_scored_actions as u64);

        for (role, count) in &summary.role_counts {
            let mutable = self.role_counts.entry(*role).or_default();
            mutable.actions = mutable.actions.saturating_add(*count as u64);
        }
        if let Some(first_role) = summary.first_role {
            self.role_counts
                .entry(first_role)
                .or_default()
                .first_actions += 1;
        }

        if let (Some(first_role), Some(first_original_action_id), Some(first_action_key)) = (
            summary.first_role,
            summary.first_original_action_id,
            summary.first_action_key.as_ref(),
        ) {
            self.remember_largest_reorder(ActionOrderingObservation {
                observed_at_state_query: self.states_observed,
                action_count: summary.action_count,
                max_position_shift: summary.max_position_shift,
                first_role,
                first_original_action_id,
                first_action_key: first_action_key.clone(),
            });
        }
        for sample in &summary.action_effect_samples {
            self.action_effect_actions = self.action_effect_actions.saturating_add(1);
            let retaliation = sample.effects.reactive;
            if retaliation.attack_retaliation_raw_player_damage_hint > 0 {
                self.attack_retaliation_actions = self.attack_retaliation_actions.saturating_add(1);
                self.attack_retaliation_trigger_count_hint = self
                    .attack_retaliation_trigger_count_hint
                    .saturating_add(retaliation.attack_retaliation_trigger_count_hint as u64);
                self.attack_retaliation_raw_player_damage_hint = self
                    .attack_retaliation_raw_player_damage_hint
                    .saturating_add(i64::from(
                        retaliation.attack_retaliation_raw_player_damage_hint,
                    ));
                self.attack_retaliation_player_block_loss_hint = self
                    .attack_retaliation_player_block_loss_hint
                    .saturating_add(i64::from(
                        retaliation.attack_retaliation_player_block_loss_hint,
                    ));
                self.attack_retaliation_player_hp_loss_hint = self
                    .attack_retaliation_player_hp_loss_hint
                    .saturating_add(i64::from(
                        retaliation.attack_retaliation_player_hp_loss_hint,
                    ));
                self.max_attack_retaliation_player_hp_loss_hint = self
                    .max_attack_retaliation_player_hp_loss_hint
                    .max(retaliation.attack_retaliation_player_hp_loss_hint);
            }
            self.remember_action_effect(ActionOrderingActionEffectObservation {
                observed_at_state_query: self.states_observed,
                original_action_id: sample.original_action_id,
                ordered_index: sample.ordered_index,
                role: sample.role,
                action_key: sample.action_key.clone(),
                effects: sample.effects,
                phase_hint: sample.phase_hint,
            });
        }
    }

    fn remember_largest_reorder(&mut self, observation: ActionOrderingObservation) {
        if observation.max_position_shift == 0 {
            return;
        }
        self.largest_reorders.push(observation);
        self.largest_reorders.sort_by(|left, right| {
            right
                .max_position_shift
                .cmp(&left.max_position_shift)
                .then_with(|| right.action_count.cmp(&left.action_count))
                .then_with(|| {
                    left.observed_at_state_query
                        .cmp(&right.observed_at_state_query)
                })
        });
        self.largest_reorders.truncate(LARGEST_REORDER_SAMPLE_LIMIT);
    }

    fn remember_action_effect(&mut self, observation: ActionOrderingActionEffectObservation) {
        self.action_effect_samples.push(observation);
        self.action_effect_samples.sort_by(|left, right| {
            right
                .phase_hint
                .awakened_one_strength_transition_setup
                .is_some()
                .cmp(
                    &left
                        .phase_hint
                        .awakened_one_strength_transition_setup
                        .is_some(),
                )
                .then_with(|| {
                    right
                        .effects
                        .derived
                        .reactive_risk_score
                        .cmp(&left.effects.derived.reactive_risk_score)
                })
                .then_with(|| {
                    right
                        .effects
                        .derived
                        .mitigation_score
                        .cmp(&left.effects.derived.mitigation_score)
                })
                .then_with(|| {
                    right
                        .effects
                        .reactive
                        .enemy_damage
                        .cmp(&left.effects.reactive.enemy_damage)
                })
                .then_with(|| {
                    left.observed_at_state_query
                        .cmp(&right.observed_at_state_query)
                })
                .then_with(|| left.ordered_index.cmp(&right.ordered_index))
        });
        self.action_effect_samples
            .truncate(ACTION_EFFECT_SAMPLE_LIMIT);
    }
}
