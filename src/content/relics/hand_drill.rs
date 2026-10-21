use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

/// Hand Drill: Whenever you break an enemy's Block, apply 2 Vulnerable.
/// Check if the engine resolves "break" specifically in damage routing, or if we need an Observer.
/// For now, we stub the `on_break_block` hook in `hooks.rs` which would be called from `apply_damage`.
pub fn on_break_block(
    _state: &CombatState,
    target_id: crate::EntityId,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: target_id,
            power_id: crate::content::powers::PowerId::Vulnerable,
            amount: 2,
        },
        insertion_mode: AddTo::Bottom,
    });
    actions
}
