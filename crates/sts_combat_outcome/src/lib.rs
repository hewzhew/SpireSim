//! Replay-verified, typed outcomes of complete exact combat action lines.
//!
//! This owner contains no search, continuation value, artifact IO, or Python.

pub use sts_agent::agent;
pub use sts_combat_evidence::ai;
pub use sts_core::{content, runtime, sim, state};

#[path = "../../../src/eval/combat_witness_materialization.rs"]
pub mod combat_witness_materialization;
