//! Run-control, exact run evidence, and learning environments over combat evaluation.

extern crate self as sts_simulator;

pub use sts_oracle_eval::{
    agent, ai, content, engine, fixtures, runtime, sim, state, test_support, EntityId,
};

pub mod testing {
    pub use sts_oracle_eval::testing::*;
}

pub mod eval;
