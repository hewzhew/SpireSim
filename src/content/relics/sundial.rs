use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;
pub const SHUFFLES_PER_TRIGGER: i32 = 3;
pub const ENERGY_AMOUNT: i32 = 2;

/// Sundial: Every 3 times you shuffle your draw pile, gain 2 Energy.
pub fn on_shuffle(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    relic_state.counter += 1;
    if relic_state.counter == SHUFFLES_PER_TRIGGER {
        relic_state.counter = 0;
        actions.push(ActionInfo {
            action: Action::GainEnergy {
                amount: ENERGY_AMOUNT,
            },
            insertion_mode: AddTo::Bottom, // Java: addToBot
        });
    }

    actions
}
