use crate::content::cards::{CardId, CardType};

use super::run_choice::RunPendingChoiceReason;

pub(crate) fn run_pending_choice_allows_card(
    reason: &RunPendingChoiceReason,
    card: &crate::runtime::combat::CombatCard,
) -> bool {
    let def = crate::content::cards::get_card_definition(card.id);
    match reason {
        RunPendingChoiceReason::Purge
        | RunPendingChoiceReason::PurgeNonBottled
        | RunPendingChoiceReason::Transform
        | RunPendingChoiceReason::TransformNonBottled
        | RunPendingChoiceReason::TransformUpgraded => master_deck_card_is_purgeable(card),
        RunPendingChoiceReason::Upgrade => master_deck_card_can_upgrade(card),
        RunPendingChoiceReason::BottleFlame => {
            master_deck_card_is_purgeable(card) && def.card_type == CardType::Attack
        }
        RunPendingChoiceReason::BottleLightning => {
            master_deck_card_is_purgeable(card) && def.card_type == CardType::Skill
        }
        RunPendingChoiceReason::BottleTornado => {
            master_deck_card_is_purgeable(card) && def.card_type == CardType::Power
        }
        _ => true,
    }
}

pub fn run_pending_choice_allows_card_for_run(
    reason: &RunPendingChoiceReason,
    card: &crate::runtime::combat::CombatCard,
    run_state: &crate::state::run::RunState,
) -> bool {
    if !run_pending_choice_allows_card(reason, card) {
        return false;
    }

    match reason {
        RunPendingChoiceReason::PurgeNonBottled | RunPendingChoiceReason::TransformNonBottled => {
            !master_deck_card_is_bottled(card, &run_state.relics)
        }
        _ => true,
    }
}

pub fn master_deck_card_can_upgrade(card: &crate::runtime::combat::CombatCard) -> bool {
    let def = crate::content::cards::get_card_definition(card.id);
    card.id == CardId::SearingBlow
        || (card.upgrades == 0
            && def.card_type != CardType::Status
            && def.card_type != CardType::Curse)
}

pub fn master_deck_card_is_purgeable(card: &crate::runtime::combat::CombatCard) -> bool {
    !matches!(
        card.id,
        CardId::AscendersBane | CardId::CurseOfTheBell | CardId::Necronomicurse
    )
}

pub fn master_deck_card_is_bottled(
    card: &crate::runtime::combat::CombatCard,
    relics: &[crate::content::relics::RelicState],
) -> bool {
    relics.iter().any(|relic| {
        matches!(
            relic.id,
            crate::content::relics::RelicId::BottledFlame
                | crate::content::relics::RelicId::BottledLightning
                | crate::content::relics::RelicId::BottledTornado
        ) && relic.amount != 0
            && crate::runtime::combat::CardLineageId::from_runtime_i32_payload(relic.amount)
                == Some(card.lineage)
    })
}

fn non_bottled_purgeable_master_deck_count(run_state: &crate::state::run::RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| {
            master_deck_card_is_purgeable(card)
                && !master_deck_card_is_bottled(card, &run_state.relics)
        })
        .count()
}

/// Reports whether a run-level remove-card choice has at least one legal
/// non-bottled target in the current master deck.
pub fn has_non_bottled_purgeable_master_deck_card(run_state: &crate::state::run::RunState) -> bool {
    non_bottled_purgeable_master_deck_count(run_state) > 0
}
