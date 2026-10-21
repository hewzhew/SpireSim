use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) fn run_bounded(mut command: Command, deadline: Duration) -> Result<Output, String> {
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start typed workbench subprocess: {error}"))?;
    let started = Instant::now();
    loop {
        match child
            .try_wait()
            .map_err(|error| format!("failed to poll typed workbench subprocess: {error}"))?
        {
            Some(_) => {
                return child.wait_with_output().map_err(|error| {
                    format!("failed to collect typed workbench subprocess: {error}")
                });
            }
            None if started.elapsed() < deadline => thread::sleep(Duration::from_millis(10)),
            None => {
                let kill_error = child.kill().err();
                let output = child.wait_with_output().map_err(|error| {
                    format!("timed-out workbench subprocess could not be collected: {error}")
                })?;
                let suffix = kill_error
                    .map(|error| format!("; termination also failed: {error}"))
                    .unwrap_or_default();
                return Err(format!(
                    "typed workbench subprocess timed out after {} seconds{suffix}: {}",
                    deadline.as_secs(),
                    failure_detail(&output.stdout, &output.stderr)
                ));
            }
        }
    }
}

pub(crate) fn failure_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let detail = if stderr.is_empty() { stdout } else { stderr };
    let text = String::from_utf8_lossy(detail);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        "no process detail".to_owned()
    } else {
        trimmed.chars().take(2_000).collect()
    }
}
