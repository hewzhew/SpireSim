use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct HappyFlower;

impl HappyFlower {
    pub fn at_turn_start(
        relic_state: &mut crate::content::relics::RelicState,
    ) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();

        relic_state.counter = if relic_state.counter == -1 {
            relic_state.counter + 2
        } else {
            relic_state.counter + 1
        };

        if relic_state.counter == 3 {
            relic_state.counter = 0;
            actions.push(ActionInfo {
                action: Action::GainEnergy { amount: 1 },
                insertion_mode: AddTo::Bottom,
            });
        }
        actions
    }
}
