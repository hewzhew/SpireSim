// PaperFrog: Enemies with Vulnerable take 75% more damage (instead of 50%).
// This is a passive modifier evaluated in the damage calculation pipeline.
// Java: AbstractCreature.atDamageGive() with Vulnerable checks Paper Phrog
// and uses 1.75x multiplier instead of 1.50x.

/// Returns the Vulnerable damage multiplier when PaperFrog is present.
/// Normal Vulnerable: target takes 150% damage (1.50 multiplier).
/// With PaperFrog: target takes 175% damage (1.75 multiplier).
pub const VULNERABLE_MULTIPLIER: f32 = 1.75;
