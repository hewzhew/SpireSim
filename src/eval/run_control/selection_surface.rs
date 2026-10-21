use crate::content::cards::CardId;
use crate::runtime::combat::{CardLineageId, CardPileView, CombatState};
use crate::state::core::{ClientInput, EngineState, PendingChoice, PileType};
use crate::state::selection::{
    SelectionReason, SelectionResolution, SelectionScope, SelectionTargetRef,
};

use super::session::RunControlSession;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SelectionSurface {
    pub scope: SelectionScope,
    pub reason: SelectionReason,
    pub min_choices: usize,
    pub max_choices: usize,
    pub can_cancel: bool,
    pub item_count: usize,
    pub items: Vec<SelectionSurfaceItem>,
    pub submit_hint: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SelectionSurfaceItem {
    pub visible_index: usize,
    pub location: SelectionItemLocation,
    pub target: SelectionTargetRef,
    pub card: CardId,
    pub upgrades: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectionItemLocation {
    Hand { index: usize },
    Grid { pile: PileType, index: usize },
    Scry { index: usize },
    MasterDeck { index: usize },
}

pub(super) fn active_selection_surface(session: &RunControlSession) -> Option<SelectionSurface> {
    match &session.engine_state {
        EngineState::PendingChoice(choice) => match choice {
            PendingChoice::HandSelect {
                candidate_lineages,
                reason,
                min_cards,
                max_cards,
                can_cancel,
            } => Some(SelectionSurface {
                scope: SelectionScope::Hand,
                reason: (*reason).into(),
                min_choices: *min_cards as usize,
                max_choices: (*max_cards as usize).min(candidate_lineages.len()),
                can_cancel: *can_cancel,
                item_count: candidate_lineages.len(),
                items: selection_items_for_combat_cards(
                    combat_hand_cards(session),
                    candidate_lineages,
                    CombatSelectionLocation::Hand,
                ),
                submit_hint: "select <idx...>",
            }),
            PendingChoice::GridSelect {
                source_pile,
                candidate_lineages,
                reason,
                min_cards,
                max_cards,
                can_cancel,
            } => Some(SelectionSurface {
                scope: SelectionScope::Grid,
                reason: (*reason).into(),
                min_choices: *min_cards as usize,
                max_choices: (*max_cards as usize).min(candidate_lineages.len()),
                can_cancel: *can_cancel,
                item_count: candidate_lineages.len(),
                items: selection_items_for_combat_cards(
                    combat_cards_for_pile(session, *source_pile),
                    candidate_lineages,
                    CombatSelectionLocation::Grid(*source_pile),
                ),
                submit_hint: "select <idx...>",
            }),
            PendingChoice::ScrySelect {
                cards,
                card_lineages,
            } => Some(SelectionSurface {
                scope: SelectionScope::Grid,
                reason: SelectionReason::PutToBottomOfDraw,
                min_choices: 0,
                max_choices: cards.len(),
                can_cancel: false,
                item_count: cards.len(),
                items: cards
                    .iter()
                    .zip(card_lineages.iter())
                    .enumerate()
                    .map(|(index, (card, lineage))| SelectionSurfaceItem {
                        visible_index: index,
                        location: SelectionItemLocation::Scry { index },
                        target: SelectionTargetRef::CardLineage(*lineage),
                        card: *card,
                        upgrades: 0,
                    })
                    .collect(),
                submit_hint: "select <idx...>",
            }),
            _ => None,
        },
        EngineState::RunPendingChoice(choice) => {
            let request = choice.selection_request(&session.run_state);
            Some(SelectionSurface {
                scope: request.scope,
                reason: request.reason,
                min_choices: choice.min_choices.saturating_sub(choice.selected.len()),
                max_choices: choice.max_choices.saturating_sub(choice.selected.len()),
                can_cancel: request.can_cancel,
                item_count: request.targets.len(),
                items: selection_items_for_master_deck_targets(session, &request.targets),
                submit_hint: "select <idx...>",
            })
        }
        _ => None,
    }
}

fn combat_hand_cards(session: &RunControlSession) -> CardPileView<'_> {
    session
        .active_combat
        .as_ref()
        .map(|active| CardPileView::Contiguous(&active.combat_state.zones.hand))
        .unwrap_or(CardPileView::Contiguous(&[]))
}

fn combat_cards_for_pile(session: &RunControlSession, pile: PileType) -> CardPileView<'_> {
    let Some(combat) = session
        .active_combat
        .as_ref()
        .map(|active| &active.combat_state)
    else {
        return CardPileView::Contiguous(&[]);
    };
    grid_source_cards(combat, pile)
}

