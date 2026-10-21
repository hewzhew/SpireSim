use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// Melange: Upon shuffling your draw pile, Scry 3.
pub fn on_shuffle() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: Action::Scry(3),
        insertion_mode: AddTo::Bottom,
    });
    actions
}
