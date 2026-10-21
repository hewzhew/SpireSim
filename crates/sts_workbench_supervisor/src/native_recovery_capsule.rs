use crate::{
    NativeRecoveryCallbackErrorV1, NativeRecoveryCallbackJobV1, NativeRecoveryCallbackPhaseV1,
    OwnedStorageRegistryV1, OwnedStorageV1, PhysicalRuntimeResultV1, RepositoryLifecycleOwnerV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use sts_content_bundle::{BundleRefV1, ContentBundleStoreV1};

const CAPSULE_SCHEMA_V1: &str = "sts-workbench-native-recovery-capsule-v1";
const CAPSULE_REF_DOMAIN_V1: &str = "workbench-native-recovery-capsule-v1";

#[derive(Debug, Eq, PartialEq)]
pub enum NativeRecoveryCapsuleErrorV1 {
    InvalidScope(String),
    Registry(String),
    Capture(String),
    Capsule(String),
    Callback(NativeRecoveryCallbackErrorV1),
    CallbackCleanup { callback: String, cleanup: String },
    Cleanup(String),
}

impl fmt::Display for NativeRecoveryCapsuleErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScope(detail) => write!(formatter, "invalid recovery scope: {detail}"),
            Self::Registry(detail) => write!(formatter, "recovery registry failed: {detail}"),
            Self::Capture(detail) => write!(formatter, "recovery capture failed: {detail}"),
            Self::Capsule(detail) => write!(formatter, "recovery capsule failed: {detail}"),
            Self::Callback(error) => write!(formatter, "recovery callback failed: {error}"),
            Self::CallbackCleanup { callback, cleanup } => write!(
                formatter,
                "recovery callback failed: {callback}; cleanup also failed: {cleanup}"
            ),
            Self::Cleanup(detail) => write!(formatter, "recovery cleanup failed: {detail}"),
        }
    }
}

impl std::error::Error for NativeRecoveryCapsuleErrorV1 {}

/// A physical storage subset fixed before an Engine callback begins.
///
/// Only marker-owned or supervised runtime owners can construct this type;
/// Engine requests cannot replace it with paths.
#[derive(Clone, Debug)]
pub struct NativeRecoveryCaptureScopeV1 {
    source_root: PathBuf,
    relative_directories: Vec<String>,
}

