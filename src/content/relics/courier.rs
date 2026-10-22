use crate::content::relics::RelicState;
use crate::runtime::combat::CombatState;

// Note: The Courier is strictly an out-of-combat relic.
// It discounts Shop prices by 20% and instantly replenishes purchased items.

pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::Courier,
};

pub fn on_equip(_state: &mut CombatState, _relic: &mut RelicState) {
    // Handled purely in RunState Shop Logic.
}
