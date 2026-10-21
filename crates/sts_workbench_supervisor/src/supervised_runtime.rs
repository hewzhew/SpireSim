use crate::{
    JavaRuntimeIdentityV1, NativeRecoveryCapsuleErrorV1, NativeRecoveryCaptureScopeV1,
    SupervisedResourcesErrorV1, SupervisedResourcesV1,
};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSessionBindingV1 {
    runtime_epoch: String,
    client_session_id: String,
    connection_id: String,
    companion_process_id: u32,
    java_build_id: String,
    mod_build_id: String,
    protocol_version: u16,
}

impl RuntimeSessionBindingV1 {
    pub fn new(
        runtime_epoch: impl Into<String>,
        client_session_id: impl Into<String>,
        connection_id: impl Into<String>,
        companion_process_id: u32,
        java_build_id: impl Into<String>,
        mod_build_id: impl Into<String>,
        protocol_version: u16,
    ) -> Result<Self, SupervisedRuntimeErrorV1> {
        let binding = Self {
            runtime_epoch: runtime_epoch.into(),
            client_session_id: client_session_id.into(),
            connection_id: connection_id.into(),
            companion_process_id,
            java_build_id: java_build_id.into(),
            mod_build_id: mod_build_id.into(),
            protocol_version,
        };
        if binding.runtime_epoch.trim().is_empty()
            || binding.client_session_id.trim().is_empty()
            || binding.connection_id.trim().is_empty()
            || binding.companion_process_id == 0
            || binding.java_build_id.trim().is_empty()
            || binding.mod_build_id.trim().is_empty()
            || binding.protocol_version == 0
        {
            return Err(SupervisedRuntimeErrorV1::InvalidSessionBinding);
        }
        Ok(binding)
    }

    pub fn runtime_epoch(&self) -> &str {
        &self.runtime_epoch
    }

    pub fn client_session_id(&self) -> &str {
        &self.client_session_id
    }

    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub fn companion_process_id(&self) -> u32 {
        self.companion_process_id
    }

    pub fn java_build_id(&self) -> &str {
        &self.java_build_id
    }

    pub fn mod_build_id(&self) -> &str {
        &self.mod_build_id
    }

    pub fn protocol_version(&self) -> u16 {
        self.protocol_version
    }
}

#[derive(Debug)]
pub enum SupervisedRuntimeErrorV1 {
    Closed,
    InvalidSessionBinding,
    CompanionNotAdmitted,
    CompanionProcessMismatch { expected: u32, observed: u32 },
    SessionIdentityMismatch,
    AutosaveAlreadyBound,
    AutosaveNotBound,
    AutosaveIdentityInvalid,
    AutosavePath(String),
    AutosaveOutsideStorage,
    AutosaveIdentityMismatch,
    FrameNotBound,
    FrameIdentityInvalid,
    FrameIdentityMismatch,
    Resources(SupervisedResourcesErrorV1),
}

impl fmt::Display for SupervisedRuntimeErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(formatter, "supervised runtime is closed"),
            Self::InvalidSessionBinding => {
                write!(formatter, "runtime session binding is invalid")
            }
            Self::CompanionNotAdmitted => {
                write!(formatter, "runtime session has no admitted companion")
            }
            Self::CompanionProcessMismatch { expected, observed } => write!(
                formatter,
                "runtime companion process mismatch: expected {expected}, observed {observed}"
            ),
            Self::SessionIdentityMismatch => write!(formatter, "runtime session identity changed"),
            Self::AutosaveAlreadyBound => write!(formatter, "runtime autosave is already bound"),
            Self::AutosaveNotBound => write!(formatter, "runtime autosave is not bound"),
            Self::AutosaveIdentityInvalid => {
                write!(formatter, "runtime autosave identity is invalid")
            }
            Self::AutosavePath(detail) => {
                write!(formatter, "runtime autosave path failure: {detail}")
            }
            Self::AutosaveOutsideStorage => {
                write!(formatter, "runtime autosave is outside owned storage")
            }
            Self::AutosaveIdentityMismatch => {
                write!(formatter, "runtime autosave identity changed")
            }
            Self::FrameNotBound => write!(formatter, "runtime frame is not bound"),
            Self::FrameIdentityInvalid => write!(formatter, "runtime frame identity is invalid"),
            Self::FrameIdentityMismatch => write!(formatter, "runtime frame identity changed"),
            Self::Resources(error) => write!(formatter, "runtime resource failure: {error}"),
        }
    }
}

