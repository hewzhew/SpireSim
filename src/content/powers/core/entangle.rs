use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        amount_unit: "active restriction",
        effect: "Prevents playing Attack cards while active. Skills and Powers remain playable.",
        timing: "Removed at the end of the affected player's turn.",
        round_duration: false,
    };

pub fn at_end_of_turn(owner: EntityId) -> smallvec::SmallVec<[Action; 2]> {
    // Entangle removes itself at the end of the turn
    smallvec::smallvec![Action::RemovePower {
        target: owner,
        power_id: PowerId::Entangle,
    }]
}
