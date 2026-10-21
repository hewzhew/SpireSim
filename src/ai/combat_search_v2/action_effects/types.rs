#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct CardPlayEffectFacts {
    pub(in crate::ai::combat_search_v2) direct: DirectCardPlayEffectFacts,
    pub(in crate::ai::combat_search_v2) reactive: ReactiveCardPlayEffectFacts,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct DirectCardPlayEffectFacts {
    pub(in crate::ai::combat_search_v2) player_block: i32,
    pub(in crate::ai::combat_search_v2) persistent_enemy_strength_down: i32,
    pub(in crate::ai::combat_search_v2) temporary_enemy_strength_down: i32,
    pub(in crate::ai::combat_search_v2) visible_attack_mitigation_hint: i32,
    pub(in crate::ai::combat_search_v2) enemy_strength_gain: i32,
    pub(in crate::ai::combat_search_v2) visible_attack_pressure_hint: i32,
    pub(in crate::ai::combat_search_v2) player_strength_gain: i32,
    pub(in crate::ai::combat_search_v2) player_temporary_strength_gain: i32,
    pub(in crate::ai::combat_search_v2) player_recurring_turn_energy_delta: i32,
    pub(in crate::ai::combat_search_v2) player_attack_replay_charges: i32,
    pub(in crate::ai::combat_search_v2) player_energy_gain: i32,
    pub(in crate::ai::combat_search_v2) declared_draw_cards: i32,
    pub(in crate::ai::combat_search_v2) conditional_draw_cards: i32,
    pub(in crate::ai::combat_search_v2) enemy_weak: i32,
    pub(in crate::ai::combat_search_v2) enemy_vulnerable: i32,
    pub(in crate::ai::combat_search_v2) player_vulnerable: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct ReactiveCardPlayEffectFacts {
    pub(in crate::ai::combat_search_v2) player_hp_loss: i32,
    pub(in crate::ai::combat_search_v2) attack_retaliation_trigger_count_hint: usize,
    pub(in crate::ai::combat_search_v2) attack_retaliation_raw_player_damage_hint: i32,
    pub(in crate::ai::combat_search_v2) attack_retaliation_player_block_loss_hint: i32,
    pub(in crate::ai::combat_search_v2) attack_retaliation_player_hp_loss_hint: i32,
    pub(in crate::ai::combat_search_v2) player_block: i32,
    pub(in crate::ai::combat_search_v2) enemy_damage: i32,
    pub(in crate::ai::combat_search_v2) bad_draw_cards: i32,
    pub(in crate::ai::combat_search_v2) forced_turn_end: bool,
    pub(in crate::ai::combat_search_v2) enemy_strength_gain: i32,
    pub(in crate::ai::combat_search_v2) visible_attack_pressure_hint: i32,
    pub(in crate::ai::combat_search_v2) enemy_weak: i32,
    pub(in crate::ai::combat_search_v2) enemy_vulnerable: i32,
    pub(in crate::ai::combat_search_v2) future_attack_damage_percent_gain_hint: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct CardPlayDerivedEffectScores {
    pub(in crate::ai::combat_search_v2) mitigation_score: i32,
    pub(in crate::ai::combat_search_v2) enemy_scaling_risk_score: i32,
    pub(in crate::ai::combat_search_v2) reactive_risk_score: i32,
    pub(in crate::ai::combat_search_v2) net_mitigation_score: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct CardPlayEffectDiagnostics {
    pub(in crate::ai::combat_search_v2) derived: CardPlayDerivedEffectScores,
    pub(in crate::ai::combat_search_v2) direct: CardPlayDirectEffectDiagnostics,
    pub(in crate::ai::combat_search_v2) reactive: CardPlayReactiveEffectDiagnostics,
    pub(in crate::ai::combat_search_v2) access: CardPlayAccessEffectDiagnostics,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct CardPlayDirectEffectDiagnostics {
    pub(in crate::ai::combat_search_v2) player_block: i32,
    pub(in crate::ai::combat_search_v2) persistent_enemy_strength_down: i32,
    pub(in crate::ai::combat_search_v2) temporary_enemy_strength_down: i32,
    pub(in crate::ai::combat_search_v2) visible_attack_mitigation_hint: i32,
    pub(in crate::ai::combat_search_v2) enemy_weak: i32,
    pub(in crate::ai::combat_search_v2) enemy_vulnerable: i32,
    pub(in crate::ai::combat_search_v2) player_vulnerable: i32,
    pub(in crate::ai::combat_search_v2) enemy_strength_gain: i32,
    pub(in crate::ai::combat_search_v2) visible_attack_pressure_hint: i32,
    pub(in crate::ai::combat_search_v2) player_strength_gain: i32,
    pub(in crate::ai::combat_search_v2) player_temporary_strength_gain: i32,
    pub(in crate::ai::combat_search_v2) player_recurring_turn_energy_delta: i32,
    pub(in crate::ai::combat_search_v2) player_attack_replay_charges: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct CardPlayReactiveEffectDiagnostics {
    pub(in crate::ai::combat_search_v2) player_hp_loss: i32,
    pub(in crate::ai::combat_search_v2) attack_retaliation_trigger_count_hint: usize,
    pub(in crate::ai::combat_search_v2) attack_retaliation_raw_player_damage_hint: i32,
    pub(in crate::ai::combat_search_v2) attack_retaliation_player_block_loss_hint: i32,
    pub(in crate::ai::combat_search_v2) attack_retaliation_player_hp_loss_hint: i32,
    pub(in crate::ai::combat_search_v2) player_block: i32,
    pub(in crate::ai::combat_search_v2) enemy_damage: i32,
    pub(in crate::ai::combat_search_v2) bad_draw_cards: i32,
    pub(in crate::ai::combat_search_v2) forced_turn_end: bool,
    pub(in crate::ai::combat_search_v2) enemy_strength_gain: i32,
    pub(in crate::ai::combat_search_v2) visible_attack_pressure_hint: i32,
    pub(in crate::ai::combat_search_v2) enemy_weak: i32,
    pub(in crate::ai::combat_search_v2) enemy_vulnerable: i32,
    pub(in crate::ai::combat_search_v2) future_attack_damage_percent_gain_hint: i32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct CardPlayAccessEffectDiagnostics {
    pub(in crate::ai::combat_search_v2) player_energy_gain: i32,
    pub(in crate::ai::combat_search_v2) declared_draw_cards: i32,
    pub(in crate::ai::combat_search_v2) conditional_draw_cards: i32,
    pub(in crate::ai::combat_search_v2) total_draw_cards: i32,
}

impl CardPlayEffectFacts {
    pub(in crate::ai::combat_search_v2) fn mitigation_ordering_score(self) -> i32 {
        self.direct
            .persistent_enemy_strength_down
            .saturating_add(self.direct.temporary_enemy_strength_down)
            .saturating_add(self.direct.visible_attack_mitigation_hint)
    }

    pub(in crate::ai::combat_search_v2) fn enemy_scaling_risk_score(self) -> i32 {
        self.direct
            .enemy_strength_gain
            .saturating_add(self.reactive.enemy_strength_gain)
            .saturating_add(self.direct.visible_attack_pressure_hint)
            .saturating_add(self.reactive.visible_attack_pressure_hint)
    }

    pub(in crate::ai::combat_search_v2) fn reactive_risk_score(self) -> i32 {
        self.enemy_scaling_risk_score()
            .saturating_add(self.reactive.player_hp_loss)
            .saturating_add(self.direct.player_vulnerable)
            .saturating_add(self.reactive.attack_retaliation_player_block_loss_hint)
            .saturating_add(self.reactive.bad_draw_cards)
            .saturating_add(i32::from(self.reactive.forced_turn_end))
    }

    pub(in crate::ai::combat_search_v2) fn net_mitigation_ordering_score(self) -> i32 {
        self.mitigation_ordering_score()
            .saturating_sub(self.reactive_risk_score())
    }

    pub(in crate::ai::combat_search_v2) fn derived_scores(self) -> CardPlayDerivedEffectScores {
        CardPlayDerivedEffectScores {
            mitigation_score: self.mitigation_ordering_score(),
            enemy_scaling_risk_score: self.enemy_scaling_risk_score(),
            reactive_risk_score: self.reactive_risk_score(),
            net_mitigation_score: self.net_mitigation_ordering_score(),
        }
    }

    pub(in crate::ai::combat_search_v2) fn total_draw_cards(self) -> i32 {
        self.direct
            .declared_draw_cards
            .saturating_add(self.direct.conditional_draw_cards)
    }

    pub(in crate::ai::combat_search_v2) fn has_future_debuff(self) -> bool {
        self.direct.enemy_weak > 0
            || self.direct.enemy_vulnerable > 0
            || self.direct.persistent_enemy_strength_down > 0
            || self.direct.temporary_enemy_strength_down > 0
            || self.reactive.enemy_weak > 0
            || self.reactive.enemy_vulnerable > 0
    }

    pub(in crate::ai::combat_search_v2) fn diagnostics(self) -> CardPlayEffectDiagnostics {
        CardPlayEffectDiagnostics {
            derived: self.derived_scores(),
            direct: CardPlayDirectEffectDiagnostics {
                player_block: self.direct.player_block,
                persistent_enemy_strength_down: self.direct.persistent_enemy_strength_down,
                temporary_enemy_strength_down: self.direct.temporary_enemy_strength_down,
                visible_attack_mitigation_hint: self.direct.visible_attack_mitigation_hint,
                enemy_weak: self.direct.enemy_weak,
                enemy_vulnerable: self.direct.enemy_vulnerable,
                player_vulnerable: self.direct.player_vulnerable,
                enemy_strength_gain: self.direct.enemy_strength_gain,
                visible_attack_pressure_hint: self.direct.visible_attack_pressure_hint,
                player_strength_gain: self.direct.player_strength_gain,
                player_temporary_strength_gain: self.direct.player_temporary_strength_gain,
                player_recurring_turn_energy_delta: self.direct.player_recurring_turn_energy_delta,
                player_attack_replay_charges: self.direct.player_attack_replay_charges,
            },
            reactive: CardPlayReactiveEffectDiagnostics {
                player_hp_loss: self.reactive.player_hp_loss,
                attack_retaliation_trigger_count_hint: self
                    .reactive
                    .attack_retaliation_trigger_count_hint,
                attack_retaliation_raw_player_damage_hint: self
                    .reactive
                    .attack_retaliation_raw_player_damage_hint,
                attack_retaliation_player_block_loss_hint: self
                    .reactive
                    .attack_retaliation_player_block_loss_hint,
                attack_retaliation_player_hp_loss_hint: self
                    .reactive
                    .attack_retaliation_player_hp_loss_hint,
                player_block: self.reactive.player_block,
                enemy_damage: self.reactive.enemy_damage,
                bad_draw_cards: self.reactive.bad_draw_cards,
                forced_turn_end: self.reactive.forced_turn_end,
                enemy_strength_gain: self.reactive.enemy_strength_gain,
                visible_attack_pressure_hint: self.reactive.visible_attack_pressure_hint,
                enemy_weak: self.reactive.enemy_weak,
                enemy_vulnerable: self.reactive.enemy_vulnerable,
                future_attack_damage_percent_gain_hint: self
                    .reactive
                    .future_attack_damage_percent_gain_hint,
            },
            access: CardPlayAccessEffectDiagnostics {
                player_energy_gain: self.direct.player_energy_gain,
                declared_draw_cards: self.direct.declared_draw_cards,
                conditional_draw_cards: self.direct.conditional_draw_cards,
                total_draw_cards: self.total_draw_cards(),
            },
        }
    }
}

impl CardPlayEffectDiagnostics {
    pub(in crate::ai::combat_search_v2) fn has_reactive_signal(self) -> bool {
        self.derived.reactive_risk_score > 0
            || self.reactive.player_block > 0
            || self.reactive.enemy_damage > 0
            || self.derived.mitigation_score > 0
    }
}
