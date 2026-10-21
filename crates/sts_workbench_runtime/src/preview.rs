use crate::artifact::publish_artifact;
use crate::runtime_manager::{
    create_preview_directory, read_json, run_typed_adapter, JavaRuntimeAccessV1,
    NativeMaterializationV1, TypedAdapterFailureV1,
};
use crate::RootBindingV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use sts_paired_root::{
    BlobRefV1, ComparisonStatusV1, PairedActionHandleV1, ProjectionDomainV1, StateRootStoreV2,
    TransitionTimingV1, TypedDifferenceV1,
};
pub(super) use sts_workbench_engine_protocol::{
    EnginePreviewMemberStatusV1 as PreviewMemberStatusV1,
    EnginePreviewMemberSummaryV1 as PreviewMemberSummaryV1,
};

const PREVIEW_REPORT_SCHEMA: &str = "sts-paired-root-preview-report-v1";
const ACTIVATION_REPORT_SCHEMA: &str = "sts-paired-root-native-activation-report-v1";
const RECOVERY_REPORT_SCHEMA: &str = "sts-paired-root-recovery-report-v1";
const PREVIEW_RESPONSE_SCHEMA: &str = "sts-workbench-preview-response-v2";
const PREVIEW_ARTIFACT_TYPE: &str = "paired_root_preview_detail";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreviewFailureV1 {
    pub code: String,
    pub detail: String,
}

impl PreviewFailureV1 {
    fn generic(detail: impl Into<String>) -> Self {
        Self {
            code: "preview_execution_failed".to_owned(),
            detail: detail.into(),
        }
    }
}

impl From<String> for PreviewFailureV1 {
    fn from(detail: String) -> Self {
        Self::generic(detail)
    }
}

