use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct Abacus;
pub const MECHANICS: &str = "Whenever your discard pile is shuffled into your draw pile, gain 6 Block. Can trigger multiple times in a turn. This relic does not itself cause a shuffle; reaching an empty draw pile while drawing can cause one.";

impl Abacus {
    pub fn on_shuffle() -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        actions.push(ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: 6,
            },
            insertion_mode: AddTo::Bottom,
        });
        actions
    }
}
