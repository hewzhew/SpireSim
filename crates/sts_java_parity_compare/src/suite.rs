use super::{
    current_unix_ms, file_identity, sha256_hex, write_json_report_new, ComparisonFileIdentityV1,
    ConstructedComparisonWorkflowV1,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use sts_java_parity_live::{
    scenario_case_catalog, scenario_probe_case_catalog, ConstructedScenarioProbeReport,
    ScenarioProbeTransition,
};

const MANIFEST_SCHEMA: &str = "sts_java_parity_constructed_suite_manifest_v2";
const PLAN_SCHEMA: &str = "sts_java_parity_constructed_suite_plan_v2";
const REPORT_SCHEMA: &str = "sts_java_parity_constructed_suite_report_v2";
const SHARED_MANIFEST_SCHEMA: &str = "sts_java_parity_shared_runtime_suite_manifest_v1";
const SHARED_PLAN_SCHEMA: &str = "sts_java_parity_shared_runtime_suite_plan_v1";

mod shared_runtime;
pub use shared_runtime::*;

mod shared_runtime_trajectory;
pub use shared_runtime_trajectory::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConstructedSuiteManifestV2 {
    pub schema: String,
    pub provenance: String,
    pub suite_id: String,
    pub members: Vec<ConstructedSuiteMemberSpecV2>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConstructedSuiteMemberSpecV2 {
    pub probe_case_id: String,
    pub launch_profile: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConstructedSuitePlanV2 {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub suite_id: String,
    pub suite_manifest_path: String,
    pub suite_manifest_sha256: String,
    pub probe_case_catalog_directory: String,
    pub scenario_case_catalog_directory: String,
    pub members: Vec<ConstructedSuitePlanMemberV2>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConstructedSuitePlanMemberV2 {
    pub probe_case_id: String,
    pub probe_draft_path: String,
    pub probe_draft_sha256: String,
    pub case_id: String,
    pub case_draft_path: String,
    pub case_draft_sha256: String,
    pub transition: ScenarioProbeTransition,
    pub launch_profile: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedSuiteCatalogV2 {
    pub schema: &'static str,
    pub catalog_directory: String,
    pub suites: Vec<ConstructedSuiteCatalogEntryV2>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedSuiteCatalogEntryV2 {
    pub suite_id: String,
    pub manifest_file: String,
    pub manifest_sha256: String,
    pub members: usize,
    pub strict_members: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedSuiteReportV2 {
    pub schema: String,
    pub created_at_unix_ms: u128,
    pub passed: bool,
    pub provenance: String,
    pub suite_id: String,
    pub evidence_scope: ConstructedSuiteEvidenceScopeV2,
    pub plan: ComparisonFileIdentityV1,
    pub members: Vec<ConstructedSuiteReportMemberV2>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedSuiteEvidenceScopeV2 {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub all_members_constructed_and_matched: bool,
    pub fresh_runtime_per_member: bool,
    pub shared_runtime_reuse: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedSuiteReportMemberV2 {
    pub index: usize,
    pub probe_case_id: String,
    pub case_id: String,
    pub transition_id: String,
    pub launch_profile: String,
    pub session_id: String,
    pub process_id: u32,
    pub isolation_id: String,
    pub comparison_kind: String,
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub workflow: ComparisonFileIdentityV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteManifestV1 {
    pub schema: String,
    pub provenance: String,
    pub suite_id: String,
    pub launch_profile: String,
    pub members: Vec<SharedRuntimeSuiteMemberSpecV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteMemberSpecV1 {
    pub probe_case_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuitePlanV1 {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub suite_id: String,
    pub launch_profile: String,
    pub suite_manifest_path: String,
    pub suite_manifest_sha256: String,
    pub probe_case_catalog_directory: String,
    pub scenario_case_catalog_directory: String,
    pub members: Vec<ConstructedSuitePlanMemberV2>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteCatalogV1 {
    pub schema: &'static str,
    pub catalog_directory: String,
    pub suites: Vec<SharedRuntimeSuiteCatalogEntryV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteCatalogEntryV1 {
    pub suite_id: String,
    pub manifest_file: String,
    pub manifest_sha256: String,
    pub launch_profile: String,
    pub members: usize,
}

pub fn constructed_suite_catalog(
    suite_catalog_directory: &Path,
) -> Result<ConstructedSuiteCatalogV2, String> {
    let manifests = read_suite_catalog(suite_catalog_directory)?;
    Ok(ConstructedSuiteCatalogV2 {
        schema: "sts_java_parity_constructed_suite_catalog_v2",
        catalog_directory: absolute_string(suite_catalog_directory)?,
        suites: manifests
            .into_iter()
            .map(|(path, bytes, manifest)| ConstructedSuiteCatalogEntryV2 {
                suite_id: manifest.suite_id,
                manifest_file: path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_owned(),
                manifest_sha256: sha256_hex(&bytes),
                members: manifest.members.len(),
                strict_members: manifest
                    .members
                    .iter()
                    .filter(|member| member.launch_profile == "strict")
                    .count(),
            })
            .collect(),
    })
}

pub fn shared_runtime_suite_catalog(
    suite_catalog_directory: &Path,
) -> Result<SharedRuntimeSuiteCatalogV1, String> {
    let manifests = read_shared_runtime_suite_catalog(suite_catalog_directory)?;
    Ok(SharedRuntimeSuiteCatalogV1 {
        schema: "sts_java_parity_shared_runtime_suite_catalog_v1",
        catalog_directory: absolute_string(suite_catalog_directory)?,
        suites: manifests
            .into_iter()
            .map(|(path, bytes, manifest)| SharedRuntimeSuiteCatalogEntryV1 {
                suite_id: manifest.suite_id,
                manifest_file: path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_owned(),
                manifest_sha256: sha256_hex(&bytes),
                launch_profile: manifest.launch_profile,
                members: manifest.members.len(),
            })
            .collect(),
    })
}

pub fn compile_constructed_suite_case(
    suite_catalog_directory: &Path,
    suite_id: &str,
    probe_case_catalog_directory: &Path,
    scenario_case_catalog_directory: &Path,
    output_plan: &Path,
) -> Result<ConstructedSuitePlanV2, String> {
    let mut matches = read_suite_catalog(suite_catalog_directory)?
        .into_iter()
        .filter(|(_, _, manifest)| manifest.suite_id == suite_id);
    let (manifest_path, manifest_bytes, manifest) = matches
        .next()
        .ok_or_else(|| format!("constructed suite catalog has no exact suite ID: {suite_id}"))?;
    if matches.next().is_some() {
        return Err(format!(
            "constructed suite catalog contains duplicate suite ID: {suite_id}"
        ));
    }
    compile_suite_manifest(
        &manifest_path,
        &manifest_bytes,
        manifest,
        probe_case_catalog_directory,
        scenario_case_catalog_directory,
        output_plan,
    )
}

pub fn compile_shared_runtime_suite_case(
    suite_catalog_directory: &Path,
    suite_id: &str,
    probe_case_catalog_directory: &Path,
    scenario_case_catalog_directory: &Path,
    output_plan: &Path,
) -> Result<SharedRuntimeSuitePlanV1, String> {
    let mut matches = read_shared_runtime_suite_catalog(suite_catalog_directory)?
        .into_iter()
        .filter(|(_, _, manifest)| manifest.suite_id == suite_id);
    let (manifest_path, manifest_bytes, manifest) = matches
        .next()
        .ok_or_else(|| format!("shared-runtime suite catalog has no exact suite ID: {suite_id}"))?;
    if matches.next().is_some() {
        return Err(format!(
            "shared-runtime suite catalog contains duplicate suite ID: {suite_id}"
        ));
    }
    compile_shared_runtime_suite_manifest(
        &manifest_path,
        &manifest_bytes,
        manifest,
        probe_case_catalog_directory,
        scenario_case_catalog_directory,
        output_plan,
    )
}

pub fn bundle_constructed_suite(
    plan_path: &Path,
    member_workflow_paths: &[PathBuf],
    output_report: &Path,
) -> Result<ConstructedSuiteReportV2, String> {
    let plan_bytes = fs::read(plan_path).map_err(|error| {
        format!(
            "failed to read constructed suite plan '{}': {error}",
            plan_path.display()
        )
    })?;
    let plan: ConstructedSuitePlanV2 = serde_json::from_slice(&plan_bytes)
        .map_err(|error| format!("failed to decode constructed suite plan: {error}"))?;
    validate_suite_plan(&plan)?;
    if plan.members.len() != member_workflow_paths.len() {
        return Err(format!(
            "constructed suite expected {} member workflows but received {}",
            plan.members.len(),
            member_workflow_paths.len()
        ));
    }

    let mut session_ids = BTreeSet::new();
    let mut process_ids = BTreeSet::new();
    let mut isolation_ids = BTreeSet::new();
    let mut launch_plan_hashes = BTreeSet::new();
    let mut members = Vec::with_capacity(plan.members.len());
    for (index, (expected, workflow_path)) in
        plan.members.iter().zip(member_workflow_paths).enumerate()
    {
        let bytes = fs::read(workflow_path).map_err(|error| {
            format!(
                "failed to read suite member workflow '{}': {error}",
                workflow_path.display()
            )
        })?;
        let workflow: ConstructedComparisonWorkflowV1 = serde_json::from_slice(&bytes)
            .map_err(|error| format!("failed to decode suite member workflow: {error}"))?;
        validate_member_workflow(expected, &workflow)?;
        if !session_ids.insert(workflow.session_id.clone()) {
            return Err("constructed suite members reused a live session identity".to_owned());
        }
        validate_recorded_file_identity(&workflow.probe_report)?;
        validate_recorded_file_identity(&workflow.action_receipt)?;
        validate_recorded_file_identity(&workflow.comparison_report)?;
        let probe_bytes = fs::read(&workflow.probe_report.path).map_err(|error| {
            format!(
                "failed to re-read suite member probe report '{}': {error}",
                workflow.probe_report.path
            )
        })?;
        let probe: ConstructedScenarioProbeReport = serde_json::from_slice(&probe_bytes)
            .map_err(|error| format!("failed to decode suite member probe report: {error}"))?;
        if probe.schema != "sts_java_parity_constructed_probe_report_v2"
            || probe.probe_case_id != expected.probe_case_id
            || probe.transition != expected.transition
            || probe.transition_id != expected.transition.id()
        {
            return Err(
                "constructed suite member probe differs from its exact typed probe case".to_owned(),
            );
        }
        let launch = probe.session.launch.as_ref().ok_or_else(|| {
            "constructed suite member probe omitted typed launch evidence".to_owned()
        })?;
        let isolation_id = launch
            .isolation_id
            .as_ref()
            .ok_or_else(|| "constructed suite member probe omitted its isolation ID".to_owned())?;
        if probe.session.session_id != workflow.session_id
            || launch.profile != workflow.launch_profile
            || launch.storage_class != "isolated_working_directory_v1"
            || !process_ids.insert(probe.session.process_id)
            || !isolation_ids.insert(isolation_id.clone())
            || !launch_plan_hashes.insert(launch.plan_sha256.clone())
        {
            return Err(
                "constructed suite members did not preserve distinct fresh runtime identities"
                    .to_owned(),
            );
        }
        members.push(ConstructedSuiteReportMemberV2 {
            index,
            probe_case_id: expected.probe_case_id.clone(),
            case_id: workflow.case_id,
            transition_id: workflow.transition_id,
            launch_profile: workflow.launch_profile,
            session_id: workflow.session_id,
            process_id: probe.session.process_id,
            isolation_id: isolation_id.clone(),
            comparison_kind: workflow.comparison_kind,
            from_frame_id: workflow.from_frame_id,
            to_frame_id: workflow.to_frame_id,
            workflow: super::file_identity_from_bytes(workflow_path, &bytes)?,
        });
    }

    let report = ConstructedSuiteReportV2 {
        schema: REPORT_SCHEMA.to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        passed: true,
        provenance: "constructed_validated".to_owned(),
        suite_id: plan.suite_id,
        evidence_scope: ConstructedSuiteEvidenceScopeV2 {
            natural_reachability: false,
            strategy_evidence: false,
            training_eligible: false,
            all_members_constructed_and_matched: true,
            fresh_runtime_per_member: true,
            shared_runtime_reuse: false,
        },
        plan: super::file_identity_from_bytes(plan_path, &plan_bytes)?,
        members,
    };
    write_json_report_new(output_report, &report)?;
    Ok(report)
}

fn compile_suite_manifest(
    manifest_path: &Path,
    manifest_bytes: &[u8],
    manifest: ConstructedSuiteManifestV2,
    probe_case_catalog_directory: &Path,
    scenario_case_catalog_directory: &Path,
    output_plan: &Path,
) -> Result<ConstructedSuitePlanV2, String> {
    validate_suite_manifest(&manifest)?;
    let probe_catalog = scenario_probe_case_catalog(probe_case_catalog_directory)?;
    let probe_cases: BTreeMap<_, _> = probe_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.probe_case_id.clone(), entry))
        .collect();
    let case_catalog = scenario_case_catalog(scenario_case_catalog_directory)?;
    let cases: BTreeMap<_, _> = case_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.case_id.clone(), entry))
        .collect();
    let mut members = Vec::with_capacity(manifest.members.len());
    for member in &manifest.members {
        let probe_case = probe_cases.get(&member.probe_case_id).ok_or_else(|| {
            format!(
                "constructed suite member references unknown exact probe case ID: {}",
                member.probe_case_id
            )
        })?;
        let case = cases.get(&probe_case.scenario_case_id).ok_or_else(|| {
            format!(
                "constructed suite member references unknown exact case ID: {}",
                probe_case.scenario_case_id
            )
        })?;
        let probe_draft_path = probe_case_catalog_directory.join(&probe_case.draft_file);
        let draft_path = scenario_case_catalog_directory.join(&case.draft_file);
        members.push(ConstructedSuitePlanMemberV2 {
            probe_case_id: member.probe_case_id.clone(),
            probe_draft_path: absolute_string(&probe_draft_path)?,
            probe_draft_sha256: probe_case.draft_sha256.clone(),
            case_id: probe_case.scenario_case_id.clone(),
            case_draft_path: absolute_string(&draft_path)?,
            case_draft_sha256: case.draft_sha256.clone(),
            transition: probe_case.transition.clone(),
            launch_profile: member.launch_profile.clone(),
        });
    }
    let plan = ConstructedSuitePlanV2 {
        schema: PLAN_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        suite_id: manifest.suite_id,
        suite_manifest_path: absolute_string(manifest_path)?,
        suite_manifest_sha256: sha256_hex(manifest_bytes),
        probe_case_catalog_directory: absolute_string(probe_case_catalog_directory)?,
        scenario_case_catalog_directory: absolute_string(scenario_case_catalog_directory)?,
        members,
    };
    write_json_report_new(output_plan, &plan)?;
    Ok(plan)
}

fn compile_shared_runtime_suite_manifest(
    manifest_path: &Path,
    manifest_bytes: &[u8],
    manifest: SharedRuntimeSuiteManifestV1,
    probe_case_catalog_directory: &Path,
    scenario_case_catalog_directory: &Path,
    output_plan: &Path,
) -> Result<SharedRuntimeSuitePlanV1, String> {
    validate_shared_runtime_suite_manifest(&manifest)?;
    let probe_catalog = scenario_probe_case_catalog(probe_case_catalog_directory)?;
    let probe_cases: BTreeMap<_, _> = probe_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.probe_case_id.clone(), entry))
        .collect();
    let case_catalog = scenario_case_catalog(scenario_case_catalog_directory)?;
    let cases: BTreeMap<_, _> = case_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.case_id.clone(), entry))
        .collect();
    let mut members = Vec::with_capacity(manifest.members.len());
    for member in &manifest.members {
        let probe_case = probe_cases.get(&member.probe_case_id).ok_or_else(|| {
            format!(
                "shared-runtime suite member references unknown exact probe case ID: {}",
                member.probe_case_id
            )
        })?;
        let case = cases.get(&probe_case.scenario_case_id).ok_or_else(|| {
            format!(
                "shared-runtime suite member references unknown exact case ID: {}",
                probe_case.scenario_case_id
            )
        })?;
        let probe_draft_path = probe_case_catalog_directory.join(&probe_case.draft_file);
        let case_draft_path = scenario_case_catalog_directory.join(&case.draft_file);
        members.push(ConstructedSuitePlanMemberV2 {
            probe_case_id: member.probe_case_id.clone(),
            probe_draft_path: absolute_string(&probe_draft_path)?,
            probe_draft_sha256: probe_case.draft_sha256.clone(),
            case_id: probe_case.scenario_case_id.clone(),
            case_draft_path: absolute_string(&case_draft_path)?,
            case_draft_sha256: case.draft_sha256.clone(),
            transition: probe_case.transition.clone(),
            launch_profile: manifest.launch_profile.clone(),
        });
    }
    let plan = SharedRuntimeSuitePlanV1 {
        schema: SHARED_PLAN_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        suite_id: manifest.suite_id,
        launch_profile: manifest.launch_profile,
        suite_manifest_path: absolute_string(manifest_path)?,
        suite_manifest_sha256: sha256_hex(manifest_bytes),
        probe_case_catalog_directory: absolute_string(probe_case_catalog_directory)?,
        scenario_case_catalog_directory: absolute_string(scenario_case_catalog_directory)?,
        members,
    };
    validate_shared_runtime_suite_plan(&plan)?;
    write_json_report_new(output_plan, &plan)?;
    Ok(plan)
}

fn read_shared_runtime_suite_catalog(
    directory: &Path,
) -> Result<Vec<(PathBuf, Vec<u8>, SharedRuntimeSuiteManifestV1)>, String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "failed to read shared-runtime suite catalog '{}': {error}",
            directory.display()
        )
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| {
                format!(
                    "failed to enumerate shared-runtime suite catalog '{}': {error}",
                    directory.display()
                )
            })?
            .path();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err("shared-runtime suite catalog contains no JSON manifests".to_owned());
    }
    let mut suite_ids = BTreeSet::new();
    let mut manifests = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path).map_err(|error| {
            format!(
                "failed to read shared-runtime suite manifest '{}': {error}",
                path.display()
            )
        })?;
        let manifest: SharedRuntimeSuiteManifestV1 =
            serde_json::from_slice(&bytes).map_err(|error| {
                format!(
                    "failed to decode shared-runtime suite manifest '{}': {error}",
                    path.display()
                )
            })?;
        validate_shared_runtime_suite_manifest(&manifest)?;
        let expected_file = format!("{}.json", manifest.suite_id);
        if path.file_name().and_then(|value| value.to_str()) != Some(expected_file.as_str()) {
            return Err(format!(
                "shared-runtime suite filename must equal its exact suite ID: expected {expected_file}"
            ));
        }
        if !suite_ids.insert(manifest.suite_id.clone()) {
            return Err(format!(
                "shared-runtime suite catalog contains duplicate suite ID: {}",
                manifest.suite_id
            ));
        }
        manifests.push((path, bytes, manifest));
    }
    Ok(manifests)
}

