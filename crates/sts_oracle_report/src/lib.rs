//! Dependency-light readers for durable oracle evidence.
//!
//! This crate never restores simulator state or runs search. It validates one
//! known report schema and projects only fields required for compact,
//! repeatable inspection.

mod census;
mod resident_corridor;

pub use census::*;
pub use resident_corridor::*;

#[cfg(test)]
mod tests;
