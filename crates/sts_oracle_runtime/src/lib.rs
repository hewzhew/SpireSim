//! Branch execution and persistence over oracle evaluation and run-control.

extern crate self as sts_simulator;

pub use sts_oracle_run_control::{
    agent, ai, content, engine, eval, fixtures, sim, state, test_support, EntityId,
};

pub mod testing {
    pub use sts_oracle_run_control::testing::*;
}

pub mod runtime;
