use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

/// Face Of Cleric
/// Event Relic
/// Gain 1 Max HP after each combat.
pub fn on_victory(_state: &CombatState, _relic: &mut RelicState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    // In Java: AbstractDungeon.player.increaseMaxHp(1, true);
    actions.push(ActionInfo {
        action: Action::GainMaxHp { amount: 1 },
        insertion_mode: AddTo::Bottom,
    });
    actions
}
