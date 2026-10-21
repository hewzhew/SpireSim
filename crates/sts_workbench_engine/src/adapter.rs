use crate::subprocess::{failure_detail, run_bounded};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REBUILD_COMMAND: &str = ".\\workbench.ps1 build";

pub(super) fn ensure_run_play_adapter_fresh(repository: &Path) -> Result<(), String> {
    let executable = repository
        .join("target")
        .join("dev-opt")
        .join(if cfg!(windows) {
            "run_play.exe"
        } else {
            "run_play"
        });
    let mut additional_inputs = vec![
        repository.join("Cargo.toml"),
        repository.join("Cargo.lock"),
        repository.join("crates/sts_run_play/Cargo.toml"),
    ];
    let cargo_config = repository.join(".cargo/config.toml");
    if cargo_config.is_file() {
        additional_inputs.push(cargo_config);
    }
    oracle_artifact_contract::ensure_depfile_artifact_fresh(
        &executable,
        repository,
        &additional_inputs,
        "run-play adapter",
        REBUILD_COMMAND,
    )
}

pub(super) fn create_operation_directory(
    repository: &Path,
    operation: &str,
    operation_count: u64,
) -> Result<PathBuf, String> {
    if !repository.is_dir() {
        return Err(format!(
            "workbench repository is absent: {}",
            repository.display()
        ));
    }
    let parent = repository
        .join(".oracle-lab")
        .join("workbench-runtime")
        .join("attempts");
    fs::create_dir_all(&parent)
        .map_err(|error| format!("failed to create workbench attempt root: {error}"))?;
    for collision in 0..16_u8 {
        let path = parent.join(format!(
            "{operation}-{}-{operation_count}-{}-{collision}",
            process::id(),
            now_unix_ms()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create workbench attempt {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Err("failed to allocate a create-new workbench attempt".to_owned())
}

pub(super) fn run_typed_adapter(
    repository: &Path,
    attempt: &Path,
    label: &str,
    arguments: &[String],
    timeout: Duration,
) -> Result<(), String> {
    let executable = repository
        .join("target")
        .join("dev-opt")
        .join(if cfg!(windows) {
            "run_play.exe"
        } else {
            "run_play"
        });
    if !executable.is_file() {
        return Err(format!(
            "run_play executable is absent: {}",
            executable.display()
        ));
    }
    let mut command = Command::new(executable);
    command
        .current_dir(repository)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(arguments);
    let output = run_bounded(command, timeout)?;
    persist_subprocess_output(attempt, label, &output)?;
    if !output.status.success() {
        return Err(format!(
            "typed adapter {label} exited with {}: {}",
            output.status,
            failure_detail(&output.stdout, &output.stderr)
        ));
    }
    Ok(())
}

pub(super) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let encoded = fs::read(path)
        .map_err(|error| format!("failed to read typed report {}: {error}", path.display()))?;
    serde_json::from_slice(&encoded)
        .map_err(|error| format!("failed to decode typed report {}: {error}", path.display()))
}

fn persist_subprocess_output(
    attempt: &Path,
    label: &str,
    output: &std::process::Output,
) -> Result<(), String> {
    fs::write(attempt.join(format!("{label}.stdout.log")), &output.stdout)
        .map_err(|error| format!("failed to store {label} stdout: {error}"))?;
    fs::write(attempt.join(format!("{label}.stderr.log")), &output.stderr)
        .map_err(|error| format!("failed to store {label} stderr: {error}"))?;
    Ok(())
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_repository_is_rejected_without_creating_an_attempt_root() {
        let repository = std::env::temp_dir().join(format!(
            "sts-workbench-engine-absent-repository-{}-{}",
            process::id(),
            now_unix_ms()
        ));
        assert!(!repository.exists());

        let error = create_operation_directory(&repository, "observe", 1).unwrap_err();

        assert!(error.contains("workbench repository is absent"));
        assert!(!repository.exists());
    }
}
