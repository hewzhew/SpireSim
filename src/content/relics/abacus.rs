use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct Abacus;
pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::Abacus,
};

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
