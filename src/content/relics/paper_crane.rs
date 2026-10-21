// PaperCrane: Enemies deal 40% less damage when you have Weak (instead of 25%).
// This is a passive modifier evaluated in the damage calculation pipeline.
// Java: AbstractPlayer.atDamageReceive() checks for Paper Krane and overrides
// the Weak damage reduction from 25% to 40%.

/// Returns the Weak damage multiplier when PaperCrane is present.
/// Normal Weak: attacker deals 75% damage (0.75 multiplier).
/// With PaperCrane: attacker deals 60% damage (0.60 multiplier).
pub const WEAK_MULTIPLIER: f32 = 0.60;
