// Odd Mushroom: Vulnerable now increases damage by 25% instead of 50%.
// This requires a behavior modifier in the `powers::vulnerable` damage calculation logic.

pub const MECHANICS: &str = "While you are Vulnerable, incoming Attack damage is multiplied by 1.25 instead of 1.5 before rounding and Block. Does not prevent or remove Vulnerable, change its duration, or weaken your attacks against Vulnerable enemies.";

pub fn on_calculate_vulnerable_multiplier() -> f32 {
    VULNERABLE_MULTIPLIER
}

pub const VULNERABLE_MULTIPLIER: f32 = 1.25;

pub fn has_odd_mushroom() -> bool {
    true
}
