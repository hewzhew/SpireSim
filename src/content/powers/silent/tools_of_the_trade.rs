use crate::runtime::action::Action;

pub fn on_post_draw(_owner: usize, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    let amount = amount.max(0);
    if amount == 0 {
        smallvec::smallvec![]
    } else {
        smallvec::smallvec![
            Action::DrawCards(amount as u32),
            Action::DiscardFromHand {
                amount,
                random: false,
                end_turn: false,
            },
        ]
    }
}
