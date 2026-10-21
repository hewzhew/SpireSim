use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

pub const MECHANICS: crate::content::powers::mechanics::PowerMechanics =
    crate::content::powers::mechanics::PowerMechanics {
        amount_unit: "permanent Strength gained each round",
        effect: "At the end of each round, gain Strength equal to amount if amount is positive. The effect itself persists and does not decrement. It does not grant Strength immediately when applied.",
        timing: "End-of-round power hook, after the monsters have executed their moves; the gained Strength affects later attacks.",
        round_duration: false,
    };

pub fn at_end_of_round(owner: EntityId, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];
    if amount > 0 {
        actions.push(Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: PowerId::Strength,
            amount,
        });
    }
    actions
}
