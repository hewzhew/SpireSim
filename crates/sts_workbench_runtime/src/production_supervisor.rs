use crate::{
    build_status, runtime_manager, runtime_ownership, RuntimeBuildStatusV1,
    RuntimeStartupReconciliationV1, RuntimeStateV1, StableRuntimeV1,
};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use sts_workbench_supervisor::{
    BusyOperationV1, EngineHostConfigurationV1, EngineHostErrorV1, EngineHostV1,
    EngineProcessErrorV1, ExchangeSettlementV1, NativeRecoveryCaptureScopeV1,
    ParkedRevalidationSettlementV1, PhysicalRuntimeCallV1, PhysicalRuntimeOperationV1,
    PhysicalRuntimeResultV1, SupervisorCallOutcomeV1, SupervisorCallV1, SupervisorModelV1,
    SupervisorPhaseV1, PHYSICAL_RUNTIME_CALL_SCHEMA_V1,
};

mod native_recovery;
#[cfg(test)]
use native_recovery::drive_native_recovery_callbacks;
pub(crate) use native_recovery::NativeRecoveryCandidateRetentionV1;

const ENGINE_IO_TIMEOUT: Duration = Duration::from_secs(5);
const NATIVE_RECOVERY_DIRECTORIES: &[&str] =
    sts_paired_root::NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1;

pub(super) struct EngineDomainFailureV1 {
    pub code: String,
    pub detail: String,
}

impl EngineDomainFailureV1 {
    fn new(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            detail: detail.into(),
        }
    }

    fn from_engine_host(context: &str, error: EngineHostErrorV1) -> Self {
        match error {
            EngineHostErrorV1::Process(EngineProcessErrorV1::Rejected { code }) => Self {
                detail: format!("{context} was rejected by the replaceable Engine: {code}"),
                code,
            },
            other => Self::new(
                "engine_operation_failed",
                format!("{context} failed: {other}"),
            ),
        }
    }
}

/// Production ownership half of the workbench process.
///
/// The Engine cache deliberately lives outside this type. This owner is the
/// single place where the Supervisor phase, the physical Java runtime, the
/// repository/process-containment lease, and the replaceable Engine process are
/// allowed to meet.
pub(super) struct ProductionSupervisorV1 {
    engine_host: EngineHostV1,
    model: SupervisorModelV1,
    actor_build: build_status::ActorBuildTrackerV1,
    physical_runtime: Option<runtime_manager::JavaPhysicalRuntimeV1>,
    native_recovery_scope: Option<NativeRecoveryCaptureScopeV1>,
    runtime_owner: Option<runtime_ownership::ActorRuntimeOwnerV1>,
    repository: Option<PathBuf>,
}

impl ProductionSupervisorV1 {
    pub(super) fn unowned() -> Self {
        Self {
            engine_host: EngineHostV1::default(),
            model: SupervisorModelV1::default(),
            actor_build: build_status::ActorBuildTrackerV1::capture(),
            physical_runtime: None,
            native_recovery_scope: None,
            runtime_owner: None,
            repository: None,
        }
    }

    pub(super) fn owned(repository: &Path) -> Result<Self, String> {
        let repository = std::fs::canonicalize(repository).map_err(|error| {
            format!(
                "failed to resolve production workbench repository {}: {error}",
                repository.display()
            )
        })?;
        Ok(Self {
            engine_host: EngineHostV1::default(),
            model: SupervisorModelV1::default(),
            actor_build: build_status::ActorBuildTrackerV1::capture(),
            physical_runtime: None,
            native_recovery_scope: None,
            runtime_owner: Some(runtime_ownership::ActorRuntimeOwnerV1::acquire(
                &repository,
            )?),
            repository: Some(repository),
        })
    }

    pub(super) fn admit_runtime(
        &mut self,
        runtime: runtime_manager::JavaPhysicalRuntimeV1,
    ) -> Result<(), String> {
        if !matches!(self.model.phase(), SupervisorPhaseV1::Idle)
            || self.physical_runtime.is_some()
            || self.native_recovery_scope.is_some()
            || self.model.runtime().is_some()
        {
            return Err("runtime admission requires an idle production Supervisor".to_owned());
        }

        let runtime_identity = runtime.supervisor_identity();
        let native_recovery_scope = runtime
            .bind_native_recovery_scope(NATIVE_RECOVERY_DIRECTORIES)
            .map_err(|error| format!("failed to bind native recovery scope: {error}"))?;
        self.ensure_engine()?;

        self.model
            .runtime_opened(runtime_identity)
            .map_err(supervisor_error)?;
        self.physical_runtime = Some(runtime);
        self.native_recovery_scope = Some(native_recovery_scope);
        Ok(())
    }

    pub(super) fn execute_engine(&mut self, input: Value) -> Result<Value, String> {
        self.execute_engine_with_timeout(input, ENGINE_IO_TIMEOUT)
    }

    pub(super) fn execute_engine_with_timeout(
        &mut self,
        input: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let configuration = self.engine_configuration()?;
        self.engine_host
            .execute(&mut self.model, &configuration, input, timeout)
            .map_err(|error| format!("Engine operation failed: {error}"))
    }

    pub(super) fn execute_engine_domain(
        &mut self,
        input: Value,
        timeout: Duration,
    ) -> Result<Value, EngineDomainFailureV1> {
        let configuration = self
            .engine_configuration()
            .map_err(|detail| EngineDomainFailureV1::new("engine_unavailable", detail))?;
        self.engine_host
            .execute(&mut self.model, &configuration, input, timeout)
            .map_err(|error| EngineDomainFailureV1::from_engine_host("Engine operation", error))
    }

