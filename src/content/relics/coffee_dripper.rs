use crate::content::relics::RelicState;
use crate::runtime::combat::CombatState;

// Note: Coffee Dripper is primarily an out-of-combat relic.
// It grants +1 Energy globally (via `on_equip`) and prevents Resting at Campfires (via `RunState` block).

pub fn on_equip(_state: &mut CombatState, _relic: &mut RelicState) {
    // Left empty for future RunState out-of-loop implementation.
}
