use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

fn move_random_hand_card_to_draw_top(state: &mut CombatState) {
    if state.zones.hand.is_empty() {
        return;
    }
    let idx = state
        .rng
        .card_random_rng
        .random(state.zones.hand.len() as i32 - 1) as usize;
    let card = state.zones.hand.remove(idx);
    state.add_card_to_draw_pile_top(card);
}

pub fn handle_put_on_deck(amount: usize, random: bool, state: &mut CombatState) {
    let amount = amount.min(state.zones.hand.len());
    if amount == 0 {
        return;
    }

    if random {
        for _ in 0..amount {
            move_random_hand_card_to_draw_top(state);
        }
        return;
    }

    if state.zones.hand.len() > amount {
        state.queue_action_front(Action::SuspendForHandSelect {
            min: amount as u8,
            max: amount as u8,
            can_cancel: false,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::PutOnDrawPile,
        });
        return;
    }

    let mut i = 0;
    while i < state.zones.hand.len() {
        move_random_hand_card_to_draw_top(state);
        i += 1;
    }
}

pub fn handle_forethought(upgraded: bool, state: &mut CombatState) {
    if state.zones.hand.is_empty() {
        return;
    }

    if !upgraded && state.zones.hand.len() == 1 {
        let mut card = state
            .zones
            .hand
            .pop()
            .expect("checked non-empty hand before Forethought auto move");
        if card.combat_cost_without_turn_override_java() > 0 {
            card.free_to_play_once = true;
        }
        state.add_card_to_draw_pile_bottom(card);
        return;
    }

    state.queue_action_front(Action::SuspendForHandSelect {
        min: if upgraded { 0 } else { 1 },
        max: if upgraded { 99 } else { 1 },
        can_cancel: upgraded,
        filter: crate::state::HandSelectFilter::Any,
        reason: crate::state::HandSelectReason::PutToBottomOfDraw,
    });
}

pub fn handle_empty_deck_shuffle(state: &mut CombatState) {
    // The caller decides whether to construct a shuffle. Once constructed,
    // Java update always shuffles discard, regardless of current pile sizes.
    state.shuffle_discard_pile_into_draw_pile();
}

pub fn handle_shuffle_all_into_draw(state: &mut CombatState) {
    state.queue_action_front(Action::PutOnDeck {
        amount: 99,
        random: true,
    });

    state.shuffle_discard_pile_into_draw_pile();
}

pub fn handle_shuffle_draw_pile(trigger_relics: bool, state: &mut CombatState) {
    if trigger_relics {
        let shuffle_actions = crate::content::relics::hooks::on_shuffle(state);
        state.queue_actions(shuffle_actions);
    }
    state.zones.draw_pile.reverse();
    crate::runtime::rng::shuffle_with_random_long(
        state.zones.draw_pile.as_mut_slice(),
        &mut state.rng.shuffle_rng,
    );
    state.zones.draw_pile.reverse();
}
