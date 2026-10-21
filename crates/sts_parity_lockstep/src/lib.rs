//! Transport-independent Java/Rust committed-trajectory lockstep core.
//!
//! This crate owns orchestration contracts only. It does not launch Java,
//! search Rust branches, select strategy, or expose an MCP surface.

mod action;
mod action_space;
mod action_view;
mod bootstrap;
mod combat;
mod comparison;
mod execution_inputs;
mod journal;
mod match_game;
mod relics;
mod rewards;
mod rng;
mod run_state;
mod runner;
mod shop;
mod snapshot;
mod timing;

pub use action::*;
pub use action_space::*;
pub use action_view::*;
pub use bootstrap::*;
pub use combat::*;
pub use comparison::*;
pub use execution_inputs::*;
pub use journal::*;
pub use match_game::*;
pub use relics::*;
pub use rewards::*;
pub use rng::*;
pub use run_state::*;
pub use runner::*;
pub use shop::*;
pub use snapshot::*;
pub use timing::*;
