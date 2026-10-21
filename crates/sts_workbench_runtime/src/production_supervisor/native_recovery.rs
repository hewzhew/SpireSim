use super::{supervisor_error, EngineDomainFailureV1, ProductionSupervisorV1, ENGINE_IO_TIMEOUT};
use crate::native_recovery_validator;
use crate::INTERNAL_NATIVE_RECOVERY_SENTINEL_FAILED_CODE;
use serde_json::Value;
use std::path::Path;
use std::time::Instant;
use sts_workbench_supervisor::{
    BusyOperationV1, EngineHostErrorV1, EngineProcessErrorV1, NativeRecoveryCallbackPhaseV1,
    NativeRecoveryCapsuleJobV1, NativeRecoveryRoundTripSettlementV1, NativeRecoveryValidatorOpenV1,
    PhysicalRuntimeCallV1, PhysicalRuntimeOperationV1, SupervisorCallOutcomeV1, SupervisorCallV1,
    SupervisorPhaseV1, PHYSICAL_RUNTIME_CALL_SCHEMA_V1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeRecoveryCandidateRetentionV1 {
    Retained { elapsed_ms: u128 },
    Ineligible,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeRecoveryFailureV1 {
    code: String,
    detail: String,
}

impl NativeRecoveryFailureV1 {
    pub(super) fn generic(detail: impl Into<String>) -> Self {
        Self {
            code: INTERNAL_NATIVE_RECOVERY_SENTINEL_FAILED_CODE.to_owned(),
            detail: detail.into(),
        }
    }

    fn from_engine(context: &str, error: EngineHostErrorV1) -> Self {
        match error {
            EngineHostErrorV1::Process(EngineProcessErrorV1::Rejected { code }) => Self {
                code,
                detail: format!("{context} was rejected by the replaceable Engine"),
            },
            other => Self::generic(format!("{context} failed: {other}")),
        }
    }

    pub(crate) fn into_parts(self) -> (String, String) {
        (self.code, self.detail)
    }
}

impl From<String> for NativeRecoveryFailureV1 {
    fn from(detail: String) -> Self {
        Self::generic(detail)
    }
}

impl From<EngineDomainFailureV1> for NativeRecoveryFailureV1 {
    fn from(error: EngineDomainFailureV1) -> Self {
        Self {
            code: error.code,
            detail: error.detail,
        }
    }
}

impl ProductionSupervisorV1 {
    pub(crate) fn retain_native_recovery_candidate(
        &mut self,
        job_id: &str,
        target_root_id: &str,
        captured_activation_id: &str,
    ) -> Result<NativeRecoveryCandidateRetentionV1, NativeRecoveryFailureV1> {
        self.require_aligned_runtime()?;
        let engine_epoch = self.ensure_engine()?;
        self.revalidate_parked_runtime(engine_epoch, job_id)?;
        let (expected_frame, expected_autosave) = match self.model.phase() {
            SupervisorPhaseV1::Parked {
                frame_token,
                autosave_identity,
                validated_engine_epoch: Some(validated),
            } if *validated == engine_epoch => (frame_token.clone(), autosave_identity.clone()),
            _ => {
                return Err(NativeRecoveryFailureV1::generic(
                    "native recovery candidate requires one Engine-validated parked runtime",
                ))
            }
        };
        self.physical_runtime
            .as_mut()
            .expect("aligned production runtime exists")
            .validate_parked_binding(&expected_frame, &expected_autosave)?;
        match self.preflight_native_recovery_candidate(
            engine_epoch,
            target_root_id,
            captured_activation_id,
        ) {
            Ok(()) => {}
            Err(error) if error.code == "native_recovery_candidate_ineligible" => {
                return Ok(NativeRecoveryCandidateRetentionV1::Ineligible)
            }
            Err(error) => return Err(error),
        }
        self.physical_runtime
            .as_mut()
            .expect("aligned production runtime exists")
            .validate_parked_binding(&expected_frame, &expected_autosave)?;

        let repository = self
            .repository
            .as_ref()
            .expect("owned production Supervisor has a repository")
            .clone();
        let capsules = self
            .runtime_owner
            .as_ref()
            .expect("owned production Supervisor has a lifecycle owner")
            .native_recovery_capsules(&repository.join(".oracle-lab/paired-root-store"))?;
        let capture_scope = self
            .native_recovery_scope
            .as_ref()
            .expect("aligned production runtime has a capture scope")
            .clone();
        let started = Instant::now();
        let capsule = {
            let physical_runtime = &mut self.physical_runtime;
            capsules.capture_unverified_candidate(&capture_scope, || {
                physical_runtime
                    .as_mut()
                    .ok_or_else(|| {
                        "native recovery candidate lost its physical Java runtime".to_owned()
                    })?
                    .validate_parked_binding(&expected_frame, &expected_autosave)
            })
        }
        .map_err(|error| error.to_string())?;
        let input = serde_json::to_value(
            sts_workbench_engine_protocol::EngineDomainRequestV1::retain_native_recovery_candidate(
                target_root_id,
                captured_activation_id,
                capsule.recovery_ref,
            ),
        )
        .map_err(|error| format!("failed to encode native recovery candidate request: {error}"))?;
        let output = self
            .execute_engine_domain(input, ENGINE_IO_TIMEOUT)
            .map_err(NativeRecoveryFailureV1::from)?;
        let publication = serde_json::from_value::<
            sts_workbench_engine_protocol::EngineNativeRecoveryCandidatePublicationV1,
        >(output)
        .map_err(|error| {
            format!("Engine returned invalid native recovery candidate publication: {error}")
        })?;
        if publication.schema
            != sts_workbench_engine_protocol::ENGINE_NATIVE_RECOVERY_CANDIDATE_PUBLICATION_SCHEMA_V1
            || publication.status
                != sts_workbench_engine_protocol::EngineNativeRecoveryCandidatePublicationStatusV1::Retained
        {
            return Err(NativeRecoveryFailureV1::generic(
                "Engine returned a mismatched native recovery candidate publication",
            ));
        }
        Ok(NativeRecoveryCandidateRetentionV1::Retained {
            elapsed_ms: started.elapsed().as_millis(),
        })
    }

    pub(crate) fn create_native_recovery_certificate(
        &mut self,
        job_id: &str,
        target_root_id: &str,
        captured_activation_id: &str,
    ) -> Result<u128, NativeRecoveryFailureV1> {
        self.require_aligned_runtime()?;
        let engine_epoch = self.ensure_engine()?;
        self.revalidate_parked_runtime(engine_epoch, job_id)?;
        let (expected_frame, expected_autosave) = match self.model.phase() {
            SupervisorPhaseV1::Parked {
                frame_token,
                autosave_identity,
                validated_engine_epoch: Some(validated),
            } if *validated == engine_epoch => (frame_token.clone(), autosave_identity.clone()),
            _ => {
                return Err(NativeRecoveryFailureV1::generic(
                    "native recovery certificate requires one Engine-validated parked runtime",
                ))
            }
        };
        self.physical_runtime
            .as_mut()
            .expect("aligned production runtime exists")
            .validate_parked_binding(&expected_frame, &expected_autosave)?;
        self.preflight_native_recovery_candidate(
            engine_epoch,
            target_root_id,
            captured_activation_id,
        )?;
        self.physical_runtime
            .as_mut()
            .expect("aligned production runtime exists")
            .validate_parked_binding(&expected_frame, &expected_autosave)?;

        let repository = self
            .repository
            .as_ref()
            .expect("owned production Supervisor has a repository")
            .clone();
        let storage_registry = self.storage_registry()?;
        let capsules = self
            .runtime_owner
            .as_ref()
            .expect("owned production Supervisor has a lifecycle owner")
            .native_recovery_capsules(&repository.join(".oracle-lab/paired-root-store"))?;
        let capture_scope = self
            .native_recovery_scope
            .as_ref()
            .expect("aligned production runtime has a capture scope")
            .clone();
        let observation_scope = sts_java_parity_projection::NativeRecoveryObservationScopeV1::new(
            engine_epoch,
            job_id,
        )?;
        let mut capsule_job =
            NativeRecoveryCapsuleJobV1::new(capsules, capture_scope, engine_epoch, job_id)
                .map_err(|error| error.to_string())?;
        let input = serde_json::to_value(
            sts_workbench_engine_protocol::EngineDomainRequestV1::create_native_recovery_certificate(
                job_id,
                target_root_id,
                captured_activation_id,
            ),
        )
        .map_err(|error| format!("failed to encode native recovery request: {error}"))?;
        self.model
            .begin_native_recovery_round_trip(engine_epoch, job_id, &expected_frame)
            .map_err(supervisor_error)?;
        let started = Instant::now();
        let driven = {
            let physical_runtime = &mut self.physical_runtime;
            let mut validate_primary = || {
                physical_runtime
                    .as_mut()
                    .ok_or_else(|| {
                        "native recovery lost its primary physical Java runtime".to_owned()
                    })?
                    .validate_parked_binding(&expected_frame, &expected_autosave)
            };
            let mut open_validator = |recovery_storage: &Path| {
                native_recovery_validator::open_native_recovery_validator(
                    &repository,
                    recovery_storage,
                    &storage_registry,
                    observation_scope.clone(),
                )
            };
            drive_native_recovery_callbacks(
                engine_epoch,
                job_id,
                &mut capsule_job,
                &mut validate_primary,
                &mut open_validator,
                |handler| {
                    self.engine_host
                        .execute_bound_with_supervisor(
                            &mut self.model,
                            engine_epoch,
                            input,
                            ENGINE_IO_TIMEOUT,
                            handler,
                        )
                        .map_err(|error| {
                            NativeRecoveryFailureV1::from_engine(
                                "Engine native recovery validation",
                                error,
                            )
                        })
                },
            )
        };

        self.settle_native_recovery_round_trip(
            engine_epoch,
            job_id,
            driven.primary.as_ref().map(|_| ()),
        )?;

        if let Err(error) = driven.primary {
            return Err(format!(
                "native recovery could not prove the primary runtime was preserved: {error}"
            )
            .into());
        }
        if let Err(error) = driven.cleanup {
            return Err(format!("native recovery cleanup failed: {error}").into());
        }
        let output = driven.execution?;
        let publication = serde_json::from_value::<
            sts_workbench_engine_protocol::EngineNativeRecoveryPublicationV1,
        >(output)
        .map_err(|error| format!("Engine returned invalid native recovery publication: {error}"))?;
        if publication.schema
            != sts_workbench_engine_protocol::ENGINE_NATIVE_RECOVERY_PUBLICATION_SCHEMA_V1
            || publication.status
                != sts_workbench_engine_protocol::EngineNativeRecoveryPublicationStatusV1::Published
        {
            return Err(NativeRecoveryFailureV1::generic(
                "Engine returned a mismatched native recovery publication",
            ));
        }
        Ok(started.elapsed().as_millis())
    }

    fn preflight_native_recovery_candidate(
        &mut self,
        engine_epoch: u64,
        target_root_id: &str,
        captured_activation_id: &str,
    ) -> Result<(), NativeRecoveryFailureV1> {
        let input = serde_json::to_value(
            sts_workbench_engine_protocol::EngineDomainRequestV1::preflight_native_recovery(
                target_root_id,
                captured_activation_id,
            ),
        )
        .map_err(|error| format!("failed to encode native recovery preflight: {error}"))?;
        let mut unexpected_callback = false;
        let output = self
            .engine_host
            .execute_bound_with_supervisor(
                &mut self.model,
                engine_epoch,
                input,
                ENGINE_IO_TIMEOUT,
                |_| {
                    unexpected_callback = true;
                    Ok(SupervisorCallOutcomeV1::Rejected {
                        code: "native_recovery_preflight_has_no_callback".to_owned(),
                    })
                },
            )
            .map_err(|error| {
                NativeRecoveryFailureV1::from_engine("Engine native recovery preflight", error)
            })?;
        if unexpected_callback {
            return Err(NativeRecoveryFailureV1::generic(
                "Engine native recovery preflight requested a physical callback",
            ));
        }
        let preflight = serde_json::from_value::<
            sts_workbench_engine_protocol::EngineNativeRecoveryPreflightV1,
        >(output)
        .map_err(|error| format!("Engine returned invalid native recovery preflight: {error}"))?;
        if preflight.schema
            != sts_workbench_engine_protocol::ENGINE_NATIVE_RECOVERY_PREFLIGHT_SCHEMA_V1
            || preflight.status
                != sts_workbench_engine_protocol::EngineNativeRecoveryPreflightStatusV1::Eligible
        {
            return Err(NativeRecoveryFailureV1::generic(
                "Engine returned a mismatched native recovery preflight",
            ));
        }
        Ok(())
    }

    fn settle_native_recovery_round_trip(
        &mut self,
        engine_epoch: u64,
        job_id: &str,
        primary: Result<(), &String>,
    ) -> Result<(), String> {
        match self.model.phase() {
            SupervisorPhaseV1::Busy {
                job_id: active_job,
                engine_epoch: active_epoch,
                operation: BusyOperationV1::NativeRecoveryRoundTrip { .. },
            } if active_job == job_id && *active_epoch == engine_epoch => self
                .model
                .complete_native_recovery_round_trip(
                    engine_epoch,
                    job_id,
                    if primary.is_ok() {
                        NativeRecoveryRoundTripSettlementV1::SourcePreserved
                    } else {
                        NativeRecoveryRoundTripSettlementV1::Dirty {
                            reason: "native_recovery_primary_drift",
                        }
                    },
                )
                .map_err(supervisor_error),
            SupervisorPhaseV1::Parked {
                validated_engine_epoch: None,
                ..
            } if self.model.engine().is_none() && primary.is_ok() => Ok(()),
            SupervisorPhaseV1::Parked {
                validated_engine_epoch: None,
                ..
            } if self.model.engine().is_none() => self
                .model
                .begin_retire("native_recovery_primary_drift")
                .map_err(supervisor_error),
            _ => Err("native recovery round trip lost its Supervisor job identity".to_owned()),
        }
    }
}

type NativeRecoveryPrimaryGuardV1<'a> = &'a mut dyn FnMut() -> Result<(), String>;
type NativeRecoveryValidatorLauncherV1<'a> =
    &'a mut dyn FnMut(&Path) -> Result<NativeRecoveryValidatorOpenV1, String>;

fn dispatch_native_recovery_call(
    engine_epoch: u64,
    job_id: &str,
    job: &mut NativeRecoveryCapsuleJobV1,
    validate_primary: NativeRecoveryPrimaryGuardV1<'_>,
    open_validator: NativeRecoveryValidatorLauncherV1<'_>,
    call: SupervisorCallV1,
) -> Result<SupervisorCallOutcomeV1, String> {
    let request = serde_json::from_value::<PhysicalRuntimeCallV1>(call.input)
        .map_err(|error| format!("invalid native recovery callback: {error}"))?;
    if request.schema != PHYSICAL_RUNTIME_CALL_SCHEMA_V1 {
        return Err("native recovery callback schema mismatch".to_owned());
    }
    let result = match request.operation {
        PhysicalRuntimeOperationV1::BeginNativeRecoveryValidation {} => job
            .begin(engine_epoch, job_id, validate_primary, open_validator)
            .map_err(|error| error.to_string())?,
        PhysicalRuntimeOperationV1::SealNativeRecoveryValidation {} => job
            .seal(engine_epoch, job_id, validate_primary)
            .map_err(|error| error.to_string())?,
        _ => return Err("native recovery callback exceeded its operation capability".to_owned()),
    };
    let output = serde_json::to_value(result)
        .map_err(|error| format!("failed to encode native recovery result: {error}"))?;
    Ok(SupervisorCallOutcomeV1::Completed { output })
}

pub(super) struct NativeRecoveryCallbackDriveV1<Error> {
    pub(super) execution: Result<Value, Error>,
    pub(super) cleanup: Result<(), String>,
    pub(super) primary: Result<(), String>,
}

pub(super) fn drive_native_recovery_callbacks<Execute, Error>(
    engine_epoch: u64,
    job_id: &str,
    job: &mut NativeRecoveryCapsuleJobV1,
    validate_primary: NativeRecoveryPrimaryGuardV1<'_>,
    open_validator: NativeRecoveryValidatorLauncherV1<'_>,
    execute: Execute,
) -> NativeRecoveryCallbackDriveV1<Error>
where
    Execute: FnOnce(
        &mut dyn FnMut(SupervisorCallV1) -> Result<SupervisorCallOutcomeV1, String>,
    ) -> Result<Value, Error>,
    Error: From<String>,
{
    let mut callback_error = None;
    let raw_execution = {
        let mut handler = |call| {
            Ok(
                match dispatch_native_recovery_call(
                    engine_epoch,
                    job_id,
                    job,
                    validate_primary,
                    open_validator,
                    call,
                ) {
                    Ok(outcome) => outcome,
                    Err(detail) => {
                        callback_error = Some(detail);
                        SupervisorCallOutcomeV1::Rejected {
                            code: "native_recovery_callback_failed".to_owned(),
                        }
                    }
                },
            )
        };
        execute(&mut handler)
    };
    let execution = match callback_error {
        Some(error) => Err(Error::from(error)),
        None => raw_execution,
    };
    let cleanup = if job.phase() == NativeRecoveryCallbackPhaseV1::Completed {
        Ok(())
    } else {
        job.abort().map_err(|error| error.to_string())
    };
    let primary = validate_primary();
    NativeRecoveryCallbackDriveV1 {
        execution,
        cleanup,
        primary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_engine_rejection_preserves_its_domain_code() {
        let failure = NativeRecoveryFailureV1::from_engine(
            "Engine native recovery validation",
            EngineHostErrorV1::Process(EngineProcessErrorV1::Rejected {
                code: "native_recovery_rng_streams_diverged".to_owned(),
            }),
        );

        assert_eq!(
            failure.into_parts(),
            (
                "native_recovery_rng_streams_diverged".to_owned(),
                "Engine native recovery validation was rejected by the replaceable Engine"
                    .to_owned(),
            )
        );
    }

    #[test]
    fn non_rejection_engine_failure_remains_generic() {
        let failure = NativeRecoveryFailureV1::from_engine(
            "Engine native recovery validation",
            EngineHostErrorV1::MissingProcess,
        );
        let (code, detail) = failure.into_parts();

        assert_eq!(code, INTERNAL_NATIVE_RECOVERY_SENTINEL_FAILED_CODE);
        assert!(detail.contains("MissingProcess"));
    }
}
