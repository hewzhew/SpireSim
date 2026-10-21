use super::super::super::action_effects::{card_play_effect_facts, CardPlayEffectFacts};
use crate::ai::analysis::card_semantics::{
    card_definition_with_upgrades, CombatEvent, InstalledRule, Mechanic, PlayEffect,
    TriggeredEffect,
};
use crate::content::cards::{self, CardId, CardTarget, CardType};
use crate::runtime::combat::{CombatCard, CombatState};

pub(super) fn key_setup_card_online_candidate(card: CardId, upgrades: u8) -> bool {
    let definition = card_definition_with_upgrades(card, upgrades);
    definition.play_effects.iter().any(|effect| {
        matches!(
            effect,
            PlayEffect::Provide(
                Mechanic::Strength | Mechanic::TemporaryStrength | Mechanic::StrengthMultiplier
            )
        )
    }) || definition.event_handlers.iter().any(|handler| {
        matches!(
            handler.effect,
            TriggeredEffect::Provide(
                Mechanic::Strength | Mechanic::TemporaryStrength | Mechanic::StrengthMultiplier
            )
        )
    }) || definition
        .installed_rules
        .contains(&InstalledRule::SkillCardsCostZeroAndExhaust)
        || definition.event_handlers.iter().any(|handler| {
            handler.on == CombatEvent::CardExhausted
                && matches!(
                    handler.effect,
                    TriggeredEffect::Provide(Mechanic::Block | Mechanic::CardDraw)
                )
        })
}

pub(super) fn current_turn_attack_setup_score(
    combat: &CombatState,
    card_index: usize,
    target: Option<usize>,
    card: &CombatCard,
    effects: CardPlayEffectFacts,
) -> i32 {
    if effects.direct.player_strength_gain <= 0
        && effects.direct.enemy_vulnerable <= 0
        && effects.reactive.future_attack_damage_percent_gain_hint <= 0
    {
        return 0;
    }

    let setup_target_kind = cards::effective_target(card);
    let setup_cost = card.cost_for_turn_java().max(0);
    let available_energy = i32::from(combat.turn.energy);
    if setup_cost > available_energy {
        return 0;
    }
    let remaining_energy = available_energy - setup_cost;
    let playable_attacks = combat
        .zones
        .hand
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            *index != card_index
                && cards::get_card_definition(candidate.id).card_type == CardType::Attack
                && cards::can_play_card(candidate, combat).is_ok()
                && attack_cost_is_payable_after_setup(candidate, remaining_energy)
                && (effects.direct.player_strength_gain > 0
                    || effects.reactive.future_attack_damage_percent_gain_hint > 0
                    || attack_can_use_enemy_debuff(
                        setup_target_kind,
                        target,
                        cards::effective_target(candidate),
                    ))
        })
        .count() as i32;

    effects
        .direct
        .player_strength_gain
        .saturating_mul(playable_attacks)
        .saturating_add(
            effects
                .direct
                .enemy_vulnerable
                .saturating_mul(playable_attacks),
        )
        .saturating_add(
            effects
                .reactive
                .future_attack_damage_percent_gain_hint
                .saturating_mul(playable_attacks),
        )
}

fn attack_can_use_enemy_debuff(
    setup_target_kind: CardTarget,
    setup_target: Option<usize>,
    attack_target_kind: CardTarget,
) -> bool {
    let setup_reaches_enemy = match setup_target_kind {
        CardTarget::Enemy | CardTarget::SelfAndEnemy => setup_target.is_some(),
        CardTarget::AllEnemy | CardTarget::All => true,
        _ => false,
    };
    let attack_reaches_enemy = matches!(
        attack_target_kind,
        CardTarget::Enemy | CardTarget::SelfAndEnemy | CardTarget::AllEnemy | CardTarget::All
    );
    setup_reaches_enemy && attack_reaches_enemy
}

