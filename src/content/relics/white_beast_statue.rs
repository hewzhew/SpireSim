// White Beast Statue: Potions always drop after combat.
// Evaluated out of combat during reward generation.

/// Overrides the room's base chance before the reward-entry size gate.
pub const POTION_DROP_CHANCE_OVERRIDE_PERCENT: i32 = 100;

pub fn has_white_beast_statue() -> bool {
    true
}
