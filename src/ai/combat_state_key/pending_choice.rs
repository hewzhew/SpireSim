use crate::content::cards::CardId;
use crate::runtime::action::CardDestination;
use crate::runtime::combat::{CardLineageId, CardPileView, CombatState};
use crate::state::core::{
    ChooseOneCardChoice, GridSelectReason, HandSelectReason, PendingChoice, PileType,
};

use super::stable::stable_card_signature;
use super::types::{StableChoiceCandidateKey, StablePendingChoiceKey};

pub(super) fn pending_choice_key(
    choice: &PendingChoice,
    combat: &CombatState,
) -> StablePendingChoiceKey {
    match choice {
        PendingChoice::GridSelect {
            source_pile,
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        } => StablePendingChoiceKey::GridSelect {
            source_pile: pile_label(*source_pile),
            min_cards: *min_cards,
            max_cards: *max_cards,
            can_cancel: *can_cancel,
            reason: grid_select_reason_key(*reason),
            candidates: candidate_cards_key(
                combat,
                candidate_lineages,
                pile_cards(combat, *source_pile),
                matches!(
                    source_pile,
                    PileType::Draw | PileType::Discard | PileType::Exhaust
                ),
                matches!(
                    source_pile,
                    PileType::MasterDeck | PileType::Draw | PileType::Discard | PileType::Exhaust
                ),
                if matches!(source_pile, PileType::MasterDeck) {
                    "master_ref"
                } else {
                    "unknown_ref"
                },
            ),
        },
        PendingChoice::HandSelect {
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        } => StablePendingChoiceKey::HandSelect {
            min_cards: *min_cards,
            max_cards: *max_cards,
            can_cancel: *can_cancel,
            reason: hand_select_reason_key(*reason),
            candidates: candidate_cards_key(
                combat,
                candidate_lineages,
                Some(CardPileView::Contiguous(&combat.zones.hand)),
                false,
                true,
                "unknown_ref",
            ),
        },
        PendingChoice::DiscoverySelect(choice) => {
            StablePendingChoiceKey::Discovery(sorted_card_ids(&choice.cards))
        }
        PendingChoice::ScrySelect { card_lineages, .. } => {
            StablePendingChoiceKey::Scry(scry_candidates_key(combat, card_lineages))
        }
        PendingChoice::CardRewardSelect {
            cards,
            destination,
            can_skip,
        } => StablePendingChoiceKey::CardRewardSelect {
            destination: card_destination_key(*destination).to_string(),
            can_skip: *can_skip,
            cards: sorted_card_ids(cards),
        },
        PendingChoice::ForeignInfluenceSelect { cards, upgraded } => {
            StablePendingChoiceKey::ForeignInfluence {
                upgraded: *upgraded,
                cards: sorted_card_ids(cards),
            }
        }
        PendingChoice::ChooseOneSelect { choices } => {
            StablePendingChoiceKey::ChooseOne(sorted_choose_one_choices(choices))
        }
        PendingChoice::StanceChoice => StablePendingChoiceKey::StanceChoice,
    }
}

fn pile_cards(combat: &CombatState, pile: PileType) -> Option<CardPileView<'_>> {
    match pile {
        PileType::Draw => Some(CardPileView::Contiguous(combat.zones.draw_pile.as_ref())),
        PileType::Discard => Some(CardPileView::Discard(&combat.zones.discard_pile)),
        PileType::Exhaust => Some(CardPileView::Contiguous(
            combat.zones.exhaust_pile.as_slice(),
        )),
        PileType::Hand => Some(CardPileView::Contiguous(&combat.zones.hand)),
        PileType::Limbo => Some(CardPileView::Contiguous(&combat.zones.limbo)),
        PileType::MasterDeck => None,
    }
}

fn candidate_cards_key(
    combat: &CombatState,
    candidate_lineages: &[CardLineageId],
    cards: Option<CardPileView<'_>>,
    allow_visible_fallback: bool,
    normalize_order: bool,
    unresolved_prefix: &'static str,
) -> Vec<StableChoiceCandidateKey> {
    let mut candidates = candidate_lineages
        .iter()
        .map(|lineage| {
            stable_candidate_card_key(combat, cards, *lineage, allow_visible_fallback).unwrap_or(
                StableChoiceCandidateKey::Ref {
                    prefix: unresolved_prefix,
                    lineage: *lineage,
                },
            )
        })
        .collect::<Vec<_>>();
    if normalize_order {
        candidates.sort();
    }
    candidates
}

