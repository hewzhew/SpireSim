pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Weak,
        round_duration: true,
    };

use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

pub fn at_end_of_round(
    owner: EntityId,
    amount: i32,
    just_applied: bool,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];
    if just_applied {
        return actions;
    }
    if amount == 0 {
        actions.push(Action::RemovePower {
            target: owner,
            power_id: PowerId::Weak,
        });
    } else {
        actions.push(Action::ReducePower {
            target: owner,
            power_id: PowerId::Weak,
            amount: 1,
        });
    }
    actions
}
