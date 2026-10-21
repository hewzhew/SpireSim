use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;
use sts_workbench_runtime::{
    RuntimeOperationV1, RuntimeRequestV1, RuntimeResponseV1, REQUEST_SCHEMA_V1,
};
use uuid::Uuid;

const ENDPOINT_SCHEMA: &str = "sts-workbench-actor-control-endpoint-v1";
const CONTROL_SCHEMA: &str = "sts-workbench-actor-control-request-v1";
const ENDPOINT_FILE: &str = "actor-control-v1.json";
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EndpointV1 {
    schema: String,
    actor_instance_id: String,
    binding_token: String,
    port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ControlRequestV1 {
    schema: String,
    actor_instance_id: String,
    binding_token: String,
    request: RuntimeRequestV1,
}

pub struct ControlEnvelopeV1 {
    pub request: RuntimeRequestV1,
    response: SyncSender<RuntimeResponseV1>,
}

impl ControlEnvelopeV1 {
    pub fn respond(self, response: RuntimeResponseV1) {
        let _ = self.response.send(response);
    }
}

pub enum IngressEventV1 {
    StdioLine(String),
    StdinClosed,
    StdinFailed(String),
    Control(ControlEnvelopeV1),
}

/// Multiplexes MCP stdio and one closed local administration operation into
/// the same actor-owned runtime state machine.
pub struct ActorIngressV1 {
    receiver: Receiver<IngressEventV1>,
    _endpoint: EndpointGuardV1,
}

impl ActorIngressV1 {
    pub fn start(repository: &Path) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let endpoint = EndpointGuardV1::publish(repository, sender.clone())?;
        thread::Builder::new()
            .name("sts-workbench-stdin-reader".to_owned())
            .spawn(move || read_stdin(sender))
            .map_err(|error| format!("failed to start workbench stdin reader: {error}"))?;
        Ok(Self {
            receiver,
            _endpoint: endpoint,
        })
    }

    pub fn recv(&self) -> Result<IngressEventV1, String> {
        self.receiver
            .recv()
            .map_err(|_| "all workbench actor request sources disconnected".to_owned())
    }
}

struct EndpointGuardV1 {
    path: PathBuf,
    actor_instance_id: String,
}

impl EndpointGuardV1 {
    fn publish(repository: &Path, sender: mpsc::Sender<IngressEventV1>) -> Result<Self, String> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("failed to bind actor control endpoint: {error}"))?;
        let actor_instance_id = format!("actor-control-v1-{}", Uuid::new_v4().simple());
        let endpoint = EndpointV1 {
            schema: ENDPOINT_SCHEMA.to_owned(),
            actor_instance_id: actor_instance_id.clone(),
            binding_token: Uuid::new_v4().simple().to_string(),
            port: listener
                .local_addr()
                .map_err(|error| format!("failed to inspect actor control endpoint: {error}"))?
                .port(),
        };
        let path = endpoint_path(repository);
        publish_endpoint(&path, &endpoint)?;
        let guard = Self {
            path,
            actor_instance_id: actor_instance_id.clone(),
        };
        thread::Builder::new()
            .name("sts-workbench-actor-control".to_owned())
            .spawn(move || serve(listener, endpoint, sender))
            .map_err(|error| format!("failed to start actor control endpoint: {error}"))?;
        Ok(guard)
    }
}

impl Drop for EndpointGuardV1 {
    fn drop(&mut self) {
        let Ok(bytes) = fs::read(&self.path) else {
            return;
        };
        let Ok(endpoint) = serde_json::from_slice::<EndpointV1>(&bytes) else {
            return;
        };
        if endpoint.actor_instance_id == self.actor_instance_id {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub fn request_natural_neow(
    repository: &Path,
    seed: u64,
    ascension: u8,
) -> Result<RuntimeResponseV1, String> {
    let endpoint = read_endpoint(repository)?;
    let request_id = format!("actor-control-v1-{}", Uuid::new_v4().simple());
    let request = ControlRequestV1 {
        schema: CONTROL_SCHEMA.to_owned(),
        actor_instance_id: endpoint.actor_instance_id.clone(),
        binding_token: endpoint.binding_token.clone(),
        request: RuntimeRequestV1 {
            schema: REQUEST_SCHEMA_V1.to_owned(),
            request_id: request_id.clone(),
            operation: RuntimeOperationV1::BootstrapNaturalNeow { seed, ascension },
        },
    };
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, endpoint.port);
    let mut stream =
        TcpStream::connect_timeout(&address.into(), Duration::from_secs(2)).map_err(|error| {
            format!("no live actor accepted the request: {error}; call service_status once")
        })?;
    stream
        .set_read_timeout(Some(REQUEST_TIMEOUT))
        .and_then(|_| stream.set_write_timeout(Some(Duration::from_secs(5))))
        .map_err(|error| format!("failed to configure actor control stream: {error}"))?;
    write_line(&mut stream, &request, MAX_REQUEST_BYTES)?;
    let response: RuntimeResponseV1 = read_line(&mut BufReader::new(stream), MAX_RESPONSE_BYTES)?;
    if response.request_id != request_id {
        return Err("actor control response was not bound to its request".to_owned());
    }
    Ok(response)
}

fn read_stdin(sender: mpsc::Sender<IngressEventV1>) {
    for line in io::stdin().lock().lines() {
        match line {
            Ok(line) => {
                if sender.send(IngressEventV1::StdioLine(line)).is_err() {
                    return;
                }
            }
            Err(error) => {
                let _ = sender.send(IngressEventV1::StdinFailed(error.to_string()));
                return;
            }
        }
    }
    let _ = sender.send(IngressEventV1::StdinClosed);
}

fn serve(listener: TcpListener, endpoint: EndpointV1, sender: mpsc::Sender<IngressEventV1>) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
        let request: ControlRequestV1 =
            match read_line(&mut BufReader::new(&mut stream), MAX_REQUEST_BYTES) {
                Ok(request) => request,
                Err(_) => continue,
            };
        let request_id = request.request.request_id.clone();
        if !valid_request(&request, &endpoint) {
            let _ = write_line(
                &mut stream,
                &RuntimeResponseV1::rejected(
                    request_id,
                    "invalid_control_request",
                    "the request is not bound to this actor or its closed operation",
                ),
                MAX_RESPONSE_BYTES,
            );
            continue;
        }
        let (response, receiver) = mpsc::sync_channel(1);
        if sender
            .send(IngressEventV1::Control(ControlEnvelopeV1 {
                request: request.request,
                response,
            }))
            .is_err()
        {
            return;
        }
        let response = receiver.recv_timeout(REQUEST_TIMEOUT).unwrap_or_else(|_| {
            RuntimeResponseV1::rejected(
                request_id,
                "actor_unavailable",
                "the owning actor stopped before completing the control request",
            )
        });
        let _ = write_line(&mut stream, &response, MAX_RESPONSE_BYTES);
    }
}

