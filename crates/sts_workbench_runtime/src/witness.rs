use crate::artifact::{inspect_artifact, publish_artifact};
use crate::runtime_manager::{
    create_operation_directory, read_json, run_typed_adapter, JavaRuntimeAccessV1,
    NativeMaterializationV1,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};
use sts_workbench_engine_protocol::{
    EngineWitnessPreviewCompletionV1, EngineWitnessPreviewStatusV1,
    EngineWitnessPreviewStepStatusV1, EngineWitnessPreviewStepV1,
    ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1,
};

const WITNESS_PLAN_SCHEMA: &str = "sts-paired-root-witness-transaction-plan-v1";
const WITNESS_TRANSACTION_SCHEMA: &str = "sts-paired-root-witness-transaction-report-v1";

#[derive(Clone, Debug)]
pub(super) struct WitnessScopeV1 {
    pub handle: String,
    pub initial_root_id: String,
    pub action_space_id: String,
    pub witness_id: String,
    pub intents: Vec<Value>,
}

#[derive(Clone, Debug)]
pub(super) struct WitnessExecutionV1 {
    pub scope: WitnessScopeV1,
    pub status: EngineWitnessPreviewStatusV1,
    pub reached_root_id: String,
    pub stopped_step_ordinal: Option<u64>,
    pub stop_reason: Option<String>,
    pub members: Vec<Value>,
    pub step_details: Vec<Value>,
    pub completion_steps: Vec<EngineWitnessPreviewStepV1>,
    pub autosave_unchanged: bool,
    pub runtime_reused: bool,
    pub runtime_ready_ms: u128,
    pub continue_ms: u128,
    pub transaction_ms: u128,
}

