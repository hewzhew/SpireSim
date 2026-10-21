use crate::runtime::action::Action;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "presence; not a numeric stack count",
    effect: "Prevents further card draw while active, including card and power triggers such as Dark Embrace and Evolve. It does not prevent adding generated cards to Hand or moving cards into Hand without drawing.",
    timing: "Removed at the owner's turn end. Battle Trance draws its cards before applying this power.",
    round_duration: false,
};

pub fn at_end_of_turn(owner: EntityId) -> smallvec::SmallVec<[Action; 2]> {
    smallvec::smallvec![Action::RemovePower {
        target: owner,
        power_id: crate::content::powers::PowerId::NoDraw,
    }]
}
