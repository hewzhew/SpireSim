use super::exhaust::move_card_to_exhaust_pile;
use crate::content::cards::CardId;
use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::{CardLineageId, CombatState};

pub fn handle_all_cost_to_hand(cost_target: i32, state: &mut CombatState) {
    let matching_lineages: Vec<CardLineageId> = state
        .zones
        .discard_pile
        .iter()
        .filter(|card| {
            card.combat_cost_without_turn_override_java() == cost_target || card.free_to_play_once
        })
        .map(|card| card.lineage)
        .collect();

    for lineage in matching_lineages {
        state.queue_action_back(Action::DiscardToHand {
            card_lineage: lineage,
            cost_for_turn: None,
        });
    }
}

pub fn handle_move_card(
    card_lineage: CardLineageId,
    from: crate::state::PileType,
    to: crate::state::PileType,
    state: &mut CombatState,
) {
    let mut removed_card = None;
    match from {
        crate::state::PileType::Hand => {
            if let Some(pos) = state
                .zones
                .hand
                .iter()
                .position(|c| c.lineage == card_lineage)
            {
                removed_card = Some(state.zones.hand.remove(pos));
            }
        }
        crate::state::PileType::Draw => {
            if let Some(pos) = state
                .zones
                .draw_pile
                .iter()
                .position(|c| c.lineage == card_lineage)
            {
                removed_card = Some(state.zones.draw_pile.remove(pos));
            }
        }
        crate::state::PileType::Discard => {
            removed_card = state.zones.discard_pile.remove_by_lineage(card_lineage);
        }
        crate::state::PileType::Exhaust => {
            if let Some(pos) = state
                .zones
                .exhaust_pile
                .iter()
                .position(|c| c.lineage == card_lineage)
            {
                removed_card = Some(state.zones.exhaust_pile.remove(pos));
            }
        }
        crate::state::PileType::Limbo => {
            if let Some(pos) = state
                .zones
                .limbo
                .iter()
                .position(|c| c.lineage == card_lineage)
            {
                removed_card = Some(state.zones.limbo.remove(pos));
            }
        }
        _ => {}
    }
    if let Some(card) = removed_card {
        match to {
            crate::state::PileType::Hand => {
                if state.zones.hand.len() < 10 {
                    state.add_card_to_hand(card);
                } else {
                    state.add_card_to_discard_pile_top(card);
                }
            }
            crate::state::PileType::Draw => state.add_card_to_draw_pile_top(card),
            crate::state::PileType::Discard => {
                super::discard::move_card_to_discard_pile(card, state)
            }
            crate::state::PileType::Exhaust => {
                if matches!(from, crate::state::PileType::Exhaust) {
                    state.add_card_to_exhaust_pile_top(card);
                } else {
                    move_card_to_exhaust_pile(card, state);
                }
            }
            _ => {}
        }
    }
}

fn monsters_are_basically_dead(state: &CombatState) -> bool {
    state.are_monsters_basically_dead_java()
}

pub fn handle_discard_pile_to_top_of_deck(state: &mut CombatState) {
    if monsters_are_basically_dead(state) {
        return;
    }

    match state.zones.discard_pile.len() {
        0 => {}
        1 => {
            let card_lineage = state.zones.discard_pile[0].lineage;
            handle_move_card(
                card_lineage,
                crate::state::PileType::Discard,
                crate::state::PileType::Draw,
                state,
            );
        }
        _ => state.queue_action_front(Action::SuspendForGridSelect {
            source_pile: crate::state::PileType::Discard,
            min: 1,
            max: 1,
            can_cancel: false,
            filter: crate::state::GridSelectFilter::Any,
            reason: crate::state::GridSelectReason::MoveToDrawPile,
        }),
    }
}

