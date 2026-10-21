fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Cargo exposes custom profiles inherited from `dev` as PROFILE=debug.
    // OUT_DIR retains the actual `<target>/<profile>/build/.../out` segment,
    // so prefer its parent-of-build component and use PROFILE only as a
    // defensive fallback.
    let profile = std::env::var_os("OUT_DIR")
        .map(std::path::PathBuf::from)
        .and_then(|out_dir| {
            out_dir
                .ancestors()
                .find(|path| path.file_name().is_some_and(|name| name == "build"))
                .and_then(std::path::Path::parent)
                .and_then(std::path::Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
        })
        .or_else(|| std::env::var("PROFILE").ok())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=STS_CARGO_PROFILE={profile}");
}
