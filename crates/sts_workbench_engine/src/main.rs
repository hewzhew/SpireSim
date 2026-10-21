use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use sts_workbench_engine::EngineRuntimeV1;
use sts_workbench_supervisor::{
    EngineContractV1, EngineHelloV1, EngineOperationV1, EngineOutcomeV1, EngineRequestV1,
    EngineResponseV1, ENGINE_REQUEST_SCHEMA_V1, MAX_ENGINE_FRAME_BYTES_V1, SUPERVISOR_PROTOCOL_V2,
};

struct ConfigurationV1 {
    repository: PathBuf,
    java_jar: PathBuf,
    mod_jar: PathBuf,
}

fn main() -> ExitCode {
    let configuration = match parse_arguments() {
        Ok(configuration) => configuration,
        Err(detail) => {
            eprintln!("{detail}");
            return ExitCode::from(2);
        }
    };
    match serve(configuration) {
        Ok(()) => ExitCode::SUCCESS,
        Err(detail) => {
            eprintln!("workbench Engine stopped: {detail}");
            ExitCode::FAILURE
        }
    }
}

fn parse_arguments() -> Result<ConfigurationV1, String> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--stdio")) {
        return Err(usage());
    }
    let mut repository = None;
    let mut java_jar = None;
    let mut mod_jar = None;
    while let Some(name) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("missing value for {}; {}", name.to_string_lossy(), usage()))?;
        match name.to_str() {
            Some("--repository") if repository.is_none() => repository = Some(value),
            Some("--java-jar") if java_jar.is_none() => java_jar = Some(value),
            Some("--mod-jar") if mod_jar.is_none() => mod_jar = Some(value),
            _ => return Err(usage()),
        }
    }
    Ok(ConfigurationV1 {
        repository: canonical_directory(required(repository, "--repository")?)?,
        java_jar: canonical_file(required(java_jar, "--java-jar")?)?,
        mod_jar: canonical_file(required(mod_jar, "--mod-jar")?)?,
    })
}

fn required(value: Option<OsString>, name: &str) -> Result<PathBuf, String> {
    value
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {name}; {}", usage()))
}

fn usage() -> String {
    "usage: sts_workbench_engine --stdio --repository <path> --java-jar <path> --mod-jar <path>"
        .to_owned()
}

