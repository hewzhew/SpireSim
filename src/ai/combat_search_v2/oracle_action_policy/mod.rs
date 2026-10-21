//! Tactical action priors, state guides, and bounded rollout contracts.
//!
//! Shared policy evaluation is kept separate from solver-owned rollout
//! execution so it can be migrated behind a neutral dependency boundary
//! without moving the AtomicExactV2 search kernel with it.

mod rollout;
mod shared;

pub use rollout::{
    oracle_combat_rollout_guide_v1, OracleCombatRolloutContractV1, OracleCombatRolloutGuideV1,
    OracleCombatRolloutWinningSuffixV1,
};
pub use shared::*;
