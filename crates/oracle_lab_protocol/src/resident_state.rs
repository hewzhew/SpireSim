use std::path::{Path, PathBuf};

/// Resolves the mutable workspace used by a resident oracle service.
///
/// Resident services autosave in place after every mutation and on shutdown.
/// Requiring their workspace to live below the ignored state root prevents a
/// committed witness fixture or historical build artifact from becoming an
/// accidental persistence target.
pub fn resolve_owned_resident_workspace(
    workspace: &Path,
    state_root: &Path,
) -> Result<PathBuf, String> {
    let workspace = workspace.canonicalize().map_err(|error| {
        format!(
            "failed to resolve resident oracle workspace '{}': {error}",
            workspace.display()
        )
    })?;
    if !workspace.is_file() {
        return Err(format!(
            "resident oracle workspace is not a file: {}",
            workspace.display()
        ));
    }
    let state_root = state_root.canonicalize().map_err(|error| {
        format!(
            "failed to resolve resident oracle state root '{}': {error}",
            state_root.display()
        )
    })?;
    if !workspace.starts_with(&state_root) {
        return Err(format!(
            "resident oracle workspace '{}' is outside owned state root '{}'; resident services \
             autosave in place. Create or copy a mutable workspace below '.oracle-lab/' before \
             starting a session; committed fixtures and historical target artifacts are read-only \
             inputs",
            workspace.display(),
            state_root.display()
        ));
    }
    Ok(workspace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn resident_workspace_must_be_owned_by_the_state_root() {
        let fixture = temporary_directory();
        let state_root = fixture.join(".oracle-lab");
        let cases = state_root.join("cases");
        fs::create_dir_all(&cases).expect("create owned state root");
        let owned = cases.join("seed.workspace.json");
        let external = fixture.join("golden.workspace.json");
        fs::write(&owned, b"{}").expect("write owned workspace");
        fs::write(&external, b"{}").expect("write external workspace");

        assert_eq!(
            resolve_owned_resident_workspace(&owned, &state_root)
                .expect("owned workspace must resolve"),
            owned.canonicalize().expect("canonical owned workspace")
        );
        let error = resolve_owned_resident_workspace(&external, &state_root)
            .expect_err("external workspace must be rejected");
        assert!(error.contains("autosave in place"));
        assert!(error.contains("read-only inputs"));
        let _ = fs::remove_dir_all(fixture);
    }

    fn temporary_directory() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "oracle-resident-state-contract-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("create resident-state fixture");
        directory
    }
}
