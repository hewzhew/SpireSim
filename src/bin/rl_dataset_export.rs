use std::fs;
use std::io::Write;
use std::path::PathBuf;

use clap::Parser;
use serde::Serialize;
use sts_oracle_runtime::eval::run_control::{
    build_planner_capture_coverage_report, build_planner_capture_dataset, load_session_trace_v1,
};

#[derive(Parser, Debug)]
#[command(
    about = "Export a typed SessionTrace planner capture as a JSON dataset",
    version
)]
struct Args {
    /// Path to one current SessionTraceV1 artifact.
    #[arg(long)]
    input: PathBuf,

    /// Output JSON path. Writes to stdout when omitted.
    #[arg(long)]
    out: Option<PathBuf>,

    /// Optional typed planner-capture coverage report.
    #[arg(long)]
    planner_coverage_out: Option<PathBuf>,
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let trace = load_session_trace_v1(&args.input)?;
    write_json(args.out, &build_planner_capture_dataset(&trace))?;
    if let Some(path) = args.planner_coverage_out {
        write_json(Some(path), &build_planner_capture_coverage_report(&trace))?;
    }
    Ok(())
}

fn write_json<T: Serialize>(output: Option<PathBuf>, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize dataset: {error}"))?;
    if let Some(path) = output {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        fs::write(&path, format!("{text}\n"))
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        eprintln!("wrote planner-capture dataset/report to {}", path.display());
    } else {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(text.as_bytes())
            .and_then(|_| stdout.write_all(b"\n"))
            .map_err(|error| format!("failed to write dataset to stdout: {error}"))?;
    }
    Ok(())
}