impl std::error::Error for SupervisedRuntimeErrorV1 {}

/// Exact external resources and opaque protocol epoch for one live runtime.
///
/// The Supervisor does not interpret game scenes, roots, actions, or frames.
/// It only binds the already-admitted companion handle to the session identity
/// that the protocol owner observed and rejects later identity drift.
#[derive(Debug)]
pub struct SupervisedRuntimeV1 {
    resources: SupervisedResourcesV1,
    session: RuntimeSessionBindingV1,
    autosave: Option<RuntimeAutosaveV1>,
    accepted_frame: Option<String>,
    closed: bool,
}

#[derive(Debug)]
struct RuntimeAutosaveV1 {
    path: PathBuf,
    native_identity: String,
    accepted_identity: String,
}

impl SupervisedRuntimeV1 {
    pub fn new(
        mut resources: SupervisedResourcesV1,
        session: RuntimeSessionBindingV1,
    ) -> Result<Self, SupervisedRuntimeErrorV1> {
        let companion_process_id = resources
            .companion_process_id()
            .ok_or(SupervisedRuntimeErrorV1::CompanionNotAdmitted)?;
        if companion_process_id != session.companion_process_id() {
            return Err(SupervisedRuntimeErrorV1::CompanionProcessMismatch {
                expected: companion_process_id,
                observed: session.companion_process_id(),
            });
        }
        resources
            .validate_running()
            .map_err(SupervisedRuntimeErrorV1::Resources)?;
        Ok(Self {
            resources,
            session,
            autosave: None,
            accepted_frame: None,
            closed: false,
        })
    }

    pub fn session(&self) -> &RuntimeSessionBindingV1 {
        &self.session
    }

    pub fn storage_root(&self) -> &Path {
        self.resources.storage_root()
    }

    pub fn isolation_id(&self) -> Uuid {
        self.resources.isolation_id()
    }

    /// Pre-bind the physical storage subset that a later recovery job may
    /// capture. Callback requests cannot replace this scope with paths.
    pub fn bind_native_recovery_scope(
        &self,
        relative_directories: &[&str],
    ) -> Result<NativeRecoveryCaptureScopeV1, NativeRecoveryCapsuleErrorV1> {
        NativeRecoveryCaptureScopeV1::bind_owned_root(
            self.resources.storage_root(),
            relative_directories,
        )
    }

    pub fn primary_process_id(&self) -> u32 {
        self.resources.primary_process_id()
    }

    /// Canonical identity consumed by the Supervisor operation model.
    ///
    /// All fields come from already-owned process/session evidence. Semantic
    /// Engine code must not reconstruct a second identity from raw manifests.
    pub fn java_runtime_identity(&self) -> JavaRuntimeIdentityV1 {
        JavaRuntimeIdentityV1 {
            process_id: self.resources.primary_process_id(),
            process_start_token: self.resources.primary_process_start_token().to_owned(),
            runtime_epoch: self.session.runtime_epoch().to_owned(),
            java_build_id: self.session.java_build_id().to_owned(),
            mod_build_id: self.session.mod_build_id().to_owned(),
            protocol_version: self.session.protocol_version(),
        }
    }

