use std::process::Command;

use serde::{Deserialize, Serialize};

/// Source revision recorded by reproducible oracle artifacts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceIdentity {
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
}

pub fn current_source_identity() -> SourceIdentity {
    SourceIdentity {
        git_commit: current_git_commit(),
        git_dirty: current_git_dirty(),
    }
}

fn current_git_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|commit| !commit.is_empty())
}

fn current_git_dirty() -> Option<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| !String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_identity_is_structured_and_serializable() {
        let source = SourceIdentity {
            git_commit: Some("abc123".to_string()),
            git_dirty: Some(false),
        };

        let value = serde_json::to_value(&source).unwrap();

        assert_eq!(value["git_commit"], "abc123");
        assert_eq!(value["git_dirty"], false);
    }

    #[test]
    fn current_source_identity_is_available_from_evaluation_library() {
        let _ = current_source_identity();
    }
}
