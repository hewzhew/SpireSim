use crate::runtime::action::Action;
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Metallicize,
        round_duration: false,
    };

pub fn at_end_of_turn(owner: crate::EntityId, amount: i32) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    actions.push(Action::GainBlock {
        target: owner,
        amount,
    });
    actions
}
