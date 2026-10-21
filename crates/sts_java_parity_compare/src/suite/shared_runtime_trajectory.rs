use super::{absolute_string, valid_exact_id, validate_recorded_file_identity};
use crate::lifecycle::{verify_isolated_lifecycle, IsolatedLifecycleExpectation};
use crate::{
    current_unix_ms, file_identity, file_identity_from_bytes, recompare_constructed_trajectory_new,
    write_json_report_new, ComparisonFileIdentityV1, ConstructedTrajectoryComparisonAttemptV3,
    ConstructedTrajectoryComparisonOutcomeV3, ConstructedTrajectoryComparisonSelectionV1,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use sts_java_parity_live::{
    compile_scenario_trajectory_case, reset_scenario_to_main_menu,
    run_constructed_combat_trajectory, scenario_case_catalog, scenario_trajectory_case_catalog,
    ConstructedScenarioReport, ConstructedScenarioTrajectoryReport, ScenarioPlan,
    ScenarioProbeTransition, ScenarioResetReport, ScenarioTrajectoryPlan, SessionManifest,
};

const MANIFEST_SCHEMA: &str = "sts_java_parity_shared_runtime_trajectory_suite_manifest_v1";
const PLAN_SCHEMA: &str = "sts_java_parity_shared_runtime_trajectory_suite_plan_v1";
const EXECUTION_SCHEMA: &str = "sts_java_parity_shared_runtime_trajectory_suite_execution_plan_v1";
const CAPTURE_SCHEMA: &str = "sts_java_parity_shared_runtime_trajectory_suite_capture_v1";
const REPORT_SCHEMA: &str = "sts_java_parity_shared_runtime_trajectory_suite_report_v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteManifestV1 {
    pub schema: String,
    pub provenance: String,
    pub suite_id: String,
    pub launch_profile: String,
    pub members: Vec<SharedRuntimeTrajectorySuiteMemberSpecV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteMemberSpecV1 {
    pub trajectory_case_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuitePlanV1 {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub suite_id: String,
    pub launch_profile: String,
    pub manifest: ComparisonFileIdentityV1,
    pub trajectory_case_catalog_directory: String,
    pub scenario_case_catalog_directory: String,
    pub members: Vec<SharedRuntimeTrajectorySuitePlanMemberV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuitePlanMemberV1 {
    pub trajectory_case_id: String,
    pub trajectory_draft: ComparisonFileIdentityV1,
    pub scenario_case_id: String,
    pub scenario_draft: ComparisonFileIdentityV1,
    pub steps: Vec<ScenarioProbeTransition>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteCatalogV1 {
    pub schema: &'static str,
    pub catalog_directory: String,
    pub suites: Vec<SharedRuntimeTrajectorySuiteCatalogEntryV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteCatalogEntryV1 {
    pub suite_id: String,
    pub manifest_file: String,
    pub manifest_sha256: String,
    pub launch_profile: String,
    pub members: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteExecutionPlanV1 {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub suite_id: String,
    pub launch_profile: String,
    pub suite_plan: ComparisonFileIdentityV1,
    pub members: Vec<SharedRuntimeTrajectorySuiteExecutionMemberV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteExecutionMemberV1 {
    pub index: usize,
    pub trajectory_case_id: String,
    pub scenario_case_id: String,
    pub scenario_id: String,
    pub steps: Vec<ScenarioProbeTransition>,
    pub scenario_plan: ComparisonFileIdentityV1,
    pub trajectory_plan: ComparisonFileIdentityV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteCaptureScopeV1 {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub simulator_comparison: bool,
    pub fresh_runtime_per_member: bool,
    pub shared_runtime_reuse: bool,
    pub expected_members: usize,
    pub captured_members: usize,
    pub expected_resets: usize,
    pub captured_resets: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteCaptureReportV1 {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub failure: Option<String>,
    pub evidence_scope: SharedRuntimeTrajectorySuiteCaptureScopeV1,
    pub suite_id: String,
    pub launch_profile: String,
    pub execution_plan: ComparisonFileIdentityV1,
    pub session_id: Option<String>,
    pub connection_id: Option<String>,
    pub process_id: Option<u32>,
    pub isolation_id: Option<String>,
    pub launch_plan_sha256: Option<String>,
    pub members: Vec<SharedRuntimeTrajectorySuiteCaptureMemberV1>,
    pub resets: Vec<SharedRuntimeTrajectorySuiteCaptureResetV1>,
    pub total_after_session_ready_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteCaptureMemberV1 {
    pub index: usize,
    pub trajectory_case_id: String,
    pub scenario_case_id: String,
    pub scenario_id: String,
    pub root_frame_id: u64,
    pub final_frame_id: u64,
    pub completed_steps: usize,
    pub scenario_plan: ComparisonFileIdentityV1,
    pub trajectory_plan: ComparisonFileIdentityV1,
    pub scenario_report: ComparisonFileIdentityV1,
    pub trajectory_report: ComparisonFileIdentityV1,
    pub elapsed_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteCaptureResetV1 {
    pub after_member_index: usize,
    pub from_frame_id: u64,
    pub successor_frame_id: u64,
    pub report: ComparisonFileIdentityV1,
    pub elapsed_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteVerifiedScopeV1 {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub simulator_comparison: bool,
    pub all_members_constructed_and_matched: bool,
    pub all_inter_member_resets_verified: bool,
    pub managed_lifecycle_verified: bool,
    pub strict_runtime_profile: bool,
    pub fresh_runtime_per_member: bool,
    pub shared_runtime_reuse: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteVerifiedReportV1 {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub passed: bool,
    pub suite_id: String,
    pub launch_profile: String,
    pub evidence_scope: SharedRuntimeTrajectorySuiteVerifiedScopeV1,
    pub execution_plan: ComparisonFileIdentityV1,
    pub capture_report: ComparisonFileIdentityV1,
    pub lifecycle_receipt: ComparisonFileIdentityV1,
    pub session_id: String,
    pub process_id: u32,
    pub isolation_id: String,
    pub members: Vec<SharedRuntimeTrajectorySuiteVerifiedMemberV1>,
    pub reset_reports: Vec<ComparisonFileIdentityV1>,
    pub capture_after_session_ready_ms: u128,
    pub lifecycle_total_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeTrajectorySuiteVerifiedMemberV1 {
    pub index: usize,
    pub trajectory_case_id: String,
    pub scenario_case_id: String,
    pub scenario_id: String,
    pub root_frame_id: u64,
    pub final_frame_id: u64,
    pub completed_steps: usize,
    pub comparison: ComparisonFileIdentityV1,
}

pub fn shared_runtime_trajectory_suite_catalog(
    suite_catalog_directory: &Path,
) -> Result<SharedRuntimeTrajectorySuiteCatalogV1, String> {
    let manifests = read_manifest_catalog(suite_catalog_directory)?;
    Ok(SharedRuntimeTrajectorySuiteCatalogV1 {
        schema: "sts_java_parity_shared_runtime_trajectory_suite_catalog_v1",
        catalog_directory: absolute_string(suite_catalog_directory)?,
        suites: manifests
            .into_iter()
            .map(
                |(path, bytes, manifest)| SharedRuntimeTrajectorySuiteCatalogEntryV1 {
                    suite_id: manifest.suite_id,
                    manifest_file: path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_owned(),
                    manifest_sha256: crate::sha256_hex(&bytes),
                    launch_profile: manifest.launch_profile,
                    members: manifest.members.len(),
                },
            )
            .collect(),
    })
}

pub fn compile_shared_runtime_trajectory_suite_case(
    suite_catalog_directory: &Path,
    suite_id: &str,
    trajectory_case_catalog_directory: &Path,
    scenario_case_catalog_directory: &Path,
    output_plan: &Path,
) -> Result<SharedRuntimeTrajectorySuitePlanV1, String> {
    let mut matches = read_manifest_catalog(suite_catalog_directory)?
        .into_iter()
        .filter(|(_, _, manifest)| manifest.suite_id == suite_id);
    let (manifest_path, manifest_bytes, manifest) = matches.next().ok_or_else(|| {
        format!("shared-runtime trajectory suite has no exact suite ID: {suite_id}")
    })?;
    if matches.next().is_some() {
        return Err(format!(
            "shared-runtime trajectory suite contains duplicate suite ID: {suite_id}"
        ));
    }

    let trajectory_catalog = scenario_trajectory_case_catalog(trajectory_case_catalog_directory)?;
    let trajectories: BTreeMap<_, _> = trajectory_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.trajectory_case_id.clone(), entry))
        .collect();
    let scenario_catalog = scenario_case_catalog(scenario_case_catalog_directory)?;
    let scenarios: BTreeMap<_, _> = scenario_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.case_id.clone(), entry))
        .collect();
    let mut members = Vec::with_capacity(manifest.members.len());
    for member in &manifest.members {
        let trajectory = trajectories
            .get(&member.trajectory_case_id)
            .ok_or_else(|| {
                format!(
                    "shared-runtime trajectory suite references unknown exact trajectory case: {}",
                    member.trajectory_case_id
                )
            })?;
        let scenario = scenarios.get(&trajectory.scenario_case_id).ok_or_else(|| {
            format!(
                "shared-runtime trajectory suite references missing scenario case: {}",
                trajectory.scenario_case_id
            )
        })?;
        members.push(SharedRuntimeTrajectorySuitePlanMemberV1 {
            trajectory_case_id: member.trajectory_case_id.clone(),
            trajectory_draft: file_identity(
                &trajectory_case_catalog_directory.join(&trajectory.draft_file),
            )?,
            scenario_case_id: trajectory.scenario_case_id.clone(),
            scenario_draft: file_identity(
                &scenario_case_catalog_directory.join(&scenario.draft_file),
            )?,
            steps: trajectory.steps.clone(),
        });
    }
    let plan = SharedRuntimeTrajectorySuitePlanV1 {
        schema: PLAN_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        suite_id: manifest.suite_id,
        launch_profile: manifest.launch_profile,
        manifest: file_identity_from_bytes(&manifest_path, &manifest_bytes)?,
        trajectory_case_catalog_directory: absolute_string(trajectory_case_catalog_directory)?,
        scenario_case_catalog_directory: absolute_string(scenario_case_catalog_directory)?,
        members,
    };
    validate_plan(&plan)?;
    write_json_report_new(output_plan, &plan)?;
    Ok(plan)
}

pub fn prepare_shared_runtime_trajectory_suite_execution(
    suite_plan_path: &Path,
    member_plan_directory: &Path,
    output_plan: &Path,
) -> Result<SharedRuntimeTrajectorySuiteExecutionPlanV1, String> {
    let suite_plan_bytes = fs::read(suite_plan_path).map_err(|error| {
        format!(
            "failed to read shared-runtime trajectory suite plan '{}': {error}",
            suite_plan_path.display()
        )
    })?;
    let suite_plan: SharedRuntimeTrajectorySuitePlanV1 = serde_json::from_slice(&suite_plan_bytes)
        .map_err(|error| {
            format!("failed to decode shared-runtime trajectory suite plan: {error}")
        })?;
    validate_plan(&suite_plan)?;
    if member_plan_directory.exists() || output_plan.exists() {
        return Err(
            "shared-runtime trajectory member directory and execution plan must be create-new"
                .to_owned(),
        );
    }
    fs::create_dir(member_plan_directory).map_err(|error| {
        format!(
            "failed to create trajectory member plan directory '{}': {error}",
            member_plan_directory.display()
        )
    })?;

    let mut scenario_ids = BTreeSet::new();
    let mut members = Vec::with_capacity(suite_plan.members.len());
    for (index, expected) in suite_plan.members.iter().enumerate() {
        let directory = member_plan_directory.join(format!("member-{index:02}"));
        fs::create_dir(&directory).map_err(|error| {
            format!(
                "failed to create trajectory member directory '{}': {error}",
                directory.display()
            )
        })?;
        let scenario_plan_path = directory.join("scenario-plan.json");
        let trajectory_plan_path = directory.join("trajectory-plan.json");
        let trajectory_plan = compile_scenario_trajectory_case(
            Path::new(&suite_plan.trajectory_case_catalog_directory),
            &expected.trajectory_case_id,
            Path::new(&suite_plan.scenario_case_catalog_directory),
            &scenario_plan_path,
            &trajectory_plan_path,
        )?;
        let scenario_plan: ScenarioPlan = read_json(&scenario_plan_path)?;
        if trajectory_plan.trajectory_case_id != expected.trajectory_case_id
            || trajectory_plan.scenario_case_id != expected.scenario_case_id
            || trajectory_plan.steps != expected.steps
            || scenario_plan.recipe.case_id != expected.scenario_case_id
            || !scenario_ids.insert(scenario_plan.recipe.scenario_id.clone())
        {
            return Err(
                "compiled trajectory member differs from its immutable suite plan".to_owned(),
            );
        }
        members.push(SharedRuntimeTrajectorySuiteExecutionMemberV1 {
            index,
            trajectory_case_id: expected.trajectory_case_id.clone(),
            scenario_case_id: expected.scenario_case_id.clone(),
            scenario_id: scenario_plan.recipe.scenario_id,
            steps: expected.steps.clone(),
            scenario_plan: file_identity(&scenario_plan_path)?,
            trajectory_plan: file_identity(&trajectory_plan_path)?,
        });
    }
    let execution = SharedRuntimeTrajectorySuiteExecutionPlanV1 {
        schema: EXECUTION_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        suite_id: suite_plan.suite_id,
        launch_profile: suite_plan.launch_profile,
        suite_plan: file_identity_from_bytes(suite_plan_path, &suite_plan_bytes)?,
        members,
    };
    validate_execution(&execution)?;
    write_json_report_new(output_plan, &execution)?;
    Ok(execution)
}

pub fn capture_shared_runtime_trajectory_suite(
    execution_plan_path: &Path,
    capture_directory: &Path,
    live_output_directory: &Path,
    expected_launch_plan: &Path,
    output_report: &Path,
    timeout: Duration,
) -> Result<SharedRuntimeTrajectorySuiteCaptureReportV1, String> {
    let execution_bytes = fs::read(execution_plan_path).map_err(|error| {
        format!(
            "failed to read shared-runtime trajectory execution plan '{}': {error}",
            execution_plan_path.display()
        )
    })?;
    let execution: SharedRuntimeTrajectorySuiteExecutionPlanV1 =
        serde_json::from_slice(&execution_bytes).map_err(|error| {
            format!("failed to decode shared-runtime trajectory execution plan: {error}")
        })?;
    validate_execution(&execution)?;
    if capture_directory.exists() || output_report.exists() {
        return Err(
            "shared-runtime trajectory capture directory and report must be create-new".to_owned(),
        );
    }
    fs::create_dir_all(
        output_report
            .parent()
            .ok_or_else(|| "trajectory capture report has no parent".to_owned())?,
    )
    .map_err(|error| format!("failed to prepare trajectory capture parent: {error}"))?;
    fs::create_dir(capture_directory).map_err(|error| {
        format!(
            "failed to create trajectory capture directory '{}': {error}",
            capture_directory.display()
        )
    })?;

    let created_at_unix_ms = current_unix_ms()?;
    let started = Instant::now();
    let expected_members = execution.members.len();
    let expected_resets = expected_members - 1;
    let mut report = SharedRuntimeTrajectorySuiteCaptureReportV1 {
        schema: CAPTURE_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms,
        completed_at_unix_ms: created_at_unix_ms,
        passed: false,
        failure: None,
        evidence_scope: SharedRuntimeTrajectorySuiteCaptureScopeV1 {
            natural_reachability: false,
            strategy_evidence: false,
            training_eligible: false,
            simulator_comparison: false,
            fresh_runtime_per_member: false,
            shared_runtime_reuse: false,
            expected_members,
            captured_members: 0,
            expected_resets,
            captured_resets: 0,
        },
        suite_id: execution.suite_id.clone(),
        launch_profile: execution.launch_profile.clone(),
        execution_plan: file_identity_from_bytes(execution_plan_path, &execution_bytes)?,
        session_id: None,
        connection_id: None,
        process_id: None,
        isolation_id: None,
        launch_plan_sha256: None,
        members: Vec::with_capacity(expected_members),
        resets: Vec::with_capacity(expected_resets),
        total_after_session_ready_ms: 0,
    };
    let outcome = capture_members(
        &execution,
        capture_directory,
        live_output_directory,
        expected_launch_plan,
        timeout,
        &mut report,
    );
    report.completed_at_unix_ms = current_unix_ms()?;
    report.total_after_session_ready_ms = started.elapsed().as_millis();
    match outcome {
        Ok(()) => {
            report.passed = true;
            report.evidence_scope.shared_runtime_reuse = true;
        }
        Err(error) => report.failure = Some(error),
    }
    write_json_report_new(output_report, &report)?;
    if report.passed {
        Ok(report)
    } else {
        Err(format!(
            "shared-runtime trajectory suite capture failed; durable report={}",
            absolute_string(output_report)?
        ))
    }
}

pub fn verify_shared_runtime_trajectory_suite_new(
    execution_plan_path: &Path,
    capture_report_path: &Path,
    lifecycle_receipt_path: &Path,
    comparison_directory: &Path,
    output_report: &Path,
) -> Result<SharedRuntimeTrajectorySuiteVerifiedReportV1, String> {
    if comparison_directory.exists() || output_report.exists() {
        return Err(
            "shared-runtime trajectory comparison directory and report must be create-new"
                .to_owned(),
        );
    }
    let execution_bytes = fs::read(execution_plan_path)
        .map_err(|error| format!("failed to read trajectory execution plan: {error}"))?;
    let execution: SharedRuntimeTrajectorySuiteExecutionPlanV1 =
        serde_json::from_slice(&execution_bytes)
            .map_err(|error| format!("failed to decode trajectory execution plan: {error}"))?;
    validate_execution(&execution)?;
    let capture_bytes = fs::read(capture_report_path)
        .map_err(|error| format!("failed to read trajectory suite capture: {error}"))?;
    let capture: SharedRuntimeTrajectorySuiteCaptureReportV1 =
        serde_json::from_slice(&capture_bytes)
            .map_err(|error| format!("failed to decode trajectory suite capture: {error}"))?;
    let execution_identity = file_identity_from_bytes(execution_plan_path, &execution_bytes)?;
    let capture_identity = file_identity_from_bytes(capture_report_path, &capture_bytes)?;
    validate_capture(&execution, &execution_identity, &capture)?;

    let session_id = capture.session_id.as_deref().unwrap();
    let isolation_id = capture.isolation_id.as_deref().unwrap();
    let launch_plan_sha256 = capture.launch_plan_sha256.as_deref().unwrap();
    let lifecycle = verify_isolated_lifecycle(
        lifecycle_receipt_path,
        IsolatedLifecycleExpectation {
            case_kind: "constructed_shared_runtime_trajectory_suite_capture",
            profile: &execution.launch_profile,
            control_profile: "parity_scenario_control",
            session_id,
            isolation_id,
            launch_plan_sha256,
            case_evidence: &capture_identity,
        },
    )?;

    fs::create_dir(comparison_directory).map_err(|error| {
        format!(
            "failed to create trajectory suite comparison directory '{}': {error}",
            comparison_directory.display()
        )
    })?;
    let mut members = Vec::with_capacity(capture.members.len());
    for (expected, captured) in execution.members.iter().zip(&capture.members) {
        let member_directory = comparison_directory.join(format!("member-{:02}", captured.index));
        fs::create_dir(&member_directory)
            .map_err(|error| format!("failed to create trajectory comparison member: {error}"))?;
        let step_directory = member_directory.join("steps");
        let comparison_path = member_directory.join("comparison.json");
        let comparison = recompare_constructed_trajectory_new(
            Path::new(&captured.trajectory_report.path),
            &step_directory,
            &comparison_path,
        )?;
        validate_member_comparison(expected, captured, &capture, &comparison)?;
        members.push(SharedRuntimeTrajectorySuiteVerifiedMemberV1 {
            index: captured.index,
            trajectory_case_id: captured.trajectory_case_id.clone(),
            scenario_case_id: captured.scenario_case_id.clone(),
            scenario_id: captured.scenario_id.clone(),
            root_frame_id: comparison
                .root_frame_id
                .ok_or_else(|| "passing trajectory comparison omitted root frame".to_owned())?,
            final_frame_id: comparison
                .final_frame_id
                .ok_or_else(|| "passing trajectory comparison omitted final frame".to_owned())?,
            completed_steps: comparison.steps.len(),
            comparison: file_identity(&comparison_path)?,
        });
    }

    let report = SharedRuntimeTrajectorySuiteVerifiedReportV1 {
        schema: REPORT_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        passed: true,
        suite_id: execution.suite_id,
        launch_profile: execution.launch_profile.clone(),
        evidence_scope: SharedRuntimeTrajectorySuiteVerifiedScopeV1 {
            natural_reachability: false,
            strategy_evidence: false,
            training_eligible: false,
            simulator_comparison: true,
            all_members_constructed_and_matched: true,
            all_inter_member_resets_verified: true,
            managed_lifecycle_verified: true,
            strict_runtime_profile: execution.launch_profile == "strict",
            fresh_runtime_per_member: false,
            shared_runtime_reuse: true,
        },
        execution_plan: execution_identity,
        capture_report: capture_identity,
        lifecycle_receipt: lifecycle.identity,
        session_id: session_id.to_owned(),
        process_id: capture.process_id.unwrap(),
        isolation_id: isolation_id.to_owned(),
        members,
        reset_reports: capture
            .resets
            .iter()
            .map(|reset| reset.report.clone())
            .collect(),
        capture_after_session_ready_ms: capture.total_after_session_ready_ms,
        lifecycle_total_ms: lifecycle.total_ms,
    };
    write_json_report_new(output_report, &report)?;
    Ok(report)
}

fn capture_members(
    execution: &SharedRuntimeTrajectorySuiteExecutionPlanV1,
    capture_directory: &Path,
    live_output_directory: &Path,
    expected_launch_plan: &Path,
    timeout: Duration,
    report: &mut SharedRuntimeTrajectorySuiteCaptureReportV1,
) -> Result<(), String> {
    let mut previous_reset_successor = None;
    for (index, member) in execution.members.iter().enumerate() {
        let member_started = Instant::now();
        let directory = capture_directory.join(format!("member-{index:02}"));
        fs::create_dir(&directory)
            .map_err(|error| format!("failed to create trajectory capture member: {error}"))?;
        let scenario_report_path = directory.join("scenario-report.json");
        let trajectory_report_path = directory.join("trajectory-report.json");
        let trajectory = run_constructed_combat_trajectory(
            live_output_directory,
            Path::new(&member.trajectory_plan.path),
            expected_launch_plan,
            &scenario_report_path,
            &trajectory_report_path,
            timeout,
        )?;
        let scenario: ConstructedScenarioReport = read_json(&scenario_report_path)?;
        let root = scenario
            .root
            .as_ref()
            .ok_or_else(|| "trajectory suite member omitted verified root".to_owned())?;
        let start = scenario
            .steps
            .first()
            .ok_or_else(|| "trajectory suite member omitted start_run receipt".to_owned())?;
        let final_successor = trajectory
            .steps
            .last()
            .and_then(|step| step.successor.as_ref())
            .ok_or_else(|| "trajectory suite member omitted final successor".to_owned())?;
        if !trajectory.passed
            || trajectory.completed_steps != member.steps.len()
            || trajectory.trajectory_case_id != member.trajectory_case_id
            || trajectory.root_frame_id != root.frame_id
            || start.name != "start_run"
            || previous_reset_successor.is_some_and(|frame| start.based_on_frame_id != frame)
            || root.scenario_context["scenario_id"].as_str() != Some(member.scenario_id.as_str())
            || final_successor.scenario_context["scenario_id"].as_str()
                != Some(member.scenario_id.as_str())
        {
            return Err("trajectory suite member broke its frame or scenario chain".to_owned());
        }
        bind_session(report, &trajectory.session, &execution.launch_profile)?;
        report
            .members
            .push(SharedRuntimeTrajectorySuiteCaptureMemberV1 {
                index,
                trajectory_case_id: member.trajectory_case_id.clone(),
                scenario_case_id: member.scenario_case_id.clone(),
                scenario_id: member.scenario_id.clone(),
                root_frame_id: root.frame_id,
                final_frame_id: final_successor.frame_id,
                completed_steps: trajectory.completed_steps,
                scenario_plan: member.scenario_plan.clone(),
                trajectory_plan: member.trajectory_plan.clone(),
                scenario_report: file_identity(&scenario_report_path)?,
                trajectory_report: file_identity(&trajectory_report_path)?,
                elapsed_ms: member_started.elapsed().as_millis(),
            });
        report.evidence_scope.captured_members = report.members.len();

        if index + 1 < execution.members.len() {
            if final_successor.boundary != "combat_player_decision" {
                return Err("trajectory suite cannot reset a non-combat final successor".to_owned());
            }
            let reset_started = Instant::now();
            let reset_report_path = directory.join("reset-report.json");
            let reset =
                reset_scenario_to_main_menu(live_output_directory, &reset_report_path, timeout)?;
            if !reset.passed
                || !reset.fresh_main_menu_verified
                || reset.from_frame_id != final_successor.frame_id
                || reset.scenario_context != final_successor.scenario_context
            {
                return Err("trajectory suite reset broke its final frame chain".to_owned());
            }
            report
                .resets
                .push(SharedRuntimeTrajectorySuiteCaptureResetV1 {
                    after_member_index: index,
                    from_frame_id: reset.from_frame_id,
                    successor_frame_id: reset.successor_frame_id,
                    report: file_identity(&reset_report_path)?,
                    elapsed_ms: reset_started.elapsed().as_millis(),
                });
            report.evidence_scope.captured_resets = report.resets.len();
            previous_reset_successor = Some(reset.successor_frame_id);
        }
    }
    if report.members.len() != execution.members.len()
        || report.resets.len() + 1 != execution.members.len()
    {
        return Err("trajectory suite ended before its complete member chain".to_owned());
    }
    Ok(())
}

fn bind_session(
    report: &mut SharedRuntimeTrajectorySuiteCaptureReportV1,
    session: &SessionManifest,
    expected_launch_profile: &str,
) -> Result<(), String> {
    let launch = session
        .launch
        .as_ref()
        .ok_or_else(|| "trajectory suite member omitted launch evidence".to_owned())?;
    let isolation_id = launch
        .isolation_id
        .as_ref()
        .ok_or_else(|| "trajectory suite member omitted isolation identity".to_owned())?;
    if launch.profile != expected_launch_profile
        || launch.storage_class != "isolated_working_directory_v1"
    {
        return Err("trajectory suite member used the wrong launch profile or storage".to_owned());
    }
    match &report.session_id {
        None => {
            report.session_id = Some(session.session_id.clone());
            report.connection_id = Some(session.connection_id.clone());
            report.process_id = Some(session.process_id);
            report.isolation_id = Some(isolation_id.clone());
            report.launch_plan_sha256 = Some(launch.plan_sha256.clone());
        }
        Some(existing)
            if existing == &session.session_id
                && report.connection_id.as_ref() == Some(&session.connection_id)
                && report.process_id == Some(session.process_id)
                && report.isolation_id.as_ref() == Some(isolation_id)
                && report.launch_plan_sha256.as_ref() == Some(&launch.plan_sha256) => {}
        Some(_) => {
            return Err("trajectory suite members did not preserve one runtime identity".to_owned())
        }
    }
    Ok(())
}

fn validate_plan(plan: &SharedRuntimeTrajectorySuitePlanV1) -> Result<(), String> {
    if plan.schema != PLAN_SCHEMA
        || plan.provenance != "constructed_validated"
        || plan.created_at_unix_ms == 0
        || !valid_exact_id(&plan.suite_id)
        || !matches!(plan.launch_profile.as_str(), "strict" | "accelerated")
        || !(2..=32).contains(&plan.members.len())
    {
        return Err("shared-runtime trajectory suite plan identity is invalid".to_owned());
    }
    validate_recorded_file_identity(&plan.manifest)?;
    let manifest: SharedRuntimeTrajectorySuiteManifestV1 =
        read_json(Path::new(&plan.manifest.path))?;
    validate_manifest(&manifest)?;
    if manifest.suite_id != plan.suite_id
        || manifest.launch_profile != plan.launch_profile
        || manifest.members.len() != plan.members.len()
    {
        return Err("trajectory suite plan differs from its manifest".to_owned());
    }
    let trajectory_catalog =
        scenario_trajectory_case_catalog(Path::new(&plan.trajectory_case_catalog_directory))?;
    let trajectories: BTreeMap<_, _> = trajectory_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.trajectory_case_id.clone(), entry))
        .collect();
    let scenario_catalog = scenario_case_catalog(Path::new(&plan.scenario_case_catalog_directory))?;
    let scenarios: BTreeMap<_, _> = scenario_catalog
        .cases
        .into_iter()
        .map(|entry| (entry.case_id.clone(), entry))
        .collect();
    let mut ids = BTreeSet::new();
    for ((manifest_member, member), index) in
        manifest.members.iter().zip(&plan.members).zip(0usize..)
    {
        validate_recorded_file_identity(&member.trajectory_draft)?;
        validate_recorded_file_identity(&member.scenario_draft)?;
        let trajectory = trajectories
            .get(&member.trajectory_case_id)
            .ok_or_else(|| format!("trajectory suite plan member {index} disappeared"))?;
        let scenario = scenarios
            .get(&member.scenario_case_id)
            .ok_or_else(|| format!("trajectory suite scenario {index} disappeared"))?;
        let catalog_trajectory_path =
            Path::new(&plan.trajectory_case_catalog_directory).join(&trajectory.draft_file);
        let catalog_scenario_path =
            Path::new(&plan.scenario_case_catalog_directory).join(&scenario.draft_file);
        if manifest_member.trajectory_case_id != member.trajectory_case_id
            || trajectory.scenario_case_id != member.scenario_case_id
            || trajectory.steps != member.steps
            || trajectory.draft_sha256 != member.trajectory_draft.sha256
            || scenario.draft_sha256 != member.scenario_draft.sha256
            || !same_path(
                &catalog_trajectory_path,
                Path::new(&member.trajectory_draft.path),
            )
            || !same_path(
                &catalog_scenario_path,
                Path::new(&member.scenario_draft.path),
            )
            || !ids.insert(member.trajectory_case_id.as_str())
        {
            return Err("trajectory suite plan member lost its catalog binding".to_owned());
        }
    }
    Ok(())
}

fn validate_execution(
    execution: &SharedRuntimeTrajectorySuiteExecutionPlanV1,
) -> Result<(), String> {
    if execution.schema != EXECUTION_SCHEMA
        || execution.provenance != "constructed_validated"
        || execution.created_at_unix_ms == 0
        || !valid_exact_id(&execution.suite_id)
        || !matches!(execution.launch_profile.as_str(), "strict" | "accelerated")
        || !(2..=32).contains(&execution.members.len())
    {
        return Err("shared-runtime trajectory execution identity is invalid".to_owned());
    }
    validate_recorded_file_identity(&execution.suite_plan)?;
    let plan: SharedRuntimeTrajectorySuitePlanV1 =
        read_json(Path::new(&execution.suite_plan.path))?;
    validate_plan(&plan)?;
    if execution.suite_id != plan.suite_id
        || execution.launch_profile != plan.launch_profile
        || execution.members.len() != plan.members.len()
    {
        return Err("trajectory execution differs from its suite plan".to_owned());
    }
    let mut scenario_ids = BTreeSet::new();
    for (index, (expected, member)) in plan.members.iter().zip(&execution.members).enumerate() {
        validate_recorded_file_identity(&member.scenario_plan)?;
        validate_recorded_file_identity(&member.trajectory_plan)?;
        let scenario: ScenarioPlan = read_json(Path::new(&member.scenario_plan.path))?;
        let trajectory: ScenarioTrajectoryPlan =
            read_json(Path::new(&member.trajectory_plan.path))?;
        if member.index != index
            || member.trajectory_case_id != expected.trajectory_case_id
            || member.scenario_case_id != expected.scenario_case_id
            || member.steps != expected.steps
            || scenario.schema != sts_java_parity_live::SCENARIO_PLAN_SCHEMA
            || scenario.provenance != "constructed_validated"
            || scenario.created_at_unix_ms == 0
            || scenario.recipe.scenario_id != member.scenario_id
            || scenario.recipe.case_id != member.scenario_case_id
            || trajectory.trajectory_case_id != member.trajectory_case_id
            || trajectory.scenario_case_id != member.scenario_case_id
            || trajectory.steps != member.steps
            || trajectory.schema != "sts_java_play_scenario_trajectory_plan_v1"
            || trajectory.provenance != "constructed_validated"
            || trajectory.created_at_unix_ms == 0
            || trajectory.trajectory_draft_sha256 != expected.trajectory_draft.sha256
            || !same_path(
                Path::new(&trajectory.trajectory_draft_path),
                Path::new(&expected.trajectory_draft.path),
            )
            || trajectory.scenario_plan_sha256 != member.scenario_plan.sha256
            || !same_path(
                Path::new(&trajectory.scenario_plan_path),
                Path::new(&member.scenario_plan.path),
            )
            || !scenario_ids.insert(member.scenario_id.as_str())
        {
            return Err("trajectory execution member lost its immutable binding".to_owned());
        }
    }
    Ok(())
}

fn validate_capture(
    execution: &SharedRuntimeTrajectorySuiteExecutionPlanV1,
    execution_identity: &ComparisonFileIdentityV1,
    capture: &SharedRuntimeTrajectorySuiteCaptureReportV1,
) -> Result<(), String> {
    let scope = &capture.evidence_scope;
    if capture.schema != CAPTURE_SCHEMA
        || capture.provenance != "constructed_validated"
        || !capture.passed
        || capture.failure.is_some()
        || capture.suite_id != execution.suite_id
        || capture.launch_profile != execution.launch_profile
        || scope.natural_reachability
        || scope.strategy_evidence
        || scope.training_eligible
        || scope.simulator_comparison
        || scope.fresh_runtime_per_member
        || !scope.shared_runtime_reuse
        || scope.expected_members != execution.members.len()
        || scope.captured_members != execution.members.len()
        || scope.expected_resets + 1 != execution.members.len()
        || scope.captured_resets != scope.expected_resets
        || capture.members.len() != execution.members.len()
        || capture.resets.len() != scope.expected_resets
        || !same_identity(&capture.execution_plan, execution_identity)
        || capture.session_id.is_none()
        || capture.connection_id.is_none()
        || capture.process_id.is_none()
        || capture.isolation_id.is_none()
        || capture.launch_plan_sha256.is_none()
    {
        return Err("shared-runtime trajectory capture identity or scope is invalid".to_owned());
    }
    let mut previous_reset_successor = None;
    for (index, (expected, captured)) in execution.members.iter().zip(&capture.members).enumerate()
    {
        for identity in [
            &captured.scenario_plan,
            &captured.trajectory_plan,
            &captured.scenario_report,
            &captured.trajectory_report,
        ] {
            validate_recorded_file_identity(identity)?;
        }
        let trajectory: ConstructedScenarioTrajectoryReport =
            read_json(Path::new(&captured.trajectory_report.path))?;
        let scenario: ConstructedScenarioReport =
            read_json(Path::new(&captured.scenario_report.path))?;
        let root = scenario
            .root
            .as_ref()
            .ok_or_else(|| "captured trajectory root disappeared".to_owned())?;
        let start = scenario
            .steps
            .first()
            .ok_or_else(|| "captured trajectory start receipt disappeared".to_owned())?;
        let final_successor = trajectory
            .steps
            .last()
            .and_then(|step| step.successor.as_ref())
            .ok_or_else(|| "captured trajectory final successor disappeared".to_owned())?;
        let launch = trajectory
            .session
            .launch
            .as_ref()
            .ok_or_else(|| "captured trajectory launch evidence disappeared".to_owned())?;
        if captured.index != index
            || captured.trajectory_case_id != expected.trajectory_case_id
            || captured.scenario_case_id != expected.scenario_case_id
            || captured.scenario_id != expected.scenario_id
            || captured.completed_steps != expected.steps.len()
            || !same_identity(&captured.scenario_plan, &expected.scenario_plan)
            || !same_identity(&captured.trajectory_plan, &expected.trajectory_plan)
            || trajectory.trajectory_case_id != expected.trajectory_case_id
            || !trajectory.passed
            || trajectory.failure.is_some()
            || trajectory.failed_step_index.is_some()
            || trajectory.planned_steps != expected.steps.len()
            || trajectory.completed_steps != expected.steps.len()
            || trajectory.steps.len() != expected.steps.len()
            || trajectory.evidence_scope.natural_reachability
            || trajectory.evidence_scope.strategy_evidence
            || trajectory.evidence_scope.training_eligible
            || !trajectory.evidence_scope.constructed_root_verified
            || !trajectory.evidence_scope.original_game_trajectory_captured
            || trajectory.evidence_scope.simulator_comparison
            || trajectory.session.session_id != capture.session_id.as_deref().unwrap()
            || trajectory.session.connection_id != capture.connection_id.as_deref().unwrap()
            || trajectory.session.process_id != capture.process_id.unwrap()
            || launch.profile != capture.launch_profile
            || launch.isolation_id.as_deref() != capture.isolation_id.as_deref()
            || launch.plan_sha256 != capture.launch_plan_sha256.as_deref().unwrap()
            || root.frame_id != captured.root_frame_id
            || final_successor.frame_id != captured.final_frame_id
            || start.name != "start_run"
            || previous_reset_successor.is_some_and(|frame| start.based_on_frame_id != frame)
            || root.scenario_context["scenario_id"].as_str() != Some(captured.scenario_id.as_str())
            || final_successor.scenario_context != root.scenario_context
        {
            return Err("captured trajectory suite member is inconsistent".to_owned());
        }
        if index < capture.resets.len() {
            let captured_reset = &capture.resets[index];
            validate_recorded_file_identity(&captured_reset.report)?;
            let reset: ScenarioResetReport = read_json(Path::new(&captured_reset.report.path))?;
            if captured_reset.after_member_index != index
                || captured_reset.from_frame_id != captured.final_frame_id
                || final_successor.boundary != "combat_player_decision"
                || reset.schema != "sts_java_parity_scenario_reset_v1"
                || reset.provenance != "constructed_validated"
                || !reset.passed
                || !reset.fresh_main_menu_verified
                || reset.session_id != capture.session_id.as_deref().unwrap()
                || reset.connection_id != capture.connection_id.as_deref().unwrap()
                || reset.from_frame_id != captured_reset.from_frame_id
                || reset.successor_frame_id != captured_reset.successor_frame_id
                || reset.scenario_context != final_successor.scenario_context
            {
                return Err("captured trajectory suite reset is inconsistent".to_owned());
            }
            previous_reset_successor = Some(reset.successor_frame_id);
        }
    }
    Ok(())
}

fn validate_member_comparison(
    expected: &SharedRuntimeTrajectorySuiteExecutionMemberV1,
    captured: &SharedRuntimeTrajectorySuiteCaptureMemberV1,
    capture: &SharedRuntimeTrajectorySuiteCaptureReportV1,
    comparison: &ConstructedTrajectoryComparisonAttemptV3,
) -> Result<(), String> {
    let scope = &comparison.evidence_scope;
    if comparison.schema != "sts_java_parity_constructed_trajectory_comparison_attempt_v4"
        || !comparison.passed
        || comparison.outcome != ConstructedTrajectoryComparisonOutcomeV3::Matched
        || comparison.failure.is_some()
        || comparison.provenance.as_deref() != Some("constructed_validated")
        || comparison.trajectory_case_id.as_deref() != Some(expected.trajectory_case_id.as_str())
        || comparison.launch_profile.as_deref() != Some(capture.launch_profile.as_str())
        || comparison.launch_plan_sha256.as_deref() != capture.launch_plan_sha256.as_deref()
        || comparison.session_id.as_deref() != capture.session_id.as_deref()
        || comparison.connection_id.as_deref() != capture.connection_id.as_deref()
        || comparison.process_id != capture.process_id
        || comparison.isolation_id.as_deref() != capture.isolation_id.as_deref()
        || comparison.root_frame_id != Some(captured.root_frame_id)
        || comparison.final_frame_id != Some(captured.final_frame_id)
        || comparison.planned_steps != Some(captured.completed_steps)
        || comparison.captured_steps != captured.completed_steps
        || comparison.comparison_selection
            != ConstructedTrajectoryComparisonSelectionV1::AllCapturedSteps
        || comparison.requested_step_indices != (0..captured.completed_steps).collect::<Vec<_>>()
        || comparison.completed_steps != captured.completed_steps
        || comparison.steps.len() != captured.completed_steps
        || scope.natural_reachability
        || scope.strategy_evidence
        || scope.training_eligible
        || !scope.constructed_root_verified
        || !scope.original_game_trajectory_captured
        || !scope.simulator_comparison_attempted
        || !scope.selected_steps_constructed_and_matched
        || !scope.all_steps_constructed_and_matched
        || !scope.narrow_mechanics_parity
    {
        return Err("trajectory suite member comparison is inconsistent".to_owned());
    }
    Ok(())
}

fn read_manifest_catalog(
    directory: &Path,
) -> Result<Vec<(PathBuf, Vec<u8>, SharedRuntimeTrajectorySuiteManifestV1)>, String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "failed to read shared-runtime trajectory suite catalog '{}': {error}",
            directory.display()
        )
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("failed to enumerate trajectory suite catalog: {error}"))?
            .path();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err("shared-runtime trajectory suite catalog contains no manifests".to_owned());
    }
    let mut ids = BTreeSet::new();
    let mut manifests = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path)
            .map_err(|error| format!("failed to read trajectory suite manifest: {error}"))?;
        let manifest: SharedRuntimeTrajectorySuiteManifestV1 = serde_json::from_slice(&bytes)
            .map_err(|error| format!("failed to decode trajectory suite manifest: {error}"))?;
        validate_manifest(&manifest)?;
        if path.file_name().and_then(|value| value.to_str())
            != Some(format!("{}.json", manifest.suite_id).as_str())
            || !ids.insert(manifest.suite_id.clone())
        {
            return Err("trajectory suite filename or exact suite identity is invalid".to_owned());
        }
        manifests.push((path, bytes, manifest));
    }
    Ok(manifests)
}

fn validate_manifest(manifest: &SharedRuntimeTrajectorySuiteManifestV1) -> Result<(), String> {
    if manifest.schema != MANIFEST_SCHEMA
        || manifest.provenance != "constructed_validated"
        || !valid_exact_id(&manifest.suite_id)
        || !matches!(manifest.launch_profile.as_str(), "strict" | "accelerated")
        || !(2..=32).contains(&manifest.members.len())
    {
        return Err("shared-runtime trajectory suite manifest is invalid".to_owned());
    }
    let mut ids = BTreeSet::new();
    if manifest.members.iter().any(|member| {
        !valid_exact_id(&member.trajectory_case_id)
            || !ids.insert(member.trajectory_case_id.as_str())
    }) {
        return Err("trajectory suite contains an invalid or duplicate member".to_owned());
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read JSON '{}': {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode JSON '{}': {error}", path.display()))
}

fn same_identity(left: &ComparisonFileIdentityV1, right: &ComparisonFileIdentityV1) -> bool {
    left.bytes == right.bytes
        && left.sha256 == right.sha256
        && same_path(Path::new(&left.path), Path::new(&right.path))
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

    #[test]
    fn trajectory_suite_manifest_rejects_duplicate_members() {
        let member = SharedRuntimeTrajectorySuiteMemberSpecV1 {
            trajectory_case_id: "cultist-two-strikes".to_owned(),
        };
        let manifest = SharedRuntimeTrajectorySuiteManifestV1 {
            schema: MANIFEST_SCHEMA.to_owned(),
            provenance: "constructed_validated".to_owned(),
            suite_id: "duplicate-trajectories".to_owned(),
            launch_profile: "accelerated".to_owned(),
            members: vec![member.clone(), member],
        };
        assert_eq!(
            validate_manifest(&manifest).unwrap_err(),
            "trajectory suite contains an invalid or duplicate member"
        );
    }

    #[test]
    fn tracked_trajectory_suite_compiles_and_prepares_distinct_scenarios() {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let suite_catalog =
            repository.join("fixtures/java_parity_shared_runtime_trajectory_suites");
        let trajectory_catalog = repository.join("fixtures/java_parity_trajectory_cases");
        let scenario_catalog = repository.join("fixtures/java_parity_cases");
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp = std::env::temp_dir().join(format!(
            "shared-trajectory-suite-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&temp).unwrap();
        let plan_path = temp.join("suite-plan.json");
        let plan = compile_shared_runtime_trajectory_suite_case(
            &suite_catalog,
            "cultist-shared-runtime-trajectories",
            &trajectory_catalog,
            &scenario_catalog,
            &plan_path,
        )
        .unwrap();
        assert_eq!(plan.members.len(), 2);
        let mut reordered = plan.clone();
        reordered.members.swap(0, 1);
        assert!(validate_plan(&reordered).is_err());
        let execution = prepare_shared_runtime_trajectory_suite_execution(
            &plan_path,
            &temp.join("member-plans"),
            &temp.join("execution-plan.json"),
        )
        .unwrap();
        assert_eq!(execution.members.len(), 2);
        assert_ne!(
            execution.members[0].scenario_id,
            execution.members[1].scenario_id
        );
        let mut reused_identity = execution.clone();
        reused_identity.members[1].scenario_id = reused_identity.members[0].scenario_id.clone();
        assert!(validate_execution(&reused_identity).is_err());
        fs::remove_dir_all(temp).unwrap();
    }
}
