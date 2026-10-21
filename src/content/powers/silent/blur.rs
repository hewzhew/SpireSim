use crate::runtime::action::Action;
use crate::EntityId;
use smallvec::SmallVec;

pub fn at_end_of_round(owner: EntityId, amount: i32) -> SmallVec<[Action; 2]> {
    if amount == 0 {
        smallvec::smallvec![Action::RemovePower {
            target: owner,
            power_id: crate::content::powers::PowerId::Blur,
        }]
    } else {
        smallvec::smallvec![Action::ReducePower {
            target: owner,
            power_id: crate::content::powers::PowerId::Blur,
            amount: 1,
        }]
    }
}
