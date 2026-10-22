use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub const BLOCK_PER_HP_LOSS: i32 = 3;
pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::SelfFormingClay,
};

/// Self-Forming Clay: Whenever you lose HP, gain 3 Block next turn.
pub fn on_lose_hp(damage_amount: i32) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    if damage_amount <= 0 {
        return actions;
    }

    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::NextTurnBlock,
            amount: BLOCK_PER_HP_LOSS,
        },
        insertion_mode: AddTo::Top,
    });

    actions
}