#[derive(Clone, Copy)]
enum CombatSelectionLocation {
    Hand,
    Grid(PileType),
}

fn selection_items_for_combat_cards(
    cards: CardPileView<'_>,
    candidate_lineages: &[CardLineageId],
    source: CombatSelectionLocation,
) -> Vec<SelectionSurfaceItem> {
    candidate_lineages
        .iter()
        .enumerate()
        .filter_map(|(visible_index, lineage)| {
            let card_index = cards.iter().position(|card| card.lineage == *lineage)?;
            let card = &cards[card_index];
            let location = match source {
                CombatSelectionLocation::Hand => SelectionItemLocation::Hand { index: card_index },
                CombatSelectionLocation::Grid(pile) => SelectionItemLocation::Grid {
                    pile,
                    index: card_index,
                },
            };
            Some(SelectionSurfaceItem {
                visible_index,
                location,
                target: SelectionTargetRef::CardLineage(*lineage),
                card: card.id,
                upgrades: card.upgrades,
            })
        })
        .collect()
}

fn selection_items_for_master_deck_targets(
    session: &RunControlSession,
    targets: &[SelectionTargetRef],
) -> Vec<SelectionSurfaceItem> {
    targets
        .iter()
        .enumerate()
        .filter_map(|(visible_index, target)| match target {
            SelectionTargetRef::CardLineage(lineage) => {
                let deck_index = session
                    .run_state
                    .master_deck
                    .iter()
                    .position(|card| card.lineage == *lineage)?;
                let card = &session.run_state.master_deck[deck_index];
                Some(SelectionSurfaceItem {
                    visible_index,
                    location: SelectionItemLocation::MasterDeck { index: deck_index },
                    target: *target,
                    card: card.id,
                    upgrades: card.upgrades,
                })
            }
        })
        .collect()
}

pub(super) fn current_selection_input_is_allowed(
    session: &RunControlSession,
    input: &ClientInput,
) -> Option<bool> {
    match &session.engine_state {
        EngineState::PendingChoice(choice) => {
            Some(pending_choice_input_is_allowed(session, choice, input))
        }
        EngineState::RunPendingChoice(choice) => match input {
            ClientInput::SelectCard(index) => Some(
                choice.min_choices == choice.max_choices
                    && choice.selected.len() < choice.max_choices
                    && session
                        .run_state
                        .master_deck
                        .get(*index)
                        .is_some_and(|card| {
                            choice
                                .selection_request(&session.run_state)
                                .targets
                                .contains(&SelectionTargetRef::CardLineage(card.lineage))
                        }),
            ),
            ClientInput::SubmitSelection(resolution) => Some(run_pending_resolution_is_allowed(
                session, choice, resolution,
            )),
            ClientInput::Cancel => Some(choice.min_choices == 0),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn pending_choice_input_is_allowed(
    session: &RunControlSession,
    choice: &PendingChoice,
    input: &ClientInput,
) -> bool {
    let Some(combat) = session
        .active_combat
        .as_ref()
        .map(|active| &active.combat_state)
    else {
        return false;
    };
    crate::sim::combat_action_surface::pending_choice_input_is_legal(choice, combat, input)
}

fn run_pending_resolution_is_allowed(
    session: &RunControlSession,
    choice: &crate::state::core::RunPendingChoiceState,
    resolution: &SelectionResolution,
) -> bool {
    if resolution.scope != SelectionScope::Deck {
        return false;
    }
    let indices = resolution
        .selected
        .iter()
        .filter_map(|target| match target {
            SelectionTargetRef::CardLineage(lineage) => session
                .run_state
                .master_deck
                .iter()
                .position(|card| card.lineage == *lineage),
        })
        .collect::<Vec<_>>();
    indices.len() == resolution.selected.len()
        && session.run_pending_selection_is_allowed(choice, &indices)
}

fn grid_source_cards(combat: &CombatState, source_pile: PileType) -> CardPileView<'_> {
    match source_pile {
        PileType::Draw => CardPileView::Contiguous(&combat.zones.draw_pile),
        PileType::Discard => CardPileView::Discard(&combat.zones.discard_pile),
        PileType::Exhaust => CardPileView::Contiguous(&combat.zones.exhaust_pile),
        PileType::Hand => CardPileView::Contiguous(&combat.zones.hand),
        PileType::Limbo => CardPileView::Contiguous(&combat.zones.limbo),
        PileType::MasterDeck => CardPileView::Contiguous(&combat.meta.master_deck_snapshot),
    }
}
