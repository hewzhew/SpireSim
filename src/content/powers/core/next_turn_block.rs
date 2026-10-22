use crate::runtime::action::Action;
use smallvec::SmallVec;

pub const MECHANICS: crate::content::powers::mechanics::PowerMechanics =
    crate::content::powers::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::NextTurnBlock,
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
