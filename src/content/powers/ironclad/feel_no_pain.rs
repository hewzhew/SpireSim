use crate::runtime::action::Action;
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Block gained per card Exhausted",
    effect: "While active, whenever any card is Exhausted, gain amount Block. Multiple deployed copies add their amounts. This trigger does not draw cards or give Energy; Dexterity and Frail do not modify this power's Block.",
    timing: "On Exhaust, after deployment; persists for the rest of combat. The Block itself follows ordinary turn-end Block rules.",
    round_duration: false,
};

pub fn on_exhaust(owner: crate::EntityId, amount: i32) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    actions.push(Action::GainBlock {
        target: owner,
        amount,
    });
    actions
}
