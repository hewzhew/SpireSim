use super::{
    absolute_string, valid_exact_id, validate_recorded_file_identity,
    validate_shared_runtime_suite_plan, SharedRuntimeSuitePlanV1,
};
use crate::lifecycle::{verify_isolated_lifecycle, IsolatedLifecycleExpectation};
use crate::{
    current_unix_ms, file_identity, file_identity_from_bytes, write_json_report_new,
    ComparisonFileIdentityV1, ConstructedComparisonWorkflowV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use sts_java_parity_live::{
    compile_scenario_probe_case, reset_scenario_to_main_menu, run_constructed_combat_probe,
    ConstructedScenarioProbeReport, ConstructedScenarioReport, ScenarioPlan, ScenarioProbePlan,
    ScenarioProbeTransition, ScenarioResetReport,
};

const SHARED_EXECUTION_PLAN_SCHEMA: &str = "sts_java_parity_shared_runtime_suite_execution_plan_v1";
pub(super) const SHARED_CAPTURE_SCHEMA: &str = "sts_java_parity_shared_runtime_suite_capture_v1";
const SHARED_REPORT_SCHEMA: &str = "sts_java_parity_shared_runtime_suite_report_v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteExecutionPlanV1 {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub suite_id: String,
    pub launch_profile: String,
    pub suite_plan: ComparisonFileIdentityV1,
    pub members: Vec<SharedRuntimeSuiteExecutionMemberV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteExecutionMemberV1 {
    pub index: usize,
    pub probe_case_id: String,
    pub case_id: String,
    pub transition: ScenarioProbeTransition,
    pub scenario_id: String,
    pub scenario_plan: ComparisonFileIdentityV1,
    pub probe_plan: ComparisonFileIdentityV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteCaptureScopeV1 {
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
pub struct SharedRuntimeSuiteCaptureReportV1 {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub failure: Option<String>,
    pub evidence_scope: SharedRuntimeSuiteCaptureScopeV1,
    pub suite_id: String,
    pub launch_profile: String,
    pub execution_plan: ComparisonFileIdentityV1,
    pub session_id: Option<String>,
    pub connection_id: Option<String>,
    pub process_id: Option<u32>,
    pub isolation_id: Option<String>,
    pub launch_plan_sha256: Option<String>,
    pub members: Vec<SharedRuntimeSuiteCaptureMemberV1>,
    pub resets: Vec<SharedRuntimeSuiteCaptureResetV1>,
    pub total_after_session_ready_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteCaptureMemberV1 {
    pub index: usize,
    pub probe_case_id: String,
    pub case_id: String,
    pub transition_id: String,
    pub scenario_id: String,
    pub root_frame_id: u64,
    pub successor_frame_id: u64,
    pub scenario_plan: ComparisonFileIdentityV1,
    pub probe_plan: ComparisonFileIdentityV1,
    pub root_report: ComparisonFileIdentityV1,
    pub probe_report: ComparisonFileIdentityV1,
    pub elapsed_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteCaptureResetV1 {
    pub after_member_index: usize,
    pub from_frame_id: u64,
    pub successor_frame_id: u64,
    pub report: ComparisonFileIdentityV1,
    pub elapsed_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteVerifiedScopeV1 {
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
pub struct SharedRuntimeSuiteVerifiedReportV1 {
    pub schema: String,
    pub provenance: String,
    pub created_at_unix_ms: u128,
    pub passed: bool,
    pub suite_id: String,
    pub launch_profile: String,
    pub evidence_scope: SharedRuntimeSuiteVerifiedScopeV1,
    pub execution_plan: ComparisonFileIdentityV1,
    pub capture_report: ComparisonFileIdentityV1,
    pub lifecycle_receipt: ComparisonFileIdentityV1,
    pub session_id: String,
    pub process_id: u32,
    pub isolation_id: String,
    pub members: Vec<SharedRuntimeSuiteVerifiedMemberV1>,
    pub reset_reports: Vec<ComparisonFileIdentityV1>,
    pub capture_after_session_ready_ms: u128,
    pub lifecycle_total_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedRuntimeSuiteVerifiedMemberV1 {
    pub index: usize,
    pub probe_case_id: String,
    pub case_id: String,
    pub transition_id: String,
    pub scenario_id: String,
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub comparison_kind: String,
    pub workflow: ComparisonFileIdentityV1,
}

pub fn prepare_shared_runtime_suite_execution(
    suite_plan_path: &Path,
    member_plan_directory: &Path,
    output_plan: &Path,
) -> Result<SharedRuntimeSuiteExecutionPlanV1, String> {
    let suite_plan_bytes = fs::read(suite_plan_path).map_err(|error| {
        format!(
            "failed to read shared-runtime suite plan '{}': {error}",
            suite_plan_path.display()
        )
    })?;
    let suite_plan: SharedRuntimeSuitePlanV1 = serde_json::from_slice(&suite_plan_bytes)
        .map_err(|error| format!("failed to decode shared-runtime suite plan: {error}"))?;
    validate_shared_runtime_suite_plan(&suite_plan)?;
    if member_plan_directory.exists() {
        return Err(format!(
            "shared-runtime member plan directory must be create-new: {}",
            member_plan_directory.display()
        ));
    }
    if output_plan.exists() {
        return Err(format!(
            "shared-runtime execution plan must be create-new: {}",
            output_plan.display()
        ));
    }
    fs::create_dir(member_plan_directory).map_err(|error| {
        format!(
            "failed to create shared-runtime member plan directory '{}': {error}",
            member_plan_directory.display()
        )
    })?;

    let mut scenario_ids = BTreeSet::new();
    let mut members = Vec::with_capacity(suite_plan.members.len());
    for (index, expected) in suite_plan.members.iter().enumerate() {
        let member_directory = member_plan_directory.join(format!("member-{index:02}"));
        fs::create_dir(&member_directory).map_err(|error| {
            format!(
                "failed to create shared-runtime member directory '{}': {error}",
                member_directory.display()
            )
        })?;
        let scenario_plan_path = member_directory.join("scenario-plan.json");
        let probe_plan_path = member_directory.join("probe-plan.json");
        let probe_plan = compile_scenario_probe_case(
            Path::new(&suite_plan.probe_case_catalog_directory),
            &expected.probe_case_id,
            Path::new(&suite_plan.scenario_case_catalog_directory),
            &scenario_plan_path,
            &probe_plan_path,
        )?;
        let scenario_plan: ScenarioPlan = read_json_file(&scenario_plan_path)?;
        if probe_plan.probe_case_id != expected.probe_case_id
            || probe_plan.scenario_case_id != expected.case_id
            || probe_plan.transition != expected.transition
            || scenario_plan.recipe.case_id != expected.case_id
            || !scenario_ids.insert(scenario_plan.recipe.scenario_id.clone())
        {
            return Err(
                "compiled shared-runtime member differs from its immutable suite plan".to_owned(),
            );
        }
        members.push(SharedRuntimeSuiteExecutionMemberV1 {
            index,
            probe_case_id: expected.probe_case_id.clone(),
            case_id: expected.case_id.clone(),
            transition: expected.transition.clone(),
            scenario_id: scenario_plan.recipe.scenario_id,
            scenario_plan: file_identity(&scenario_plan_path)?,
            probe_plan: file_identity(&probe_plan_path)?,
        });
    }

    let execution = SharedRuntimeSuiteExecutionPlanV1 {
        schema: SHARED_EXECUTION_PLAN_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        suite_id: suite_plan.suite_id,
        launch_profile: suite_plan.launch_profile,
        suite_plan: file_identity_from_bytes(suite_plan_path, &suite_plan_bytes)?,
        members,
    };
    validate_shared_runtime_suite_execution_plan(&execution)?;
    write_json_report_new(output_plan, &execution)?;
    Ok(execution)
}

pub fn capture_shared_runtime_suite(
    execution_plan_path: &Path,
    capture_directory: &Path,
    live_output_directory: &Path,
    expected_launch_plan: &Path,
    output_report: &Path,
    timeout: Duration,
) -> Result<SharedRuntimeSuiteCaptureReportV1, String> {
    let execution_plan_bytes = fs::read(execution_plan_path).map_err(|error| {
        format!(
            "failed to read shared-runtime execution plan '{}': {error}",
            execution_plan_path.display()
        )
    })?;
    let execution: SharedRuntimeSuiteExecutionPlanV1 =
        serde_json::from_slice(&execution_plan_bytes)
            .map_err(|error| format!("failed to decode shared-runtime execution plan: {error}"))?;
    validate_shared_runtime_suite_execution_plan(&execution)?;
    if capture_directory.exists() || output_report.exists() {
        return Err(
            "shared-runtime capture directory and report must both be create-new".to_owned(),
        );
    }
    if let Some(parent) = output_report.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to prepare shared-runtime capture report parent '{}': {error}",
                parent.display()
            )
        })?;
    }
    fs::create_dir(capture_directory).map_err(|error| {
        format!(
            "failed to create shared-runtime capture directory '{}': {error}",
            capture_directory.display()
        )
    })?;

    let created_at_unix_ms = current_unix_ms()?;
    let started = Instant::now();
    let expected_members = execution.members.len();
    let expected_resets = expected_members - 1;
    let mut report = SharedRuntimeSuiteCaptureReportV1 {
        schema: SHARED_CAPTURE_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms,
        completed_at_unix_ms: created_at_unix_ms,
        passed: false,
        failure: None,
        evidence_scope: SharedRuntimeSuiteCaptureScopeV1 {
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
        execution_plan: file_identity_from_bytes(execution_plan_path, &execution_plan_bytes)?,
        session_id: None,
        connection_id: None,
        process_id: None,
        isolation_id: None,
        launch_plan_sha256: None,
        members: Vec::with_capacity(expected_members),
        resets: Vec::with_capacity(expected_resets),
        total_after_session_ready_ms: 0,
    };

    let outcome = capture_shared_runtime_members(
        &execution,
        capture_directory,
        live_output_directory,
        expected_launch_plan,
        timeout,
        started,
        &mut report,
    );
    publish_shared_runtime_capture_result(
        report,
        output_report,
        outcome,
        started.elapsed().as_millis(),
    )
}

pub(super) fn publish_shared_runtime_capture_result(
    mut report: SharedRuntimeSuiteCaptureReportV1,
    output_report: &Path,
    outcome: Result<(), String>,
    elapsed_ms: u128,
) -> Result<SharedRuntimeSuiteCaptureReportV1, String> {
    report.completed_at_unix_ms = current_unix_ms()?;
    report.total_after_session_ready_ms = elapsed_ms;
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
            "shared-runtime suite capture failed; durable report={}",
            absolute_string(output_report)?
        ))
    }
}

pub fn verify_shared_runtime_suite_new(
    execution_plan_path: &Path,
    capture_report_path: &Path,
    lifecycle_receipt_path: &Path,
    comparison_directory: &Path,
    output_report: &Path,
) -> Result<SharedRuntimeSuiteVerifiedReportV1, String> {
    if comparison_directory.exists() || output_report.exists() {
        return Err(
            "shared-runtime comparison directory and report must both be create-new".to_owned(),
        );
    }
    let execution_plan_bytes = fs::read(execution_plan_path).map_err(|error| {
        format!(
            "failed to read shared-runtime execution plan '{}': {error}",
            execution_plan_path.display()
        )
    })?;
    let execution: SharedRuntimeSuiteExecutionPlanV1 =
        serde_json::from_slice(&execution_plan_bytes)
            .map_err(|error| format!("failed to decode shared-runtime execution plan: {error}"))?;
    validate_shared_runtime_suite_execution_plan(&execution)?;
    let capture_bytes = fs::read(capture_report_path).map_err(|error| {
        format!(
            "failed to read shared-runtime capture report '{}': {error}",
            capture_report_path.display()
        )
    })?;
    let capture: SharedRuntimeSuiteCaptureReportV1 = serde_json::from_slice(&capture_bytes)
        .map_err(|error| format!("failed to decode shared-runtime capture report: {error}"))?;
    let execution_identity = file_identity_from_bytes(execution_plan_path, &execution_plan_bytes)?;
    let capture_identity = file_identity_from_bytes(capture_report_path, &capture_bytes)?;
    validate_shared_runtime_capture(&execution, &execution_identity, &capture)?;
    let session_id = capture
        .session_id
        .as_deref()
        .ok_or_else(|| "shared-runtime capture omitted session identity".to_owned())?;
    let isolation_id = capture
        .isolation_id
        .as_deref()
        .ok_or_else(|| "shared-runtime capture omitted isolation identity".to_owned())?;
    let launch_plan_sha256 = capture
        .launch_plan_sha256
        .as_deref()
        .ok_or_else(|| "shared-runtime capture omitted launch plan identity".to_owned())?;
    let lifecycle = verify_isolated_lifecycle(
        lifecycle_receipt_path,
        IsolatedLifecycleExpectation {
            case_kind: "constructed_shared_runtime_suite_capture",
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
            "failed to create shared-runtime comparison directory '{}': {error}",
            comparison_directory.display()
        )
    })?;
    let mut members = Vec::with_capacity(capture.members.len());
    for (expected, captured) in execution.members.iter().zip(&capture.members) {
        let member_directory = comparison_directory.join(format!("member-{:02}", captured.index));
        fs::create_dir(&member_directory).map_err(|error| {
            format!(
                "failed to create shared-runtime comparison member directory '{}': {error}",
                member_directory.display()
            )
        })?;
        let comparison_path = member_directory.join("comparison.json");
        let workflow_path = member_directory.join("workflow.json");
        let (_, workflow) = crate::recompare_constructed_probe_new(
            Path::new(&captured.probe_report.path),
            &comparison_path,
            &workflow_path,
        )?;
        validate_shared_runtime_member_workflow(expected, &execution.launch_profile, &workflow)?;
        if workflow.session_id != capture.session_id.as_deref().unwrap_or_default()
            || workflow.from_frame_id != captured.root_frame_id
            || workflow.to_frame_id != captured.successor_frame_id
        {
            return Err(
                "shared-runtime member workflow differs from its captured frame chain".to_owned(),
            );
        }
        members.push(SharedRuntimeSuiteVerifiedMemberV1 {
            index: captured.index,
            probe_case_id: captured.probe_case_id.clone(),
            case_id: captured.case_id.clone(),
            transition_id: captured.transition_id.clone(),
            scenario_id: captured.scenario_id.clone(),
            from_frame_id: workflow.from_frame_id,
            to_frame_id: workflow.to_frame_id,
            comparison_kind: workflow.comparison_kind,
            workflow: file_identity(&workflow_path)?,
        });
    }

    let report = SharedRuntimeSuiteVerifiedReportV1 {
        schema: SHARED_REPORT_SCHEMA.to_owned(),
        provenance: "constructed_validated".to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        passed: true,
        suite_id: execution.suite_id,
        launch_profile: execution.launch_profile.clone(),
        evidence_scope: SharedRuntimeSuiteVerifiedScopeV1 {
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

pub(super) fn validate_shared_runtime_suite_execution_plan(
    execution: &SharedRuntimeSuiteExecutionPlanV1,
) -> Result<(), String> {
    if execution.schema != SHARED_EXECUTION_PLAN_SCHEMA
        || execution.provenance != "constructed_validated"
        || execution.created_at_unix_ms == 0
        || !valid_exact_id(&execution.suite_id)
        || !matches!(execution.launch_profile.as_str(), "strict" | "accelerated")
        || !(2..=32).contains(&execution.members.len())
    {
        return Err("shared-runtime execution plan identity or member count is invalid".to_owned());
    }
    validate_recorded_file_identity(&execution.suite_plan)?;
    let suite_plan: SharedRuntimeSuitePlanV1 =
        read_json_file(Path::new(&execution.suite_plan.path))?;
    validate_shared_runtime_suite_plan(&suite_plan)?;
    if execution.suite_id != suite_plan.suite_id
        || execution.launch_profile != suite_plan.launch_profile
        || execution.members.len() != suite_plan.members.len()
    {
        return Err("shared-runtime execution plan differs from its suite plan".to_owned());
    }
    let mut scenario_ids = BTreeSet::new();
    for (index, (expected, member)) in suite_plan
        .members
        .iter()
        .zip(&execution.members)
        .enumerate()
    {
        validate_recorded_file_identity(&member.scenario_plan)?;
        validate_recorded_file_identity(&member.probe_plan)?;
        let scenario_plan: ScenarioPlan = read_json_file(Path::new(&member.scenario_plan.path))?;
        let probe_plan: ScenarioProbePlan = read_json_file(Path::new(&member.probe_plan.path))?;
        if member.index != index
            || member.probe_case_id != expected.probe_case_id
            || member.case_id != expected.case_id
            || member.transition != expected.transition
        {
            return Err("shared-runtime execution member differs from suite order".to_owned());
        }
        if member.scenario_id != scenario_plan.recipe.scenario_id
            || scenario_plan.recipe.case_id != member.case_id
        {
            return Err("shared-runtime execution member scenario plan is inconsistent".to_owned());
        }
        if probe_plan.probe_case_id != member.probe_case_id
            || probe_plan.scenario_case_id != member.case_id
            || probe_plan.transition != member.transition
        {
            return Err("shared-runtime execution member probe plan is inconsistent".to_owned());
        }
        let bound_scenario_path = fs::canonicalize(&probe_plan.scenario_plan_path)
            .map_err(|error| format!("failed to resolve probe-bound scenario plan: {error}"))?;
        let recorded_scenario_path = fs::canonicalize(&member.scenario_plan.path)
            .map_err(|error| format!("failed to resolve recorded scenario plan: {error}"))?;
        if bound_scenario_path != recorded_scenario_path {
            return Err("shared-runtime probe plan lost its scenario-plan path binding".to_owned());
        }
        if probe_plan.scenario_plan_sha256 != member.scenario_plan.sha256 {
            return Err("shared-runtime probe plan lost its scenario-plan hash binding".to_owned());
        }
        if !scenario_ids.insert(member.scenario_id.as_str()) {
            return Err("shared-runtime execution reused a scenario UUID".to_owned());
        }
    }
    Ok(())
}

fn validate_shared_runtime_capture(
    execution: &SharedRuntimeSuiteExecutionPlanV1,
    execution_identity: &ComparisonFileIdentityV1,
    capture: &SharedRuntimeSuiteCaptureReportV1,
) -> Result<(), String> {
    let scope = &capture.evidence_scope;
    if capture.schema != SHARED_CAPTURE_SCHEMA
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
        || !same_file_identity(&capture.execution_plan, execution_identity)
    {
        return Err("shared-runtime capture identity or evidence scope is invalid".to_owned());
    }
    let session_id = capture
        .session_id
        .as_deref()
        .ok_or_else(|| "shared-runtime capture omitted its session ID".to_owned())?;
    let connection_id = capture
        .connection_id
        .as_deref()
        .ok_or_else(|| "shared-runtime capture omitted its connection ID".to_owned())?;
    let process_id = capture
        .process_id
        .ok_or_else(|| "shared-runtime capture omitted its process ID".to_owned())?;
    let isolation_id = capture
        .isolation_id
        .as_deref()
        .ok_or_else(|| "shared-runtime capture omitted its isolation ID".to_owned())?;
    let launch_plan_sha256 = capture
        .launch_plan_sha256
        .as_deref()
        .ok_or_else(|| "shared-runtime capture omitted its launch plan hash".to_owned())?;
    let mut previous_reset_successor = None;
    let mut scenario_ids = BTreeSet::new();
    for (index, (expected, captured)) in execution.members.iter().zip(&capture.members).enumerate()
    {
        for identity in [
            &captured.scenario_plan,
            &captured.probe_plan,
            &captured.root_report,
            &captured.probe_report,
        ] {
            validate_recorded_file_identity(identity)?;
        }
        if captured.index != index
            || captured.probe_case_id != expected.probe_case_id
            || captured.case_id != expected.case_id
            || captured.transition_id != expected.transition.id()
            || captured.scenario_id != expected.scenario_id
            || !same_file_identity(&captured.scenario_plan, &expected.scenario_plan)
            || !same_file_identity(&captured.probe_plan, &expected.probe_plan)
            || !scenario_ids.insert(captured.scenario_id.as_str())
        {
            return Err(
                "shared-runtime captured member differs from its execution plan".to_owned(),
            );
        }
        let root_report: ConstructedScenarioReport =
            read_json_file(Path::new(&captured.root_report.path))?;
        let probe_report: ConstructedScenarioProbeReport =
            read_json_file(Path::new(&captured.probe_report.path))?;
        let root = root_report
            .root
            .as_ref()
            .ok_or_else(|| "shared-runtime root report omitted its root".to_owned())?;
        let successor = probe_report
            .successor
            .as_ref()
            .ok_or_else(|| "shared-runtime probe report omitted its successor".to_owned())?;
        let action = probe_report
            .action
            .as_ref()
            .ok_or_else(|| "shared-runtime probe report omitted its action".to_owned())?;
        let start = root_report
            .steps
            .first()
            .ok_or_else(|| "shared-runtime root report omitted start_run".to_owned())?;
        let launch = probe_report
            .session
            .launch
            .as_ref()
            .ok_or_else(|| "shared-runtime probe report omitted launch evidence".to_owned())?;
        if probe_report.schema != "sts_java_parity_constructed_probe_report_v2"
            || !probe_report.passed
            || probe_report.provenance != "constructed_validated"
            || probe_report.probe_case_id != captured.probe_case_id
            || probe_report.transition_id != captured.transition_id
            || probe_report.session.session_id != session_id
            || probe_report.session.connection_id != connection_id
            || probe_report.session.process_id != process_id
            || launch.profile != capture.launch_profile
            || launch.storage_class != "isolated_working_directory_v1"
            || launch.isolation_id.as_deref() != Some(isolation_id)
            || launch.plan_sha256 != launch_plan_sha256
            || root.frame_id != captured.root_frame_id
            || probe_report.root_frame_id != captured.root_frame_id
            || action.successor_frame_id != Some(captured.successor_frame_id)
            || successor.frame_id != captured.successor_frame_id
            || start.name != "start_run"
            || previous_reset_successor.is_some_and(|frame| start.based_on_frame_id != frame)
            || root.scenario_context["scenario_id"].as_str() != Some(captured.scenario_id.as_str())
            || successor.scenario_context["scenario_id"].as_str()
                != Some(captured.scenario_id.as_str())
        {
            return Err("shared-runtime captured member evidence is inconsistent".to_owned());
        }

        if index < capture.resets.len() {
            let captured_reset = &capture.resets[index];
            validate_recorded_file_identity(&captured_reset.report)?;
            let reset: ScenarioResetReport =
                read_json_file(Path::new(&captured_reset.report.path))?;
            if captured_reset.after_member_index != index
                || captured_reset.from_frame_id != captured.successor_frame_id
                || reset.schema != "sts_java_parity_scenario_reset_v1"
                || reset.provenance != "constructed_validated"
                || !reset.passed
                || !reset.fresh_main_menu_verified
                || reset.session_id != session_id
                || reset.connection_id != connection_id
                || reset.from_frame_id != captured_reset.from_frame_id
                || reset.successor_frame_id != captured_reset.successor_frame_id
                || reset.scenario_context != successor.scenario_context
            {
                return Err("shared-runtime reset evidence is inconsistent".to_owned());
            }
            previous_reset_successor = Some(captured_reset.successor_frame_id);
        }
    }
    Ok(())
}

fn same_file_identity(left: &ComparisonFileIdentityV1, right: &ComparisonFileIdentityV1) -> bool {
    canonical_paths_equal(Path::new(&left.path), Path::new(&right.path))
        && left.bytes == right.bytes
        && left.sha256 == right.sha256
}

fn canonical_paths_equal(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn capture_shared_runtime_members(
    execution: &SharedRuntimeSuiteExecutionPlanV1,
    capture_directory: &Path,
    live_output_directory: &Path,
    expected_launch_plan: &Path,
    timeout: Duration,
    started: Instant,
    report: &mut SharedRuntimeSuiteCaptureReportV1,
) -> Result<(), String> {
    let mut previous_reset_successor = None;
    for (index, member) in execution.members.iter().enumerate() {
        let member_started = Instant::now();
        let member_directory = capture_directory.join(format!("member-{index:02}"));
        fs::create_dir(&member_directory).map_err(|error| {
            format!(
                "failed to create shared-runtime capture member directory '{}': {error}",
                member_directory.display()
            )
        })?;
        let root_report_path = member_directory.join("root-report.json");
        let probe_report_path = member_directory.join("probe-report.json");
        let probe = run_constructed_combat_probe(
            live_output_directory,
            Path::new(&member.probe_plan.path),
            expected_launch_plan,
            &root_report_path,
            &probe_report_path,
            timeout,
        )?;
        let root_report: ConstructedScenarioReport = read_json_file(&root_report_path)?;
        let root = root_report
            .root
            .as_ref()
            .ok_or_else(|| "shared-runtime member omitted its verified root".to_owned())?;
        let action = probe
            .action
            .as_ref()
            .ok_or_else(|| "shared-runtime member omitted its action receipt".to_owned())?;
        let successor = probe
            .successor
            .as_ref()
            .ok_or_else(|| "shared-runtime member omitted its verified successor".to_owned())?;
        let successor_frame_id = action
            .successor_frame_id
            .ok_or_else(|| "shared-runtime member action omitted its successor".to_owned())?;
        let start = root_report
            .steps
            .first()
            .ok_or_else(|| "shared-runtime member omitted its start_run receipt".to_owned())?;
        if !probe.passed
            || root.frame_id != probe.root_frame_id
            || successor.frame_id != successor_frame_id
            || start.name != "start_run"
            || previous_reset_successor.is_some_and(|frame_id| start.based_on_frame_id != frame_id)
            || root.scenario_context["scenario_id"].as_str() != Some(member.scenario_id.as_str())
            || successor.scenario_context["scenario_id"].as_str()
                != Some(member.scenario_id.as_str())
        {
            return Err("shared-runtime member broke its frame or scenario chain".to_owned());
        }
        bind_shared_runtime_session(report, &probe, &execution.launch_profile)?;
        report.members.push(SharedRuntimeSuiteCaptureMemberV1 {
            index,
            probe_case_id: member.probe_case_id.clone(),
            case_id: member.case_id.clone(),
            transition_id: member.transition.id().to_owned(),
            scenario_id: member.scenario_id.clone(),
            root_frame_id: root.frame_id,
            successor_frame_id,
            scenario_plan: member.scenario_plan.clone(),
            probe_plan: member.probe_plan.clone(),
            root_report: file_identity(&root_report_path)?,
            probe_report: file_identity(&probe_report_path)?,
            elapsed_ms: member_started.elapsed().as_millis(),
        });
        report.evidence_scope.captured_members = report.members.len();

        if index + 1 < execution.members.len() {
            let reset_started = Instant::now();
            let reset_report_path = member_directory.join("reset-report.json");
            let reset =
                reset_scenario_to_main_menu(live_output_directory, &reset_report_path, timeout)?;
            if !reset.passed
                || !reset.fresh_main_menu_verified
                || reset.from_frame_id != successor_frame_id
                || reset.scenario_context != successor.scenario_context
            {
                return Err("shared-runtime inter-member reset broke its frame chain".to_owned());
            }
            report.resets.push(SharedRuntimeSuiteCaptureResetV1 {
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
    report.total_after_session_ready_ms = started.elapsed().as_millis();
    if report.members.len() != execution.members.len()
        || report.resets.len() + 1 != execution.members.len()
    {
        return Err("shared-runtime capture ended before its complete member chain".to_owned());
    }
    Ok(())
}

fn bind_shared_runtime_session(
    report: &mut SharedRuntimeSuiteCaptureReportV1,
    probe: &ConstructedScenarioProbeReport,
    expected_launch_profile: &str,
) -> Result<(), String> {
    let launch = probe
        .session
        .launch
        .as_ref()
        .ok_or_else(|| "shared-runtime member omitted launch evidence".to_owned())?;
    let isolation_id = launch
        .isolation_id
        .as_ref()
        .ok_or_else(|| "shared-runtime member omitted isolation identity".to_owned())?;
    if launch.profile != expected_launch_profile
        || launch.storage_class != "isolated_working_directory_v1"
    {
        return Err("shared-runtime member used the wrong launch profile or storage".to_owned());
    }
    match &report.session_id {
        None => {
            report.session_id = Some(probe.session.session_id.clone());
            report.connection_id = Some(probe.session.connection_id.clone());
            report.process_id = Some(probe.session.process_id);
            report.isolation_id = Some(isolation_id.clone());
            report.launch_plan_sha256 = Some(launch.plan_sha256.clone());
        }
        Some(session_id)
            if session_id == &probe.session.session_id
                && report.connection_id.as_ref() == Some(&probe.session.connection_id)
                && report.process_id == Some(probe.session.process_id)
                && report.isolation_id.as_ref() == Some(isolation_id)
                && report.launch_plan_sha256.as_ref() == Some(&launch.plan_sha256) => {}
        Some(_) => {
            return Err(
                "shared-runtime members did not preserve one session/process/isolation identity"
                    .to_owned(),
            )
        }
    }
    Ok(())
}

pub(super) fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read JSON file '{}': {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode JSON file '{}': {error}", path.display()))
}

fn validate_shared_runtime_member_workflow(
    expected: &SharedRuntimeSuiteExecutionMemberV1,
    launch_profile: &str,
    workflow: &ConstructedComparisonWorkflowV1,
) -> Result<(), String> {
    let evidence = &workflow.evidence_scope;
    if workflow.schema != "sts_java_parity_constructed_comparison_workflow_v1"
        || !workflow.passed
        || workflow.provenance != "constructed_validated"
        || workflow.case_id != expected.case_id
        || workflow.transition_id != expected.transition.id()
        || workflow.launch_profile != launch_profile
        || evidence.natural_reachability
        || evidence.strategy_evidence
        || evidence.training_eligible
        || !evidence.constructed_root_verified
        || !evidence.original_game_transition_captured
        || !evidence.simulator_comparison
        || !evidence.narrow_mechanics_parity
        || evidence.strict_runtime_profile != (launch_profile == "strict")
    {
        return Err(
            "shared-runtime member workflow is inconsistent with its execution plan".to_owned(),
        );
    }
    Ok(())
}