    fn ensure_engine(&mut self) -> Result<u64, String> {
        let repository = self
            .repository
            .as_ref()
            .ok_or_else(|| "external Engine requires production Supervisor ownership".to_owned())?;
        build_status::ensure_engine_fresh(repository)?;
        let configuration = self.engine_configuration()?;
        self.engine_host
            .ensure_engine(&mut self.model, &configuration, ENGINE_IO_TIMEOUT)
            .map_err(|error| format!("failed to ensure production Engine: {error}"))
    }

    fn engine_configuration(&self) -> Result<EngineHostConfigurationV1, String> {
        let repository = self
            .repository
            .as_ref()
            .ok_or_else(|| "external Engine requires production Supervisor ownership".to_owned())?;
        let storage = self.storage_registry()?;
        let java_jar = storage.source_game_directory().join("desktop-1.0.jar");
        let mod_jar = storage
            .source_game_directory()
            .join("mods")
            .join("CommunicationMod.jar");
        let protocol = u16::try_from(sts_java_parity_protocol::PROTOCOL_MINOR)
            .map_err(|_| "Java protocol minor does not fit the Supervisor contract".to_owned())?;
        EngineHostConfigurationV1::new(
            repository,
            repository
                .join("target")
                .join("dev-opt")
                .join(if cfg!(windows) {
                    "sts_workbench_engine.exe"
                } else {
                    "sts_workbench_engine"
                }),
            repository
                .join(".oracle-lab")
                .join("workbench-runtime")
                .join("engine-images"),
            java_jar,
            mod_jar,
            protocol,
        )
        .map_err(|error| format!("invalid production Engine configuration: {error}"))
    }

