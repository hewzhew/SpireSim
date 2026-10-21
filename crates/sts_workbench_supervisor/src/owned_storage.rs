use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::{
    validate_repository_owner_id_v1, NativeRecoveryCapsuleErrorV1, NativeRecoveryCaptureScopeV1,
    RepositoryLifecycleOwnerV1, PROCESS_CONTAINMENT_CLASS_V1,
};

const STORAGE_OWNER_SCHEMA: &str = "sts-workbench-runtime-owned-storage-v1";
const STORAGE_OWNER_FILE: &str = ".sts-workbench-runtime-owner.json";
const STARTUP_RECONCILIATION_SCHEMA: &str = "sts-workbench-runtime-startup-reconciliation-v1";

#[derive(Debug, Eq, PartialEq)]
pub enum OwnedStorageErrorV1 {
    SourceGameDirectory(String),
    StorageParent(String),
    StorageRoot(String),
    Marker(String),
    RefusedRoot(PathBuf),
    Cleanup(String),
    ReconciliationArtifact(String),
}

impl fmt::Display for OwnedStorageErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceGameDirectory(detail) => {
                write!(formatter, "invalid source game directory: {detail}")
            }
            Self::StorageParent(detail) => {
                write!(formatter, "owned-storage parent failed: {detail}")
            }
            Self::StorageRoot(detail) => write!(formatter, "owned-storage root failed: {detail}"),
            Self::Marker(detail) => write!(formatter, "owned-storage marker failed: {detail}"),
            Self::RefusedRoot(root) => {
                write!(
                    formatter,
                    "refusing unverified owned storage {}",
                    root.display()
                )
            }
            Self::Cleanup(detail) => write!(formatter, "owned-storage cleanup failed: {detail}"),
            Self::ReconciliationArtifact(detail) => {
                write!(
                    formatter,
                    "startup reconciliation artifact failed: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for OwnedStorageErrorV1 {}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OwnedStorageMarkerV1 {
    schema: String,
    owner_id: String,
    repository: String,
    source_game_directory: String,
    isolation_id: String,
    process_containment: String,
    created_at_unix_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedStorageReconciliationV1 {
    pub schema: String,
    pub owner_id: String,
    pub repository: String,
    pub reclaimed_storage: Vec<String>,
    pub foreign_owned_storage: Vec<String>,
    pub unrecognized_markers: Vec<OwnedStorageReconciliationIssueV1>,
    pub completed_at_unix_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedStorageReconciliationIssueV1 {
    pub path: String,
    pub reason: String,
}

/// Repository-scoped authority over the workbench's disposable game storage.
///
/// The source game directory is an explicit process configuration input. The
/// registry derives the one supported storage parent and never accepts an
/// arbitrary deletion root from an Engine or model request.
#[derive(Clone, Debug)]
pub struct OwnedStorageRegistryV1 {
    _repository_lease: Arc<File>,
    owner_id: String,
    repository: PathBuf,
    runtime_directory: PathBuf,
    source_game_directory: PathBuf,
}

impl OwnedStorageRegistryV1 {
    pub fn new(owner: &RepositoryLifecycleOwnerV1, source_game_directory: &Path) -> Self {
        Self {
            _repository_lease: owner.lease_guard(),
            owner_id: owner.owner_id().to_owned(),
            repository: owner.repository().to_path_buf(),
            runtime_directory: owner.runtime_directory().to_path_buf(),
            source_game_directory: source_game_directory.to_path_buf(),
        }
    }

    pub fn source_game_directory(&self) -> &Path {
        &self.source_game_directory
    }

    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }

    pub fn repository(&self) -> &Path {
        &self.repository
    }

    pub fn create(&self) -> Result<OwnedStorageV1, OwnedStorageErrorV1> {
        let source_game_directory = canonical_directory(
            &self.source_game_directory,
            "source game directory",
            OwnedStorageErrorV1::SourceGameDirectory,
        )?;
        let parent = storage_parent(&source_game_directory);
        fs::create_dir_all(&parent)
            .map_err(|error| OwnedStorageErrorV1::StorageParent(error.to_string()))?;
        let isolation_id = Uuid::new_v4();
        let root = parent.join(isolation_id.hyphenated().to_string());
        fs::create_dir(&root)
            .map_err(|error| OwnedStorageErrorV1::StorageRoot(error.to_string()))?;
        if let Err(error) = write_json_new(
            &root.join(STORAGE_OWNER_FILE),
            &OwnedStorageMarkerV1 {
                schema: STORAGE_OWNER_SCHEMA.to_owned(),
                owner_id: self.owner_id.clone(),
                repository: self.repository.to_string_lossy().into_owned(),
                source_game_directory: source_game_directory.to_string_lossy().into_owned(),
                isolation_id: isolation_id.hyphenated().to_string(),
                process_containment: PROCESS_CONTAINMENT_CLASS_V1.to_owned(),
                created_at_unix_ms: now_unix_ms(),
            },
        ) {
            let _ = fs::remove_dir(&root);
            return Err(error);
        }
        Ok(OwnedStorageV1 {
            registry: self.clone(),
            root,
            isolation_id,
            source_game_directory,
            cleaned: false,
            cleanup_on_drop: true,
        })
    }

    pub fn reconcile_startup(&self) -> Result<OwnedStorageReconciliationV1, OwnedStorageErrorV1> {
        let mut report = OwnedStorageReconciliationV1 {
            schema: STARTUP_RECONCILIATION_SCHEMA.to_owned(),
            owner_id: self.owner_id.clone(),
            repository: self.repository.to_string_lossy().into_owned(),
            reclaimed_storage: Vec::new(),
            foreign_owned_storage: Vec::new(),
            unrecognized_markers: Vec::new(),
            completed_at_unix_ms: 0,
        };
        if !self.source_game_directory.is_dir() {
            report.completed_at_unix_ms = now_unix_ms();
            return Ok(report);
        }
        let source_game_directory = canonical_directory(
            &self.source_game_directory,
            "source game directory",
            OwnedStorageErrorV1::SourceGameDirectory,
        )?;
        let parent = storage_parent(&source_game_directory);
        if !parent.is_dir() {
            report.completed_at_unix_ms = now_unix_ms();
            return Ok(report);
        }
        for entry in fs::read_dir(&parent)
            .map_err(|error| OwnedStorageErrorV1::StorageParent(error.to_string()))?
        {
            let entry =
                entry.map_err(|error| OwnedStorageErrorV1::StorageParent(error.to_string()))?;
            let root = entry.path();
            if !entry
                .file_type()
                .map_err(|error| OwnedStorageErrorV1::StorageRoot(error.to_string()))?
                .is_dir()
                || !root.join(STORAGE_OWNER_FILE).exists()
            {
                continue;
            }
            let marker = match read_marker(&root) {
                Ok(marker) => marker,
                Err(error) => {
                    report
                        .unrecognized_markers
                        .push(OwnedStorageReconciliationIssueV1 {
                            path: root.to_string_lossy().into_owned(),
                            reason: error.to_string(),
                        });
                    continue;
                }
            };
            if !same_path(Path::new(&marker.repository), &self.repository) {
                report
                    .foreign_owned_storage
                    .push(root.to_string_lossy().into_owned());
                continue;
            }
            if let Err(error) = validate_marker(
                &marker,
                &root,
                &source_game_directory,
                &self.repository,
                None,
            ) {
                report
                    .unrecognized_markers
                    .push(OwnedStorageReconciliationIssueV1 {
                        path: root.to_string_lossy().into_owned(),
                        reason: error.to_string(),
                    });
                continue;
            }
            remove_owned_storage_root(&root)?;
            report
                .reclaimed_storage
                .push(root.to_string_lossy().into_owned());
        }
        report.completed_at_unix_ms = now_unix_ms();
        Ok(report)
    }

    pub fn reconcile_startup_and_persist(
        &self,
    ) -> Result<OwnedStorageReconciliationV1, OwnedStorageErrorV1> {
        let report = self.reconcile_startup()?;
        let parent = self.runtime_directory.join("startup-reconciliation");
        fs::create_dir_all(&parent)
            .map_err(|error| OwnedStorageErrorV1::ReconciliationArtifact(error.to_string()))?;
        let path = parent.join(format!(
            "reconciliation-{}-{}.json",
            now_unix_ms(),
            Uuid::new_v4().simple()
        ));
        let encoded = serde_json::to_vec_pretty(&report)
            .map_err(|error| OwnedStorageErrorV1::ReconciliationArtifact(error.to_string()))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| OwnedStorageErrorV1::ReconciliationArtifact(error.to_string()))?;
        file.write_all(&encoded)
            .and_then(|_| file.sync_data())
            .map_err(|error| OwnedStorageErrorV1::ReconciliationArtifact(error.to_string()))?;
        Ok(report)
    }

    fn cleanup(&self, root: &Path) -> Result<(), OwnedStorageErrorV1> {
        let marker = read_marker(root)?;
        validate_marker(
            &marker,
            root,
            &self.source_game_directory,
            &self.repository,
            Some(&self.owner_id),
        )?;
        remove_owned_storage_root(root)
    }
}

#[derive(Debug)]
pub struct OwnedStorageV1 {
    registry: OwnedStorageRegistryV1,
    root: PathBuf,
    isolation_id: Uuid,
    source_game_directory: PathBuf,
    cleaned: bool,
    cleanup_on_drop: bool,
}

impl OwnedStorageV1 {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn isolation_id(&self) -> Uuid {
        self.isolation_id
    }

    pub fn source_game_directory(&self) -> &Path {
        &self.source_game_directory
    }

    /// Bind a read-only recovery capture scope while this marker-owned storage
    /// is still under the current repository lease.
    pub fn bind_native_recovery_scope(
        &self,
        relative_directories: &[&str],
    ) -> Result<NativeRecoveryCaptureScopeV1, NativeRecoveryCapsuleErrorV1> {
        NativeRecoveryCaptureScopeV1::bind_owned_root(&self.root, relative_directories)
    }

    pub fn cleanup(&mut self) -> Result<(), OwnedStorageErrorV1> {
        if self.cleaned {
            return Ok(());
        }
        self.registry.cleanup(&self.root)?;
        self.cleaned = true;
        Ok(())
    }

    /// Preserve this marker-owned directory for the next lease holder.
    ///
    /// This is used when external process termination cannot be proven. An
    /// explicit later `cleanup` can still remove it after the process boundary
    /// becomes safe; ordinary Drop leaves it for startup reconciliation.
    pub fn retain_for_reconciliation(&mut self) {
        self.cleanup_on_drop = false;
    }
}

impl Drop for OwnedStorageV1 {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            let _ = self.cleanup();
        }
    }
}

