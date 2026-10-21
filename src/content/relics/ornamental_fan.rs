use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Ornamental Fan: Every time you play 3 Attacks in a single turn, gain 4 Block.
pub fn on_use_card(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    let current = if relic_state.counter < 0 {
        0
    } else {
        relic_state.counter
    };
    let next_counter = current + 1;

    if next_counter >= 3 {
        relic_state.counter = 0;
        actions.push(ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: 4,
            },
            insertion_mode: AddTo::Bottom,
        });
    } else {
        relic_state.counter = next_counter;
    }

    actions
}

pub fn at_turn_start(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = 0;
}

pub fn on_victory(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = -1;
}
