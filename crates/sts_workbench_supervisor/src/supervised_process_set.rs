use crate::{
    SupervisedChildErrorV1, SupervisedChildV1, SupervisedProcessErrorV1, SupervisedProcessV1,
};
use std::fmt;
use std::time::Duration;

#[derive(Debug)]
pub enum SupervisedProcessSetErrorV1 {
    Closed,
    PrimaryExited,
    CompanionRequired,
    CompanionAlreadyAdmitted,
    CompanionExited,
    Primary(SupervisedChildErrorV1),
    Companion(SupervisedProcessErrorV1),
    Shutdown {
        primary: Option<String>,
        companion: Option<String>,
    },
}

impl fmt::Display for SupervisedProcessSetErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(formatter, "supervised process set is closed"),
            Self::PrimaryExited => write!(formatter, "supervised primary process exited"),
            Self::CompanionRequired => write!(formatter, "supervised companion is not admitted"),
            Self::CompanionAlreadyAdmitted => {
                write!(formatter, "supervised companion is already admitted")
            }
            Self::CompanionExited => write!(formatter, "supervised companion process exited"),
            Self::Primary(error) => write!(formatter, "primary process failure: {error}"),
            Self::Companion(error) => write!(formatter, "companion process failure: {error}"),
            Self::Shutdown { primary, companion } => write!(
                formatter,
                "process-set shutdown failed (primary={}, companion={})",
                primary.as_deref().unwrap_or("none"),
                companion.as_deref().unwrap_or("none")
            ),
        }
    }
}

impl std::error::Error for SupervisedProcessSetErrorV1 {}

/// Exact process ownership for one external runtime.
///
/// The primary process is a directly spawned child whose descendants are
/// terminated as a tree. The companion is an indirectly spawned leaf that was
/// separately admitted by exact OS handle. This type owns only their liveness
/// and shutdown ordering; protocol sessions and semantic runtime state remain
/// outside it.
#[derive(Debug)]
pub struct SupervisedProcessSetV1 {
    primary: SupervisedChildV1,
    companion: Option<SupervisedProcessV1>,
    closed: bool,
}

impl SupervisedProcessSetV1 {
    pub fn new(primary: SupervisedChildV1) -> Self {
        Self {
            primary,
            companion: None,
            closed: false,
        }
    }

    pub fn admit_companion(
        &mut self,
        companion: SupervisedProcessV1,
    ) -> Result<(), SupervisedProcessSetErrorV1> {
        if self.closed {
            return Err(SupervisedProcessSetErrorV1::Closed);
        }
        if self.companion.is_some() {
            return Err(SupervisedProcessSetErrorV1::CompanionAlreadyAdmitted);
        }
        self.companion = Some(companion);
        Ok(())
    }

    pub fn primary_process_id(&self) -> u32 {
        self.primary.process_id()
    }

    pub fn primary_process_start_token(&self) -> &str {
        self.primary.process_start_token()
    }

    pub fn companion_process_id(&self) -> Option<u32> {
        self.companion.as_ref().map(SupervisedProcessV1::process_id)
    }

    pub fn validate_primary_running(&mut self) -> Result<(), SupervisedProcessSetErrorV1> {
        if self.closed {
            return Err(SupervisedProcessSetErrorV1::Closed);
        }
        if self
            .primary
            .try_wait()
            .map_err(SupervisedProcessSetErrorV1::Primary)?
            .is_some()
        {
            return Err(SupervisedProcessSetErrorV1::PrimaryExited);
        }
        Ok(())
    }

    pub fn validate_running(&mut self) -> Result<(), SupervisedProcessSetErrorV1> {
        self.validate_primary_running()?;
        let companion = self
            .companion
            .as_ref()
            .ok_or(SupervisedProcessSetErrorV1::CompanionRequired)?;
        if !companion
            .is_running()
            .map_err(SupervisedProcessSetErrorV1::Companion)?
        {
            return Err(SupervisedProcessSetErrorV1::CompanionExited);
        }
        Ok(())
    }

    pub fn shutdown(
        &mut self,
        primary_tree_timeout: Duration,
        primary_forced_timeout: Duration,
        companion_timeout: Duration,
    ) -> Result<(), SupervisedProcessSetErrorV1> {
        if self.closed {
            return Ok(());
        }
        let primary = self
            .primary
            .shutdown(primary_tree_timeout, primary_forced_timeout)
            .err()
            .map(|error| error.to_string());
        let companion = self
            .companion
            .as_mut()
            .and_then(|companion| companion.shutdown(companion_timeout).err())
            .map(|error| error.to_string());
        if primary.is_some() || companion.is_some() {
            return Err(SupervisedProcessSetErrorV1::Shutdown { primary, companion });
        }
        self.closed = true;
        Ok(())
    }
}

impl Drop for SupervisedProcessSetV1 {
    fn drop(&mut self) {
        let _ = self.shutdown(
            Duration::from_secs(10),
            Duration::from_secs(5),
            Duration::from_secs(5),
        );
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::*;

    #[cfg(windows)]
    #[test]
    fn process_set_validates_and_reaps_both_members_idempotently() {
        use crate::ExistingProcessCandidateV1;
        use std::path::PathBuf;
        use std::process::{Command, Stdio};

        let executable = PathBuf::from(std::env::var_os("COMSPEC").unwrap());
        let spawn = || {
            Command::new(&executable)
                .args(["/D", "/C", "ping", "-n", "30", "127.0.0.1"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap()
        };
        let primary = spawn();
        let mut companion = spawn();
        let companion_handle =
            ExistingProcessCandidateV1::open_running(companion.id(), &executable)
                .unwrap()
                .admit();
        let mut processes = SupervisedProcessSetV1::new(SupervisedChildV1::adopt(primary));
        assert_eq!(
            processes.validate_running().unwrap_err().to_string(),
            "supervised companion is not admitted"
        );
        processes.admit_companion(companion_handle).unwrap();

        processes.validate_running().unwrap();
        processes
            .shutdown(
                Duration::from_secs(5),
                Duration::from_secs(5),
                Duration::from_secs(5),
            )
            .unwrap();
        processes
            .shutdown(
                Duration::from_millis(1),
                Duration::from_millis(1),
                Duration::from_millis(1),
            )
            .unwrap();
        companion.wait().unwrap();
    }
}
