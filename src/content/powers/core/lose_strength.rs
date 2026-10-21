use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Strength to subtract at owner's turn end",
    effect: "Subtract the stored amount from the owner's then-current Strength, then remove this power. This is a fixed subtraction, not a reset of Strength; other Strength gains remain except for that subtraction. Artifact can block application of this debuff.",
    timing: "Owner's turn end, once; not a recurring loss each round.",
    round_duration: false,
};

pub fn at_end_of_turn(owner: EntityId, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::SmallVec::new();
    actions.push(Action::ApplyPower {
        source: owner,
        target: owner,
        power_id: PowerId::Strength,
        amount: -amount,
    });
    actions.push(Action::RemovePower {
        target: owner,
        power_id: PowerId::LoseStrength,
    });
    actions
}
