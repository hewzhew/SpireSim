use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use sts_java_parity_runtime::{
    launch_java_parity_runtime_v1, JavaParityInitialBoundaryV1, JavaParityLaunchProfileV1,
    JavaParityLaunchRequestV1, JavaParityStorageSeedV1,
};
use sts_parity_lockstep::ProducerIdentityV1;
use sts_parity_lockstep_java::{producer_identity_from_live_v1, LeasedJavaLockstepTransportV1};
use sts_workbench_supervisor::{
    ExecutableImageStoreV1, OwnedStorageRegistryV1, RepositoryLifecycleOwnerV1,
    RepositoryLifecycleRoleV1,
};
use uuid::Uuid;

pub const JAVA_ORACLE_HOST_ENDPOINT_SCHEMA_V2: &str = "sts-java-oracle-host-endpoint-v2";
const JAVA_ORACLE_HOST_POINTER_SCHEMA_V1: &str = "sts-java-oracle-host-pointer-v1";
const JAVA_ORACLE_HOST_IMAGE_PREFIX_V1: &str = "sts-parity-oracle-host";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaOracleHostEndpointV1 {
    pub schema: String,
    pub host_id: String,
    pub live_output: PathBuf,
    pub host_lease: PathBuf,
    pub worker_lease: PathBuf,
    pub stop_request: PathBuf,
    pub producer: ProducerIdentityV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct JavaOracleHostPointerV1 {
    schema: String,
    host_id: String,
    endpoint: PathBuf,
}

struct CurrentHostPointerGuardV1 {
    path: PathBuf,
    host_id: String,
}

impl Drop for CurrentHostPointerGuardV1 {
    fn drop(&mut self) {
        let _ = remove_current_if_owned(&self.path, &self.host_id);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaOracleHostRunReportV1 {
    pub startup: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaOracleHostLaunchReportV1 {
    pub startup: Duration,
    pub diagnostic_id: String,
}

struct HostLaunchDiagnostics {
    id: String,
    directory: PathBuf,
}

impl HostLaunchDiagnostics {
    fn create(repository: &Path) -> Result<Self, String> {
        let root = repository.join(".oracle-lab/parity-host/launch-logs");
        fs::create_dir_all(&root).map_err(|e| format!("failed to create host log store: {e}"))?;
        loop {
            let id = format!("L{}", &Uuid::new_v4().simple().to_string()[..12]);
            let directory = root.join(&id);
            match fs::create_dir(&directory) {
                Ok(()) => return Ok(Self { id, directory }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("failed to reserve host launch logs: {e}")),
            }
        }
    }

    fn spawn(&self, command: &mut Command) -> Result<Child, String> {
        let stdout = File::create(self.directory.join("stdout.log"))
            .map_err(|e| format!("failed to open host stdout log: {e}"))?;
        let stderr = File::create(self.directory.join("stderr.log"))
            .map_err(|e| format!("failed to open host stderr log: {e}"))?;
        // A resident child must not retain the caller's pipes. Otherwise tools
        // awaiting EOF keep the completed launch command open for the JVM's
        // entire lifetime. Logs outlive both the launcher and isolated storage.
        command.stdin(Stdio::null()).stdout(stdout).stderr(stderr);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            use windows_sys::Win32::{
                Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE},
                System::Console::{
                    GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
                },
            };
            // Windows CreateProcess also inherits handles not named in its
            // STARTUPINFO. Keep our caller's pipes usable here, but prevent
            // incidental inheritance into the resident process. Explicit Rust
            // Stdio::inherit uses a fresh inheritable duplicate when requested.
            for stream in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                // SAFETY: GetStdHandle returns borrowed process handles; changing
                // their inheritance flag neither closes nor redirects them.
                unsafe {
                    let handle = GetStdHandle(stream);
                    if !handle.is_null()
                        && handle != INVALID_HANDLE_VALUE
                        && SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) == 0
                    {
                        return Err(format!(
                            "failed to detach host launch pipe: {}",
                            std::io::Error::last_os_error()
                        ));
                    }
                }
            }
            command.creation_flags(0x0800_0000);
        }
        command
            .spawn()
            .map_err(|e| format!("failed to spawn Java oracle host image: {e}"))
    }

    fn failure(&self, reason: &str) -> String {
        let _ = fs::write(self.directory.join("failure.txt"), reason);
        let tail = (|| -> std::io::Result<String> {
            let mut file = File::open(self.directory.join("stderr.log"))?;
            let len = file.metadata()?.len();
            file.seek(SeekFrom::Start(len.saturating_sub(2048)))?;
            let mut bytes = Vec::new();
            file.take(2048).read_to_end(&mut bytes)?;
            let text = String::from_utf8_lossy(&bytes);
            let lines = text.lines().rev().take(4).collect::<Vec<_>>();
            Ok(lines
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
                .chars()
                .take(800)
                .collect())
        })()
        .unwrap_or_default();
        if tail.trim().is_empty() {
            format!("{reason} | logs {}", self.id)
        } else {
            format!("{reason} | logs {}\n{}", self.id, tail.trim())
        }
    }
}

