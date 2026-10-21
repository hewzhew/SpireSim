use crate::RuntimeBuildStatusV1;
use oracle_artifact_contract::ensure_depfile_artifact_fresh;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

const REBUILD_COMMAND: &str = ".\\workbench.ps1 build";

pub(crate) struct ActorBuildTrackerV1 {
    executable: Option<PathBuf>,
    startup_executable_sha256: Option<String>,
}

impl ActorBuildTrackerV1 {
    pub(crate) fn capture() -> Self {
        Self::capture_at(std::env::current_exe().ok())
    }

    fn capture_at(executable: Option<PathBuf>) -> Self {
        let startup_executable_sha256 = executable
            .as_deref()
            .and_then(|path| file_sha256(path).ok());
        Self {
            executable,
            startup_executable_sha256,
        }
    }

    pub(crate) fn status(&self, repository: &Path) -> RuntimeBuildStatusV1 {
        let current_executable_sha256 = self
            .executable
            .as_deref()
            .and_then(|path| file_sha256(path).ok());
        let canonical_actor = actor_executable(repository);
        let current_actor_sha256 = file_sha256(&canonical_actor).ok();
        let binary_matches_current = self
            .startup_executable_sha256
            .as_ref()
            .zip(current_executable_sha256.as_ref())
            .zip(current_actor_sha256.as_ref())
            .map(|((startup, running), canonical)| startup == running && running == canonical);
        let actor_freshness = artifact_freshness(&canonical_actor, repository);
        let engine_freshness = dependency_freshness(ensure_engine_fresh(repository));
        let run_play_freshness = dependency_freshness(ensure_run_play_adapter_fresh(repository));
        let live_bridge_freshness = dependency_freshness(ensure_live_bridge_fresh(repository));
        let artifact_freshness = combine_freshness([
            actor_freshness,
            engine_freshness,
            run_play_freshness,
            live_bridge_freshness,
        ]);
        RuntimeBuildStatusV1 {
            startup_executable_sha256: self.startup_executable_sha256.clone(),
            current_executable_sha256,
            binary_matches_current,
            actor_freshness: actor_freshness.to_owned(),
            engine_freshness: engine_freshness.to_owned(),
            run_play_freshness: run_play_freshness.to_owned(),
            live_bridge_freshness: live_bridge_freshness.to_owned(),
            artifact_freshness: artifact_freshness.to_owned(),
        }
    }
}

fn actor_executable(repository: &Path) -> PathBuf {
    repository
        .join("target")
        .join("dev-opt")
        .join(if cfg!(windows) {
            "sts_workbench_actor.exe"
        } else {
            "sts_workbench_actor"
        })
}

pub(crate) fn ensure_engine_fresh(repository: &Path) -> Result<(), String> {
    ensure_named_artifact_fresh(
        repository,
        &repository
            .join("target")
            .join("dev-opt")
            .join(if cfg!(windows) {
                "sts_workbench_engine.exe"
            } else {
                "sts_workbench_engine"
            }),
        &[
            repository.join("crates/sts_workbench_engine/Cargo.toml"),
            repository.join("crates/sts_workbench_engine_protocol/Cargo.toml"),
            repository.join("crates/sts_workbench_artifact_store/Cargo.toml"),
            repository.join("crates/sts_java_parity_protocol/Cargo.toml"),
        ],
        "STS workbench Engine",
    )
}

pub(crate) fn ensure_run_play_adapter_fresh(repository: &Path) -> Result<(), String> {
    ensure_named_artifact_fresh(
        repository,
        &repository
            .join("target")
            .join("dev-opt")
            .join(if cfg!(windows) {
                "run_play.exe"
            } else {
                "run_play"
            }),
        &[repository.join("crates/sts_run_play/Cargo.toml")],
        "run-play adapter",
    )
}

pub(crate) fn ensure_live_bridge_fresh(repository: &Path) -> Result<(), String> {
    ensure_named_artifact_fresh(
        repository,
        &repository
            .join("target")
            .join("debug")
            .join(if cfg!(windows) {
                "sts_java_parity_live.exe"
            } else {
                "sts_java_parity_live"
            }),
        &[
            repository.join("crates/sts_java_parity_live/Cargo.toml"),
            repository.join("crates/sts_java_parity_protocol/Cargo.toml"),
        ],
        "Java parity live bridge",
    )
}

