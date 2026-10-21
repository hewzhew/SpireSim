pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Strength restored at the owner's turn end",
    effect: "At the end of the owner's turn, apply amount Strength to the owner only if amount is positive, then remove Shackled. Non-positive amounts restore no Strength; Shackled is still removed. The Strength reduction is a separate effect, not performed by Shackled itself.",
    timing: "Removed at the owner's turn end, not an end-of-round duration counter.",
    round_duration: false,
};

use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

pub fn at_end_of_turn(owner: EntityId, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];
    if amount > 0 {
        actions.push(Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: PowerId::Strength,
            amount,
        });
    }
    actions.push(Action::RemovePower {
        target: owner,
        power_id: PowerId::Shackled,
    });
    actions
}
