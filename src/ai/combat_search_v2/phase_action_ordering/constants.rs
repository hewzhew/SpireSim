// Kept smaller than the main role gaps in action_priority; phase facts nudge nearby
// ordering decisions without turning this module into an alternate policy.
pub(super) const PHASE_ROLE_ADJUSTMENT: i32 = 12;
pub(super) const AWAKENED_POWER_PENALTY: i32 = PHASE_ROLE_ADJUSTMENT * 2;
pub(super) const TIME_EATER_CLOCK_PENALTY: i32 = PHASE_ROLE_ADJUSTMENT;
// Pure nonlethal damage during Haste is healed back to half HP. This is a
// stronger semantic distinction than ordinary clock nudging: it should sit
// behind block/setup/access actions while remaining legal and searchable.
pub(super) const TIME_EATER_HASTE_WASTE_PENALTY: i32 = PHASE_ROLE_ADJUSTMENT * 3;
pub(super) const STASIS_TARGET_SETUP_MAX: i32 = 20;
pub(super) const AWAKENED_STRENGTH_TRANSITION_SETUP_BONUS: i32 = 36;
