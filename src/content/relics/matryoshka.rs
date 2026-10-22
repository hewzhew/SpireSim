// Matryoshka: The first 2 chests you open each contain 2 relics.
// This is handled in the treasure room logic, not via combat hooks.
// Uses counter: starts at 2, decremented each chest opened until 0.
// When counter > 0, an extra relic is given from the pool during chest opening.

pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::Matryoshka,
};

/// Returns true if Matryoshka should grant an extra relic (counter > 0).
pub fn should_grant_extra_relic(counter: i32) -> bool {
    counter > 0
}
