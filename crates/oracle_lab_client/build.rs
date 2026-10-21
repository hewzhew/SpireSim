use std::path::PathBuf;

const BUILD_INPUTS: &str =
    include_str!("../oracle_artifact_contract/build-inputs/oracle-client.txt");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR")
            .expect("Cargo always provides CARGO_MANIFEST_DIR to build scripts"),
    );
    let repository = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("oracle client package lives below <repository>/crates");
    for input in BUILD_INPUTS
        .lines()
        .map(str::trim)
        .filter(|input| !input.is_empty() && !input.starts_with('#'))
    {
        println!(
            "cargo:rerun-if-changed={}",
            repository.join(input).display()
        );
    }
}
