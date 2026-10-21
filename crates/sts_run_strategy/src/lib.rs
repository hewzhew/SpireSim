//! Hand-authored run strategy, construction facts, and non-combat policy.
//!
//! This is deliberately a compilation boundary above simulator mechanics and
//! combat evidence. Strategy iteration must not relink those lower owners.

extern crate self as sts_simulator;

pub use sts_core::{content, engine, fixtures, runtime, sim, state, test_support, EntityId};

pub mod ai;

mod falling_owner;
pub mod owner_policy;