    pub(super) fn shutdown(&mut self) -> Result<(), String> {
        let runtime_result = self.retire_runtime("actor_shutdown");
        let engine_result = self.shutdown_engine();
        match (runtime_result, engine_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(runtime), Ok(())) => Err(runtime),
            (Ok(()), Err(engine)) => Err(engine),
            (Err(runtime), Err(engine)) => {
                Err(format!("{runtime}; Engine shutdown also failed: {engine}"))
            }
        }
    }

    fn shutdown_engine(&mut self) -> Result<(), String> {
        self.engine_host
            .shutdown(&mut self.model, ENGINE_IO_TIMEOUT)
            .map_err(|error| format!("Engine shutdown failed: {error}"))
    }

    pub(super) fn begin_exchange(
        &mut self,
        engine_cache: &RuntimeStateV1,
        operation_id: &str,
    ) -> Result<(), String> {
        self.require_aligned_runtime()?;
        let engine_epoch = self.ensure_engine()?;
        self.revalidate_parked_runtime(engine_epoch, operation_id)
            .map_err(|error| error.detail)?;
        let expected_frame = match (&engine_cache.stable, self.model.phase()) {
            (StableRuntimeV1::Neutral, SupervisorPhaseV1::Neutral) => None,
            (StableRuntimeV1::AtRoot { .. }, SupervisorPhaseV1::Parked { frame_token, .. }) => {
                let runtime_frame = self
                    .physical_runtime
                    .as_ref()
                    .expect("aligned production runtime exists")
                    .accepted_frame_token();
                if runtime_frame != frame_token {
                    return Err(
                        "physical Java frame and production Supervisor boundary disagree"
                            .to_owned(),
                    );
                }
                Some(frame_token.clone())
            }
            _ => return Err("Engine cache and production Supervisor boundary disagree".to_owned()),
        };
        self.model
            .begin_exchange(engine_epoch, operation_id, expected_frame.as_deref())
            .map_err(supervisor_error)
    }

    /// Rebind a parked Java boundary to the current replaceable Engine before
    /// that Engine grants a pending A/W execution lease. Once alias resolution
    /// is pending, the Engine intentionally rejects unrelated lifecycle work.
    pub(super) fn revalidate_parked_before_view_execution(
        &mut self,
        operation_id: &str,
    ) -> Result<(), EngineDomainFailureV1> {
        if !self.has_runtime() {
            return Ok(());
        }
        self.require_aligned_runtime()
            .map_err(|detail| EngineDomainFailureV1::new("runtime_state_inconsistent", detail))?;
        let engine_epoch = self
            .ensure_engine()
            .map_err(|detail| EngineDomainFailureV1::new("engine_unavailable", detail))?;
        self.revalidate_parked_runtime(engine_epoch, operation_id)
    }

    fn revalidate_parked_runtime(
        &mut self,
        engine_epoch: u64,
        operation_id: &str,
    ) -> Result<(), EngineDomainFailureV1> {
        let (expected_frame, expected_autosave) = match self.model.phase() {
            SupervisorPhaseV1::Parked {
                frame_token,
                autosave_identity,
                validated_engine_epoch: None,
            } => (frame_token.clone(), autosave_identity.clone()),
            _ => return Ok(()),
        };
        let input = serde_json::to_value(
            sts_workbench_engine_protocol::EngineDomainRequestV1::revalidate_parked_runtime(),
        )
        .map_err(|error| {
            EngineDomainFailureV1::new(
                "engine_request_invalid",
                format!("failed to encode parked revalidation request: {error}"),
            )
        })?;
        let job_id = format!("revalidate-{operation_id}");
        self.model
            .begin_parked_revalidation(engine_epoch, &job_id, &expected_frame)
            .map_err(|error| {
                EngineDomainFailureV1::new("runtime_state_inconsistent", supervisor_error(error))
            })?;
        let mut callback_error = None;
        let execution = {
            let physical_runtime = &mut self.physical_runtime;
            self.engine_host.execute_bound_with_supervisor(
                &mut self.model,
                engine_epoch,
                input,
                ENGINE_IO_TIMEOUT,
                |call| {
                    Ok(
                        match dispatch_physical_runtime_call(
                            physical_runtime,
                            PhysicalRuntimeCapabilityV1::ValidateParked {
                                expected_frame: &expected_frame,
                                expected_autosave: &expected_autosave,
                            },
                            call,
                        ) {
                            Ok(outcome) => outcome,
                            Err(detail) => {
                                callback_error = Some(detail);
                                SupervisorCallOutcomeV1::Rejected {
                                    code: "parked_runtime_validation_failed".to_owned(),
                                }
                            }
                        },
                    )
                },
            )
        };

        match execution {
            Ok(output) => {
                let result = serde_json::from_value::<
                    sts_workbench_engine_protocol::EngineParkedRevalidationV1,
                >(output)
                .map_err(|error| {
                    EngineDomainFailureV1::new(
                        "engine_response_invalid",
                        format!("Engine returned an invalid parked revalidation result: {error}"),
                    )
                });
                let verified = callback_error.is_none()
                    && result.as_ref().is_ok_and(|result| {
                        result.schema
                            == sts_workbench_engine_protocol::ENGINE_PARKED_REVALIDATION_SCHEMA_V1
                            && result.status
                                == sts_workbench_engine_protocol::EngineParkedRevalidationStatusV1::Validated
                    });
                self.model
                    .complete_parked_revalidation(
                        engine_epoch,
                        &job_id,
                        if verified {
                            ParkedRevalidationSettlementV1::Verified
                        } else {
                            ParkedRevalidationSettlementV1::Rejected
                        },
                    )
                    .map_err(|error| {
                        EngineDomainFailureV1::new(
                            "runtime_state_inconsistent",
                            supervisor_error(error),
                        )
                    })?;
                if !verified {
                    return Err(if let Some(detail) = callback_error {
                        EngineDomainFailureV1::new("parked_runtime_validation_failed", detail)
                    } else if let Some(error) = result.err() {
                        error
                    } else {
                        EngineDomainFailureV1::new(
                            "engine_response_invalid",
                            "Engine returned a mismatched parked revalidation result",
                        )
                    });
                }
                Ok(())
            }
            Err(error) => {
                if matches!(
                    self.model.phase(),
                    SupervisorPhaseV1::Busy { job_id: active, .. } if active == &job_id
                ) {
                    self.model
                        .complete_parked_revalidation(
                            engine_epoch,
                            &job_id,
                            ParkedRevalidationSettlementV1::Rejected,
                        )
                        .map_err(|settlement| {
                            EngineDomainFailureV1::new(
                                "runtime_state_inconsistent",
                                supervisor_error(settlement),
                            )
                        })?;
                }
                Err(if let Some(detail) = callback_error {
                    EngineDomainFailureV1::new("parked_runtime_validation_failed", detail)
                } else {
                    EngineDomainFailureV1::from_engine_host("Engine parked revalidation", error)
                })
            }
        }
    }

    pub(super) fn park_exchange(&mut self, operation_id: &str) -> Result<(), String> {
        self.require_aligned_runtime()?;
        let runtime = self
            .physical_runtime
            .as_ref()
            .expect("aligned production runtime exists");
        let frame_token = runtime.accepted_frame_token().to_owned();
        let autosave_identity = runtime.autosave_sha256().to_owned();
        self.model
            .complete_exchange(
                self.engine_epoch()?,
                operation_id,
                ExchangeSettlementV1::Parked {
                    frame_token,
                    autosave_identity,
                },
            )
            .map_err(supervisor_error)
    }

    pub(super) fn reset_to_native(
        &mut self,
        operation_id: &str,
        request_count: u64,
    ) -> Result<u128, String> {
        self.require_aligned_runtime()?;
        let engine_epoch = self.engine_epoch()?;
        if !matches!(
            self.model.phase(),
            SupervisorPhaseV1::Busy {
                job_id,
                engine_epoch: active_epoch,
                operation: BusyOperationV1::Exchange { .. },
            } if job_id == operation_id && *active_epoch == engine_epoch
        ) {
            return Err("natural native reset requires its active production exchange".to_owned());
        }
        let elapsed = self.execute_reset_to_native_callback(
            engine_epoch,
            request_count,
            runtime_manager::ResetFrameTrustV1::Accepted,
        )?;
        self.model
            .complete_exchange(engine_epoch, operation_id, ExchangeSettlementV1::Neutral)
            .map_err(supervisor_error)?;
        Ok(elapsed)
    }

    pub(super) fn recover_dirty_to_native(
        &mut self,
        operation_id: &str,
        request_count: u64,
    ) -> Result<u128, String> {
        self.require_aligned_runtime()?;
        if !matches!(self.model.phase(), SupervisorPhaseV1::Dirty { .. }) {
            return Err("native recovery requires a Dirty production runtime".to_owned());
        }
        let engine_epoch = self.ensure_engine()?;
        self.model
            .begin_recovery(engine_epoch, operation_id)
            .map_err(supervisor_error)?;
        match self.execute_reset_to_native_callback(
            engine_epoch,
            request_count,
            runtime_manager::ResetFrameTrustV1::Dirty,
        ) {
            Ok(elapsed) => {
                self.model
                    .complete_recovery(
                        engine_epoch,
                        operation_id,
                        sts_workbench_supervisor::RecoverySettlementV1::Neutral,
                    )
                    .map_err(supervisor_error)?;
                Ok(elapsed)
            }
            Err(error) => {
                if matches!(
                    self.model.phase(),
                    SupervisorPhaseV1::Busy {
                        job_id,
                        engine_epoch: active_epoch,
                        operation: BusyOperationV1::Recovery { .. },
                    } if job_id == operation_id && *active_epoch == engine_epoch
                ) {
                    self.model
                        .complete_recovery(
                            engine_epoch,
                            operation_id,
                            sts_workbench_supervisor::RecoverySettlementV1::Dirty {
                                reason: "native_reset_recovery_failed",
                            },
                        )
                        .map_err(supervisor_error)?;
                }
                Err(error)
            }
        }
    }

    fn execute_reset_to_native_callback(
        &mut self,
        engine_epoch: u64,
        request_count: u64,
        frame_trust: runtime_manager::ResetFrameTrustV1,
    ) -> Result<u128, String> {
        let input = serde_json::to_value(
            sts_workbench_engine_protocol::EngineDomainRequestV1::reset_to_native(),
        )
        .map_err(|error| format!("failed to encode natural native reset request: {error}"))?;
        let started = Instant::now();
        let mut callback_error = None;
        let execution = {
            let physical_runtime = &mut self.physical_runtime;
            self.engine_host.execute_bound_with_supervisor(
                &mut self.model,
                engine_epoch,
                input,
                ENGINE_IO_TIMEOUT,
                |call| {
                    Ok(
                        match dispatch_physical_runtime_call(
                            physical_runtime,
                            PhysicalRuntimeCapabilityV1::ResetToNative {
                                request_count,
                                frame_trust,
                            },
                            call,
                        ) {
                            Ok(outcome) => outcome,
                            Err(detail) => {
                                callback_error = Some(detail);
                                SupervisorCallOutcomeV1::Rejected {
                                    code: "native_reset_failed".to_owned(),
                                }
                            }
                        },
                    )
                },
            )
        };
        let output = execution.map_err(|error| {
            callback_error.unwrap_or_else(|| format!("Engine natural native reset failed: {error}"))
        })?;
        let result =
            serde_json::from_value::<sts_workbench_engine_protocol::EngineNativeResetV1>(output)
                .map_err(|error| {
                    format!("Engine returned an invalid natural native reset: {error}")
                })?;
        if result.schema != sts_workbench_engine_protocol::ENGINE_NATIVE_RESET_SCHEMA_V1
            || result.status != sts_workbench_engine_protocol::EngineNativeResetStatusV1::Reset
        {
            return Err("Engine returned a mismatched natural native reset".to_owned());
        }
        Ok(started.elapsed().as_millis())
    }

    pub(super) fn retire_runtime(&mut self, _reason: &str) -> Result<(), String> {
        match self.model.phase().clone() {
            SupervisorPhaseV1::Idle => {
                if self.physical_runtime.is_some()
                    || self.native_recovery_scope.is_some()
                    || self.model.runtime().is_some()
                {
                    return Err(
                        "idle production Supervisor retained a Java runtime owner".to_owned()
                    );
                }
                return Ok(());
            }
            SupervisorPhaseV1::Retiring { .. } => {}
            SupervisorPhaseV1::Busy {
                job_id,
                engine_epoch,
                operation: BusyOperationV1::Exchange { .. },
            } => self
                .model
                .complete_exchange(
                    engine_epoch,
                    &job_id,
                    ExchangeSettlementV1::Retire {
                        reason: "production_exchange_requires_retirement",
                    },
                )
                .map_err(supervisor_error)?,
            SupervisorPhaseV1::Busy {
                job_id,
                engine_epoch,
                operation: BusyOperationV1::NativeRecoveryRoundTrip { .. },
            } => self
                .model
                .complete_native_recovery_round_trip(
                    engine_epoch,
                    &job_id,
                    sts_workbench_supervisor::NativeRecoveryRoundTripSettlementV1::Retire {
                        reason: "production_native_recovery_requires_retirement",
                    },
                )
                .map_err(supervisor_error)?,
            SupervisorPhaseV1::Busy { .. } => {
                return Err(
                    "production runtime cannot retire another Supervisor operation".to_owned(),
                )
            }
            _ => self
                .model
                .begin_retire("production_runtime_retirement")
                .map_err(supervisor_error)?,
        }

        let runtime = self.physical_runtime.as_mut().ok_or_else(|| {
            "retiring production Supervisor lost its Java runtime owner".to_owned()
        })?;
        runtime.shutdown()?;
        self.model.complete_retirement().map_err(supervisor_error)?;
        self.physical_runtime = None;
        self.native_recovery_scope = None;
        Ok(())
    }

    fn engine_epoch(&self) -> Result<u64, String> {
        self.model
            .engine()
            .map(|engine| engine.epoch)
            .ok_or_else(|| "production Supervisor has no admitted Engine".to_owned())
    }

    fn require_aligned_runtime(&self) -> Result<(), String> {
        let runtime = self
            .physical_runtime
            .as_ref()
            .ok_or_else(|| "production Supervisor has no physical Java runtime".to_owned())?;
        let identity = self.model.runtime().ok_or_else(|| {
            "production Supervisor omitted the admitted Java runtime identity".to_owned()
        })?;
        if runtime.supervisor_identity() != *identity {
            return Err(
                "physical Java runtime and production Supervisor identity disagree".to_owned(),
            );
        }
        if self.native_recovery_scope.is_none() {
            return Err("production Supervisor omitted the admitted recovery scope".to_owned());
        }
        Ok(())
    }

    pub(super) fn phase(
        &self,
        engine_cache: &RuntimeStateV1,
        engine_runtime_present: bool,
    ) -> &'static str {
        match self.model.phase() {
            SupervisorPhaseV1::Idle
                if engine_cache.is_idle()
                    && !engine_runtime_present
                    && self.physical_runtime.is_none()
                    && self.native_recovery_scope.is_none()
                    && self.model.runtime().is_none() =>
            {
                "idle"
            }
            SupervisorPhaseV1::Neutral
                if engine_runtime_present
                    && engine_cache.is_neutral()
                    && self.require_aligned_runtime().is_ok() =>
            {
                "neutral"
            }
            SupervisorPhaseV1::Parked {
                validated_engine_epoch,
                ..
            } if matches!(engine_cache.stable, StableRuntimeV1::AtRoot { .. })
                && engine_runtime_present
                && *validated_engine_epoch == self.model.engine().map(|engine| engine.epoch)
                && self.require_aligned_runtime().is_ok() =>
            {
                "at_root"
            }
            SupervisorPhaseV1::Busy { .. } => "busy",
            SupervisorPhaseV1::Dirty { .. } => "dirty",
            SupervisorPhaseV1::Retiring { .. } => "retiring",
            SupervisorPhaseV1::Idle
            | SupervisorPhaseV1::Neutral
            | SupervisorPhaseV1::Parked { .. } => "inconsistent",
        }
    }

    pub(super) fn reusable(
        &self,
        engine_cache: &RuntimeStateV1,
        engine_runtime_present: bool,
    ) -> bool {
        if !engine_runtime_present || self.require_aligned_runtime().is_err() {
            return false;
        }
        match (&engine_cache.stable, self.model.phase()) {
            (StableRuntimeV1::Neutral, SupervisorPhaseV1::Neutral) => true,
            (
                StableRuntimeV1::AtRoot { .. },
                SupervisorPhaseV1::Parked {
                    validated_engine_epoch: Some(validated),
                    ..
                },
            ) => self
                .model
                .engine()
                .is_some_and(|engine| engine.epoch == *validated),
            _ => false,
        }
    }

    pub(super) fn is_idle(&self) -> bool {
        matches!(self.model.phase(), SupervisorPhaseV1::Idle)
            && self.physical_runtime.is_none()
            && self.native_recovery_scope.is_none()
            && self.model.runtime().is_none()
    }

    pub(super) fn is_neutral(&self) -> bool {
        matches!(self.model.phase(), SupervisorPhaseV1::Neutral)
            && self.require_aligned_runtime().is_ok()
    }

    pub(super) fn has_runtime(&self) -> bool {
        self.physical_runtime.is_some()
    }

    pub(super) fn is_dirty(&self) -> bool {
        matches!(self.model.phase(), SupervisorPhaseV1::Dirty { .. })
            && self.physical_runtime.is_some()
            && self.model.runtime().is_some()
    }

    pub(super) fn native_recovery_failure_requires_retirement(&self) -> bool {
        matches!(
            self.model.phase(),
            SupervisorPhaseV1::Busy {
                operation: BusyOperationV1::NativeRecoveryRoundTrip { .. },
                ..
            } | SupervisorPhaseV1::Dirty { .. }
                | SupervisorPhaseV1::Retiring { .. }
        )
    }

    pub(super) fn runtime_mut(&mut self) -> Option<&mut runtime_manager::JavaPhysicalRuntimeV1> {
        self.physical_runtime.as_mut()
    }

    pub(super) fn ownership_mode(&self) -> &'static str {
        if self.runtime_owner.is_some() {
            "exclusive_repository_lease_windows_job_v1"
        } else {
            "unowned_in_process"
        }
    }

    pub(super) fn startup_reconciliation(&self) -> Option<&RuntimeStartupReconciliationV1> {
        self.runtime_owner
            .as_ref()
            .map(runtime_ownership::ActorRuntimeOwnerV1::startup_reconciliation)
    }

    pub(super) fn storage_registry(
        &self,
    ) -> Result<sts_workbench_supervisor::OwnedStorageRegistryV1, String> {
        self.runtime_owner
            .as_ref()
            .map(runtime_ownership::ActorRuntimeOwnerV1::storage_registry)
            .ok_or_else(|| {
                "Java preview requires an actor-owned process-containment lease".to_owned()
            })
    }

    pub(super) fn build_status(&self, repository: &Path) -> RuntimeBuildStatusV1 {
        self.actor_build.status(repository)
    }
}