impl NativeRecoveryCaptureScopeV1 {
    pub(crate) fn bind_owned_root(
        source_root: &Path,
        relative_directories: &[&str],
    ) -> Result<Self, NativeRecoveryCapsuleErrorV1> {
        let source_root = canonical_directory(source_root).map_err(|error| {
            NativeRecoveryCapsuleErrorV1::InvalidScope(format!(
                "owned storage root {} is unavailable: {error}",
                source_root.display()
            ))
        })?;
        let mut relative_directories = relative_directories
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        relative_directories.sort();
        if !canonical_direct_children(&relative_directories) {
            return Err(NativeRecoveryCapsuleErrorV1::InvalidScope(
                "relative directories must be canonical, non-empty, and unique".to_owned(),
            ));
        }
        for relative in &relative_directories {
            let selected = source_root.join(relative);
            let metadata = fs::symlink_metadata(&selected).map_err(|error| {
                NativeRecoveryCapsuleErrorV1::InvalidScope(format!(
                    "failed to inspect {}: {error}",
                    selected.display()
                ))
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(NativeRecoveryCapsuleErrorV1::InvalidScope(format!(
                    "scope member is not a physical directory: {}",
                    selected.display()
                )));
            }
            let resolved = canonical_directory(&selected)
                .map_err(NativeRecoveryCapsuleErrorV1::InvalidScope)?;
            if resolved.parent() != Some(source_root.as_path()) {
                return Err(NativeRecoveryCapsuleErrorV1::InvalidScope(format!(
                    "scope member escaped owned storage: {}",
                    selected.display()
                )));
            }
        }
        Ok(Self {
            source_root,
            relative_directories,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRecoveryCapsuleArtifactV1 {
    pub schema: String,
    pub recovery_ref: String,
    pub relative_directories: Vec<String>,
    pub bundle: BundleRefV1,
}

/// Read-only access to already sealed native-recovery capsules.
///
/// Unlike [`NativeRecoveryCapsuleRegistryV1`], this type owns no repository
/// lifecycle lease, staging storage, capture, materialization, or enumeration
/// capability. Domain code can use one exact opaque ref after a Supervisor
/// callback without inheriting the physical lifecycle owner.
#[derive(Clone, Debug)]
pub struct NativeRecoveryCapsuleReaderV1 {
    content_store_root: PathBuf,
    capsules_directory: PathBuf,
}

impl NativeRecoveryCapsuleReaderV1 {
    pub fn new(content_store_root: &Path) -> Result<Self, NativeRecoveryCapsuleErrorV1> {
        let content_store_root = canonical_directory(content_store_root)
            .map_err(NativeRecoveryCapsuleErrorV1::Registry)?;
        let capsules_directory = content_store_root.join("native-recovery-capsules");
        let metadata = fs::symlink_metadata(&capsules_directory).map_err(|error| {
            NativeRecoveryCapsuleErrorV1::Registry(format!(
                "failed to inspect capsule directory {}: {error}",
                capsules_directory.display()
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(NativeRecoveryCapsuleErrorV1::Registry(
                "capsule directory is not a physical directory".to_owned(),
            ));
        }
        let resolved = canonical_directory(&capsules_directory)
            .map_err(NativeRecoveryCapsuleErrorV1::Registry)?;
        if resolved.parent() != Some(content_store_root.as_path()) {
            return Err(NativeRecoveryCapsuleErrorV1::Registry(
                "capsule directory escaped its content store".to_owned(),
            ));
        }
        Ok(Self {
            content_store_root,
            capsules_directory: resolved,
        })
    }

    pub fn load_capsule(
        &self,
        recovery_ref: &str,
    ) -> Result<NativeRecoveryCapsuleArtifactV1, NativeRecoveryCapsuleErrorV1> {
        load_capsule_from_store(
            &self.content_store_root,
            &self.capsules_directory,
            recovery_ref,
        )
    }
}

/// Repository-scoped owner for disposable staging and sealed bundle refs.
///
/// Staging reuses `OwnedStorageV1`, including marker-guarded Drop cleanup and
/// startup reconciliation. Sealed capsules have no enumeration/planner API;
/// domain code must separately publish a verified recovery certificate.
#[derive(Clone, Debug)]
pub struct NativeRecoveryCapsuleRegistryV1 {
    staging_registry: OwnedStorageRegistryV1,
    content_store_root: PathBuf,
    capsules_directory: PathBuf,
}

impl NativeRecoveryCapsuleRegistryV1 {
    pub fn new(
        owner: &RepositoryLifecycleOwnerV1,
        staging_registry: OwnedStorageRegistryV1,
        content_store_root: &Path,
    ) -> Result<Self, NativeRecoveryCapsuleErrorV1> {
        if staging_registry.owner_id() != owner.owner_id()
            || staging_registry.repository() != owner.repository()
        {
            return Err(NativeRecoveryCapsuleErrorV1::Registry(
                "staging registry belongs to another repository owner".to_owned(),
            ));
        }
        fs::create_dir_all(content_store_root).map_err(|error| {
            NativeRecoveryCapsuleErrorV1::Registry(format!(
                "failed to create content store {}: {error}",
                content_store_root.display()
            ))
        })?;
        let content_store_root = canonical_directory(content_store_root)
            .map_err(NativeRecoveryCapsuleErrorV1::Registry)?;
        if !content_store_root.starts_with(owner.repository()) {
            return Err(NativeRecoveryCapsuleErrorV1::Registry(
                "content store is outside the owned repository".to_owned(),
            ));
        }
        let capsules_directory = content_store_root.join("native-recovery-capsules");
        fs::create_dir_all(&capsules_directory).map_err(|error| {
            NativeRecoveryCapsuleErrorV1::Registry(format!(
                "failed to create capsule directory: {error}"
            ))
        })?;
        ContentBundleStoreV1::new(&content_store_root)
            .initialize()
            .map_err(NativeRecoveryCapsuleErrorV1::Registry)?;
        Ok(Self {
            staging_registry,
            content_store_root,
            capsules_directory,
        })
    }

    pub fn load_capsule(
        &self,
        recovery_ref: &str,
    ) -> Result<NativeRecoveryCapsuleArtifactV1, NativeRecoveryCapsuleErrorV1> {
        load_capsule_from_store(
            &self.content_store_root,
            &self.capsules_directory,
            recovery_ref,
        )
    }

    pub fn materialize_capsule_new(
        &self,
        recovery_ref: &str,
        target: &Path,
    ) -> Result<NativeRecoveryCapsuleArtifactV1, NativeRecoveryCapsuleErrorV1> {
        let artifact = self.load_capsule(recovery_ref)?;
        ContentBundleStoreV1::new(&self.content_store_root)
            .materialize_bundle_new(&artifact.bundle, target)
            .map_err(NativeRecoveryCapsuleErrorV1::Capsule)?;
        for relative in &artifact.relative_directories {
            fs::create_dir_all(target.join(relative)).map_err(|error| {
                NativeRecoveryCapsuleErrorV1::Capsule(format!(
                    "failed to restore capsule directory {relative:?}: {error}"
                ))
            })?;
        }
        Ok(artifact)
    }

    /// Captures and seals one native-storage snapshot without starting a
    /// validator JVM.
    ///
    /// The returned capsule is deliberately only physical evidence. It does
    /// not become a recovery certificate until a domain owner separately binds it
    /// to an eligible logical root and proves a fresh-runtime round trip.
    /// `validate_source` is called before capture, after the content-consistent
    /// bundle is stored, and after the immutable capsule is published. A
    /// failure after publication can leave only an unreferenced capsule for
    /// later GC; it never grants recovery capability.
    pub fn capture_unverified_candidate(
        &self,
        scope: &NativeRecoveryCaptureScopeV1,
        mut validate_source: impl FnMut() -> Result<(), String>,
    ) -> Result<NativeRecoveryCapsuleArtifactV1, NativeRecoveryCapsuleErrorV1> {
        validate_source().map_err(|error| {
            NativeRecoveryCapsuleErrorV1::Capture(format!(
                "candidate source validation before capture failed: {error}"
            ))
        })?;
        let bundle = ContentBundleStoreV1::new(&self.content_store_root)
            .with_operation_verification_cache()
            .put_selected_directories_consistent(&scope.source_root, &scope.relative_directories)
            .map_err(NativeRecoveryCapsuleErrorV1::Capture)?;
        validate_source().map_err(|error| {
            NativeRecoveryCapsuleErrorV1::Capture(format!(
                "candidate source validation after capture failed: {error}"
            ))
        })?;
        let capsule = self.seal(&bundle, &scope.relative_directories)?;
        validate_source().map_err(|error| {
            NativeRecoveryCapsuleErrorV1::Capture(format!(
                "candidate source validation after seal failed: {error}"
            ))
        })?;
        Ok(capsule)
    }

    fn capture(
        &self,
        scope: &NativeRecoveryCaptureScopeV1,
    ) -> Result<NativeRecoveryStagingV1, NativeRecoveryCapsuleErrorV1> {
        let mut storage = self
            .staging_registry
            .create()
            .map_err(|error| NativeRecoveryCapsuleErrorV1::Capture(error.to_string()))?;
        let store =
            ContentBundleStoreV1::new(&self.content_store_root).with_operation_verification_cache();
        let bundle = store
            .put_selected_directories_consistent(&scope.source_root, &scope.relative_directories)
            .map_err(NativeRecoveryCapsuleErrorV1::Capture)?;
        let payload = storage.root().join("payload");
        if let Err(error) = store.materialize_bundle_new(&bundle, &payload) {
            let _ = storage.cleanup();
            return Err(NativeRecoveryCapsuleErrorV1::Capture(error));
        }
        for relative in &scope.relative_directories {
            fs::create_dir_all(payload.join(relative)).map_err(|error| {
                NativeRecoveryCapsuleErrorV1::Capture(format!(
                    "failed to complete staging shape: {error}"
                ))
            })?;
        }
        Ok(NativeRecoveryStagingV1 {
            storage,
            payload,
            bundle,
            cleanup_allowed: true,
        })
    }

    fn seal(
        &self,
        bundle: &BundleRefV1,
        relative_directories: &[String],
    ) -> Result<NativeRecoveryCapsuleArtifactV1, NativeRecoveryCapsuleErrorV1> {
        if !canonical_direct_children(relative_directories) {
            return Err(NativeRecoveryCapsuleErrorV1::Capsule(
                "capsule directories are not canonical direct children".to_owned(),
            ));
        }
        ContentBundleStoreV1::new(&self.content_store_root)
            .verify_bundle(bundle)
            .map_err(NativeRecoveryCapsuleErrorV1::Capsule)?;
        let recovery_ref = capsule_ref(bundle, relative_directories)?;
        let artifact = NativeRecoveryCapsuleArtifactV1 {
            schema: CAPSULE_SCHEMA_V1.to_owned(),
            recovery_ref: recovery_ref.clone(),
            relative_directories: relative_directories.to_vec(),
            bundle: bundle.clone(),
        };
        publish_json_immutable(&self.capsule_path(&recovery_ref), &artifact)
            .map_err(NativeRecoveryCapsuleErrorV1::Capsule)?;
        self.load_capsule(&recovery_ref)
    }

    fn capsule_path(&self, recovery_ref: &str) -> PathBuf {
        self.capsules_directory.join(format!("{recovery_ref}.json"))
    }
}

fn load_capsule_from_store(
    content_store_root: &Path,
    capsules_directory: &Path,
    recovery_ref: &str,
) -> Result<NativeRecoveryCapsuleArtifactV1, NativeRecoveryCapsuleErrorV1> {
    require_scoped_content_id(recovery_ref, CAPSULE_REF_DOMAIN_V1)
        .map_err(NativeRecoveryCapsuleErrorV1::Capsule)?;
    let path = capsules_directory.join(format!("{recovery_ref}.json"));
    let bytes = fs::read(&path).map_err(|error| {
        NativeRecoveryCapsuleErrorV1::Capsule(format!("failed to read {}: {error}", path.display()))
    })?;
    let artifact: NativeRecoveryCapsuleArtifactV1 = serde_json::from_slice(&bytes)
        .map_err(|error| NativeRecoveryCapsuleErrorV1::Capsule(error.to_string()))?;
    if artifact.schema != CAPSULE_SCHEMA_V1
        || artifact.recovery_ref != recovery_ref
        || !canonical_direct_children(&artifact.relative_directories)
        || capsule_ref(&artifact.bundle, &artifact.relative_directories)? != artifact.recovery_ref
    {
        return Err(NativeRecoveryCapsuleErrorV1::Capsule(
            "capsule content identity is invalid".to_owned(),
        ));
    }
    ContentBundleStoreV1::new(content_store_root)
        .verify_bundle(&artifact.bundle)
        .map_err(NativeRecoveryCapsuleErrorV1::Capsule)?;
    Ok(artifact)
}

#[derive(Debug)]
struct NativeRecoveryStagingV1 {
    storage: OwnedStorageV1,
    payload: PathBuf,
    bundle: BundleRefV1,
    cleanup_allowed: bool,
}

impl NativeRecoveryStagingV1 {
    fn cleanup(&mut self) -> Result<(), NativeRecoveryCapsuleErrorV1> {
        if !self.cleanup_allowed {
            return Ok(());
        }
        self.storage
            .cleanup()
            .map_err(|error| NativeRecoveryCapsuleErrorV1::Cleanup(error.to_string()))
    }

    fn retain_for_reconciliation(&mut self) {
        self.cleanup_allowed = false;
        self.storage.retain_for_reconciliation();
    }
}

pub trait NativeRecoveryValidatorV1: fmt::Debug {
    fn shutdown(&mut self) -> Result<(), String>;
}

#[derive(Debug)]
pub struct NativeRecoveryValidatorOpenV1 {
    validator: Box<dyn NativeRecoveryValidatorV1>,
    observation_ref: String,
}

impl NativeRecoveryValidatorOpenV1 {
    pub fn new(
        validator: impl NativeRecoveryValidatorV1 + 'static,
        observation_ref: impl Into<String>,
    ) -> Self {
        Self {
            validator: Box::new(validator),
            observation_ref: observation_ref.into(),
        }
    }
}

/// One job's complete staging, validator, and callback resource owner.
pub struct NativeRecoveryCapsuleJobV1 {
    callback: NativeRecoveryCallbackJobV1,
    registry: NativeRecoveryCapsuleRegistryV1,
    scope: NativeRecoveryCaptureScopeV1,
    staging: Option<NativeRecoveryStagingV1>,
    validator: Option<Box<dyn NativeRecoveryValidatorV1>>,
}

impl NativeRecoveryCapsuleJobV1 {
    pub fn new(
        registry: NativeRecoveryCapsuleRegistryV1,
        scope: NativeRecoveryCaptureScopeV1,
        engine_epoch: u64,
        job_id: impl Into<String>,
    ) -> Result<Self, NativeRecoveryCapsuleErrorV1> {
        Ok(Self {
            callback: NativeRecoveryCallbackJobV1::new(engine_epoch, job_id)
                .map_err(NativeRecoveryCapsuleErrorV1::Callback)?,
            registry,
            scope,
            staging: None,
            validator: None,
        })
    }

    pub fn phase(&self) -> NativeRecoveryCallbackPhaseV1 {
        self.callback.phase()
    }

    pub fn begin(
        &mut self,
        engine_epoch: u64,
        job_id: &str,
        mut validate_primary: impl FnMut() -> Result<(), String>,
        open_validator: impl FnOnce(&Path) -> Result<NativeRecoveryValidatorOpenV1, String>,
    ) -> Result<PhysicalRuntimeResultV1, NativeRecoveryCapsuleErrorV1> {
        let registry = &self.registry;
        let scope = &self.scope;
        let staging = &mut self.staging;
        let validator = &mut self.validator;
        let callback = self.callback.begin(engine_epoch, job_id, || {
            validate_primary()?;
            let captured = registry.capture(scope).map_err(|error| error.to_string())?;
            validate_primary()?;
            let opened = open_validator(&captured.payload)?;
            *staging = Some(captured);
            *validator = Some(opened.validator);
            validate_primary()?;
            Ok(opened.observation_ref)
        });
        self.finish_callback(callback)
    }

    pub fn seal(
        &mut self,
        engine_epoch: u64,
        job_id: &str,
        mut validate_primary: impl FnMut() -> Result<(), String>,
    ) -> Result<PhysicalRuntimeResultV1, NativeRecoveryCapsuleErrorV1> {
        let registry = &self.registry;
        let relative_directories = &self.scope.relative_directories;
        let staging = &mut self.staging;
        let validator = &mut self.validator;
        let callback = self.callback.seal(engine_epoch, job_id, || {
            validate_primary()?;
            let mut disposable = validator
                .take()
                .ok_or_else(|| "native recovery validator is absent".to_owned())?;
            if let Err(error) = disposable.shutdown() {
                if let Some(staging) = staging.as_mut() {
                    staging.retain_for_reconciliation();
                }
                return Err(format!(
                    "native recovery validator shutdown was not proven: {error}"
                ));
            }
            validate_primary()?;
            let captured = staging
                .as_mut()
                .ok_or_else(|| "native recovery staging is absent".to_owned())?;
            let capsule = registry
                .seal(&captured.bundle, relative_directories)
                .map_err(|error| error.to_string())?;
            validate_primary()?;
            captured.cleanup().map_err(|error| error.to_string())?;
            *staging = None;
            Ok(capsule.recovery_ref)
        });
        self.finish_callback(callback)
    }

    pub fn abort(&mut self) -> Result<(), NativeRecoveryCapsuleErrorV1> {
        self.callback
            .abort()
            .map_err(NativeRecoveryCapsuleErrorV1::Callback)?;
        self.cleanup_resources()
    }

    fn finish_callback(
        &mut self,
        callback: Result<PhysicalRuntimeResultV1, NativeRecoveryCallbackErrorV1>,
    ) -> Result<PhysicalRuntimeResultV1, NativeRecoveryCapsuleErrorV1> {
        match callback {
            Ok(result) => Ok(result),
            Err(callback) if self.callback.phase() == NativeRecoveryCallbackPhaseV1::Failed => {
                match self.cleanup_resources() {
                    Ok(()) => Err(NativeRecoveryCapsuleErrorV1::Callback(callback)),
                    Err(cleanup) => Err(NativeRecoveryCapsuleErrorV1::CallbackCleanup {
                        callback: callback.to_string(),
                        cleanup: cleanup.to_string(),
                    }),
                }
            }
            Err(callback) => Err(NativeRecoveryCapsuleErrorV1::Callback(callback)),
        }
    }

    fn cleanup_resources(&mut self) -> Result<(), NativeRecoveryCapsuleErrorV1> {
        let mut errors = Vec::new();
        let mut validator_stopped = true;
        if let Some(mut validator) = self.validator.take() {
            if let Err(error) = validator.shutdown() {
                validator_stopped = false;
                errors.push(format!("validator shutdown was not proven: {error}"));
            }
        }
        if let Some(mut staging) = self.staging.take() {
            if validator_stopped && staging.cleanup_allowed {
                if let Err(error) = staging.cleanup() {
                    staging.retain_for_reconciliation();
                    errors.push(error.to_string());
                }
            } else {
                staging.retain_for_reconciliation();
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(NativeRecoveryCapsuleErrorV1::Cleanup(errors.join("; ")))
        }
    }
}

impl Drop for NativeRecoveryCapsuleJobV1 {
    fn drop(&mut self) {
        let _ = self.cleanup_resources();
    }
}

fn capsule_ref(
    bundle: &BundleRefV1,
    relative_directories: &[String],
) -> Result<String, NativeRecoveryCapsuleErrorV1> {
    let bytes = canonical_bytes(&(bundle, relative_directories))
        .map_err(NativeRecoveryCapsuleErrorV1::Capsule)?;
    Ok(scoped_content_id(CAPSULE_REF_DOMAIN_V1, &bytes))
}

fn canonical_directory(path: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(|error| error.to_string())?;
    if !canonical.is_dir() {
        return Err(format!("not a directory: {}", canonical.display()));
    }
    Ok(canonical)
}

fn safe_direct_child(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\', ':'])
        && !value.chars().any(char::is_control)
}

fn canonical_direct_children(values: &[String]) -> bool {
    !values.is_empty()
        && !values.windows(2).any(|pair| pair[0] >= pair[1])
        && values.iter().all(|value| safe_direct_child(value))
}

fn canonical_bytes(value: &impl Serialize) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| error.to_string())
}

fn scoped_content_id(domain: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    format!("{domain}-{:x}", hasher.finalize())
}

fn require_scoped_content_id(value: &str, domain: &str) -> Result<(), String> {
    let Some(suffix) = value.strip_prefix(&format!("{domain}-")) else {
        return Err("opaque recovery reference has the wrong domain".to_owned());
    };
    if suffix.len() != 64
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("opaque recovery reference has an invalid digest".to_owned());
    }
    Ok(())
}

fn publish_json_immutable(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file
            .write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(path).map_err(|error| error.to_string())?;
            if existing == bytes {
                Ok(())
            } else {
                Err("immutable capsule index changed".to_owned())
            }
        }
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::PhysicalRuntimeOutcomeV1;
    use std::cell::Cell;
    use std::env;
    use std::rc::Rc;
    use uuid::Uuid;

    #[derive(Debug)]
    struct FakeValidatorV1 {
        shutdowns: Rc<Cell<usize>>,
        fail_shutdown: bool,
    }

    impl NativeRecoveryValidatorV1 for FakeValidatorV1 {
        fn shutdown(&mut self) -> Result<(), String> {
            self.shutdowns.set(self.shutdowns.get() + 1);
            if self.fail_shutdown {
                Err("fake validator remained live".to_owned())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn capture_seal_round_trip_leaves_only_an_opaque_capsule() {
        let fixture = FixtureV1::new("seal");
        let guards = Cell::new(0);
        let shutdowns = Rc::new(Cell::new(0));
        let mut job = fixture.job("job-1");
        job.begin(
            7,
            "job-1",
            || {
                guards.set(guards.get() + 1);
                Ok(())
            },
            |payload| {
                assert_eq!(
                    fs::read(payload.join("saves/save.autosave")).unwrap(),
                    b"save"
                );
                Ok(NativeRecoveryValidatorOpenV1::new(
                    FakeValidatorV1 {
                        shutdowns: Rc::clone(&shutdowns),
                        fail_shutdown: false,
                    },
                    observation_ref('a'),
                ))
            },
        )
        .unwrap();
        let sealed = job
            .seal(7, "job-1", || {
                guards.set(guards.get() + 1);
                Ok(())
            })
            .unwrap();
        let recovery_ref = match sealed.outcome {
            PhysicalRuntimeOutcomeV1::NativeRecoverySealed { recovery_ref } => recovery_ref,
            other => panic!("unexpected result: {other:?}"),
        };
        assert_eq!(guards.get(), 6);
        assert_eq!(shutdowns.get(), 1);
        let read_only = NativeRecoveryCapsuleReaderV1::new(&fixture.content_store)
            .unwrap()
            .load_capsule(&recovery_ref)
            .unwrap();
        assert_eq!(read_only.recovery_ref, recovery_ref);
        assert_eq!(
            read_only.relative_directories,
            vec!["betaPreferences", "preferences", "saves"]
        );
        let restored = fixture.base.join("restored");
        fixture
            .capsules
            .materialize_capsule_new(&recovery_ref, &restored)
            .unwrap();
        assert!(restored.join("betaPreferences").is_dir());
        assert!(!restored.join("mods").exists());
        assert_eq!(
            fs::read(restored.join("preferences/prefs")).unwrap(),
            b"prefs"
        );
        assert_eq!(
            fs::read(restored.join("saves/save.autosave")).unwrap(),
            b"save"
        );
        assert_eq!(fixture.staging_count(), 0);
        drop(job);
        fixture.cleanup();
    }

    #[test]
    fn unverified_candidate_capture_seals_without_staging_or_a_validator() {
        let fixture = FixtureV1::new("candidate");
        let guards = Cell::new(0);

        let capsule = fixture
            .capsules
            .capture_unverified_candidate(&fixture.scope, || {
                guards.set(guards.get() + 1);
                Ok(())
            })
            .unwrap();

        assert_eq!(guards.get(), 3);
        assert_eq!(fixture.staging_count(), 0);
        let loaded = NativeRecoveryCapsuleReaderV1::new(&fixture.content_store)
            .unwrap()
            .load_capsule(&capsule.recovery_ref)
            .unwrap();
        assert_eq!(loaded, capsule);
        fixture.cleanup();
    }

    #[test]
    fn unverified_candidate_capture_rejects_source_drift_before_seal() {
        let fixture = FixtureV1::new("candidate-drift");
        let guards = Cell::new(0);

        let error = fixture
            .capsules
            .capture_unverified_candidate(&fixture.scope, || {
                guards.set(guards.get() + 1);
                if guards.get() == 2 {
                    Err("parked autosave changed".to_owned())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();

        assert!(error.to_string().contains("parked autosave changed"));
        assert_eq!(guards.get(), 2);
        assert_eq!(fixture.staging_count(), 0);
        assert_eq!(
            fs::read_dir(fixture.content_store.join("native-recovery-capsules"))
                .unwrap()
                .count(),
            0
        );
        fixture.cleanup();
    }

    #[test]
    fn failed_primary_revalidation_cleans_validator_and_staging() {
        let fixture = FixtureV1::new("revalidate");
        let guards = Cell::new(0);
        let shutdowns = Rc::new(Cell::new(0));
        let mut job = fixture.job("job-2");
        let error = job
            .begin(
                7,
                "job-2",
                || {
                    guards.set(guards.get() + 1);
                    if guards.get() == 3 {
                        Err("primary frame drifted".to_owned())
                    } else {
                        Ok(())
                    }
                },
                |_| {
                    Ok(NativeRecoveryValidatorOpenV1::new(
                        FakeValidatorV1 {
                            shutdowns: Rc::clone(&shutdowns),
                            fail_shutdown: false,
                        },
                        observation_ref('b'),
                    ))
                },
            )
            .unwrap_err();
        assert!(error.to_string().contains("primary frame drifted"));
        assert_eq!(shutdowns.get(), 1);
        assert_eq!(job.phase(), NativeRecoveryCallbackPhaseV1::Failed);
        assert_eq!(fixture.staging_count(), 0);
        drop(job);
        fixture.cleanup();
    }

    #[test]
    fn unproven_shutdown_retains_staging_for_existing_startup_reconciliation() {
        let fixture = FixtureV1::new("retained");
        let shutdowns = Rc::new(Cell::new(0));
        let mut job = fixture.job("job-3");
        job.begin(
            7,
            "job-3",
            || Ok(()),
            |_| {
                Ok(NativeRecoveryValidatorOpenV1::new(
                    FakeValidatorV1 {
                        shutdowns: Rc::clone(&shutdowns),
                        fail_shutdown: true,
                    },
                    observation_ref('c'),
                ))
            },
        )
        .unwrap();
        let error = job.seal(7, "job-3", || Ok(())).unwrap_err();
        assert!(error.to_string().contains("shutdown was not proven"));
        drop(job);
        assert_eq!(shutdowns.get(), 1);
        assert_eq!(fixture.staging_count(), 1);
        let report = fixture.staging_registry.reconcile_startup().unwrap();
        assert_eq!(report.reclaimed_storage.len(), 1);
        fixture.cleanup();
    }

    struct FixtureV1 {
        base: PathBuf,
        game: PathBuf,
        _owner: RepositoryLifecycleOwnerV1,
        staging_registry: OwnedStorageRegistryV1,
        capsules: NativeRecoveryCapsuleRegistryV1,
        content_store: PathBuf,
        scope: NativeRecoveryCaptureScopeV1,
    }

    impl FixtureV1 {
        fn new(label: &str) -> Self {
            let base =
                env::temp_dir().join(format!("sts-native-recovery-{label}-{}", Uuid::new_v4()));
            let repository = base.join("repository");
            let game = base.join("game");
            let source = base.join("source");
            fs::create_dir_all(&repository).unwrap();
            fs::create_dir_all(&game).unwrap();
            fs::create_dir_all(&source).unwrap();
            for directory in ["preferences", "betaPreferences", "saves"] {
                fs::create_dir(source.join(directory)).unwrap();
            }
            fs::create_dir(source.join("mods")).unwrap();
            fs::write(source.join("preferences/prefs"), b"prefs").unwrap();
            fs::write(source.join("saves/save.autosave"), b"save").unwrap();
            fs::write(source.join("mods/not-captured.jar"), b"mod").unwrap();
            let owner = RepositoryLifecycleOwnerV1::acquire(&repository).unwrap();
            let staging_registry = OwnedStorageRegistryV1::new(&owner, &game);
            let content_store = repository.join(".oracle-lab/paired-root-store");
            let capsules = NativeRecoveryCapsuleRegistryV1::new(
                &owner,
                staging_registry.clone(),
                &content_store,
            )
            .unwrap();
            let scope = NativeRecoveryCaptureScopeV1::bind_owned_root(
                &source,
                &["preferences", "betaPreferences", "saves"],
            )
            .unwrap();
            Self {
                base,
                game,
                _owner: owner,
                staging_registry,
                capsules,
                content_store,
                scope,
            }
        }

        fn job(&self, job_id: &str) -> NativeRecoveryCapsuleJobV1 {
            NativeRecoveryCapsuleJobV1::new(self.capsules.clone(), self.scope.clone(), 7, job_id)
                .unwrap()
        }

        fn staging_count(&self) -> usize {
            let parent = self.game.join(".sts-java-play/isolated-storage");
            fs::read_dir(parent)
                .map(|entries| entries.count())
                .unwrap_or(0)
        }

        fn cleanup(self) {
            let base = self.base.clone();
            drop(self);
            fs::remove_dir_all(base).unwrap();
        }
    }

    fn observation_ref(fill: char) -> String {
        format!(
            "workbench-native-recovery-observation-v1-{}",
            fill.to_string().repeat(64)
        )
    }
}
