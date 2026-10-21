use crate::content::cards::CardType;
use crate::runtime::action::CardDestination;
use crate::state::core::{GridSelectReason, HandSelectReason, PendingChoice, PileType};

use super::super::types::{
    CombatCardDestinationKey, CombatCardTypeKey, CombatChooseOneCardKey, CombatGridSelectReasonKey,
    CombatHandSelectReasonKey, CombatPendingChoiceKey, CombatPileTypeKey,
};

pub(super) fn pending_choice_key(choice: &PendingChoice) -> CombatPendingChoiceKey {
    match choice {
        PendingChoice::GridSelect {
            source_pile,
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        } => CombatPendingChoiceKey::GridSelect {
            source_pile: pile_type_key(*source_pile),
            candidate_lineages: candidate_lineages.clone(),
            min_cards: *min_cards,
            max_cards: *max_cards,
            can_cancel: *can_cancel,
            reason: grid_select_reason_key(*reason),
        },
        PendingChoice::HandSelect {
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        } => CombatPendingChoiceKey::HandSelect {
            candidate_lineages: candidate_lineages.clone(),
            min_cards: *min_cards,
            max_cards: *max_cards,
            can_cancel: *can_cancel,
            reason: hand_select_reason_key(*reason),
        },
        PendingChoice::DiscoverySelect(state) => CombatPendingChoiceKey::DiscoverySelect {
            cards: state.cards.clone(),
            colorless: state.colorless,
            card_type: state.card_type.map(card_type_key),
            amount: state.amount,
            can_skip: state.can_skip,
        },
        PendingChoice::ScrySelect {
            cards,
            card_lineages,
        } => CombatPendingChoiceKey::ScrySelect {
            cards: cards.clone(),
            card_lineages: card_lineages.clone(),
        },
        PendingChoice::CardRewardSelect {
            cards,
            destination,
            can_skip,
        } => CombatPendingChoiceKey::CardRewardSelect {
            cards: cards.clone(),
            destination: card_destination_key(*destination),
            can_skip: *can_skip,
        },
        PendingChoice::ForeignInfluenceSelect { cards, upgraded } => {
            CombatPendingChoiceKey::ForeignInfluenceSelect {
                cards: cards.clone(),
                upgraded: *upgraded,
            }
        }
        PendingChoice::ChooseOneSelect { choices } => CombatPendingChoiceKey::ChooseOneSelect {
            choices: choices
                .iter()
                .map(|choice| CombatChooseOneCardKey {
                    card_id: choice.card_id,
                    upgrades: choice.upgrades,
                })
                .collect(),
        },
        PendingChoice::StanceChoice => CombatPendingChoiceKey::StanceChoice,
    }
}

fn pile_type_key(value: PileType) -> CombatPileTypeKey {
    match value {
        PileType::Draw => CombatPileTypeKey::Draw,
        PileType::Discard => CombatPileTypeKey::Discard,
        PileType::Exhaust => CombatPileTypeKey::Exhaust,
        PileType::Hand => CombatPileTypeKey::Hand,
        PileType::Limbo => CombatPileTypeKey::Limbo,
        PileType::MasterDeck => CombatPileTypeKey::MasterDeck,
    }
}

fn hand_select_reason_key(value: HandSelectReason) -> CombatHandSelectReasonKey {
    match value {
        HandSelectReason::Exhaust => CombatHandSelectReasonKey::Exhaust,
        HandSelectReason::PotionExhaust {
            has_pending_use_effect,
        } => CombatHandSelectReasonKey::PotionExhaust {
            has_pending_use_effect,
        },
        HandSelectReason::Discard => CombatHandSelectReasonKey::Discard,
        HandSelectReason::Retain => CombatHandSelectReasonKey::Retain,
        HandSelectReason::PutOnDrawPile => CombatHandSelectReasonKey::PutOnDrawPile,
        HandSelectReason::PutToBottomOfDraw => CombatHandSelectReasonKey::PutToBottomOfDraw,
        HandSelectReason::Setup => CombatHandSelectReasonKey::Setup,
        HandSelectReason::Copy { amount } => CombatHandSelectReasonKey::Copy { amount },
        HandSelectReason::Nightmare { amount } => CombatHandSelectReasonKey::Nightmare { amount },
        HandSelectReason::Upgrade => CombatHandSelectReasonKey::Upgrade,
        HandSelectReason::GamblingChip => CombatHandSelectReasonKey::GamblingChip,
        HandSelectReason::Recycle => CombatHandSelectReasonKey::Recycle,
    }
}

fn grid_select_reason_key(value: GridSelectReason) -> CombatGridSelectReasonKey {
    match value {
        GridSelectReason::MoveToDrawPile => CombatGridSelectReasonKey::MoveToDrawPile,
        GridSelectReason::Exhume { upgrade } => CombatGridSelectReasonKey::Exhume { upgrade },
        GridSelectReason::DrawPileToHand => CombatGridSelectReasonKey::DrawPileToHand,
        GridSelectReason::SkillFromDeckToHand => CombatGridSelectReasonKey::SkillFromDeckToHand,
        GridSelectReason::AttackFromDeckToHand => CombatGridSelectReasonKey::AttackFromDeckToHand,
        GridSelectReason::DiscardToHand => CombatGridSelectReasonKey::DiscardToHand,
        GridSelectReason::DiscardToHandNoCostChange => {
            CombatGridSelectReasonKey::DiscardToHandNoCostChange
        }
        GridSelectReason::DiscardToHandRetain => CombatGridSelectReasonKey::DiscardToHandRetain,
        GridSelectReason::Omniscience { play_amount } => {
            CombatGridSelectReasonKey::Omniscience { play_amount }
        }
    }
}

fn card_type_key(value: CardType) -> CombatCardTypeKey {
    match value {
        CardType::Attack => CombatCardTypeKey::Attack,
        CardType::Skill => CombatCardTypeKey::Skill,
        CardType::Power => CombatCardTypeKey::Power,
        CardType::Status => CombatCardTypeKey::Status,
        CardType::Curse => CombatCardTypeKey::Curse,
    }
}

fn card_destination_key(value: CardDestination) -> CombatCardDestinationKey {
    match value {
        CardDestination::Hand => CombatCardDestinationKey::Hand,
        CardDestination::DrawPileRandom => CombatCardDestinationKey::DrawPileRandom,
    }
}
