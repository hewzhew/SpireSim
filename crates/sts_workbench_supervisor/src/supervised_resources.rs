use crate::{
    OwnedStorageErrorV1, OwnedStorageV1, SupervisedProcessSetErrorV1, SupervisedProcessSetV1,
    SupervisedProcessV1,
};
use std::fmt;
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug)]
pub enum SupervisedResourcesErrorV1 {
    Closed,
    Processes(SupervisedProcessSetErrorV1),
    Storage(OwnedStorageErrorV1),
}

impl fmt::Display for SupervisedResourcesErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(formatter, "supervised runtime resources are closed"),
            Self::Processes(error) => write!(formatter, "runtime process failure: {error}"),
            Self::Storage(error) => write!(formatter, "runtime storage failure: {error}"),
        }
    }
}

impl std::error::Error for SupervisedResourcesErrorV1 {}

/// Process and disposable-storage ownership for one external runtime.
///
/// Storage is removed only after every supervised process has been proven
/// stopped. If process shutdown is uncertain, the marker-owned directory is
/// retained for reconciliation by the next repository lease holder.
#[derive(Debug)]
pub struct SupervisedResourcesV1 {
    processes: SupervisedProcessSetV1,
    storage: OwnedStorageV1,
    closed: bool,
}

impl SupervisedResourcesV1 {
    pub fn new(processes: SupervisedProcessSetV1, storage: OwnedStorageV1) -> Self {
        Self {
            processes,
            storage,
            closed: false,
        }
    }

    pub fn storage_root(&self) -> &Path {
        self.storage.root()
    }

    pub fn source_game_directory(&self) -> &Path {
        self.storage.source_game_directory()
    }

    pub fn isolation_id(&self) -> Uuid {
        self.storage.isolation_id()
    }

    pub fn primary_process_id(&self) -> u32 {
        self.processes.primary_process_id()
    }

    pub fn primary_process_start_token(&self) -> &str {
        self.processes.primary_process_start_token()
    }

    pub fn companion_process_id(&self) -> Option<u32> {
        self.processes.companion_process_id()
    }

    pub fn admit_companion(
        &mut self,
        companion: SupervisedProcessV1,
    ) -> Result<(), SupervisedResourcesErrorV1> {
        self.processes
            .admit_companion(companion)
            .map_err(SupervisedResourcesErrorV1::Processes)
    }

    pub fn validate_primary_running(&mut self) -> Result<(), SupervisedResourcesErrorV1> {
        if self.closed {
            return Err(SupervisedResourcesErrorV1::Closed);
        }
        self.processes
            .validate_primary_running()
            .map_err(SupervisedResourcesErrorV1::Processes)
    }

    pub fn validate_running(&mut self) -> Result<(), SupervisedResourcesErrorV1> {
        if self.closed {
            return Err(SupervisedResourcesErrorV1::Closed);
        }
        self.processes
            .validate_running()
            .map_err(SupervisedResourcesErrorV1::Processes)
    }

    pub fn shutdown(
        &mut self,
        primary_tree_timeout: Duration,
        primary_forced_timeout: Duration,
        companion_timeout: Duration,
    ) -> Result<(), SupervisedResourcesErrorV1> {
        if self.closed {
            return Ok(());
        }
        if let Err(error) = self.processes.shutdown(
            primary_tree_timeout,
            primary_forced_timeout,
            companion_timeout,
        ) {
            self.storage.retain_for_reconciliation();
            return Err(SupervisedResourcesErrorV1::Processes(error));
        }
        self.storage
            .cleanup()
            .map_err(SupervisedResourcesErrorV1::Storage)?;
        self.closed = true;
        Ok(())
    }
}

impl Drop for SupervisedResourcesV1 {
    fn drop(&mut self) {
        let _ = self.shutdown(
            Duration::from_secs(10),
            Duration::from_secs(5),
            Duration::from_secs(5),
        );
    }
}