fn validate_shared_runtime_suite_manifest(
    manifest: &SharedRuntimeSuiteManifestV1,
) -> Result<(), String> {
    if manifest.schema != SHARED_MANIFEST_SCHEMA
        || manifest.provenance != "constructed_validated"
        || !valid_exact_id(&manifest.suite_id)
        || !matches!(manifest.launch_profile.as_str(), "strict" | "accelerated")
        || !(2..=32).contains(&manifest.members.len())
    {
        return Err("shared-runtime suite manifest identity or member count is invalid".to_owned());
    }
    let mut probe_case_ids = BTreeSet::new();
    if manifest.members.iter().any(|member| {
        !valid_exact_id(&member.probe_case_id)
            || !probe_case_ids.insert(member.probe_case_id.as_str())
    }) {
        return Err("shared-runtime suite contains an invalid or duplicate member".to_owned());
    }
    Ok(())
}

fn validate_shared_runtime_suite_plan(plan: &SharedRuntimeSuitePlanV1) -> Result<(), String> {
    if plan.schema != SHARED_PLAN_SCHEMA
        || plan.provenance != "constructed_validated"
        || plan.created_at_unix_ms == 0
        || !valid_exact_id(&plan.suite_id)
        || !matches!(plan.launch_profile.as_str(), "strict" | "accelerated")
        || !(2..=32).contains(&plan.members.len())
    {
        return Err("shared-runtime suite plan identity or member count is invalid".to_owned());
    }
    let manifest_bytes = fs::read(&plan.suite_manifest_path)
        .map_err(|error| format!("failed to re-read shared-runtime suite manifest: {error}"))?;
    if sha256_hex(&manifest_bytes) != plan.suite_manifest_sha256 {
        return Err("shared-runtime suite manifest changed after plan compilation".to_owned());
    }
    let manifest: SharedRuntimeSuiteManifestV1 = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("failed to re-decode shared-runtime suite manifest: {error}"))?;
    validate_shared_runtime_suite_manifest(&manifest)?;
    if manifest.suite_id != plan.suite_id
        || manifest.launch_profile != plan.launch_profile
        || manifest.members.len() != plan.members.len()
    {
        return Err("shared-runtime suite plan no longer matches its bound manifest".to_owned());
    }
    let probe_catalog_directory = Path::new(&plan.probe_case_catalog_directory);
    let probe_catalog = scenario_probe_case_catalog(probe_catalog_directory)?;
    let probe_cases: BTreeMap<_, _> = probe_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.probe_case_id.clone(), entry))
        .collect();
    let case_catalog_directory = Path::new(&plan.scenario_case_catalog_directory);
    let case_catalog = scenario_case_catalog(case_catalog_directory)?;
    let cases: BTreeMap<_, _> = case_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.case_id.clone(), entry))
        .collect();
    let mut probe_case_ids = BTreeSet::new();
    for (manifest_member, member) in manifest.members.iter().zip(&plan.members) {
        let probe_case = probe_cases.get(&member.probe_case_id).ok_or_else(|| {
            format!(
                "shared-runtime suite plan references missing probe case: {}",
                member.probe_case_id
            )
        })?;
        let case = cases.get(&member.case_id).ok_or_else(|| {
            format!(
                "shared-runtime suite plan references missing scenario case: {}",
                member.case_id
            )
        })?;
        let expected_probe_path =
            absolute_string(&probe_catalog_directory.join(&probe_case.draft_file))?;
        let expected_case_path = absolute_string(&case_catalog_directory.join(&case.draft_file))?;
        if manifest_member.probe_case_id != member.probe_case_id
            || member.launch_profile != plan.launch_profile
            || probe_case.scenario_case_id != member.case_id
            || probe_case.transition != member.transition
            || !probe_case_ids.insert(member.probe_case_id.as_str())
            || member.probe_draft_path != expected_probe_path
            || member.probe_draft_sha256 != probe_case.draft_sha256
            || sha256_file(Path::new(&member.probe_draft_path))? != member.probe_draft_sha256
            || member.case_draft_path != expected_case_path
            || member.case_draft_sha256 != case.draft_sha256
            || sha256_file(Path::new(&member.case_draft_path))? != member.case_draft_sha256
        {
            return Err(
                "shared-runtime suite plan member changed or differs from its bound catalogs"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn read_suite_catalog(
    directory: &Path,
) -> Result<Vec<(PathBuf, Vec<u8>, ConstructedSuiteManifestV2)>, String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "failed to read constructed suite catalog '{}': {error}",
            directory.display()
        )
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| {
                format!(
                    "failed to enumerate constructed suite catalog '{}': {error}",
                    directory.display()
                )
            })?
            .path();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err("constructed suite catalog contains no JSON manifests".to_owned());
    }
    let mut suite_ids = BTreeSet::new();
    let mut manifests = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path).map_err(|error| {
            format!(
                "failed to read constructed suite manifest '{}': {error}",
                path.display()
            )
        })?;
        let manifest: ConstructedSuiteManifestV2 =
            serde_json::from_slice(&bytes).map_err(|error| {
                format!(
                    "failed to decode constructed suite manifest '{}': {error}",
                    path.display()
                )
            })?;
        validate_suite_manifest(&manifest)?;
        let expected_file = format!("{}.json", manifest.suite_id);
        if path.file_name().and_then(|value| value.to_str()) != Some(expected_file.as_str()) {
            return Err(format!(
                "constructed suite filename must equal its exact suite ID: expected {expected_file}"
            ));
        }
        if !suite_ids.insert(manifest.suite_id.clone()) {
            return Err(format!(
                "constructed suite catalog contains duplicate suite ID: {}",
                manifest.suite_id
            ));
        }
        manifests.push((path, bytes, manifest));
    }
    Ok(manifests)
}

