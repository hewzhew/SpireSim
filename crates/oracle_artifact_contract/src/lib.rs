use std::fs;
use std::path::{Path, PathBuf};

#[cfg(feature = "oracle-host")]
const ORACLE_HOST_BUILD_INPUTS: &str = include_str!("../build-inputs/oracle-host.txt");
#[cfg(feature = "oracle-client")]
const ORACLE_CLIENT_BUILD_INPUTS: &str = include_str!("../build-inputs/oracle-client.txt");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalArtifact {
    #[cfg(feature = "oracle-host")]
    OracleHost,
    #[cfg(feature = "oracle-client")]
    OracleClient,
}

impl CanonicalArtifact {
    fn declared_build_inputs(self) -> &'static str {
        match self {
            #[cfg(feature = "oracle-host")]
            Self::OracleHost => ORACLE_HOST_BUILD_INPUTS,
            #[cfg(feature = "oracle-client")]
            Self::OracleClient => ORACLE_CLIENT_BUILD_INPUTS,
        }
    }
}

pub fn artifact_dependencies(
    executable: &Path,
    repository: &Path,
    artifact: CanonicalArtifact,
    label: &str,
    rebuild_command: &str,
) -> Result<Vec<PathBuf>, String> {
    let depfile = executable.with_extension("d");
    let depfile_text = fs::read_to_string(&depfile).map_err(|error| {
        format!(
            "{label} dependency manifest is missing at '{}': {error}; rebuild with `{rebuild_command}`",
            depfile.display()
        )
    })?;
    let mut dependencies = depfile_dependencies(&depfile_text)
        .into_iter()
        .map(|dependency| absolute_from(repository, dependency))
        .collect::<Vec<_>>();
    dependencies.extend(
        artifact
            .declared_build_inputs()
            .lines()
            .map(str::trim)
            .filter(|input| !input.is_empty() && !input.starts_with('#'))
            .map(|input| repository.join(input)),
    );
    dependencies.sort();
    dependencies.dedup();
    Ok(dependencies)
}

pub fn ensure_artifact_fresh(
    executable: &Path,
    repository: &Path,
    artifact: CanonicalArtifact,
    label: &str,
    rebuild_command: &str,
) -> Result<(), String> {
    let dependencies =
        artifact_dependencies(executable, repository, artifact, label, rebuild_command)?;
    ensure_dependencies_fresh(executable, &dependencies, label, rebuild_command)
}

/// Validates an ordinary Cargo executable against the dependency manifest
/// emitted beside it. Callers may add manifests or configuration files that
/// rustc does not list in the depfile but that still affect the artifact.
pub fn ensure_depfile_artifact_fresh(
    executable: &Path,
    repository: &Path,
    additional_dependencies: &[PathBuf],
    label: &str,
    rebuild_command: &str,
) -> Result<(), String> {
    let depfile = executable.with_extension("d");
    let depfile_text = fs::read_to_string(&depfile).map_err(|error| {
        format!(
            "{label} dependency manifest is missing at '{}': {error}; rebuild with `{rebuild_command}`",
            depfile.display()
        )
    })?;
    let dependencies = depfile_dependencies(&depfile_text)
        .into_iter()
        .map(|dependency| absolute_from(repository, dependency))
        .collect::<Vec<_>>();
    ensure_dependencies_fresh(executable, &dependencies, label, rebuild_command)?;

    // Cargo may accept a manifest-only edit without relinking the executable
    // (for example, a comment change), but a successful build refreshes the
    // depfile.  Treat that Cargo-owned depfile as the receipt for manifests
    // and configuration instead of requiring their mtimes to predate the
    // machine code.  Source dependencies remain checked against the executable
    // above, so a no-op Cargo invocation cannot bless stale Rust code.
    let mut cargo_inputs = additional_dependencies
        .iter()
        .cloned()
        .map(|dependency| absolute_from(repository, dependency))
        .collect::<Vec<_>>();
    cargo_inputs.sort();
    cargo_inputs.dedup();
    ensure_dependencies_fresh(&depfile, &cargo_inputs, label, rebuild_command)
}

fn ensure_dependencies_fresh(
    executable: &Path,
    dependencies: &[PathBuf],
    label: &str,
    rebuild_command: &str,
) -> Result<(), String> {
    let executable_modified = fs::metadata(executable)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| {
            format!(
                "failed to inspect {label} '{}': {error}",
                executable.display()
            )
        })?;
    for dependency in dependencies {
        let modified = fs::metadata(dependency)
            .and_then(|metadata| metadata.modified())
            .map_err(|error| {
                format!(
                    "{label} build input '{}' is unavailable: {error}. Rebuild once with \
                     `{rebuild_command}`; refusing to trust an unverifiable artifact",
                    dependency.display()
                )
            })?;
        if modified > executable_modified {
            return Err(format!(
                "{label} is stale: '{}' is newer than '{}'. Rebuild once with \
                 `{rebuild_command}`; refusing to run stale search code",
                dependency.display(),
                executable.display()
            ));
        }
    }
    Ok(())
}

fn absolute_from(repository: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        repository.join(path)
    }
}

