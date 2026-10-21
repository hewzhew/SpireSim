// These ranks only decide child-generation order inside the same legal action set.
// They never merge, prune, or claim that two actions are equivalent.
pub(super) const ROLE_LETHAL_CARD: i32 = 130;
pub(super) const ROLE_PREVENT_VISIBLE_LETHAL: i32 = 120;
// A nonlethal incoming attack is exactly when two-or-more-card access can find
// the rest of the defense while there is still energy to use it.  Keep this
// below already-proven lethal prevention, but above committing mitigation or
// one known block card before seeing the additional hand information.
pub(super) const ROLE_DEFENSIVE_ACTION_SUPPLY: i32 = 105;
pub(super) const ROLE_SUSTAINED_MITIGATION: i32 = 95;
pub(super) const ROLE_KEY_SETUP_CARD: i32 = 90;
pub(super) const ROLE_TACTICAL_POTION_BASE: i32 = 60;
pub(super) const ROLE_PREVENT_HP_LOSS: i32 = 85;
// This role is emitted only after exact order projection proves that the
// defensive card and a retaliation-triggering attack are both payable and
// that defense-first preserves more HP. Keep it ahead of the attack's lethal
// label so target death does not erase the already-projected retaliation.
pub(super) const ROLE_CURRENT_TURN_RETALIATION_PROTECTION: i32 = ROLE_LETHAL_CARD + 1;
pub(super) const ROLE_CURRENT_TURN_ATTACK_SETUP: i32 = 80;
pub(super) const ROLE_DEFERRED_SETUP: i32 = 75;
pub(super) const ROLE_IMMEDIATE_ACTION_SUPPLY: i32 = 70;
pub(super) const ROLE_DAMAGE_PROGRESS: i32 = 60;
pub(super) const ROLE_REACTIVE_RISK_PREVENT_HP_LOSS: i32 = 55;
pub(super) const ROLE_BLOCK: i32 = 45;
pub(super) const ROLE_UTILITY_PLAY: i32 = 35;
pub(super) const ROLE_END_TURN: i32 = 0;
pub(super) const ROLE_PENDING_VALUE_SELECTION: i32 = 70;
pub(super) const ROLE_PENDING_REMOVAL_SELECTION: i32 = 65;
pub(super) const ROLE_PENDING_NEUTRAL_SELECTION: i32 = 20;
pub(super) const ROLE_PENDING_CANCEL: i32 = -10;
pub(super) const ROLE_DISCARD_POTION: i32 = -20;