pub fn handle_exhume_card(card_lineage: CardLineageId, upgrade: bool, state: &mut CombatState) {
    if state.zones.hand.len() >= 10 {
        return;
    }

    let Some(pos) = state
        .zones
        .exhaust_pile
        .iter()
        .position(|c| c.lineage == card_lineage && c.id != CardId::Exhume)
    else {
        return;
    };

    let mut card = state.zones.exhaust_pile.remove(pos);
    if upgrade {
        crate::content::cards::upgrade_card_once_java(&mut card);
    }
    if store::has_power(state, 0, PowerId::Corruption)
        && crate::content::cards::get_card_definition(card.id).card_type
            == crate::content::cards::CardType::Skill
    {
        card.set_cost_for_turn_java(0);
    }
    state.add_card_to_hand(card);
}

pub fn handle_remove_card_from_pile(
    card_lineage: CardLineageId,
    from: crate::state::PileType,
    state: &mut CombatState,
) {
    if from == crate::state::PileType::Draw {
        state.zones.draw_pile.remove_by_lineage(card_lineage);
        return;
    }
    if from == crate::state::PileType::Discard {
        state.zones.discard_pile.remove_by_lineage(card_lineage);
        return;
    }
    let source = match from {
        crate::state::PileType::Hand => &mut state.zones.hand,
        crate::state::PileType::Draw => unreachable!("draw pile handled above"),
        crate::state::PileType::Discard => unreachable!("discard pile handled above"),
        crate::state::PileType::Exhaust => &mut state.zones.exhaust_pile,
        crate::state::PileType::Limbo => &mut state.zones.limbo,
        crate::state::PileType::MasterDeck => return,
    };
    if let Some(pos) = source.iter().position(|c| c.lineage == card_lineage) {
        source.remove(pos);
    }
}

pub fn handle_draw_pile_to_hand_by_type(
    amount: u8,
    card_type: crate::content::cards::CardType,
    state: &mut CombatState,
) {
    let mut candidates: Vec<CardLineageId> = Vec::new();
    let matching_lineages: Vec<CardLineageId> = state
        .zones
        .draw_pile
        .iter()
        .rev()
        .filter(|card| crate::content::cards::get_card_definition(card.id).card_type == card_type)
        .map(|card| card.lineage)
        .collect();
    for lineage in matching_lineages {
        if candidates.is_empty() {
            candidates.push(lineage);
        } else {
            let index = state
                .rng
                .card_random_rng
                .random(candidates.len() as i32 - 1) as usize;
            candidates.insert(index, lineage);
        }
    }

    for _ in 0..amount {
        if candidates.is_empty() {
            break;
        }
        crate::runtime::rng::shuffle_with_random_long(&mut candidates, &mut state.rng.shuffle_rng);
        let chosen_lineage = candidates.remove(0);
        if let Some(pos) = state
            .zones
            .draw_pile
            .iter()
            .position(|card| card.lineage == chosen_lineage)
        {
            let card = state.zones.draw_pile.remove(pos);
            if state.zones.hand.len() < 10 {
                state.add_card_to_hand(card);
            } else {
                state.add_card_to_discard_pile_top(card);
            }
        }
    }
}

pub fn handle_meditate(amount: u8, state: &mut CombatState) {
    if amount == 0 || state.zones.discard_pile.is_empty() {
        return;
    }

    if state.zones.discard_pile.len() <= amount as usize {
        let lineages: Vec<CardLineageId> = state
            .zones
            .discard_pile
            .iter()
            .map(|card| card.lineage)
            .collect();
        for lineage in lineages {
            let pos = state
                .zones
                .discard_pile
                .iter()
                .position(|card| card.lineage == lineage);
            if let Some(pos) = pos {
                state.zones.discard_pile[pos].retain_override = Some(true);
                if state.zones.hand.len() < 10 {
                    let card = state.zones.discard_pile.remove(pos);
                    state.add_card_to_hand(card);
                }
            }
        }
        return;
    }

    state.queue_action_front(Action::SuspendForGridSelect {
        source_pile: crate::state::PileType::Discard,
        min: amount,
        max: amount,
        can_cancel: false,
        filter: crate::state::GridSelectFilter::Any,
        reason: crate::state::GridSelectReason::DiscardToHandRetain,
    });
}