fn canonical_directory(path: PathBuf) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(&path)
        .map_err(|error| format!("failed to resolve repository {}: {error}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!(
            "repository is not a directory: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn canonical_file(path: PathBuf) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(&path).map_err(|error| {
        format!(
            "failed to resolve contract artifact {}: {error}",
            path.display()
        )
    })?;
    if !canonical.is_file() {
        return Err(format!(
            "contract artifact is not a file: {}",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn serve(configuration: ConfigurationV1) -> Result<(), String> {
    let contract = current_contract(&configuration)?;
    let mut stdout = io::stdout().lock();
    write_frame(&mut stdout, &EngineHelloV1::new(contract))?;

    let mut reader = BufReader::new(io::stdin().lock());
    let mut bound_epoch = None;
    let mut engine = EngineRuntimeV1::new(&configuration.repository);
    loop {
        let Some(line) = read_bounded_line(&mut reader)? else {
            return Ok(());
        };
        let request: EngineRequestV1 = serde_json::from_str(&line)
            .map_err(|error| format!("invalid Supervisor request: {error}"))?;
        let outcome = if request.schema != ENGINE_REQUEST_SCHEMA_V1 {
            EngineOutcomeV1::Rejected {
                code: "request_schema_mismatch".to_owned(),
            }
        } else {
            match request.operation {
                EngineOperationV1::Bind if bound_epoch.is_none() && request.engine_epoch != 0 => {
                    match engine.bind_epoch(request.engine_epoch) {
                        Ok(()) => {
                            bound_epoch = Some(request.engine_epoch);
                            EngineOutcomeV1::Bound
                        }
                        Err(_) => EngineOutcomeV1::Rejected {
                            code: "engine_epoch_mismatch".to_owned(),
                        },
                    }
                }
                EngineOperationV1::Execute { input }
                    if bound_epoch == Some(request.engine_epoch) =>
                {
                    engine.execute(input)
                }
                EngineOperationV1::ResumeSupervisorCall { call_id, outcome }
                    if bound_epoch == Some(request.engine_epoch) =>
                {
                    engine.resume_supervisor_call(&call_id, outcome)
                }
                EngineOperationV1::Shutdown
                    if bound_epoch == Some(request.engine_epoch)
                        || (bound_epoch.is_none() && request.engine_epoch == 0) =>
                {
                    write_frame(
                        &mut stdout,
                        &EngineResponseV1::new(
                            request.request_id,
                            request.engine_epoch,
                            EngineOutcomeV1::Stopped,
                        ),
                    )?;
                    return Ok(());
                }
                _ => EngineOutcomeV1::Rejected {
                    code: "engine_epoch_mismatch".to_owned(),
                },
            }
        };
        write_frame(
            &mut stdout,
            &EngineResponseV1::new(request.request_id, request.engine_epoch, outcome),
        )?;
    }
}

fn current_contract(configuration: &ConfigurationV1) -> Result<EngineContractV1, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("failed to resolve Engine executable: {error}"))?;
    let protocol = u16::try_from(sts_java_parity_protocol::PROTOCOL_MINOR)
        .map_err(|_| "Java protocol minor does not fit the Supervisor contract".to_owned())?;
    Ok(EngineContractV1 {
        image_id: file_sha256(&executable, false)?,
        supervisor_protocol: SUPERVISOR_PROTOCOL_V2,
        java_build_id: file_sha256(&configuration.java_jar, true)?,
        mod_build_id: file_sha256(&configuration.mod_jar, true)?,
        minimum_java_protocol: protocol,
        maximum_java_protocol: protocol,
    })
}

fn file_sha256(path: &Path, uppercase: bool) -> Result<String, String> {
    let mut input = File::open(path).map_err(|error| {
        format!(
            "failed to open contract artifact {}: {error}",
            path.display()
        )
    })?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut input, &mut hasher).map_err(|error| {
        format!(
            "failed to hash contract artifact {}: {error}",
            path.display()
        )
    })?;
    let digest = hasher.finalize();
    Ok(if uppercase {
        format!("{digest:X}")
    } else {
        format!("{digest:x}")
    })
}

fn write_frame(output: &mut impl Write, value: &impl serde::Serialize) -> Result<(), String> {
    let encoded = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode Engine response: {error}"))?;
    if encoded.len() + 1 > MAX_ENGINE_FRAME_BYTES_V1 {
        return Err("Engine response exceeds the machine frame limit".to_owned());
    }
    output
        .write_all(&encoded)
        .and_then(|_| output.write_all(b"\n"))
        .and_then(|_| output.flush())
        .map_err(|error| format!("failed to write Engine response: {error}"))
}

fn read_bounded_line<R: Read>(reader: &mut BufReader<R>) -> Result<Option<String>, String> {
    let mut bytes = Vec::new();
    loop {
        let (consumed, found_newline) = {
            let available = reader
                .fill_buf()
                .map_err(|error| format!("failed to read Supervisor request: {error}"))?;
            if available.is_empty() {
                if bytes.is_empty() {
                    return Ok(None);
                }
                return Err("Supervisor request ended without a newline".to_owned());
            }
            let consumed = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |index| index + 1);
            if bytes.len() + consumed > MAX_ENGINE_FRAME_BYTES_V1 {
                return Err("Supervisor request exceeds the machine frame limit".to_owned());
            }
            bytes.extend_from_slice(&available[..consumed]);
            (consumed, available[consumed - 1] == b'\n')
        };
        reader.consume(consumed);
        if found_newline {
            return String::from_utf8(bytes)
                .map(Some)
                .map_err(|error| format!("Supervisor request is not UTF-8: {error}"));
        }
    }
}
