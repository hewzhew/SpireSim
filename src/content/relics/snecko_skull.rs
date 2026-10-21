use crate::runtime::action::ActionInfo;
// use crate::content::powers::PowerId;
use smallvec::SmallVec;

/// Snecko Skull: Whenever you apply Poison, apply 1 additional Poison.
pub fn on_apply_power(_power_id: crate::content::powers::PowerId) -> SmallVec<[ActionInfo; 4]> {
    // Note: In Slay the Spire, Snecko Skull operates via scattered engine logic
    // rather than an on-apply post-hook (which risks infinite loops).
    // The implementation for this (+1 Poison amount mutation) is natively
    // integrated inside `src/engine/action_handlers.rs` within `Action::ApplyPower`.
    // This blank implementation serves as a placeholder for the Relic framework.
    SmallVec::new()
}