pub(super) fn current_turn_retaliation_protection_score(
    combat: &CombatState,
    setup_card_index: usize,
    setup_block: i32,
    setup_cost: i32,
    visible_incoming_damage: i32,
) -> i32 {
    let available_energy = i32::from(combat.turn.energy);
    if setup_block <= 0 || setup_cost < 0 || setup_cost > available_energy {
        return 0;
    }
    if !combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .any(|monster| {
            super::super::super::attack_retaliation::attack_retaliation_for_target(
                combat, monster.id,
            )
            .is_some()
        })
    {
        return 0;
    }

    let mut post_setup = combat.clone();
    if setup_card_index >= post_setup.zones.hand.len() {
        return 0;
    }
    post_setup.zones.hand.remove(setup_card_index);
    post_setup.turn.energy = (available_energy - setup_cost) as u8;
    post_setup.turn.counters.cards_played_this_turn = post_setup
        .turn
        .counters
        .cards_played_this_turn
        .saturating_add(1);
    post_setup.entities.player.block = post_setup.entities.player.block.saturating_add(setup_block);

    combat
        .zones
        .hand
        .iter()
        .enumerate()
        .filter(|(index, card)| {
            *index != setup_card_index
                && cards::get_card_definition(card.id).card_type == CardType::Attack
                && cards::can_play_card(card, combat).is_ok()
        })
        .filter_map(|(_, current_attack)| {
            let projected_attack = post_setup
                .zones
                .hand
                .iter()
                .find(|card| card.lineage == current_attack.lineage)?;
            if projected_attack.cost_for_turn_java() < 0 && post_setup.turn.energy == 0 {
                return None;
            }
            if cards::can_play_card(projected_attack, &post_setup).is_err() {
                return None;
            }
            attack_targets(combat, current_attack)
                .into_iter()
                .map(|target| {
                    retaliation_order_hp_benefit(
                        combat,
                        &post_setup,
                        current_attack,
                        projected_attack,
                        target,
                        setup_block,
                        visible_incoming_damage,
                    )
                })
                .max()
        })
        .max()
        .unwrap_or_default()
        .max(0)
}

fn attack_targets(combat: &CombatState, card: &CombatCard) -> Vec<Option<usize>> {
    match cards::effective_target(card) {
        CardTarget::Enemy | CardTarget::SelfAndEnemy => combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .map(|monster| Some(monster.id))
            .collect(),
        CardTarget::AllEnemy | CardTarget::All => vec![None],
        _ => Vec::new(),
    }
}

fn retaliation_order_hp_benefit(
    combat: &CombatState,
    post_setup: &CombatState,
    current_attack: &CombatCard,
    projected_attack: &CombatCard,
    target: Option<usize>,
    setup_block: i32,
    visible_incoming_damage: i32,
) -> i32 {
    let current = card_play_effect_facts(combat, current_attack, target);
    if current.reactive.attack_retaliation_player_hp_loss_hint <= 0 {
        return 0;
    }
    let projected = card_play_effect_facts(post_setup, projected_attack, target);
    let attack_first_remaining_block = combat
        .entities
        .player
        .block
        .saturating_sub(current.reactive.attack_retaliation_player_block_loss_hint)
        .saturating_add(setup_block);
    let block_first_remaining_block = post_setup
        .entities
        .player
        .block
        .saturating_sub(projected.reactive.attack_retaliation_player_block_loss_hint);
    let attack_first_known_loss = current
        .reactive
        .attack_retaliation_player_hp_loss_hint
        .saturating_add(
            visible_incoming_damage
                .saturating_sub(attack_first_remaining_block)
                .max(0),
        );
    let block_first_known_loss = projected
        .reactive
        .attack_retaliation_player_hp_loss_hint
        .saturating_add(
            visible_incoming_damage
                .saturating_sub(block_first_remaining_block)
                .max(0),
        );
    attack_first_known_loss.saturating_sub(block_first_known_loss)
}

fn attack_cost_is_payable_after_setup(card: &CombatCard, remaining_energy: i32) -> bool {
    let cost = card.cost_for_turn_java();
    if cost < 0 {
        return remaining_energy > 0;
    }
    cost <= remaining_energy
}
