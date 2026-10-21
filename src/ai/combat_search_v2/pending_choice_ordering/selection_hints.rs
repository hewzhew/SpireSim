use crate::ai::card_semantics_v1::card_reward_facts_v1;
use crate::content::cards::{self, CardId};
use crate::runtime::combat::CombatCard;
use crate::runtime::combat::CombatState;
use crate::state::core::{GridSelectReason, HandSelectReason};
use crate::state::rewards::RewardCard;

use super::card_selection::{aggregate_card_facts, CardSelectionFacts};
use super::{PendingChoiceOrderingHint, PendingChoiceOrderingRole};

const RECYCLE_ENERGY_FACTOR: i32 = 10;
const SETUP_EXPENSIVE_CARD_BONUS: i32 = 25;
// A Wound is normally the first card a one-card Exhaust choice should remove.
// Once the Power Through / Second Wind / Dark Embrace engine is connected,
// however, that same Wound is five-or-more Block plus one draw when converted
// by Second Wind.  This penalty deliberately exceeds the ordinary +1000
// undesirable-card removal value so the context-specific engine prior changes
// the ordering by a full tier instead of becoming another small tiebreak.
const CONNECTED_SECOND_WIND_WOUND_PRESERVATION: i32 = 2_000;
const CONNECTED_SECOND_WIND_WOUND_LOG2_BIAS: i32 = -10;
// An Exhaust choice should not casually consume the cards that answer the
// currently visible attack or provide scarce enemy Strength control. Keep
// them a full tier below ordinary fodder without removing the legal choice.
const TACTICAL_CARD_PRESERVATION: i32 = 1_000;
const TACTICAL_CARD_PRESERVATION_LOG2_BIAS: i32 = -6;

pub(super) fn selection_hint_for_hand_reason(
    combat: &CombatState,
    reason: HandSelectReason,
    cards: &[&CombatCard],
    selected_count: usize,
) -> PendingChoiceOrderingHint {
    match reason {
        HandSelectReason::Exhaust | HandSelectReason::PotionExhaust { .. } => {
            exhaust_selection_hint(combat, cards, selected_count)
        }
        HandSelectReason::Discard | HandSelectReason::GamblingChip => {
            removal_selection_hint(cards, selected_count)
        }
        HandSelectReason::Recycle => recycle_selection_hint(cards, selected_count),
        HandSelectReason::Upgrade => upgrade_selection_hint(cards, selected_count),
        HandSelectReason::Copy { amount } | HandSelectReason::Nightmare { amount } => {
            repeated_value_selection_hint(cards, selected_count, amount)
        }
        HandSelectReason::Retain => value_selection_hint(cards, selected_count),
        HandSelectReason::PutOnDrawPile
        | HandSelectReason::PutToBottomOfDraw
        | HandSelectReason::Setup => draw_pile_setup_selection_hint(cards, selected_count),
    }
}

