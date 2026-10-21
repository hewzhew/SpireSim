use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Dexterity to subtract at owner's turn end",
    effect: "Subtract the stored amount from the owner's then-current Dexterity, then remove this power. Already gained Block stays. This is a fixed subtraction, not a reset; Artifact can block application of this debuff.",
    timing: "Owner's turn end, once; Speed Potion's temporary Dexterity expires this way.",
    round_duration: false,
};

/// Java: LoseDexterityPower.atEndOfTurn() applies negative Dexterity,
/// then removes the temporary Lose Dex power itself.
pub fn at_end_of_turn(owner: EntityId, amount: i32) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::SmallVec::new();
    actions.push(Action::ApplyPower {
        source: owner,
        target: owner,
        power_id: PowerId::Dexterity,
        amount: -amount,
    });
    actions.push(Action::RemovePower {
        target: owner,
        power_id: PowerId::DexterityDown,
    });
    actions
}
