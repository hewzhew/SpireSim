use super::super::super::{
    CombatSearchV2DiagnosticsActionEffectAccess, CombatSearchV2DiagnosticsActionEffectDerived,
    CombatSearchV2DiagnosticsActionEffectDirect, CombatSearchV2DiagnosticsActionEffectReactive,
    CombatSearchV2DiagnosticsActionEffectSample, CombatSearchV2DiagnosticsActionPhase,
    CombatSearchV2DiagnosticsActionRoleCount, CombatSearchV2DiagnosticsOrdering,
    CombatSearchV2DiagnosticsOrderingSample,
};
use super::collector::ActionOrderingDiagnosticsCollector;

impl ActionOrderingDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn finish(&self) -> CombatSearchV2DiagnosticsOrdering {
        CombatSearchV2DiagnosticsOrdering {
            ordering_policy:
                "semantic_role_ordering_for_player_turn_and_pending_choice_boundaries",
            behavioral_effect: "child_generation_order_only_no_prune_no_merge",
            states_observed: self.states_observed,
            states_reordered: self.states_reordered,
            reordered_state_ratio: rounded_ratio(self.states_reordered, self.states_observed),
            total_actions_observed: self.total_actions_observed,
            action_effect_actions: self.action_effect_actions,
            attack_retaliation_actions: self.attack_retaliation_actions,
            attack_retaliation_trigger_count_hint: self.attack_retaliation_trigger_count_hint,
            attack_retaliation_raw_player_damage_hint: self
                .attack_retaliation_raw_player_damage_hint,
            attack_retaliation_player_block_loss_hint: self
                .attack_retaliation_player_block_loss_hint,
            attack_retaliation_player_hp_loss_hint: self
                .attack_retaliation_player_hp_loss_hint,
            max_attack_retaliation_player_hp_loss_hint: self
                .max_attack_retaliation_player_hp_loss_hint,
            phase_action_hint_actions: self.phase_action_hint_actions,
            root_action_prior_scored_states: self.root_action_prior_scored_states,
            root_action_prior_scored_actions: self.root_action_prior_scored_actions,
            max_position_shift: self.max_position_shift,
            avg_position_shift: rounded_ratio(self.total_position_shift, self.states_observed),
            action_role_counts: self.action_role_counts(),
            largest_reorders: self.largest_reorder_samples(),
            action_effect_samples: self.action_effect_samples(),
            notes: vec![
                "ordering diagnostics summarize which semantic roles are explored first",
                "original action ids are preserved in action traces after ordering",
                "a reorder sample is kept only when action order changed",
                "ordering does not remove legal actions or prove action equivalence",
                "reactive power risk is derived from simulator power hooks, not monster-name policy",
                "enemy phase transition hints only reorder children and never suppress phase-triggering actions",
                "phase action hints reuse phase_profile and only add ordering tiebreaks",
                "pending choice ordering uses typed selection facts and never drops alternatives",
                "root action prior hints are opt-in child ordering hints; they never remove legal actions",
                "attack retaliation totals separate raw damage, projected block loss, and projected HP loss across candidate observations; they are not unique paths or realized trajectory loss",
            ],
        }
    }

    fn action_role_counts(&self) -> Vec<CombatSearchV2DiagnosticsActionRoleCount> {
        self.role_counts
            .iter()
            .map(|(role, count)| CombatSearchV2DiagnosticsActionRoleCount {
                role: role.label().to_string(),
                actions: count.actions,
                first_actions: count.first_actions,
            })
            .collect()
    }

    fn largest_reorder_samples(&self) -> Vec<CombatSearchV2DiagnosticsOrderingSample> {
        self.largest_reorders
            .iter()
            .map(|sample| CombatSearchV2DiagnosticsOrderingSample {
                observed_at_state_query: sample.observed_at_state_query,
                action_count: sample.action_count,
                max_position_shift: sample.max_position_shift,
                first_role: sample.first_role.label().to_string(),
                first_original_action_id: sample.first_original_action_id,
                first_action_key: sample.first_action_key.clone(),
            })
            .collect()
    }

    fn action_effect_samples(&self) -> Vec<CombatSearchV2DiagnosticsActionEffectSample> {
        self.action_effect_samples
            .iter()
            .map(|sample| {
                let opportunity = sample.phase_hint.awakened_one_strength_transition_setup;
                CombatSearchV2DiagnosticsActionEffectSample {
                    observed_at_state_query: sample.observed_at_state_query,
                    original_action_id: sample.original_action_id,
                    ordered_index: sample.ordered_index,
                    role: sample.role.label().to_string(),
                    action_key: sample.action_key.clone(),
                    direct: CombatSearchV2DiagnosticsActionEffectDirect {
                        persistent_enemy_strength_down: sample
                            .effects
                            .direct
                            .persistent_enemy_strength_down,
                        temporary_enemy_strength_down: sample
                            .effects
                            .direct
                            .temporary_enemy_strength_down,
                        visible_attack_mitigation_hint: sample
                            .effects
                            .direct
                            .visible_attack_mitigation_hint,
                        enemy_weak: sample.effects.direct.enemy_weak,
                        enemy_vulnerable: sample.effects.direct.enemy_vulnerable,
                        enemy_strength_gain: sample.effects.direct.enemy_strength_gain,
                        visible_attack_pressure_hint: sample
                            .effects
                            .direct
                            .visible_attack_pressure_hint,
                        player_strength_gain: sample.effects.direct.player_strength_gain,
                        player_temporary_strength_gain: sample
                            .effects
                            .direct
                            .player_temporary_strength_gain,
                    },
                    reactive: CombatSearchV2DiagnosticsActionEffectReactive {
                        player_hp_loss: sample.effects.reactive.player_hp_loss,
                        attack_retaliation_trigger_count_hint: sample
                            .effects
                            .reactive
                            .attack_retaliation_trigger_count_hint,
                        attack_retaliation_raw_player_damage_hint: sample
                            .effects
                            .reactive
                            .attack_retaliation_raw_player_damage_hint,
                        attack_retaliation_player_block_loss_hint: sample
                            .effects
                            .reactive
                            .attack_retaliation_player_block_loss_hint,
                        attack_retaliation_player_hp_loss_hint: sample
                            .effects
                            .reactive
                            .attack_retaliation_player_hp_loss_hint,
                        player_block: sample.effects.reactive.player_block,
                        enemy_damage: sample.effects.reactive.enemy_damage,
                        bad_draw_cards: sample.effects.reactive.bad_draw_cards,
                        forced_turn_end: sample.effects.reactive.forced_turn_end,
                        enemy_strength_gain: sample.effects.reactive.enemy_strength_gain,
                        visible_attack_pressure_hint: sample
                            .effects
                            .reactive
                            .visible_attack_pressure_hint,
                        enemy_weak: sample.effects.reactive.enemy_weak,
                        enemy_vulnerable: sample.effects.reactive.enemy_vulnerable,
                    },
                    access: CombatSearchV2DiagnosticsActionEffectAccess {
                        declared_draw_cards: sample.effects.access.declared_draw_cards,
                        conditional_draw_cards: sample.effects.access.conditional_draw_cards,
                        total_draw_cards: sample.effects.access.total_draw_cards,
                    },
                    derived: CombatSearchV2DiagnosticsActionEffectDerived {
                        mitigation_score: sample.effects.derived.mitigation_score,
                        reactive_risk_score: sample.effects.derived.reactive_risk_score,
                        enemy_scaling_risk_score: sample.effects.derived.enemy_scaling_risk_score,
                        net_mitigation_score: sample.effects.derived.net_mitigation_score,
                    },
                    phase: CombatSearchV2DiagnosticsActionPhase {
                        awakened_one_strength_transition: opportunity.is_some(),
                        temporary_strength_down: opportunity
                            .map(|value| value.temporary_strength_down)
                            .unwrap_or_default(),
                        convertible_positive_strength: opportunity
                            .map(|value| value.convertible_positive_strength)
                            .unwrap_or_default(),
                        remaining_damage_upper_bound: opportunity
                            .map(|value| value.remaining_damage_upper_bound)
                            .unwrap_or_default(),
                        phase_one_hp_with_block: opportunity
                            .map(|value| value.phase_one_hp_with_block)
                            .unwrap_or_default(),
                    },
                }
            })
            .collect()
    }
}

fn rounded_ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    let value = numerator as f64 / denominator as f64;
    (value * 100.0).round() / 100.0
}
