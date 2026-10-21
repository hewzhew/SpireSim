mod build_status;
mod engine_state;
mod model_error;
mod native_recovery_validator;
mod preview;
mod production_supervisor;
mod runtime_manager;
mod runtime_ownership;
mod subprocess;
mod witness;

use engine_state::WorkbenchEngineStateV1;
use production_supervisor::ProductionSupervisorV1;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
pub(crate) use sts_workbench_artifact_store as artifact;
use sts_workbench_supervisor::{
    EnginePublicRequestV1, ShellErrorV1, ShellRequestV1, ShellResponseV1, ShellRouteV1,
    SHELL_REQUEST_SCHEMA_V1, SHELL_RESPONSE_SCHEMA_V1,
};

pub const REQUEST_SCHEMA_V1: &str = SHELL_REQUEST_SCHEMA_V1;
pub const RESPONSE_SCHEMA_V1: &str = SHELL_RESPONSE_SCHEMA_V1;
pub const STATUS_SCHEMA_V2: &str = "sts-workbench-runtime-status-v2";
pub(crate) const INTERNAL_NATIVE_RECOVERY_SENTINEL_FAILED_CODE: &str =
    "internal_native_recovery_sentinel_failed";
const ENGINE_SEARCH_IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(35);
const ENGINE_VIEW_IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeStartupReconciliationV1 {
    pub health: String,
    pub reclaimed_storage: usize,
    pub foreign_owned_storage: usize,
    pub unrecognized_markers: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResidentBindingV1 {
    pub resident_id: String,
    pub process_id: u32,
    pub native_root_id: String,
    pub runtime_epoch: String,
    pub autosave_sha256: String,
    pub build_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RootBindingV1 {
    pub root_id: String,
    pub activation_id: String,
    pub action_space_id: String,
    pub source_head_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StableRuntimeV1 {
    Idle,
    Neutral,
    AtRoot { root: RootBindingV1 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeStateV1 {
    stable: StableRuntimeV1,
}

impl Default for RuntimeStateV1 {
    fn default() -> Self {
        Self {
            stable: StableRuntimeV1::Idle,
        }
    }
}

impl RuntimeStateV1 {
    fn park(&mut self, root: RootBindingV1) -> Result<(), String> {
        validate_root(&root)?;
        self.stable = StableRuntimeV1::AtRoot { root };
        Ok(())
    }

    fn reset_to_neutral(&mut self) {
        self.stable = StableRuntimeV1::Neutral;
    }

    fn clear(&mut self) {
        self.stable = StableRuntimeV1::Idle;
    }

    pub fn active_root_id(&self) -> Option<&str> {
        match &self.stable {
            StableRuntimeV1::AtRoot { root } => Some(root.root_id.as_str()),
            _ => None,
        }
    }

    fn is_idle(&self) -> bool {
        self.stable == StableRuntimeV1::Idle
    }

    fn is_neutral(&self) -> bool {
        self.stable == StableRuntimeV1::Neutral
    }

    fn parked_root(&self, root_id: &str, action_space_id: &str) -> Option<RootBindingV1> {
        match &self.stable {
            StableRuntimeV1::AtRoot { root }
                if root.root_id == root_id && root.action_space_id == action_space_id =>
            {
                Some(root.clone())
            }
            _ => None,
        }
    }
}

fn validate_root(root: &RootBindingV1) -> Result<(), String> {
    if !content_id_has_shape(&root.root_id, "state-root-v2")
        || !content_id_has_shape(&root.activation_id, "state-root-activation-v2")
        || root.action_space_id.trim().is_empty()
        || !is_sha256(&root.source_head_sha256)
    {
        return Err("root binding has invalid typed identity".to_owned());
    }
    Ok(())
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

pub type RuntimeRequestV1 = ShellRequestV1<RuntimeOperationV1>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeOperationV1 {
    BootstrapNaturalNeow {
        seed: u64,
        ascension: u8,
    },
    BootstrapConstructedCombat {
        case_id: String,
    },
    InternalCreateCurrentNativeRecoveryCertificate {},
    PreviewOne {
        root_id: String,
        action_id: String,
    },
    PreviewActions {
        root_id: String,
        action_ids: Vec<String>,
    },
    PreviewAliases {
        action_aliases: Vec<String>,
    },
    PreviewWitness {
        handle: String,
    },
    PreviewWitnessAlias {
        witness_alias: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimePrerequisiteV1 {
    LiveRuntime,
}

impl RuntimeOperationV1 {
    fn prerequisite(&self) -> Option<RuntimePrerequisiteV1> {
        match self {
            Self::BootstrapNaturalNeow { .. }
            | Self::BootstrapConstructedCombat { .. }
            | Self::InternalCreateCurrentNativeRecoveryCertificate {}
            | Self::PreviewOne { .. }
            | Self::PreviewActions { .. }
            | Self::PreviewWitness { .. } => Some(RuntimePrerequisiteV1::LiveRuntime),
            Self::PreviewAliases { .. } | Self::PreviewWitnessAlias { .. } => None,
        }
    }
}

fn runtime_owns_operation(operation: &Value) -> bool {
    matches!(
        operation.get("kind").and_then(Value::as_str),
        Some(
            "bootstrap_natural_neow"
                | "bootstrap_constructed_combat"
                | "internal_create_current_native_recovery_certificate"
                | "preview_one"
                | "preview_actions"
                | "preview_aliases"
                | "preview_witness"
                | "preview_witness_alias"
        )
    )
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceStatusV1 {
    pub schema: String,
    pub service_name: String,
    pub process_id: u32,
    pub started_at_unix_ms: u128,
    pub request_count: u64,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_root_id: Option<String>,
    pub reusable_runtime: bool,
    pub runtime_ownership_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_reconciliation: Option<RuntimeStartupReconciliationV1>,
    pub implementation_mode: String,
    pub build: RuntimeBuildStatusV1,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBuildStatusV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_executable_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_executable_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_matches_current: Option<bool>,
    pub actor_freshness: String,
    pub engine_freshness: String,
    pub run_play_freshness: String,
    pub live_bridge_freshness: String,
    pub artifact_freshness: String,
}

pub type RuntimeResponseV1 = ShellResponseV1;
pub type RuntimeErrorV1 = ShellErrorV1;

struct PreviewExecutionV1 {
    response: Value,
    summaries: Vec<preview::PreviewMemberSummaryV1>,
    detail_handle: String,
}

pub struct WorkbenchRuntimeV1 {
    engine: WorkbenchEngineStateV1,
    supervisor: ProductionSupervisorV1,
    repository: PathBuf,
    started_at_unix_ms: u128,
    request_count: u64,
}

impl Default for WorkbenchRuntimeV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkbenchRuntimeV1 {
    pub fn new() -> Self {
        Self::new_at(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    pub fn new_at(repository: PathBuf) -> Self {
        Self {
            engine: WorkbenchEngineStateV1::default(),
            supervisor: ProductionSupervisorV1::unowned(),
            repository,
            started_at_unix_ms: now_unix_ms(),
            request_count: 0,
        }
    }

    pub fn try_new_owned() -> Result<Self, String> {
        let repository = std::env::current_dir()
            .map_err(|error| format!("failed to resolve workbench repository: {error}"))?;
        Self::try_new_owned_at(repository)
    }

    pub fn try_new_owned_at(repository: PathBuf) -> Result<Self, String> {
        let supervisor = ProductionSupervisorV1::owned(&repository)?;
        Ok(Self {
            engine: WorkbenchEngineStateV1::default(),
            supervisor,
            repository,
            started_at_unix_ms: now_unix_ms(),
            request_count: 0,
        })
    }

    pub fn handle_json(&mut self, input: &str) -> (RuntimeResponseV1, bool) {
        let request = match serde_json::from_str::<ShellRequestV1>(input) {
            Ok(request) => request,
            Err(error) => {
                return (
                    rejected("unknown", "invalid_request", error.to_string()),
                    false,
                );
            }
        };
        let request_id = request.request_id.clone();
        let (request_id, route) = match request.route() {
            Ok(routed) => routed,
            Err(error) => {
                return (
                    rejected(&request_id, error.code, error.detail.to_owned()),
                    false,
                );
            }
        };
        match route {
            ShellRouteV1::ServiceStatus => {
                self.request_count = self.request_count.saturating_add(1);
                (
                    ready(
                        &request_id,
                        serde_json::to_value(self.status()).expect("status serializes"),
                    ),
                    false,
                )
            }
            ShellRouteV1::Shutdown => {
                self.request_count = self.request_count.saturating_add(1);
                let retirement = self.supervisor.shutdown();
                let result = match retirement {
                    Ok(()) => {
                        self.engine.clear_runtime();
                        ready(
                            &request_id,
                            json!({"shutdown": true, "runtime_cache_persisted": false}),
                        )
                    }
                    Err(detail) => rejected(&request_id, "shutdown_failed", detail),
                };
                (result, true)
            }
            ShellRouteV1::Domain(operation) => {
                if !runtime_owns_operation(&operation) {
                    return self.handle_engine_public_operation(request_id, operation);
                }
                let operation = match serde_json::from_value::<RuntimeOperationV1>(operation) {
                    Ok(operation) => operation,
                    Err(error) => {
                        return (
                            rejected(&request_id, "invalid_operation", error.to_string()),
                            false,
                        );
                    }
                };
                self.handle_domain_operation(RuntimeRequestV1 {
                    schema: REQUEST_SCHEMA_V1.to_owned(),
                    request_id,
                    operation,
                })
            }
        }
    }

    fn handle_domain_operation(&mut self, request: RuntimeRequestV1) -> (RuntimeResponseV1, bool) {
        let operation = request.operation;
        self.request_count = self.request_count.saturating_add(1);
        if let Some(prerequisite) = operation.prerequisite() {
            let freshness = match prerequisite {
                RuntimePrerequisiteV1::LiveRuntime => {
                    build_status::ensure_live_runtime_fresh(&self.repository)
                }
            };
            if let Err(detail) = freshness {
                return (
                    rejected(&request.request_id, "runtime_prerequisite_stale", detail),
                    false,
                );
            }
        }
        match operation {
            RuntimeOperationV1::BootstrapNaturalNeow { seed, ascension } => {
                if let Err(error) = self.clear_engine_view() {
                    return (rejected_error(&request.request_id, error), false);
                }
                let response = match self.bootstrap_natural_neow(seed, ascension) {
                    Ok(bootstrapped) => ready(&request.request_id, bootstrapped),
                    Err(detail) => {
                        rejected(&request.request_id, "bootstrap_natural_neow_failed", detail)
                    }
                };
                (response, false)
            }
            RuntimeOperationV1::BootstrapConstructedCombat { case_id } => {
                if let Err(error) = self.clear_engine_view() {
                    return (rejected_error(&request.request_id, error), false);
                }
                let response = match self.bootstrap_constructed_combat(&case_id) {
                    Ok(bootstrapped) => ready(&request.request_id, bootstrapped),
                    Err(detail) => rejected(
                        &request.request_id,
                        "bootstrap_constructed_combat_failed",
                        detail,
                    ),
                };
                (response, false)
            }
            RuntimeOperationV1::InternalCreateCurrentNativeRecoveryCertificate {} => {
                let response = match self.create_current_native_recovery_certificate() {
                    Ok(result) => ready(&request.request_id, result),
                    Err(error) => rejected_error(&request.request_id, error),
                };
                (response, false)
            }
            RuntimeOperationV1::PreviewOne { root_id, action_id } => {
                let response = match self.preview_one(&root_id, &action_id) {
                    Ok(previewed) => ready(&request.request_id, previewed.response),
                    Err(error) => rejected_error(
                        &request.request_id,
                        RuntimeErrorV1 {
                            code: error.code,
                            detail: error.detail,
                        },
                    ),
                };
                (response, false)
            }
            RuntimeOperationV1::PreviewActions {
                root_id,
                action_ids,
            } => {
                let response = match self.preview_actions(&root_id, &action_ids) {
                    Ok(previewed) => ready(&request.request_id, previewed.response),
                    Err(error) => rejected_error(
                        &request.request_id,
                        RuntimeErrorV1 {
                            code: error.code,
                            detail: error.detail,
                        },
                    ),
                };
                (response, false)
            }
            RuntimeOperationV1::PreviewAliases { action_aliases } => {
                let response = match self.preview_action_aliases(&action_aliases) {
                    Ok(manifest) => ready(&request.request_id, manifest),
                    Err(error) => rejected_error(&request.request_id, error),
                };
                (response, false)
            }
            RuntimeOperationV1::PreviewWitness { handle } => {
                let response = match self.clear_engine_view() {
                    Ok(()) => match self.preview_witness(&handle) {
                        Ok(previewed) => ready(&request.request_id, previewed),
                        Err(detail) => {
                            rejected(&request.request_id, "preview_witness_failed", detail)
                        }
                    },
                    Err(error) => rejected_error(&request.request_id, error),
                };
                (response, false)
            }
            RuntimeOperationV1::PreviewWitnessAlias { witness_alias } => {
                let response = match self.preview_witness_alias(&witness_alias) {
                    Ok(manifest) => ready(&request.request_id, manifest),
                    Err(error) => rejected_error(&request.request_id, error),
                };
                (response, false)
            }
        }
    }

    fn handle_engine_public_operation(
        &mut self,
        request_id: String,
        operation: Value,
    ) -> (RuntimeResponseV1, bool) {
        self.request_count = self.request_count.saturating_add(1);
        let input = match serde_json::to_value(EnginePublicRequestV1::new(operation)) {
            Ok(input) => input,
            Err(error) => {
                return (
                    rejected(&request_id, "invalid_operation", error.to_string()),
                    false,
                )
            }
        };
        let response = match self
            .supervisor
            .execute_engine_domain(input, ENGINE_SEARCH_IO_TIMEOUT)
        {
            Ok(output) => ready(&request_id, output),
            Err(error) => rejected(&request_id, &error.code, error.detail),
        };
        (response, false)
    }

    fn execute_engine_domain_value(
        &mut self,
        request: sts_workbench_engine_protocol::EngineDomainRequestV1,
        timeout: std::time::Duration,
    ) -> Result<Value, RuntimeErrorV1> {
        let input = serde_json::to_value(request).map_err(|error| RuntimeErrorV1 {
            code: "engine_request_invalid".to_owned(),
            detail: format!("failed to encode Engine request: {error}"),
        })?;
        self.supervisor
            .execute_engine_domain(input, timeout)
            .map_err(|error| RuntimeErrorV1 {
                code: error.code,
                detail: error.detail,
            })
    }

    fn execute_view_manifest(
        &mut self,
        request: sts_workbench_engine_protocol::EngineDomainRequestV1,
        timeout: std::time::Duration,
        public_schema: &str,
    ) -> Result<Value, RuntimeErrorV1> {
        let output = self.execute_engine_domain_value(request, timeout)?;
        let manifest =
            serde_json::from_value::<sts_workbench_engine_protocol::EngineViewManifestV1>(output)
                .map_err(|error| RuntimeErrorV1 {
                code: "engine_response_invalid".to_owned(),
                detail: format!("Engine returned an invalid view manifest: {error}"),
            })?;
        if manifest.schema != sts_workbench_engine_protocol::ENGINE_VIEW_MANIFEST_SCHEMA_V1 {
            return Err(RuntimeErrorV1 {
                code: "engine_response_invalid".to_owned(),
                detail: "Engine view manifest schema mismatch".to_owned(),
            });
        }
        Ok(json!({
            "schema": public_schema,
            "manifest": manifest.manifest,
        }))
    }

    fn clear_engine_view(&mut self) -> Result<(), RuntimeErrorV1> {
        self.execute_view_manifest(
            sts_workbench_engine_protocol::EngineDomainRequestV1::clear_view(),
            ENGINE_VIEW_IO_TIMEOUT,
            "sts-workbench-internal-clear-view-v1",
        )
        .map(|_| ())
    }

    fn preview_action_aliases(&mut self, aliases: &[String]) -> Result<Value, RuntimeErrorV1> {
        self.prepare_alias_execution("action")?;
        let output = self.execute_engine_domain_value(
            sts_workbench_engine_protocol::EngineDomainRequestV1::resolve_action_aliases(
                aliases.to_vec(),
            ),
            ENGINE_VIEW_IO_TIMEOUT,
        )?;
        let resolution = match serde_json::from_value::<
            sts_workbench_engine_protocol::EngineActionPreviewResolutionV1,
        >(output)
        {
            Ok(resolution) => resolution,
            Err(error) => {
                let error = RuntimeErrorV1 {
                    code: "engine_response_invalid".to_owned(),
                    detail: format!("Engine returned an invalid action resolution: {error}"),
                };
                return Err(self.abort_engine_view_execution(
                    sts_workbench_engine_protocol::EngineDomainRequestV1::abort_action_preview(),
                    error,
                ));
            }
        };
        if resolution.schema
            != sts_workbench_engine_protocol::ENGINE_ACTION_PREVIEW_RESOLUTION_SCHEMA_V1
            || !content_id_has_shape(&resolution.root_id, "state-root-v2")
            || !content_id_has_shape(&resolution.action_space_id, "paired-action-space-v1")
            || resolution.actions.len() != aliases.len()
            || resolution
                .actions
                .iter()
                .zip(aliases)
                .any(|(action, alias)| action.alias != *alias || action.action_id.trim().is_empty())
        {
            let error = RuntimeErrorV1 {
                code: "engine_response_invalid".to_owned(),
                detail: "Engine action resolution changed its requested alias scope".to_owned(),
            };
            return Err(self.abort_engine_view_execution(
                sts_workbench_engine_protocol::EngineDomainRequestV1::abort_action_preview(),
                error,
            ));
        }
        let action_ids = resolution
            .actions
            .iter()
            .map(|action| action.action_id.clone())
            .collect::<Vec<_>>();
        let previewed = match self.preview_authorized_actions(
            &resolution.root_id,
            &resolution.action_space_id,
            &action_ids,
        ) {
            Ok(previewed) => previewed,
            Err(error) => {
                let abort = self.execute_engine_domain_value(
                    sts_workbench_engine_protocol::EngineDomainRequestV1::abort_action_preview(),
                    ENGINE_VIEW_IO_TIMEOUT,
                );
                return Err(RuntimeErrorV1 {
                    code: error.code,
                    detail: append_engine_abort_failure(error.detail, abort.err()),
                });
            }
        };
        let completion = sts_workbench_engine_protocol::EngineActionPreviewCompletionV1 {
            schema: sts_workbench_engine_protocol::ENGINE_ACTION_PREVIEW_COMPLETION_SCHEMA_V1
                .to_owned(),
            root_id: resolution.root_id,
            action_space_id: resolution.action_space_id,
            summaries: previewed.summaries,
            detail_handle: previewed.detail_handle,
        };
        match self.execute_view_manifest(
            sts_workbench_engine_protocol::EngineDomainRequestV1::accept_action_preview(completion),
            ENGINE_VIEW_IO_TIMEOUT,
            "sts-workbench-preview-manifest-response-v1",
        ) {
            Ok(manifest) => Ok(manifest),
            Err(error) => Err(self.retire_after_unleased_preview(error)),
        }
    }

    fn preview_witness_alias(&mut self, alias: &str) -> Result<Value, RuntimeErrorV1> {
        self.prepare_alias_execution("witness")?;
        let output = self.execute_engine_domain_value(
            sts_workbench_engine_protocol::EngineDomainRequestV1::resolve_witness_alias(alias),
            ENGINE_VIEW_IO_TIMEOUT,
        )?;
        let resolution = match serde_json::from_value::<
            sts_workbench_engine_protocol::EngineWitnessPreviewResolutionV1,
        >(output)
        {
            Ok(resolution) => resolution,
            Err(error) => {
                let error = RuntimeErrorV1 {
                    code: "engine_response_invalid".to_owned(),
                    detail: format!("Engine returned an invalid witness resolution: {error}"),
                };
                return Err(self.abort_engine_view_execution(
                    sts_workbench_engine_protocol::EngineDomainRequestV1::abort_witness_preview(),
                    error,
                ));
            }
        };
        if resolution.schema
            != sts_workbench_engine_protocol::ENGINE_WITNESS_PREVIEW_RESOLUTION_SCHEMA_V1
            || !content_id_has_shape(&resolution.root_id, "state-root-v2")
            || !content_id_has_shape(&resolution.action_space_id, "paired-action-space-v1")
            || !artifact_handle_has_shape(&resolution.witness_handle)
            || resolution.plan_step_count == 0
        {
            let error = RuntimeErrorV1 {
                code: "engine_response_invalid".to_owned(),
                detail: "Engine witness resolution has an invalid scope".to_owned(),
            };
            return Err(self.abort_engine_view_execution(
                sts_workbench_engine_protocol::EngineDomainRequestV1::abort_witness_preview(),
                error,
            ));
        }
        let scope = match witness::load_scope(&self.repository, &resolution.witness_handle) {
            Ok(scope) => scope,
            Err(detail) => {
                let error = RuntimeErrorV1 {
                    code: "preview_witness_alias_failed".to_owned(),
                    detail,
                };
                return Err(self.abort_engine_view_execution(
                    sts_workbench_engine_protocol::EngineDomainRequestV1::abort_witness_preview(),
                    error,
                ));
            }
        };
        if scope.initial_root_id != resolution.root_id
            || scope.action_space_id != resolution.action_space_id
        {
            let error = RuntimeErrorV1 {
                code: "WitnessExpired".to_owned(),
                detail: "W alias no longer matches its durable witness scope".to_owned(),
            };
            return Err(self.abort_engine_view_execution(
                sts_workbench_engine_protocol::EngineDomainRequestV1::abort_witness_preview(),
                error,
            ));
        }
        let previewed = match self.preview_witness_scope(scope) {
            Ok(previewed) => previewed,
            Err(detail) => {
                let abort = self.execute_engine_domain_value(
                    sts_workbench_engine_protocol::EngineDomainRequestV1::abort_witness_preview(),
                    ENGINE_VIEW_IO_TIMEOUT,
                );
                return Err(RuntimeErrorV1 {
                    code: "preview_witness_alias_failed".to_owned(),
                    detail: append_engine_abort_failure(detail, abort.err()),
                });
            }
        };
        match self.execute_view_manifest(
            sts_workbench_engine_protocol::EngineDomainRequestV1::accept_witness_preview(
                previewed.completion,
            ),
            ENGINE_VIEW_IO_TIMEOUT,
            "sts-workbench-witness-preview-manifest-response-v1",
        ) {
            Ok(manifest) => Ok(manifest),
            Err(error) => Err(self.retire_after_unleased_preview(error)),
        }
    }

    fn retire_after_unleased_preview(&mut self, mut error: RuntimeErrorV1) -> RuntimeErrorV1 {
        if let Err(retirement) = self.retire_runtime("engine_view_completion_failed") {
            error.detail = format!(
                "{}; managed runtime retirement also failed: {retirement}",
                error.detail
            );
        }
        error
    }

    fn prepare_alias_execution(&mut self, kind: &str) -> Result<(), RuntimeErrorV1> {
        self.require_runtime_halves_aligned()
            .map_err(|detail| RuntimeErrorV1 {
                code: "runtime_state_inconsistent".to_owned(),
                detail,
            })?;
        let operation_id = format!(
            "preview-{kind}-alias-preflight-{}-{}",
            process::id(),
            self.request_count
        );
        self.supervisor
            .revalidate_parked_before_view_execution(&operation_id)
            .map_err(|error| RuntimeErrorV1 {
                code: error.code,
                detail: error.detail,
            })
    }

    fn abort_engine_view_execution(
        &mut self,
        request: sts_workbench_engine_protocol::EngineDomainRequestV1,
        mut error: RuntimeErrorV1,
    ) -> RuntimeErrorV1 {
        let abort = self.execute_engine_domain_value(request, ENGINE_VIEW_IO_TIMEOUT);
        error.detail = append_engine_abort_failure(error.detail, abort.err());
        error
    }

    fn authorize_actions(
        &mut self,
        root_id: &str,
        action_ids: &[String],
    ) -> Result<sts_workbench_engine_protocol::EngineActionAuthorizationV1, String> {
        let input = serde_json::to_value(
            sts_workbench_engine_protocol::EngineDomainRequestV1::authorize_actions(
                root_id,
                action_ids.to_vec(),
            ),
        )
        .map_err(|error| format!("failed to encode Engine action authorization: {error}"))?;
        let output = self.supervisor.execute_engine(input)?;
        let authorized = serde_json::from_value::<
            sts_workbench_engine_protocol::EngineActionAuthorizationV1,
        >(output)
        .map_err(|error| format!("Engine returned invalid action authorization: {error}"))?;
        if authorized.schema != sts_workbench_engine_protocol::ENGINE_ACTION_AUTHORIZATION_SCHEMA_V1
            || authorized.root_id != root_id
            || !content_id_has_shape(&authorized.action_space_id, "paired-action-space-v1")
        {
            return Err("Engine returned an invalid action authorization scope".to_owned());
        }
        Ok(authorized)
    }

    pub fn status(&self) -> ServiceStatusV1 {
        let runtime_cache = self.engine.runtime_cache();
        let engine_runtime_present = self.engine.has_java_runtime();
        let phase = self.supervisor.phase(runtime_cache, engine_runtime_present);
        ServiceStatusV1 {
            schema: STATUS_SCHEMA_V2.to_owned(),
            service_name: "sts_workbench_runtime".to_owned(),
            process_id: process::id(),
            started_at_unix_ms: self.started_at_unix_ms,
            request_count: self.request_count,
            phase: phase.to_owned(),
            active_root_id: (phase == "at_root")
                .then(|| runtime_cache.active_root_id())
                .flatten()
                .map(str::to_owned),
            reusable_runtime: self
                .supervisor
                .reusable(runtime_cache, engine_runtime_present),
            runtime_ownership_mode: self.supervisor.ownership_mode().to_owned(),
            startup_reconciliation: self.supervisor.startup_reconciliation().cloned(),
            implementation_mode: "long_lived_rust_actor_jsonl_v1".to_owned(),
            build: self.supervisor.build_status(&self.repository),
            capabilities: vec![
                "service_status".to_owned(),
                "internal_bootstrap_natural_neow_parentless_root".to_owned(),
                "internal_bootstrap_constructed_combat_exact_fixture".to_owned(),
                "internal_create_current_native_recovery_certificate".to_owned(),
                "observe_explicit_root".to_owned(),
                "observe_selected_root".to_owned(),
                "observe_preview_alias".to_owned(),
                "replaceable_engine_owned_action_alias_lease".to_owned(),
                "replaceable_engine_owned_preview_alias_lease".to_owned(),
                "replaceable_engine_owned_witness_alias_lease".to_owned(),
                "replaceable_engine_owned_current_view_diagnostic_lease".to_owned(),
                "commit_preview_alias".to_owned(),
                "preview_witness_alias".to_owned(),
                "inspect_current_view_diagnostic_alias".to_owned(),
                "inspect_content_addressed_artifact".to_owned(),
                "external_engine_inspect".to_owned(),
                "search_explicit_root".to_owned(),
                "search_selected_root".to_owned(),
                "search_current_view".to_owned(),
                "preview_one_exact_child_parking".to_owned(),
                "preview_same_root_independent_actions".to_owned(),
                "preview_sequential_witness".to_owned(),
                "commit_selected_receipt".to_owned(),
                "typed_natural_combat_reset".to_owned(),
                "rust_owned_java_runtime".to_owned(),
                "typed_lifecycle".to_owned(),
            ],
        }
    }

    fn bootstrap_natural_neow(&mut self, seed: u64, ascension: u8) -> Result<Value, String> {
        let total_started = Instant::now();
        self.require_runtime_halves_aligned()?;
        if !self.engine.runtime_is_idle() || !self.supervisor.is_idle() {
            return Err("natural Neow bootstrap requires one idle actor".to_owned());
        }
        let storage_registry = self.owned_storage_registry()?;
        let operation_id = format!("bootstrap-neow-{}-{}", process::id(), self.request_count);
        let opened = match runtime_manager::ManagedJavaRuntimeV1::bootstrap_natural_neow(
            &self.repository,
            seed,
            ascension,
            self.request_count,
            &storage_registry,
        ) {
            Ok(opened) => opened,
            Err(error) => {
                let retirement = self.retire_runtime("natural_neow_bootstrap_failed");
                return match retirement {
                    Ok(()) => Err(error),
                    Err(retirement) => Err(format!(
                        "{error}; bootstrap retirement also failed: {retirement}"
                    )),
                };
            }
        };
        let resident = opened.runtime.resident_binding();
        self.admit_managed_runtime(opened.runtime)?;
        if let Err(error) = self
            .supervisor
            .begin_exchange(self.engine.runtime_cache(), &operation_id)
        {
            let _ = self.retire_runtime("natural_neow_bootstrap_binding_failed");
            return Err(error);
        }
        if let Err(error) = self.park_managed_runtime(&operation_id, opened.root.clone()) {
            let _ = self.retire_runtime("natural_neow_bootstrap_parked_state_rejected");
            return Err(error);
        }
        let candidate_job_id = format!(
            "retain-bootstrap-candidate-{}-{}",
            process::id(),
            self.request_count
        );
        let candidate_capture_ms = match self.supervisor.retain_native_recovery_candidate(
            &candidate_job_id,
            &opened.root.root_id,
            &opened.root.activation_id,
        ) {
            Ok(production_supervisor::NativeRecoveryCandidateRetentionV1::Retained {
                elapsed_ms,
            }) => elapsed_ms,
            Ok(production_supervisor::NativeRecoveryCandidateRetentionV1::Ineligible) => {
                let _ = self.retire_runtime("natural_neow_bootstrap_candidate_ineligible");
                return Err(
                    "natural Neow state root was not eligible for native candidate capture"
                        .to_owned(),
                );
            }
            Err(error) => {
                let (code, detail) = error.into_parts();
                let retirement = self.retire_runtime("natural_neow_bootstrap_candidate_failed");
                return match retirement {
                    Ok(()) => Err(format!("{code}: {detail}")),
                    Err(retirement) => Err(format!(
                        "{code}: {detail}; managed runtime retirement also failed: {retirement}"
                    )),
                };
            }
        };
        Ok(json!({
            "schema": "sts-workbench-bootstrap-natural-neow-response-v2",
            "status": "ready",
            "seed": opened.seed.to_string(),
            "ascension": opened.ascension,
            "neow_offer": opened.neow_offer,
            "resident": resident,
            "root": opened.root,
            "comparisons": {
                "java_rust_exact": opened.exact_components,
            },
            "recovery": {
                "candidate": "retained",
                "certificate": "pending",
            },
            "timing_ms": {
                "runtime_ready": opened.runtime_ready_ms,
                "adapter": opened.adapter_ms,
                "state_root_ready": opened.total_ms,
                "candidate_capture": candidate_capture_ms,
                "total": total_started.elapsed().as_millis(),
            },
        }))
    }

    fn bootstrap_constructed_combat(&mut self, case_id: &str) -> Result<Value, String> {
        self.require_runtime_halves_aligned()?;
        if !self.engine.runtime_is_idle() || !self.supervisor.is_idle() {
            return Err("constructed combat bootstrap requires one idle actor".to_owned());
        }
        let storage_registry = self.owned_storage_registry()?;
        let operation_id = format!(
            "bootstrap-constructed-{}-{}",
            process::id(),
            self.request_count
        );
        let opened = match runtime_manager::ManagedJavaRuntimeV1::bootstrap_constructed_combat(
            &self.repository,
            case_id,
            self.request_count,
            &storage_registry,
        ) {
            Ok(opened) => opened,
            Err(error) => {
                let retirement = self.retire_runtime("constructed_combat_bootstrap_failed");
                return match retirement {
                    Ok(()) => Err(error),
                    Err(retirement) => Err(format!(
                        "{error}; bootstrap retirement also failed: {retirement}"
                    )),
                };
            }
        };
        let resident = opened.runtime.resident_binding();
        self.admit_managed_runtime(opened.runtime)?;
        if let Err(error) = self
            .supervisor
            .begin_exchange(self.engine.runtime_cache(), &operation_id)
        {
            let _ = self.retire_runtime("constructed_combat_binding_failed");
            return Err(error);
        }
        if let Err(error) = self.park_managed_runtime(&operation_id, opened.root.clone()) {
            let _ = self.retire_runtime("constructed_combat_parked_state_rejected");
            return Err(error);
        }
        Ok(json!({
            "schema": "sts-workbench-bootstrap-constructed-combat-response-v1",
            "status": "ready",
            "case_id": opened.case_id,
            "scenario_id": opened.scenario_id,
            "resident": resident,
            "root": opened.root,
            "comparisons": {
                "java_rust_exact": opened.exact_components,
            },
            "java_cold_recovery": "unsupported",
            "timing_ms": {
                "runtime_ready": opened.runtime_ready_ms,
                "scenario": opened.scenario_ms,
                "adapter": opened.adapter_ms,
                "total": opened.total_ms,
            },
        }))
    }

    fn create_current_native_recovery_certificate(&mut self) -> Result<Value, RuntimeErrorV1> {
        self.require_runtime_halves_aligned()
            .map_err(native_recovery_sentinel_error)?;
        let root = match &self.engine.runtime_cache().stable {
            StableRuntimeV1::AtRoot { root } => root.clone(),
            _ => {
                return Err(native_recovery_sentinel_error(
                    "native recovery sentinel requires one current Exact parked root",
                ));
            }
        };
        let job_id = format!(
            "native-recovery-sentinel-{}-{}",
            process::id(),
            self.request_count
        );
        match self.supervisor.create_native_recovery_certificate(
            &job_id,
            &root.root_id,
            &root.activation_id,
        ) {
            Ok(elapsed_ms) => Ok(json!({
                "schema": "sts-workbench-native-recovery-sentinel-response-v1",
                "status": "published",
                "source_preserved": true,
                "elapsed_ms": elapsed_ms,
            })),
            Err(error)
                if self
                    .supervisor
                    .native_recovery_failure_requires_retirement() =>
            {
                let retirement = self.retire_runtime("native_recovery_sentinel_failed");
                let (code, detail) = error.into_parts();
                match retirement {
                    Ok(()) => Err(RuntimeErrorV1 { code, detail }),
                    Err(retirement) => Err(RuntimeErrorV1 {
                        code,
                        detail: format!(
                            "{detail}; native recovery retirement also failed: {retirement}"
                        ),
                    }),
                }
            }
            Err(error) => {
                let (code, detail) = error.into_parts();
                Err(RuntimeErrorV1 { code, detail })
            }
        }
    }

    fn preview_one(
        &mut self,
        root_id: &str,
        action_id: &str,
    ) -> Result<PreviewExecutionV1, preview::PreviewFailureV1> {
        self.preview_actions(root_id, &[action_id.to_owned()])
    }

    fn preview_witness(&mut self, handle: &str) -> Result<Value, String> {
        let scope = witness::load_scope(&self.repository, handle)?;
        self.preview_witness_scope(scope)
            .map(|published| published.response)
    }

    fn preview_witness_scope(
        &mut self,
        scope: witness::WitnessScopeV1,
    ) -> Result<witness::PublishedWitnessPreviewV1, String> {
        self.require_runtime_halves_aligned()?;
        let initial_root_id = scope.initial_root_id.clone();
        let parked_source = self
            .engine
            .parked_root(&initial_root_id, &scope.action_space_id)
            .filter(|_| self.supervisor.has_runtime());
        if self.supervisor.has_runtime() && parked_source.is_none() {
            let neutral_compatible = self.engine.runtime_is_neutral()
                && self.supervisor.is_neutral()
                && self
                    .runtime_access()
                    .expect("managed runtime exists")
                    .can_recover_target(&initial_root_id)?;
            if !neutral_compatible {
                self.retire_runtime("witness_requires_compatible_runtime")?;
            }
        }
        let operation_id = format!("preview-witness-{}-{}", process::id(), self.request_count);
        let mut runtime_ready_ms = 0_u128;
        let mut opened = false;
        if !self.supervisor.has_runtime() {
            let opened_runtime = match runtime_manager::ManagedJavaRuntimeV1::open(
                &self.repository,
                &initial_root_id,
                self.request_count,
                &self.owned_storage_registry()?,
            ) {
                Ok(opened_runtime) => opened_runtime,
                Err(error) => {
                    let _ = self.retire_runtime("witness_runtime_open_failed");
                    return Err(error.into());
                }
            };
            self.admit_managed_runtime(opened_runtime.runtime)?;
            runtime_ready_ms = opened_runtime.runtime_ready_ms;
            opened = true;
        }
        if let Err(error) = self
            .supervisor
            .begin_exchange(self.engine.runtime_cache(), &operation_id)
        {
            let _ = self.retire_runtime("witness_preview_state_rejected");
            return Err(error);
        }
        let total_started = Instant::now();
        let execution = {
            let repository = self.repository.clone();
            let request_count = self.request_count;
            let mut runtime = self
                .runtime_access()
                .expect("witness preview has a managed runtime");
            if let Some(parked) = &parked_source {
                witness::execute_from_parked(
                    &repository,
                    &mut runtime,
                    &parked.activation_id,
                    scope,
                    request_count,
                )
            } else {
                let materialization = runtime.materialization().clone();
                witness::execute_from_neutral(
                    &repository,
                    &mut runtime,
                    &materialization,
                    scope,
                    request_count,
                    runtime_ready_ms,
                    !opened,
                )
            }
        };
        let execution = match execution {
            Ok(execution) => execution,
            Err(error) => {
                let retirement = self.retire_runtime("witness_execution_failed");
                return match retirement {
                    Ok(()) => Err(error),
                    Err(retirement) => Err(format!(
                        "{error}; managed runtime retirement also failed: {retirement}"
                    )),
                };
            }
        };
        let reset_eligible = matches!(
            execution.status,
            sts_workbench_engine_protocol::EngineWitnessPreviewStatusV1::Ready
                | sts_workbench_engine_protocol::EngineWitnessPreviewStatusV1::IntentUnavailable
        ) && execution.autosave_unchanged;
        let mut reset_ms = 0_u128;
        let reset_capability = if reset_eligible && parked_source.is_none() {
            let mut runtime = self
                .runtime_access()
                .expect("witness execution retained its runtime");
            runtime
                .can_recover_target(&initial_root_id)
                .and_then(|recoverable| {
                    if recoverable {
                        runtime.natural_reset_available()
                    } else {
                        Ok(false)
                    }
                })
        } else {
            Ok(false)
        };
        let can_reset = match reset_capability {
            Ok(can_reset) => can_reset,
            Err(error) => {
                let retirement = self.retire_runtime("witness_reset_preflight_failed");
                return match retirement {
                    Ok(()) => Err(error),
                    Err(retirement) => Err(format!(
                        "{error}; managed runtime retirement also failed: {retirement}"
                    )),
                };
            }
        };
        if can_reset {
            let reset = self
                .supervisor
                .reset_to_native(&operation_id, self.request_count);
            reset_ms = match reset {
                Ok(reset_ms) => reset_ms,
                Err(error) => {
                    return Err(self.recover_or_retire_after_native_reset_failure(
                        error,
                        "witness_typed_reset_failed",
                    ));
                }
            };
            if let Err(error) = self.engine.reset_runtime() {
                let _ = self.retire_runtime("witness_reset_context_rejected");
                return Err(error);
            }
        } else {
            self.retire_runtime(&format!("witness_settled_{}", execution.status.token()))?;
        }
        witness::publish_response(
            &self.repository,
            execution,
            reset_ms,
            runtime_ready_ms.saturating_add(total_started.elapsed().as_millis()),
        )
    }

    fn preview_actions(
        &mut self,
        root_id: &str,
        action_ids: &[String],
    ) -> Result<PreviewExecutionV1, preview::PreviewFailureV1> {
        self.require_runtime_halves_aligned()?;
        let scope = self.authorize_actions(root_id, action_ids)?;
        self.preview_authorized_actions(root_id, &scope.action_space_id, action_ids)
    }

    fn preview_authorized_actions(
        &mut self,
        root_id: &str,
        action_space_id: &str,
        action_ids: &[String],
    ) -> Result<PreviewExecutionV1, preview::PreviewFailureV1> {
        self.require_runtime_halves_aligned()?;
        let aggregate_started = Instant::now();
        let mut outcomes = Vec::new();
        let mut settlements = Vec::new();
        let mut reset_count = 0_usize;
        let mut cold_runtime_count = 0_usize;
        let mut reset_ms = 0_u128;

        for (member_index, action_id) in action_ids.iter().enumerate() {
            let parked_source = self
                .engine
                .parked_root(root_id, action_space_id)
                .filter(|_| self.supervisor.has_runtime());
            if parked_source.is_none() && self.supervisor.has_runtime() {
                let neutral_compatible = self.engine.runtime_is_neutral()
                    && self.supervisor.is_neutral()
                    && self
                        .runtime_access()
                        .expect("managed runtime exists")
                        .can_recover_target(root_id)?;
                if !neutral_compatible {
                    self.retire_runtime("requested_root_is_not_recoverable_by_resident")?;
                }
            }

            let operation_id = format!(
                "preview-actions-{}-{}-{member_index}",
                process::id(),
                self.request_count
            );
            let mut runtime_ready_ms = 0_u128;
            let mut opened_for_member = false;
            if !self.supervisor.has_runtime() {
                let opened = match runtime_manager::ManagedJavaRuntimeV1::open(
                    &self.repository,
                    root_id,
                    self.request_count,
                    &self.owned_storage_registry()?,
                ) {
                    Ok(opened) => opened,
                    Err(error) => {
                        let _ = self.retire_runtime("java_runtime_open_failed");
                        return Err(error.into());
                    }
                };
                self.admit_managed_runtime(opened.runtime)?;
                runtime_ready_ms = opened.runtime_ready_ms;
                opened_for_member = true;
                cold_runtime_count += 1;
            }

            if let Err(error) = self
                .supervisor
                .begin_exchange(self.engine.runtime_cache(), &operation_id)
            {
                let _ = self.retire_runtime("preview_state_rejected");
                return Err(error.into());
            }
            let execution = {
                let repository = self.repository.clone();
                let request_count = self.request_count;
                let Some(mut runtime) = self.runtime_access() else {
                    let _ = self.retire_runtime("preview_runtime_missing");
                    return Err("preview lifecycle omitted its managed Java runtime"
                        .to_owned()
                        .into());
                };
                if let Some(source) = &parked_source {
                    preview::preview_from_parked(
                        &repository,
                        &mut runtime,
                        root_id,
                        &source.activation_id,
                        action_id,
                        action_space_id,
                        request_count,
                    )
                } else {
                    let materialization = runtime.materialization().clone();
                    preview::preview_from_neutral(
                        &repository,
                        &mut runtime,
                        &materialization,
                        root_id,
                        action_id,
                        action_space_id,
                        request_count,
                        runtime_ready_ms,
                        !opened_for_member,
                    )
                }
            };
            let outcome = match execution {
                Ok(outcome) => outcome,
                Err(error) => {
                    let retirement = self.retire_runtime("preview_execution_failed");
                    return match retirement {
                        Ok(()) => Err(error),
                        Err(retirement) => Err(preview::PreviewFailureV1 {
                            code: error.code,
                            detail: format!(
                                "{}; managed runtime retirement also failed: {retirement}",
                                error.detail
                            ),
                        }),
                    };
                }
            };

            if action_ids.len() == 1 {
                let response = outcome.response.clone();
                let summary = outcome.summary.clone();
                let detail_handle = outcome.detail_handle.clone();
                if let Some(parked) = outcome.parked {
                    let parked_root = parked.root;
                    if let Err(error) =
                        self.park_managed_runtime(&operation_id, parked_root.clone())
                    {
                        let _ = self.retire_runtime("parked_state_rejected");
                        return Err(error.into());
                    }
                    if !outcome.autosave_unchanged {
                        let candidate_job_id = format!(
                            "retain-native-candidate-{}-{}",
                            process::id(),
                            self.request_count
                        );
                        match self.supervisor.retain_native_recovery_candidate(
                            &candidate_job_id,
                            &parked_root.root_id,
                            &parked_root.activation_id,
                        ) {
                            Ok(production_supervisor::NativeRecoveryCandidateRetentionV1::Retained {
                                ..
                            }
                            | production_supervisor::NativeRecoveryCandidateRetentionV1::Ineligible) => {}
                            Err(error) => {
                                let (code, detail) = error.into_parts();
                                let retirement =
                                    self.retire_runtime("native_recovery_candidate_failed");
                                return Err(preview::PreviewFailureV1 {
                                    code,
                                    detail: match retirement {
                                        Ok(()) => detail,
                                        Err(retirement) => format!(
                                            "{detail}; managed runtime retirement also failed: {retirement}"
                                        ),
                                    },
                                });
                            }
                        }
                    }
                } else {
                    self.retire_runtime(&format!("preview_settled_{}", outcome.status.token()))?;
                }
                return Ok(PreviewExecutionV1 {
                    response,
                    summaries: vec![summary],
                    detail_handle,
                });
            }

            if outcome.status != preview::PreviewSettlementV1::Ready {
                settlements.push("retire_after_non_ready".to_owned());
                outcomes.push(outcome);
                self.retire_runtime("independent_preview_stopped_non_ready")?;
                break;
            }

            let has_more = member_index + 1 < action_ids.len();
            let can_reset = if outcome.autosave_unchanged {
                let reset_capability = {
                    let mut runtime = self
                        .runtime_access()
                        .expect("preview outcome retained its managed runtime");
                    runtime.can_recover_target(root_id).and_then(|recoverable| {
                        if recoverable {
                            runtime.natural_reset_available()
                        } else {
                            Ok(false)
                        }
                    })
                };
                match reset_capability {
                    Ok(available) => available,
                    Err(error) => {
                        let retirement = self.retire_runtime("typed_reset_preflight_failed");
                        return match retirement {
                            Ok(()) => Err(error.into()),
                            Err(retirement) => Err(format!(
                                "{error}; managed runtime retirement also failed: {retirement}"
                            )
                            .into()),
                        };
                    }
                }
            } else {
                false
            };
            if can_reset {
                let reset_result = self
                    .supervisor
                    .reset_to_native(&operation_id, self.request_count);
                let elapsed = match reset_result {
                    Ok(elapsed) => elapsed,
                    Err(error) => {
                        return Err(self
                            .recover_or_retire_after_native_reset_failure(
                                error,
                                "typed_natural_reset_failed",
                            )
                            .into());
                    }
                };
                if let Err(error) = self.engine.reset_runtime() {
                    let _ = self.retire_runtime("reset_context_rejected");
                    return Err(error.into());
                }
                reset_count += 1;
                reset_ms = reset_ms.saturating_add(elapsed);
                settlements.push("reset_to_native".to_owned());
            } else if has_more {
                settlements.push("retire_and_reopen".to_owned());
                self.retire_runtime("independent_preview_requires_fresh_runtime")?;
            } else {
                let Some(parked) = outcome.parked.as_ref() else {
                    let _ = self.retire_runtime("ready_preview_omitted_parked_successor");
                    return Err("ready preview omitted its Exact parked successor"
                        .to_owned()
                        .into());
                };
                if let Err(error) = self.park_managed_runtime(&operation_id, parked.root.clone()) {
                    let _ = self.retire_runtime("parked_state_rejected");
                    return Err(error.into());
                }
                settlements.push("park_at_successor".to_owned());
            }
            outcomes.push(outcome);
        }

        let summaries = outcomes
            .iter()
            .map(|outcome| outcome.summary.clone())
            .collect();
        let aggregated = preview::aggregate_previews(
            &self.repository,
            root_id,
            action_space_id,
            action_ids,
            &outcomes,
            &settlements,
            reset_count,
            cold_runtime_count,
            reset_ms,
            aggregate_started.elapsed().as_millis(),
        )?;
        Ok(PreviewExecutionV1 {
            response: aggregated.response,
            summaries,
            detail_handle: aggregated.detail_handle,
        })
    }

    fn admit_managed_runtime(
        &mut self,
        runtime: runtime_manager::ManagedJavaRuntimeV1,
    ) -> Result<(), String> {
        if !self.engine.runtime_is_idle() || self.engine.has_java_runtime() {
            return Err("runtime admission requires an idle Engine cache".to_owned());
        }
        let (physical, engine) = runtime.into_parts();
        self.supervisor.admit_runtime(physical)?;
        if let Err(error) = self.engine.admit_runtime(engine) {
            let retirement = self
                .supervisor
                .retire_runtime("engine_context_admission_failed");
            self.engine.clear_runtime();
            return match retirement {
                Ok(()) => Err(error),
                Err(retirement) => Err(format!(
                    "{error}; physical runtime retirement also failed: {retirement}"
                )),
            };
        }
        Ok(())
    }

    fn park_managed_runtime(
        &mut self,
        operation_id: &str,
        root: RootBindingV1,
    ) -> Result<(), String> {
        validate_root(&root)?;
        self.supervisor.park_exchange(operation_id)?;
        self.engine.park_runtime(root)
    }

    fn retire_runtime(&mut self, reason: &str) -> Result<(), String> {
        self.supervisor.retire_runtime(reason)?;
        self.engine.clear_runtime();
        Ok(())
    }

    fn recover_or_retire_after_native_reset_failure(
        &mut self,
        error: String,
        retire_reason: &str,
    ) -> String {
        if self.supervisor.is_dirty() {
            let recovery_id = format!(
                "recover-native-reset-{}-{}",
                process::id(),
                self.request_count
            );
            match self
                .supervisor
                .recover_dirty_to_native(&recovery_id, self.request_count)
            {
                Ok(_) => match self.engine.reset_runtime() {
                    Ok(()) => {
                        return format!(
                            "{error}; managed Java runtime recovered to its neutral native boundary"
                        );
                    }
                    Err(context) => {
                        let retirement = self.retire_runtime("native_recovery_context_failed");
                        return match retirement {
                            Ok(()) => format!(
                                "{error}; physical recovery succeeded but Engine context failed: {context}"
                            ),
                            Err(retirement) => format!(
                                "{error}; physical recovery succeeded but Engine context failed: {context}; managed runtime retirement also failed: {retirement}"
                            ),
                        };
                    }
                },
                Err(recovery) => {
                    let retirement = self.retire_runtime("native_reset_recovery_failed");
                    return match retirement {
                        Ok(()) => format!(
                            "{error}; managed Java runtime recovery failed: {recovery}"
                        ),
                        Err(retirement) => format!(
                            "{error}; managed Java runtime recovery failed: {recovery}; retirement also failed: {retirement}"
                        ),
                    };
                }
            }
        }
        let retirement = self.retire_runtime(retire_reason);
        match retirement {
            Ok(()) => error,
            Err(retirement) => {
                format!("{error}; managed runtime retirement also failed: {retirement}")
            }
        }
    }

    fn require_runtime_halves_aligned(&mut self) -> Result<(), String> {
        if self.supervisor.has_runtime() == self.engine.has_java_runtime() {
            Ok(())
        } else {
            let detail = "physical Java runtime and Engine context ownership disagree";
            match self.retire_runtime("runtime_ownership_halves_disagree") {
                Ok(()) => Err(detail.to_owned()),
                Err(retirement) => Err(format!(
                    "{detail}; fail-closed retirement also failed: {retirement}"
                )),
            }
        }
    }

    fn runtime_access(&mut self) -> Option<runtime_manager::JavaRuntimeAccessV1<'_>> {
        let physical = self.supervisor.runtime_mut()?;
        let engine = self.engine.java_runtime_mut()?;
        Some(runtime_manager::JavaRuntimeAccessV1::new(physical, engine))
    }

    fn owned_storage_registry(
        &self,
    ) -> Result<sts_workbench_supervisor::OwnedStorageRegistryV1, String> {
        self.supervisor.storage_registry()
    }
}

fn ready(request_id: &str, result: Value) -> RuntimeResponseV1 {
    RuntimeResponseV1::ready(request_id, result)
}

fn rejected(request_id: &str, code: &str, detail: String) -> RuntimeResponseV1 {
    RuntimeResponseV1::rejected(request_id, code, detail)
        .with_error_manifest(model_error::render(code))
}

fn rejected_error(request_id: &str, error: RuntimeErrorV1) -> RuntimeResponseV1 {
    let manifest = model_error::render(&error.code);
    RuntimeResponseV1::rejected_with_error(request_id, error).with_error_manifest(manifest)
}

fn native_recovery_sentinel_error(detail: impl Into<String>) -> RuntimeErrorV1 {
    RuntimeErrorV1 {
        code: INTERNAL_NATIVE_RECOVERY_SENTINEL_FAILED_CODE.to_owned(),
        detail: detail.into(),
    }
}

fn append_engine_abort_failure(detail: String, abort: Option<RuntimeErrorV1>) -> String {
    match abort {
        Some(abort) => format!(
            "{detail}; Engine preview-resolution abort also failed: {}",
            abort.detail
        ),
        None => detail,
    }
}

fn artifact_handle_has_shape(value: &str) -> bool {
    value
        .strip_prefix("workbench-artifact-v1-")
        .is_some_and(|digest| {
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
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

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn root() -> RootBindingV1 {
        RootBindingV1 {
            root_id: format!("state-root-v2-{}", digest('d')),
            activation_id: format!("state-root-activation-v2-{}", digest('e')),
            action_space_id: "action-space-1".to_owned(),
            source_head_sha256: digest('f'),
        }
    }

    #[test]
    fn engine_cache_parks_exact_successor_and_can_reset() {
        let mut cache = RuntimeStateV1::default();
        assert!(cache.is_idle());
        cache.reset_to_neutral();
        assert!(cache.is_neutral());
        cache.park(root()).unwrap();
        let StableRuntimeV1::AtRoot { root: parked } = &cache.stable else {
            panic!("expected parked root")
        };
        assert_eq!(parked, &root());
        cache.reset_to_neutral();
        assert!(cache.is_neutral());
        cache.clear();
        assert!(cache.is_idle());
    }

    #[test]
    fn engine_cache_rejects_an_invalid_root_binding() {
        let mut cache = RuntimeStateV1::default();
        let mut invalid = root();
        invalid.activation_id = "not-content-addressed".to_owned();
        assert!(cache.park(invalid).is_err());
        assert!(cache.is_idle());
    }

    #[test]
    fn status_fails_closed_when_engine_cache_has_no_physical_owner() {
        let mut runtime = WorkbenchRuntimeV1::new();
        runtime.engine.park_runtime(root()).unwrap();
        let status = runtime.status();
        assert_eq!(status.phase, "inconsistent");
        assert_eq!(status.active_root_id, None);
        assert!(!status.reusable_runtime);
    }

    #[test]
    fn actor_returns_compact_status_and_stays_alive() {
        let mut runtime = WorkbenchRuntimeV1::new();
        let request = json!({
            "schema": REQUEST_SCHEMA_V1,
            "request_id": "request-1",
            "operation": {"kind": "service_status"}
        });
        let (response, shutdown) = runtime.handle_json(&request.to_string());
        assert!(!shutdown);
        assert_eq!(response.status, "ready");
        let status = response.result.unwrap();
        assert_eq!(status["phase"], "idle");
        assert_eq!(
            status["implementation_mode"],
            "long_lived_rust_actor_jsonl_v1"
        );
        assert_eq!(status["schema"], STATUS_SCHEMA_V2);
        assert!(status["build"]["artifact_freshness"].is_string());
        assert_eq!(runtime.status().request_count, 1);
    }

    #[test]
    fn stable_shell_forwards_unknown_domain_operations_without_parsing_them() {
        let mut runtime = WorkbenchRuntimeV1::new();
        let request = json!({
            "schema": REQUEST_SCHEMA_V1,
            "request_id": "future-operation-1",
            "operation": {
                "kind": "future_domain_operation",
                "nested": {"typed": true}
            }
        });

        let (response, shutdown) = runtime.handle_json(&request.to_string());

        assert!(!shutdown);
        assert_eq!(response.request_id, "future-operation-1");
        assert_eq!(response.status, "rejected");
        assert_eq!(
            response.error.as_ref().map(|error| error.code.as_str()),
            Some("engine_unavailable")
        );
        let manifest = response.error_manifest.as_deref().unwrap();
        assert!(manifest.starts_with("STS-ERROR 1\ncode=engine_unavailable\n"));
        assert!(manifest.contains("retry=after_build"));
        assert_eq!(runtime.status().request_count, 1);
    }

    #[test]
    fn rejected_response_separates_public_guidance_from_private_detail() {
        let response = rejected(
            "preview-1",
            "view_execution_in_progress",
            "private diagnostic at D:/workbench/runtime.json".to_owned(),
        );

        assert!(response
            .error
            .as_ref()
            .is_some_and(|error| error.detail.contains("D:/workbench")));
        let manifest = response.error_manifest.unwrap();
        assert!(manifest.contains("code=view_execution_in_progress"));
        assert!(manifest.contains("effect=official_head_unchanged"));
        assert!(manifest.contains("retry=forbidden"));
        assert!(!manifest.contains("D:/workbench"));
        assert!(!manifest.contains("service_status"));
    }

    #[test]
    fn natural_neow_bootstrap_is_typed_and_requires_the_live_runtime_closure() {
        let mut runtime = WorkbenchRuntimeV1::new();
        assert!(runtime
            .status()
            .capabilities
            .iter()
            .any(|capability| capability == "internal_bootstrap_natural_neow_parentless_root"));
        let request = json!({
            "schema": REQUEST_SCHEMA_V1,
            "request_id": "bootstrap-1",
            "operation": {
                "kind": "bootstrap_natural_neow",
                "seed": 2026090101_u64,
                "ascension": 20,
            }
        });
        let parsed: RuntimeRequestV1 = serde_json::from_value(request).unwrap();
        assert_eq!(
            parsed.operation.prerequisite(),
            Some(RuntimePrerequisiteV1::LiveRuntime)
        );
        let RuntimeOperationV1::BootstrapNaturalNeow { seed, ascension } = parsed.operation else {
            panic!("typed bootstrap request changed operation kind");
        };
        assert!(runtime.bootstrap_natural_neow(seed, ascension).is_err());
        assert_eq!(runtime.status().phase, "idle");
    }

    #[test]
    fn native_recovery_sentinel_is_internal_parameterless_and_requires_a_parked_root() {
        let mut runtime = WorkbenchRuntimeV1::new();
        assert!(runtime
            .status()
            .capabilities
            .iter()
            .any(|capability| capability == "internal_create_current_native_recovery_certificate"));
        let request = json!({
            "schema": REQUEST_SCHEMA_V1,
            "request_id": "native-recovery-sentinel-1",
            "operation": {
                "kind": "internal_create_current_native_recovery_certificate"
            }
        });
        let parsed: RuntimeRequestV1 = serde_json::from_value(request).unwrap();
        assert_eq!(
            parsed.operation.prerequisite(),
            Some(RuntimePrerequisiteV1::LiveRuntime)
        );
        assert!(matches!(
            parsed.operation,
            RuntimeOperationV1::InternalCreateCurrentNativeRecoveryCertificate {}
        ));
        let error = runtime
            .create_current_native_recovery_certificate()
            .unwrap_err();
        assert_eq!(error.code, INTERNAL_NATIVE_RECOVERY_SENTINEL_FAILED_CODE);
        assert_eq!(runtime.status().phase, "idle");

        let smuggled = json!({
            "kind": "internal_create_current_native_recovery_certificate",
            "root_id": "must-not-be-caller-controlled"
        });
        assert!(serde_json::from_value::<RuntimeOperationV1>(smuggled).is_err());
    }
}
