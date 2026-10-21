use fs2::FileExt;
use serde::Serialize;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::{ProcessContainmentErrorV1, ProcessContainmentV1, PROCESS_CONTAINMENT_CLASS_V1};

const RUNTIME_LEASE_SCHEMA: &str = "sts-java-parity-repository-runtime-lease-v1";
const LEGACY_OWNER_ID_PREFIX: &str = "workbench-actor-v1-";
const WORKBENCH_OWNER_ID_PREFIX: &str = "java-runtime-workbench-v1-";
const LOCKSTEP_OWNER_ID_PREFIX: &str = "java-runtime-lockstep-v1-";
const ORACLE_HOST_OWNER_ID_PREFIX: &str = "java-runtime-parity-host-v1-";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryLifecycleRoleV1 {
    WorkbenchActor,
    LockstepRunner,
    ParityOracleHost,
}

impl RepositoryLifecycleRoleV1 {
    const fn owner_prefix(self) -> &'static str {
        match self {
            Self::WorkbenchActor => WORKBENCH_OWNER_ID_PREFIX,
            Self::LockstepRunner => LOCKSTEP_OWNER_ID_PREFIX,
            Self::ParityOracleHost => ORACLE_HOST_OWNER_ID_PREFIX,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::WorkbenchActor => "workbench",
            Self::LockstepRunner => "lockstep",
            Self::ParityOracleHost => "parity_oracle_host",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum RepositoryOwnerErrorV1 {
    ResolveRepository(String),
    CreateRuntimeDirectory(String),
    OpenLease(String),
    AlreadyOwned { repository: PathBuf, detail: String },
    ProcessContainment(ProcessContainmentErrorV1),
    EncodeLease(String),
    RewriteLease(String),
}

impl fmt::Display for RepositoryOwnerErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResolveRepository(detail) => {
                write!(
                    formatter,
                    "failed to resolve workbench repository: {detail}"
                )
            }
            Self::CreateRuntimeDirectory(detail) => {
                write!(
                    formatter,
                    "failed to create workbench runtime directory: {detail}"
                )
            }
            Self::OpenLease(detail) => {
                write!(formatter, "failed to open workbench actor lease: {detail}")
            }
            Self::AlreadyOwned { repository, detail } => write!(
                formatter,
                "another workbench actor already owns repository {}: {detail}",
                repository.display()
            ),
            Self::ProcessContainment(error) => {
                write!(
                    formatter,
                    "failed to acquire workbench process containment: {error}"
                )
            }
            Self::EncodeLease(detail) => {
                write!(formatter, "failed to encode locked JSON: {detail}")
            }
            Self::RewriteLease(detail) => {
                write!(formatter, "failed to rewrite actor lease: {detail}")
            }
        }
    }
}

impl std::error::Error for RepositoryOwnerErrorV1 {}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeLeaseDocumentV1 {
    schema: String,
    owner_id: String,
    process_id: u32,
    repository: String,
    process_containment: String,
    role: String,
    acquired_at_unix_ms: u128,
}

/// Process-lifetime ownership of one repository's external runtime resources.
///
/// This type deliberately knows nothing about game directories, isolated
/// storage, scenes, or paired roots. It acquires the repository lease before
/// process containment, writes the existing lease schema for compatibility,
/// and keeps both handles alive for the lifetime of the owner process.
pub struct RepositoryLifecycleOwnerV1 {
    owner_id: String,
    repository: PathBuf,
    runtime_directory: PathBuf,
    lease: Arc<File>,
    _process_containment: ProcessContainmentV1,
}

impl RepositoryLifecycleOwnerV1 {
    pub fn acquire(repository: &Path) -> Result<Self, RepositoryOwnerErrorV1> {
        Self::acquire_for(repository, RepositoryLifecycleRoleV1::WorkbenchActor)
    }

