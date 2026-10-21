use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Snake Ring: At the start of each combat, draw 2 additional cards.
pub fn at_battle_start() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    actions.push(ActionInfo {
        action: Action::DrawCards(2),
        insertion_mode: AddTo::Bottom,
    });

    actions
}
