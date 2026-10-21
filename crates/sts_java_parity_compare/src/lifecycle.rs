use crate::{file_identity, file_identity_from_bytes, ComparisonFileIdentityV1};
use serde::Deserialize;
use std::fs;
use std::path::Path;

const ISOLATED_LIFECYCLE_SCHEMA: &str = "sts_java_play_isolated_lifecycle_v1";
const ISOLATED_STORAGE_CLASS: &str = "isolated_working_directory_v1";

pub(crate) struct IsolatedLifecycleExpectation<'a> {
    pub case_kind: &'a str,
    pub profile: &'a str,
    pub control_profile: &'a str,
    pub session_id: &'a str,
    pub isolation_id: &'a str,
    pub launch_plan_sha256: &'a str,
    pub case_evidence: &'a ComparisonFileIdentityV1,
}

pub(crate) struct VerifiedIsolatedLifecycleV1 {
    pub identity: ComparisonFileIdentityV1,
    pub total_ms: u128,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IsolatedLifecycleReceiptV1 {
    schema: String,
    case_kind: String,
    created_at_unix_ms: u128,
    completed_at_unix_ms: u128,
    passed: bool,
    failure: Option<String>,
    profile: String,
    control_profile: String,
    session_id: Option<String>,
    shutdown_failure: Option<String>,
    inventory_failure: Option<String>,
    launch_plan: Option<PowerShellFileIdentityV1>,
    case_evidence: Option<PowerShellFileIdentityV1>,
    timings_ms: IsolatedLifecycleTimingsV1,
    isolated_storage: IsolatedStorageBindingV1,
    installed_storage_guard: InstalledStorageGuardBindingV1,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PowerShellFileIdentityV1 {
    path: String,
    length: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IsolatedLifecycleTimingsV1 {
    #[serde(rename = "startup_timeout_budget")]
    _startup_timeout_budget: u128,
    #[serde(rename = "prepare_storage_and_guard")]
    _prepare_storage_and_guard: u128,
    #[serde(rename = "launch_to_session_ready")]
    _launch_to_session_ready: Option<u128>,
    #[serde(rename = "case_after_session_ready")]
    _case_after_session_ready: Option<u128>,
    #[serde(rename = "managed_shutdown")]
    _managed_shutdown: u128,
    #[serde(rename = "final_inventories")]
    _final_inventories: u128,
    #[serde(rename = "cleanup")]
    _cleanup: u128,
    total: u128,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IsolatedStorageBindingV1 {
    class: String,
    isolation_id: String,
    #[serde(rename = "working_directory")]
    _working_directory: String,
    retained: bool,
    cleanup_failure: Option<String>,
    #[serde(rename = "initial")]
    _initial: serde_json::Value,
    #[serde(rename = "final")]
    _final: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledStorageGuardBindingV1 {
    unchanged: bool,
    #[serde(rename = "before")]
    _before: serde_json::Value,
    #[serde(rename = "after")]
    _after: serde_json::Value,
}

pub(crate) fn verify_isolated_lifecycle(
    path: &Path,
    expected: IsolatedLifecycleExpectation<'_>,
) -> Result<VerifiedIsolatedLifecycleV1, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read isolated lifecycle receipt '{}': {error}",
            path.display()
        )
    })?;
    let lifecycle: IsolatedLifecycleReceiptV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode isolated lifecycle receipt: {error}"))?;
    let launch_plan = lifecycle
        .launch_plan
        .as_ref()
        .ok_or_else(|| "isolated lifecycle omitted its launch plan identity".to_owned())?;
    let case_evidence = lifecycle
        .case_evidence
        .as_ref()
        .ok_or_else(|| "isolated lifecycle omitted its case evidence identity".to_owned())?;
    validate_recorded_identity(launch_plan)?;
    validate_recorded_identity(case_evidence)?;

    if lifecycle.schema != ISOLATED_LIFECYCLE_SCHEMA
        || lifecycle.case_kind != expected.case_kind
        || lifecycle.created_at_unix_ms == 0
        || lifecycle.completed_at_unix_ms < lifecycle.created_at_unix_ms
        || !lifecycle.passed
        || lifecycle.failure.is_some()
        || lifecycle.profile != expected.profile
        || lifecycle.control_profile != expected.control_profile
        || lifecycle.session_id.as_deref() != Some(expected.session_id)
        || lifecycle.shutdown_failure.is_some()
        || lifecycle.inventory_failure.is_some()
        || lifecycle.isolated_storage.class != ISOLATED_STORAGE_CLASS
        || lifecycle.isolated_storage.isolation_id != expected.isolation_id
        || lifecycle.isolated_storage.retained
        || lifecycle.isolated_storage.cleanup_failure.is_some()
        || !lifecycle.installed_storage_guard.unchanged
        || launch_plan.sha256 != expected.launch_plan_sha256
        || !recorded_identity_matches(case_evidence, expected.case_evidence)
    {
        return Err(format!(
            "isolated lifecycle does not bind one clean '{}' run",
            expected.case_kind
        ));
    }

    Ok(VerifiedIsolatedLifecycleV1 {
        identity: file_identity_from_bytes(path, &bytes)?,
        total_ms: lifecycle.timings_ms.total,
    })
}

fn validate_recorded_identity(identity: &PowerShellFileIdentityV1) -> Result<(), String> {
    let actual = file_identity(Path::new(&identity.path))?;
    if !recorded_identity_matches(identity, &actual) {
        return Err(format!(
            "PowerShell lifecycle input changed after recording: {}",
            identity.path
        ));
    }
    Ok(())
}

fn recorded_identity_matches(
    recorded: &PowerShellFileIdentityV1,
    actual: &ComparisonFileIdentityV1,
) -> bool {
    same_path(Path::new(&recorded.path), Path::new(&actual.path))
        && recorded.length == actual.bytes
        && recorded.sha256 == actual.sha256
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fresh_test_directory() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "sts-java-parity-lifecycle-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn shared_lifecycle_verifier_binds_files_and_rejects_later_mutation() {
        let directory = fresh_test_directory();
        let launch_path = directory.join("launch.json");
        let evidence_path = directory.join("capture.json");
        let lifecycle_path = directory.join("lifecycle.json");
        fs::write(&launch_path, b"launch").unwrap();
        fs::write(&evidence_path, b"capture").unwrap();
        let launch = file_identity(&launch_path).unwrap();
        let evidence = file_identity(&evidence_path).unwrap();
        let recorded_identity = |identity: &ComparisonFileIdentityV1| {
            json!({
                "path": identity.path,
                "length": identity.bytes,
                "sha256": identity.sha256,
            })
        };
        let lifecycle = json!({
            "schema": ISOLATED_LIFECYCLE_SCHEMA,
            "case_kind": "test_case",
            "created_at_unix_ms": 1,
            "completed_at_unix_ms": 2,
            "passed": true,
            "failure": null,
            "profile": "accelerated",
            "control_profile": "parity_scenario_control",
            "session_id": "session",
            "shutdown_failure": null,
            "inventory_failure": null,
            "launch_plan": recorded_identity(&launch),
            "case_evidence": recorded_identity(&evidence),
            "timings_ms": {
                "startup_timeout_budget": 60_000,
                "prepare_storage_and_guard": 1,
                "launch_to_session_ready": 2,
                "case_after_session_ready": 3,
                "managed_shutdown": 4,
                "final_inventories": 5,
                "cleanup": 6,
                "total": 21,
            },
            "isolated_storage": {
                "class": ISOLATED_STORAGE_CLASS,
                "isolation_id": "isolation",
                "working_directory": directory,
                "retained": false,
                "cleanup_failure": null,
                "initial": {},
                "final": {},
            },
            "installed_storage_guard": {
                "unchanged": true,
                "before": {},
                "after": {},
            },
        });
        fs::write(
            &lifecycle_path,
            serde_json::to_vec_pretty(&lifecycle).unwrap(),
        )
        .unwrap();

        let expectation = || IsolatedLifecycleExpectation {
            case_kind: "test_case",
            profile: "accelerated",
            control_profile: "parity_scenario_control",
            session_id: "session",
            isolation_id: "isolation",
            launch_plan_sha256: &launch.sha256,
            case_evidence: &evidence,
        };
        let verified = verify_isolated_lifecycle(&lifecycle_path, expectation()).unwrap();
        assert_eq!(verified.total_ms, 21);

        let wrong_control = IsolatedLifecycleExpectation {
            control_profile: "parity_run_control",
            ..expectation()
        };
        assert!(verify_isolated_lifecycle(&lifecycle_path, wrong_control).is_err());

        fs::write(&evidence_path, b"mutated capture").unwrap();
        assert!(verify_isolated_lifecycle(&lifecycle_path, expectation()).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