fn validate_suite_manifest(manifest: &ConstructedSuiteManifestV2) -> Result<(), String> {
    if manifest.schema != MANIFEST_SCHEMA
        || manifest.provenance != "constructed_validated"
        || !valid_exact_id(&manifest.suite_id)
        || manifest.members.is_empty()
        || manifest.members.len() > 32
    {
        return Err("constructed suite manifest identity or member count is invalid".to_owned());
    }
    let mut identities = BTreeSet::new();
    for member in &manifest.members {
        if !valid_exact_id(&member.probe_case_id)
            || !matches!(member.launch_profile.as_str(), "strict" | "accelerated")
            || !identities.insert((
                member.probe_case_id.as_str(),
                member.launch_profile.as_str(),
            ))
        {
            return Err("constructed suite contains an invalid or duplicate member".to_owned());
        }
    }
    Ok(())
}

fn validate_suite_plan(plan: &ConstructedSuitePlanV2) -> Result<(), String> {
    if plan.schema != PLAN_SCHEMA
        || plan.provenance != "constructed_validated"
        || plan.created_at_unix_ms == 0
        || !valid_exact_id(&plan.suite_id)
        || plan.members.is_empty()
        || plan.members.len() > 32
    {
        return Err("constructed suite plan identity or member count is invalid".to_owned());
    }
    let manifest_bytes = fs::read(&plan.suite_manifest_path)
        .map_err(|error| format!("failed to re-read suite manifest: {error}"))?;
    if sha256_hex(&manifest_bytes) != plan.suite_manifest_sha256 {
        return Err("constructed suite manifest changed after plan compilation".to_owned());
    }
    let manifest: ConstructedSuiteManifestV2 = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("failed to re-decode constructed suite manifest: {error}"))?;
    validate_suite_manifest(&manifest)?;
    if manifest.suite_id != plan.suite_id || manifest.members.len() != plan.members.len() {
        return Err("constructed suite plan no longer matches its bound manifest".to_owned());
    }
    let probe_case_catalog_directory = Path::new(&plan.probe_case_catalog_directory);
    let probe_catalog = scenario_probe_case_catalog(probe_case_catalog_directory)?;
    let probe_cases: BTreeMap<_, _> = probe_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.probe_case_id.clone(), entry))
        .collect();
    let scenario_case_catalog_directory = Path::new(&plan.scenario_case_catalog_directory);
    let case_catalog = scenario_case_catalog(scenario_case_catalog_directory)?;
    let cases: BTreeMap<_, _> = case_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.case_id.clone(), entry))
        .collect();
    let mut identities = BTreeSet::new();
    for (manifest_member, member) in manifest.members.iter().zip(&plan.members) {
        let probe_case = probe_cases.get(&member.probe_case_id).ok_or_else(|| {
            format!(
                "constructed suite plan references missing exact probe case ID: {}",
                member.probe_case_id
            )
        })?;
        let case = cases.get(&member.case_id).ok_or_else(|| {
            format!(
                "constructed suite plan references missing exact case ID: {}",
                member.case_id
            )
        })?;
        let expected_probe_draft_path =
            absolute_string(&probe_case_catalog_directory.join(&probe_case.draft_file))?;
        let expected_draft_path =
            absolute_string(&scenario_case_catalog_directory.join(&case.draft_file))?;
        if !valid_exact_id(&member.probe_case_id)
            || !valid_exact_id(&member.case_id)
            || manifest_member.probe_case_id != member.probe_case_id
            || manifest_member.launch_profile != member.launch_profile
            || probe_case.scenario_case_id != member.case_id
            || probe_case.transition != member.transition
            || !matches!(member.launch_profile.as_str(), "strict" | "accelerated")
            || !identities.insert((
                member.probe_case_id.as_str(),
                member.launch_profile.as_str(),
            ))
            || member.probe_draft_path != expected_probe_draft_path
            || member.probe_draft_sha256 != probe_case.draft_sha256
            || sha256_file(Path::new(&member.probe_draft_path))? != member.probe_draft_sha256
            || member.case_draft_path != expected_draft_path
            || member.case_draft_sha256 != case.draft_sha256
            || sha256_file(Path::new(&member.case_draft_path))? != member.case_draft_sha256
        {
            return Err(
                "constructed suite plan member is invalid, changed, or differs from its bound catalogs"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn validate_member_workflow(
    expected: &ConstructedSuitePlanMemberV2,
    workflow: &ConstructedComparisonWorkflowV1,
) -> Result<(), String> {
    let evidence = &workflow.evidence_scope;
    if workflow.schema != "sts_java_parity_constructed_comparison_workflow_v1"
        || !workflow.passed
        || workflow.provenance != "constructed_validated"
        || workflow.case_id != expected.case_id
        || workflow.transition_id != expected.transition.id()
        || workflow.launch_profile != expected.launch_profile
        || evidence.natural_reachability
        || evidence.strategy_evidence
        || evidence.training_eligible
        || !evidence.constructed_root_verified
        || !evidence.original_game_transition_captured
        || !evidence.simulator_comparison
        || !evidence.narrow_mechanics_parity
        || evidence.strict_runtime_profile != (expected.launch_profile == "strict")
    {
        return Err("constructed suite member workflow is inconsistent with its plan".to_owned());
    }
    Ok(())
}

fn validate_recorded_file_identity(identity: &ComparisonFileIdentityV1) -> Result<(), String> {
    let actual = file_identity(Path::new(&identity.path))?;
    if actual.path != identity.path
        || actual.bytes != identity.bytes
        || actual.sha256 != identity.sha256
    {
        return Err(format!(
            "constructed workflow input changed after it was recorded: {}",
            identity.path
        ));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| format!("failed to hash '{}': {error}", path.display()))
}

fn absolute_string(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve current directory: {error}"))?
            .join(path)
    };
    Ok(absolute.to_string_lossy().into_owned())
}

fn valid_exact_id(value: &str) -> bool {
    (1..=96).contains(&value.len())
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && !value.starts_with('-')
        && !value.ends_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn tracked_suite_compiles_exact_members_and_profiles() {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let suite_catalog = repository.join("fixtures/java_parity_suites");
        let probe_catalog = repository.join("fixtures/java_parity_probe_cases");
        let case_catalog = repository.join("fixtures/java_parity_cases");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output = std::env::temp_dir().join(format!(
            "sts-java-parity-suite-plan-{}-{nonce}.json",
            std::process::id()
        ));

        let catalog = constructed_suite_catalog(&suite_catalog).unwrap();
        let plan = compile_constructed_suite_case(
            &suite_catalog,
            "cultist-end-turn-rng-boundaries",
            &probe_catalog,
            &case_catalog,
            &output,
        )
        .unwrap();

        assert_eq!(catalog.suites.len(), 1);
        assert_eq!(catalog.suites[0].members, 3);
        assert_eq!(catalog.suites[0].strict_members, 1);
        assert_eq!(plan.members.len(), 3);
        assert_eq!(plan.members[0].launch_profile, "accelerated");
        assert_eq!(plan.members[0].probe_case_id, "cultist-base-end-turn");
        assert_eq!(
            plan.members[1].case_id,
            "cultist-five-card-remainder-no-shuffle"
        );
        assert_eq!(plan.members[2].launch_profile, "strict");
        assert!(plan
            .members
            .iter()
            .all(|member| member.case_draft_sha256.len() == 64
                && member.probe_draft_sha256.len() == 64));

        let mut rewritten_plan = plan.clone();
        rewritten_plan.suite_id = "rewritten-suite-id".to_owned();
        assert_eq!(
            validate_suite_plan(&rewritten_plan).unwrap_err(),
            "constructed suite plan no longer matches its bound manifest"
        );
        fs::remove_file(output).unwrap();
    }

    #[test]
    fn shared_runtime_suite_has_one_profile_and_distinct_exact_members() {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let suite_catalog = repository.join("fixtures/java_parity_shared_runtime_suites");
        let probe_catalog = repository.join("fixtures/java_parity_probe_cases");
        let case_catalog = repository.join("fixtures/java_parity_cases");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output = std::env::temp_dir().join(format!(
            "sts-java-parity-shared-suite-plan-{}-{nonce}.json",
            std::process::id()
        ));
        let execution_output = std::env::temp_dir().join(format!(
            "sts-java-parity-shared-execution-plan-{}-{nonce}.json",
            std::process::id()
        ));
        let member_plan_directory = std::env::temp_dir().join(format!(
            "sts-java-parity-shared-member-plans-{}-{nonce}",
            std::process::id()
        ));

        let catalog = shared_runtime_suite_catalog(&suite_catalog).unwrap();
        let plan = compile_shared_runtime_suite_case(
            &suite_catalog,
            "cultist-shared-runtime-smoke",
            &probe_catalog,
            &case_catalog,
            &output,
        )
        .unwrap();

        assert_eq!(catalog.suites.len(), 1);
        assert_eq!(catalog.suites[0].launch_profile, "accelerated");
        assert_eq!(catalog.suites[0].members, 5);
        assert_eq!(plan.members.len(), 5);
        assert!(plan
            .members
            .iter()
            .all(|member| member.launch_profile == plan.launch_profile));
        assert_eq!(plan.members[0].probe_case_id, "cultist-base-end-turn");
        assert_eq!(plan.members[1].probe_case_id, "cultist-second-strike");
        assert_eq!(plan.members[2].probe_case_id, "cultist-block-potion-use");
        assert_eq!(plan.members[3].probe_case_id, "cultist-fire-potion-use");
        assert_eq!(
            plan.members[4].probe_case_id,
            "three-sentries-second-fire-potion-use"
        );

        let mut changed = plan.clone();
        changed.members[1].launch_profile = "strict".to_owned();
        assert!(validate_shared_runtime_suite_plan(&changed).is_err());

        let execution = prepare_shared_runtime_suite_execution(
            &output,
            &member_plan_directory,
            &execution_output,
        )
        .unwrap();
        assert_eq!(execution.members.len(), 5);
        assert_eq!(
            execution
                .members
                .iter()
                .map(|member| member.scenario_id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            5
        );
        assert!(execution.members.iter().all(|member| {
            Path::new(&member.scenario_plan.path).is_file()
                && Path::new(&member.probe_plan.path).is_file()
        }));
        let mut changed_execution = execution.clone();
        changed_execution.members[1].scenario_id = changed_execution.members[0].scenario_id.clone();
        assert!(
            shared_runtime::validate_shared_runtime_suite_execution_plan(&changed_execution)
                .is_err()
        );

        fs::remove_dir_all(member_plan_directory).unwrap();
        fs::remove_file(execution_output).unwrap();
        fs::remove_file(output).unwrap();
    }

    #[test]
    fn failed_shared_runtime_capture_is_published_without_a_passing_scope() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output = std::env::temp_dir().join(format!(
            "sts-java-parity-failed-shared-capture-{}-{nonce}.json",
            std::process::id()
        ));
        let report = SharedRuntimeSuiteCaptureReportV1 {
            schema: shared_runtime::SHARED_CAPTURE_SCHEMA.to_owned(),
            provenance: "constructed_validated".to_owned(),
            created_at_unix_ms: 1,
            completed_at_unix_ms: 1,
            passed: false,
            failure: None,
            evidence_scope: SharedRuntimeSuiteCaptureScopeV1 {
                natural_reachability: false,
                strategy_evidence: false,
                training_eligible: false,
                simulator_comparison: false,
                fresh_runtime_per_member: false,
                shared_runtime_reuse: false,
                expected_members: 3,
                captured_members: 1,
                expected_resets: 2,
                captured_resets: 1,
            },
            suite_id: "capture-failure-smoke".to_owned(),
            launch_profile: "accelerated".to_owned(),
            execution_plan: ComparisonFileIdentityV1 {
                path: "execution.json".to_owned(),
                bytes: 1,
                sha256: "0".repeat(64),
            },
            session_id: Some("session".to_owned()),
            connection_id: Some("connection".to_owned()),
            process_id: Some(1),
            isolation_id: Some("isolation".to_owned()),
            launch_plan_sha256: Some("1".repeat(64)),
            members: Vec::new(),
            resets: Vec::new(),
            total_after_session_ready_ms: 0,
        };

        let result = shared_runtime::publish_shared_runtime_capture_result(
            report,
            &output,
            Err("second member failed".to_owned()),
            42,
        );
        assert!(result.is_err());
        let retained: SharedRuntimeSuiteCaptureReportV1 =
            shared_runtime::read_json_file(&output).unwrap();
        assert!(!retained.passed);
        assert_eq!(retained.failure.as_deref(), Some("second member failed"));
        assert!(!retained.evidence_scope.shared_runtime_reuse);
        assert_eq!(retained.total_after_session_ready_ms, 42);
        fs::remove_file(output).unwrap();
    }
}
