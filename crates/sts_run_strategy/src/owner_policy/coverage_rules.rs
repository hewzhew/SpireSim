use crate::state::events::EventActionKind;
use crate::state::run::RunState;

use super::{action, event_screen, option_index, EventOwnerOptionSelector};

use crate::ai::deck_mutation_compiler_v1::best_duplicate_target_for_shop_v1;
use crate::ai::event_resource_budget::EventGainClass;
use crate::content::cards::CardId;
use crate::state::events::{EventCardKind, EventEffect};

use super::{
    effect, event_resource_budget_for, has_omamori_charge, has_safe_purge_target, hp_loss_class,
    spend_reserved_or_worse,
};

pub(super) fn gremlin_wheel_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 | 1 | 3 => action(EventActionKind::Special),
        _ => action(EventActionKind::Leave),
    }
}

pub(super) fn lab_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Gain),
        _ => action(EventActionKind::Leave),
    }
}

pub(super) fn colosseum_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Continue),
        1 => action(EventActionKind::Fight),
        _ => action(EventActionKind::Leave),
    }
}

pub(super) fn the_joust_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 | 2 | 3 => action(EventActionKind::Continue),
        1 => option_index(0),
        _ => action(EventActionKind::Leave),
    }
}

pub(super) fn golden_shrine_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    if has_omamori_charge(run_state) {
        return effect(EventEffect::ObtainCurse {
            count: 1,
            kind: EventCardKind::Specific(CardId::Regret),
        });
    }
    let gold = if run_state.ascension_level >= 15 {
        50
    } else {
        100
    };
    effect(EventEffect::GainGold(gold))
}

pub(super) fn fountain_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if crate::content::events::fountain::removable_curse_count(run_state) > 0 => {
            action(EventActionKind::DeckOperation)
        }
        _ => action(EventActionKind::Leave),
    }
}

pub(super) fn upgrade_shrine_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if run_state
            .master_deck
            .iter()
            .any(crate::state::core::master_deck_card_can_upgrade) =>
        {
            action(EventActionKind::DeckOperation)
        }
        _ => action(EventActionKind::Leave),
    }
}

pub(super) fn accursed_blacksmith_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    if run_state
        .master_deck
        .iter()
        .any(crate::state::core::master_deck_card_can_upgrade)
    {
        action(EventActionKind::DeckOperation)
    } else if has_omamori_charge(run_state) {
        action(EventActionKind::Trade)
    } else {
        action(EventActionKind::Leave)
    }
}

pub(super) fn duplicator_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if best_duplicate_target_for_shop_v1(run_state).is_some() => {
            action(EventActionKind::DeckOperation)
        }
        _ => action(EventActionKind::Leave),
    }
}

pub(super) fn note_for_yourself_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Continue),
        1 if !default_note_is_ignorable(run_state) && has_safe_purge_target(run_state) => {
            action(EventActionKind::DeckOperation)
        }
        1 => action(EventActionKind::Decline),
        _ => action(EventActionKind::Leave),
    }
}

pub(super) fn ssssserpent_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 if has_omamori_charge(run_state) => action(EventActionKind::Accept),
        0 => action(EventActionKind::Decline),
        1 => action(EventActionKind::Continue),
        _ => action(EventActionKind::Leave),
    }
}

pub(super) fn addict_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    if event_screen(run_state) != 0 {
        return action(EventActionKind::Leave);
    }
    if has_omamori_charge(run_state) {
        return option_index(1);
    }
    let budget = event_resource_budget_for(run_state);
    let can_pay = run_state.gold >= 85
        && run_state.gold - 85 >= budget.gold.estimated_next_shop_purge_cost
        && !spend_reserved_or_worse(budget.gold.spend_75);
    if can_pay {
        option_index(0)
    } else {
        action(EventActionKind::Leave)
    }
}

pub(super) fn knowing_skull_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Continue),
        1 => {
            let state = run_state
                .event_state
                .as_ref()
                .map(|event| event.internal_state)
                .unwrap_or_default();
            let cost = crate::content::events::knowing_skull::gold_cost(state);
            let budget = event_resource_budget_for(run_state);
            if budget.gold.gold_gain != EventGainClass::Blocked
                && !spend_reserved_or_worse(hp_loss_class(&budget, cost))
            {
                effect(EventEffect::GainGold(90))
            } else {
                action(EventActionKind::Leave)
            }
        }
        _ => action(EventActionKind::Leave),
    }
}

pub(super) fn sensory_stone_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Continue),
        1 => option_index(sensory_focus_choice(run_state)),
        _ => action(EventActionKind::Leave),
    }
}

fn default_note_is_ignorable(run_state: &RunState) -> bool {
    let note_card = run_state.note_for_yourself_card;
    let definition = crate::content::cards::get_card_definition(note_card);
    if matches!(
        definition.card_type,
        crate::content::cards::CardType::Curse | crate::content::cards::CardType::Status
    ) {
        return true;
    }
    note_card == crate::content::cards::CardId::IronWave
        && run_state.note_for_yourself_upgrades == 0
}

fn sensory_focus_choice(run_state: &RunState) -> usize {
    if sensory_hp_safe_after(run_state, 10) {
        2
    } else if sensory_hp_safe_after(run_state, 5) {
        1
    } else {
        0
    }
}

fn sensory_hp_safe_after(run_state: &RunState, loss: i32) -> bool {
    let after = run_state.current_hp - loss;
    after >= 20 && after * 100 >= run_state.max_hp * 35
}

pub(super) fn secret_portal_choice(run_state: &RunState) -> EventOwnerOptionSelector {
    match event_screen(run_state) {
        0 => action(EventActionKind::Decline),
        1 => action(EventActionKind::Special),
        _ => action(EventActionKind::Leave),
    }
}
