use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use blake2::{Blake2b512, Digest};
use oracle_artifact_contract::{artifact_dependencies, ensure_artifact_fresh, CanonicalArtifact};
use serde_json::{json, Value};

pub(super) fn validate(canonical_oracle: bool) -> Result<(), String> {
    const REQUIRED_PROFILE: &str = "release";
    const BUILT_PROFILE: &str = env!("STS_CARGO_PROFILE");
    const REPOSITORY_ROOT: &str = env!("STS_REPOSITORY_ROOT");

    if !canonical_oracle {
        return Err(
            "oracle_lab refuses direct execution; run `.\\scripts\\ol.ps1 <command> ...`"
                .to_string(),
        );
    }
    if BUILT_PROFILE != REQUIRED_PROFILE {
        return Err(format!(
            "oracle_lab was built with forbidden profile `{BUILT_PROFILE}`; \
             run `.\\scripts\\ol.ps1 build oracle`"
        ));
    }
    let executable_name = if cfg!(windows) {
        "oracle_lab.exe"
    } else {
        "oracle_lab"
    };
    let expected = PathBuf::from(REPOSITORY_ROOT)
        .join("target")
        .join(REQUIRED_PROFILE)
        .join(executable_name);
    let current = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("failed to identify running oracle_lab: {error}"))?;
    let expected = expected.canonicalize().map_err(|error| {
        format!(
            "canonical oracle_lab artifact is missing at {}: {error}; \
             run `.\\scripts\\ol.ps1 build oracle`",
            expected.display()
        )
    })?;
    if current != expected {
        return Err(format!(
            "oracle_lab refuses non-canonical artifact {}; expected {}; \
             run `.\\scripts\\ol.ps1 <command> ...`",
            current.display(),
            expected.display()
        ));
    }
    validate_source_freshness(&expected)?;
    Ok(())
}

fn validate_source_freshness(executable: &Path) -> Result<(), String> {
    let repository = PathBuf::from(env!("STS_REPOSITORY_ROOT"));
    ensure_artifact_fresh(
        executable,
        &repository,
        CanonicalArtifact::OracleHost,
        "canonical oracle laboratory",
        ".\\scripts\\ol.ps1 build oracle",
    )
}

pub(super) fn source_content_fingerprint(
    repository: &Path,
    dependencies: &[PathBuf],
) -> Result<String, String> {
    let mut files = BTreeSet::new();
    for dependency in dependencies {
        let dependency = if dependency.is_absolute() {
            dependency.clone()
        } else {
            repository.join(dependency)
        };
        collect_fingerprint_files(&dependency, &mut files)?;
    }
    let mut digest = Blake2b512::new();
    for file in files {
        let bytes = std::fs::read(&file).map_err(|error| {
            format!(
                "failed to fingerprint canonical dependency '{}': {error}",
                file.display()
            )
        })?;
        digest.update(file.to_string_lossy().as_bytes());
        digest.update([0]);
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn collect_fingerprint_files(path: &Path, files: &mut BTreeSet<PathBuf>) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        format!(
            "failed to inspect canonical dependency '{}': {error}",
            path.display()
        )
    })?;
    if metadata.is_file() {
        let canonical = path.canonicalize().map_err(|error| {
            format!(
                "failed to canonicalize source dependency '{}': {error}",
                path.display()
            )
        })?;
        files.insert(canonical);
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(format!(
            "canonical dependency is neither file nor directory: {}",
            path.display()
        ));
    }

    let mut entries = std::fs::read_dir(path)
        .map_err(|error| {
            format!(
                "failed to enumerate canonical dependency directory '{}': {error}",
                path.display()
            )
        })?
        .map(|entry| {
            entry.map(|entry| entry.path()).map_err(|error| {
                format!(
                    "failed to read canonical dependency directory entry under '{}': {error}",
                    path.display()
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    for entry in entries {
        collect_fingerprint_files(&entry, files)?;
    }
    Ok(())
}

pub(super) fn runtime_identity() -> Value {
    let repository = PathBuf::from(env!("STS_REPOSITORY_ROOT"));
    let executable = std::env::current_exe().ok();
    let metadata = executable
        .as_ref()
        .and_then(|path| std::fs::metadata(path).ok());
    let modified_unix_ms = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| {
            modified
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .ok()
        })
        .map(|duration| duration.as_millis());
    let git_head = read_git_head_fast(&repository);
    json!({
        "profile": env!("STS_CARGO_PROFILE"),
        "executable": executable,
        "artifact_bytes": metadata.map(|metadata| metadata.len()),
        "artifact_modified_unix_ms": modified_unix_ms,
        "git_head": git_head,
        "git_dirty": Value::Null,
        "dirty_scan": "omitted_in_compact_mode",
    })
}

/// Content identity of every source dependency recorded for the canonical
/// executable. Unlike `git_head`, this also distinguishes binaries built from
/// different uncommitted worktrees at the same revision.
pub(super) fn runtime_source_content_fingerprint() -> Result<String, String> {
    let repository = PathBuf::from(env!("STS_REPOSITORY_ROOT"));
    let executable = std::env::current_exe()
        .map_err(|error| format!("failed to identify running oracle_lab: {error}"))?;
    let dependencies = artifact_dependencies(
        &executable,
        &repository,
        CanonicalArtifact::OracleHost,
        "canonical oracle laboratory",
        ".\\scripts\\ol.ps1 build oracle",
    )?;
    source_content_fingerprint(&repository, &dependencies)
}

fn read_git_head_fast(repository: &Path) -> Option<String> {
    let dot_git = repository.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        let pointer = std::fs::read_to_string(dot_git).ok()?;
        let relative = pointer.trim().strip_prefix("gitdir:")?.trim();
        repository.join(relative)
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let revision = if let Some(reference) = head.trim().strip_prefix("ref: ") {
        std::fs::read_to_string(git_dir.join(reference))
            .ok()
            .or_else(|| {
                std::fs::read_to_string(git_dir.join("packed-refs"))
                    .ok()?
                    .lines()
                    .find_map(|line| {
                        let (hash, name) = line.split_once(' ')?;
                        (name == reference).then(|| hash.to_owned())
                    })
            })?
    } else {
        head
    };
    Some(revision.trim().chars().take(12).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_fingerprint_expands_directory_dependencies_without_double_counting_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sts-oracle-source-fingerprint-{}-{unique}",
            std::process::id()
        ));
        let source = root.join("src");
        std::fs::create_dir_all(source.join("nested")).expect("create fixture tree");
        std::fs::write(source.join("lib.rs"), b"pub fn fixture() {}\n")
            .expect("write fixture source");
        std::fs::write(source.join("nested").join("data.txt"), b"fixture\n")
            .expect("write fixture data");

        let directory = source_content_fingerprint(&root, &[PathBuf::from("src")])
            .expect("fingerprint directory dependency");
        let explicit = source_content_fingerprint(
            &root,
            &[
                PathBuf::from("src/lib.rs"),
                PathBuf::from("src/nested/data.txt"),
                PathBuf::from("src/lib.rs"),
            ],
        )
        .expect("fingerprint explicit dependencies");

        assert_eq!(directory, explicit);
        std::fs::remove_dir_all(root).expect("remove fixture tree");
    }
}
