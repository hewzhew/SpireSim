use super::*;
use crate::content::powers::PowerId;
use crate::runtime::combat::CombatCard;

mod attack_retaliation_observation;
mod monster_signals;
mod observation;
mod reactive_observation;
use super::super::{visible_incoming_damage, visible_incoming_damage_with_player_intangible};
use monster_signals::{
    is_living_monster_id, visible_strength_down_mitigation_hint,
    visible_strength_gain_pressure_hint, visible_weak_mitigation_hint,
};
use observation::{observe_card_play_effects, CardPlayEffectAccumulator};
use sts_combat_knowledge::combat_tactical_monster_attack_relevance_v1 as monster_attack_relevance;

pub(super) fn card_play_effect_facts(
    combat: &CombatState,
    card: &CombatCard,
    target: Option<usize>,
) -> CardPlayEffectFacts {
    let actions = crate::content::cards::resolve_card_play_with_context(
        card.id,
        combat,
        card,
        target,
        crate::content::cards::CardUseContext {
            played_from_hand: true,
        },
    );
    let accumulator =
        observe_card_play_effects(combat, card, actions.into_iter().map(|info| info.action));
    card_play_facts_from_accumulator(combat, accumulator)
}

fn card_play_facts_from_accumulator(
    combat: &CombatState,
    accumulator: CardPlayEffectAccumulator,
) -> CardPlayEffectFacts {
    let mut reactive = accumulator.reactive;
    reactive.player_hp_loss = reactive
        .player_hp_loss
        .saturating_add(accumulator.direct.player_hp_loss);
    let mut facts = CardPlayEffectFacts {
        direct: DirectCardPlayEffectFacts {
            player_block: accumulator.direct.player_block,
            declared_draw_cards: accumulator.direct.declared_draw_cards,
            conditional_draw_cards: accumulator.direct.conditional_draw_cards,
            enemy_weak: accumulator.direct.enemy_weak,
            enemy_vulnerable: accumulator.direct.enemy_vulnerable,
            player_vulnerable: accumulator.direct.player_vulnerable,
            player_strength_gain: accumulator.direct.player_strength_gain,
            player_temporary_strength_gain: accumulator
                .direct
                .player_strength_gain
                .min(accumulator.direct.player_lose_strength),
            player_recurring_turn_energy_delta: accumulator
                .direct
                .player_recurring_turn_energy_delta,
            player_attack_replay_charges: accumulator.direct.player_attack_replay_charges,
            player_energy_gain: accumulator.direct.player_energy_gain,
            ..DirectCardPlayEffectFacts::default()
        },
        reactive,
    };

    for (target, amount) in accumulator.direct.enemy_strength_down_by_target {
        if !is_living_monster_id(combat, target) {
            continue;
        }
        let weighted_amount = amount.saturating_mul(monster_attack_relevance(combat, target));
        if accumulator.direct.shackled_targets.contains(&target) {
            facts.direct.temporary_enemy_strength_down = facts
                .direct
                .temporary_enemy_strength_down
                .saturating_add(weighted_amount);
        } else {
            facts.direct.persistent_enemy_strength_down = facts
                .direct
                .persistent_enemy_strength_down
                .saturating_add(weighted_amount);
        }
        facts.direct.visible_attack_mitigation_hint =
            facts.direct.visible_attack_mitigation_hint.saturating_add(
                visible_strength_down_mitigation_hint(combat, target, amount),
            );
    }
    for (target, amount) in accumulator.direct.enemy_strength_gain_by_target {
        if !is_living_monster_id(combat, target) {
            continue;
        }
        let weighted_amount = amount.saturating_mul(monster_attack_relevance(combat, target));
        facts.direct.enemy_strength_gain = facts
            .direct
            .enemy_strength_gain
            .saturating_add(weighted_amount);
        facts.direct.visible_attack_pressure_hint = facts
            .direct
            .visible_attack_pressure_hint
            .saturating_add(visible_strength_gain_pressure_hint(combat, target, amount));
    }
    for (target, amount) in accumulator.direct.enemy_weak_by_target {
        if !is_living_monster_id(combat, target) {
            continue;
        }
        facts.direct.visible_attack_mitigation_hint = facts
            .direct
            .visible_attack_mitigation_hint
            .saturating_add(visible_weak_mitigation_hint(combat, target, amount));
    }
    if accumulator.direct.player_intangible_gain > 0
        && combat.get_power(combat.entities.player.id, PowerId::IntangiblePlayer) <= 0
    {
        let mitigation = visible_incoming_damage(combat)
            .saturating_sub(visible_incoming_damage_with_player_intangible(combat, true))
            .max(0);
        facts.direct.visible_attack_mitigation_hint = facts
            .direct
            .visible_attack_mitigation_hint
            .saturating_add(mitigation);
    }

    facts
}
