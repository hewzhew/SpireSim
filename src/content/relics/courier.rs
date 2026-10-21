use crate::content::relics::RelicState;
use crate::runtime::combat::CombatState;

// Note: The Courier is strictly an out-of-combat relic.
// It discounts Shop prices by 20% and instantly replenishes purchased items.

pub const MECHANICS: &str = "Shop card, relic and potion prices are 20% lower. Card removal is also discounted unless another relic overrides its price; use the displayed price after all modifiers. Purchased cards, relics and potions are immediately replaced by new stock. Inspect the returned stock and prices after each purchase. Card removal remains limited to once per shop.";

pub fn on_equip(_state: &mut CombatState, _relic: &mut RelicState) {
    // Handled purely in RunState Shop Logic.
}