fn exhaust_selection_hint(
    combat: &CombatState,
    cards: &[&CombatCard],
    selected_count: usize,
) -> PendingChoiceOrderingHint {
    let facts = aggregate_card_facts(cards.iter().copied().map(CardSelectionFacts::from_card));
    let preserved_wounds = if sts_combat_knowledge::connected_second_wind_wound_engine_v1(combat) {
        cards
            .iter()
            .filter(|card| card.id == CardId::Wound)
            .count()
            .try_into()
            .unwrap_or(i32::MAX)
    } else {
        0
    };
    let visible_loss = super::super::visible_incoming_damage(combat)
        .saturating_sub(combat.entities.player.block)
        .max(0);
    let preserved_tactical_cards = cards
        .iter()
        .filter(|card| {
            let definition = cards::get_card_definition(card.id);
            let preserves_visible_block = visible_loss > 0
                && card.cost_for_turn_java() >= 0
                && card.cost_for_turn_java() <= i32::from(combat.turn.energy)
                && definition.base_block.saturating_add(
                    definition
                        .upgrade_block
                        .saturating_mul(i32::from(card.upgrades)),
                ) > 0;
            let preserves_strength_control =
                card_reward_facts_v1(&RewardCard::new(card.id, card.upgrades)).enemy_strength_down
                    > 0;
            preserves_visible_block || preserves_strength_control
        })
        .count()
        .try_into()
        .unwrap_or(i32::MAX);
    PendingChoiceOrderingHint {
        role: PendingChoiceOrderingRole::RemovalSelection,
        primary: facts
            .removal_value
            .saturating_sub(
                preserved_wounds.saturating_mul(CONNECTED_SECOND_WIND_WOUND_PRESERVATION),
            )
            .saturating_sub(preserved_tactical_cards.saturating_mul(TACTICAL_CARD_PRESERVATION)),
        secondary: -facts.keep_value,
        selected_count_tiebreak: -(selected_count as i32),
        policy_log2_bias: preserved_wounds
            .saturating_mul(CONNECTED_SECOND_WIND_WOUND_LOG2_BIAS)
            .saturating_add(
                preserved_tactical_cards.saturating_mul(TACTICAL_CARD_PRESERVATION_LOG2_BIAS),
            ),
    }
}

pub(super) fn selection_hint_for_grid_reason(
    reason: GridSelectReason,
    cards: &[&CombatCard],
    selected_count: usize,
) -> PendingChoiceOrderingHint {
    match reason {
        GridSelectReason::MoveToDrawPile
        | GridSelectReason::DrawPileToHand
        | GridSelectReason::SkillFromDeckToHand
        | GridSelectReason::AttackFromDeckToHand
        | GridSelectReason::DiscardToHand
        | GridSelectReason::DiscardToHandNoCostChange
        | GridSelectReason::DiscardToHandRetain => value_selection_hint(cards, selected_count),
        GridSelectReason::Exhume { upgrade } => {
            exhume_selection_hint(cards, selected_count, upgrade)
        }
        GridSelectReason::Omniscience { play_amount } => {
            repeated_value_selection_hint(cards, selected_count, play_amount)
        }
    }
}

fn value_selection_hint(cards: &[&CombatCard], selected_count: usize) -> PendingChoiceOrderingHint {
    let facts = aggregate_card_facts(cards.iter().copied().map(CardSelectionFacts::from_card));
    PendingChoiceOrderingHint {
        role: PendingChoiceOrderingRole::ValueSelection,
        primary: facts.keep_value,
        secondary: -facts.removal_value,
        selected_count_tiebreak: -(selected_count as i32),
        policy_log2_bias: 0,
    }
}

pub(super) fn value_selection_hint_from_card_id(
    card_id: CardId,
    selected_count: usize,
) -> PendingChoiceOrderingHint {
    let facts = CardSelectionFacts::from_card_id(card_id);
    PendingChoiceOrderingHint {
        role: PendingChoiceOrderingRole::ValueSelection,
        primary: facts.keep_value,
        secondary: -facts.removal_value,
        selected_count_tiebreak: -(selected_count as i32),
        policy_log2_bias: 0,
    }
}

fn repeated_value_selection_hint(
    cards: &[&CombatCard],
    selected_count: usize,
    repeat_count: u8,
) -> PendingChoiceOrderingHint {
    let facts = aggregate_card_facts(cards.iter().copied().map(CardSelectionFacts::from_card));
    let repeat_count = i32::from(repeat_count.max(1));
    PendingChoiceOrderingHint {
        role: PendingChoiceOrderingRole::ValueSelection,
        primary: facts.keep_value.saturating_mul(repeat_count),
        secondary: facts.upgrade_value.saturating_sub(facts.removal_value),
        selected_count_tiebreak: -(selected_count as i32),
        policy_log2_bias: 0,
    }
}

