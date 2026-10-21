use crate::content::relics::RelicState;
use crate::runtime::combat::CombatState;

// Note: Cursed Key is primarily an out-of-combat relic.
// It grants +1 Energy globally (via `on_equip`) and forces a Curse drop into the deck
// whenever a non-Boss Chest is opened (via `RunState` block).

pub fn on_equip(_state: &mut CombatState, _relic: &mut RelicState) {
    // Left empty for future RunState out-of-loop implementation.
}
