use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct AncientTeaSet;

impl AncientTeaSet {
    pub fn at_pre_battle(
        relic_state: &mut crate::content::relics::RelicState,
    ) -> SmallVec<[ActionInfo; 4]> {
        relic_state.used_up = false;
        SmallVec::new()
    }

    pub fn at_turn_start(
        relic_state: &mut crate::content::relics::RelicState,
    ) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();

        if !relic_state.used_up {
            relic_state.used_up = true;
            if relic_state.counter == -2 {
                relic_state.counter = -1;
                actions.push(ActionInfo {
                    action: Action::GainEnergy { amount: 2 },
                    insertion_mode: AddTo::Top,
                });
            }
        }

        actions
    }

    pub fn on_enter_rest_room(relic_state: &mut crate::content::relics::RelicState) {
        relic_state.counter = -2;
    }
}