/// Launch the durable host from a content-addressed copy of the current
/// executable and wait only until it is ready or has failed. After admission,
/// Windows locks only the immutable image, leaving Cargo's canonical worker
/// binary replaceable.
pub fn spawn_java_oracle_host_image_v1(
    repository: &Path,
    game_directory: &Path,
    storage_seed: Option<&Path>,
) -> Result<JavaOracleHostLaunchReportV1, String> {
    let started = Instant::now();
    let repository = canonical_directory(repository, "repository")?;
    let game_directory = canonical_directory(game_directory, "game directory")?;
    let source = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|error| format!("failed to resolve parity executable: {error}"))?;
    let store = ExecutableImageStoreV1::new(
        repository
            .join(".oracle-lab")
            .join("parity-host")
            .join("images"),
        JAVA_ORACLE_HOST_IMAGE_PREFIX_V1,
    )
    .map_err(|error| error.to_string())?;
    let image = store
        .materialize(&source)
        .map_err(|error| error.to_string())?;
    reclaim_old_host_images(
        image
            .executable()
            .parent()
            .expect("materialized host image has a parent"),
        image.executable(),
    );
    let mut command = image.command().map_err(|error| error.to_string())?;
    if check_current_java_oracle_v1(&repository).is_ok() {
        return Err("Java oracle host is already ready".to_owned());
    }
    command
        .arg("host")
        .arg("--repository")
        .arg(&repository)
        .arg("--game-directory")
        .arg(&game_directory);
    if let Some(storage_seed) = storage_seed {
        command.arg("--storage-seed").arg(storage_seed);
    }
    let diagnostics = HostLaunchDiagnostics::create(&repository)?;
    write_json_create_new(
        &diagnostics.directory.join("launch.json"),
        &serde_json::json!({
            "schema": "sts-java-oracle-host-launch-v1",
            "runner_version": env!("CARGO_PKG_VERSION"),
            "executable_image": image.executable(),
            "image_id": image.image_id(),
            "game_directory": game_directory,
            "storage_seed": storage_seed,
        }),
    )
    .map_err(|e| diagnostics.failure(&e))?;
    command.current_dir(&repository);
    let mut child = diagnostics
        .spawn(&mut command)
        .map_err(|e| diagnostics.failure(&e))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if check_current_java_oracle_v1(&repository).is_ok() {
            let endpoint =
                load_current_endpoint(&repository).map_err(|e| diagnostics.failure(&e))?;
            write_json_create_new(
                &diagnostics.directory.join("ready.json"),
                &serde_json::json!({
                    "host_id": endpoint.host_id,
                    "startup_ms": started.elapsed().as_millis(),
                }),
            )
            .map_err(|e| diagnostics.failure(&e))?;
            drop(child);
            return Ok(JavaOracleHostLaunchReportV1 {
                startup: started.elapsed(),
                diagnostic_id: diagnostics.id,
            });
        }
        if let Some(status) = child.try_wait().map_err(|error| {
            diagnostics.failure(&format!(
                "failed to inspect Java oracle host image: {error}"
            ))
        })? {
            return Err(
                diagnostics.failure(&format!("Java oracle host exited before ready ({status})"))
            );
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                diagnostics.failure("Java oracle host did not become ready within 60 seconds")
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn run_java_oracle_host_v1(
    repository: &Path,
    game_directory: &Path,
    storage_seed: Option<&Path>,
    on_ready: impl FnOnce(Duration),
) -> Result<JavaOracleHostRunReportV1, String> {
    let repository = canonical_directory(repository, "repository")?;
    let game_directory = canonical_directory(game_directory, "game directory")?;
    validate_current_host_image(&repository)?;
    let lifecycle = RepositoryLifecycleOwnerV1::acquire_for(
        &repository,
        RepositoryLifecycleRoleV1::ParityOracleHost,
    )
    .map_err(|error| format!("runtime ownership unavailable: {error}"))?;
    let storage_registry = OwnedStorageRegistryV1::new(&lifecycle, &game_directory);
    let storage_seed = storage_seed
        .map(|path| canonical_directory(path, "Java storage seed"))
        .transpose()?;
    if storage_seed.as_deref() == Some(game_directory.as_path()) {
        return Err("Java oracle host cannot seed from the installed game directory".to_owned());
    }
    let (storage_seed, initial_boundary) = match storage_seed.as_deref() {
        Some(path) => (
            JavaParityStorageSeedV1::Native(path),
            JavaParityInitialBoundaryV1::NeutralContinue,
        ),
        None => (
            JavaParityStorageSeedV1::Fresh,
            JavaParityInitialBoundaryV1::FreshStart,
        ),
    };
    let (mut prepared, startup) = launch_java_parity_runtime_v1(JavaParityLaunchRequestV1 {
        storage_registry: &storage_registry,
        storage_seed,
        profile: JavaParityLaunchProfileV1::RunControl,
        initial_boundary,
    })?;
    let live = prepared.status()?;
    let producer = producer_identity_from_live_v1(&live);
    let live_output = prepared.live_output().to_path_buf();
    let mut owner = prepared.into_owner().map_err(|error| error.to_string())?;

    let host_root = repository.join(".oracle-lab").join("parity-host");
    let host_id = format!("host-{}", Uuid::new_v4().simple());
    let session = host_root.join("sessions").join(&host_id);
    fs::create_dir_all(&session)
        .map_err(|error| format!("failed to create Java oracle host session: {error}"))?;
    let _host_lease = acquire_host_lease(&session.join("host.lock"))?;
    let endpoint_path = session.join("endpoint.json");
    let endpoint = JavaOracleHostEndpointV1 {
        schema: JAVA_ORACLE_HOST_ENDPOINT_SCHEMA_V2.to_owned(),
        host_id: host_id.clone(),
        live_output,
        host_lease: session.join("host.lock"),
        worker_lease: session.join("worker.lock"),
        stop_request: session.join("stop.request"),
        producer,
    };
    validate_endpoint(&endpoint)?;
    write_json_create_new(&endpoint_path, &endpoint)?;
    let current_path = host_root.join("current.json");
    write_json_replace(
        &current_path,
        &JavaOracleHostPointerV1 {
            schema: JAVA_ORACLE_HOST_POINTER_SCHEMA_V1.to_owned(),
            host_id: host_id.clone(),
            endpoint: endpoint_path,
        },
    )?;
    let _current_guard = CurrentHostPointerGuardV1 {
        path: current_path,
        host_id,
    };
    on_ready(startup);

    let health_interval = Duration::from_secs(1);
    let mut last_health = Instant::now();
    while !endpoint.stop_request.is_file() {
        thread::sleep(Duration::from_millis(100));
        if last_health.elapsed() >= health_interval {
            owner.status().map_err(|error| error.to_string())?;
            last_health = Instant::now();
        }
    }
    owner.shutdown().map_err(|error| error.to_string())?;
    drop(storage_registry);
    drop(lifecycle);
    Ok(JavaOracleHostRunReportV1 { startup })
}

pub fn connect_current_java_oracle_v1(
    repository: &Path,
    action_timeout: Duration,
) -> Result<LeasedJavaLockstepTransportV1, String> {
    let endpoint = load_current_endpoint(repository)?;
    require_live_host_lease(&endpoint)?;
    LeasedJavaLockstepTransportV1::connect(
        endpoint.live_output,
        &endpoint.worker_lease,
        endpoint.producer,
        action_timeout,
    )
    .map_err(|failure| failure.message)
}

pub fn check_current_java_oracle_v1(repository: &Path) -> Result<(), String> {
    let endpoint = load_current_endpoint(repository)?;
    require_live_host_lease(&endpoint)?;
    let live = sts_java_parity_live::status(&endpoint.live_output)?;
    if producer_identity_from_live_v1(&live) != endpoint.producer {
        return Err("Java oracle host producer epoch changed".to_owned());
    }
    Ok(())
}

pub fn request_current_java_oracle_stop_v1(repository: &Path) -> Result<(), String> {
    let endpoint = load_current_endpoint(repository)?;
    require_live_host_lease(&endpoint)?;
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&endpoint.stop_request)
    {
        Ok(mut file) => file
            .write_all(b"stop\n")
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("failed to persist Java oracle stop request: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(format!(
            "failed to create Java oracle stop request: {error}"
        )),
    }
}