pub(crate) fn ensure_live_runtime_fresh(repository: &Path) -> Result<(), String> {
    ensure_run_play_adapter_fresh(repository)?;
    ensure_live_bridge_fresh(repository)
}

fn ensure_named_artifact_fresh(
    repository: &Path,
    executable: &Path,
    additional_inputs: &[PathBuf],
    label: &str,
) -> Result<(), String> {
    let mut cargo_inputs = vec![repository.join("Cargo.toml"), repository.join("Cargo.lock")];
    cargo_inputs.extend_from_slice(additional_inputs);
    let cargo_config = repository.join(".cargo/config.toml");
    if cargo_config.is_file() {
        cargo_inputs.push(cargo_config);
    }
    ensure_depfile_artifact_fresh(
        executable,
        repository,
        &cargo_inputs,
        label,
        REBUILD_COMMAND,
    )
}

fn dependency_freshness(result: Result<(), String>) -> &'static str {
    match result {
        Ok(()) => "fresh",
        Err(detail) if detail.contains(" is stale:") => "stale",
        Err(_) => "unverifiable",
    }
}

fn combine_freshness<const N: usize>(values: [&str; N]) -> &'static str {
    if values.contains(&"stale") {
        "stale"
    } else if values.iter().all(|value| *value == "fresh") {
        "fresh"
    } else {
        "unverifiable"
    }
}

