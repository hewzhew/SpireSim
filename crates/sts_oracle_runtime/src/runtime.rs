//! Runtime state plus the higher-level branch/oracle control surface.

pub use sts_core::runtime::{action, combat, monster_move, rng};

#[path = "../../../src/runtime/branch/mod.rs"]
pub mod branch;