fn valid_request(request: &ControlRequestV1, endpoint: &EndpointV1) -> bool {
    request.schema == CONTROL_SCHEMA
        && request.actor_instance_id == endpoint.actor_instance_id
        && request.binding_token == endpoint.binding_token
        && request.request.schema == REQUEST_SCHEMA_V1
        && !request.request.request_id.trim().is_empty()
        && matches!(
            request.request.operation,
            RuntimeOperationV1::BootstrapNaturalNeow { .. }
        )
}

fn read_line<T: for<'de> Deserialize<'de>>(
    reader: &mut impl BufRead,
    maximum: usize,
) -> Result<T, String> {
    let mut bytes = Vec::new();
    let mut limited = std::io::Read::take(&mut *reader, (maximum + 1) as u64);
    limited
        .read_until(b'\n', &mut bytes)
        .map_err(|error| format!("failed to read actor control message: {error}"))?;
    if bytes.is_empty() || bytes.len() > maximum || bytes.pop() != Some(b'\n') {
        return Err("actor control message is empty, oversized, or unterminated".to_owned());
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("actor control message is invalid: {error}"))
}

fn write_line(
    stream: &mut TcpStream,
    value: &impl Serialize,
    maximum: usize,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode actor control message: {error}"))?;
    if bytes.len() + 1 > maximum {
        return Err("actor control message exceeds its size contract".to_owned());
    }
    bytes.push(b'\n');
    stream
        .write_all(&bytes)
        .and_then(|_| stream.flush())
        .map_err(|error| format!("failed to write actor control message: {error}"))
}

fn read_endpoint(repository: &Path) -> Result<EndpointV1, String> {
    let bytes = fs::read(endpoint_path(repository)).map_err(|error| {
        format!("no actor control endpoint is published: {error}; call service_status once")
    })?;
    let endpoint: EndpointV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("actor control endpoint is invalid: {error}"))?;
    if endpoint.schema != ENDPOINT_SCHEMA
        || endpoint.port == 0
        || !endpoint.actor_instance_id.starts_with("actor-control-v1-")
        || Uuid::parse_str(&endpoint.binding_token).is_err()
    {
        return Err("actor control endpoint has an invalid identity".to_owned());
    }
    Ok(endpoint)
}

fn publish_endpoint(path: &Path, endpoint: &EndpointV1) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "actor control endpoint has no parent".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create workbench runtime directory: {error}"))?;
    let staged = parent.join(format!(".actor-control-{}.tmp", Uuid::new_v4().simple()));
    fs::write(
        &staged,
        serde_json::to_vec(endpoint)
            .map_err(|error| format!("failed to encode actor control endpoint: {error}"))?,
    )
    .map_err(|error| format!("failed to stage actor control endpoint: {error}"))?;
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("failed to replace stale actor control endpoint: {error}"))?;
    }
    fs::rename(staged, path)
        .map_err(|error| format!("failed to publish actor control endpoint: {error}"))
}

fn endpoint_path(repository: &Path) -> PathBuf {
    repository
        .join(".oracle-lab")
        .join("workbench-runtime")
        .join(ENDPOINT_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn control_client_reaches_only_the_published_closed_operation() {
        let repository = std::env::temp_dir().join(format!(
            "sts-workbench-control-test-{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&repository).unwrap();
        let (sender, events) = mpsc::channel();
        let endpoint = EndpointGuardV1::publish(&repository, sender).unwrap();
        let client_repository = repository.clone();
        let client = thread::spawn(move || {
            request_natural_neow(&client_repository, 2026090304, 20).unwrap()
        });
        let control = match events.recv_timeout(Duration::from_secs(2)).unwrap() {
            IngressEventV1::Control(control) => control,
            _ => panic!("control endpoint emitted the wrong event"),
        };
        assert!(matches!(
            control.request.operation,
            RuntimeOperationV1::BootstrapNaturalNeow {
                seed: 2026090304,
                ascension: 20
            }
        ));
        let request_id = control.request.request_id.clone();
        control.respond(RuntimeResponseV1::ready(
            request_id,
            json!({"accepted": true}),
        ));
        assert_eq!(
            client.join().unwrap().result,
            Some(json!({"accepted": true}))
        );
        drop(endpoint);
        assert!(!endpoint_path(&repository).exists());
        fs::remove_dir_all(repository).unwrap();
    }
}
