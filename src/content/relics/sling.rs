use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;
pub const ELITE_STRENGTH_AMOUNT: i32 = 2;

/// Sling: at battle start, gain 2 Strength only in elite combats.
pub fn at_battle_start(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    if !super::mechanics::RelicBattleStartScopeV1::EliteCombat.applies(state.meta.is_elite_fight) {
        return actions;
    }

    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Strength,
            amount: ELITE_STRENGTH_AMOUNT,
        },
        insertion_mode: AddTo::Top, // Java: addToTop
    });
    actions
}
