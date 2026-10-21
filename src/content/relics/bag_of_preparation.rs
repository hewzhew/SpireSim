use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct BagOfPreparation;

impl BagOfPreparation {
    pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        actions.push(ActionInfo {
            action: Action::DrawCards(2),
            insertion_mode: AddTo::Bottom,
        });
        actions
    }
}