fn stable_candidate_card_key(
    combat: &CombatState,
    cards: Option<CardPileView<'_>>,
    lineage: CardLineageId,
    allow_visible_fallback: bool,
) -> Option<StableChoiceCandidateKey> {
    cards
        .and_then(|cards| cards.iter().find(|card| card.lineage == lineage))
        .or_else(|| {
            allow_visible_fallback.then(|| {
                visible_card_zones(combat)
                    .iter()
                    .find_map(|zone| zone.iter().find(|card| card.lineage == lineage))
            })?
        })
        .map(|card| StableChoiceCandidateKey::Card(stable_card_signature(card)))
}

fn visible_card_zones(combat: &CombatState) -> [CardPileView<'_>; 5] {
    [
        CardPileView::Contiguous(&combat.zones.hand),
        CardPileView::Contiguous(combat.zones.draw_pile.as_ref()),
        CardPileView::Discard(&combat.zones.discard_pile),
        CardPileView::Contiguous(combat.zones.exhaust_pile.as_slice()),
        CardPileView::Contiguous(&combat.zones.limbo),
    ]
}

fn scry_candidates_key(
    combat: &CombatState,
    card_lineages: &[CardLineageId],
) -> Vec<StableChoiceCandidateKey> {
    card_lineages
        .iter()
        .map(|lineage| {
            stable_candidate_card_key(
                combat,
                Some(CardPileView::Contiguous(combat.zones.draw_pile.as_ref())),
                *lineage,
                false,
            )
            .unwrap_or(StableChoiceCandidateKey::Ref {
                prefix: "scry_ref",
                lineage: *lineage,
            })
        })
        .collect()
}

fn pile_label(pile: PileType) -> &'static str {
    match pile {
        PileType::Draw => "Draw",
        PileType::Discard => "Discard",
        PileType::Exhaust => "Exhaust",
        PileType::Hand => "Hand",
        PileType::Limbo => "Limbo",
        PileType::MasterDeck => "MasterDeck",
    }
}

fn grid_select_reason_key(reason: GridSelectReason) -> String {
    match reason {
        GridSelectReason::MoveToDrawPile => "move_to_draw_pile".to_string(),
        GridSelectReason::Exhume { upgrade } => format!("exhume:upgrade={upgrade}"),
        GridSelectReason::DrawPileToHand => "draw_pile_to_hand".to_string(),
        GridSelectReason::SkillFromDeckToHand => "skill_from_deck_to_hand".to_string(),
        GridSelectReason::AttackFromDeckToHand => "attack_from_deck_to_hand".to_string(),
        GridSelectReason::DiscardToHand => "discard_to_hand".to_string(),
        GridSelectReason::DiscardToHandNoCostChange => "discard_to_hand_no_cost_change".to_string(),
        GridSelectReason::DiscardToHandRetain => "discard_to_hand_retain".to_string(),
        GridSelectReason::Omniscience { play_amount } => {
            format!("omniscience:play_amount={play_amount}")
        }
    }
}

fn hand_select_reason_key(reason: HandSelectReason) -> String {
    match reason {
        HandSelectReason::Exhaust => "exhaust".to_string(),
        HandSelectReason::PotionExhaust {
            has_pending_use_effect,
        } => format!("potion_exhaust:pending_use_effect={has_pending_use_effect}"),
        HandSelectReason::Discard => "discard".to_string(),
        HandSelectReason::Retain => "retain".to_string(),
        HandSelectReason::PutOnDrawPile => "put_on_draw_pile".to_string(),
        HandSelectReason::PutToBottomOfDraw => "put_to_bottom_of_draw".to_string(),
        HandSelectReason::Setup => "setup".to_string(),
        HandSelectReason::Copy { amount } => format!("copy:amount={amount}"),
        HandSelectReason::Nightmare { amount } => format!("nightmare:amount={amount}"),
        HandSelectReason::Upgrade => "upgrade".to_string(),
        HandSelectReason::GamblingChip => "gambling_chip".to_string(),
        HandSelectReason::Recycle => "recycle".to_string(),
    }
}

fn card_destination_key(destination: CardDestination) -> &'static str {
    match destination {
        CardDestination::Hand => "hand",
        CardDestination::DrawPileRandom => "draw_pile_random",
    }
}

fn sorted_card_ids(values: &[CardId]) -> Vec<String> {
    let mut values = values
        .iter()
        .map(|value| value.stable_id().to_owned())
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn sorted_choose_one_choices(values: &[ChooseOneCardChoice]) -> Vec<String> {
    let mut values = values
        .iter()
        .map(|value| format!("{}:u{}", value.card_id.stable_id(), value.upgrades))
        .collect::<Vec<_>>();
    values.sort();
    values
}