    pub fn bind_autosave(
        &mut self,
        path: &Path,
        native_identity: &str,
    ) -> Result<(), SupervisedRuntimeErrorV1> {
        if self.closed {
            return Err(SupervisedRuntimeErrorV1::Closed);
        }
        if self.autosave.is_some() {
            return Err(SupervisedRuntimeErrorV1::AutosaveAlreadyBound);
        }
        if !is_sha256(native_identity) {
            return Err(SupervisedRuntimeErrorV1::AutosaveIdentityInvalid);
        }
        let saves = fs::canonicalize(self.resources.storage_root().join("saves"))
            .map_err(|error| SupervisedRuntimeErrorV1::AutosavePath(error.to_string()))?;
        let path = fs::canonicalize(path)
            .map_err(|error| SupervisedRuntimeErrorV1::AutosavePath(error.to_string()))?;
        if !path.is_file() || path.parent() != Some(saves.as_path()) {
            return Err(SupervisedRuntimeErrorV1::AutosaveOutsideStorage);
        }
        let observed = file_sha256(&path)?;
        let native_identity = native_identity.to_ascii_lowercase();
        if observed != native_identity {
            return Err(SupervisedRuntimeErrorV1::AutosaveIdentityMismatch);
        }
        self.autosave = Some(RuntimeAutosaveV1 {
            path,
            native_identity: native_identity.clone(),
            accepted_identity: native_identity,
        });
        Ok(())
    }

    pub fn autosave_path(&self) -> Result<&Path, SupervisedRuntimeErrorV1> {
        self.autosave
            .as_ref()
            .map(|autosave| autosave.path.as_path())
            .ok_or(SupervisedRuntimeErrorV1::AutosaveNotBound)
    }

    pub fn autosave_identity(&self) -> Result<&str, SupervisedRuntimeErrorV1> {
        self.autosave
            .as_ref()
            .map(|autosave| autosave.accepted_identity.as_str())
            .ok_or(SupervisedRuntimeErrorV1::AutosaveNotBound)
    }

    pub fn native_autosave_identity(&self) -> Result<&str, SupervisedRuntimeErrorV1> {
        self.autosave
            .as_ref()
            .map(|autosave| autosave.native_identity.as_str())
            .ok_or(SupervisedRuntimeErrorV1::AutosaveNotBound)
    }

    pub fn native_autosave_is_current(&self) -> Result<bool, SupervisedRuntimeErrorV1> {
        let autosave = self
            .autosave
            .as_ref()
            .ok_or(SupervisedRuntimeErrorV1::AutosaveNotBound)?;
        Ok(autosave.accepted_identity == autosave.native_identity)
    }

    pub fn current_autosave_identity_optional(
        &self,
    ) -> Result<Option<String>, SupervisedRuntimeErrorV1> {
        let path = self.autosave_path()?;
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => file_sha256(path).map(Some),
            Ok(_) => Err(SupervisedRuntimeErrorV1::AutosaveOutsideStorage),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(SupervisedRuntimeErrorV1::AutosavePath(error.to_string())),
        }
    }

    pub fn verify_autosave(&self) -> Result<(), SupervisedRuntimeErrorV1> {
        if self.current_autosave_identity_optional()?.as_deref() != Some(self.autosave_identity()?)
        {
            return Err(SupervisedRuntimeErrorV1::AutosaveIdentityMismatch);
        }
        Ok(())
    }

    pub fn accept_current_autosave(&mut self) -> Result<&str, SupervisedRuntimeErrorV1> {
        let observed = self
            .current_autosave_identity_optional()?
            .ok_or(SupervisedRuntimeErrorV1::AutosaveIdentityMismatch)?;
        let autosave = self
            .autosave
            .as_mut()
            .ok_or(SupervisedRuntimeErrorV1::AutosaveNotBound)?;
        autosave.accepted_identity = observed;
        Ok(&autosave.accepted_identity)
    }

    pub fn bind_frame(
        &mut self,
        frame_token: impl Into<String>,
    ) -> Result<(), SupervisedRuntimeErrorV1> {
        if self.closed {
            return Err(SupervisedRuntimeErrorV1::Closed);
        }
        if self.accepted_frame.is_some() {
            return Err(SupervisedRuntimeErrorV1::FrameIdentityMismatch);
        }
        let frame_token = frame_token.into();
        validate_frame_token(&frame_token)?;
        self.accepted_frame = Some(frame_token);
        Ok(())
    }

    pub fn frame_token(&self) -> Result<&str, SupervisedRuntimeErrorV1> {
        self.accepted_frame
            .as_deref()
            .ok_or(SupervisedRuntimeErrorV1::FrameNotBound)
    }

    pub fn verify_frame(&self, observed: &str) -> Result<(), SupervisedRuntimeErrorV1> {
        validate_frame_token(observed)?;
        if self.frame_token()? != observed {
            return Err(SupervisedRuntimeErrorV1::FrameIdentityMismatch);
        }
        Ok(())
    }

    pub fn accept_frame(
        &mut self,
        observed: impl Into<String>,
    ) -> Result<(), SupervisedRuntimeErrorV1> {
        if self.accepted_frame.is_none() {
            return Err(SupervisedRuntimeErrorV1::FrameNotBound);
        }
        let observed = observed.into();
        validate_frame_token(&observed)?;
        self.accepted_frame = Some(observed);
        Ok(())
    }

    pub fn validate_running(
        &mut self,
        observed: &RuntimeSessionBindingV1,
    ) -> Result<(), SupervisedRuntimeErrorV1> {
        if self.closed {
            return Err(SupervisedRuntimeErrorV1::Closed);
        }
        self.resources
            .validate_running()
            .map_err(SupervisedRuntimeErrorV1::Resources)?;
        if &self.session != observed {
            return Err(SupervisedRuntimeErrorV1::SessionIdentityMismatch);
        }
        Ok(())
    }

    pub fn shutdown(
        &mut self,
        primary_tree_timeout: Duration,
        primary_forced_timeout: Duration,
        companion_timeout: Duration,
    ) -> Result<(), SupervisedRuntimeErrorV1> {
        if self.closed {
            return Ok(());
        }
        self.resources
            .shutdown(
                primary_tree_timeout,
                primary_forced_timeout,
                companion_timeout,
            )
            .map_err(SupervisedRuntimeErrorV1::Resources)?;
        self.closed = true;
        Ok(())
    }
}

