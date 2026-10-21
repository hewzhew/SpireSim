use crate::content::powers::PowerId;
use crate::runtime::action::Action;

pub fn at_end_of_turn(owner: usize, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    smallvec::smallvec![Action::ApplyPower {
        source: owner,
        target: owner,
        power_id: PowerId::Dexterity,
        amount,
    }]
}
