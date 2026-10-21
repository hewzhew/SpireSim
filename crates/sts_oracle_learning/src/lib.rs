//! Exact learning environments and model-facing projections over run-control.

extern crate self as sts_simulator;

pub use sts_oracle_learning_env::{
    agent, ai, content, engine, fixtures, runtime, sim, state, test_support, EntityId,
};

pub mod testing {
    pub use sts_oracle_learning_env::testing::*;
}

pub mod eval;
