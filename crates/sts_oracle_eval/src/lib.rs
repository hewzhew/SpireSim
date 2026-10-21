//! Combat evaluation and exact-search orchestration above the stable simulator and planner.

extern crate self as sts_simulator;

pub use sts_agent::agent;
pub use sts_core::{content, engine, fixtures, sim, state, test_support, EntityId};

pub mod testing {
    pub use sts_core::fixtures;
    pub use sts_core::test_support as support;
}

pub mod ai {
    pub use sts_combat_atomic::ai::{combat_auto_policy_v1, combat_search_v2};
    pub use sts_run_strategy::ai::*;
}

pub mod runtime {
    pub use sts_core::runtime::{action, combat, event_time, monster_move, rng};
}

#[cfg(test)]
mod semantics {
    pub mod combat {
        pub use crate::runtime::monster_move::*;
    }
}

#[path = "../../../src/eval/mod.rs"]
pub mod eval;

// Compile the build-contract owner's own invariants into the package that
// consumes its emitted IDs. Build scripts are not unit-test targets by
// themselves, so this test-only inclusion keeps the contract executable.
#[cfg(test)]
#[path = "../../../tools/combat_action_imitation_build_contract.rs"]
#[allow(dead_code)]
mod combat_action_imitation_build_contract_tests;
