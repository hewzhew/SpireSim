use crate::content::relics::RelicState;
use crate::runtime::combat::CombatState;

// Note: Cultist Mask is purely a cosmetic/UI relic ("CAW CAW!").
// Since the rust engine is headless, there is no UI logic to fire.

pub fn on_equip(_state: &mut CombatState, _relic: &mut RelicState) {
    // SQAWK!
}
