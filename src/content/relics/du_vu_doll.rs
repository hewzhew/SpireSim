use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::state::core::EngineState;
use crate::state::run::RunState;

/// Du-Vu Doll: At the start of each combat, gain 1 Strength for each Curse in your deck.
/// Java: atBattleStart() → count curses in masterDeck → applyPower(Strength, count)
pub fn at_battle_start(
    _state: &CombatState,
    relic: &mut RelicState,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();
    let curse_count = relic.counter;

    if curse_count > 0 {
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                target: 0,
                source: 0,
                power_id: crate::content::powers::PowerId::Strength,
                amount: curse_count,
            },
            insertion_mode: AddTo::Top,
        });
    }
    actions
}

pub fn on_equip(run_state: &mut RunState) -> Option<EngineState> {
    refresh_counters_from_deck(&run_state.master_deck, &mut run_state.relics);
    None
}

pub fn refresh_counters_from_deck(deck: &[CombatCard], relics: &mut [RelicState]) {
    let curse_count = curse_count_in_deck(deck);
    for relic in relics {
        if relic.id == crate::content::relics::RelicId::DuVuDoll {
            relic.counter = curse_count;
        }
    }
}

pub fn curse_count_in_deck(deck: &[CombatCard]) -> i32 {
    deck.iter()
        .filter(|card| {
            crate::content::cards::get_card_definition(card.id).card_type
                == crate::content::cards::CardType::Curse
        })
        .count() as i32
}
