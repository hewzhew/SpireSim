use std::fmt;
use std::process::{Child, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Eq, PartialEq)]
pub enum SupervisedChildErrorV1 {
    Poll(String),
    Terminate(String),
    Deadline,
}

impl fmt::Display for SupervisedChildErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poll(detail) => write!(formatter, "failed to poll supervised child: {detail}"),
            Self::Terminate(detail) => {
                write!(formatter, "failed to terminate supervised child: {detail}")
            }
            Self::Deadline => write!(
                formatter,
                "supervised child did not exit before its deadline"
            ),
        }
    }
}

impl std::error::Error for SupervisedChildErrorV1 {}

/// An exact production child handle adopted by the lifecycle Supervisor.
///
/// The caller still decides what the process means. This owner only prevents a
/// live child handle from remaining in semantic Engine state and centralizes
/// the existing tree-termination plus bounded-wait behavior.
#[derive(Debug)]
pub struct SupervisedChildV1 {
    child: Child,
    process_start_token: String,
    closed: bool,
}

impl SupervisedChildV1 {
    pub fn adopt(child: Child) -> Self {
        Self {
            child,
            process_start_token: format!("supervised-process-start-v1-{}", Uuid::new_v4().simple()),
            closed: false,
        }
    }

    pub fn process_id(&self) -> u32 {
        self.child.id()
    }

    /// Opaque identity minted when the exact child handle is adopted.
    ///
    /// Callers must compare this together with the process ID. It deliberately
    /// avoids rediscovering a process by PID or guessing identity from wall time.
    pub fn process_start_token(&self) -> &str {
        &self.process_start_token
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, SupervisedChildErrorV1> {
        self.child
            .try_wait()
            .map_err(|error| SupervisedChildErrorV1::Poll(error.to_string()))
    }

    pub fn shutdown(
        &mut self,
        tree_exit_timeout: Duration,
        forced_exit_timeout: Duration,
    ) -> Result<(), SupervisedChildErrorV1> {
        if self.closed {
            return Ok(());
        }
        if self.try_wait()?.is_some() {
            self.closed = true;
            return Ok(());
        }
        request_process_tree_termination(self.process_id());
        if wait_for_exit(&mut self.child, tree_exit_timeout).is_err() {
            self.child
                .kill()
                .map_err(|error| SupervisedChildErrorV1::Terminate(error.to_string()))?;
            wait_for_exit(&mut self.child, forced_exit_timeout)?;
        }
        self.closed = true;
        Ok(())
    }
}

impl Drop for SupervisedChildV1 {
    fn drop(&mut self) {
        let _ = self.shutdown(Duration::from_secs(10), Duration::from_secs(5));
    }
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<(), SupervisedChildErrorV1> {
    let deadline = Instant::now() + timeout;
    loop {
        if child
            .try_wait()
            .map_err(|error| SupervisedChildErrorV1::Poll(error.to_string()))?
            .is_some()
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(SupervisedChildErrorV1::Deadline);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(windows)]
fn request_process_tree_termination(process_id: u32) {
    use std::process::{Command, Stdio};

    let _ = Command::new("taskkill.exe")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .arg("/PID")
        .arg(process_id.to_string())
        .arg("/T")
        .arg("/F")
        .status();
}

#[cfg(not(windows))]
fn request_process_tree_termination(_process_id: u32) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn already_exited_child_closes_without_targeting_a_reused_process_id() {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("cmd.exe");
            command.args(["/D", "/C", "exit", "0"]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", "exit 0"]);
            command
        };
        let mut child = SupervisedChildV1::adopt(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        child
            .shutdown(Duration::from_millis(10), Duration::from_millis(10))
            .unwrap();
        assert!(child.closed);
    }

    #[test]
    fn running_child_is_reaped_and_repeated_shutdown_is_idempotent() {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("cmd.exe");
            command.args(["/D", "/C", "ping", "-n", "30", "127.0.0.1"]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 30"]);
            command
        };
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        let mut child = SupervisedChildV1::adopt(command.spawn().unwrap());
        assert!(child.process_id() > 0);

        child
            .shutdown(Duration::from_secs(5), Duration::from_secs(5))
            .unwrap();
        child
            .shutdown(Duration::from_millis(1), Duration::from_millis(1))
            .unwrap();
        assert!(child.closed);
    }
}
