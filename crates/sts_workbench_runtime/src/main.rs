mod actor_control;

use actor_control::{ActorIngressV1, ControlEnvelopeV1, IngressEventV1};
use serde::Serialize;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;
use sts_workbench_runtime::{RuntimeResponseV1, WorkbenchRuntimeV1};

const MAX_REQUEST_BYTES: usize = 64 * 1024;
const USAGE: &str = "usage: sts_workbenchd --status | --stdio | \
                     --bootstrap-natural-neow --seed <u64> --ascension <u8>";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandV1 {
    Status,
    Stdio,
    BootstrapNaturalNeow { seed: u64, ascension: u8 },
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct NaturalNeowControlHandoffV3 {
    schema: &'static str,
    root_id: String,
    runtime_ready_ms: u64,
    adapter_ms: u64,
    state_root_ready_ms: u64,
    candidate_capture_ms: u64,
    total_ms: u64,
}

fn main() -> ExitCode {
    let command = match parse_command(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match command {
        CommandV1::Status => {
            let runtime = WorkbenchRuntimeV1::new();
            match serde_json::to_string(&runtime.status()) {
                Ok(document) => {
                    println!("{document}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("failed to encode workbench status: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        CommandV1::Stdio => serve_stdio(),
        CommandV1::BootstrapNaturalNeow { seed, ascension } => {
            bootstrap_natural_neow_via_actor(seed, ascension)
        }
    }
}

fn parse_command(arguments: impl IntoIterator<Item = String>) -> Result<CommandV1, String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    match arguments.as_slice() {
        [command] if command == "--status" => Ok(CommandV1::Status),
        [command] if command == "--stdio" => Ok(CommandV1::Stdio),
        [command, seed_flag, seed, ascension_flag, ascension]
            if command == "--bootstrap-natural-neow"
                && seed_flag == "--seed"
                && ascension_flag == "--ascension" =>
        {
            let seed = seed
                .parse::<u64>()
                .map_err(|_| "--seed must be an unsigned 64-bit integer".to_owned())?;
            let ascension = ascension
                .parse::<u8>()
                .map_err(|_| "--ascension must be an unsigned 8-bit integer".to_owned())?;
            Ok(CommandV1::BootstrapNaturalNeow { seed, ascension })
        }
        _ => Err("unsupported workbench command".to_owned()),
    }
}

fn bootstrap_natural_neow_via_actor(seed: u64, ascension: u8) -> ExitCode {
    let repository = match std::env::current_dir().and_then(fs::canonicalize) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("failed to resolve workbench repository: {error}");
            return ExitCode::FAILURE;
        }
    };
    match actor_control::request_natural_neow(&repository, seed, ascension) {
        Ok(response) if response.status == "ready" => match natural_neow_handoff(&response)
            .and_then(|handoff| {
                serde_json::to_string(&handoff)
                    .map_err(|error| format!("failed to encode natural Neow handoff: {error}"))
            }) {
            Ok(document) => {
                println!("{document}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("natural Neow bootstrap returned an invalid handoff: {error}");
                ExitCode::FAILURE
            }
        },
        Ok(response) => {
            eprintln!(
                "natural Neow bootstrap was rejected: {}",
                response_error(&response, "actor returned an inconsistent rejection")
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("natural Neow bootstrap could not reach the owning actor: {error}");
            ExitCode::FAILURE
        }
    }
}

fn natural_neow_handoff(
    response: &RuntimeResponseV1,
) -> Result<NaturalNeowControlHandoffV3, String> {
    if response.status != "ready" {
        return Err(response_error(response, "bootstrap did not return ready"));
    }
    let result = response
        .result
        .as_ref()
        .ok_or_else(|| "bootstrap response omitted its result".to_owned())?;
    if result.get("schema").and_then(|value| value.as_str())
        != Some("sts-workbench-bootstrap-natural-neow-response-v2")
    {
        return Err("bootstrap response used an unsupported result schema".to_owned());
    }
    let root_id = result
        .pointer("/root/root_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "bootstrap response omitted its root identity".to_owned())?;
    let timing = |name: &str| {
        result
            .pointer(&format!("/timing_ms/{name}"))
            .and_then(|value| value.as_u64())
            .ok_or_else(|| format!("bootstrap response omitted its {name} timing"))
    };
    Ok(NaturalNeowControlHandoffV3 {
        schema: "sts-workbench-natural-neow-handoff-v3",
        root_id: root_id.to_owned(),
        runtime_ready_ms: timing("runtime_ready")?,
        adapter_ms: timing("adapter")?,
        state_root_ready_ms: timing("state_root_ready")?,
        candidate_capture_ms: timing("candidate_capture")?,
        total_ms: timing("total")?,
    })
}

fn response_error(response: &RuntimeResponseV1, fallback: &str) -> String {
    response
        .error
        .as_ref()
        .map(|error| format!("{}: {}", error.code, error.detail))
        .unwrap_or_else(|| fallback.to_owned())
}

fn serve_stdio() -> ExitCode {
    let repository = match std::env::current_dir().and_then(fs::canonicalize) {
        Ok(repository) => repository,
        Err(error) => {
            eprintln!("failed to resolve workbench repository: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut stdout = io::stdout().lock();
    let mut runtime = match WorkbenchRuntimeV1::try_new_owned_at(repository.clone()) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("failed to acquire workbench runtime ownership: {error}");
            return ExitCode::FAILURE;
        }
    };
    let ingress = match ActorIngressV1::start(&repository) {
        Ok(ingress) => ingress,
        Err(error) => {
            eprintln!("failed to publish workbench actor ingress: {error}");
            return ExitCode::FAILURE;
        }
    };
    loop {
        match ingress.recv() {
            Ok(IngressEventV1::StdioLine(line)) => {
                if line.len() > MAX_REQUEST_BYTES {
                    eprintln!("workbench request exceeds {MAX_REQUEST_BYTES} bytes");
                    return ExitCode::FAILURE;
                }
                let (response, shutdown) = runtime.handle_json(&line);
                let document = match serde_json::to_string(&response) {
                    Ok(document) => document,
                    Err(error) => {
                        eprintln!("failed to encode workbench response: {error}");
                        return ExitCode::FAILURE;
                    }
                };
                if writeln!(stdout, "{document}")
                    .and_then(|_| stdout.flush())
                    .is_err()
                {
                    return ExitCode::FAILURE;
                }
                if shutdown {
                    return ExitCode::SUCCESS;
                }
            }
            Ok(IngressEventV1::Control(control)) => {
                let response = handle_control(&mut runtime, &control);
                control.respond(response);
            }
            Ok(IngressEventV1::StdinClosed) => return ExitCode::SUCCESS,
            Ok(IngressEventV1::StdinFailed(error)) => {
                eprintln!("failed to read workbench request: {error}");
                return ExitCode::FAILURE;
            }
            Err(error) => {
                eprintln!("workbench actor ingress failed: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
}

fn handle_control(
    runtime: &mut WorkbenchRuntimeV1,
    control: &ControlEnvelopeV1,
) -> RuntimeResponseV1 {
    let request_id = control.request.request_id.clone();
    let encoded = match serde_json::to_string(&control.request) {
        Ok(encoded) => encoded,
        Err(error) => {
            return RuntimeResponseV1::rejected(
                request_id,
                "control_request_invalid",
                error.to_string(),
            )
        }
    };
    let (response, shutdown) = runtime.handle_json(&encoded);
    if shutdown {
        return RuntimeResponseV1::rejected(
            request_id,
            "actor_shutdown_unexpected",
            "the internal bootstrap unexpectedly requested actor shutdown",
        );
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sts_workbench_supervisor::ShellResponseV1;

    #[test]
    fn parses_closed_actor_control_bootstrap_command() {
        assert_eq!(
            parse_command([
                "--bootstrap-natural-neow".to_owned(),
                "--seed".to_owned(),
                "2026090101".to_owned(),
                "--ascension".to_owned(),
                "20".to_owned(),
            ])
            .unwrap(),
            CommandV1::BootstrapNaturalNeow {
                seed: 2026090101,
                ascension: 20,
            }
        );
        assert!(parse_command([
            "--bootstrap-natural-neow".to_owned(),
            "--ascension".to_owned(),
            "20".to_owned(),
            "--seed".to_owned(),
            "2026090101".to_owned(),
        ])
        .is_err());
    }

    #[test]
    fn extracts_only_the_small_durable_handoff() {
        let response = ShellResponseV1::ready(
            "bootstrap-natural-neow-once",
            json!({
                "schema": "sts-workbench-bootstrap-natural-neow-response-v2",
                "root": {"root_id": "state-root-v2-opaque"},
                "timing_ms": {
                    "runtime_ready": 25_000,
                    "adapter": 5_000,
                    "state_root_ready": 30_000,
                    "candidate_capture": 1_337,
                    "total": 31_337,
                },
                "resident": {"large": "private"},
                "comparisons": {"large": "durable"},
            }),
        );

        let handoff = natural_neow_handoff(&response).unwrap();

        assert_eq!(handoff.root_id, "state-root-v2-opaque");
        assert_eq!(handoff.total_ms, 31_337);
        assert_eq!(
            serde_json::to_value(handoff).unwrap(),
            json!({
                "schema": "sts-workbench-natural-neow-handoff-v3",
                "root_id": "state-root-v2-opaque",
                "runtime_ready_ms": 25_000,
                "adapter_ms": 5_000,
                "state_root_ready_ms": 30_000,
                "candidate_capture_ms": 1_337,
                "total_ms": 31_337,
            })
        );
    }

    #[test]
    fn preserves_typed_bootstrap_rejection() {
        let response = ShellResponseV1::rejected(
            "bootstrap-natural-neow-once",
            "runtime_prerequisite_stale",
            "rebuild the runtime closure",
        );

        assert_eq!(
            natural_neow_handoff(&response).unwrap_err(),
            "runtime_prerequisite_stale: rebuild the runtime closure"
        );
    }
}
