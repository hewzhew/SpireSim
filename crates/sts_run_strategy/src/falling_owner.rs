use crate::content::cards::{
    get_card_definition, is_starter_defend, is_starter_strike, CardId, CardType,
};
use crate::runtime::combat::{CardLineageId, CombatCard};
use crate::state::events::{EventActionKind, EventEffect, EventOptionSemantics};
use crate::state::run::RunState;

use super::owner_policy::EventOwnerOptionSelector;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct RemovalHarm {
    tier: u8,
    upgraded_penalty: u8,
    unique_penalty: u8,
}

pub(super) fn falling_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => EventOwnerOptionSelector::Action(EventActionKind::Continue),
        1 => falling_remove_choice(run_state)
            .unwrap_or(EventOwnerOptionSelector::Action(EventActionKind::Decline)),
        _ => EventOwnerOptionSelector::Action(EventActionKind::Leave),
    }
}

fn falling_remove_choice(run_state: &RunState) -> Option<EventOwnerOptionSelector> {
    crate::engine::event_handler::get_event_options(run_state)
        .into_iter()
        .enumerate()
        .filter(|(_, option)| !option.ui.disabled)
        .filter_map(|(index, option)| {
            let lineage = remove_target_lineage(&option.semantics)?;
            let card = run_state
                .master_deck
                .iter()
                .find(|card| card.lineage == lineage)?;
            Some((falling_removal_harm(card, run_state), index))
        })
        .min_by_key(|(harm, index)| (*harm, *index))
        .map(|(_, index)| EventOwnerOptionSelector::OptionIndex(index))
}

fn remove_target_lineage(semantics: &EventOptionSemantics) -> Option<CardLineageId> {
    semantics.effects.iter().find_map(|effect| match effect {
        EventEffect::RemoveCard {
            count: 1,
            target_lineage: Some(lineage),
            ..
        } => Some(*lineage),
        _ => None,
    })
}

fn falling_removal_harm(card: &CombatCard, run_state: &RunState) -> RemovalHarm {
    let tier = if get_card_definition(card.id).card_type == CardType::Curse {
        0
    } else if is_starter_strike(card.id) {
        1
    } else if is_starter_defend(card.id) {
        if has_corruption(run_state) && skill_count(run_state) <= 7 {
            2
        } else {
            1
        }
    } else if is_critical_card(card.id, run_state) {
        6
    } else if is_high_protect_card(card.id) {
        5
    } else if is_unsupported_payoff(card.id, run_state) || is_low_impact_card(card.id) {
        2
    } else {
        3
    };
    RemovalHarm {
        tier,
        upgraded_penalty: u8::from(card.upgrades > 0),
        unique_penalty: u8::from(same_card_count(run_state, card.id) <= 1),
    }
}

fn is_critical_card(card: CardId, run_state: &RunState) -> bool {
    match card {
        CardId::Corruption | CardId::DarkEmbrace => true,
        CardId::DemonForm | CardId::Inflame | CardId::SpotWeakness => {
            is_unique_strength_scaling_source(card, run_state)
        }
        CardId::FeelNoPain => {
            has_corruption(run_state) || same_card_count(run_state, CardId::FeelNoPain) == 1
        }
        CardId::LimitBreak => has_strength_source(run_state),
        CardId::Barricade | CardId::Entrench => has_block_engine(run_state),
        CardId::Evolve => has_status_source(run_state),
        _ => false,
    }
}

fn is_unique_strength_scaling_source(card: CardId, run_state: &RunState) -> bool {
    is_direct_strength_source(card)
        && run_state
            .master_deck
            .iter()
            .filter(|candidate| is_direct_strength_source(candidate.id))
            .count()
            == 1
}

fn is_direct_strength_source(card: CardId) -> bool {
    matches!(
        card,
        CardId::Inflame | CardId::SpotWeakness | CardId::DemonForm
    )
}

