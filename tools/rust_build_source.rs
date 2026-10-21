//! Conservative local Rust source provenance, captured by Cargo, never at runtime.
//!
//! This is a declared input superset, not a semantic version or a binary digest.
//! Source bytes, including tests/comments, are significant. Dependency resolution
//! is recorded through Cargo.lock; this does not attest registry/cache contents.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub const INPUT_SCOPE: &str = "workspace-rust-source-v2";
const INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "build.rs",
    ".cargo",
    "src",
    "crates",
    "tools",
];

fn collect(root: &Path, relative: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() {
        return Err(std::io::Error::other(format!(
            "build source must not follow a symlink: {}",
            relative.display()
        )));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            // Installed dependencies are neither workspace Rust sources nor
            // build-time helpers. Skip the directory before checking links.
            if entry.file_name() != "target"
                && entry.file_name() != ".git"
                && entry.file_name() != "node_modules"
            {
                collect(root, &relative.join(entry.file_name()), files)?;
            }
        }
    } else if matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("rs" | "toml" | "lock" | "json" | "txt")
    ) {
        // tools owns build-time Rust helpers; Python/NLP/reference artifacts
        // there are not inputs to this Rust source identity.
        if !relative.starts_with("tools") || path.extension().is_some_and(|value| value == "rs") {
            files.push(relative.to_path_buf());
        }
    }
    Ok(())
}

pub fn source_manifest(root: &Path) -> std::io::Result<serde_json::Value> {
    let mut files = Vec::new();
    for input in INPUTS {
        collect(root, Path::new(input), &mut files)?;
    }
    files.sort_by_key(|path| path.to_string_lossy().replace('\\', "/"));
    files.dedup();
    let mut hash = Sha256::new();
    hash.update(INPUT_SCOPE.as_bytes());
    let mut entries = Vec::new();
    for relative in files {
        let name = relative
            .to_str()
            .ok_or_else(|| std::io::Error::other("build source requires UTF-8 relative paths"))?
            .replace('\\', "/");
        let bytes = fs::read(root.join(&relative))?;
        // Length framing prevents path/content concatenation ambiguities.
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
        entries.push(serde_json::json!({
            "path": name, "sha256": format!("{:x}", Sha256::digest(&bytes))
        }));
    }
    Ok(serde_json::json!({
        "input_scope": INPUT_SCOPE,
        "source_sha256": format!("{:x}", hash.finalize()),
        "files": entries,
    }))
}

#[cfg(not(test))]
pub fn emit(root: &Path, profile: &str) {
    // Directory watches also detect added and removed source files.
    for input in INPUTS {
        println!("cargo:rerun-if-changed={}", root.join(input).display());
    }
    let manifest = source_manifest(root).expect("Rust build source inputs must be readable");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    fs::write(
        out.join("rust-build-source-manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("source manifest JSON"),
    )
    .expect("write build source manifest");

    let mut configuration = std::collections::BTreeMap::new();
    for name in [
        "TARGET",
        "HOST",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_ENCODED_RUSTFLAGS",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        configuration.insert(name.to_owned(), std::env::var(name).unwrap_or_default());
    }
    for (name, value) in std::env::vars() {
        if name.starts_with("CARGO_FEATURE_") || name.starts_with("CARGO_CFG_") {
            configuration.insert(name, value);
        }
    }
    configuration.insert("profile".into(), profile.into());
    let compiler = std::process::Command::new(std::env::var_os("RUSTC").expect("Cargo RUSTC"))
        .arg("-vV")
        .output()
        .expect("read build compiler version");
    assert!(compiler.status.success(), "build compiler version failed");
    configuration.insert(
        "rustc".into(),
        String::from_utf8(compiler.stdout)
            .expect("compiler version UTF-8")
            .replace("\r\n", "\n"),
    );
    let identity = serde_json::json!({
        "schema": "sts-rust-build-source-v1",
        "input_scope": INPUT_SCOPE,
        "source_sha256": manifest["source_sha256"],
        "configuration": configuration,
    });
    println!("cargo:rustc-env=STS_RUN_PLAY_BUILD_SOURCE={identity}");
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree(PathBuf);
    impl Tree {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "sts-build-source-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let tree = Self(root);
            for directory in [".cargo", "src", "crates", "tools"] {
                fs::create_dir(tree.0.join(directory)).unwrap();
            }
            for file in ["Cargo.toml", "Cargo.lock", "build.rs", "src/lib.rs"] {
                tree.write(file, "initial");
            }
            tree
        }
        fn write(&self, path: &str, bytes: &str) {
            fs::write(self.0.join(path), bytes).unwrap();
        }
        fn identity(&self) -> serde_json::Value {
            source_manifest(&self.0).unwrap()
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn source_changes_additions_removals_and_contract_inputs_change_identity() {
        let tree = Tree::new();
        let initial = tree.identity();
        for file in ["src/lib.rs", "Cargo.lock", "build.rs"] {
            tree.write(file, "changed while manual semantics label stays fixed");
            assert_ne!(tree.identity()["source_sha256"], initial["source_sha256"]);
            tree.write(file, "initial");
        }
        tree.write("src/new.rs", "new module");
        assert_ne!(tree.identity()["source_sha256"], initial["source_sha256"]);
        fs::remove_file(tree.0.join("src/new.rs")).unwrap();
        assert_eq!(tree.identity(), initial);
        tree.write("src/identity.json", "generated content contract");
        assert_ne!(tree.identity()["source_sha256"], initial["source_sha256"]);
    }

    #[test]
    fn relocation_and_unrelated_reference_edits_do_not_change_identity() {
        let first = Tree::new();
        let second = Tree::new();
        second.write("tools/query.py", "unrelated reference reader");
        second.write("tools/draft.json", "unrelated NLP draft");
        second.write("src/README.md", "documentation");
        assert_eq!(first.identity(), second.identity());
        second.write("tools/contract.rs", "build-time helper");
        assert_ne!(first.identity(), second.identity());
    }

    #[test]
    fn missing_declared_input_fails_instead_of_hashing_partial_sources() {
        let tree = Tree::new();
        fs::remove_file(tree.0.join("Cargo.lock")).unwrap();
        assert!(source_manifest(&tree.0).is_err());
    }

    #[test]
    fn installed_rust_dependencies_do_not_change_workspace_source_identity() {
        let tree = Tree::new();
        let before = tree.identity();
        fs::create_dir_all(tree.0.join("tools/player/node_modules/vendor/crates")).unwrap();
        tree.write(
            "tools/player/node_modules/vendor/crates/lib.rs",
            "installed vendor source",
        );
        assert_eq!(tree.identity(), before);
        fs::create_dir_all(tree.0.join("tools/player")).unwrap();
        tree.write("tools/player/build_helper.rs", "actual build helper");
        assert_ne!(tree.identity(), before);
    }

    #[cfg(unix)]
    #[test]
    fn installed_links_are_ignored_but_source_links_are_rejected() {
        let tree = Tree::new();
        let before = tree.identity();
        fs::create_dir_all(tree.0.join("tools/player")).unwrap();
        std::os::unix::fs::symlink(
            "missing-installed-tree",
            tree.0.join("tools/player/node_modules"),
        )
        .unwrap();
        assert_eq!(tree.identity(), before);
        std::os::unix::fs::symlink("missing-rust-source", tree.0.join("src/linked.rs")).unwrap();
        assert!(source_manifest(&tree.0).is_err());
    }
}
