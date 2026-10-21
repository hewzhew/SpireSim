use crate::content::cards::{self, CardId, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::combat::{CombatCard, CombatState};
use sts_combat_knowledge::combat_tactical_visible_incoming_damage_v1 as visible_incoming_damage;

use super::action_facts::CombatSearchV2ActionResourceTimingFacts;

pub(in crate::ai::combat_search_v2) fn resource_timing_facts_for_play(
    combat: &CombatState,
    card_index: usize,
    target: Option<usize>,
) -> CombatSearchV2ActionResourceTimingFacts {
    let Some(card) = combat.zones.hand.get(card_index) else {
        return CombatSearchV2ActionResourceTimingFacts::default();
    };
    let targets = hand_exhaust_targets(combat, card_index, card.id);
    if targets.is_empty() {
        return CombatSearchV2ActionResourceTimingFacts::default();
    }

    let evaluated = cards::evaluate_card_for_play(card, combat, target);
    let conversion_damage_hint =
        conversion_damage_hint(card.id, evaluated.base_damage_mut, targets.len());
    let conversion_block_hint =
        conversion_block_hint(card.id, evaluated.base_block_mut, targets.len());
    let target_kind = cards::effective_target(card);
    let progress_hint = target_progress_hint(combat, target_kind, target, conversion_damage_hint);
    let lethal_window = target_progress_kills(combat, target_kind, target, conversion_damage_hint);
    let fuel_count = targets
        .iter()
        .filter(|target| is_conversion_fuel(target))
        .count();
    let high_value_count = targets
        .iter()
        .filter(|target| card_resource_value_at_risk(target) >= 5)
        .count();
    let value_at_risk = targets
        .iter()
        .map(|target| card_resource_value_at_risk(target))
        .sum::<i32>();
    let window_score = conversion_window_score(
        combat,
        card.id,
        target,
        progress_hint,
        lethal_window,
        conversion_block_hint,
        fuel_count,
    );
    let premature_risk = value_at_risk.saturating_sub(window_score).max(0);
    let ordering_score = window_score.saturating_sub(value_at_risk);
    let role_rank_adjustment = if premature_risk > 0 {
        -(premature_risk.saturating_mul(4)).min(50)
    } else {
        ordering_score.clamp(0, 10)
    };

    CombatSearchV2ActionResourceTimingFacts {
        hand_resource_conversion: true,
        hand_exhaust_target_count: targets.len(),
        hand_exhaust_fuel_count: fuel_count,
        hand_exhaust_high_value_count: high_value_count,
        hand_exhaust_value_at_risk: value_at_risk,
        conversion_damage_hint,
        conversion_block_hint,
        conversion_window_score: window_score,
        premature_conversion_risk: premature_risk,
        ordering_score,
        role_rank_adjustment,
    }
}

fn hand_exhaust_targets(
    combat: &CombatState,
    card_index: usize,
    card_id: CardId,
) -> Vec<&CombatCard> {
    combat
        .zones
        .hand
        .iter()
        .enumerate()
        .filter_map(|(index, card)| {
            if index == card_index || !card_would_be_consumed_by(card_id, card) {
                return None;
            }
            Some(card)
        })
        .collect()
}

fn card_would_be_consumed_by(source: CardId, target: &CombatCard) -> bool {
    match source {
        CardId::FiendFire => true,
        CardId::SecondWind | CardId::SeverSoul => {
            cards::get_card_definition(target.id).card_type != CardType::Attack
        }
        _ => false,
    }
}

fn conversion_damage_hint(card_id: CardId, base_damage: i32, target_count: usize) -> i32 {
    match card_id {
        CardId::FiendFire => base_damage.max(0).saturating_mul(target_count as i32),
        CardId::SeverSoul => base_damage.max(0),
        _ => 0,
    }
}

fn conversion_block_hint(card_id: CardId, base_block: i32, target_count: usize) -> i32 {
    match card_id {
        CardId::SecondWind => base_block.max(0).saturating_mul(target_count as i32),
        _ => 0,
    }
}

fn card_resource_value_at_risk(card: &CombatCard) -> i32 {
    let definition = cards::get_card_definition(card.id);
    let base = match card.id {
        CardId::Offering | CardId::Shockwave => 8,
        CardId::DemonForm | CardId::Corruption | CardId::DarkEmbrace | CardId::FeelNoPain => 7,
        CardId::Barricade | CardId::SpotWeakness | CardId::LimitBreak | CardId::Inflame => 6,
        CardId::BattleTrance | CardId::BurningPact | CardId::PommelStrike | CardId::ShrugItOff => 5,
        CardId::Bloodletting
        | CardId::SeeingRed
        | CardId::Bash
        | CardId::Uppercut
        | CardId::Disarm
        | CardId::Impervious
        | CardId::FlameBarrier
        | CardId::PowerThrough
        | CardId::FiendFire
        | CardId::SecondWind
        | CardId::SeverSoul => 4,
        CardId::Strike | CardId::Defend => 0,
        _ if matches!(definition.card_type, CardType::Power) => 5,
        _ if matches!(definition.card_type, CardType::Curse | CardType::Status) => 0,
        _ => 1,
    };
    base + i32::from(card.upgrades > 0 && base > 0)
}

fn is_conversion_fuel(card: &CombatCard) -> bool {
    let definition = cards::get_card_definition(card.id);
    matches!(
        card.id,
        CardId::Strike
            | CardId::Defend
            | CardId::Burn
            | CardId::Dazed
            | CardId::Slimed
            | CardId::Wound
            | CardId::Clash
            | CardId::WildStrike
            | CardId::RecklessCharge
    ) || matches!(definition.card_type, CardType::Curse | CardType::Status)
}

fn conversion_window_score(
    combat: &CombatState,
    card_id: CardId,
    target: Option<usize>,
    progress_hint: i32,
    lethal_window: bool,
    conversion_block_hint: i32,
    fuel_count: usize,
) -> i32 {
    let mut score = (fuel_count as i32).saturating_mul(2);
    if lethal_window {
        score = score.saturating_add(30);
    } else if progress_hint >= 40 {
        score = score.saturating_add(12);
    } else if progress_hint >= 25 {
        score = score.saturating_add(8);
    } else if progress_hint >= 15 {
        score = score.saturating_add(4);
    }

    if combat.get_power(0, PowerId::Strength) > 0 {
        score = score.saturating_add(combat.get_power(0, PowerId::Strength).min(5));
    }
    if target.is_some_and(|target| combat.get_power(target, PowerId::Vulnerable) > 0) {
        score = score.saturating_add(5);
    }

    if card_id == CardId::SecondWind {
        let visible_loss = (visible_incoming_damage(combat) - combat.entities.player.block).max(0);
        if visible_loss > 0 && conversion_block_hint >= visible_loss {
            score = score.saturating_add(12);
        } else if visible_loss > 0 && conversion_block_hint > 0 {
            score = score.saturating_add(6);
        }
    }

    score
}

fn target_progress_hint(
    combat: &CombatState,
    target_kind: CardTarget,
    target: Option<usize>,
    damage: i32,
) -> i32 {
    if damage <= 0 {
        return 0;
    }

    match target_kind {
        CardTarget::AllEnemy => combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .map(|monster| damage.min(monster.current_hp + monster.block).max(0))
            .sum(),
        CardTarget::Enemy | CardTarget::SelfAndEnemy => target
            .and_then(|target| monster_hp_with_block(combat, target))
            .map(|hp| damage.min(hp).max(0))
            .unwrap_or_default(),
        _ => 0,
    }
}

fn target_progress_kills(
    combat: &CombatState,
    target_kind: CardTarget,
    target: Option<usize>,
    damage: i32,
) -> bool {
    if damage <= 0 {
        return false;
    }

    match target_kind {
        CardTarget::AllEnemy => combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .any(|monster| damage >= monster.current_hp + monster.block),
        CardTarget::Enemy | CardTarget::SelfAndEnemy => target
            .and_then(|target| monster_hp_with_block(combat, target))
            .is_some_and(|hp| damage >= hp),
        _ => false,
    }
}

fn monster_hp_with_block(combat: &CombatState, entity_id: usize) -> Option<i32> {
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == entity_id && monster.is_alive_for_action())
        .map(|monster| monster.current_hp + monster.block)
}