fn is_high_protect_card(card: CardId) -> bool {
    matches!(
        card,
        CardId::Offering
            | CardId::BattleTrance
            | CardId::BurningPact
            | CardId::ShrugItOff
            | CardId::Shockwave
            | CardId::Disarm
            | CardId::Impervious
            | CardId::FiendFire
            | CardId::Feed
            | CardId::Reaper
            | CardId::Immolate
            | CardId::Whirlwind
            | CardId::SecondWind
            | CardId::PowerThrough
            | CardId::TrueGrit
            | CardId::Uppercut
    )
}

fn is_unsupported_payoff(card: CardId, run_state: &RunState) -> bool {
    matches!(
        card,
        CardId::HeavyBlade | CardId::SwordBoomerang | CardId::Pummel
    ) && !has_strength_source(run_state)
        || matches!(card, CardId::BodySlam | CardId::Juggernaut) && !has_block_engine(run_state)
        || card == CardId::FireBreathing && !has_status_source(run_state)
        || card == CardId::Rupture && !has_self_damage_source(run_state)
        || card == CardId::PerfectedStrike
            && run_state
                .master_deck
                .iter()
                .filter(|card| matches!(card.id, CardId::Strike | CardId::PerfectedStrike))
                .count()
                < 5
}

fn is_low_impact_card(card: CardId) -> bool {
    matches!(
        card,
        CardId::Clash
            | CardId::WildStrike
            | CardId::TwinStrike
            | CardId::Cleave
            | CardId::Clothesline
            | CardId::IronWave
            | CardId::Flex
            | CardId::Warcry
            | CardId::Havoc
            | CardId::Metallicize
    )
}

fn has_corruption(run_state: &RunState) -> bool {
    has_card(run_state, CardId::Corruption)
}

fn has_strength_source(run_state: &RunState) -> bool {
    run_state.master_deck.iter().any(|card| {
        matches!(
            card.id,
            CardId::Inflame | CardId::SpotWeakness | CardId::DemonForm
        )
    })
}

fn has_block_engine(run_state: &RunState) -> bool {
    run_state.master_deck.iter().any(|card| {
        matches!(
            card.id,
            CardId::Barricade
                | CardId::Entrench
                | CardId::BodySlam
                | CardId::Impervious
                | CardId::PowerThrough
                | CardId::FlameBarrier
        )
    })
}

fn has_status_source(run_state: &RunState) -> bool {
    run_state.master_deck.iter().any(|card| {
        matches!(
            card.id,
            CardId::PowerThrough | CardId::WildStrike | CardId::RecklessCharge
        )
    })
}

fn has_self_damage_source(run_state: &RunState) -> bool {
    run_state.master_deck.iter().any(|card| {
        matches!(
            card.id,
            CardId::Offering | CardId::Bloodletting | CardId::Hemokinesis
        )
    })
}

fn skill_count(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| get_card_definition(card.id).card_type == CardType::Skill)
        .count()
}

fn same_card_count(run_state: &RunState, id: CardId) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| card.id == id)
        .count()
}

fn has_card(run_state: &RunState, id: CardId) -> bool {
    run_state.master_deck.iter().any(|card| card.id == id)
}

fn event_screen(run_state: &RunState) -> usize {
    run_state
        .event_state
        .as_ref()
        .map(|event| event.current_screen)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_demon_form_is_protected_over_generic_access_card() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        let demon_form = CombatCard::for_scenario(CardId::DemonForm, 1);
        let shrug = CombatCard::for_scenario(CardId::ShrugItOff, 2);
        run_state.master_deck = vec![demon_form.clone(), shrug.clone()];

        assert!(
            falling_removal_harm(&demon_form, &run_state)
                > falling_removal_harm(&shrug, &run_state),
            "unique Demon Form should be harder for Falling to sacrifice than a generic access card"
        );
    }

    #[test]
    fn limit_break_does_not_count_itself_as_a_strength_source() {
        let mut run_state = RunState::new(1, 0, true, "Ironclad");
        run_state.master_deck = vec![CombatCard::for_scenario(CardId::LimitBreak, 1)];

        assert!(!has_strength_source(&run_state));
        assert!(!is_critical_card(CardId::LimitBreak, &run_state));
    }
}
