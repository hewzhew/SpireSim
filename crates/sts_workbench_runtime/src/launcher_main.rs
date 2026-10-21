//! Stable process boundary used by the Python MCP facade.
//!
//! The frequently rebuilt Rust actor is copied to an immutable image before
//! launch. Windows therefore locks the image, not Cargo's canonical output,
//! and a new actor can be built while the previous one is still serving.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, ExitStatus, Stdio};
use sts_workbench_supervisor::ExecutableImageStoreV1;

const ACTOR_IMAGE_PREFIX: &str = "sts-workbench-actor";

fn main() -> ExitCode {
    match launch_actor(std::env::args_os().skip(1)) {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!("workbench actor exited with {status}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("failed to launch workbench actor: {error}");
            ExitCode::FAILURE
        }
    }
}

fn launch_actor(arguments: impl IntoIterator<Item = OsString>) -> Result<ExitStatus, String> {
    let repository = std::env::current_dir()
        .and_then(fs::canonicalize)
        .map_err(|error| format!("failed to resolve workbench repository: {error}"))?;
    let canonical_actor = canonical_actor(&repository);
    if !canonical_actor.is_file() {
        return Err(format!(
            "canonical workbench actor is absent: {}; run .\\workbench.ps1 build",
            canonical_actor.display()
        ));
    }
    let image_store = ExecutableImageStoreV1::new(
        repository
            .join(".oracle-lab")
            .join("workbench-runtime")
            .join("actor-images"),
        ACTOR_IMAGE_PREFIX,
    )
    .map_err(|error| error.to_string())?;
    let image = image_store
        .materialize(&canonical_actor)
        .map_err(|error| error.to_string())?;
    reclaim_old_actor_images(
        image
            .executable()
            .parent()
            .expect("materialized image has a parent"),
        image.executable(),
    );
    let mut command = image.command().map_err(|error| error.to_string())?;
    command
        .args(arguments)
        .current_dir(&repository)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| error.to_string())
}

fn canonical_actor(repository: &Path) -> PathBuf {
    repository
        .join("target")
        .join("dev-opt")
        .join(if cfg!(windows) {
            "sts_workbench_actor.exe"
        } else {
            "sts_workbench_actor"
        })
}

fn reclaim_old_actor_images(directory: &Path, active: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == active {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(&format!("{ACTOR_IMAGE_PREFIX}-"))
            || !entry.file_type().is_ok_and(|kind| kind.is_file())
        {
            continue;
        }
        // A concurrently running image is locked on Windows and is simply
        // reclaimed on a later launch. Cleanup never blocks actor admission.
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_actor_is_distinct_from_the_stable_launcher() {
        let repository = Path::new("repository");
        let actor = canonical_actor(repository);

        assert!(actor.to_string_lossy().contains("sts_workbench_actor"));
        assert!(!actor.to_string_lossy().ends_with("sts_workbenchd.exe"));
    }
}