fn load_current_endpoint(repository: &Path) -> Result<JavaOracleHostEndpointV1, String> {
    let repository = canonical_directory(repository, "repository")?;
    let current_path = repository
        .join(".oracle-lab")
        .join("parity-host")
        .join("current.json");
    let pointer: JavaOracleHostPointerV1 = read_json(&current_path, "Java oracle host pointer")?;
    if pointer.schema != JAVA_ORACLE_HOST_POINTER_SCHEMA_V1 || pointer.host_id.is_empty() {
        return Err("Java oracle host pointer is invalid".to_owned());
    }
    let endpoint: JavaOracleHostEndpointV1 =
        read_json(&pointer.endpoint, "Java oracle host endpoint")?;
    validate_endpoint(&endpoint)?;
    if endpoint.host_id != pointer.host_id {
        return Err("Java oracle host pointer and endpoint disagree".to_owned());
    }
    Ok(endpoint)
}

fn validate_endpoint(endpoint: &JavaOracleHostEndpointV1) -> Result<(), String> {
    if endpoint.schema != JAVA_ORACLE_HOST_ENDPOINT_SCHEMA_V2
        || endpoint.host_id.is_empty()
        || endpoint.producer.implementation.is_empty()
        || endpoint.producer.version.is_empty()
        || endpoint.producer.session_epoch.is_empty()
        || !endpoint.live_output.is_dir()
        || !endpoint.host_lease.is_file()
        || endpoint.host_lease.parent() != endpoint.worker_lease.parent()
        || endpoint.worker_lease.parent() != endpoint.stop_request.parent()
    {
        return Err("Java oracle host endpoint is invalid".to_owned());
    }
    Ok(())
}

