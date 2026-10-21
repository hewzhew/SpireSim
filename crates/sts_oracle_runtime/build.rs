use std::path::PathBuf;

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
        .expect("oracle runtime package lives below <repository>/crates")
        .to_path_buf();
    println!(
        "cargo:rustc-env=STS_REPOSITORY_ROOT={}",
        repository_root.display()
    );
}