fn artifact_freshness(executable: &Path, repository: &Path) -> &'static str {
    let mut cargo_inputs = vec![
        repository.join("Cargo.toml"),
        repository.join("Cargo.lock"),
        repository.join("crates/sts_workbench_runtime/Cargo.toml"),
        repository.join("crates/sts_workbench_engine_protocol/Cargo.toml"),
    ];
    let cargo_config = repository.join(".cargo/config.toml");
    if cargo_config.is_file() {
        cargo_inputs.push(cargo_config);
    }
    match ensure_depfile_artifact_fresh(
        executable,
        repository,
        &cargo_inputs,
        "STS workbench actor",
        REBUILD_COMMAND,
    ) {
        Ok(()) => "fresh",
        Err(detail) if detail.contains(" is stale:") => "stale",
        Err(_) => "unverifiable",
    }
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read actor executable for hashing: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn build_status_distinguishes_source_staleness_from_replaced_machine_code() {
        let repository = temporary_directory("actor-build-status");
        let actor_crate = repository.join("crates/sts_workbench_runtime");
        let run_play_crate = repository.join("crates/sts_run_play");
        let live_crate = repository.join("crates/sts_java_parity_live");
        let protocol_crate = repository.join("crates/sts_java_parity_protocol");
        let engine_crate = repository.join("crates/sts_workbench_engine");
        let engine_protocol_crate = repository.join("crates/sts_workbench_engine_protocol");
        let artifact_crate = repository.join("crates/sts_workbench_artifact_store");
        for directory in [
            &actor_crate,
            &run_play_crate,
            &live_crate,
            &protocol_crate,
            &engine_crate,
            &engine_protocol_crate,
            &artifact_crate,
        ] {
            fs::create_dir_all(directory).expect("create crate directory");
        }
        for path in [
            repository.join("Cargo.toml"),
            repository.join("Cargo.lock"),
            actor_crate.join("Cargo.toml"),
            run_play_crate.join("Cargo.toml"),
            live_crate.join("Cargo.toml"),
            protocol_crate.join("Cargo.toml"),
            engine_crate.join("Cargo.toml"),
            engine_protocol_crate.join("Cargo.toml"),
            artifact_crate.join("Cargo.toml"),
        ] {
            fs::write(path, b"manifest").expect("write manifest");
        }
        let actor_dependency = actor_crate.join("src.rs");
        let run_play_dependency = run_play_crate.join("src.rs");
        let live_dependency = live_crate.join("src.rs");
        let engine_dependency = engine_crate.join("src.rs");
        fs::write(&actor_dependency, b"source-v1").expect("write dependency");
        fs::write(&run_play_dependency, b"source-v1").expect("write dependency");
        fs::write(&live_dependency, b"source-v1").expect("write dependency");
        fs::write(&engine_dependency, b"source-v1").expect("write dependency");
        thread::sleep(Duration::from_millis(30));
        let dev_opt = repository.join("target/dev-opt");
        let debug = repository.join("target/debug");
        fs::create_dir_all(&dev_opt).expect("create dev-opt directory");
        fs::create_dir_all(&debug).expect("create debug directory");
        let actor = dev_opt.join(if cfg!(windows) {
            "sts_workbench_actor.exe"
        } else {
            "sts_workbench_actor"
        });
        let executable = repository.join(if cfg!(windows) {
            "running-actor-image.exe"
        } else {
            "running-actor-image"
        });
        let run_play = dev_opt.join(if cfg!(windows) {
            "run_play.exe"
        } else {
            "run_play"
        });
        let engine = dev_opt.join(if cfg!(windows) {
            "sts_workbench_engine.exe"
        } else {
            "sts_workbench_engine"
        });
        let live_bridge = debug.join(if cfg!(windows) {
            "sts_java_parity_live.exe"
        } else {
            "sts_java_parity_live"
        });
        fs::write(&actor, b"machine-v1").expect("write executable");
        fs::write(&executable, b"machine-v1").expect("write executable");
        fs::write(&run_play, b"machine-v1").expect("write executable");
        fs::write(&engine, b"machine-v1").expect("write executable");
        fs::write(&live_bridge, b"machine-v1").expect("write executable");
        thread::sleep(Duration::from_millis(30));
        for (artifact, dependency) in [
            (&actor, &actor_dependency),
            (&run_play, &run_play_dependency),
            (&engine, &engine_dependency),
            (&live_bridge, &live_dependency),
        ] {
            fs::write(
                artifact.with_extension("d"),
                format!("{}: {}\n", artifact.display(), dependency.display()),
            )
            .expect("write depfile");
        }

        let tracker = ActorBuildTrackerV1::capture_at(Some(executable.clone()));
        let fresh = tracker.status(&repository);
        assert_eq!(fresh.artifact_freshness, "fresh");
        assert_eq!(fresh.actor_freshness, "fresh");
        assert_eq!(fresh.engine_freshness, "fresh");
        assert_eq!(fresh.binary_matches_current, Some(true));

        thread::sleep(Duration::from_millis(30));
        fs::write(&engine_dependency, b"source-v2").expect("rewrite Engine dependency");
        assert_eq!(artifact_freshness(&actor, &repository), "fresh");
        assert!(ensure_engine_fresh(&repository).is_err());
        let engine_stale = tracker.status(&repository);
        assert_eq!(engine_stale.artifact_freshness, "stale");
        assert_eq!(engine_stale.actor_freshness, "fresh");
        assert_eq!(engine_stale.engine_freshness, "stale");

        thread::sleep(Duration::from_millis(30));
        fs::write(&engine, b"machine-v2").expect("replace Engine executable");
        let engine_only_rebuilt = tracker.status(&repository);
        assert_eq!(engine_only_rebuilt.artifact_freshness, "fresh");
        assert_eq!(engine_only_rebuilt.actor_freshness, "fresh");
        assert_eq!(engine_only_rebuilt.engine_freshness, "fresh");
        assert_eq!(engine_only_rebuilt.binary_matches_current, Some(true));

        thread::sleep(Duration::from_millis(30));
        fs::write(&run_play_dependency, b"source-v2").expect("rewrite dependency");
        let stale = tracker.status(&repository);
        assert_eq!(stale.artifact_freshness, "stale");
        assert_eq!(stale.actor_freshness, "fresh");
        assert_eq!(stale.run_play_freshness, "stale");
        assert_eq!(stale.binary_matches_current, Some(true));
        assert!(ensure_run_play_adapter_fresh(&repository).is_err());

        thread::sleep(Duration::from_millis(30));
        fs::write(&actor, b"machine-v2").expect("replace executable");
        let replaced = tracker.status(&repository);
        assert_eq!(replaced.binary_matches_current, Some(false));
        let _ = fs::remove_dir_all(repository);
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("sts-{label}-{}-{nonce}", process::id()))
    }
}
