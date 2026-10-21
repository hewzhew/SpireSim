use crate::content::cards::CardTarget;
use crate::runtime::combat::CombatState;
use crate::state::core::ClientInput;

use super::super::action_effects::card_play_effect_facts;
use super::super::action_resource_timing::resource_timing_facts_for_play;
use super::payload::resolved_card_action_payload_facts;
use super::target::{all_enemy_progress_hint, target_progress_hint};
use super::types::{
    CombatSearchV2ActionAccessMechanicsFacts, CombatSearchV2ActionCardFacts,
    CombatSearchV2ActionDerivedMechanicsFacts, CombatSearchV2ActionDirectMechanicsFacts,
    CombatSearchV2ActionImmediateFacts, CombatSearchV2ActionMechanicsFacts,
    CombatSearchV2ActionReactiveMechanicsFacts,
};

pub(super) fn immediate_and_mechanics_facts(
    combat: &CombatState,
    input: &ClientInput,
    card: Option<&CombatSearchV2ActionCardFacts>,
) -> (
    CombatSearchV2ActionImmediateFacts,
    CombatSearchV2ActionMechanicsFacts,
) {
    let Some(card_facts) = card else {
        return (
            CombatSearchV2ActionImmediateFacts::default(),
            CombatSearchV2ActionMechanicsFacts::default(),
        );
    };
    let ClientInput::PlayCard { card_index, target } = *input else {
        return (
            CombatSearchV2ActionImmediateFacts::default(),
            CombatSearchV2ActionMechanicsFacts::default(),
        );
    };
    let Some(runtime_card) = combat.zones.hand.get(card_index) else {
        return (
            CombatSearchV2ActionImmediateFacts::default(),
            CombatSearchV2ActionMechanicsFacts::default(),
        );
    };
    let payload = resolved_card_action_payload_facts(combat, runtime_card, target);
    let effects = card_play_effect_facts(combat, runtime_card, target);
    let resource_timing = resource_timing_facts_for_play(combat, card_index, target);
    let damage_hint = card_facts
        .evaluated_damage
        .max(resource_timing.conversion_damage_hint);
    let conversion_damage_hit_count = if resource_timing.conversion_damage_hint > 0 {
        resource_timing.hand_exhaust_target_count
    } else {
        0
    };
    let block_hint = card_facts
        .evaluated_block
        .max(payload.player_block_hint)
        .max(resource_timing.conversion_block_hint)
        .saturating_add(effects.reactive.player_block);
    let target_progress_damage = if card_facts.effective_target == CardTarget::AllEnemy {
        damage_hint
    } else {
        payload.damage_total_hint.max(damage_hint)
    };
    let target_progress = target_progress_hint(
        combat,
        card_facts.effective_target,
        target,
        target_progress_damage,
    );
    let all_enemy_progress = all_enemy_progress_hint(
        combat,
        card_facts.effective_target,
        card_facts.evaluated_damage,
    );

    (
        CombatSearchV2ActionImmediateFacts {
            damage_hint,
            action_payload_damage_hint: payload
                .damage_total_hint
                .max(resource_timing.conversion_damage_hint),
            action_payload_damage_hit_count_hint: payload
                .damage_hit_count_hint
                .max(conversion_damage_hit_count),
            block_hint,
            target_progress_hint: target_progress,
            all_enemy_progress_hint: all_enemy_progress,
            exhausts_card: card_facts.exhaust,
            creates_pending_choice_after_one_step: false,
        },
        CombatSearchV2ActionMechanicsFacts {
            direct: CombatSearchV2ActionDirectMechanicsFacts {
                persistent_enemy_strength_down: effects.direct.persistent_enemy_strength_down,
                temporary_enemy_strength_down: effects.direct.temporary_enemy_strength_down,
                visible_attack_mitigation_hint: effects.direct.visible_attack_mitigation_hint,
                enemy_weak: effects.direct.enemy_weak,
                enemy_vulnerable: effects.direct.enemy_vulnerable,
                enemy_strength_gain: effects.direct.enemy_strength_gain,
                visible_attack_pressure_hint: effects.direct.visible_attack_pressure_hint,
                player_strength_gain: effects.direct.player_strength_gain,
                player_temporary_strength_gain: effects.direct.player_temporary_strength_gain,
                player_recurring_turn_energy_delta: effects
                    .direct
                    .player_recurring_turn_energy_delta,
                player_attack_replay_charges: effects.direct.player_attack_replay_charges,
            },
            reactive: CombatSearchV2ActionReactiveMechanicsFacts {
                player_hp_loss: effects.reactive.player_hp_loss,
                attack_retaliation_trigger_count_hint: effects
                    .reactive
                    .attack_retaliation_trigger_count_hint,
                attack_retaliation_raw_player_damage_hint: effects
                    .reactive
                    .attack_retaliation_raw_player_damage_hint,
                attack_retaliation_player_block_loss_hint: effects
                    .reactive
                    .attack_retaliation_player_block_loss_hint,
                attack_retaliation_player_hp_loss_hint: effects
                    .reactive
                    .attack_retaliation_player_hp_loss_hint,
                player_block: effects.reactive.player_block,
                enemy_damage: effects.reactive.enemy_damage,
                bad_draw_cards: effects.reactive.bad_draw_cards,
                forced_turn_end: effects.reactive.forced_turn_end,
                enemy_strength_gain: effects.reactive.enemy_strength_gain,
                visible_attack_pressure_hint: effects.reactive.visible_attack_pressure_hint,
                enemy_weak: effects.reactive.enemy_weak,
                enemy_vulnerable: effects.reactive.enemy_vulnerable,
                future_attack_damage_percent_gain_hint: effects
                    .reactive
                    .future_attack_damage_percent_gain_hint,
            },
            access: CombatSearchV2ActionAccessMechanicsFacts {
                declared_draw_cards: effects.direct.declared_draw_cards,
                conditional_draw_cards: effects.direct.conditional_draw_cards,
                total_draw_cards: effects.total_draw_cards(),
            },
            resource_timing,
            derived: CombatSearchV2ActionDerivedMechanicsFacts {
                mitigation_score: effects.mitigation_ordering_score(),
                enemy_scaling_risk_score: effects.enemy_scaling_risk_score(),
                reactive_risk_score: effects.reactive_risk_score(),
                net_mitigation_score: effects.net_mitigation_ordering_score(),
                enemy_weak: effects
                    .direct
                    .enemy_weak
                    .saturating_add(effects.reactive.enemy_weak),
                enemy_vulnerable: effects
                    .direct
                    .enemy_vulnerable
                    .saturating_add(effects.reactive.enemy_vulnerable),
                enemy_strength_gain: effects
                    .direct
                    .enemy_strength_gain
                    .saturating_add(effects.reactive.enemy_strength_gain),
                visible_attack_pressure_hint: effects
                    .direct
                    .visible_attack_pressure_hint
                    .saturating_add(effects.reactive.visible_attack_pressure_hint),
            },
        },
    )
}