fn acquire_host_lease(path: &Path) -> Result<File, String> {
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| format!("failed to open Java oracle host lease: {error}"))?;
    lease
        .try_lock_exclusive()
        .map_err(|error| format!("failed to acquire Java oracle host lease: {error}"))?;
    Ok(lease)
}

fn require_live_host_lease(endpoint: &JavaOracleHostEndpointV1) -> Result<(), String> {
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&endpoint.host_lease)
        .map_err(|error| format!("failed to open Java oracle host lease: {error}"))?;
    match lease.try_lock_exclusive() {
        Ok(()) => {
            FileExt::unlock(&lease)
                .map_err(|error| format!("failed to release stale host lease probe: {error}"))?;
            Err("Java oracle host pointer is stale; its owner is not running".to_owned())
        }
        Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => Ok(()),
        Err(error) => Err(format!("failed to inspect Java oracle host lease: {error}")),
    }
}

fn remove_current_if_owned(path: &Path, host_id: &str) -> Result<(), String> {
    let pointer = read_json::<JavaOracleHostPointerV1>(path, "Java oracle host pointer")?;
    if pointer.host_id != host_id {
        return Err("Java oracle host lost ownership of its current pointer".to_owned());
    }
    fs::remove_file(path)
        .map_err(|error| format!("failed to remove stopped Java oracle host pointer: {error}"))
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let path =
        fs::canonicalize(path).map_err(|error| format!("failed to resolve {label}: {error}"))?;
    path.is_dir()
        .then_some(path)
        .ok_or_else(|| format!("{label} is not a directory"))
}