fn file_sha256(path: &Path) -> Result<String, SupervisedRuntimeErrorV1> {
    let mut file = File::open(path)
        .map_err(|error| SupervisedRuntimeErrorV1::AutosavePath(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| SupervisedRuntimeErrorV1::AutosavePath(error.to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_frame_token(value: &str) -> Result<(), SupervisedRuntimeErrorV1> {
    if value.trim().is_empty() {
        return Err(SupervisedRuntimeErrorV1::FrameIdentityInvalid);
    }
    Ok(())
}

impl Drop for SupervisedRuntimeV1 {
    fn drop(&mut self) {
        let _ = self.shutdown(
            Duration::from_secs(10),
            Duration::from_secs(5),
            Duration::from_secs(5),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_binding_rejects_missing_identity() {
        assert!(matches!(
            RuntimeSessionBindingV1::new(
                "",
                "session",
                "connection",
                1,
                "java-build",
                "mod-build",
                41,
            ),
            Err(SupervisedRuntimeErrorV1::InvalidSessionBinding)
        ));
        assert!(matches!(
            RuntimeSessionBindingV1::new(
                "epoch",
                "session",
                "connection",
                0,
                "java-build",
                "mod-build",
                41,
            ),
            Err(SupervisedRuntimeErrorV1::InvalidSessionBinding)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn runtime_binds_exact_session_to_resources_and_cleans_them_together() {
        use crate::{
            ExistingProcessCandidateV1, OwnedStorageRegistryV1, RepositoryLifecycleOwnerV1,
            SupervisedChildV1, SupervisedProcessSetV1,
        };
        use std::env;
        use std::fs;
        use std::path::PathBuf;
        use std::process::{Command, Stdio};

        let base = env::temp_dir().join(format!("sts-workbench-runtime-{}", Uuid::new_v4()));
        let repository = base.join("repository");
        let game_directory = base.join("game");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(&game_directory).unwrap();
        let owner = RepositoryLifecycleOwnerV1::acquire(&repository).unwrap();
        let registry = OwnedStorageRegistryV1::new(&owner, &game_directory);
        let storage = registry.create().unwrap();
        let storage_root = storage.root().to_path_buf();
        let saves = storage_root.join("saves");
        fs::create_dir(&saves).unwrap();
        let autosave = saves.join("IRONCLAD.autosave");
        fs::write(&autosave, b"native autosave").unwrap();
        let native_autosave_identity = file_sha256(&autosave).unwrap();

        let executable = PathBuf::from(env::var_os("COMSPEC").unwrap());
        let spawn = || {
            Command::new(&executable)
                .args(["/D", "/C", "ping", "-n", "30", "127.0.0.1"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap()
        };
        let primary = spawn();
        let mut companion = spawn();
        let companion_process_id = companion.id();
        let companion_handle =
            ExistingProcessCandidateV1::open_running(companion_process_id, &executable)
                .unwrap()
                .admit();
        let mut processes = SupervisedProcessSetV1::new(SupervisedChildV1::adopt(primary));
        processes.admit_companion(companion_handle).unwrap();
        let resources = SupervisedResourcesV1::new(processes, storage);
        let session = RuntimeSessionBindingV1::new(
            "epoch-1",
            "client-session-1",
            "connection-1",
            companion_process_id,
            "java-build-1",
            "mod-build-1",
            41,
        )
        .unwrap();
        let mut runtime = SupervisedRuntimeV1::new(resources, session.clone()).unwrap();

        runtime.validate_running(&session).unwrap();
        let java_identity = runtime.java_runtime_identity();
        assert_eq!(java_identity.process_id, runtime.primary_process_id());
        assert!(java_identity
            .process_start_token
            .starts_with("supervised-process-start-v1-"));
        assert_eq!(java_identity.runtime_epoch, "epoch-1");
        assert_eq!(java_identity.java_build_id, "java-build-1");
        assert_eq!(java_identity.mod_build_id, "mod-build-1");
        assert_eq!(java_identity.protocol_version, 41);
        runtime
            .bind_autosave(&autosave, &native_autosave_identity)
            .unwrap();
        runtime.bind_frame("frame-1").unwrap();
        runtime.verify_frame("frame-1").unwrap();
        assert!(matches!(
            runtime.verify_frame("frame-2"),
            Err(SupervisedRuntimeErrorV1::FrameIdentityMismatch)
        ));
        runtime.accept_frame("frame-2").unwrap();
        runtime.verify_frame("frame-2").unwrap();
        runtime.verify_autosave().unwrap();
        assert!(runtime.native_autosave_is_current().unwrap());
        fs::write(&autosave, b"accepted successor autosave").unwrap();
        assert!(matches!(
            runtime.verify_autosave(),
            Err(SupervisedRuntimeErrorV1::AutosaveIdentityMismatch)
        ));
        runtime.accept_current_autosave().unwrap();
        runtime.verify_autosave().unwrap();
        assert!(!runtime.native_autosave_is_current().unwrap());
        let changed = RuntimeSessionBindingV1::new(
            "epoch-2",
            "client-session-1",
            "connection-1",
            companion_process_id,
            "java-build-1",
            "mod-build-1",
            41,
        )
        .unwrap();
        assert!(matches!(
            runtime.validate_running(&changed),
            Err(SupervisedRuntimeErrorV1::SessionIdentityMismatch)
        ));
        let changed_build = RuntimeSessionBindingV1::new(
            "epoch-1",
            "client-session-1",
            "connection-1",
            companion_process_id,
            "java-build-1",
            "mod-build-2",
            41,
        )
        .unwrap();
        assert!(matches!(
            runtime.validate_running(&changed_build),
            Err(SupervisedRuntimeErrorV1::SessionIdentityMismatch)
        ));
        let changed_protocol = RuntimeSessionBindingV1::new(
            "epoch-1",
            "client-session-1",
            "connection-1",
            companion_process_id,
            "java-build-1",
            "mod-build-1",
            42,
        )
        .unwrap();
        assert!(matches!(
            runtime.validate_running(&changed_protocol),
            Err(SupervisedRuntimeErrorV1::SessionIdentityMismatch)
        ));
        runtime
            .shutdown(
                Duration::from_secs(5),
                Duration::from_secs(5),
                Duration::from_secs(5),
            )
            .unwrap();
        companion.wait().unwrap();
        assert!(!storage_root.exists());

        drop(runtime);
        drop(registry);
        drop(owner);
        fs::remove_dir_all(base).unwrap();
    }
}