fn depfile_dependencies(depfile: &str) -> Vec<PathBuf> {
    depfile
        .lines()
        .filter_map(|line| line.split_once(": ").map(|(_, dependencies)| dependencies))
        .flat_map(str::split_whitespace)
        .filter(|dependency| !dependency.ends_with(':'))
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn windows_depfile_parser_preserves_drive_prefixes() {
        let dependencies = depfile_dependencies(
            "D:\\rust\\target\\oracle_lab.exe: D:\\rust\\src\\lib.rs D:\\rust\\src\\main.rs\n",
        );
        assert_eq!(
            dependencies,
            [
                PathBuf::from(r"D:\rust\src\lib.rs"),
                PathBuf::from(r"D:\rust\src\main.rs"),
            ]
        );
    }

    #[test]
    fn missing_declared_build_input_rejects_unverifiable_artifact() {
        let directory = temporary_directory("missing-input");
        let executable = directory.join("oracle_lab_client.exe");
        fs::write(&executable, b"artifact").expect("write artifact");
        fs::write(
            executable.with_extension("d"),
            format!("{}:\n", executable.display()),
        )
        .expect("write depfile");

        let error = ensure_dependencies_fresh(
            &executable,
            &[directory.join("missing-Cargo.toml")],
            "test artifact",
            "cargo build",
        )
        .expect_err("missing declared input must fail closed");
        assert!(error.contains("unverifiable artifact"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn generic_depfile_artifact_rejects_a_newer_source() {
        let directory = temporary_directory("generic-depfile-stale");
        let executable = directory.join("run_play.exe");
        let dependency = directory.join("run_play.rs");
        fs::write(&dependency, b"old source").expect("write source");
        std::thread::sleep(Duration::from_millis(20));
        fs::write(&executable, b"artifact").expect("write artifact");
        fs::write(
            executable.with_extension("d"),
            format!("{}: {}\n", executable.display(), dependency.display()),
        )
        .expect("write depfile");
        ensure_depfile_artifact_fresh(
            &executable,
            &directory,
            &[],
            "test workbench",
            "cargo build",
        )
        .expect("fresh artifact");

        std::thread::sleep(Duration::from_millis(20));
        fs::write(&dependency, b"new source").expect("rewrite source");
        let error = ensure_depfile_artifact_fresh(
            &executable,
            &directory,
            &[],
            "test workbench",
            "cargo build",
        )
        .expect_err("newer source must reject stale artifact");
        assert!(error.contains("refusing to run stale search code"));

        std::thread::sleep(Duration::from_millis(20));
        fs::write(
            executable.with_extension("d"),
            format!("{}: {}\n", executable.display(), dependency.display()),
        )
        .expect("refresh depfile");
        ensure_depfile_artifact_fresh(
            &executable,
            &directory,
            &[],
            "test workbench",
            "cargo build",
        )
        .expect_err("a refreshed Cargo receipt cannot bless stale Rust machine code");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn successful_cargo_receipt_accepts_manifest_change_without_relink() {
        let directory = temporary_directory("manifest-receipt");
        let executable = directory.join("run_play.exe");
        let depfile = executable.with_extension("d");
        let source = directory.join("run_play.rs");
        let manifest = directory.join("Cargo.toml");
        fs::write(&source, b"source").expect("write source");
        fs::write(&manifest, b"[package]\nname = \"fixture\"\n").expect("write manifest");
        std::thread::sleep(Duration::from_millis(20));
        fs::write(&executable, b"artifact").expect("write artifact");
        fs::write(
            &depfile,
            format!("{}: {}\n", executable.display(), source.display()),
        )
        .expect("write depfile");

        std::thread::sleep(Duration::from_millis(20));
        fs::write(
            &manifest,
            b"[package]\nname = \"fixture\"\n# comment only\n",
        )
        .expect("rewrite manifest");
        ensure_depfile_artifact_fresh(
            &executable,
            &directory,
            std::slice::from_ref(&manifest),
            "test workbench",
            "cargo build",
        )
        .expect_err("manifest newer than the Cargo receipt must be rejected");

        std::thread::sleep(Duration::from_millis(20));
        fs::write(
            &depfile,
            format!("{}: {}\n", executable.display(), source.display()),
        )
        .expect("refresh depfile after successful Cargo build");
        ensure_depfile_artifact_fresh(
            &executable,
            &directory,
            &[manifest],
            "test workbench",
            "cargo build",
        )
        .expect("Cargo receipt may advance without relinking unchanged machine code");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn newer_dependency_rejects_stale_artifact() {
        let directory = temporary_directory("stale-input");
        let executable = directory.join("oracle_lab.exe");
        let dependency = directory.join("search.rs");
        fs::write(&executable, b"artifact").expect("write artifact");
        std::thread::sleep(Duration::from_millis(20));
        fs::write(&dependency, b"new source").expect("write dependency");

        let error =
            ensure_dependencies_fresh(&executable, &[dependency], "test artifact", "cargo build")
                .expect_err("newer dependency must reject stale artifact");
        assert!(error.contains("refusing to run stale search code"));
        let _ = fs::remove_dir_all(directory);
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "oracle-artifact-contract-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("create artifact-contract fixture");
        directory
    }
}
