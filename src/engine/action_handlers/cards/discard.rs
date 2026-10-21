use crate::content::cards::CardId;
use crate::runtime::action::Action;
use crate::runtime::combat::{CardLineageId, CombatState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardHookOrder {
    /// Java end-turn all-card discard path: count the discard, but do not
    /// trigger manual-discard card hooks or relic hooks.
    None,
    /// Java DiscardAction random path with endTurn=true: card hook still fires,
    /// but GameActionManager.incrementDiscard does not fire relic hooks.
    CardOnly,
    /// Java DiscardAction: moveToDiscardPile, triggerOnManualDiscard,
    /// then GameActionManager.incrementDiscard.
    CardThenRelics,
    /// Java DiscardSpecificCardAction/GamblingChipAction: moveToDiscardPile,
    /// GameActionManager.incrementDiscard, then triggerOnManualDiscard.
    RelicsThenCard,
}

fn queue_manual_discard_hooks(
    card: &crate::runtime::combat::CombatCard,
    order: DiscardHookOrder,
    state: &mut CombatState,
) {
    match order {
        DiscardHookOrder::None => {
            state.turn.increment_cards_discarded();
        }
        DiscardHookOrder::CardOnly => {
            let card_actions = crate::content::cards::resolve_card_on_manual_discard(card, state);
            state.queue_actions(card_actions);
            state.turn.increment_cards_discarded();
        }
        DiscardHookOrder::CardThenRelics => {
            let card_actions = crate::content::cards::resolve_card_on_manual_discard(card, state);
            state.queue_actions(card_actions);
            state.turn.increment_cards_discarded();
            apply_player_update_cards_on_discard(state);
            let relic_actions = crate::content::relics::hooks::on_discard(state);
            state.queue_actions(relic_actions);
        }
        DiscardHookOrder::RelicsThenCard => {
            state.turn.increment_cards_discarded();
            apply_player_update_cards_on_discard(state);
            let relic_actions = crate::content::relics::hooks::on_discard(state);
            state.queue_actions(relic_actions);
            let card_actions = crate::content::cards::resolve_card_on_manual_discard(card, state);
            state.queue_actions(card_actions);
        }
    }
}

fn apply_player_update_cards_on_discard(state: &mut CombatState) {
    for card in state
        .zones
        .hand
        .iter_mut()
        .chain(state.zones.discard_pile.iter_mut())
        .chain(state.zones.draw_pile.iter_mut())
    {
        if card.id == CardId::Eviscerate {
            card.set_cost_for_turn_java(card.cost_for_turn_java() - 1);
        }
    }
}

fn move_hand_card_to_discard_at(
    pos: usize,
    hook_order: DiscardHookOrder,
    state: &mut CombatState,
    moved: &mut Vec<CardLineageId>,
) {
    let card = state.zones.hand.remove(pos);
    // Soul.discard inserts immediately, so this card also receives didDiscard.
    // Soul.update clears its temporary attributes once the movement settles.
    moved.push(card.lineage);
    begin_discard_movement(card.clone(), state);
    queue_manual_discard_hooks(&card, hook_order, state);
}

/// CardGroup.moveToDiscardPile, after removal from the source group. Raw
/// generated-card overflow does not call this hook in the original game.
pub fn move_card_to_discard_pile(
    card: crate::runtime::combat::CombatCard,
    state: &mut CombatState,
) {
    let lineage = card.lineage;
    begin_discard_movement(card, state);
    settle_discard_movements(state, &[lineage]);
}

fn begin_discard_movement(card: crate::runtime::combat::CombatCard, state: &mut CombatState) {
    state.add_card_to_discard_pile_top(card);
    crate::content::cards::ironclad::corruption::corruption_on_draw_or_discard(state);
}

fn settle_discard_movements(state: &mut CombatState, moved: &[CardLineageId]) {
    // Soul.update -> clearPowers -> resetAttributes, after this action's
    // synchronous discard notifications. Pre-existing pile cards keep their
    // didDiscard reductions; only cards moved by this action are reset.
    for card in state.zones.discard_pile.iter_mut() {
        if moved.contains(&card.lineage) {
            card.reset_attributes_java();
        }
    }
}

pub fn handle_scrape_follow_up(state: &mut CombatState) {
    let drawn = state.runtime.last_drawn_cards.clone();
    let mut moved = Vec::new();
    for record in drawn {
        let Some(pos) = state
            .zones
            .hand
            .iter()
            .position(|card| card.lineage == record.card_lineage)
        else {
            continue;
        };
        let card = &state.zones.hand[pos];
        if card.cost_for_turn_java() == 0 || card.free_to_play_once {
            continue;
        }
        move_hand_card_to_discard_at(pos, DiscardHookOrder::CardThenRelics, state, &mut moved);
    }
    settle_discard_movements(state, &moved);
}

pub fn handle_calculated_gamble(draw_extra: bool, state: &mut CombatState) {
    let count = state.zones.hand.len() as u32;
    if count == 0 && !draw_extra {
        return;
    }

    let draw_count = count + u32::from(draw_extra);
    state.queue_action_front(Action::DrawCards(draw_count));
    state.queue_action_front(Action::DiscardFromHand {
        amount: count as i32,
        random: true,
        end_turn: false,
    });
}

pub fn handle_discard_card(card_lineage: CardLineageId, state: &mut CombatState) {
    handle_discard_cards_with_order(&[card_lineage], DiscardHookOrder::RelicsThenCard, state);
}

pub fn handle_discard_cards_with_order(
    card_lineages: &[CardLineageId],
    hook_order: DiscardHookOrder,
    state: &mut CombatState,
) {
    let mut moved = Vec::with_capacity(card_lineages.len());
    for card_lineage in card_lineages {
        if let Some(pos) = state
            .zones
            .hand
            .iter()
            .position(|c| c.lineage == *card_lineage)
        {
            move_hand_card_to_discard_at(pos, hook_order, state, &mut moved);
        }
    }
    settle_discard_movements(state, &moved);
}

pub fn handle_discard_from_hand(
    amount: i32,
    random: bool,
    end_turn: bool,
    state: &mut CombatState,
) {
    if state.are_monsters_basically_dead_java() {
        return;
    }

    if state.zones.hand.is_empty() {
        return;
    }

    if amount < 0 && !random {
        state.queue_action_front(Action::SuspendForHandSelect {
            min: 0,
            max: 99,
            can_cancel: true,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::Discard,
        });
        return;
    }

    let amount = amount.max(0) as usize;
    let mut moved = Vec::new();
    if state.zones.hand.len() <= amount {
        let hook_order = if end_turn {
            DiscardHookOrder::None
        } else {
            DiscardHookOrder::CardThenRelics
        };
        while !state.zones.hand.is_empty() {
            let top = state.zones.hand.len() - 1;
            move_hand_card_to_discard_at(top, hook_order, state, &mut moved);
        }
        settle_discard_movements(state, &moved);
        return;
    }

    if random {
        let hook_order = if end_turn {
            DiscardHookOrder::CardOnly
        } else {
            DiscardHookOrder::CardThenRelics
        };
        for _ in 0..amount {
            if state.zones.hand.is_empty() {
                break;
            }
            let idx = state
                .rng
                .card_random_rng
                .random(state.zones.hand.len() as i32 - 1) as usize;
            move_hand_card_to_discard_at(idx, hook_order, state, &mut moved);
        }
        settle_discard_movements(state, &moved);
        return;
    }

    state.queue_action_front(Action::SuspendForHandSelect {
        min: amount as u8,
        max: amount as u8,
        can_cancel: false,
        filter: crate::state::HandSelectFilter::Any,
        reason: crate::state::HandSelectReason::Discard,
    });
}

pub fn handle_discard_to_hand(
    card_lineage: CardLineageId,
    cost_for_turn: Option<u8>,
    state: &mut CombatState,
) {
    if state.zones.hand.len() >= 10 {
        return;
    }

    let Some(pos) = state
        .zones
        .discard_pile
        .iter()
        .position(|card| card.lineage == card_lineage)
    else {
        return;
    };

    let mut card = state.zones.discard_pile.remove(pos);
    if let Some(cost) = cost_for_turn {
        card.set_cost_for_turn_java(cost as i32);
    }
    crate::content::cards::evaluate_card(&mut card, state, None);
    state.add_card_to_hand(card);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::combat::CombatCard;

    fn card(id: CardId, n: u32) -> CombatCard {
        CombatCard::for_scenario(id, n)
    }

    #[test]
    fn discard_settlement_resets_moved_eviscerate_but_preserves_other_pile_discounts() {
        let mut state = crate::test_support::blank_test_combat();
        let departing = card(CardId::Eviscerate, 101);
        let remaining = card(CardId::Eviscerate, 102);
        state.zones.hand = vec![departing.clone(), remaining.clone()];
        state.zones.draw_pile = vec![card(CardId::Eviscerate, 103)].into();
        state.zones.discard_pile = vec![card(CardId::Eviscerate, 104)].into();
        handle_discard_card(departing.lineage, &mut state);
        assert_eq!(state.turn.counters.cards_discarded_this_turn, 1);
        assert_eq!(state.zones.discard_pile[0].cost_for_turn_java(), 2);
        assert_eq!(state.zones.discard_pile[1].cost_for_turn_java(), 3);
        assert_eq!(state.zones.hand[0].cost_for_turn_java(), 2);
        assert_eq!(state.zones.draw_pile[0].cost_for_turn_java(), 2);
        // At the next action, the earlier arrival is part of the settled pile.
        handle_discard_card(remaining.lineage, &mut state);
        assert_eq!(state.zones.discard_pile[1].cost_for_turn_java(), 2);
        assert_eq!(state.zones.discard_pile[2].cost_for_turn_java(), 3);
    }

    #[test]
    fn one_discard_batch_settles_after_all_notifications_and_resets_every_moved_card() {
        for all_cards in [false, true] {
            let mut state = crate::test_support::blank_test_combat();
            state
                .entities
                .monsters
                .push(crate::test_support::test_monster(
                    crate::content::monsters::EnemyId::JawWorm,
                ));
            let first = card(CardId::Eviscerate, 201);
            let middle = card(CardId::StrikeG, 202);
            let last = card(CardId::Eviscerate, 203);
            state.zones.hand = vec![first.clone(), middle.clone(), last.clone()];
            state.zones.draw_pile = vec![card(CardId::Eviscerate, 204)].into();
            state.zones.discard_pile = vec![card(CardId::Eviscerate, 205)].into();
            if all_cards {
                handle_discard_from_hand(3, false, false, &mut state);
                assert_eq!(state.turn.counters.cards_discarded_this_turn, 3);
                assert_eq!(state.zones.discard_pile[1].lineage, last.lineage);
                assert_eq!(state.zones.discard_pile[1].cost_for_turn_java(), 3);
                assert_eq!(state.zones.discard_pile[3].lineage, first.lineage);
                assert_eq!(state.zones.discard_pile[3].cost_for_turn_java(), 3);
                assert_eq!(state.zones.draw_pile[0].cost_for_turn_java(), 0);
            } else {
                handle_discard_cards_with_order(
                    &[first.lineage, middle.lineage],
                    DiscardHookOrder::CardThenRelics,
                    &mut state,
                );
                assert_eq!(state.turn.counters.cards_discarded_this_turn, 2);
                assert_eq!(state.zones.discard_pile[1].cost_for_turn_java(), 3);
                assert_eq!(state.zones.hand[0].lineage, last.lineage);
                assert_eq!(state.zones.hand[0].cost_for_turn_java(), 1);
                assert_eq!(state.zones.draw_pile[0].cost_for_turn_java(), 1);
            }
        }
    }

    #[test]
    fn settled_movement_clears_temporary_cost_but_raw_generated_overflow_keeps_it() {
        let mut state = crate::test_support::blank_test_combat();
        let mut moving = card(CardId::WraithForm, 301);
        moving.set_cost_for_turn_java(0);
        moving.free_to_play_once = true;
        move_card_to_discard_pile(moving, &mut state);
        assert_eq!(state.zones.discard_pile[0].cost_for_turn_java(), 3);
        assert!(state.zones.discard_pile[0].free_to_play_once);
        let mut generated = card(CardId::WraithForm, 302);
        generated.set_cost_for_turn_java(0);
        state.add_card_to_discard_pile_top(generated);
        assert_eq!(state.zones.discard_pile[1].cost_for_turn_java(), 0);
    }
}