fn read_marker(root: &Path) -> Result<OwnedStorageMarkerV1, OwnedStorageErrorV1> {
    let path = root.join(STORAGE_OWNER_FILE);
    let metadata =
        fs::metadata(&path).map_err(|error| OwnedStorageErrorV1::Marker(error.to_string()))?;
    if !metadata.is_file() {
        return Err(OwnedStorageErrorV1::Marker(format!(
            "marker is not a regular file: {}",
            path.display()
        )));
    }
    let encoded =
        fs::read(&path).map_err(|error| OwnedStorageErrorV1::Marker(error.to_string()))?;
    serde_json::from_slice(&encoded).map_err(|error| OwnedStorageErrorV1::Marker(error.to_string()))
}

fn validate_marker(
    marker: &OwnedStorageMarkerV1,
    root: &Path,
    source_game_directory: &Path,
    repository: &Path,
    expected_owner_id: Option<&str>,
) -> Result<(), OwnedStorageErrorV1> {
    if !validate_repository_owner_id_v1(&marker.owner_id)
        || marker.schema != STORAGE_OWNER_SCHEMA
        || marker.process_containment != PROCESS_CONTAINMENT_CLASS_V1
        || expected_owner_id.is_some_and(|expected| marker.owner_id != expected)
    {
        return Err(OwnedStorageErrorV1::Marker(
            "invalid ownership identity".to_owned(),
        ));
    }
    let isolation_id = Uuid::parse_str(&marker.isolation_id)
        .map_err(|_| OwnedStorageErrorV1::Marker("invalid isolation UUID".to_owned()))?;
    let source_game_directory = canonical_directory(
        source_game_directory,
        "source game directory",
        OwnedStorageErrorV1::SourceGameDirectory,
    )?;
    let repository = canonical_directory(
        repository,
        "workbench repository",
        OwnedStorageErrorV1::Marker,
    )?;
    let root = canonical_directory(root, "owned storage root", OwnedStorageErrorV1::StorageRoot)?;
    let parent = storage_parent(&source_game_directory);
    let expected_root = parent.join(isolation_id.hyphenated().to_string());
    if root != expected_root
        || root.parent() != Some(parent.as_path())
        || !same_path(
            Path::new(&marker.source_game_directory),
            &source_game_directory,
        )
        || !same_path(Path::new(&marker.repository), &repository)
    {
        return Err(OwnedStorageErrorV1::RefusedRoot(root));
    }
    Ok(())
}

