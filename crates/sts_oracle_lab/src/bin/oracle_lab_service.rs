//! Thin resident compute host for one exact oracle workspace.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use clap::Parser;
use sts_oracle_runtime::runtime::branch::{
    load_oracle_analysis_workspace_v1, resolve_owned_resident_workspace,
    serve_oracle_analysis_tcp_v1,
};

#[derive(Debug, Parser)]
#[command(
    name = "oracle_lab_service",
    about = "Dedicated resident compute host for one oracle workspace"
)]
struct Cli {
    #[arg(long, hide = true)]
    canonical_oracle: bool,
    #[arg(long)]
    workspace: PathBuf,
    #[arg(long)]
    endpoint: PathBuf,
    #[arg(long, default_value = "127.0.0.1:0")]
    listen: SocketAddr,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();
    validate_canonical_launch(cli.canonical_oracle)?;
    let workspace_path =
        resolve_owned_resident_workspace(&cli.workspace, &repository_root().join(".oracle-lab"))?;
    let workspace = load_oracle_analysis_workspace_v1(&workspace_path)?;
    serve_oracle_analysis_tcp_v1(
        &workspace_path,
        workspace,
        cli.listen,
        &absolute_from_repository(&cli.endpoint),
    )?;
    Ok(())
}

fn absolute_from_repository(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repository_root().join(path)
    }
}

fn validate_canonical_launch(canonical_oracle: bool) -> Result<(), String> {
    if !canonical_oracle {
        return Err(
            "oracle_lab_service is an internal resident host; start it with `.\\scripts\\ol-live.ps1 start`"
                .to_string(),
        );
    }
    const REQUIRED_PROFILE: &str = "release";
    const BUILT_PROFILE: &str = env!("STS_CARGO_PROFILE");
    if BUILT_PROFILE != REQUIRED_PROFILE {
        return Err(format!(
            "oracle_lab_service was built with profile `{BUILT_PROFILE}`; expected `{REQUIRED_PROFILE}`"
        ));
    }
    let image_directory = repository_root()
        .join(".oracle-lab")
        .join("hosts")
        .canonicalize()
        .map_err(|error| format!("resident-host image directory is missing: {error}"))?;
    let current = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("failed to identify resident oracle host: {error}"))?;
    let valid_name = current
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("oracle_lab_service-"));
    if current.parent() != Some(image_directory.as_path()) || !valid_name {
        return Err(format!(
            "resident oracle host refuses mutable or foreign artifact {}; expected an immutable image below {}",
            current.display(),
            image_directory.display()
        ));
    }
    Ok(())
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("STS_REPOSITORY_ROOT"))
}