fn dispatch_physical_runtime_call(
    runtime: &mut Option<runtime_manager::JavaPhysicalRuntimeV1>,
    capability: PhysicalRuntimeCapabilityV1<'_>,
    call: SupervisorCallV1,
) -> Result<SupervisorCallOutcomeV1, String> {
    let request = serde_json::from_value::<PhysicalRuntimeCallV1>(call.input)
        .map_err(|error| format!("invalid physical runtime callback: {error}"))?;
    if request.schema != PHYSICAL_RUNTIME_CALL_SCHEMA_V1 {
        return Err("physical runtime callback schema mismatch".to_owned());
    }
    match (capability, request.operation) {
        (
            PhysicalRuntimeCapabilityV1::ValidateParked {
                expected_frame,
                expected_autosave,
            },
            PhysicalRuntimeOperationV1::ValidateParked {},
        ) => {
            runtime
                .as_mut()
                .ok_or_else(|| "parked validation omitted its physical Java runtime".to_owned())?
                .validate_parked_binding(expected_frame, expected_autosave)?;
            let output = serde_json::to_value(PhysicalRuntimeResultV1::parked_validated())
                .map_err(|error| format!("failed to encode physical runtime result: {error}"))?;
            Ok(SupervisorCallOutcomeV1::Completed { output })
        }
        (
            PhysicalRuntimeCapabilityV1::ResetToNative {
                request_count,
                frame_trust,
            },
            PhysicalRuntimeOperationV1::ResetToNative {},
        ) => {
            runtime
                .as_mut()
                .ok_or_else(|| "natural native reset omitted its physical Java runtime".to_owned())?
                .reset_to_native(request_count, frame_trust)?;
            let output = serde_json::to_value(PhysicalRuntimeResultV1::native_reset())
                .map_err(|error| format!("failed to encode physical runtime result: {error}"))?;
            Ok(SupervisorCallOutcomeV1::Completed { output })
        }
        _ => Err("physical runtime callback exceeded its operation capability".to_owned()),
    }
}