fn remove_owned_storage_root(root: &Path) -> Result<(), OwnedStorageErrorV1> {
    for junction in [root.join("mods"), root.join("jre")] {
        if junction.exists() {
            fs::remove_dir(&junction)
                .map_err(|error| OwnedStorageErrorV1::Cleanup(error.to_string()))?;
        }
    }
    fs::remove_dir_all(root).map_err(|error| OwnedStorageErrorV1::Cleanup(error.to_string()))
}

fn storage_parent(source_game_directory: &Path) -> PathBuf {
    source_game_directory
        .join(".sts-java-play")
        .join("isolated-storage")
}

fn canonical_directory(
    path: &Path,
    label: &str,
    error: impl Fn(String) -> OwnedStorageErrorV1,
) -> Result<PathBuf, OwnedStorageErrorV1> {
    let canonical = fs::canonicalize(path).map_err(|detail| {
        error(format!(
            "failed to resolve {label} {}: {detail}",
            path.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(error(format!(
            "{label} is not a directory: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalized_windows_path(left).eq_ignore_ascii_case(&normalized_windows_path(right))
}

fn normalized_windows_path(path: &Path) -> String {
    let rendered = path.to_string_lossy();
    if let Some(tail) = rendered.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{tail}")
    } else if let Some(tail) = rendered.strip_prefix(r"\\?\") {
        tail.to_owned()
    } else {
        rendered.into_owned()
    }
}

fn write_json_new(path: &Path, value: &impl Serialize) -> Result<(), OwnedStorageErrorV1> {
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|error| OwnedStorageErrorV1::Marker(error.to_string()))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| OwnedStorageErrorV1::Marker(error.to_string()))?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_data())
        .map_err(|error| OwnedStorageErrorV1::Marker(error.to_string()))
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn startup_reconciliation_only_removes_strictly_owned_storage() {
        let base = env::temp_dir().join(format!("sts-workbench-reconcile-{}", Uuid::new_v4()));
        let repository = base.join("repository");
        let foreign_repository = base.join("foreign-repository");
        let game_directory = base.join("game");
        let parent = storage_parent(&game_directory);
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(&foreign_repository).unwrap();
        fs::create_dir_all(&parent).unwrap();
        let registry = fixture_registry(&repository, &game_directory);
        let repository = fs::canonicalize(repository).unwrap();
        let foreign_repository = fs::canonicalize(foreign_repository).unwrap();
        let game_directory = fs::canonicalize(game_directory).unwrap();

        let owned_id = Uuid::new_v4();
        let owned_root = parent.join(owned_id.hyphenated().to_string());
        fs::create_dir(&owned_root).unwrap();
        write_json_new(
            &owned_root.join(STORAGE_OWNER_FILE),
            &fixture_marker(&registry.owner_id, &repository, &game_directory, owned_id),
        )
        .unwrap();

        let foreign_id = Uuid::new_v4();
        let foreign_root = parent.join(foreign_id.hyphenated().to_string());
        fs::create_dir(&foreign_root).unwrap();
        write_json_new(
            &foreign_root.join(STORAGE_OWNER_FILE),
            &fixture_marker(
                &registry.owner_id,
                &foreign_repository,
                &game_directory,
                foreign_id,
            ),
        )
        .unwrap();

        let unknown_root = parent.join(Uuid::new_v4().hyphenated().to_string());
        fs::create_dir(&unknown_root).unwrap();
        fs::write(unknown_root.join(STORAGE_OWNER_FILE), b"{}").unwrap();

        let report = registry.reconcile_startup_and_persist().unwrap();
        assert_eq!(report.reclaimed_storage.len(), 1);
        assert!(same_path(
            Path::new(&report.reclaimed_storage[0]),
            &owned_root
        ));
        assert_eq!(report.foreign_owned_storage.len(), 1);
        assert!(same_path(
            Path::new(&report.foreign_owned_storage[0]),
            &foreign_root
        ));
        assert_eq!(report.unrecognized_markers.len(), 1);
        assert!(same_path(
            Path::new(&report.unrecognized_markers[0].path),
            &unknown_root
        ));
        assert!(report.unrecognized_markers[0]
            .reason
            .contains("owned-storage marker failed"));
        assert!(!owned_root.exists());
        assert!(foreign_root.exists());
        assert!(unknown_root.exists());
        let report_directory = registry.runtime_directory.join("startup-reconciliation");
        let reports = fs::read_dir(report_directory)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(reports.len(), 1);
        let persisted: OwnedStorageReconciliationV1 =
            serde_json::from_slice(&fs::read(reports[0].path()).unwrap()).unwrap();
        assert_eq!(persisted, report);

        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn created_storage_is_marker_guarded_and_cleanup_is_owner_scoped() {
        let base = env::temp_dir().join(format!("sts-workbench-storage-{}", Uuid::new_v4()));
        let repository = base.join("repository");
        let game_directory = base.join("game");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(&game_directory).unwrap();
        let registry = fixture_registry(&repository, &game_directory);
        let mut storage = registry.create().unwrap();
        let root = storage.root().to_path_buf();
        assert!(root.join(STORAGE_OWNER_FILE).is_file());
        let marker = read_marker(&root).unwrap();
        assert_eq!(marker.schema, STORAGE_OWNER_SCHEMA);
        assert_eq!(marker.owner_id, registry.owner_id);
        assert_eq!(marker.process_containment, PROCESS_CONTAINMENT_CLASS_V1);
        assert!(same_path(
            Path::new(&marker.repository),
            &registry.repository
        ));
        assert!(same_path(
            Path::new(&marker.source_game_directory),
            storage.source_game_directory()
        ));
        fs::create_dir(root.join("mods")).unwrap();
        fs::create_dir(root.join("jre")).unwrap();

        let mut foreign = registry.clone();
        foreign.owner_id = format!("workbench-actor-v1-{}", Uuid::new_v4().simple());
        assert!(foreign.cleanup(&root).is_err());
        assert!(root.is_dir());

        storage.cleanup().unwrap();
        assert!(!root.exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn retained_storage_survives_drop_and_is_reclaimed_by_the_same_owner() {
        let base = env::temp_dir().join(format!("sts-workbench-retained-{}", Uuid::new_v4()));
        let repository = base.join("repository");
        let game_directory = base.join("game");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(&game_directory).unwrap();
        let registry = fixture_registry(&repository, &game_directory);
        let mut storage = registry.create().unwrap();
        let root = storage.root().to_path_buf();
        storage.retain_for_reconciliation();
        drop(storage);
        assert!(root.is_dir());

        let report = registry.reconcile_startup_and_persist().unwrap();
        assert_eq!(report.reclaimed_storage.len(), 1);
        assert!(!root.exists());
        fs::remove_dir_all(base).unwrap();
    }

    fn fixture_registry(repository: &Path, game_directory: &Path) -> OwnedStorageRegistryV1 {
        OwnedStorageRegistryV1 {
            _repository_lease: Arc::new(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(repository.join("test-owner.lock"))
                    .unwrap(),
            ),
            owner_id: format!("workbench-actor-v1-{}", Uuid::new_v4().simple()),
            repository: fs::canonicalize(repository).unwrap(),
            runtime_directory: repository.join(".oracle-lab").join("workbench-runtime"),
            source_game_directory: fs::canonicalize(game_directory).unwrap(),
        }
    }

    fn fixture_marker(
        owner_id: &str,
        repository: &Path,
        game_directory: &Path,
        isolation_id: Uuid,
    ) -> OwnedStorageMarkerV1 {
        OwnedStorageMarkerV1 {
            schema: STORAGE_OWNER_SCHEMA.to_owned(),
            owner_id: owner_id.to_owned(),
            repository: repository.to_string_lossy().into_owned(),
            source_game_directory: game_directory.to_string_lossy().into_owned(),
            isolation_id: isolation_id.hyphenated().to_string(),
            process_containment: PROCESS_CONTAINMENT_CLASS_V1.to_owned(),
            created_at_unix_ms: now_unix_ms(),
        }
    }
}
