use crate::runtime::action::Action;
use smallvec::SmallVec;

pub const MECHANICS: crate::content::powers::mechanics::PowerMechanics =
    crate::content::powers::mechanics::PowerMechanics {
        amount_unit: "accumulated Block to gain at the owner's next turn start",
        effect: "At the owner's next turn start, gain Block equal to amount, then remove this power. Multiple applications add to the same pending amount. The delayed Block does not protect during the turn when this power is applied.",
        timing: "Owner turn-start power hook, after normal Block clearing; consumed in one trigger.",
        round_duration: false,
    };

/// Next Turn Block: At the start of your next turn, gain X Block. (Used by Self-Forming Clay, Dodge and Roll, etc.)
pub fn at_turn_start(entity_id: crate::EntityId, amount: i32) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();

    // Gain block
    actions.push(Action::GainBlock {
        target: entity_id,
        amount,
    });

    // Remove the power
    actions.push(Action::RemovePower {
        target: entity_id,
        power_id: crate::content::powers::PowerId::NextTurnBlock,
    });

    actions
}