fn validate_current_host_image(repository: &Path) -> Result<(), String> {
    let current = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|error| format!("failed to resolve running parity executable: {error}"))?;
    let expected_parent = repository
        .join(".oracle-lab")
        .join("parity-host")
        .join("images");
    let expected_parent = fs::canonicalize(expected_parent)
        .map_err(|error| format!("failed to resolve Java oracle host image store: {error}"))?;
    let filename = current
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Java oracle host image has no UTF-8 filename".to_owned())?;
    if current.parent() != Some(expected_parent.as_path())
        || !filename.starts_with(&format!("{JAVA_ORACLE_HOST_IMAGE_PREFIX_V1}-"))
    {
        return Err(
            "Java oracle host must run from its immutable executable image; use host-launch"
                .to_owned(),
        );
    }
    Ok(())
}

fn reclaim_old_host_images(directory: &Path, active: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == active {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(&format!("{JAVA_ORACLE_HOST_IMAGE_PREFIX_V1}-"))
            || !entry.file_type().is_ok_and(|kind| kind.is_file())
        {
            continue;
        }
        // A running Windows image stays locked and is reclaimed on a later
        // launch; cleanup never blocks admission of the current image.
        let _ = fs::remove_file(path);
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read {label}: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("failed to decode {label}: {error}"))
}

fn write_json_create_new(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode Java oracle host endpoint: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create Java oracle host endpoint: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist Java oracle host endpoint: {error}"))
}

