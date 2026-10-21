//! Optional diagnostic artifact for the existing CLI, not a replay surface.
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::PathBuf, process::ExitCode};
use sts_parity_lockstep::{timing_span_v1, TimingCaptureV1};

pub fn run(command: impl FnOnce() -> ExitCode) -> ExitCode {
    let Some(path) = std::env::var_os("STS_LOCKSTEP_PROFILE") else {
        return command();
    };
    // A spawned resident host must not try to open its parent's output file.
    std::env::remove_var("STS_LOCKSTEP_PROFILE");
    let output = PathBuf::from(path);
    // Reserve before any Java mutation; an existing artifact is never replaced.
    let prepared = (|| {
        if !output.is_absolute() {
            return Err("STS_LOCKSTEP_PROFILE requires a new absolute output path".to_owned());
        }
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let executable_hash = format!(
            "{:x}",
            Sha256::digest(fs::read(executable).map_err(|e| e.to_string())?)
        );
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(|e| format!("cannot reserve timing report: {e}"))?;
        let capture = TimingCaptureV1::start()?;
        Ok::<_, String>((file, capture, executable_hash))
    })();
    let (mut file, capture, executable_hash) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("profile rejected: {error}");
            return ExitCode::from(2);
        }
    };
    let exit = {
        let _timing = timing_span_v1("command");
        command()
    };
    let report = serde_json::json!({
        "schema": "sts-lockstep-profile-artifact-v1",
        "producer": {
            "package_version": env!("CARGO_PKG_VERSION"),
            "executable_sha256": executable_hash,
        },
        "arguments": std::env::args().skip(1).collect::<Vec<_>>(),
        "command_succeeded": exit == ExitCode::SUCCESS,
        "timing": capture.finish(),
    });
    let saved = serde_json::to_writer(&mut file, &report)
        .map_err(|e| e.to_string())
        .and_then(|_| file.flush().map_err(|e| e.to_string()));
    if let Err(error) = saved {
        // A report is not evidence and cannot change the completed trajectory.
        eprintln!("timing report could not be saved: {error}");
        return ExitCode::FAILURE;
    }
    eprintln!("timing report saved | diagnostic only");
    exit
}
