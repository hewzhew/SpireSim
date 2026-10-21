use serde::{Deserialize, Serialize};

use super::RunProgressStepV1;

pub const RUN_PROGRESS_JOURNAL_SCHEMA_NAME: &str = "RunProgressJournal";
pub const RUN_PROGRESS_JOURNAL_SCHEMA_VERSION: u32 = 1;

/// One ordered segment of committed run progress produced by a bounded driver.
///
/// Stops are deliberately kept outside the journal: they describe why a drive
/// yielded, while journal entries describe mutations that actually committed.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunProgressJournalV1 {
    schema_name: String,
    schema_version: u32,
    entries: Vec<RunProgressStepV1>,
}

impl Default for RunProgressJournalV1 {
    fn default() -> Self {
        Self {
            schema_name: RUN_PROGRESS_JOURNAL_SCHEMA_NAME.to_string(),
            schema_version: RUN_PROGRESS_JOURNAL_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

impl RunProgressJournalV1 {
    pub fn from_committed_steps(entries: Vec<RunProgressStepV1>) -> Result<Self, String> {
        let mut journal = Self::default();
        journal.append_committed_steps(entries)?;
        Ok(journal)
    }

    pub fn append_committed_steps(
        &mut self,
        entries: impl IntoIterator<Item = RunProgressStepV1>,
    ) -> Result<(), String> {
        let entries = entries.into_iter().collect::<Vec<_>>();
        if entries
            .iter()
            .any(|entry| matches!(entry, RunProgressStepV1::Stop(_)))
        {
            return Err("run progress journal cannot contain stop records".to_string());
        }
        self.entries.extend(entries);
        Ok(())
    }

    pub fn append(&mut self, other: Self) -> Result<(), String> {
        self.append_committed_steps(other.entries)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[RunProgressStepV1] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::run_control::{RunControlAutoStopKind, RunControlAutoStopV1};

    #[test]
    fn journal_rejects_stop_records() {
        let result = RunProgressJournalV1::from_committed_steps(vec![RunProgressStepV1::Stop(
            RunControlAutoStopV1 {
                kind: RunControlAutoStopKind::HumanBoundary,
                reason: "test".to_string(),
                applied_operations: 0,
            },
        )]);

        assert_eq!(
            result.unwrap_err(),
            "run progress journal cannot contain stop records"
        );
    }
}
