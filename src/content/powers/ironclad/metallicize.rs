use crate::runtime::action::Action;
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Block gained at each owner's turn end",
    effect: "Gain amount Block at the end of the owner's turn. Does not decay each round; Lagavulin removes its sleeping Metallicize when it wakes.",
    timing: "Owner turn end, not on card play or damage. For the player, end-of-turn relic checks such as Orichalcum happen before this Block gain; this gain then precedes queued end-of-turn Status and Curse effects.",
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