fn write_json_replace(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Java oracle host pointer has no parent".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create Java oracle host directory: {error}"))?;
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode Java oracle host pointer: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|error| format!("failed to open Java oracle host pointer: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist Java oracle host pointer: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resident_host_releases_launcher_pipes_and_keeps_diagnostics() {
        const MODE: &str = "STS_LOCKSTEP_HOST_PIPE_TEST";
        const ROOT: &str = "STS_LOCKSTEP_HOST_PIPE_TEST_ROOT";
        fn child(mode: &str, repository: &Path) -> Command {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "host::tests::resident_host_releases_launcher_pipes_and_keeps_diagnostics",
                    "--nocapture",
                ])
                .env(MODE, mode)
                .env(ROOT, repository);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x0800_0000);
            }
            command
        }
        fn wait_for(path: &Path) -> bool {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !path.exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            path.exists()
        }
        if let Ok(mode) = std::env::var(MODE) {
            let repository = PathBuf::from(std::env::var_os(ROOT).unwrap());
            if mode == "resident" {
                println!("resident stdout evidence");
                eprintln!("resident stderr evidence");
                std::io::stdout().flush().unwrap();
                std::io::stderr().flush().unwrap();
                fs::write(repository.join("resident.ready"), b"ready").unwrap();
                // A deadline prevents orphan test processes after a failed parent.
                let deadline = Instant::now() + Duration::from_secs(15);
                while !repository.join("release").exists() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(10));
                }
                fs::write(repository.join("resident.exited"), b"exited").unwrap();
            } else {
                let logs = HostLaunchDiagnostics::create(&repository).unwrap();
                fs::write(repository.join("diagnostic-id"), &logs.id).unwrap();
                let resident = logs.spawn(&mut child("resident", &repository)).unwrap();
                assert!(wait_for(&repository.join("resident.ready")));
                drop(resident);
            }
            return;
        }

        let repository = std::env::temp_dir().join(format!("host-pipe-test-{}", Uuid::new_v4()));
        fs::create_dir(&repository).unwrap();
        let mut launcher = child("launcher", &repository)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = launcher.stdout.take().unwrap();
        let stderr = launcher.stderr.take().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let readers = [
            (Box::new(stdout) as Box<dyn Read + Send>, send.clone()),
            (Box::new(stderr) as Box<dyn Read + Send>, send),
        ]
        .map(|(mut pipe, send)| {
            thread::spawn(move || {
                let mut output = Vec::new();
                let result = pipe.read_to_end(&mut output);
                let _ = send.send(result);
            })
        });
        let status = launcher.wait().unwrap();
        let eof = (0..2).all(|_| {
            receive
                .recv_timeout(Duration::from_secs(2))
                .is_ok_and(|r| r.is_ok())
        });
        let still_resident = repository.join("resident.ready").exists()
            && !repository.join("resident.exited").exists();
        fs::write(repository.join("release"), b"release").unwrap();
        let exited = wait_for(&repository.join("resident.exited"));
        for reader in readers {
            reader.join().unwrap();
        }
        let id = fs::read_to_string(repository.join("diagnostic-id")).unwrap();
        let logs = HostLaunchDiagnostics {
            directory: repository
                .join(".oracle-lab/parity-host/launch-logs")
                .join(&id),
            id,
        };
        let output = fs::read_to_string(logs.directory.join("stdout.log")).unwrap();
        let error = logs.failure("test failure");
        assert!(repository.starts_with(std::env::temp_dir()));
        // The resident publishes exited before its test harness releases files.
        for _ in 0..100 {
            if fs::remove_dir_all(&repository).is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(status.success());
        assert!(
            still_resident && eof,
            "launcher pipes must reach EOF while its resident child is alive"
        );
        assert!(exited);
        assert!(output.contains("resident stdout evidence"));
        assert!(error.contains("resident stderr evidence") && error.contains(&logs.id));
        assert!(!repository.exists());
    }

    #[test]
    fn current_endpoint_is_epoch_bound_and_stop_is_idempotent() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repository = std::env::temp_dir().join(format!(
            "sts-java-oracle-host-contract-{}-{unique}",
            std::process::id()
        ));
        let live_output = repository.join("live");
        let session = repository
            .join(".oracle-lab")
            .join("parity-host")
            .join("sessions")
            .join("host-test");
        fs::create_dir_all(&live_output).unwrap();
        fs::create_dir_all(&session).unwrap();
        let endpoint_path = session.join("endpoint.json");
        let host_lease = acquire_host_lease(&session.join("host.lock")).unwrap();
        let endpoint = JavaOracleHostEndpointV1 {
            schema: JAVA_ORACLE_HOST_ENDPOINT_SCHEMA_V2.to_owned(),
            host_id: "host-test".to_owned(),
            live_output,
            host_lease: session.join("host.lock"),
            worker_lease: session.join("worker.lock"),
            stop_request: session.join("stop.request"),
            producer: ProducerIdentityV1 {
                implementation: "communicationmod".to_owned(),
                version: "test-version".to_owned(),
                session_epoch: "test-epoch".to_owned(),
            },
        };
        write_json_create_new(&endpoint_path, &endpoint).unwrap();
        let current = repository
            .join(".oracle-lab")
            .join("parity-host")
            .join("current.json");
        write_json_replace(
            &current,
            &JavaOracleHostPointerV1 {
                schema: JAVA_ORACLE_HOST_POINTER_SCHEMA_V1.to_owned(),
                host_id: endpoint.host_id.clone(),
                endpoint: endpoint_path,
            },
        )
        .unwrap();

        assert_eq!(load_current_endpoint(&repository).unwrap(), endpoint);
        request_current_java_oracle_stop_v1(&repository).unwrap();
        request_current_java_oracle_stop_v1(&repository).unwrap();
        assert_eq!(fs::read(&endpoint.stop_request).unwrap(), b"stop\n");

        drop(host_lease);
        assert!(require_live_host_lease(&endpoint)
            .unwrap_err()
            .contains("owner is not running"));

        drop(CurrentHostPointerGuardV1 {
            path: current.clone(),
            host_id: endpoint.host_id.clone(),
        });
        assert!(!current.exists());
        fs::remove_dir_all(repository).unwrap();
    }
}
