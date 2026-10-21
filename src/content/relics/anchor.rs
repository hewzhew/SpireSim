use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct Anchor;

impl Anchor {
    pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        actions.push(ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: 10,
            },
            insertion_mode: AddTo::Bottom,
        });
        actions
    }
}
