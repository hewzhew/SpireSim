use crate::content::cards::CardId;
use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use crate::state::selection::{DomainCardSnapshot, DomainEvent, DomainEventSource};

fn apply_card_trigger_when_drawn(
    card: &mut crate::runtime::combat::CombatCard,
    state: &mut CombatState,
) {
    if card.id == CardId::Eviscerate {
        card.set_cost_for_turn_java(
            card.combat_cost_without_turn_override_java()
                - state.turn.counters.cards_discarded_this_turn as i32,
        );
    } else if card.id == CardId::EndlessAgony {
        let constructed =
            crate::content::cards::prepare_make_temp_card_in_hand_constructor(card.clone(), state);
        state.queue_action_front(Action::MakeConstructedCopyInHand {
            original: Box::new(constructed),
            amount: 1,
        });
    } else if card.id == CardId::DeusExMachina {
        let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
        let amount = evaluated.base_magic_num_mut.max(0).min(u8::MAX as i32) as u8;
        state.queue_action_front(
            crate::content::cards::make_constructed_temp_card_in_hand_action(
                CardId::Miracle,
                amount,
                false,
                state,
            ),
        );
        state.queue_action_front(Action::ExhaustCard {
            card_lineage: card.lineage,
            source_pile: crate::state::PileType::Hand,
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawHistoryMode {
    Untouched,
    Track { clear_history: bool },
}

pub fn handle_draw_cards(amount: u32, state: &mut CombatState) {
    handle_draw_cards_inner(amount, DrawHistoryMode::Untouched, state);
}

pub fn handle_draw_for_unique_orb_types(amount_per_orb_type: u32, state: &mut CombatState) {
    if amount_per_orb_type == 0 {
        return;
    }

    let mut seen = Vec::new();
    for orb in &state.entities.player.orbs {
        if orb.id != crate::runtime::combat::OrbId::Empty && !seen.contains(&orb.id) {
            seen.push(orb.id);
        }
    }

    let to_draw = seen.len() as u32 * amount_per_orb_type;
    if to_draw > 0 {
        state.queue_action_front(Action::DrawCards(to_draw));
    }
}

pub fn handle_draw_cards_with_history(amount: u32, clear_history: bool, state: &mut CombatState) {
    handle_draw_cards_inner(amount, DrawHistoryMode::Track { clear_history }, state);
}

fn queue_split_draw_action_front(amount: u32, mode: DrawHistoryMode, state: &mut CombatState) {
    match mode {
        DrawHistoryMode::Untouched => state.queue_action_front(Action::DrawCards(amount)),
        DrawHistoryMode::Track { .. } => state.queue_action_front(Action::DrawCardsWithHistory {
            amount,
            clear_history: false,
        }),
    }
}

fn handle_draw_cards_inner(amount: u32, mode: DrawHistoryMode, state: &mut CombatState) {
    if matches!(
        mode,
        DrawHistoryMode::Track {
            clear_history: true
        }
    ) {
        state.runtime.last_drawn_cards.clear();
    }

    let has_no_draw = store::has_power(state, 0, PowerId::NoDraw);
    if has_no_draw {
        return;
    }

    if amount == 0 {
        return;
    }

    let deck_size = state.zones.draw_pile.len();
    let discard_size = state.zones.discard_pile.len();
    if deck_size + discard_size == 0 {
        return;
    }

    let hand_size = state.zones.hand.len();
    if hand_size >= 10 {
        return;
    }

    let amount = amount.min((10 - hand_size) as u32);
    if amount == 0 {
        return;
    }

    if amount as usize > deck_size {
        let remaining = amount - deck_size as u32;
        queue_split_draw_action_front(remaining, mode, state);
        state.queue_action_front(Action::EmptyDeckShuffle);
        if deck_size != 0 {
            queue_split_draw_action_front(deck_size as u32, mode, state);
        }
        return;
    }

    for _ in 0..amount {
        if state.zones.draw_pile.is_empty() {
            return;
        }
        let mut card = state
            .draw_top_card()
            .expect("draw pile was checked non-empty before drawing");

        apply_card_trigger_when_drawn(&mut card, state);

        if card.id == CardId::Void {
            let void_actions = crate::content::cards::status::void::on_drawn(state);
            state.queue_actions(void_actions);
        }

        if matches!(mode, DrawHistoryMode::Track { .. }) {
            state
                .runtime
                .last_drawn_cards
                .push(crate::runtime::combat::DrawnCardRecord {
                    card_lineage: card.lineage,
                    card_id: card.id,
                });
        }
        state.emit_event(DomainEvent::CardDrawn {
            card: DomainCardSnapshot {
                id: card.id,
                upgrades: card.upgrades,
                lineage: card.lineage,
            },
            source: DomainEventSource::CombatDraw,
        });

        // Apply pre-draw powers (like Corruption, Confusion)
        for power in &store::powers_snapshot_for(state, 0) {
            crate::content::powers::resolve_power_on_card_draw(power.power_type, state, &mut card);
        }

        state.add_card_to_hand(card.clone());

        // Post-draw actions for powers and specific card hooks
        for power in &store::powers_snapshot_for(state, 0) {
            let actions = crate::content::powers::resolve_power_on_card_drawn(
                power.power_type,
                state,
                0,
                power.amount,
                card.lineage,
            );
            for a in actions {
                state.queue_action_back(a);
            }
        }
        crate::content::cards::ironclad::corruption::corruption_on_draw_or_discard(state);
        // DrawCardAction refreshes the layout after each actual draw, even
        // when further actions or a selection still precede the idle boundary.
        crate::content::powers::core::surrounded::sync_back_attack_markers(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::combat::CombatCard;

    #[test]
    fn ordinary_draw_emits_exact_draw_event() {
        let mut state = crate::test_support::blank_test_combat();
        state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::DefendB, 377)]).into();

        crate::engine::action_handlers::execute_action(Action::DrawCards(1), &mut state);

        assert!(state.runtime.emitted_events.iter().any(|event| {
            matches!(
                event,
                DomainEvent::CardDrawn {
                    card: DomainCardSnapshot {
                        id: CardId::DefendB,
                        upgrades: 0,
                        lineage,
                    },
                    source: DomainEventSource::CombatDraw,
                } if *lineage == crate::test_support::scenario_lineage(377)
            )
        }));
    }

    #[test]
    fn draw_continues_after_shuffling_a_nonempty_discard_pile() {
        let mut state = crate::test_support::blank_test_combat();
        state.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
            CombatCard::for_scenario(CardId::Bash, 3),
        ])
        .into();
        state.zones.discard_pile = (vec![
            CombatCard::for_scenario(CardId::Anger, 4),
            CombatCard::for_scenario(CardId::ShrugItOff, 5),
        ])
        .into();

        crate::engine::action_handlers::execute_action(Action::DrawCards(5), &mut state);
        while let Some(action) = state.pop_next_action() {
            crate::engine::action_handlers::execute_action(action, &mut state);
        }

        assert_eq!(state.zones.hand.len(), 5);
        assert!(state.zones.draw_pile.is_empty());
        assert!(state.zones.discard_pile.is_empty());
    }

    #[test]
    fn oversized_draw_preserves_java_empty_second_shuffle_rng_consumption() {
        let mut state = crate::test_support::blank_test_combat();
        state.zones.discard_pile = (vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ])
        .into();
        let shuffle_counter_before = state.rng.shuffle_rng.counter;

        crate::engine::action_handlers::execute_action(Action::DrawCards(5), &mut state);
        while let Some(action) = state.pop_next_action() {
            crate::engine::action_handlers::execute_action(action, &mut state);
        }

        assert_eq!(state.zones.hand.len(), 2);
        assert_eq!(state.rng.shuffle_rng.counter, shuffle_counter_before + 2);
    }
}
