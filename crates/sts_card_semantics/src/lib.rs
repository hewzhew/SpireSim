//! Typed card mechanics and upgrade-sensitive semantic projections.
//!
//! This crate sits above the exact simulator and below combat and run policy.
//! Changing card interpretation must not invalidate the simulator mechanics
//! crate itself.

extern crate self as sts_simulator;

pub use sts_core::{content, engine, fixtures, runtime, sim, state, test_support, EntityId};

pub mod ai;
