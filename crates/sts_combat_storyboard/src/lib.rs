//! Replay-only, typed explanations of exact combat action lines.
//!
//! This owner deliberately has no search, run-control, Python, or artifact IO.

pub use sts_core::{content, runtime, sim, state, test_support, EntityId};

#[path = "../../../src/eval/combat_witness_storyboard.rs"]
pub mod combat_witness_storyboard;
