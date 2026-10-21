use clap::{ArgGroup, Parser};
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};
use sts_java_parity_capture::{
    capture_with_io, derive_fresh_capture_paths, CaptureConfig, CapturePaths,
};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "sts_java_parity_capture")]
#[command(about = "Capture validated observe-only CommunicationMod parity frames")]
#[command(group(
    ArgGroup::new("destination")
        .required(true)
        .args(["output", "output_dir"])
))]
struct Cli {
    /// Fresh JSONL artifact path. Existing files are never overwritten.
    #[arg(long, requires = "receipt", conflicts_with = "output_dir")]
    output: Option<PathBuf>,

    /// Fresh completion-receipt path. It is created only after all frames pass.
    #[arg(long, requires = "output", conflicts_with = "output_dir")]
    receipt: Option<PathBuf>,

    /// Directory in which unique artifact and receipt paths are derived.
    #[arg(long, conflicts_with_all = ["output", "receipt"])]
    output_dir: Option<PathBuf>,

    /// Safe filename label used with --output-dir.
    #[arg(long, requires = "output_dir")]
    label: Option<String>,

    /// Number of stable state frames to capture.
    #[arg(long, default_value_t = 2)]
    frames: usize,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let capture_id = Uuid::new_v4();
    let paths = match resolve_paths(&cli, capture_id) {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("parity capture failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut protocol_output = stdout.lock();
    let config = CaptureConfig {
        artifact_path: paths.artifact_path,
        receipt_path: paths.receipt_path,
        frame_count: cli.frames,
    };

    match capture_with_io(&config, &mut input, &mut protocol_output, capture_id) {
        Ok(receipt) => {
            eprintln!(
                "captured {} validated parity frames in {}",
                receipt.captured_frames, receipt.artifact.path
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("parity capture failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn resolve_paths(cli: &Cli, capture_id: Uuid) -> Result<CapturePaths, String> {
    if let Some(directory) = &cli.output_dir {
        let created_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock precedes UNIX epoch: {error}"))?
            .as_millis();
        return derive_fresh_capture_paths(
            directory,
            cli.label.as_deref().unwrap_or("capture"),
            capture_id,
            created_at_unix_ms,
        );
    }
    Ok(CapturePaths {
        artifact_path: cli
            .output
            .clone()
            .ok_or_else(|| "--output is required without --output-dir".to_owned())?,
        receipt_path: cli
            .receipt
            .clone()
            .ok_or_else(|| "--receipt is required with --output".to_owned())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_directory_mode_needs_no_per_capture_receipt_argument() {
        let cli = Cli::try_parse_from(["capture", "--output-dir", "captures"]).unwrap();
        let capture_id = Uuid::parse_str("11111111-2222-4333-8444-555555555555").unwrap();
        let paths = resolve_paths(&cli, capture_id).unwrap();
        let artifact_name = paths.artifact_path.file_name().unwrap().to_string_lossy();

        assert_eq!(
            paths.artifact_path.parent(),
            Some(PathBuf::from("captures").as_path())
        );
        assert!(
            artifact_name.starts_with("capture-")
                && artifact_name.ends_with("-11111111222243338444555555555555.jsonl"),
            "{artifact_name}"
        );
    }

    #[test]
    fn explicit_output_mode_still_requires_a_receipt() {
        let error = Cli::try_parse_from(["capture", "--output", "capture.jsonl"])
            .err()
            .unwrap();

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }
}
