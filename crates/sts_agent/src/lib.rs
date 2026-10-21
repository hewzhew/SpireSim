//! Public-information and belief mechanics for the learned agent.
//!
//! This package is deliberately separate from the simulator core so agent
//! iteration does not relink the complete mechanics test harness or invalidate
//! unrelated search and run-control owners.

extern crate self as sts_simulator;

pub use sts_core::{ai, content, engine, fixtures, runtime, sim, state, test_support, EntityId};

#[path = "../../../src/agent/mod.rs"]
pub mod agent;