    pub fn acquire_for(
        repository: &Path,
        role: RepositoryLifecycleRoleV1,
    ) -> Result<Self, RepositoryOwnerErrorV1> {
        let repository = fs::canonicalize(repository)
            .map_err(|error| RepositoryOwnerErrorV1::ResolveRepository(error.to_string()))?;
        if !repository.is_dir() {
            return Err(RepositoryOwnerErrorV1::ResolveRepository(format!(
                "not a directory: {}",
                repository.display()
            )));
        }

        let runtime_directory = repository.join(".oracle-lab").join("workbench-runtime");
        fs::create_dir_all(&runtime_directory)
            .map_err(|error| RepositoryOwnerErrorV1::CreateRuntimeDirectory(error.to_string()))?;
        let mut lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(runtime_directory.join("java-runtime.lock"))
            .map_err(|error| RepositoryOwnerErrorV1::OpenLease(error.to_string()))?;
        lease
            .try_lock_exclusive()
            .map_err(|error| RepositoryOwnerErrorV1::AlreadyOwned {
                repository: repository.clone(),
                detail: error.to_string(),
            })?;

        let owner_id = format!("{}{}", role.owner_prefix(), Uuid::new_v4().simple());
        let process_containment = ProcessContainmentV1::acquire_current_process()
            .map_err(RepositoryOwnerErrorV1::ProcessContainment)?;
        rewrite_locked_json(
            &mut lease,
            &RuntimeLeaseDocumentV1 {
                schema: RUNTIME_LEASE_SCHEMA.to_owned(),
                owner_id: owner_id.clone(),
                process_id: std::process::id(),
                repository: repository.to_string_lossy().into_owned(),
                process_containment: PROCESS_CONTAINMENT_CLASS_V1.to_owned(),
                role: role.label().to_owned(),
                acquired_at_unix_ms: now_unix_ms(),
            },
        )?;

        Ok(Self {
            owner_id,
            repository,
            runtime_directory,
            lease: Arc::new(lease),
            _process_containment: process_containment,
        })
    }

    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }

    pub fn repository(&self) -> &Path {
        &self.repository
    }

    pub fn runtime_directory(&self) -> &Path {
        &self.runtime_directory
    }

    pub(crate) fn lease_guard(&self) -> Arc<File> {
        Arc::clone(&self.lease)
    }
}

pub fn validate_repository_owner_id_v1(owner_id: &str) -> bool {
    [
        LEGACY_OWNER_ID_PREFIX,
        WORKBENCH_OWNER_ID_PREFIX,
        LOCKSTEP_OWNER_ID_PREFIX,
        ORACLE_HOST_OWNER_ID_PREFIX,
    ]
    .iter()
    .find_map(|prefix| owner_id.strip_prefix(prefix))
    .is_some_and(|suffix| Uuid::parse_str(suffix).is_ok())
}

fn rewrite_locked_json(
    file: &mut File,
    value: &impl Serialize,
) -> Result<(), RepositoryOwnerErrorV1> {
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|error| RepositoryOwnerErrorV1::EncodeLease(error.to_string()))?;
    file.set_len(0)
        .and_then(|_| file.seek(SeekFrom::Start(0)).map(|_| ()))
        .and_then(|_| file.write_all(&encoded))
        .and_then(|_| file.sync_data())
        .map_err(|error| RepositoryOwnerErrorV1::RewriteLease(error.to_string()))
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

    #[test]
    fn owner_identity_is_closed_and_versioned() {
        assert!(validate_repository_owner_id_v1(
            "workbench-actor-v1-0123456789abcdef0123456789abcdef"
        ));
        assert!(!validate_repository_owner_id_v1(
            "workbench-supervisor-v1-0123456789abcdef0123456789abcdef"
        ));
        assert!(validate_repository_owner_id_v1(
            "java-runtime-workbench-v1-0123456789abcdef0123456789abcdef"
        ));
        assert!(validate_repository_owner_id_v1(
            "java-runtime-lockstep-v1-0123456789abcdef0123456789abcdef"
        ));
        assert!(!validate_repository_owner_id_v1("workbench-actor-v1-"));
        assert!(!validate_repository_owner_id_v1(
            "workbench-actor-v1-not-a-uuid"
        ));
    }
}
