use std::path::PathBuf;

const BUILD_INPUTS: &str = include_str!("../oracle_artifact_contract/build-inputs/oracle-host.txt");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=STS_CARGO_PROFILE={profile}");

    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR")
            .expect("Cargo always provides CARGO_MANIFEST_DIR to build scripts"),
    );
    let repository_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("oracle lab package lives below <repository>/crates")
        .to_path_buf();
    // Freshness validation and Cargo invalidation consume the same declared
    // input contract. This prevents a manifest-only edit from producing a
    // permanent "stale, but Cargo rebuilt nothing" loop.
    for input in BUILD_INPUTS
        .lines()
        .map(str::trim)
        .filter(|input| !input.is_empty() && !input.starts_with('#'))
    {
        println!(
            "cargo:rerun-if-changed={}",
            repository_root.join(input).display()
        );
    }
    println!(
        "cargo:rustc-env=STS_REPOSITORY_ROOT={}",
        repository_root.display()
    );

    // Unoptimized orchestration code uses larger stack frames than the hot
    // simulator/planner crates. The Windows default executable stack is too
    // small for deep but finite exact-search replays in the oracle tools.
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-arg-bin=oracle_lab=/STACK:8388608");
        println!("cargo:rustc-link-arg-bin=oracle_lab_service=/STACK:8388608");
    }
}