enum PhysicalRuntimeCapabilityV1<'a> {
    ValidateParked {
        expected_frame: &'a str,
        expected_autosave: &'a str,
    },
    ResetToNative {
        request_count: u64,
        frame_trust: runtime_manager::ResetFrameTrustV1,
    },
}

fn supervisor_error(error: impl std::fmt::Debug) -> String {
    format!("Supervisor rejected runtime transition: {error:?}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[cfg(windows)]
    use std::{cell::Cell, fs, rc::Rc};
    #[cfg(windows)]
    use sts_workbench_supervisor::{
        NativeRecoveryCapsuleJobV1, NativeRecoveryCapsuleRegistryV1, NativeRecoveryValidatorOpenV1,
        NativeRecoveryValidatorV1, OwnedStorageRegistryV1, OwnedStorageV1,
        PhysicalRuntimeOutcomeV1, RepositoryLifecycleOwnerV1,
    };
    #[cfg(windows)]
    use uuid::Uuid;

    #[test]
    fn engine_domain_failure_preserves_a_closed_engine_rejection_code() {
        let failure = EngineDomainFailureV1::from_engine_host(
            "parked revalidation",
            EngineHostErrorV1::Process(EngineProcessErrorV1::Rejected {
                code: "view_execution_in_progress".to_owned(),
            }),
        );

        assert_eq!(failure.code, "view_execution_in_progress");
        assert!(failure.detail.contains("parked revalidation"));
    }

    #[test]
    fn engine_domain_failure_keeps_transport_failures_generic() {
        let failure = EngineDomainFailureV1::from_engine_host(
            "parked revalidation",
            EngineHostErrorV1::Process(EngineProcessErrorV1::Disconnected),
        );

        assert_eq!(failure.code, "engine_operation_failed");
        assert!(failure.detail.contains("Disconnected"));
    }

    #[cfg(windows)]
    #[derive(Debug)]
    struct FakeNativeValidatorV1(Rc<Cell<usize>>);

    #[cfg(windows)]
    impl NativeRecoveryValidatorV1 for FakeNativeValidatorV1 {
        fn shutdown(&mut self) -> Result<(), String> {
            self.0.set(self.0.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn production_dispatch_rejects_untyped_or_absent_physical_runtime() {
        let mut runtime = None;
        let error = dispatch_physical_runtime_call(
            &mut runtime,
            PhysicalRuntimeCapabilityV1::ValidateParked {
                expected_frame: "frame-1",
                expected_autosave: "autosave-1",
            },
            SupervisorCallV1 {
                call_id: "call-1".to_owned(),
                input: json!({"kind": "untyped_escape"}),
            },
        )
        .unwrap_err();
        assert!(error.contains("invalid physical runtime callback"));

        let error = dispatch_physical_runtime_call(
            &mut runtime,
            PhysicalRuntimeCapabilityV1::ValidateParked {
                expected_frame: "frame-1",
                expected_autosave: "autosave-1",
            },
            SupervisorCallV1 {
                call_id: "call-2".to_owned(),
                input: serde_json::to_value(PhysicalRuntimeCallV1::validate_parked()).unwrap(),
            },
        )
        .unwrap_err();
        assert_eq!(error, "parked validation omitted its physical Java runtime");

        let error = dispatch_physical_runtime_call(
            &mut runtime,
            PhysicalRuntimeCapabilityV1::ValidateParked {
                expected_frame: "frame-1",
                expected_autosave: "autosave-1",
            },
            SupervisorCallV1 {
                call_id: "call-3".to_owned(),
                input: serde_json::to_value(PhysicalRuntimeCallV1::reset_to_native()).unwrap(),
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            "physical runtime callback exceeded its operation capability"
        );

        let error = dispatch_physical_runtime_call(
            &mut runtime,
            PhysicalRuntimeCapabilityV1::ResetToNative {
                request_count: 7,
                frame_trust: runtime_manager::ResetFrameTrustV1::Accepted,
            },
            SupervisorCallV1 {
                call_id: "call-4".to_owned(),
                input: serde_json::to_value(PhysicalRuntimeCallV1::reset_to_native()).unwrap(),
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            "natural native reset omitted its physical Java runtime"
        );
    }

    #[cfg(windows)]
    #[test]
    fn production_dispatch_owns_the_fake_native_capsule_round_trip() {
        let base =
            std::env::temp_dir().join(format!("sts-production-native-recovery-{}", Uuid::new_v4()));
        let repository = base.join("repository");
        let game = base.join("game");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(&game).unwrap();
        let owner = RepositoryLifecycleOwnerV1::acquire(&repository).unwrap();
        let storage_registry = OwnedStorageRegistryV1::new(&owner, &game);
        let mut source = storage_registry.create().unwrap();
        for relative in NATIVE_RECOVERY_DIRECTORIES {
            fs::create_dir(source.root().join(relative)).unwrap();
        }
        fs::write(source.root().join("preferences/prefs"), b"prefs").unwrap();
        fs::write(source.root().join("saves/save.autosave"), b"save").unwrap();
        let scope = source
            .bind_native_recovery_scope(NATIVE_RECOVERY_DIRECTORIES)
            .unwrap();
        let capsules = NativeRecoveryCapsuleRegistryV1::new(
            &owner,
            storage_registry.clone(),
            &repository.join(".oracle-lab/paired-root-store"),
        )
        .unwrap();
        let mut job = NativeRecoveryCapsuleJobV1::new(capsules.clone(), scope, 7, "job-1").unwrap();
        let guards = Cell::new(0);
        let shutdowns = Rc::new(Cell::new(0));
        let mut validate_primary = || {
            guards.set(guards.get() + 1);
            Ok(())
        };
        let mut open_validator = |payload: &Path| {
            assert_eq!(
                fs::read(payload.join("saves/save.autosave")).unwrap(),
                b"save"
            );
            Ok(NativeRecoveryValidatorOpenV1::new(
                FakeNativeValidatorV1(Rc::clone(&shutdowns)),
                format!(
                    "workbench-native-recovery-observation-v1-{}",
                    "a".repeat(64)
                ),
            ))
        };
        let driven = drive_native_recovery_callbacks::<_, String>(
            7,
            "job-1",
            &mut job,
            &mut validate_primary,
            &mut open_validator,
            |handler| {
                let begin = handler(SupervisorCallV1 {
                    call_id: "call-1".to_owned(),
                    input: serde_json::to_value(
                        PhysicalRuntimeCallV1::begin_native_recovery_validation(),
                    )
                    .unwrap(),
                })?;
                let SupervisorCallOutcomeV1::Completed { output } = begin else {
                    return Err("native recovery begin was rejected".to_owned());
                };
                assert!(matches!(
                    serde_json::from_value::<PhysicalRuntimeResultV1>(output)
                        .unwrap()
                        .outcome,
                    PhysicalRuntimeOutcomeV1::NativeRecoveryValidatorReady { .. }
                ));
                let sealed = handler(SupervisorCallV1 {
                    call_id: "call-2".to_owned(),
                    input: serde_json::to_value(
                        PhysicalRuntimeCallV1::seal_native_recovery_validation(),
                    )
                    .unwrap(),
                })?;
                let SupervisorCallOutcomeV1::Completed { output } = sealed else {
                    return Err("native recovery seal was rejected".to_owned());
                };
                Ok(output)
            },
        );
        assert!(driven.cleanup.is_ok());
        assert!(driven.primary.is_ok());
        let recovery_ref =
            match serde_json::from_value::<PhysicalRuntimeResultV1>(driven.execution.unwrap())
                .unwrap()
                .outcome
            {
                PhysicalRuntimeOutcomeV1::NativeRecoverySealed { recovery_ref } => recovery_ref,
                other => panic!("unexpected native recovery result: {other:?}"),
            };
        let restored = base.join("restored");
        capsules
            .materialize_capsule_new(&recovery_ref, &restored)
            .unwrap();
        assert_eq!(guards.get(), 7);
        assert_eq!(shutdowns.get(), 1);
        assert_eq!(
            fs::read(restored.join("preferences/prefs")).unwrap(),
            b"prefs"
        );
        assert!(restored.join("betaPreferences").is_dir());

        drop(job);
        source.cleanup().unwrap();
        drop(capsules);
        drop(storage_registry);
        drop(owner);
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn recovery_driver_aborts_after_projection_divergence_or_launcher_failure() {
        let fixture = NativeRecoveryDriverFixtureV1::new("abort");

        let shutdowns = Rc::new(Cell::new(0));
        let guards = Cell::new(0);
        let mut divergence_job = fixture.job("job-divergence");
        let mut validate_primary = || {
            guards.set(guards.get() + 1);
            Ok(())
        };
        let mut open_validator = |_: &Path| {
            Ok(NativeRecoveryValidatorOpenV1::new(
                FakeNativeValidatorV1(Rc::clone(&shutdowns)),
                observation_ref('b'),
            ))
        };
        let driven = drive_native_recovery_callbacks::<_, String>(
            7,
            "job-divergence",
            &mut divergence_job,
            &mut validate_primary,
            &mut open_validator,
            |handler| {
                assert!(matches!(
                    handler(begin_native_recovery_call())?,
                    SupervisorCallOutcomeV1::Completed { .. }
                ));
                Err("fake projection divergence".to_owned())
            },
        );
        assert_eq!(driven.execution.unwrap_err(), "fake projection divergence");
        assert!(driven.cleanup.is_ok());
        assert!(driven.primary.is_ok());
        assert_eq!(guards.get(), 4);
        assert_eq!(shutdowns.get(), 1);
        assert_eq!(fixture.owned_storage_count(), 1);

        let guards = Cell::new(0);
        let mut launcher_job = fixture.job("job-launcher");
        let mut validate_primary = || {
            guards.set(guards.get() + 1);
            Ok(())
        };
        let mut fail_launcher = |_: &Path| Err("fake launcher failed".to_owned());
        let driven = drive_native_recovery_callbacks::<_, String>(
            7,
            "job-launcher",
            &mut launcher_job,
            &mut validate_primary,
            &mut fail_launcher,
            |handler| {
                assert!(matches!(
                    handler(begin_native_recovery_call())?,
                    SupervisorCallOutcomeV1::Rejected { .. }
                ));
                Ok(json!({"status": "must_not_override_callback_rejection"}))
            },
        );
        assert!(driven
            .execution
            .unwrap_err()
            .contains("fake launcher failed"));
        assert!(driven.cleanup.is_ok());
        assert!(driven.primary.is_ok());
        assert_eq!(guards.get(), 3);
        assert_eq!(fixture.owned_storage_count(), 1);

        fixture.cleanup();
    }

    #[cfg(windows)]
    #[test]
    fn recovery_driver_reports_primary_drift_after_a_completed_capsule() {
        let fixture = NativeRecoveryDriverFixtureV1::new("primary-drift");
        let mut job = fixture.job("job-primary-drift");
        let shutdowns = Rc::new(Cell::new(0));
        let guards = Cell::new(0);
        let mut validate_primary = || {
            guards.set(guards.get() + 1);
            if guards.get() == 7 {
                Err("fake primary drift".to_owned())
            } else {
                Ok(())
            }
        };
        let mut open_validator = |_: &Path| {
            Ok(NativeRecoveryValidatorOpenV1::new(
                FakeNativeValidatorV1(Rc::clone(&shutdowns)),
                observation_ref('c'),
            ))
        };

        let driven = drive_native_recovery_callbacks::<_, String>(
            7,
            "job-primary-drift",
            &mut job,
            &mut validate_primary,
            &mut open_validator,
            |handler| {
                assert!(matches!(
                    handler(begin_native_recovery_call())?,
                    SupervisorCallOutcomeV1::Completed { .. }
                ));
                assert!(matches!(
                    handler(seal_native_recovery_call())?,
                    SupervisorCallOutcomeV1::Completed { .. }
                ));
                Ok(json!({"status": "published"}))
            },
        );

        assert!(driven.execution.is_ok());
        assert!(driven.cleanup.is_ok());
        assert_eq!(driven.primary.unwrap_err(), "fake primary drift");
        assert_eq!(shutdowns.get(), 1);
        assert_eq!(fixture.owned_storage_count(), 1);
        fixture.cleanup();
    }

    #[cfg(windows)]
    struct NativeRecoveryDriverFixtureV1 {
        base: PathBuf,
        _owner: RepositoryLifecycleOwnerV1,
        storage_registry: OwnedStorageRegistryV1,
        source: OwnedStorageV1,
        scope: NativeRecoveryCaptureScopeV1,
        capsules: NativeRecoveryCapsuleRegistryV1,
    }

    #[cfg(windows)]
    impl NativeRecoveryDriverFixtureV1 {
        fn new(label: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "sts-production-native-recovery-driver-{label}-{}",
                Uuid::new_v4()
            ));
            let repository = base.join("repository");
            let game = base.join("game");
            fs::create_dir_all(&repository).unwrap();
            fs::create_dir_all(&game).unwrap();
            let owner = RepositoryLifecycleOwnerV1::acquire(&repository).unwrap();
            let storage_registry = OwnedStorageRegistryV1::new(&owner, &game);
            let source = storage_registry.create().unwrap();
            for relative in NATIVE_RECOVERY_DIRECTORIES {
                fs::create_dir(source.root().join(relative)).unwrap();
            }
            fs::write(source.root().join("preferences/prefs"), b"prefs").unwrap();
            fs::write(source.root().join("saves/save.autosave"), b"save").unwrap();
            let scope = source
                .bind_native_recovery_scope(NATIVE_RECOVERY_DIRECTORIES)
                .unwrap();
            let capsules = NativeRecoveryCapsuleRegistryV1::new(
                &owner,
                storage_registry.clone(),
                &repository.join(".oracle-lab/paired-root-store"),
            )
            .unwrap();
            Self {
                base,
                _owner: owner,
                storage_registry,
                source,
                scope,
                capsules,
            }
        }

        fn job(&self, job_id: &str) -> NativeRecoveryCapsuleJobV1 {
            NativeRecoveryCapsuleJobV1::new(self.capsules.clone(), self.scope.clone(), 7, job_id)
                .unwrap()
        }

        fn owned_storage_count(&self) -> usize {
            fs::read_dir(
                self.storage_registry
                    .source_game_directory()
                    .join(".sts-java-play/isolated-storage"),
            )
            .map(|entries| entries.count())
            .unwrap_or(0)
        }

        fn cleanup(mut self) {
            self.source.cleanup().unwrap();
            let base = self.base.clone();
            drop(self);
            fs::remove_dir_all(base).unwrap();
        }
    }

    #[cfg(windows)]
    fn begin_native_recovery_call() -> SupervisorCallV1 {
        SupervisorCallV1 {
            call_id: "begin".to_owned(),
            input: serde_json::to_value(PhysicalRuntimeCallV1::begin_native_recovery_validation())
                .unwrap(),
        }
    }

    #[cfg(windows)]
    fn seal_native_recovery_call() -> SupervisorCallV1 {
        SupervisorCallV1 {
            call_id: "seal".to_owned(),
            input: serde_json::to_value(PhysicalRuntimeCallV1::seal_native_recovery_validation())
                .unwrap(),
        }
    }

    #[cfg(windows)]
    fn observation_ref(fill: char) -> String {
        format!(
            "workbench-native-recovery-observation-v1-{}",
            fill.to_string().repeat(64)
        )
    }
}
