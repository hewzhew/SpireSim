//! Game-agnostic content-addressed blobs and directory bundles.
//!
//! The store owns byte identity, safe relative paths, immutable publication,
//! and create-new materialization. It deliberately knows nothing about paired
//! roots, Java processes, scenes, or recovery eligibility.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

pub const BUNDLE_SCHEMA_V1: &str = "sts-paired-bundle-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BlobRefV1 {
    pub sha256: String,
    pub bytes: u64,
    pub media_type: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleFileV1 {
    pub relative_path: String,
    pub blob: BlobRefV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifestV1 {
    pub schema: String,
    pub files: Vec<BundleFileV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleRefV1 {
    pub bundle_id: String,
    pub manifest: BlobRefV1,
}

pub struct ContentBundleStoreV1 {
    root: PathBuf,
    verification_cache: Option<Mutex<VerificationCacheV1>>,
}

#[derive(Default)]
struct VerificationCacheV1 {
    blobs: BTreeSet<(String, u64, String)>,
    bundles: BTreeMap<(String, String, u64, String), BundleManifestV1>,
}

impl ContentBundleStoreV1 {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            verification_cache: None,
        }
    }

    /// Cache successful immutable verification only within one caller-owned
    /// operation. A fresh store starts with an empty cache and re-audits disk.
    pub fn with_operation_verification_cache(mut self) -> Self {
        self.verification_cache = Some(Mutex::new(VerificationCacheV1::default()));
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn initialize(&self) -> Result<(), String> {
        for path in [self.blobs_dir(), self.bundles_dir()] {
            fs::create_dir_all(&path)
                .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
        }
        Ok(())
    }

    pub fn put_bytes(&self, media_type: &str, bytes: &[u8]) -> Result<BlobRefV1, String> {
        self.initialize()?;
        require_token(media_type, "blob media_type")?;
        let sha256 = sha256_hex(bytes);
        let reference = BlobRefV1 {
            sha256: sha256.clone(),
            bytes: u64::try_from(bytes.len()).map_err(|_| "blob length overflowed u64")?,
            media_type: media_type.to_owned(),
        };
        publish_immutable(&self.blob_path(&sha256), bytes)?;
        self.verify_blob(&reference)?;
        Ok(reference)
    }

    pub fn put_file(&self, media_type: &str, path: &Path) -> Result<BlobRefV1, String> {
        let bytes = fs::read(path)
            .map_err(|error| format!("failed to read blob source {}: {error}", path.display()))?;
        self.put_bytes(media_type, &bytes)
    }

    pub fn put_directory(&self, directory: &Path) -> Result<BundleRefV1, String> {
        self.initialize()?;
        let canonical = fs::canonicalize(directory).map_err(|error| {
            format!(
                "failed to resolve bundle directory {}: {error}",
                directory.display()
            )
        })?;
        if !canonical.is_dir() {
            return Err(format!(
                "bundle source is not a directory: {}",
                canonical.display()
            ));
        }
        let mut paths = Vec::new();
        collect_regular_files(&canonical, &canonical, &mut paths)?;
        paths.sort_by(|left, right| left.0.cmp(&right.0));
        let files = paths
            .into_iter()
            .map(|(relative_path, path)| {
                Ok(BundleFileV1 {
                    relative_path,
                    blob: self.put_file("application/octet-stream", &path)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.publish_bundle_manifest(BundleManifestV1 {
            schema: BUNDLE_SCHEMA_V1.to_owned(),
            files,
        })
    }

    /// Capture a fixed set of direct child directories and prove that their
    /// file set and bytes did not change while the bundle was assembled.
    ///
    /// This is intended for a caller-owned physical capture scope. Unselected
    /// siblings are never traversed, so links or runtime-only files outside
    /// that pre-bound scope cannot leak into the bundle.
    pub fn put_selected_directories_consistent(
        &self,
        directory: &Path,
        relative_directories: &[String],
    ) -> Result<BundleRefV1, String> {
        self.initialize()?;
        let canonical = fs::canonicalize(directory).map_err(|error| {
            format!(
                "failed to resolve bundle directory {}: {error}",
                directory.display()
            )
        })?;
        if !canonical.is_dir() {
            return Err(format!(
                "bundle source is not a directory: {}",
                canonical.display()
            ));
        }
        if relative_directories.is_empty()
            || relative_directories
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || relative_directories
                .iter()
                .any(|relative| !safe_direct_child(relative))
        {
            return Err("selected bundle directories are not canonical direct children".to_owned());
        }
        let before = self.collect_selected_files(&canonical, relative_directories)?;
        let files = before
            .iter()
            .map(|(relative_path, path)| {
                Ok(BundleFileV1 {
                    relative_path: relative_path.clone(),
                    blob: self.put_file("application/octet-stream", path)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let after = self.collect_selected_files(&canonical, relative_directories)?;
        let after_files = after
            .iter()
            .map(|(relative_path, path)| {
                Ok(BundleFileV1 {
                    relative_path: relative_path.clone(),
                    blob: self.put_file("application/octet-stream", path)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        if files != after_files {
            return Err("selected bundle source changed during capture".to_owned());
        }
        self.publish_bundle_manifest(BundleManifestV1 {
            schema: BUNDLE_SCHEMA_V1.to_owned(),
            files,
        })
    }

    pub fn verify_blob(&self, reference: &BlobRefV1) -> Result<(), String> {
        require_sha256(&reference.sha256, "paired blob sha256")?;
        require_token(&reference.media_type, "paired blob media_type")?;
        let cache_key = (
            reference.sha256.clone(),
            reference.bytes,
            reference.media_type.clone(),
        );
        if self
            .lock_operation_verification_cache()?
            .is_some_and(|cache| cache.blobs.contains(&cache_key))
        {
            return Ok(());
        }
        let bytes = fs::read(self.blob_path(&reference.sha256))
            .map_err(|error| format!("paired blob {} is unavailable: {error}", reference.sha256))?;
        if u64::try_from(bytes.len()).ok() != Some(reference.bytes)
            || sha256_hex(&bytes) != reference.sha256
        {
            return Err(format!("paired blob {} changed", reference.sha256));
        }
        if let Some(mut cache) = self.lock_operation_verification_cache()? {
            cache.blobs.insert(cache_key);
        }
        Ok(())
    }

    pub fn read_blob(&self, reference: &BlobRefV1) -> Result<Vec<u8>, String> {
        self.verify_blob(reference)?;
        fs::read(self.blob_path(&reference.sha256))
            .map_err(|error| format!("failed to read paired blob {}: {error}", reference.sha256))
    }

    pub fn read_bundle_file(
        &self,
        reference: &BundleRefV1,
        relative_path: &str,
    ) -> Result<Vec<u8>, String> {
        if !safe_relative_bundle_path(relative_path) {
            return Err("paired bundle file request has an unsafe relative path".to_owned());
        }
        let manifest = self.verify_bundle(reference)?;
        let file = manifest
            .files
            .iter()
            .find(|file| file.relative_path == relative_path)
            .ok_or_else(|| format!("paired bundle omits file {relative_path:?}"))?;
        self.read_blob(&file.blob)
    }

    pub fn materialize_bundle_new(
        &self,
        reference: &BundleRefV1,
        target: &Path,
    ) -> Result<BundleManifestV1, String> {
        let manifest = self.verify_bundle(reference)?;
        if target.exists() {
            return Err(format!(
                "paired bundle materialization target already exists: {}",
                target.display()
            ));
        }
        fs::create_dir(target).map_err(|error| {
            format!(
                "failed to create paired bundle materialization {}: {error}",
                target.display()
            )
        })?;
        for file in &manifest.files {
            let destination = file
                .relative_path
                .split('/')
                .fold(target.to_path_buf(), |path, component| path.join(component));
            let parent = destination
                .parent()
                .ok_or_else(|| "paired bundle materialization file has no parent".to_owned())?;
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create paired bundle materialization directory {}: {error}",
                    parent.display()
                )
            })?;
            let bytes = self.read_blob(&file.blob)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .map_err(|error| {
                    format!(
                        "failed to create paired bundle materialization file {}: {error}",
                        destination.display()
                    )
                })?;
            output.write_all(&bytes).map_err(|error| {
                format!(
                    "failed to write paired bundle materialization file {}: {error}",
                    destination.display()
                )
            })?;
        }
        Ok(manifest)
    }

    pub fn verify_bundle(&self, reference: &BundleRefV1) -> Result<BundleManifestV1, String> {
        require_content_id(&reference.bundle_id, "paired-bundle-v1", "paired bundle ID")?;
        let cache_key = (
            reference.bundle_id.clone(),
            reference.manifest.sha256.clone(),
            reference.manifest.bytes,
            reference.manifest.media_type.clone(),
        );
        if let Some(cache) = self.lock_operation_verification_cache()? {
            if let Some(manifest) = cache.bundles.get(&cache_key).cloned() {
                return Ok(manifest);
            }
        }
        let bytes = self.read_blob(&reference.manifest)?;
        let manifest: BundleManifestV1 = serde_json::from_slice(&bytes)
            .map_err(|error| format!("failed to decode paired bundle manifest: {error}"))?;
        validate_bundle_manifest(&manifest)?;
        let expected_id = content_id("paired-bundle-v1", &canonical_bytes(&manifest)?);
        if reference.bundle_id != expected_id {
            return Err("paired bundle content identity is invalid".to_owned());
        }
        let stored: BundleManifestV1 = read_json(&self.bundle_path(&reference.bundle_id))?;
        if stored != manifest {
            return Err("paired bundle index disagrees with its manifest blob".to_owned());
        }
        for file in &manifest.files {
            self.verify_blob(&file.blob)?;
        }
        if let Some(mut cache) = self.lock_operation_verification_cache()? {
            cache.bundles.insert(cache_key, manifest.clone());
        }
        Ok(manifest)
    }

    #[doc(hidden)]
    pub fn operation_verification_cache_counts(&self) -> Option<(usize, usize)> {
        self.verification_cache.as_ref().map(|cache| {
            let cache = cache.lock().unwrap();
            (cache.blobs.len(), cache.bundles.len())
        })
    }

    fn lock_operation_verification_cache(
        &self,
    ) -> Result<Option<MutexGuard<'_, VerificationCacheV1>>, String> {
        self.verification_cache
            .as_ref()
            .map(|cache| {
                cache.lock().map_err(|_| {
                    "content bundle operation verification cache is poisoned".to_owned()
                })
            })
            .transpose()
    }

    fn collect_selected_files(
        &self,
        root: &Path,
        relative_directories: &[String],
    ) -> Result<Vec<(String, PathBuf)>, String> {
        let mut paths = Vec::new();
        for relative in relative_directories {
            let selected = root.join(relative);
            let metadata = fs::symlink_metadata(&selected).map_err(|error| {
                format!("failed to inspect selected bundle directory {relative:?}: {error}")
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(format!(
                    "selected bundle member is not a physical directory: {relative:?}"
                ));
            }
            let resolved = fs::canonicalize(&selected).map_err(|error| {
                format!("failed to resolve selected bundle directory {relative:?}: {error}")
            })?;
            if resolved.parent() != Some(root) {
                return Err(format!(
                    "selected bundle directory escaped its root: {relative:?}"
                ));
            }
            collect_regular_files(root, &resolved, &mut paths)?;
        }
        paths.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(paths)
    }

    fn publish_bundle_manifest(&self, manifest: BundleManifestV1) -> Result<BundleRefV1, String> {
        let manifest_bytes = canonical_bytes(&manifest)?;
        let bundle_id = content_id("paired-bundle-v1", &manifest_bytes);
        let manifest_ref =
            self.put_bytes("application/vnd.sts.paired-bundle+json", &manifest_bytes)?;
        publish_json_immutable(&self.bundle_path(&bundle_id), &manifest)?;
        let reference = BundleRefV1 {
            bundle_id,
            manifest: manifest_ref,
        };
        self.verify_bundle(&reference)?;
        Ok(reference)
    }

    fn blobs_dir(&self) -> PathBuf {
        self.root.join("blobs").join("sha256")
    }

    fn bundles_dir(&self) -> PathBuf {
        self.root.join("bundles")
    }

    fn blob_path(&self, sha256: &str) -> PathBuf {
        self.blobs_dir().join(sha256)
    }

    fn bundle_path(&self, bundle_id: &str) -> PathBuf {
        self.bundles_dir().join(format!("{bundle_id}.json"))
    }
}

fn validate_bundle_manifest(manifest: &BundleManifestV1) -> Result<(), String> {
    if manifest.schema != BUNDLE_SCHEMA_V1 {
        return Err("paired bundle schema is invalid".to_owned());
    }
    if manifest
        .files
        .windows(2)
        .any(|pair| pair[0].relative_path >= pair[1].relative_path)
    {
        return Err("paired bundle files are not in canonical order".to_owned());
    }
    for file in &manifest.files {
        if !safe_relative_bundle_path(&file.relative_path) {
            return Err("paired bundle contains an unsafe relative path".to_owned());
        }
    }
    Ok(())
}

fn collect_regular_files(
    root: &Path,
    directory: &Path,
    output: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to inspect {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to enumerate {}: {error}", directory.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "bundle source contains a link: {}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            collect_regular_files(root, &entry.path(), output)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| "bundle file escaped its root")?
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if !safe_relative_bundle_path(&relative) {
                return Err("bundle file has an unsafe relative path".to_owned());
            }
            output.push((relative, entry.path()));
        } else {
            return Err(format!(
                "bundle source contains a special file: {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

fn safe_relative_bundle_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains(':')
        && !value.chars().any(char::is_control)
        && value
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn safe_direct_child(value: &str) -> bool {
    safe_relative_bundle_path(value) && !value.contains('/')
}

fn publish_json_immutable<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode {}: {error}", path.display()))?;
    publish_immutable(path, &bytes)
}

fn publish_immutable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file
            .write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("failed to publish {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = fs::read(path)
                .map_err(|read_error| format!("failed to read {}: {read_error}", path.display()))?;
            if existing == bytes {
                Ok(())
            } else {
                Err(format!(
                    "immutable artifact already exists with different bytes: {}",
                    path.display()
                ))
            }
        }
        Err(error) => Err(format!("failed to create {}: {error}", path.display())),
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode {}: {error}", path.display()))
}

fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|error| format!("failed to encode canonical JSON: {error}"))
}

fn content_id(domain: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    format!("{domain}-{}", hex_lower(&hasher.finalize()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn require_sha256(value: &str, label: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{label} must be 64 lowercase hexadecimal bytes"));
    }
    Ok(())
}

fn require_content_id(value: &str, domain: &str, label: &str) -> Result<(), String> {
    let Some(digest) = value
        .strip_prefix(domain)
        .and_then(|value| value.strip_prefix('-'))
    else {
        return Err(format!("{label} has the wrong content-address domain"));
    };
    require_sha256(digest, label)
}

fn require_token(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(format!(
            "{label} must be a non-control token of 1..=256 bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn directory_bundle_round_trips_with_operation_local_cache() {
        let base = temporary_directory("round-trip");
        let source = base.join("source");
        fs::create_dir_all(source.join("nested")).unwrap();
        fs::write(source.join("a.txt"), b"alpha").unwrap();
        fs::write(source.join("nested").join("b.txt"), b"beta").unwrap();

        let store =
            ContentBundleStoreV1::new(base.join("store")).with_operation_verification_cache();
        let bundle = store.put_directory(&source).unwrap();
        let manifest = store.verify_bundle(&bundle).unwrap();
        assert_eq!(
            manifest
                .files
                .iter()
                .map(|file| file.relative_path.as_str())
                .collect::<Vec<_>>(),
            vec!["a.txt", "nested/b.txt"]
        );
        let after_first = store.operation_verification_cache_counts().unwrap();
        assert_eq!(store.verify_bundle(&bundle).unwrap(), manifest);
        assert_eq!(
            store.operation_verification_cache_counts().unwrap(),
            after_first
        );

        let restored = base.join("restored");
        store.materialize_bundle_new(&bundle, &restored).unwrap();
        assert_eq!(fs::read(restored.join("a.txt")).unwrap(), b"alpha");
        assert_eq!(
            fs::read(restored.join("nested").join("b.txt")).unwrap(),
            b"beta"
        );
        fs::remove_dir_all(base).unwrap();
    }

    fn temporary_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sts-content-bundle-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }
}
