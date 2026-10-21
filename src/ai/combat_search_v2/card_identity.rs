mod collector;
mod report;
mod summary;

pub(super) use collector::CardIdentityDiagnosticsCollector;
pub(super) use summary::{summarize_card_identity, CardIdentitySummary};

#[cfg(test)]
mod tests;
