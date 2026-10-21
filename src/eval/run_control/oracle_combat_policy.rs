//! Shared exact-planner policy surface.
//!
//! Complete-line advisors and external lookahead proposals are deliberately
//! absent: they could influence production stopping without owning the exact
//! contract frontier. The maintained run-control surface exposes only typed
//! action and state guidance to the exact planners.

pub use sts_combat_atomic::retained_combat_tactical_policy_v1;
pub(super) use sts_combat_atomic::RetainedCombatTacticalPolicyV1;
pub use sts_combat_knowledge::{
    authorized_potion_trial_policy_v1, darkling_survival_guide_service_bias_v1,
};
