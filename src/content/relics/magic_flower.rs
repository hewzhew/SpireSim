/// Magic Flower: Healing is 50% more effective during combat.
/// Handled statically inside engine HP modifications (`Heal` action).
/// This hook is essentially a no-op but acts as documentation. (A function could be exported to wrap heals).
pub const MECHANICS: &str = "Healing during combat is 50% more effective, rounded to the nearest whole HP and capped by missing HP. This includes Reaper, healing potions used in combat, and combat-end healing from Burning Blood (6 becomes 9) or Black Blood (12 becomes 18). Does not increase out-of-combat recovery such as campfire Rest or Eternal Feather.";

pub fn modify_heal(amount: i32) -> i32 {
    (amount as f32 * 1.5).round() as i32
}
