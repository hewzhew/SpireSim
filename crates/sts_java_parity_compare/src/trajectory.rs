use crate::lifecycle::{verify_isolated_lifecycle, IsolatedLifecycleExpectation};
use crate::{
    compare_live_action_typed, current_unix_ms, file_identity, file_identity_from_bytes,
    write_json_report_new, ComparisonFileIdentityV1, JavaParityLiveComparisonFailureKindV1,
    JavaParityLiveComparisonFailureV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use sts_java_parity_live::{
    ActionReceipt, ConstructedScenarioReport, ConstructedScenarioTrajectoryReport, ScenarioPlan,
    ScenarioTrajectoryDraft, ScenarioTrajectoryPlan,
};

const TRAJECTORY_REPORT_SCHEMA: &str = "sts_java_parity_constructed_trajectory_report_v1";
const TRAJECTORY_PLAN_SCHEMA: &str = "sts_java_play_scenario_trajectory_plan_v1";
const TRAJECTORY_DRAFT_SCHEMA: &str = "sts_java_play_scenario_trajectory_draft_v1";
const TRAJECTORY_WORKFLOW_SCHEMA: &str = "sts_java_parity_constructed_trajectory_workflow_v3";
const TRAJECTORY_ATTEMPT_SCHEMA: &str =
    "sts_java_parity_constructed_trajectory_comparison_attempt_v4";

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedTrajectoryWorkflowV3 {
    pub schema: String,
    pub created_at_unix_ms: u128,
    pub passed: bool,
    pub provenance: String,
    pub trajectory_case_id: String,
    pub launch_profile: String,
    pub evidence_scope: ConstructedTrajectoryWorkflowScopeV3,
    pub trajectory_report: ComparisonFileIdentityV1,
    pub trajectory_plan: ComparisonFileIdentityV1,
    pub scenario_report: ComparisonFileIdentityV1,
    pub comparison_attempt: ComparisonFileIdentityV1,
    pub lifecycle_receipt: ComparisonFileIdentityV1,
    pub session_id: String,
    pub process_id: u32,
    pub isolation_id: String,
    pub root_frame_id: u64,
    pub final_frame_id: u64,
    pub steps: Vec<ConstructedTrajectoryWorkflowStepV1>,
    pub lifecycle_total_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedTrajectoryWorkflowScopeV3 {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub constructed_root_verified: bool,
    pub original_game_trajectory_captured: bool,
    pub simulator_comparison: bool,
    pub all_steps_constructed_and_matched: bool,
    pub narrow_mechanics_parity: bool,
    pub managed_lifecycle_verified: bool,
    pub strict_runtime_profile: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedTrajectoryWorkflowStepV1 {
    pub index: usize,
    pub transition_id: String,
    pub from_frame_id: u64,
    pub to_frame_id: u64,
    pub comparison_kind: String,
    pub action_receipt: ComparisonFileIdentityV1,
    pub comparison_report: ComparisonFileIdentityV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedTrajectoryComparisonAttemptV3 {
    pub schema: String,
    pub created_at_unix_ms: u128,
    pub completed_at_unix_ms: u128,
    pub passed: bool,
    pub outcome: ConstructedTrajectoryComparisonOutcomeV3,
    pub failure: Option<ConstructedTrajectoryComparisonFailureV3>,
    pub provenance: Option<String>,
    pub trajectory_case_id: Option<String>,
    pub launch_profile: Option<String>,
    pub launch_plan_sha256: Option<String>,
    pub evidence_scope: ConstructedTrajectoryComparisonScopeV3,
    pub trajectory_report: Option<ComparisonFileIdentityV1>,
    pub trajectory_plan: Option<ComparisonFileIdentityV1>,
    pub scenario_report: Option<ComparisonFileIdentityV1>,
    pub session_id: Option<String>,
    pub connection_id: Option<String>,
    pub process_id: Option<u32>,
    pub isolation_id: Option<String>,
    pub root_frame_id: Option<u64>,
    pub final_frame_id: Option<u64>,
    pub planned_steps: Option<usize>,
    pub captured_steps: usize,
    pub comparison_selection: ConstructedTrajectoryComparisonSelectionV1,
    pub requested_step_indices: Vec<usize>,
    pub completed_steps: usize,
    pub steps: Vec<ConstructedTrajectoryWorkflowStepV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstructedTrajectoryComparisonSelectionV1 {
    AllCapturedSteps,
    SelectedStepIndices,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedTrajectoryComparisonScopeV3 {
    pub natural_reachability: bool,
    pub strategy_evidence: bool,
    pub training_eligible: bool,
    pub constructed_root_verified: bool,
    pub original_game_trajectory_captured: bool,
    pub simulator_comparison_attempted: bool,
    pub selected_steps_constructed_and_matched: bool,
    pub all_steps_constructed_and_matched: bool,
    pub narrow_mechanics_parity: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstructedTrajectoryComparisonOutcomeV3 {
    Pending,
    Matched,
    RejectedEvidence,
    TypedDivergence,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructedTrajectoryComparisonFailureV3 {
    pub stage: ConstructedTrajectoryComparisonFailureStageV3,
    pub step_index: Option<usize>,
    pub transition_id: Option<String>,
    pub cause: ConstructedTrajectoryComparisonFailureCauseV3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConstructedTrajectoryComparisonFailureCauseV3 {
    Pipeline {
        message: String,
    },
    Comparator {
        action_receipt: ComparisonFileIdentityV1,
        failure_artifact: ComparisonFileIdentityV1,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstructedTrajectoryComparisonFailureStageV3 {
    ReadCapture,
    DecodeCapture,
    ValidateCapture,
    BindTrajectoryPlan,
    BindScenarioRoot,
    BindLaunch,
    ValidateStepSelector,
    ValidateStepReceipt,
    CompareTransition,
    PersistComparison,
    ValidateComparison,
}

pub fn recompare_constructed_trajectory_new(
    trajectory_report_path: &Path,
    comparison_directory: &Path,
    comparison_report_path: &Path,
) -> Result<ConstructedTrajectoryComparisonAttemptV3, String> {
    if comparison_directory.exists() || comparison_report_path.exists() {
        return Err(
            "trajectory comparison directory and report must both be create-new".to_owned(),
        );
    }
    fs::create_dir(comparison_directory).map_err(|error| {
        format!(
            "failed to create trajectory comparison directory '{}': {error}",
            comparison_directory.display()
        )
    })?;
    let attempt =
        compare_constructed_trajectory_attempt(trajectory_report_path, comparison_directory, None)?;
    write_json_report_new(comparison_report_path, &attempt)?;
    if attempt.passed {
        Ok(attempt)
    } else {
        Err(format!(
            "constructed trajectory comparison did not match; durable attempt={}",
            comparison_report_path.display()
        ))
    }
}

pub fn recompare_constructed_trajectory_steps_new(
    trajectory_report_path: &Path,
    comparison_directory: &Path,
    comparison_report_path: &Path,
    requested_step_indices: &[usize],
) -> Result<ConstructedTrajectoryComparisonAttemptV3, String> {
    if comparison_directory.exists() || comparison_report_path.exists() {
        return Err(
            "trajectory comparison directory and report must both be create-new".to_owned(),
        );
    }
    fs::create_dir(comparison_directory).map_err(|error| {
        format!(
            "failed to create trajectory comparison directory '{}': {error}",
            comparison_directory.display()
        )
    })?;
    let attempt = compare_constructed_trajectory_attempt(
        trajectory_report_path,
        comparison_directory,
        Some(requested_step_indices),
    )?;
    write_json_report_new(comparison_report_path, &attempt)?;
    if attempt.passed {
        Ok(attempt)
    } else {
        Err(format!(
            "selected trajectory transitions did not match; durable attempt={}",
            comparison_report_path.display()
        ))
    }
}

pub fn verify_constructed_trajectory_new(
    trajectory_report_path: &Path,
    lifecycle_receipt_path: &Path,
    comparison_directory: &Path,
    workflow_path: &Path,
) -> Result<ConstructedTrajectoryWorkflowV3, String> {
    if comparison_directory.exists() || workflow_path.exists() {
        return Err(
            "trajectory comparison directory and workflow must both be create-new".to_owned(),
        );
    }
    let attempt_path = comparison_directory.join("trajectory-comparison-attempt.json");
    let comparison = recompare_constructed_trajectory_new(
        trajectory_report_path,
        comparison_directory,
        &attempt_path,
    )?;
    let launch_profile = comparison
        .launch_profile
        .as_deref()
        .ok_or_else(|| "passing trajectory attempt omitted launch profile".to_owned())?;
    let session_id = comparison
        .session_id
        .as_deref()
        .ok_or_else(|| "passing trajectory attempt omitted session ID".to_owned())?;
    let isolation_id = comparison
        .isolation_id
        .as_deref()
        .ok_or_else(|| "passing trajectory attempt omitted isolation ID".to_owned())?;
    let launch_plan_sha256 = comparison
        .launch_plan_sha256
        .as_deref()
        .ok_or_else(|| "passing trajectory attempt omitted launch plan identity".to_owned())?;
    let trajectory_identity = comparison
        .trajectory_report
        .as_ref()
        .ok_or_else(|| "passing trajectory attempt omitted capture identity".to_owned())?;
    let lifecycle = verify_isolated_lifecycle(
        lifecycle_receipt_path,
        IsolatedLifecycleExpectation {
            case_kind: "constructed_combat_trajectory",
            profile: launch_profile,
            control_profile: "parity_scenario_control",
            session_id,
            isolation_id,
            launch_plan_sha256,
            case_evidence: trajectory_identity,
        },
    )?;
    let workflow = ConstructedTrajectoryWorkflowV3 {
        schema: TRAJECTORY_WORKFLOW_SCHEMA.to_owned(),
        created_at_unix_ms: current_unix_ms()?,
        passed: true,
        provenance: "constructed_validated".to_owned(),
        trajectory_case_id: comparison
            .trajectory_case_id
            .clone()
            .ok_or_else(|| "passing trajectory attempt omitted case ID".to_owned())?,
        launch_profile: launch_profile.to_owned(),
        evidence_scope: ConstructedTrajectoryWorkflowScopeV3 {
            natural_reachability: false,
            strategy_evidence: false,
            training_eligible: false,
            constructed_root_verified: true,
            original_game_trajectory_captured: true,
            simulator_comparison: true,
            all_steps_constructed_and_matched: true,
            narrow_mechanics_parity: true,
            managed_lifecycle_verified: true,
            strict_runtime_profile: launch_profile == "strict",
        },
        trajectory_report: trajectory_identity.clone(),
        trajectory_plan: comparison
            .trajectory_plan
            .clone()
            .ok_or_else(|| "passing trajectory attempt omitted plan identity".to_owned())?,
        scenario_report: comparison
            .scenario_report
            .clone()
            .ok_or_else(|| "passing trajectory attempt omitted root identity".to_owned())?,
        comparison_attempt: file_identity(&attempt_path)?,
        lifecycle_receipt: lifecycle.identity,
        session_id: session_id.to_owned(),
        process_id: comparison
            .process_id
            .ok_or_else(|| "passing trajectory attempt omitted process ID".to_owned())?,
        isolation_id: isolation_id.to_owned(),
        root_frame_id: comparison
            .root_frame_id
            .ok_or_else(|| "passing trajectory attempt omitted root frame".to_owned())?,
        final_frame_id: comparison
            .final_frame_id
            .ok_or_else(|| "passing trajectory attempt omitted final frame".to_owned())?,
        steps: comparison.steps,
        lifecycle_total_ms: lifecycle.total_ms,
    };
    write_json_report_new(workflow_path, &workflow)?;
    Ok(workflow)
}

fn compare_constructed_trajectory_attempt(
    trajectory_report_path: &Path,
    comparison_directory: &Path,
    requested_step_indices: Option<&[usize]>,
) -> Result<ConstructedTrajectoryComparisonAttemptV3, String> {
    let created_at_unix_ms = current_unix_ms()?;
    let mut attempt = new_comparison_attempt(created_at_unix_ms);

    let trajectory_bytes = match fs::read(trajectory_report_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            record_attempt_failure(
                &mut attempt,
                ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                ConstructedTrajectoryComparisonFailureStageV3::ReadCapture,
                None,
                None,
                format!(
                    "failed to read constructed trajectory report '{}': {error}",
                    trajectory_report_path.display()
                ),
            );
            attempt.completed_at_unix_ms = current_unix_ms()?;
            return Ok(attempt);
        }
    };
    attempt.trajectory_report =
        match file_identity_from_bytes(trajectory_report_path, &trajectory_bytes) {
            Ok(identity) => Some(identity),
            Err(error) => {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::ReadCapture,
                    None,
                    None,
                    error,
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
    let trajectory: ConstructedScenarioTrajectoryReport =
        match serde_json::from_slice(&trajectory_bytes) {
            Ok(trajectory) => trajectory,
            Err(error) => {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::DecodeCapture,
                    None,
                    None,
                    format!("failed to decode constructed trajectory report: {error}"),
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
    attempt.provenance = Some(trajectory.provenance.clone());
    attempt.trajectory_case_id = Some(trajectory.trajectory_case_id.clone());
    attempt.session_id = Some(trajectory.session.session_id.clone());
    attempt.connection_id = Some(trajectory.session.connection_id.clone());
    attempt.process_id = Some(trajectory.session.process_id);
    attempt.root_frame_id = Some(trajectory.root_frame_id);
    attempt.planned_steps = Some(trajectory.planned_steps);
    attempt.captured_steps = trajectory.completed_steps;
    if let Err(error) = validate_trajectory_capture(&trajectory) {
        record_attempt_failure(
            &mut attempt,
            ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
            ConstructedTrajectoryComparisonFailureStageV3::ValidateCapture,
            trajectory.failed_step_index,
            None,
            error,
        );
        attempt.completed_at_unix_ms = current_unix_ms()?;
        return Ok(attempt);
    }
    let requested = match requested_step_indices {
        Some(indices) => {
            if indices.is_empty() {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::ValidateCapture,
                    None,
                    None,
                    "selected trajectory comparison requires at least one step index".to_owned(),
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
            let unique = indices.iter().copied().collect::<BTreeSet<_>>();
            if unique.len() != indices.len()
                || indices.windows(2).any(|pair| pair[0] >= pair[1])
                || unique.iter().any(|index| *index >= trajectory.steps.len())
            {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::ValidateCapture,
                    None,
                    None,
                    "selected trajectory step indices must be unique, ascending, and captured"
                        .to_owned(),
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
            attempt.comparison_selection =
                ConstructedTrajectoryComparisonSelectionV1::SelectedStepIndices;
            indices.to_vec()
        }
        None => (0..trajectory.steps.len()).collect(),
    };
    attempt.requested_step_indices = requested.clone();
    attempt.evidence_scope.original_game_trajectory_captured = true;
    attempt.final_frame_id = trajectory
        .steps
        .last()
        .and_then(|step| step.successor.as_ref())
        .map(|successor| successor.frame_id);

    let trajectory_plan_path = PathBuf::from(&trajectory.trajectory_plan_path);
    let trajectory_plan_bytes = match fs::read(&trajectory_plan_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            record_attempt_failure(
                &mut attempt,
                ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                ConstructedTrajectoryComparisonFailureStageV3::BindTrajectoryPlan,
                None,
                None,
                format!(
                    "failed to read trajectory plan '{}': {error}",
                    trajectory_plan_path.display()
                ),
            );
            attempt.completed_at_unix_ms = current_unix_ms()?;
            return Ok(attempt);
        }
    };
    let trajectory_plan: ScenarioTrajectoryPlan =
        match serde_json::from_slice(&trajectory_plan_bytes) {
            Ok(plan) => plan,
            Err(error) => {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::BindTrajectoryPlan,
                    None,
                    None,
                    format!("failed to decode trajectory plan: {error}"),
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
    let trajectory_plan_identity =
        match file_identity_from_bytes(&trajectory_plan_path, &trajectory_plan_bytes) {
            Ok(identity) => identity,
            Err(error) => {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::BindTrajectoryPlan,
                    None,
                    None,
                    error,
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
    if let Err(error) =
        validate_trajectory_plan_binding(&trajectory, &trajectory_plan, &trajectory_plan_identity)
    {
        record_attempt_failure(
            &mut attempt,
            ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
            ConstructedTrajectoryComparisonFailureStageV3::BindTrajectoryPlan,
            None,
            None,
            error,
        );
        attempt.completed_at_unix_ms = current_unix_ms()?;
        return Ok(attempt);
    }
    attempt.trajectory_plan = Some(trajectory_plan_identity);

    let scenario_report_path = PathBuf::from(&trajectory.scenario_report_path);
    let scenario_report_bytes = match fs::read(&scenario_report_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            record_attempt_failure(
                &mut attempt,
                ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                ConstructedTrajectoryComparisonFailureStageV3::BindScenarioRoot,
                None,
                None,
                format!(
                    "failed to read trajectory root report '{}': {error}",
                    scenario_report_path.display()
                ),
            );
            attempt.completed_at_unix_ms = current_unix_ms()?;
            return Ok(attempt);
        }
    };
    let scenario_report: ConstructedScenarioReport =
        match serde_json::from_slice(&scenario_report_bytes) {
            Ok(report) => report,
            Err(error) => {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::BindScenarioRoot,
                    None,
                    None,
                    format!("failed to decode trajectory root report: {error}"),
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
    let scenario_report_identity =
        match file_identity_from_bytes(&scenario_report_path, &scenario_report_bytes) {
            Ok(identity) => identity,
            Err(error) => {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::BindScenarioRoot,
                    None,
                    None,
                    error,
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
    if let Err(error) = validate_root_binding(
        &trajectory,
        &trajectory_plan,
        &scenario_report,
        &scenario_report_identity,
    ) {
        record_attempt_failure(
            &mut attempt,
            ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
            ConstructedTrajectoryComparisonFailureStageV3::BindScenarioRoot,
            None,
            None,
            error,
        );
        attempt.completed_at_unix_ms = current_unix_ms()?;
        return Ok(attempt);
    }
    attempt.scenario_report = Some(scenario_report_identity);
    attempt.evidence_scope.constructed_root_verified = true;

    let launch = match trajectory.session.launch.as_ref() {
        Some(launch) => launch,
        None => {
            record_attempt_failure(
                &mut attempt,
                ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                ConstructedTrajectoryComparisonFailureStageV3::BindLaunch,
                None,
                None,
                "trajectory session omitted typed launch evidence".to_owned(),
            );
            attempt.completed_at_unix_ms = current_unix_ms()?;
            return Ok(attempt);
        }
    };
    let isolation_id = match launch.isolation_id.as_deref() {
        Some(isolation_id) => isolation_id,
        None => {
            record_attempt_failure(
                &mut attempt,
                ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                ConstructedTrajectoryComparisonFailureStageV3::BindLaunch,
                None,
                None,
                "trajectory launch omitted isolation identity".to_owned(),
            );
            attempt.completed_at_unix_ms = current_unix_ms()?;
            return Ok(attempt);
        }
    };
    attempt.launch_profile = Some(launch.profile.clone());
    attempt.launch_plan_sha256 = Some(launch.plan_sha256.clone());
    attempt.isolation_id = Some(isolation_id.to_owned());
    attempt.steps.reserve(trajectory.steps.len());
    for (expected, captured) in trajectory_plan.steps.iter().zip(&trajectory.steps) {
        if expected != &captured.transition {
            record_attempt_failure(
                &mut attempt,
                ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                ConstructedTrajectoryComparisonFailureStageV3::ValidateStepSelector,
                Some(captured.index),
                Some(captured.transition_id.clone()),
                format!(
                    "trajectory step {} differs from its pre-launch selector",
                    captured.index
                ),
            );
            attempt.completed_at_unix_ms = current_unix_ms()?;
            return Ok(attempt);
        }
        let receipt_path = PathBuf::from(&captured.action.receipt_path);
        let receipt_identity = match file_identity(&receipt_path) {
            Ok(identity) => identity,
            Err(error) => {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::ValidateStepReceipt,
                    Some(captured.index),
                    Some(captured.transition_id.clone()),
                    error,
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
        let receipt_bytes = match fs::read(&receipt_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::ValidateStepReceipt,
                    Some(captured.index),
                    Some(captured.transition_id.clone()),
                    format!(
                        "failed to read trajectory action receipt '{}': {error}",
                        receipt_path.display()
                    ),
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
        let receipt: ActionReceipt = match serde_json::from_slice(&receipt_bytes) {
            Ok(receipt) => receipt,
            Err(error) => {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                    ConstructedTrajectoryComparisonFailureStageV3::ValidateStepReceipt,
                    Some(captured.index),
                    Some(captured.transition_id.clone()),
                    format!("failed to decode trajectory action receipt: {error}"),
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
        if let Err(error) =
            validate_step_receipt(&trajectory, captured, &receipt, &receipt_identity)
        {
            record_attempt_failure(
                &mut attempt,
                ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                ConstructedTrajectoryComparisonFailureStageV3::ValidateStepReceipt,
                Some(captured.index),
                Some(captured.transition_id.clone()),
                error,
            );
            attempt.completed_at_unix_ms = current_unix_ms()?;
            return Ok(attempt);
        }
        if !requested.contains(&captured.index) {
            continue;
        }

        let comparison_path = comparison_directory.join(format!("step-{:02}.json", captured.index));
        attempt.evidence_scope.simulator_comparison_attempted = true;
        let comparison = match compare_live_action_typed(&receipt_path) {
            Ok(comparison) => comparison,
            Err(failure) => {
                let failure_path =
                    comparison_directory.join(format!("step-{:02}-failure.json", captured.index));
                let failure_identity = match persist_comparator_failure_new(&failure_path, &failure)
                {
                    Ok(identity) => identity,
                    Err(error) => {
                        record_attempt_failure(
                            &mut attempt,
                            ConstructedTrajectoryComparisonOutcomeV3::Unknown,
                            ConstructedTrajectoryComparisonFailureStageV3::PersistComparison,
                            Some(captured.index),
                            Some(captured.transition_id.clone()),
                            error,
                        );
                        attempt.completed_at_unix_ms = current_unix_ms()?;
                        return Ok(attempt);
                    }
                };
                let outcome = trajectory_outcome_for_comparison_failure(&failure.kind);
                record_comparator_attempt_failure(
                    &mut attempt,
                    outcome,
                    captured.index,
                    captured.transition_id.clone(),
                    receipt_identity,
                    failure_identity,
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
        if let Err(error) = write_json_report_new(&comparison_path, &comparison) {
            record_attempt_failure(
                &mut attempt,
                ConstructedTrajectoryComparisonOutcomeV3::Unknown,
                ConstructedTrajectoryComparisonFailureStageV3::PersistComparison,
                Some(captured.index),
                Some(captured.transition_id.clone()),
                error,
            );
            attempt.completed_at_unix_ms = current_unix_ms()?;
            return Ok(attempt);
        }
        if comparison.status() != "matched" {
            record_attempt_failure(
                &mut attempt,
                ConstructedTrajectoryComparisonOutcomeV3::TypedDivergence,
                ConstructedTrajectoryComparisonFailureStageV3::ValidateComparison,
                Some(captured.index),
                Some(captured.transition_id.clone()),
                format!(
                    "trajectory step {} returned comparison status '{}'",
                    captured.index,
                    comparison.status()
                ),
            );
            attempt.completed_at_unix_ms = current_unix_ms()?;
            return Ok(attempt);
        }
        if comparison.frame_ids()
            != (
                captured.from_frame_id,
                captured.successor.as_ref().unwrap().frame_id,
            )
            || comparison.source().session_id != trajectory.session.session_id
            || !comparison.evidence_scope().constructed_root_verified
            || !comparison
                .evidence_scope()
                .original_game_transition_captured
            || !comparison.evidence_scope().simulator_comparison
            || !comparison.evidence_scope().narrow_mechanics_parity
            || comparison.evidence_scope().natural_reachability
            || comparison.evidence_scope().training_eligible
        {
            record_attempt_failure(
                &mut attempt,
                ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence,
                ConstructedTrajectoryComparisonFailureStageV3::ValidateComparison,
                Some(captured.index),
                Some(captured.transition_id.clone()),
                format!(
                    "trajectory step {} did not produce a scoped matched comparison",
                    captured.index
                ),
            );
            attempt.completed_at_unix_ms = current_unix_ms()?;
            return Ok(attempt);
        }
        let comparison_identity = match file_identity(&comparison_path) {
            Ok(identity) => identity,
            Err(error) => {
                record_attempt_failure(
                    &mut attempt,
                    ConstructedTrajectoryComparisonOutcomeV3::Unknown,
                    ConstructedTrajectoryComparisonFailureStageV3::PersistComparison,
                    Some(captured.index),
                    Some(captured.transition_id.clone()),
                    error,
                );
                attempt.completed_at_unix_ms = current_unix_ms()?;
                return Ok(attempt);
            }
        };
        attempt.steps.push(ConstructedTrajectoryWorkflowStepV1 {
            index: captured.index,
            transition_id: captured.transition_id.clone(),
            from_frame_id: captured.from_frame_id,
            to_frame_id: captured.successor.as_ref().unwrap().frame_id,
            comparison_kind: comparison.transition_kind().to_owned(),
            action_receipt: receipt_identity,
            comparison_report: comparison_identity,
        });
        attempt.completed_steps = attempt.steps.len();
    }

    attempt.passed = true;
    attempt.outcome = ConstructedTrajectoryComparisonOutcomeV3::Matched;
    attempt.failure = None;
    attempt
        .evidence_scope
        .selected_steps_constructed_and_matched = true;
    attempt.evidence_scope.all_steps_constructed_and_matched =
        requested.len() == trajectory.steps.len();
    attempt.evidence_scope.narrow_mechanics_parity = true;
    attempt.completed_at_unix_ms = current_unix_ms()?;
    Ok(attempt)
}

fn new_comparison_attempt(created_at_unix_ms: u128) -> ConstructedTrajectoryComparisonAttemptV3 {
    ConstructedTrajectoryComparisonAttemptV3 {
        schema: TRAJECTORY_ATTEMPT_SCHEMA.to_owned(),
        created_at_unix_ms,
        completed_at_unix_ms: created_at_unix_ms,
        passed: false,
        outcome: ConstructedTrajectoryComparisonOutcomeV3::Pending,
        failure: None,
        provenance: None,
        trajectory_case_id: None,
        launch_profile: None,
        launch_plan_sha256: None,
        evidence_scope: ConstructedTrajectoryComparisonScopeV3 {
            natural_reachability: false,
            strategy_evidence: false,
            training_eligible: false,
            constructed_root_verified: false,
            original_game_trajectory_captured: false,
            simulator_comparison_attempted: false,
            selected_steps_constructed_and_matched: false,
            all_steps_constructed_and_matched: false,
            narrow_mechanics_parity: false,
        },
        trajectory_report: None,
        trajectory_plan: None,
        scenario_report: None,
        session_id: None,
        connection_id: None,
        process_id: None,
        isolation_id: None,
        root_frame_id: None,
        final_frame_id: None,
        planned_steps: None,
        captured_steps: 0,
        comparison_selection: ConstructedTrajectoryComparisonSelectionV1::AllCapturedSteps,
        requested_step_indices: Vec::new(),
        completed_steps: 0,
        steps: Vec::new(),
    }
}

fn record_attempt_failure(
    attempt: &mut ConstructedTrajectoryComparisonAttemptV3,
    outcome: ConstructedTrajectoryComparisonOutcomeV3,
    stage: ConstructedTrajectoryComparisonFailureStageV3,
    step_index: Option<usize>,
    transition_id: Option<String>,
    message: String,
) {
    attempt.passed = false;
    attempt.outcome = outcome;
    attempt.failure = Some(ConstructedTrajectoryComparisonFailureV3 {
        stage,
        step_index,
        transition_id,
        cause: ConstructedTrajectoryComparisonFailureCauseV3::Pipeline { message },
    });
    attempt
        .evidence_scope
        .selected_steps_constructed_and_matched = false;
    attempt.evidence_scope.all_steps_constructed_and_matched = false;
    attempt.evidence_scope.narrow_mechanics_parity = false;
}

fn record_comparator_attempt_failure(
    attempt: &mut ConstructedTrajectoryComparisonAttemptV3,
    outcome: ConstructedTrajectoryComparisonOutcomeV3,
    step_index: usize,
    transition_id: String,
    action_receipt: ComparisonFileIdentityV1,
    failure_artifact: ComparisonFileIdentityV1,
) {
    attempt.passed = false;
    attempt.outcome = outcome;
    attempt.failure = Some(ConstructedTrajectoryComparisonFailureV3 {
        stage: ConstructedTrajectoryComparisonFailureStageV3::CompareTransition,
        step_index: Some(step_index),
        transition_id: Some(transition_id),
        cause: ConstructedTrajectoryComparisonFailureCauseV3::Comparator {
            action_receipt,
            failure_artifact,
        },
    });
    attempt
        .evidence_scope
        .selected_steps_constructed_and_matched = false;
    attempt.evidence_scope.all_steps_constructed_and_matched = false;
    attempt.evidence_scope.narrow_mechanics_parity = false;
}

fn persist_comparator_failure_new(
    path: &Path,
    failure: &JavaParityLiveComparisonFailureV1,
) -> Result<ComparisonFileIdentityV1, String> {
    write_json_report_new(path, failure)?;
    file_identity(path)
}

fn trajectory_outcome_for_comparison_failure(
    kind: &JavaParityLiveComparisonFailureKindV1,
) -> ConstructedTrajectoryComparisonOutcomeV3 {
    match kind {
        JavaParityLiveComparisonFailureKindV1::EvidenceRejected => {
            ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence
        }
        JavaParityLiveComparisonFailureKindV1::MechanicsDivergence => {
            ConstructedTrajectoryComparisonOutcomeV3::TypedDivergence
        }
        JavaParityLiveComparisonFailureKindV1::Unsupported
        | JavaParityLiveComparisonFailureKindV1::Unknown => {
            ConstructedTrajectoryComparisonOutcomeV3::Unknown
        }
    }
}

fn validate_trajectory_capture(
    trajectory: &ConstructedScenarioTrajectoryReport,
) -> Result<(), String> {
    if trajectory.schema != TRAJECTORY_REPORT_SCHEMA
        || trajectory.provenance != "constructed_validated"
        || !trajectory.passed
        || trajectory.failure.is_some()
        || trajectory.failed_step_index.is_some()
        || trajectory.planned_steps < 2
        || trajectory.planned_steps != trajectory.completed_steps
        || trajectory.planned_steps != trajectory.steps.len()
        || trajectory.evidence_scope.natural_reachability
        || trajectory.evidence_scope.strategy_evidence
        || trajectory.evidence_scope.training_eligible
        || !trajectory.evidence_scope.constructed_root_verified
        || !trajectory.evidence_scope.original_game_trajectory_captured
        || trajectory.evidence_scope.simulator_comparison
    {
        return Err("trajectory workflow requires one complete capture-only report".to_owned());
    }
    let mut expected_frame = trajectory.root_frame_id;
    for (index, step) in trajectory.steps.iter().enumerate() {
        let successor = step
            .successor
            .as_ref()
            .ok_or_else(|| format!("trajectory step {index} omitted its successor"))?;
        if step.index != index
            || step.transition_id != step.transition.id()
            || step.from_frame_id != expected_frame
            || successor.frame_id != expected_frame + 1
            || !step.adjacent_stable_successor_verified
            || !step.constructed_context_preserved
            || successor.scenario_context["scenario_id"]
                != trajectory.steps[0]
                    .successor
                    .as_ref()
                    .unwrap()
                    .scenario_context["scenario_id"]
        {
            return Err(format!(
                "trajectory step {index} does not form one contiguous constructed chain"
            ));
        }
        expected_frame = successor.frame_id;
    }
    Ok(())
}

fn validate_trajectory_plan_binding(
    trajectory: &ConstructedScenarioTrajectoryReport,
    plan: &ScenarioTrajectoryPlan,
    identity: &ComparisonFileIdentityV1,
) -> Result<(), String> {
    if plan.schema != TRAJECTORY_PLAN_SCHEMA
        || plan.provenance != "constructed_validated"
        || plan.created_at_unix_ms == 0
        || plan.trajectory_case_id != trajectory.trajectory_case_id
        || plan.steps.len() != trajectory.steps.len()
        || identity.sha256 != trajectory.trajectory_plan_sha256
        || !same_path(
            Path::new(&identity.path),
            Path::new(&trajectory.trajectory_plan_path),
        )
    {
        return Err("trajectory capture disagrees with its immutable plan".to_owned());
    }
    let draft_path = PathBuf::from(&plan.trajectory_draft_path);
    let draft_identity = file_identity(&draft_path)?;
    let draft: ScenarioTrajectoryDraft = read_json(&draft_path)?;
    if draft.schema != TRAJECTORY_DRAFT_SCHEMA
        || draft.provenance != "constructed_validated"
        || draft.trajectory_case_id != plan.trajectory_case_id
        || draft.scenario_case_id != plan.scenario_case_id
        || draft.steps != plan.steps
        || draft_identity.sha256 != plan.trajectory_draft_sha256
    {
        return Err("trajectory plan disagrees with its bound draft".to_owned());
    }
    let scenario_plan_path = PathBuf::from(&plan.scenario_plan_path);
    let scenario_plan_identity = file_identity(&scenario_plan_path)?;
    let scenario_plan: ScenarioPlan = read_json(&scenario_plan_path)?;
    if scenario_plan.recipe.case_id != plan.scenario_case_id
        || scenario_plan_identity.sha256 != plan.scenario_plan_sha256
    {
        return Err("trajectory plan disagrees with its bound scenario plan".to_owned());
    }
    Ok(())
}

fn validate_root_binding(
    trajectory: &ConstructedScenarioTrajectoryReport,
    plan: &ScenarioTrajectoryPlan,
    root_report: &ConstructedScenarioReport,
    identity: &ComparisonFileIdentityV1,
) -> Result<(), String> {
    let root = root_report
        .root
        .as_ref()
        .ok_or_else(|| "trajectory root report omitted its stable root".to_owned())?;
    let scenario_plan: ScenarioPlan = read_json(Path::new(&plan.scenario_plan_path))?;
    let expected_context = serde_json::to_value(&scenario_plan.recipe)
        .map_err(|error| format!("failed to encode bound scenario recipe: {error}"))?;
    if !root_report.passed
        || root_report.provenance != "constructed_validated"
        || root_report.session.session_id != trajectory.session.session_id
        || root_report.session.connection_id != trajectory.session.connection_id
        || root_report.session.process_id != trajectory.session.process_id
        || root.frame_id != trajectory.root_frame_id
        || root.scenario_context["case_id"].as_str() != Some(plan.scenario_case_id.as_str())
        || root.scenario_context != expected_context
        || trajectory.steps.iter().any(|step| {
            step.successor
                .as_ref()
                .is_none_or(|successor| successor.scenario_context != root.scenario_context)
        })
        || identity.sha256 != trajectory.scenario_report_sha256
        || !same_path(
            Path::new(&identity.path),
            Path::new(&trajectory.scenario_report_path),
        )
    {
        return Err("trajectory capture disagrees with its verified root report".to_owned());
    }
    Ok(())
}

fn validate_step_receipt(
    trajectory: &ConstructedScenarioTrajectoryReport,
    step: &sts_java_parity_live::ScenarioTrajectoryStepReport,
    receipt: &ActionReceipt,
    identity: &ComparisonFileIdentityV1,
) -> Result<(), String> {
    let successor = step.successor.as_ref().unwrap();
    let successor_identity = file_identity(Path::new(&successor.frame_path))?;
    if receipt.status != "completed"
        || receipt.session_id != trajectory.session.session_id
        || receipt.connection_id != trajectory.session.connection_id
        || receipt.request_id != step.action.request_id
        || receipt.action_id != step.action.action_id
        || receipt.based_on_frame_id != step.from_frame_id
        || receipt.successor_frame_id != Some(successor.frame_id)
        || receipt.parameters != step.action.parameters
        || identity.sha256 != step.action.receipt_sha256
        || !same_path(
            Path::new(&identity.path),
            Path::new(&step.action.receipt_path),
        )
        || step.action.status != "completed"
        || step.action.successor_frame_id != Some(successor.frame_id)
        || step.action.successor_frame_sha256.as_deref() != Some(successor.frame_sha256.as_str())
        || step.action.successor_frame_path.as_deref() != Some(successor.frame_path.as_str())
        || receipt.successor_frame_path.as_deref() != Some(successor.frame_path.as_str())
        || successor_identity.sha256 != successor.frame_sha256
    {
        return Err(format!(
            "trajectory step {} receipt identity is inconsistent",
            step.index
        ));
    }
    Ok(())
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read '{}': {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode '{}': {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_java_parity_live::{
        LaunchEvidence, ScenarioProbeTransition, ScenarioStepProof,
        ScenarioTrajectoryEvidenceScope, ScenarioTrajectoryFrameProof,
        ScenarioTrajectoryStepReport, SessionManifest,
    };

    fn captured_trajectory() -> ConstructedScenarioTrajectoryReport {
        let scenario_context = serde_json::json!({"scenario_id": "scenario"});
        let step = |index: usize, from: u64| ScenarioTrajectoryStepReport {
            index,
            transition_id: "combat.end_turn.v1".to_owned(),
            transition: ScenarioProbeTransition::CombatEndTurn,
            from_frame_id: from,
            action: ScenarioStepProof {
                name: format!("trajectory_step_{index}"),
                request_id: format!("request-{index}"),
                action_id: "end_turn".to_owned(),
                based_on_frame_id: from,
                successor_frame_id: Some(from + 1),
                status: "completed".to_owned(),
                settlement_wall_time_ms: Some(1),
                parameters: serde_json::json!({}),
                receipt_path: format!("receipt-{index}.json"),
                receipt_sha256: format!("receipt-sha-{index}"),
                successor_frame_path: Some(format!("frame-{}.json", from + 1)),
                successor_frame_sha256: Some(format!("frame-sha-{}", from + 1)),
            },
            successor: Some(ScenarioTrajectoryFrameProof {
                frame_id: from + 1,
                frame_path: format!("frame-{}.json", from + 1),
                frame_sha256: format!("frame-sha-{}", from + 1),
                boundary: "combat_player_decision".to_owned(),
                scenario_context: scenario_context.clone(),
            }),
            adjacent_stable_successor_verified: true,
            constructed_context_preserved: true,
        };
        ConstructedScenarioTrajectoryReport {
            schema: TRAJECTORY_REPORT_SCHEMA.to_owned(),
            provenance: "constructed_validated".to_owned(),
            evidence_scope: ScenarioTrajectoryEvidenceScope {
                natural_reachability: false,
                strategy_evidence: false,
                training_eligible: false,
                constructed_root_verified: true,
                original_game_trajectory_captured: true,
                simulator_comparison: false,
            },
            created_at_unix_ms: 1,
            completed_at_unix_ms: 2,
            passed: true,
            failure: None,
            failed_step_index: None,
            trajectory_case_id: "two-steps".to_owned(),
            trajectory_plan_path: "trajectory-plan.json".to_owned(),
            trajectory_plan_sha256: "trajectory-plan-sha".to_owned(),
            scenario_report_path: "scenario-report.json".to_owned(),
            scenario_report_sha256: "scenario-report-sha".to_owned(),
            session: SessionManifest {
                schema: "sts_java_parity_live_session_v3".to_owned(),
                session_id: "session".to_owned(),
                created_at_unix_ms: 1,
                process_id: 7,
                session_dir: "session-dir".to_owned(),
                capture_id: "capture".to_owned(),
                producer_session_id: "producer".to_owned(),
                connection_id: "connection".to_owned(),
                profile: "parity_scenario_control".to_owned(),
                source_commit: "commit".to_owned(),
                source_dirty: false,
                loaded_mod_jar_sha256: "mod".to_owned(),
                sts_jar_sha256: "sts".to_owned(),
                mod_the_spire_jar_sha256: "mts".to_owned(),
                launch: Some(LaunchEvidence {
                    profile: "accelerated".to_owned(),
                    control_profile: "parity_scenario_control".to_owned(),
                    plan_path: "launch-plan.json".to_owned(),
                    plan_sha256: "launch-plan-sha".to_owned(),
                    storage_class: "isolated_working_directory_v1".to_owned(),
                    working_directory: "sandbox".to_owned(),
                    source_game_directory: "game".to_owned(),
                    profile_substrate: "canonical_tutorial_complete_preferences_v1".to_owned(),
                    tutorial_policy: "precompleted_via_profile_v1".to_owned(),
                    isolation_id: Some("isolation".to_owned()),
                }),
            },
            root_frame_id: 4,
            planned_steps: 2,
            completed_steps: 2,
            steps: vec![step(0, 4), step(1, 5)],
        }
    }

    #[test]
    fn trajectory_capture_validator_rejects_partial_or_training_labeled_chains() {
        let capture = captured_trajectory();
        validate_trajectory_capture(&capture).unwrap();

        let mut partial = capture.clone();
        partial.completed_steps = 1;
        assert!(validate_trajectory_capture(&partial).is_err());

        let mut training = capture.clone();
        training.evidence_scope.training_eligible = true;
        assert!(validate_trajectory_capture(&training).is_err());

        let mut non_adjacent = capture;
        non_adjacent.steps[1].from_frame_id = 9;
        assert!(validate_trajectory_capture(&non_adjacent).is_err());
    }

    #[test]
    fn missing_capture_publishes_a_typed_rejection_attempt() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "trajectory-attempt-rejection-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let attempt_path = root.join("attempt.json");
        let error = recompare_constructed_trajectory_new(
            &root.join("missing-capture.json"),
            &root.join("steps"),
            &attempt_path,
        )
        .unwrap_err();
        assert!(error.contains("durable attempt="));
        let attempt: ConstructedTrajectoryComparisonAttemptV3 = read_json(&attempt_path).unwrap();
        assert_eq!(
            attempt.schema,
            "sts_java_parity_constructed_trajectory_comparison_attempt_v4"
        );
        assert!(!attempt.passed);
        assert_eq!(
            attempt.outcome,
            ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence
        );
        assert_eq!(attempt.completed_steps, 0);
        assert!(!attempt.evidence_scope.simulator_comparison_attempted);
        let failure = attempt.failure.unwrap();
        assert_eq!(
            failure.stage,
            ConstructedTrajectoryComparisonFailureStageV3::ReadCapture
        );
        let ConstructedTrajectoryComparisonFailureCauseV3::Pipeline { message } = failure.cause
        else {
            panic!("missing capture rejection was not owned by the pipeline")
        };
        assert!(message.contains("failed to read constructed trajectory report"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn string_only_comparator_failure_remains_unknown() {
        let mut attempt = new_comparison_attempt(1);
        record_attempt_failure(
            &mut attempt,
            ConstructedTrajectoryComparisonOutcomeV3::Unknown,
            ConstructedTrajectoryComparisonFailureStageV3::CompareTransition,
            Some(1),
            Some("combat.play_card.v1".to_owned()),
            "untyped comparator error".to_owned(),
        );
        assert_eq!(
            attempt.outcome,
            ConstructedTrajectoryComparisonOutcomeV3::Unknown
        );
        assert_eq!(attempt.failure.as_ref().unwrap().step_index, Some(1));
        assert!(!attempt.evidence_scope.narrow_mechanics_parity);
        let json = serde_json::to_value(&attempt).unwrap();
        assert_eq!(json["outcome"], "unknown");
        assert_eq!(json["failure"]["stage"], "compare_transition");
        assert_eq!(json["failure"]["cause"]["kind"], "pipeline");
        assert_eq!(
            json["failure"]["cause"]["message"],
            "untyped comparator error"
        );
    }

    #[test]
    fn comparator_failure_cause_binds_receipt_and_typed_failure_artifact() {
        let receipt = ComparisonFileIdentityV1 {
            path: "receipt.json".to_owned(),
            bytes: 11,
            sha256: "A".repeat(64),
        };
        let failure_artifact = ComparisonFileIdentityV1 {
            path: "step-01-failure.json".to_owned(),
            bytes: 22,
            sha256: "B".repeat(64),
        };
        let mut attempt = new_comparison_attempt(1);

        record_comparator_attempt_failure(
            &mut attempt,
            ConstructedTrajectoryComparisonOutcomeV3::Unknown,
            1,
            "combat.play_card.v1".to_owned(),
            receipt,
            failure_artifact,
        );

        let json = serde_json::to_value(&attempt).unwrap();
        assert_eq!(json["outcome"], "unknown");
        assert_eq!(json["failure"]["stage"], "compare_transition");
        assert_eq!(json["failure"]["cause"]["kind"], "comparator");
        assert_eq!(json["failure"]["cause"]["action_receipt"]["bytes"], 11);
        assert_eq!(
            json["failure"]["cause"]["failure_artifact"]["sha256"],
            "B".repeat(64)
        );
    }

    #[test]
    fn comparator_failure_artifact_is_create_new_and_typed() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "trajectory-comparator-failure-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("step-00-failure.json");
        let failure = JavaParityLiveComparisonFailureV1::new(
            JavaParityLiveComparisonFailureKindV1::Unsupported,
            crate::JavaParityLiveComparisonFailureStageV1::SelectComparator,
            "valid typed action has no comparator".to_owned(),
        );

        let identity = persist_comparator_failure_new(&path, &failure).unwrap();
        let stored: JavaParityLiveComparisonFailureV1 = read_json(&path).unwrap();

        assert_eq!(identity.sha256, file_identity(&path).unwrap().sha256);
        assert_eq!(stored.schema, "sts_java_parity_live_comparison_failure_v1");
        assert_eq!(
            stored.kind,
            JavaParityLiveComparisonFailureKindV1::Unsupported
        );
        assert!(persist_comparator_failure_new(&path, &failure).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trajectory_outcome_uses_comparator_owned_failure_kind() {
        assert_eq!(
            trajectory_outcome_for_comparison_failure(
                &JavaParityLiveComparisonFailureKindV1::EvidenceRejected
            ),
            ConstructedTrajectoryComparisonOutcomeV3::RejectedEvidence
        );
        assert_eq!(
            trajectory_outcome_for_comparison_failure(
                &JavaParityLiveComparisonFailureKindV1::MechanicsDivergence
            ),
            ConstructedTrajectoryComparisonOutcomeV3::TypedDivergence
        );
        for kind in [
            JavaParityLiveComparisonFailureKindV1::Unsupported,
            JavaParityLiveComparisonFailureKindV1::Unknown,
        ] {
            assert_eq!(
                trajectory_outcome_for_comparison_failure(&kind),
                ConstructedTrajectoryComparisonOutcomeV3::Unknown
            );
        }
    }
}
