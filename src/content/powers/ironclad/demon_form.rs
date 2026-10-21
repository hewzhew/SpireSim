use crate::runtime::action::Action;
use smallvec::SmallVec;

pub fn on_post_draw(owner: crate::EntityId, amount: i32) -> SmallVec<[Action; 2]> {
    smallvec::smallvec![Action::ApplyPower {
        source: owner,
        target: owner,
        power_id: crate::content::powers::PowerId::Strength,
        amount,
    }]
}
