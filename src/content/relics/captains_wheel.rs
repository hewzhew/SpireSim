use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct CaptainsWheel;

impl CaptainsWheel {
    pub fn at_battle_start(relic_state: &mut crate::content::relics::RelicState) {
        relic_state.counter = 0;
    }

    pub fn at_turn_start(
        relic_state: &mut crate::content::relics::RelicState,
    ) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        if relic_state.counter != -1 {
            relic_state.counter += 1;
        }

        if relic_state.counter == 3 {
            relic_state.counter = -1;
            actions.push(ActionInfo {
                action: Action::GainBlock {
                    target: 0,
                    amount: 18,
                },
                insertion_mode: AddTo::Bottom,
            });
        }
        actions
    }

    pub fn on_victory(relic_state: &mut crate::content::relics::RelicState) {
        relic_state.counter = -1;
    }
}