pub(super) fn load_scope(repository: &Path, handle: &str) -> Result<WitnessScopeV1, String> {
    let inspected = inspect_artifact(repository, handle)?;
    if inspected["artifact_type"].as_str() != Some("paired_root_search_witness") {
        return Err("preview requires a search witness artifact".to_owned());
    }
    let document = inspected["document"]
        .as_object()
        .ok_or_else(|| "search witness artifact omitted its document".to_owned())?;
    let witness = document
        .get("witness")
        .and_then(Value::as_object)
        .ok_or_else(|| "search witness artifact omitted its witness".to_owned())?;
    let translation = witness
        .get("translation")
        .and_then(Value::as_object)
        .ok_or_else(|| "search witness artifact omitted its translation".to_owned())?;
    let initial_root_id = required_string(document.get("root_id"), "initial root")?;
    let action_space_id = required_string(document.get("action_space_id"), "action space")?;
    let witness_id = required_string(witness.get("witness_id"), "witness identity")?;
    let intents = translation
        .get("intents")
        .and_then(Value::as_array)
        .ok_or_else(|| "preview requires a complete search witness".to_owned())?;
    let step_count = witness
        .get("plan_step_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| "search witness omitted its step count".to_owned())?;
    if document.get("schema").and_then(Value::as_str) != Some("sts-workbench-search-witness-v1")
        || translation.get("status").and_then(Value::as_str) != Some("complete")
        || !content_id_has_shape(initial_root_id, "state-root-v2")
        || !content_id_has_shape(action_space_id, "paired-action-space-v1")
        || !content_id_has_shape(witness_id, "paired-search-witness-v1")
        || intents.is_empty()
        || intents.len() > 64
        || u64::try_from(intents.len()).ok() != Some(step_count)
        || intents.iter().any(|intent| {
            !intent.is_object()
                || !matches!(intent["kind"].as_str(), Some("play_card" | "end_turn"))
        })
    {
        return Err("preview requires a complete scoped search witness".to_owned());
    }
    Ok(WitnessScopeV1 {
        handle: handle.to_owned(),
        initial_root_id: initial_root_id.to_owned(),
        action_space_id: action_space_id.to_owned(),
        witness_id: witness_id.to_owned(),
        intents: intents.clone(),
    })
}

pub(super) fn execute_from_parked(
    repository: &Path,
    runtime: &mut JavaRuntimeAccessV1<'_>,
    activation_id: &str,
    scope: WitnessScopeV1,
    request_count: u64,
) -> Result<WitnessExecutionV1, String> {
    runtime.validate_owned_session()?;
    let source_session = runtime
        .source_session()
        .ok_or_else(|| "parked witness runtime omitted its paired source session".to_owned())?
        .to_path_buf();
    let attempt = create_operation_directory(repository, "witness-parked", request_count)?;
    let plan_path = attempt.join("witness-plan.json");
    write_json_new(
        &plan_path,
        &json!({
            "schema": WITNESS_PLAN_SCHEMA,
            "witness_id": scope.witness_id,
            "initial_root_id": scope.initial_root_id,
            "intents": scope.intents
        }),
    )?;
    let before_autosave = runtime.autosave_sha256().to_owned();
    let transaction_path = attempt.join("transaction.json");
    let arguments = vec![
        "paired-root-preview-witness".to_owned(),
        "--session".to_owned(),
        path_text(&source_session),
        "--root".to_owned(),
        scope.initial_root_id.clone(),
        "--activation".to_owned(),
        activation_id.to_owned(),
        "--witness-plan".to_owned(),
        path_text(&plan_path),
        "--java-live-output".to_owned(),
        path_text(runtime.live_output()),
        "--store".to_owned(),
        path_text(&repository.join(".oracle-lab").join("paired-root-store")),
        "--attempt".to_owned(),
        path_text(&attempt.join("transaction-work")),
        "--report".to_owned(),
        path_text(&transaction_path),
    ];
    let transaction_started = Instant::now();
    run_typed_adapter(
        repository,
        &attempt,
        "preview-witness-parked",
        &arguments,
        Duration::from_secs(600),
    )?;
    let transaction_ms = transaction_started.elapsed().as_millis();
    runtime.validate_transition_identity()?;
    let transaction: Value = read_json(&transaction_path)?;
    let projected = project_transaction(&scope, &transaction)?;
    let autosave_unchanged =
        runtime.current_autosave_sha256_optional()?.as_deref() == Some(before_autosave.as_str());
    Ok(WitnessExecutionV1 {
        scope,
        status: projected.status,
        reached_root_id: projected.reached_root_id,
        stopped_step_ordinal: projected.stopped_step_ordinal,
        stop_reason: projected.stop_reason,
        members: projected.members,
        step_details: projected.step_details,
        completion_steps: projected.completion_steps,
        autosave_unchanged,
        runtime_reused: true,
        runtime_ready_ms: 0,
        continue_ms: 0,
        transaction_ms,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_from_neutral(
    repository: &Path,
    runtime: &mut JavaRuntimeAccessV1<'_>,
    materialization: &NativeMaterializationV1,
    scope: WitnessScopeV1,
    request_count: u64,
    runtime_ready_ms: u128,
    runtime_reused: bool,
) -> Result<WitnessExecutionV1, String> {
    let attempt = create_operation_directory(repository, "witness", request_count)?;
    let plan_path = attempt.join("witness-plan.json");
    write_json_new(
        &plan_path,
        &json!({
            "schema": WITNESS_PLAN_SCHEMA,
            "witness_id": scope.witness_id,
            "initial_root_id": scope.initial_root_id,
            "intents": scope.intents
        }),
    )?;
    let before_autosave = runtime.autosave_sha256().to_owned();
    let operation_started = Instant::now();
    runtime.restore_native(materialization)?;
    let continue_ms = operation_started.elapsed().as_millis();
    let activation_path = attempt.join("activation.json");
    let recovery_path = attempt.join("recovery.json");
    let transaction_path = attempt.join("transaction.json");
    let arguments = vec![
        "paired-root-resume-witness".to_owned(),
        "--target-root".to_owned(),
        scope.initial_root_id.clone(),
        "--witness-plan".to_owned(),
        path_text(&plan_path),
        "--run-root".to_owned(),
        path_text(&runtime.run_root()),
        "--java-live-output".to_owned(),
        path_text(runtime.live_output()),
        "--store".to_owned(),
        path_text(&repository.join(".oracle-lab").join("paired-root-store")),
        "--recovery-attempt".to_owned(),
        path_text(&attempt.join("recovery-work")),
        "--transaction-attempt".to_owned(),
        path_text(&attempt.join("transaction-work")),
        "--activation-report".to_owned(),
        path_text(&activation_path),
        "--recovery-report".to_owned(),
        path_text(&recovery_path),
        "--transaction-report".to_owned(),
        path_text(&transaction_path),
    ];
    let transaction_started = Instant::now();
    run_typed_adapter(
        repository,
        &attempt,
        "resume-witness",
        &arguments,
        Duration::from_secs(600),
    )?;
    let transaction_ms = transaction_started.elapsed().as_millis();
    runtime.validate_transition_identity()?;
    let activation: Value = read_json(&activation_path)?;
    let recovery: Value = read_json(&recovery_path)?;
    validate_activation_recovery(
        &activation,
        &recovery,
        materialization,
        &scope.initial_root_id,
    )?;
    let source_session = activation["source_session"]
        .as_str()
        .ok_or_else(|| "witness activation omitted its source session".to_owned())?;
    runtime.bind_source_session(Path::new(source_session).to_path_buf())?;
    let transaction: Value = read_json(&transaction_path)?;
    let projected = project_transaction(&scope, &transaction)?;
    let autosave_unchanged =
        runtime.current_autosave_sha256_optional()?.as_deref() == Some(before_autosave.as_str());
    Ok(WitnessExecutionV1 {
        scope,
        status: projected.status,
        reached_root_id: projected.reached_root_id,
        stopped_step_ordinal: projected.stopped_step_ordinal,
        stop_reason: projected.stop_reason,
        members: projected.members,
        step_details: projected.step_details,
        completion_steps: projected.completion_steps,
        autosave_unchanged,
        runtime_reused,
        runtime_ready_ms,
        continue_ms,
        transaction_ms,
    })
}

pub(super) struct PublishedWitnessPreviewV1 {
    pub response: Value,
    pub completion: EngineWitnessPreviewCompletionV1,
}

pub(super) fn publish_response(
    repository: &Path,
    execution: WitnessExecutionV1,
    reset_ms: u128,
    total_ms: u128,
) -> Result<PublishedWitnessPreviewV1, String> {
    let timings = json!({
        "runtime_ready": bounded_millis(execution.runtime_ready_ms),
        "continue": bounded_millis(execution.continue_ms),
        "transaction": bounded_millis(execution.transaction_ms),
        "reset": bounded_millis(reset_ms),
        "total": bounded_millis(total_ms)
    });
    let detail = json!({
        "schema": "sts-workbench-witness-preview-detail-v1",
        "mode": "sequential_witness",
        "status": execution.status.token(),
        "witness_id": execution.scope.witness_id,
        "witness_handle": execution.scope.handle,
        "initial_root_id": execution.scope.initial_root_id,
        "reached_root_id": execution.reached_root_id,
        "stopped_step_ordinal": execution.stopped_step_ordinal,
        "stop_reason": execution.stop_reason,
        "steps": execution.step_details,
        "runtime_reused": execution.runtime_reused,
        "autosave_unchanged": execution.autosave_unchanged,
        "source_head_unchanged": true,
        "timings_ms": timings
    });
    let detail_handle = publish_artifact(repository, "paired_root_preview_detail", &detail)?;
    let completion = EngineWitnessPreviewCompletionV1 {
        schema: ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1.to_owned(),
        root_id: execution.scope.initial_root_id.clone(),
        action_space_id: execution.scope.action_space_id.clone(),
        witness_handle: execution.scope.handle.clone(),
        reached_root_id: execution.reached_root_id.clone(),
        status: execution.status,
        steps: execution.completion_steps.clone(),
        stopped_step_ordinal: execution.stopped_step_ordinal,
        stop_reason: execution.stop_reason.clone(),
        detail_handle: detail_handle.clone(),
    };
    let response = json!({
        "schema": "sts-workbench-preview-response-v2",
        "mode": "sequential_witness",
        "status": execution.status.token(),
        "root_id": execution.scope.initial_root_id,
        "action_space_id": execution.scope.action_space_id,
        "members": execution.members,
        "witness_id": execution.scope.witness_id,
        "witness_handle": execution.scope.handle,
        "reached_root_id": execution.reached_root_id,
        "stopped_step_ordinal": execution.stopped_step_ordinal,
        "stop_reason": execution.stop_reason,
        "runtime_reused": execution.runtime_reused,
        "autosave_unchanged": execution.autosave_unchanged,
        "source_head_unchanged": true,
        "detail_handle": detail_handle,
        "timings_ms": timings
    });
    Ok(PublishedWitnessPreviewV1 {
        response,
        completion,
    })
}

struct ProjectedTransactionV1 {
    status: EngineWitnessPreviewStatusV1,
    reached_root_id: String,
    stopped_step_ordinal: Option<u64>,
    stop_reason: Option<String>,
    members: Vec<Value>,
    step_details: Vec<Value>,
    completion_steps: Vec<EngineWitnessPreviewStepV1>,
}

fn project_transaction(
    scope: &WitnessScopeV1,
    transaction: &Value,
) -> Result<ProjectedTransactionV1, String> {
    let object = transaction
        .as_object()
        .ok_or_else(|| "witness transaction is not a JSON object".to_owned())?;
    let status = match required_string(object.get("status"), "transaction status")? {
        "ready" => EngineWitnessPreviewStatusV1::Ready,
        "diverged" => EngineWitnessPreviewStatusV1::Diverged,
        "intent_unavailable" => EngineWitnessPreviewStatusV1::IntentUnavailable,
        "successor_boundary_unsupported" => {
            EngineWitnessPreviewStatusV1::SuccessorBoundaryUnsupported
        }
        _ => return Err("witness transaction has an invalid status".to_owned()),
    };
    let reached_root_id = required_string(object.get("reached_root_id"), "reached root")?;
    let stopped_step_ordinal = optional_u64(object.get("stopped_step_ordinal"), "stopped step")?;
    let stop_reason = optional_string(object.get("stop_reason"), "stop reason")?;
    let steps = object
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| "witness transaction omitted its steps".to_owned())?;
    if object.get("schema").and_then(Value::as_str) != Some(WITNESS_TRANSACTION_SCHEMA)
        || object.get("witness_id").and_then(Value::as_str) != Some(&scope.witness_id)
        || object.get("initial_root_id").and_then(Value::as_str) != Some(&scope.initial_root_id)
        || !content_id_has_shape(reached_root_id, "state-root-v2")
        || transaction["source_head_guard"]["unchanged"].as_bool() != Some(true)
        || steps.is_empty()
        || steps.len() > scope.intents.len()
        || status == EngineWitnessPreviewStatusV1::Ready && steps.len() != scope.intents.len()
    {
        return Err("paired-root witness transaction is not witness-bound".to_owned());
    }
    let mut current_root = scope.initial_root_id.clone();
    let mut members = Vec::new();
    let mut details = Vec::new();
    let mut completion_steps = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let step_object = step
            .as_object()
            .ok_or_else(|| "witness step is not an object".to_owned())?;
        let resolution = step_object
            .get("resolution")
            .and_then(Value::as_object)
            .ok_or_else(|| "witness step omitted its resolution".to_owned())?;
        let index_u64 = u64::try_from(index).map_err(|_| "witness step overflow".to_owned())?;
        if step_object.get("step_ordinal").and_then(Value::as_u64) != Some(index_u64)
            || step_object.get("source_root_id").and_then(Value::as_str)
                != Some(current_root.as_str())
            || resolution.get("schema").and_then(Value::as_str)
                != Some("sts-paired-root-search-intent-resolution-v1")
            || resolution.get("root_id").and_then(Value::as_str) != Some(current_root.as_str())
            || !resolution
                .get("action_space_id")
                .and_then(Value::as_str)
                .is_some_and(|value| content_id_has_shape(value, "paired-action-space-v1"))
            || resolution.get("witness_id").and_then(Value::as_str)
                != Some(scope.witness_id.as_str())
            || resolution.get("step_ordinal").and_then(Value::as_u64) != Some(index_u64)
            || resolution.get("intent") != Some(&scope.intents[index])
        {
            return Err("paired-root witness step resolution is not scoped".to_owned());
        }
        let resolution_status = required_string(resolution.get("status"), "resolution status")?;
        if resolution_status == "unavailable" {
            if step_object
                .get("preview")
                .is_some_and(|value| !value.is_null())
                || resolution
                    .get("action_handle_id")
                    .is_some_and(|value| !value.is_null())
                || resolution
                    .get("reason")
                    .and_then(Value::as_str)
                    .is_none_or(|reason| reason.trim().is_empty())
                || status != EngineWitnessPreviewStatusV1::IntentUnavailable
                || stopped_step_ordinal != Some(index_u64)
            {
                return Err("unavailable witness intent has inconsistent evidence".to_owned());
            }
            details.push(json!({
                "step_ordinal": index,
                "source_root_id": current_root,
                "intent": scope.intents[index],
                "resolution": resolution
            }));
            continue;
        }
        if resolution_status != "ready" {
            return Err("witness intent resolution has an invalid status".to_owned());
        }
        if resolution
            .get("reason")
            .is_some_and(|reason| !reason.is_null())
        {
            return Err("ready witness intent unexpectedly reported a gap reason".to_owned());
        }
        let action_id = required_string(resolution.get("action_handle_id"), "action handle")?;
        let preview = step_object
            .get("preview")
            .and_then(Value::as_object)
            .ok_or_else(|| "witness step omitted its preview".to_owned())?;
        let preview_status = required_string(preview.get("status"), "preview status")?;
        let completion_status = match preview_status {
            "ready" => EngineWitnessPreviewStepStatusV1::Ready,
            "diverged" => EngineWitnessPreviewStepStatusV1::Diverged,
            "successor_boundary_unsupported" => {
                EngineWitnessPreviewStepStatusV1::SuccessorBoundaryUnsupported
            }
            _ => return Err("witness preview has an invalid status".to_owned()),
        };
        let components = preview
            .get("components")
            .and_then(Value::as_array)
            .ok_or_else(|| "witness preview omitted typed comparisons".to_owned())?;
        if !content_id_has_shape(action_id, "paired-action-v1")
            || preview.get("schema").and_then(Value::as_str)
                != Some("sts-paired-root-preview-report-v1")
            || preview.get("source_root_id").and_then(Value::as_str) != Some(current_root.as_str())
            || preview["action"]["handle_id"].as_str() != Some(action_id)
            || preview["source_head_guard"]["unchanged"].as_bool() != Some(true)
            || !matches!(
                preview_status,
                "ready" | "diverged" | "successor_boundary_unsupported"
            )
        {
            return Err("paired-root witness step omitted its scoped preview".to_owned());
        }
        let boundary_unsupported = preview_status == "successor_boundary_unsupported";
        let mut domains = BTreeSet::new();
        if (!boundary_unsupported && components.is_empty())
            || (boundary_unsupported
                && (!components.is_empty()
                    || preview["successor_boundary"]["stable"].as_bool() != Some(true)))
            || components.iter().any(|component| {
                let Some(component) = component.as_object() else {
                    return true;
                };
                let Some(domain) = component
                    .get("domain")
                    .and_then(Value::as_str)
                    .filter(|domain| !domain.trim().is_empty())
                else {
                    return true;
                };
                !domains.insert(domain)
                    || component
                        .get("schema")
                        .and_then(Value::as_str)
                        .is_none_or(|schema| schema.trim().is_empty())
                    || !matches!(
                        component.get("status").and_then(Value::as_str),
                        Some("exact" | "diverged" | "unknown")
                    )
            })
        {
            return Err("paired-root witness step omitted typed comparisons".to_owned());
        }
        let exact_count = components
            .iter()
            .filter(|component| component["status"].as_str() == Some("exact"))
            .count();
        completion_steps.push(EngineWitnessPreviewStepV1 {
            status: completion_status,
            exact_count: u64::try_from(exact_count)
                .map_err(|_| "witness Exact count overflow".to_owned())?,
            total_count: u64::try_from(components.len())
                .map_err(|_| "witness comparison count overflow".to_owned())?,
        });
        let member = json!({
            "action_handle_id": action_id,
            "status": preview_status,
            "successor_root_id": preview.get("successor_root_id"),
            "receipt_id": preview.get("receipt_id"),
            "divergence_id": preview.get("divergence_id"),
            "successor_boundary": preview.get("successor_boundary"),
            "parity": {"exact_count": exact_count, "total_count": components.len()}
        });
        details.push(json!({
            "step_ordinal": index,
            "source_root_id": current_root,
            "intent": scope.intents[index],
            "resolution": resolution,
            "result": member,
            "comparison": {
                "exact_count": exact_count,
                "total_count": components.len(),
                "components": components
            }
        }));
        if preview_status == "ready" {
            if exact_count != components.len() || components.is_empty() {
                return Err("ready witness step is not Exact".to_owned());
            }
            current_root =
                required_string(preview.get("successor_root_id"), "successor root")?.to_owned();
            if !content_id_has_shape(&current_root, "state-root-v2")
                || !preview
                    .get("successor_activation_id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| content_id_has_shape(value, "state-root-activation-v2"))
                || !preview
                    .get("receipt_id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| {
                        content_id_has_shape(value, "paired-transition-receipt-v1")
                    })
                || !is_nullish(preview.get("divergence_id"))
                || !is_nullish(preview.get("successor_boundary"))
            {
                return Err("Exact witness step omitted its successor receipt".to_owned());
            }
        } else {
            let typed_settlement = match preview_status {
                "diverged" => {
                    components
                        .iter()
                        .any(|component| component["status"].as_str() == Some("diverged"))
                        && is_nullish(preview.get("successor_root_id"))
                        && is_nullish(preview.get("successor_activation_id"))
                        && is_nullish(preview.get("receipt_id"))
                        && preview
                            .get("divergence_id")
                            .and_then(Value::as_str)
                            .is_some_and(|value| {
                                content_id_has_shape(value, "paired-divergence-v1")
                            })
                        && is_nullish(preview.get("successor_boundary"))
                }
                "successor_boundary_unsupported" => {
                    is_nullish(preview.get("successor_root_id"))
                        && is_nullish(preview.get("successor_activation_id"))
                        && is_nullish(preview.get("receipt_id"))
                        && is_nullish(preview.get("divergence_id"))
                }
                _ => false,
            };
            if !typed_settlement || stopped_step_ordinal != Some(index_u64) {
                return Err("non-Exact witness step did not stop with typed evidence".to_owned());
            }
        }
        members.push(member);
    }
    if current_root != reached_root_id {
        return Err("paired-root witness reached-root chain is inconsistent".to_owned());
    }
    if status == EngineWitnessPreviewStatusV1::Ready {
        if stopped_step_ordinal.is_some()
            || stop_reason.is_some()
            || members.iter().any(|member| member["status"] != "ready")
        {
            return Err("ready witness transaction contains a stopped step".to_owned());
        }
    } else {
        let terminal_ordinal = u64::try_from(steps.len() - 1).ok();
        let has_reason = stop_reason
            .as_deref()
            .is_some_and(|reason| !reason.trim().is_empty());
        let terminal_matches = if status == EngineWitnessPreviewStatusV1::IntentUnavailable {
            steps
                .last()
                .and_then(|step| step["resolution"]["status"].as_str())
                == Some("unavailable")
        } else {
            members.last().and_then(|member| member["status"].as_str()) == Some(status.token())
        };
        if stopped_step_ordinal != terminal_ordinal || !has_reason || !terminal_matches {
            return Err("stopped witness transaction has no terminal evidence".to_owned());
        }
    }
    Ok(ProjectedTransactionV1 {
        status,
        reached_root_id: reached_root_id.to_owned(),
        stopped_step_ordinal,
        stop_reason,
        members,
        step_details: details,
        completion_steps,
    })
}

fn validate_activation_recovery(
    activation: &Value,
    recovery: &Value,
    materialization: &NativeMaterializationV1,
    root_id: &str,
) -> Result<(), String> {
    let native_activation_id = activation["native_activation_id"]
        .as_str()
        .unwrap_or_default();
    let activation_components = activation["components"].as_array();
    let version_compatibility = activation["version_compatibility"].as_array();
    let activation_guard = &activation["source_head_guard"];
    let recovery_guard = &recovery["source_head_guard"];
    if activation["schema"].as_str() != Some("sts-paired-root-native-activation-report-v1")
        || activation["requested_target_root_id"].as_str() != Some(root_id)
        || activation["native_root_id"].as_str() != Some(&materialization.native_root_id)
        || !content_id_has_shape(native_activation_id, "state-root-activation-v2")
        || activation_components.is_none_or(|components| {
            components.is_empty()
                || components.iter().any(|component| {
                    component["schema"]
                        .as_str()
                        .is_none_or(|schema| schema.trim().is_empty())
                        || component["status"].as_str() != Some("exact")
                })
        })
        || version_compatibility.is_none_or(|items| {
            items.is_empty()
                || items.iter().any(|item| {
                    !matches!(
                        item["status"].as_str(),
                        Some("exact" | "producer_changed_semantics_compatible")
                    )
                })
        })
        || !unchanged_head_guard(activation_guard)
        || recovery["schema"].as_str() != Some("sts-paired-root-recovery-report-v1")
        || recovery["status"].as_str() != Some("ready")
        || recovery["native_root_id"].as_str() != Some(&materialization.native_root_id)
        || recovery["native_activation_id"].as_str() != Some(native_activation_id)
        || recovery["target_root_id"].as_str() != Some(root_id)
        || !recovery["target_activation_id"]
            .as_str()
            .is_some_and(|value| content_id_has_shape(value, "state-root-activation-v2"))
        || !is_nullish(recovery.get("divergence_id"))
        || !unchanged_head_guard(recovery_guard)
    {
        return Err("witness activation/recovery did not reach its Exact target".to_owned());
    }
    Ok(())
}

fn write_json_new(path: &Path, value: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode witness plan: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create witness plan: {error}"))?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("failed to persist witness plan: {error}"))
}

fn required_string<'a>(value: Option<&'a Value>, label: &str) -> Result<&'a str, String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("witness evidence omitted {label}"))
}

fn optional_string(value: Option<&Value>, label: &str) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("witness evidence has an invalid {label}")),
    }
}

fn optional_u64(value: Option<&Value>, label: &str) -> Result<Option<u64>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("witness evidence has an invalid {label}")),
    }
}

fn is_nullish(value: Option<&Value>) -> bool {
    value.is_none_or(Value::is_null)
}

fn unchanged_head_guard(value: &Value) -> bool {
    value["unchanged"].as_bool() == Some(true)
        && value["before_sha256"].as_str().is_some_and(is_sha256)
        && value["after_sha256"].as_str() == value["before_sha256"].as_str()
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
