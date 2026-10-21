use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

/// Hovering Kite: The first time you discard a card each turn, gain 1 Energy.
/// Check hooks `on_discard` during turn, tracking state via relic `used_up` each turn.
pub fn at_turn_start(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    relic_state.used_up = false;
    SmallVec::new()
}

pub fn on_discard(
    _state: &CombatState,
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    if !relic_state.used_up {
        relic_state.used_up = true;
        // Triggers UI effect explicitly in Java, just energy gain in Headless
        actions.push(ActionInfo {
            action: Action::GainEnergy { amount: 1 },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
