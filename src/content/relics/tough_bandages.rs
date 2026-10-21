use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

/// ToughBandages: Whenever you discard a card, gain 3 Block.
pub fn on_discard() -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: Action::GainBlock {
            target: 0,
            amount: 3,
        },
        insertion_mode: AddTo::Bottom,
    });
    actions
}
