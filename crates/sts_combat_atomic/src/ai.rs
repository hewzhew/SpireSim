//! AtomicExactV2 implementation namespace and its required semantic contracts.

pub use sts_card_semantics::ai::{analysis, card_semantics_v1, combat_upgrade_coverage_v1};
pub use sts_combat_evidence::ai::{combat_persistent_outcome_v1, combat_witness_contract};
pub use sts_core::ai::{combat_state_key, decision_tags_v1, planner_core};

#[path = "../../../src/ai/combat_auto_policy_v1/mod.rs"]
pub mod combat_auto_policy_v1;
#[path = "../../../src/ai/combat_search_v2/mod.rs"]
pub mod combat_search_v2;
