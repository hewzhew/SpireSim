mod collector;
mod finish;
mod ratio;

pub(super) const FRONTIER_SAMPLE_LIMIT: usize = 8;

pub(super) use collector::SearchDiagnosticsCollector;
pub(super) use finish::SearchDiagnosticsFinish;

#[cfg(test)]
mod tests;
