pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "cards subtracted from the normal start-of-turn draw",
    effect: "Reduce the owner's normal turn-start draw by amount. Does not reduce card-effect draws such as Pommel Strike or Shrug It Off.",
    timing: "At end of round, reduce amount by 1; skip the first decrement while newly applied, so the next normal draw is reduced.",
    round_duration: false,
};

use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

pub fn at_end_of_round(
    owner: EntityId,
    _amount: i32,
    just_applied: bool,
) -> smallvec::SmallVec<[Action; 2]> {
    if just_applied {
        return smallvec::smallvec![];
    }

    smallvec::smallvec![Action::ReducePower {
        target: owner,
        power_id: PowerId::DrawReduction,
        amount: 1,
    }]
}
