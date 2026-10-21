//! Learned-agent contracts over exact simulator mechanics.
//!
//! This tree owns what a deployed agent may observe and how hidden futures are
//! represented. It does not own run-control artifacts, evaluation verdicts, or
//! training objectives.

pub mod belief;
pub mod information;
pub mod task;
