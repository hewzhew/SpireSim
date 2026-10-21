use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Shuriken: Every time you play 3 Attacks in a single turn, gain 1 Strength.
/// Java: counter-based, resets each turn.
pub fn at_turn_start(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = 0;
}

pub fn on_use_card(
    card_id: crate::content::cards::CardId,
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    let def = crate::content::cards::get_card_definition(card_id);

    if def.card_type == crate::content::cards::CardType::Attack {
        let current = relic_state.counter.max(0);
        let next_counter = if current + 1 >= 3 { 0 } else { current + 1 };
        relic_state.counter = next_counter;

        if next_counter == 0 {
            actions.push(ActionInfo {
                action: Action::ApplyPower {
                    source: 0,
                    target: 0,
                    power_id: crate::content::powers::PowerId::Strength,
                    amount: 1,
                },
                insertion_mode: AddTo::Bottom,
            });
        }
    }

    actions
}

pub fn on_victory(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = -1;
}
