use super::*;
#[cfg(test)]
use crate::content::cards::CardId;
use crate::runtime::combat::{CardLineageId, CardPileView, CombatCard};
#[cfg(test)]
use crate::state::core::{GridSelectReason, HandSelectReason};
use crate::state::core::{PendingChoice, PileType};
use crate::state::selection::SelectionScope;

mod card_selection;
mod selection_hints;
mod types;

use selection_hints::{
    removal_selection_hint_from_card_ids, selection_hint_for_grid_reason,
    selection_hint_for_hand_reason, value_selection_hint_from_card_id,
};
pub(super) use types::{PendingChoiceOrderingHint, PendingChoiceOrderingRole};

pub(super) fn pending_choice_ordering_hint(
    engine: &EngineState,
    combat: &CombatState,
    input: &ClientInput,
) -> Option<PendingChoiceOrderingHint> {
    let EngineState::PendingChoice(choice) = engine else {
        return None;
    };

    match (choice, input) {
        (_, ClientInput::Cancel) => Some(PendingChoiceOrderingHint {
            role: PendingChoiceOrderingRole::Cancel,
            primary: 0,
            secondary: 0,
            selected_count_tiebreak: 0,
            policy_log2_bias: 0,
        }),
        (
            PendingChoice::HandSelect {
                candidate_lineages,
                reason,
                ..
            },
            ClientInput::SubmitSelection(resolution),
        ) if resolution.scope == SelectionScope::Hand
            && selection_is_subset(&resolution.selected_card_lineages(), candidate_lineages) =>
        {
            let lineages = resolution.selected_card_lineages();
            let cards = lineages
                .iter()
                .filter_map(|lineage| {
                    find_card_by_lineage(CardPileView::Contiguous(&combat.zones.hand), *lineage)
                })
                .collect::<Vec<_>>();
            Some(selection_hint_for_hand_reason(
                combat,
                *reason,
                &cards,
                lineages.len(),
            ))
        }
        (
            PendingChoice::GridSelect {
                source_pile,
                candidate_lineages,
                reason,
                ..
            },
            ClientInput::SubmitSelection(resolution),
        ) if resolution.scope == SelectionScope::Grid
            && selection_is_subset(&resolution.selected_card_lineages(), candidate_lineages) =>
        {
            let lineages = resolution.selected_card_lineages();
            let cards = lineages
                .iter()
                .filter_map(|lineage| {
                    find_card_by_lineage(pile_cards(combat, *source_pile), *lineage)
                })
                .collect::<Vec<_>>();
            Some(selection_hint_for_grid_reason(
                *reason,
                &cards,
                lineages.len(),
            ))
        }
        (PendingChoice::ScrySelect { cards, .. }, ClientInput::SubmitScryDiscard(indices)) => {
            let selected_cards = indices
                .iter()
                .filter_map(|idx| cards.get(*idx).copied())
                .collect::<Vec<_>>();
            Some(removal_selection_hint_from_card_ids(
                &selected_cards,
                indices.len(),
            ))
        }
        (PendingChoice::DiscoverySelect(choice), ClientInput::SubmitDiscoverChoice(idx))
            if *idx < choice.cards.len() =>
        {
            Some(value_selection_hint_from_card_id(choice.cards[*idx], 1))
        }
        (PendingChoice::CardRewardSelect { cards, .. }, ClientInput::SubmitDiscoverChoice(idx))
            if *idx < cards.len() =>
        {
            Some(value_selection_hint_from_card_id(cards[*idx], 1))
        }
        (
            PendingChoice::ForeignInfluenceSelect { cards, .. },
            ClientInput::SubmitDiscoverChoice(idx),
        ) if *idx < cards.len() => Some(value_selection_hint_from_card_id(cards[*idx], 1)),
        (PendingChoice::ChooseOneSelect { choices }, ClientInput::SubmitDiscoverChoice(idx))
            if *idx < choices.len() =>
        {
            Some(value_selection_hint_from_card_id(choices[*idx].card_id, 1))
        }
        (PendingChoice::StanceChoice, ClientInput::SubmitDiscoverChoice(idx)) if *idx <= 1 => {
            Some(PendingChoiceOrderingHint {
                role: PendingChoiceOrderingRole::NeutralSelection,
                primary: -(*idx as i32),
                secondary: 0,
                selected_count_tiebreak: -1,
                policy_log2_bias: 0,
            })
        }
        _ => None,
    }
}

pub(super) fn pending_choice_prepared_draw_card_lineage(
    engine: &EngineState,
    input: &ClientInput,
) -> Option<CardLineageId> {
    let EngineState::PendingChoice(choice) = engine else {
        return None;
    };
    let ClientInput::SubmitSelection(resolution) = input else {
        return None;
    };
    let selected = resolution.selected_card_lineages();
    let [selected_uuid] = selected.as_slice() else {
        return None;
    };
    let prepares_draw_top = match choice {
        PendingChoice::HandSelect {
            candidate_lineages,
            reason,
            ..
        } => {
            resolution.scope == SelectionScope::Hand
                && candidate_lineages.contains(selected_uuid)
                && matches!(
                    reason,
                    crate::state::core::HandSelectReason::PutOnDrawPile
                        | crate::state::core::HandSelectReason::Setup
                )
        }
        PendingChoice::GridSelect {
            candidate_lineages,
            reason,
            ..
        } => {
            resolution.scope == SelectionScope::Grid
                && candidate_lineages.contains(selected_uuid)
                && matches!(reason, crate::state::core::GridSelectReason::MoveToDrawPile)
        }
        _ => false,
    };
    prepares_draw_top.then_some(*selected_uuid)
}

fn selection_is_subset(selected: &[CardLineageId], candidates: &[CardLineageId]) -> bool {
    selected.iter().all(|lineage| candidates.contains(lineage))
}

fn find_card_by_lineage(cards: CardPileView<'_>, lineage: CardLineageId) -> Option<&CombatCard> {
    cards.iter().find(|card| card.lineage == lineage)
}

fn pile_cards(combat: &CombatState, pile: PileType) -> CardPileView<'_> {
    match pile {
        PileType::Draw => CardPileView::Contiguous(combat.zones.draw_pile.as_ref()),
        PileType::Discard => CardPileView::Discard(&combat.zones.discard_pile),
        PileType::Exhaust => CardPileView::Contiguous(combat.zones.exhaust_pile.as_slice()),
        PileType::Hand => CardPileView::Contiguous(&combat.zones.hand),
        PileType::Limbo => CardPileView::Contiguous(&combat.zones.limbo),
        PileType::MasterDeck => CardPileView::Contiguous(&[]),
    }
}

#[cfg(test)]
mod tests;
