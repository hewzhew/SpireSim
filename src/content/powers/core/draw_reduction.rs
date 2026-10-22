pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::DrawReduction,
        round_duration: false,
    };

use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

pub fn at_end_of_round(
    owner: EntityId,
    _amount: i32,
    just_applied: bool,
) -> smallvec::SmallVec<[Action; 2]> {
    if just_applied {
        return smallvec::smallvec![];
    }

    smallvec::smallvec![Action::ReducePower {
        target: owner,
        power_id: PowerId::DrawReduction,
        amount: 1,
    }]
}
