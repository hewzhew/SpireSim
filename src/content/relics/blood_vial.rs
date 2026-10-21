use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct BloodVial;

impl BloodVial {
    pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        actions.push(ActionInfo {
            action: Action::Heal {
                target: 0,
                amount: 2,
            },
            insertion_mode: AddTo::Top, // Java: addToTop
        });
        actions
    }
}
