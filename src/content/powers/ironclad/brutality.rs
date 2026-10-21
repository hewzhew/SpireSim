use crate::runtime::action::Action;
use smallvec::SmallVec;

pub fn on_post_draw(owner: crate::EntityId, amount: i32) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    actions.push(Action::DrawCards(amount as u32));
    actions.push(Action::LoseHp {
        target: owner,
        amount,
        triggers_rupture: true,
    });
    actions
}
