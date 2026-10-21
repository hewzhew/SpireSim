use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub const BLOCK_PER_HP_LOSS: i32 = 3;
pub const MECHANICS: &str = "Whenever you lose a positive amount of HP in combat, add 3 Next Turn Block per HP-loss event, regardless of the amount lost. Gain the accumulated Block at the start of your next turn, then remove Next Turn Block. Separate damaging hits and self HP loss can each trigger it; fully blocked hits do not. This delayed Block does not protect against the triggering loss or the remaining hits of that enemy turn.";

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
