#[cfg(test)]
use super::*;

mod compression;
mod diagnostics;
mod keys;
mod types;

pub(super) use compression::compress_equivalent_actions;
pub(super) use diagnostics::ActionEquivalenceDiagnosticsCollector;
use keys::{ActionEquivalenceKey, ActionEquivalenceKind};
use types::ActionEquivalenceGroupSummary;
pub(super) use types::ActionEquivalenceSummary;

#[cfg(test)]
mod tests;
