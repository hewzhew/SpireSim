// Matryoshka: The first 2 chests you open each contain 2 relics.
// This is handled in the treasure room logic, not via combat hooks.
// Uses counter: starts at 2, decremented each chest opened until 0.
// When counter > 0, an extra relic is given from the pool during chest opening.

pub const MECHANICS: &str = "The first 2 non-boss chests you open each add 1 extra relic reward, for 2 relic rewards before other chest effects. The extra relic is Common with 75% probability or Uncommon with 25%. Each opening consumes one charge; the counter shows remaining openings. Boss relic chests do not consume charges or receive an extra relic. A Sapphire Key replaces only its linked base relic; Cursed Key still adds its curse when the chest is opened.";

/// Returns true if Matryoshka should grant an extra relic (counter > 0).
pub fn should_grant_extra_relic(counter: i32) -> bool {
    counter > 0
}
