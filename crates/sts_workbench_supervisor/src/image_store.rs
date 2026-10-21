use crate::EngineContractV1;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMPORARY_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Eq, PartialEq)]
pub enum EngineImageErrorV1 {
    SourceUnavailable(String),
    SourceNotFile,
    StoreUnavailable(String),
    CopyFailed(String),
    SourceChangedDuringMaterialization,
    CorruptImage,
    PublishFailed(String),
    ContractMismatch,
}

impl fmt::Display for EngineImageErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for EngineImageErrorV1 {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineImageV1 {
    image_id: String,
    executable: PathBuf,
    size_bytes: u64,
}

/// One immutable executable copy that can be launched without locking its
/// canonical Cargo output. The generic image has no Engine protocol meaning;
/// callers own the process contract layered on top of it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutableImageV1 {
    image_id: String,
    executable: PathBuf,
    size_bytes: u64,
}

impl ExecutableImageV1 {
    pub fn image_id(&self) -> &str {
        &self.image_id
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn command(&self) -> Result<Command, EngineImageErrorV1> {
        verify_image(&self.executable, &self.image_id, self.size_bytes)?;
        Ok(Command::new(&self.executable))
    }
}

/// Content-addressed images for a non-Engine executable such as the durable
/// workbench actor or its Java bridge. `filename_prefix` is a closed filename
/// token, never a path supplied by a child process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutableImageStoreV1 {
    root: PathBuf,
    filename_prefix: String,
}

impl ExecutableImageStoreV1 {
    pub fn new(
        root: impl Into<PathBuf>,
        filename_prefix: impl Into<String>,
    ) -> Result<Self, EngineImageErrorV1> {
        let filename_prefix = filename_prefix.into();
        if filename_prefix.is_empty()
            || filename_prefix.len() > 64
            || !filename_prefix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(EngineImageErrorV1::StoreUnavailable(
                "executable image prefix is not a closed filename token".to_owned(),
            ));
        }
        Ok(Self {
            root: root.into(),
            filename_prefix,
        })
    }

    pub fn materialize(&self, source: &Path) -> Result<ExecutableImageV1, EngineImageErrorV1> {
        let image = materialize_image(&self.root, &self.filename_prefix, source)?;
        Ok(ExecutableImageV1 {
            image_id: image.image_id,
            executable: image.executable,
            size_bytes: image.size_bytes,
        })
    }
}

