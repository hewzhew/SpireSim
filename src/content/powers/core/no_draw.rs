use crate::runtime::action::Action;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::NoDraw,
        round_duration: false,
    };

pub fn at_end_of_turn(owner: EntityId) -> smallvec::SmallVec<[Action; 2]> {
    smallvec::smallvec![Action::RemovePower {
        target: owner,
        power_id: crate::content::powers::PowerId::NoDraw,
    }]
}
