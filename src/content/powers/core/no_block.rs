use crate::runtime::action::Action;
use crate::runtime::combat::PowerId;
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        amount_unit: "remaining round-end ticks of card Block prevention",
        effect: "Sets calculated Block from cards to 0. Block gained by powers and relics, such as Feel No Pain and Metallicize, still applies. Existing Block is not removed by application. Artifact can block this debuff.",
        timing: "Decreases by 1 at each round end, including the round it was applied. Panic Button applies 2, affecting further card Block this turn and next turn.",
        round_duration: true,
    };

pub fn at_end_of_round(
    owner: EntityId,
    amount: i32,
    just_applied: bool,
) -> smallvec::SmallVec<[Action; 2]> {
    if just_applied {
        return smallvec::smallvec![];
    }
    if amount == 0 {
        smallvec::smallvec![Action::RemovePower {
            target: owner,
            power_id: PowerId::NoBlock,
        }]
    } else {
        smallvec::smallvec![Action::ReducePower {
            target: owner,
            power_id: PowerId::NoBlock,
            amount: 1,
        }]
    }
}

pub fn on_calculate_block(_block: f32, _amount: i32) -> f32 {
    0.0
}
