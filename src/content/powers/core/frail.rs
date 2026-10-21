pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "remaining rounds",
    effect: "Reduces Block gained from cards by 25%, rounded down. More stacks extend duration, not the reduction.",
    timing: "Decreases by 1 at end of round, unless the newly-applied flag skips that tick.",
    round_duration: true,
};

use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

pub fn on_calculate_block(mut block: f32, amount: i32) -> f32 {
    if amount > 0 {
        block = (block * 0.75).floor();
    }
    block
}

pub fn at_end_of_round(
    owner: EntityId,
    amount: i32,
    just_applied: bool,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];
    if just_applied {
        return actions;
    }
    if amount == 0 {
        actions.push(Action::RemovePower {
            target: owner,
            power_id: PowerId::Frail,
        });
    } else {
        actions.push(Action::ReducePower {
            target: owner,
            power_id: PowerId::Frail,
            amount: 1,
        });
    }
    actions
}