impl EngineImageV1 {
    pub fn image_id(&self) -> &str {
        &self.image_id
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    /// Revalidates the immutable image and binds it to the advertised Engine
    /// contract before returning a launch command.
    pub fn command(&self, contract: &EngineContractV1) -> Result<Command, EngineImageErrorV1> {
        if contract.image_id != self.image_id {
            return Err(EngineImageErrorV1::ContractMismatch);
        }
        verify_image(&self.executable, &self.image_id, self.size_bytes)?;
        Ok(Command::new(&self.executable))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineImageStoreV1 {
    root: PathBuf,
}

impl EngineImageStoreV1 {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn materialize(&self, source: &Path) -> Result<EngineImageV1, EngineImageErrorV1> {
        let image = materialize_image(&self.root, "sts-workbench-engine", source)?;
        Ok(EngineImageV1 {
            image_id: image.image_id,
            executable: image.executable,
            size_bytes: image.size_bytes,
        })
    }
}

struct MaterializedImageV1 {
    image_id: String,
    executable: PathBuf,
    size_bytes: u64,
}

fn materialize_image(
    root: &Path,
    filename_prefix: &str,
    source: &Path,
) -> Result<MaterializedImageV1, EngineImageErrorV1> {
    let metadata = fs::metadata(source)
        .map_err(|error| EngineImageErrorV1::SourceUnavailable(error.to_string()))?;
    if !metadata.is_file() {
        return Err(EngineImageErrorV1::SourceNotFile);
    }
    fs::create_dir_all(root)
        .map_err(|error| EngineImageErrorV1::StoreUnavailable(error.to_string()))?;

    let image_id = file_sha256(source)
        .map_err(|error| EngineImageErrorV1::SourceUnavailable(error.to_string()))?;
    let executable = root.join(image_filename(filename_prefix, &image_id));
    if executable.exists() {
        verify_image(&executable, &image_id, metadata.len())?;
        return Ok(MaterializedImageV1 {
            image_id,
            executable,
            size_bytes: metadata.len(),
        });
    }

    let temporary = root.join(temporary_filename(filename_prefix));
    let result = (|| {
        copy_new(source, &temporary)?;
        verify_image(&temporary, &image_id, metadata.len())
            .map_err(|_| EngineImageErrorV1::SourceChangedDuringMaterialization)?;
        match fs::hard_link(&temporary, &executable) {
            Ok(()) => {}
            Err(_) if executable.is_file() => {
                verify_image(&executable, &image_id, metadata.len())?;
            }
            Err(error) => {
                return Err(EngineImageErrorV1::PublishFailed(error.to_string()));
            }
        }
        verify_image(&executable, &image_id, metadata.len())?;
        Ok(MaterializedImageV1 {
            image_id,
            executable,
            size_bytes: metadata.len(),
        })
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn copy_new(source: &Path, target: &Path) -> Result<(), EngineImageErrorV1> {
    let mut input =
        File::open(source).map_err(|error| EngineImageErrorV1::CopyFailed(error.to_string()))?;
    let source_permissions = input
        .metadata()
        .map_err(|error| EngineImageErrorV1::CopyFailed(error.to_string()))?
        .permissions();
    let mut output = File::options()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| EngineImageErrorV1::CopyFailed(error.to_string()))?;
    std::io::copy(&mut input, &mut output)
        .map_err(|error| EngineImageErrorV1::CopyFailed(error.to_string()))?;
    output
        .flush()
        .map_err(|error| EngineImageErrorV1::CopyFailed(error.to_string()))?;
    output
        .sync_all()
        .map_err(|error| EngineImageErrorV1::CopyFailed(error.to_string()))?;
    fs::set_permissions(target, source_permissions)
        .map_err(|error| EngineImageErrorV1::CopyFailed(error.to_string()))?;
    Ok(())
}

fn verify_image(path: &Path, image_id: &str, size_bytes: u64) -> Result<(), EngineImageErrorV1> {
    let metadata = fs::metadata(path).map_err(|_| EngineImageErrorV1::CorruptImage)?;
    let digest_matches = file_sha256(path).is_ok_and(|digest| digest == image_id);
    if !metadata.is_file() || metadata.len() != size_bytes || !digest_matches {
        return Err(EngineImageErrorV1::CorruptImage);
    }
    Ok(())
}

fn file_sha256(path: &Path) -> Result<String, std::io::Error> {
    let mut input = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn image_filename(filename_prefix: &str, image_id: &str) -> String {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    format!("{filename_prefix}-{image_id}{suffix}")
}

fn temporary_filename(filename_prefix: &str) -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT_TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        ".{filename_prefix}.tmp-{}-{nonce}-{sequence}",
        std::process::id()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialization_is_immutable_and_content_addressed() {
        let base = temporary_directory("content-addressed");
        let source = base.join("canonical-engine");
        let store = EngineImageStoreV1::new(base.join("images"));
        fs::create_dir_all(&base).unwrap();
        fs::write(&source, b"engine-v1").unwrap();

        let first = store.materialize(&source).unwrap();
        let repeated = store.materialize(&source).unwrap();
        assert_eq!(first, repeated);
        assert_eq!(fs::read(first.executable()).unwrap(), b"engine-v1");

        fs::write(&source, b"engine-v2").unwrap();
        let second = store.materialize(&source).unwrap();
        assert_ne!(first.image_id(), second.image_id());
        assert_ne!(first.executable(), second.executable());
        assert_eq!(fs::read(first.executable()).unwrap(), b"engine-v1");
        assert_eq!(fs::read(second.executable()).unwrap(), b"engine-v2");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn corrupt_existing_image_and_contract_mismatch_fail_closed() {
        let base = temporary_directory("corrupt-image");
        let source = base.join("canonical-engine");
        let store = EngineImageStoreV1::new(base.join("images"));
        fs::create_dir_all(&base).unwrap();
        fs::write(&source, b"engine-v1").unwrap();
        let image = store.materialize(&source).unwrap();

        let mut wrong_contract = contract(image.image_id());
        wrong_contract.image_id = "different-image".to_owned();
        assert_eq!(
            image.command(&wrong_contract).unwrap_err(),
            EngineImageErrorV1::ContractMismatch
        );
        fs::write(image.executable(), b"corrupt!!").unwrap();
        assert_eq!(
            store.materialize(&source).unwrap_err(),
            EngineImageErrorV1::CorruptImage
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn generic_executable_images_use_a_closed_independent_prefix() {
        let base = temporary_directory("generic-executable");
        let source = base.join("canonical-actor");
        fs::create_dir_all(&base).unwrap();
        fs::write(&source, b"actor-v1").unwrap();
        let store = ExecutableImageStoreV1::new(base.join("images"), "sts-actor").unwrap();

        let image = store.materialize(&source).unwrap();

        assert!(image
            .executable()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("sts-actor-"));
        assert_eq!(fs::read(image.executable()).unwrap(), b"actor-v1");
        assert!(image.command().is_ok());
        assert!(ExecutableImageStoreV1::new(base.join("bad"), "../escape").is_err());
        fs::remove_dir_all(base).unwrap();
    }

    fn contract(image_id: &str) -> EngineContractV1 {
        EngineContractV1 {
            image_id: image_id.to_owned(),
            supervisor_protocol: crate::SUPERVISOR_PROTOCOL_V2,
            java_build_id: "java-build-1".to_owned(),
            mod_build_id: "mod-build-1".to_owned(),
            minimum_java_protocol: 41,
            maximum_java_protocol: 41,
        }
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sts-workbench-engine-image-{label}-{}-{nonce}",
            std::process::id()
        ))
    }
}