fn upgrade_selection_hint(
    cards: &[&CombatCard],
    selected_count: usize,
) -> PendingChoiceOrderingHint {
    let facts = aggregate_card_facts(cards.iter().copied().map(CardSelectionFacts::from_card));
    PendingChoiceOrderingHint {
        role: PendingChoiceOrderingRole::ValueSelection,
        primary: facts.upgrade_value,
        secondary: facts.keep_value.saturating_sub(facts.removal_value),
        selected_count_tiebreak: -(selected_count as i32),
        policy_log2_bias: 0,
    }
}

fn exhume_selection_hint(
    cards: &[&CombatCard],
    selected_count: usize,
    upgrade: bool,
) -> PendingChoiceOrderingHint {
    let facts = aggregate_card_facts(cards.iter().copied().map(CardSelectionFacts::from_card));
    let upgrade_bonus = if upgrade { facts.upgrade_value } else { 0 };
    PendingChoiceOrderingHint {
        role: PendingChoiceOrderingRole::ValueSelection,
        primary: facts.keep_value.saturating_add(upgrade_bonus),
        secondary: -facts.removal_value,
        selected_count_tiebreak: -(selected_count as i32),
        policy_log2_bias: 0,
    }
}

fn removal_selection_hint(
    cards: &[&CombatCard],
    selected_count: usize,
) -> PendingChoiceOrderingHint {
    let facts = aggregate_card_facts(cards.iter().copied().map(CardSelectionFacts::from_card));
    PendingChoiceOrderingHint {
        role: PendingChoiceOrderingRole::RemovalSelection,
        primary: facts.removal_value,
        secondary: -facts.keep_value,
        selected_count_tiebreak: -(selected_count as i32),
        policy_log2_bias: 0,
    }
}

pub(super) fn removal_selection_hint_from_card_ids(
    card_ids: &[CardId],
    selected_count: usize,
) -> PendingChoiceOrderingHint {
    let facts = aggregate_card_facts(
        card_ids
            .iter()
            .copied()
            .map(CardSelectionFacts::from_card_id),
    );
    PendingChoiceOrderingHint {
        role: PendingChoiceOrderingRole::RemovalSelection,
        primary: facts.removal_value,
        secondary: -facts.keep_value,
        selected_count_tiebreak: -(selected_count as i32),
        policy_log2_bias: 0,
    }
}

fn recycle_selection_hint(
    cards: &[&CombatCard],
    selected_count: usize,
) -> PendingChoiceOrderingHint {
    let facts = aggregate_card_facts(cards.iter().copied().map(CardSelectionFacts::from_card));
    let energy_return = cards
        .iter()
        .map(|card| card.combat_cost_without_turn_override_java().max(0))
        .sum::<i32>();
    PendingChoiceOrderingHint {
        role: PendingChoiceOrderingRole::RemovalSelection,
        primary: energy_return
            .saturating_mul(RECYCLE_ENERGY_FACTOR)
            .saturating_add(facts.removal_value),
        secondary: -facts.keep_value,
        selected_count_tiebreak: -(selected_count as i32),
        policy_log2_bias: 0,
    }
}

fn draw_pile_setup_selection_hint(
    cards: &[&CombatCard],
    selected_count: usize,
) -> PendingChoiceOrderingHint {
    let facts = aggregate_card_facts(cards.iter().copied().map(CardSelectionFacts::from_card));
    let currently_expensive = cards
        .iter()
        .filter(|card| card.cost_for_turn_java() > 0)
        .count() as i32;
    PendingChoiceOrderingHint {
        role: PendingChoiceOrderingRole::ValueSelection,
        primary: facts
            .keep_value
            .saturating_add(currently_expensive.saturating_mul(SETUP_EXPENSIVE_CARD_BONUS)),
        secondary: -facts.removal_value,
        selected_count_tiebreak: -(selected_count as i32),
        policy_log2_bias: 0,
    }
}