impl From<TypedAdapterFailureV1> for PreviewFailureV1 {
    fn from(failure: TypedAdapterFailureV1) -> Self {
        Self {
            code: failure.code,
            detail: failure.detail,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ParkedSuccessorV1 {
    pub root: RootBindingV1,
}

#[derive(Clone, Debug)]
pub(super) struct PreviewOutcomeV1 {
    pub response: Value,
    pub member: Value,
    pub detail_member: Value,
    pub summary: PreviewMemberSummaryV1,
    pub detail_handle: String,
    pub parked: Option<ParkedSuccessorV1>,
    pub status: PreviewSettlementV1,
    pub autosave_unchanged: bool,
    pub timings: PreviewTimingsV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PreviewSettlementV1 {
    Ready,
    Diverged,
    SuccessorBoundaryUnsupported,
}

impl PreviewSettlementV1 {
    pub(super) fn token(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Diverged => "diverged",
            Self::SuccessorBoundaryUnsupported => "successor_boundary_unsupported",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct AggregatedPreviewV1 {
    pub response: Value,
    pub detail_handle: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PreviewTimingsV1 {
    pub runtime_ready_ms: u128,
    pub preview_ms: u128,
    pub rust_execution_ms: u128,
    pub java_execution_ms: u128,
    pub comparison_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceHeadGuardV1 {
    before_sha256: String,
    after_sha256: String,
    file_count: usize,
    unchanged: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectionSummaryV1 {
    domain: ProjectionDomainV1,
    schema: String,
    status: ComparisonStatusV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    first_difference: Option<TypedDifferenceV1>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SuccessorBoundaryV1 {
    kind: String,
    stable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    screen_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    room_phase: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PreviewReportV1 {
    schema: String,
    status: PreviewSettlementV1,
    source_root_id: String,
    source_activation_id: String,
    action: PairedActionHandleV1,
    components: Vec<ProjectionSummaryV1>,
    source_head_guard: SourceHeadGuardV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    successor_root_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    successor_activation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    divergence_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    successor_boundary: Option<SuccessorBoundaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    java_evidence: Option<BlobRefV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rust_evidence: Option<BlobRefV1>,
    timings: TransitionTimingV1,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionCompatibilityV1 {
    component: String,
    stored_identity: String,
    active_identity: String,
    enforcement: String,
    status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeActivationReportV1 {
    schema: String,
    requested_target_root_id: String,
    native_root_id: String,
    native_activation_id: String,
    source_session: String,
    version_compatibility: Vec<VersionCompatibilityV1>,
    components: Vec<ProjectionSummaryV1>,
    source_head_guard: SourceHeadGuardV1,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryReportV1 {
    schema: String,
    status: String,
    native_root_id: String,
    native_activation_id: String,
    target_root_id: String,
    #[serde(default)]
    target_activation_id: Option<String>,
    steps: Vec<Value>,
    source_head_guard: SourceHeadGuardV1,
    #[serde(default)]
    divergence_id: Option<String>,
    timings: TransitionTimingV1,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn preview_from_neutral(
    repository: &Path,
    runtime: &mut JavaRuntimeAccessV1<'_>,
    materialization: &NativeMaterializationV1,
    root_id: &str,
    action_id: &str,
    expected_action_space_id: &str,
    request_count: u64,
    runtime_ready_ms: u128,
    runtime_reused: bool,
) -> Result<PreviewOutcomeV1, PreviewFailureV1> {
    let attempt = create_preview_directory(repository, request_count)?;
    let before_autosave = runtime.autosave_sha256().to_owned();
    let started = Instant::now();
    runtime.restore_native(materialization)?;
    let activation_path = attempt.join("activation.json");
    let recovery_path = attempt.join("recovery.json");
    let preview_path = attempt.join("preview.json");
    let arguments = vec![
        "paired-root-resume-preview".to_owned(),
        "--target-root".to_owned(),
        root_id.to_owned(),
        "--action".to_owned(),
        action_id.to_owned(),
        "--run-root".to_owned(),
        path_text(&runtime.run_root()),
        "--java-live-output".to_owned(),
        path_text(runtime.live_output()),
        "--store".to_owned(),
        path_text(&repository.join(".oracle-lab").join("paired-root-store")),
        "--recovery-attempt".to_owned(),
        path_text(&attempt.join("recovery-work")),
        "--preview-attempt".to_owned(),
        path_text(&attempt.join("preview-work")),
        "--activation-report".to_owned(),
        path_text(&activation_path),
        "--recovery-report".to_owned(),
        path_text(&recovery_path),
        "--preview-report".to_owned(),
        path_text(&preview_path),
    ];
    run_typed_adapter(
        repository,
        &attempt,
        "resume-preview",
        &arguments,
        Duration::from_secs(180),
    )?;
    let activation: NativeActivationReportV1 = read_json(&activation_path)?;
    let recovery: RecoveryReportV1 = read_json(&recovery_path)?;
    validate_cold_reports(&activation, &recovery, materialization, root_id)?;
    runtime.bind_source_session(PathBuf::from(&activation.source_session))?;
    let report: PreviewReportV1 = read_json(&preview_path)?;
    if recovery.target_activation_id.as_deref() != Some(report.source_activation_id.as_str()) {
        return Err(
            "cold preview did not execute from the recovered target activation"
                .to_owned()
                .into(),
        );
    }
    finalize_preview(
        repository,
        runtime,
        root_id,
        action_id,
        expected_action_space_id,
        &before_autosave,
        report,
        runtime_reused,
        runtime_ready_ms,
        started.elapsed().as_millis(),
    )
    .map_err(PreviewFailureV1::from)
}

pub(super) fn preview_from_parked(
    repository: &Path,
    runtime: &mut JavaRuntimeAccessV1<'_>,
    root_id: &str,
    activation_id: &str,
    action_id: &str,
    expected_action_space_id: &str,
    request_count: u64,
) -> Result<PreviewOutcomeV1, PreviewFailureV1> {
    runtime.validate_owned_session()?;
    let source_session = runtime
        .source_session()
        .ok_or_else(|| "parked runtime omitted its paired source session".to_owned())?
        .to_path_buf();
    let attempt = create_preview_directory(repository, request_count)?;
    let preview_path = attempt.join("preview.json");
    let before_autosave = runtime.autosave_sha256().to_owned();
    let started = Instant::now();
    let arguments = vec![
        "paired-root-preview-action".to_owned(),
        "--session".to_owned(),
        path_text(&source_session),
        "--root".to_owned(),
        root_id.to_owned(),
        "--activation".to_owned(),
        activation_id.to_owned(),
        "--action".to_owned(),
        action_id.to_owned(),
        "--java-live-output".to_owned(),
        path_text(runtime.live_output()),
        "--store".to_owned(),
        path_text(&repository.join(".oracle-lab").join("paired-root-store")),
        "--attempt".to_owned(),
        path_text(&attempt.join("preview-work")),
        "--report".to_owned(),
        path_text(&preview_path),
    ];
    run_typed_adapter(
        repository,
        &attempt,
        "parked-preview",
        &arguments,
        Duration::from_secs(120),
    )?;
    let report: PreviewReportV1 = read_json(&preview_path)?;
    if report.source_activation_id != activation_id {
        return Err(
            "parked preview returned evidence for a different activation"
                .to_owned()
                .into(),
        );
    }
    finalize_preview(
        repository,
        runtime,
        root_id,
        action_id,
        expected_action_space_id,
        &before_autosave,
        report,
        true,
        0,
        started.elapsed().as_millis(),
    )
    .map_err(PreviewFailureV1::from)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn aggregate_previews(
    repository: &Path,
    root_id: &str,
    expected_action_space_id: &str,
    requested_action_ids: &[String],
    outcomes: &[PreviewOutcomeV1],
    settlements: &[String],
    reset_count: usize,
    cold_runtime_count: usize,
    reset_ms: u128,
    total_ms: u128,
) -> Result<AggregatedPreviewV1, String> {
    if requested_action_ids.len() < 2
        || outcomes.is_empty()
        || outcomes.len() != settlements.len()
        || outcomes.len() > requested_action_ids.len()
    {
        return Err("aggregate preview received an invalid member prefix".to_owned());
    }
    let observed_action_ids = outcomes
        .iter()
        .map(|outcome| {
            outcome.member["action_handle_id"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "preview member omitted its action identity".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if observed_action_ids != requested_action_ids[..outcomes.len()] {
        return Err("aggregate preview members do not match the requested prefix".to_owned());
    }
    let status = outcomes
        .iter()
        .find(|outcome| outcome.status != PreviewSettlementV1::Ready)
        .map_or(PreviewSettlementV1::Ready, |outcome| outcome.status);
    if status == PreviewSettlementV1::Ready && outcomes.len() != requested_action_ids.len() {
        return Err("ready aggregate preview omitted requested members".to_owned());
    }
    let members = outcomes
        .iter()
        .map(|outcome| outcome.member.clone())
        .collect::<Vec<_>>();
    let detail_members = outcomes
        .iter()
        .zip(settlements)
        .map(|(outcome, settlement)| {
            let mut member = outcome.detail_member.clone();
            let object = member
                .as_object_mut()
                .expect("validated preview detail member is an object");
            object.insert(
                "runtime_settlement".to_owned(),
                Value::String(settlement.clone()),
            );
            member
        })
        .collect::<Vec<_>>();
    let runtime_ready_ms = outcomes
        .iter()
        .map(|outcome| outcome.timings.runtime_ready_ms)
        .sum::<u128>();
    let preview_ms = outcomes
        .iter()
        .map(|outcome| outcome.timings.preview_ms)
        .sum::<u128>();
    let rust_execution_ms = outcomes
        .iter()
        .map(|outcome| outcome.timings.rust_execution_ms)
        .sum::<u128>();
    let java_execution_ms = outcomes
        .iter()
        .map(|outcome| outcome.timings.java_execution_ms)
        .sum::<u128>();
    let comparison_ms = outcomes
        .iter()
        .map(|outcome| outcome.timings.comparison_ms)
        .sum::<u128>();
    let timings = json!({
        "runtime_ready": bounded_millis(runtime_ready_ms),
        "preview": bounded_millis(preview_ms),
        "reset": bounded_millis(reset_ms),
        "rust_execution": bounded_millis(rust_execution_ms),
        "java_execution": bounded_millis(java_execution_ms),
        "comparison": bounded_millis(comparison_ms),
        "total": bounded_millis(total_ms)
    });
    let runtime_reused = cold_runtime_count == 0;
    let autosave_unchanged = outcomes.iter().all(|outcome| outcome.autosave_unchanged);
    let detail = json!({
        "schema": "sts-workbench-preview-detail-v1",
        "status": status,
        "stop_reason": null,
        "target_root_id": root_id,
        "action_space_id": expected_action_space_id,
        "requested_action_ids": requested_action_ids,
        "members": detail_members,
        "runtime_reused": runtime_reused,
        "autosave_unchanged": autosave_unchanged,
        "source_head_unchanged": true,
        "timings_ms": timings,
        "runtime_policy": "same_root_independent_actions_v1",
        "runtime_summary": {
            "reset_count": reset_count,
            "cold_runtime_count": cold_runtime_count,
            "completed_member_count": outcomes.len()
        }
    });
    let detail_handle = publish_artifact(repository, PREVIEW_ARTIFACT_TYPE, &detail)?;
    let response = json!({
        "schema": PREVIEW_RESPONSE_SCHEMA,
        "mode": "independent_actions",
        "status": status,
        "root_id": root_id,
        "action_space_id": expected_action_space_id,
        "members": members,
        "witness_id": null,
        "witness_handle": null,
        "reached_root_id": null,
        "stopped_step_ordinal": null,
        "stop_reason": null,
        "runtime_reused": runtime_reused,
        "autosave_unchanged": autosave_unchanged,
        "source_head_unchanged": true,
        "detail_handle": detail_handle,
        "timings_ms": timings
    });
    Ok(AggregatedPreviewV1 {
        response,
        detail_handle,
    })
}

#[allow(clippy::too_many_arguments)]
fn finalize_preview(
    repository: &Path,
    runtime: &mut JavaRuntimeAccessV1<'_>,
    root_id: &str,
    action_id: &str,
    expected_action_space_id: &str,
    before_autosave: &str,
    report: PreviewReportV1,
    runtime_reused: bool,
    runtime_ready_ms: u128,
    preview_wall_ms: u128,
) -> Result<PreviewOutcomeV1, String> {
    validate_preview_report(&report, root_id, action_id, expected_action_space_id)?;
    let current_autosave = runtime.current_autosave_sha256_optional()?;
    let autosave_unchanged = current_autosave.as_deref() == Some(before_autosave);
    let exact_count = report
        .components
        .iter()
        .filter(|component| component.status == ComparisonStatusV1::Exact)
        .count();
    let total_count = report.components.len();
    let parked = if report.status == PreviewSettlementV1::Ready {
        let successor_root_id = report
            .successor_root_id
            .as_ref()
            .expect("validated ready successor");
        let successor_activation_id = report
            .successor_activation_id
            .as_ref()
            .expect("validated ready activation");
        let store = StateRootStoreV2::new(repository.join(".oracle-lab").join("paired-root-store"))
            .with_operation_verification_cache();
        store.initialize()?;
        let activation = store.load_activation(successor_activation_id)?;
        if activation.activation.root_id != *successor_root_id
            || activation.activation.java_runtime_epoch != runtime.runtime_epoch()
            || !content_id_has_shape(
                &activation.activation.action_space_id,
                "paired-action-space-v1",
            )
        {
            return Err("Exact child activation is not bound to the managed runtime".to_owned());
        }
        runtime.accept_successor_autosave()?;
        Some(ParkedSuccessorV1 {
            root: RootBindingV1 {
                root_id: successor_root_id.clone(),
                activation_id: successor_activation_id.clone(),
                action_space_id: activation.activation.action_space_id,
                source_head_sha256: report.source_head_guard.after_sha256.clone(),
            },
        })
    } else {
        None
    };
    let member = json!({
        "action_handle_id": report.action.handle_id,
        "status": report.status,
        "successor_root_id": report.successor_root_id,
        "receipt_id": report.receipt_id,
        "divergence_id": report.divergence_id,
        "successor_boundary": report.successor_boundary,
        "parity": {
            "exact_count": exact_count,
            "total_count": total_count
        }
    });
    let typed_timings = PreviewTimingsV1 {
        runtime_ready_ms,
        preview_ms: preview_wall_ms,
        rust_execution_ms: report.timings.rust_execution_ms,
        java_execution_ms: report.timings.java_execution_ms,
        comparison_ms: report.timings.comparison_ms,
    };
    let timings = json!({
        "runtime_ready": bounded_millis(typed_timings.runtime_ready_ms),
        "preview": bounded_millis(typed_timings.preview_ms),
        "rust_execution": bounded_millis(typed_timings.rust_execution_ms),
        "java_execution": bounded_millis(typed_timings.java_execution_ms),
        "comparison": bounded_millis(typed_timings.comparison_ms),
        "total": bounded_millis(runtime_ready_ms.saturating_add(preview_wall_ms))
    });
    let detail_member = json!({
        "action_handle_id": report.action.handle_id,
        "status": report.status,
        "successor_root_id": report.successor_root_id,
        "successor_activation_id": report.successor_activation_id,
        "receipt_id": report.receipt_id,
        "divergence_id": report.divergence_id,
        "successor_boundary": report.successor_boundary,
        "comparison": {
            "exact_count": exact_count,
            "total_count": total_count,
            "components": report.components
        },
        "source_activation_id": report.source_activation_id
    });
    let detail = json!({
        "schema": "sts-workbench-preview-detail-v1",
        "status": report.status,
        "stop_reason": null,
        "target_root_id": root_id,
        "action_space_id": expected_action_space_id,
        "requested_action_ids": [action_id],
        "members": [detail_member.clone()],
        "runtime_reused": runtime_reused,
        "autosave_unchanged": autosave_unchanged,
        "source_head_unchanged": true,
        "timings_ms": timings,
        "runtime_policy": "exact_child_park_or_retire"
    });
    let detail_handle = publish_artifact(repository, PREVIEW_ARTIFACT_TYPE, &detail)?;
    let response = json!({
        "schema": PREVIEW_RESPONSE_SCHEMA,
        "mode": "independent_actions",
        "status": report.status,
        "root_id": root_id,
        "action_space_id": expected_action_space_id,
        "members": [member],
        "witness_id": null,
        "witness_handle": null,
        "reached_root_id": null,
        "stopped_step_ordinal": null,
        "stop_reason": null,
        "runtime_reused": runtime_reused,
        "autosave_unchanged": autosave_unchanged,
        "source_head_unchanged": true,
        "detail_handle": detail_handle,
        "timings_ms": timings
    });
    let summary_status = match report.status {
        PreviewSettlementV1::Ready => PreviewMemberStatusV1::Exact {
            receipt_id: report
                .receipt_id
                .clone()
                .expect("validated ready preview has a receipt"),
            successor_root_id: report
                .successor_root_id
                .clone()
                .expect("validated ready preview has a successor"),
        },
        PreviewSettlementV1::Diverged => PreviewMemberStatusV1::Diverged {
            divergence_id: report
                .divergence_id
                .clone()
                .expect("validated divergence has an identity"),
        },
        PreviewSettlementV1::SuccessorBoundaryUnsupported => {
            PreviewMemberStatusV1::SuccessorBoundaryUnsupported {}
        }
    };
    Ok(PreviewOutcomeV1 {
        response,
        member,
        detail_member,
        summary: PreviewMemberSummaryV1 {
            action_id: report.action.handle_id,
            status: summary_status,
            exact_count: u64::try_from(exact_count).unwrap_or(u64::MAX),
            total_count: u64::try_from(total_count).unwrap_or(u64::MAX),
        },
        detail_handle,
        parked,
        status: report.status,
        autosave_unchanged,
        timings: typed_timings,
    })
}

fn validate_cold_reports(
    activation: &NativeActivationReportV1,
    recovery: &RecoveryReportV1,
    materialization: &NativeMaterializationV1,
    target_root_id: &str,
) -> Result<(), String> {
    if activation.schema != ACTIVATION_REPORT_SCHEMA
        || activation.requested_target_root_id != target_root_id
        || activation.native_root_id != materialization.native_root_id
        || !content_id_has_shape(&activation.native_activation_id, "state-root-activation-v2")
        || !activation.source_head_guard.unchanged
        || activation.components.is_empty()
        || activation
            .components
            .iter()
            .any(|component| component.status != ComparisonStatusV1::Exact)
        || activation.version_compatibility.is_empty()
        || activation.version_compatibility.iter().any(|item| {
            item.component.trim().is_empty()
                || item.stored_identity.trim().is_empty()
                || item.active_identity.trim().is_empty()
                || item.enforcement.trim().is_empty()
                || !matches!(
                    item.status.as_str(),
                    "exact" | "producer_changed_semantics_compatible"
                )
        })
    {
        return Err("native activation returned invalid scoped evidence".to_owned());
    }
    if recovery.schema != RECOVERY_REPORT_SCHEMA
        || recovery.status != "ready"
        || recovery.native_root_id != activation.native_root_id
        || recovery.native_activation_id != activation.native_activation_id
        || recovery.target_root_id != target_root_id
        || recovery.target_activation_id.is_none()
        || recovery.divergence_id.is_some()
        || !recovery.source_head_guard.unchanged
    {
        return Err("paired-root recovery did not reach its exact target".to_owned());
    }
    let _ = (&recovery.steps, &recovery.timings);
    Ok(())
}

fn validate_preview_report(
    report: &PreviewReportV1,
    root_id: &str,
    action_id: &str,
    expected_action_space_id: &str,
) -> Result<(), String> {
    if report.schema != PREVIEW_REPORT_SCHEMA
        || report.source_root_id != root_id
        || report.action.handle_id != action_id
        || report.action.source_root_id != root_id
        || report.action.action_space_id != expected_action_space_id
        || !content_id_has_shape(&report.source_activation_id, "state-root-activation-v2")
        || !report.source_head_guard.unchanged
        || report.source_head_guard.before_sha256 != report.source_head_guard.after_sha256
        || !is_sha256(&report.source_head_guard.after_sha256)
    {
        return Err("preview returned evidence for the wrong source scope".to_owned());
    }
    let mut domains = BTreeSet::new();
    if report.components.iter().any(|component| {
        component.schema.trim().is_empty()
            || !domains.insert(component.domain)
            || component.status == ComparisonStatusV1::Unknown
    }) {
        return Err("preview comparison evidence is incomplete or duplicated".to_owned());
    }
    match report.status {
        PreviewSettlementV1::Ready
            if !report.components.is_empty()
                && report
                    .components
                    .iter()
                    .all(|component| component.status == ComparisonStatusV1::Exact)
                && report
                    .successor_root_id
                    .as_deref()
                    .is_some_and(|value| content_id_has_shape(value, "state-root-v2"))
                && report
                    .successor_activation_id
                    .as_deref()
                    .is_some_and(|value| {
                        content_id_has_shape(value, "state-root-activation-v2")
                    })
                && report.receipt_id.as_deref().is_some_and(|value| {
                    content_id_has_shape(value, "paired-transition-receipt-v1")
                })
                && report.divergence_id.is_none()
                && report.successor_boundary.is_none() => {}
        PreviewSettlementV1::Diverged
            if !report.components.is_empty()
                && report
                    .components
                    .iter()
                    .any(|component| component.status == ComparisonStatusV1::Diverged)
                && report.successor_root_id.is_none()
                && report.successor_activation_id.is_none()
                && report.receipt_id.is_none()
                && report
                    .divergence_id
                    .as_deref()
                    .is_some_and(|value| content_id_has_shape(value, "paired-divergence-v1"))
                && report.successor_boundary.is_none() => {}
        PreviewSettlementV1::SuccessorBoundaryUnsupported
            if report.components.is_empty()
                && report.successor_root_id.is_none()
                && report.successor_activation_id.is_none()
                && report.receipt_id.is_none()
                && report.divergence_id.is_none()
                && report
                    .successor_boundary
                    .as_ref()
                    .is_some_and(|boundary| boundary.stable) => {}
        _ => return Err("preview result has an invalid typed settlement".to_owned()),
    }
    Ok(())
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn bounded_millis(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn content_id_has_shape(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|tail| tail.strip_prefix('-'))
        .is_some_and(is_sha256)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_paired_root::{PairedActionV1, ProjectionDomainV1};

    #[test]
    fn typed_adapter_failure_code_crosses_the_preview_boundary() {
        let preview = PreviewFailureV1::from(TypedAdapterFailureV1 {
            code: "persistent_master_deck_diverged".to_owned(),
            detail: "private comparison detail".to_owned(),
        });

        assert_eq!(preview.code, "persistent_master_deck_diverged");
        assert_eq!(preview.detail, "private comparison detail");
    }

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn ready_report() -> PreviewReportV1 {
        PreviewReportV1 {
            schema: PREVIEW_REPORT_SCHEMA.to_owned(),
            status: PreviewSettlementV1::Ready,
            source_root_id: format!("state-root-v2-{}", digest('a')),
            source_activation_id: format!("state-root-activation-v2-{}", digest('b')),
            action: PairedActionHandleV1 {
                handle_id: format!("paired-action-v1-{}", digest('c')),
                source_root_id: format!("state-root-v2-{}", digest('a')),
                action_space_id: format!("paired-action-space-v1-{}", digest('d')),
                action: PairedActionV1::EndTurn,
            },
            components: vec![ProjectionSummaryV1 {
                domain: ProjectionDomainV1::Combat,
                schema: "sts-paired-combat-root-v1".to_owned(),
                status: ComparisonStatusV1::Exact,
                first_difference: None,
            }],
            source_head_guard: SourceHeadGuardV1 {
                before_sha256: digest('e'),
                after_sha256: digest('e'),
                file_count: 2,
                unchanged: true,
            },
            successor_root_id: Some(format!("state-root-v2-{}", digest('f'))),
            successor_activation_id: Some(format!("state-root-activation-v2-{}", digest('1'))),
            receipt_id: Some(format!("paired-transition-receipt-v1-{}", digest('2'))),
            divergence_id: None,
            successor_boundary: None,
            java_evidence: None,
            rust_evidence: None,
            timings: TransitionTimingV1 {
                rust_execution_ms: 1,
                java_execution_ms: 2,
                comparison_ms: 3,
            },
        }
    }

    #[test]
    fn ready_preview_requires_exact_scoped_successor() {
        let report = ready_report();
        validate_preview_report(
            &report,
            &report.source_root_id,
            &report.action.handle_id,
            &report.action.action_space_id,
        )
        .unwrap();

        let mut invalid = report.clone();
        invalid.components[0].status = ComparisonStatusV1::Unknown;
        let error = validate_preview_report(
            &invalid,
            &invalid.source_root_id,
            &invalid.action.handle_id,
            &invalid.action.action_space_id,
        )
        .unwrap_err();
        assert!(error.contains("incomplete"));
    }
}
