use super::super::action_effects::CardPlayEffectFacts;
use super::super::enemy_mechanics_profile::EnemyMechanicsProfileV1;
use super::{damage_projection, PhaseProjection};
use crate::content::cards::{self, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct AwakenedOneStrengthTransitionOpportunity {
    pub(in crate::ai::combat_search_v2) temporary_strength_down: i32,
    pub(in crate::ai::combat_search_v2) convertible_positive_strength: i32,
    pub(in crate::ai::combat_search_v2) remaining_damage_upper_bound: i32,
    pub(in crate::ai::combat_search_v2) phase_one_hp_with_block: i32,
}

pub(in crate::ai::combat_search_v2) fn awakened_one_strength_transition_opportunity(
    combat: &CombatState,
    setup_card_index: usize,
    target: Option<usize>,
    profile: EnemyMechanicsProfileV1,
    effects: CardPlayEffectFacts,
) -> Option<AwakenedOneStrengthTransitionOpportunity> {
    let target_id = profile.awakened_one_form_one_target?;
    let hp_with_block = profile.awakened_one_form_one_hp_with_block?;
    let positive_strength = profile.awakened_one_positive_strength?.max(0);
    let weighted_temporary_strength_down = effects.direct.temporary_enemy_strength_down.max(0);
    if target != Some(target_id) || positive_strength == 0 || weighted_temporary_strength_down == 0
    {
        return None;
    }
    let temporary_strength_down =
        raw_temporary_strength_down_for_target(combat, setup_card_index, target_id);
    if temporary_strength_down == 0 {
        return None;
    }

    let remaining_damage_upper_bound =
        remaining_hand_damage_upper_bound(combat, setup_card_index, target_id);
    if remaining_damage_upper_bound < hp_with_block {
        return None;
    }

    Some(AwakenedOneStrengthTransitionOpportunity {
        temporary_strength_down,
        convertible_positive_strength: temporary_strength_down.min(positive_strength),
        remaining_damage_upper_bound,
        phase_one_hp_with_block: hp_with_block,
    })
}

fn raw_temporary_strength_down_for_target(
    combat: &CombatState,
    card_index: usize,
    target_id: usize,
) -> i32 {
    let Some(card) = combat.zones.hand.get(card_index) else {
        return 0;
    };
    let mut candidate = card.clone();
    if candidate.cost_for_turn_java() < 0 {
        candidate.energy_on_use = i32::from(combat.turn.energy);
    }
    let evaluated = cards::evaluate_card_for_play(&candidate, combat, Some(target_id));
    let actions = cards::resolve_card_play_with_context(
        evaluated.id,
        combat,
        &evaluated,
        Some(target_id),
        cards::CardUseContext {
            played_from_hand: true,
        },
    );
    let mut strength_loss = 0i32;
    let mut shackled = 0i32;
    for info in actions {
        match info.action {
            Action::ApplyPower {
                target,
                power_id,
                amount,
                ..
            }
            | Action::ApplyPowerDetailed {
                target,
                power_id,
                amount,
                ..
            }
            | Action::ApplyPowerWithPayload {
                target,
                power_id,
                amount,
                ..
            } if target == target_id => match power_id {
                PowerId::Strength if amount < 0 => {
                    strength_loss = strength_loss.saturating_add(amount.saturating_neg());
                }
                PowerId::Shackled if amount > 0 => {
                    shackled = shackled.saturating_add(amount);
                }
                _ => {}
            },
            _ => {}
        }
    }
    strength_loss.min(shackled)
}

fn remaining_hand_damage_upper_bound(
    combat: &CombatState,
    setup_card_index: usize,
    target_id: usize,
) -> i32 {
    let Some(setup) = combat.zones.hand.get(setup_card_index) else {
        return 0;
    };
    let available = i32::from(combat.turn.energy);
    if cards::can_play_card(setup, combat).is_err() {
        return 0;
    }
    let setup_cost = if setup.cost_for_turn_java() < 0 {
        available
    } else {
        setup.cost_for_turn_java().max(0)
    };
    if setup_cost > available {
        return 0;
    }

    let mut post_setup = combat.clone();
    post_setup.zones.hand.remove(setup_card_index);
    post_setup.turn.energy = (available - setup_cost) as u8;
    post_setup.turn.counters.cards_played_this_turn = post_setup
        .turn
        .counters
        .cards_played_this_turn
        .saturating_add(1);

    let energy = usize::from(post_setup.turn.energy);
    let mut best = vec![0i32; energy + 1];
    for card in &post_setup.zones.hand {
        if cards::get_card_definition(card.id).card_type != CardType::Attack
            || cards::can_play_card_ignoring_energy(card, &post_setup).is_err()
        {
            continue;
        }
        let before = best.clone();
        if card.cost_for_turn_java() < 0 {
            for spend in 0..=energy {
                let damage = projected_attack_damage(&post_setup, card, target_id, spend as i32);
                for budget in spend..=energy {
                    best[budget] = best[budget].max(before[budget - spend].saturating_add(damage));
                }
            }
        } else {
            let cost = card.cost_for_turn_java().max(0) as usize;
            if cost > energy {
                continue;
            }
            let damage = projected_attack_damage(&post_setup, card, target_id, 0);
            for budget in cost..=energy {
                best[budget] = best[budget].max(before[budget - cost].saturating_add(damage));
            }
        }
    }
    best[energy]
}

fn projected_attack_damage(
    combat: &CombatState,
    card: &CombatCard,
    target_id: usize,
    energy_on_use: i32,
) -> i32 {
    let Some(monster) = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target_id && monster.is_alive_for_action())
    else {
        return 0;
    };
    let before_effort = monster.current_hp.saturating_add(monster.block);
    let mut candidate = card.clone();
    if candidate.cost_for_turn_java() < 0 {
        candidate.energy_on_use = energy_on_use;
    }
    let target = match cards::effective_target(&candidate) {
        CardTarget::Enemy | CardTarget::SelfAndEnemy => Some(target_id),
        CardTarget::AllEnemy | CardTarget::All => None,
        CardTarget::SelfTarget | CardTarget::None => return 0,
    };
    let evaluated = cards::evaluate_card_for_play(&candidate, combat, target);
    let actions = cards::resolve_card_play_with_context(
        evaluated.id,
        combat,
        &evaluated,
        target,
        cards::CardUseContext {
            played_from_hand: true,
        },
    );
    let mut projection = PhaseProjection::from_combat(combat);
    damage_projection::observe_actions_damage(
        &mut projection,
        actions.into_iter().map(|info| info.action),
    );
    let Some(projected) = projection.monsters.get(&target_id) else {
        return 0;
    };
    before_effort.saturating_sub(
        projected
            .projected_hp
            .saturating_add(projected.projected_block),
    )
}
