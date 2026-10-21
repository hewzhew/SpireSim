#[path = "../../tools/rust_build_source.rs"]
mod rust_build_source;

fn main() {
    let profile = std::env::var_os("OUT_DIR")
        .and_then(|out_dir| {
            std::path::Path::new(&out_dir)
                .ancestors()
                .nth(3)
                .and_then(std::path::Path::file_name)
                .map(|name| name.to_string_lossy().into_owned())
        })
        .or_else(|| std::env::var("PROFILE").ok())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=STS_RUN_PLAY_BUILD_PROFILE={profile}");
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    rust_build_source::emit(manifest.parent().unwrap().parent().unwrap(), &profile);
}
