use crate::runtime::action::ActionInfo;
use crate::runtime::combat::{CombatCard, CombatState};

fn apply_card_at_turn_start(card: &mut CombatCard) {
    if card.id == crate::content::cards::CardId::Eviscerate {
        card.cost_for_turn = None;
    }
}

pub fn at_turn_start_in_hand(state: &mut CombatState) -> smallvec::SmallVec<[ActionInfo; 4]> {
    // Java: Player.applyStartOfTurnCards()
    // Despite the historical Rust name, Java applies this to draw pile, hand,
    // then discard pile. Eviscerate resets its temporary discard reduction here.
    state
        .zones
        .draw_pile
        .mutate_cards_with_kind_tracking(apply_card_at_turn_start);
    for card in state
        .zones
        .hand
        .iter_mut()
        .chain(state.zones.discard_pile.iter_mut())
    {
        apply_card_at_turn_start(card);
    }
    smallvec::smallvec![]
}

pub fn on_scry(state: &CombatState) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();
    for card in &state.zones.discard_pile {
        if card.id == crate::content::cards::CardId::Weave {
            actions.push(ActionInfo {
                action: crate::runtime::action::Action::DiscardToHand {
                    card_lineage: card.lineage,
                    cost_for_turn: None,
                },
                insertion_mode: crate::runtime::action::AddTo::Bottom,
            });
        }
    }
    actions
}

pub fn on_change_stance_from_discard(state: &CombatState) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();
    for card in &state.zones.discard_pile {
        if card.id == crate::content::cards::CardId::FlurryOfBlows {
            actions.push(ActionInfo {
                action: crate::runtime::action::Action::DiscardToHand {
                    card_lineage: card.lineage,
                    cost_for_turn: None,
                },
                insertion_mode: crate::runtime::action::AddTo::Bottom,
            });
        }
    }
    actions
}
