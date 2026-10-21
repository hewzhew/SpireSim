//! Exact single-episode learning environments and opaque root artifacts.

extern crate self as sts_simulator;

pub use sts_oracle_run_control::{
    agent, ai, content, engine, fixtures, runtime, sim, state, test_support, EntityId,
};

pub mod testing {
    pub use sts_oracle_run_control::testing::*;
}

pub mod eval;
