//! Game-agnostic contract for preserving a Java runtime while replacing the
//! semantic workbench engine.
//!
//! This crate deliberately has no game or workspace dependencies. It owns
//! process/protocol epochs, never scenes, paired roots, actions, or parity.

mod engine_host;
mod engine_process;
mod engine_protocol;
mod image_store;
mod native_recovery;
mod native_recovery_capsule;
mod owned_storage;
mod physical_runtime_protocol;
mod process_containment;
mod repository_owner;
mod shell_protocol;
mod supervised_child;
mod supervised_process;
mod supervised_process_set;
mod supervised_resources;
mod supervised_runtime;

pub use engine_host::{EngineHostConfigurationV1, EngineHostErrorV1, EngineHostV1};
pub use engine_process::{
    EngineProcessErrorV1, EngineProcessV1, MAX_ENGINE_FRAME_BYTES_V1,
    MAX_SUPERVISOR_CALLS_PER_EXECUTE_V1,
};
pub use engine_protocol::{
    EngineHelloV1, EngineOperationV1, EngineOutcomeV1, EngineRequestV1, EngineResponseV1,
    SupervisorCallOutcomeV1, SupervisorCallV1, ENGINE_HELLO_SCHEMA_V1, ENGINE_REQUEST_SCHEMA_V1,
    ENGINE_RESPONSE_SCHEMA_V1,
};
pub use image_store::{
    EngineImageErrorV1, EngineImageStoreV1, EngineImageV1, ExecutableImageStoreV1,
    ExecutableImageV1,
};
pub use native_recovery::{
    NativeRecoveryCallbackErrorV1, NativeRecoveryCallbackJobV1, NativeRecoveryCallbackPhaseV1,
};
pub use native_recovery_capsule::{
    NativeRecoveryCapsuleArtifactV1, NativeRecoveryCapsuleErrorV1, NativeRecoveryCapsuleJobV1,
    NativeRecoveryCapsuleReaderV1, NativeRecoveryCapsuleRegistryV1, NativeRecoveryCaptureScopeV1,
    NativeRecoveryValidatorOpenV1, NativeRecoveryValidatorV1,
};
pub use owned_storage::{
    OwnedStorageErrorV1, OwnedStorageReconciliationIssueV1, OwnedStorageReconciliationV1,
    OwnedStorageRegistryV1, OwnedStorageV1,
};
pub use physical_runtime_protocol::{
    PhysicalRuntimeCallV1, PhysicalRuntimeOperationV1, PhysicalRuntimeOutcomeV1,
    PhysicalRuntimeResultV1, PHYSICAL_RUNTIME_CALL_SCHEMA_V1, PHYSICAL_RUNTIME_RESULT_SCHEMA_V1,
};
pub use process_containment::{
    ProcessContainmentErrorV1, ProcessContainmentV1, PROCESS_CONTAINMENT_CLASS_V1,
};
pub use repository_owner::{
    validate_repository_owner_id_v1, RepositoryLifecycleOwnerV1, RepositoryLifecycleRoleV1,
    RepositoryOwnerErrorV1,
};
pub use shell_protocol::{
    EnginePublicRequestV1, ShellErrorV1, ShellRequestV1, ShellResponseV1, ShellRouteV1,
    ShellRoutingErrorV1, ENGINE_PUBLIC_REQUEST_SCHEMA_V1, SHELL_REQUEST_SCHEMA_V1,
    SHELL_RESPONSE_SCHEMA_V1,
};
pub use supervised_child::{SupervisedChildErrorV1, SupervisedChildV1};
pub use supervised_process::{
    ExistingProcessCandidateV1, SupervisedProcessErrorV1, SupervisedProcessV1,
};
pub use supervised_process_set::{SupervisedProcessSetErrorV1, SupervisedProcessSetV1};
pub use supervised_resources::{SupervisedResourcesErrorV1, SupervisedResourcesV1};
pub use supervised_runtime::{
    RuntimeSessionBindingV1, SupervisedRuntimeErrorV1, SupervisedRuntimeV1,
};

use serde::{Deserialize, Serialize};

pub const SUPERVISOR_PROTOCOL_V2: u16 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineContractV1 {
    pub image_id: String,
    pub supervisor_protocol: u16,
    pub java_build_id: String,
    pub mod_build_id: String,
    pub minimum_java_protocol: u16,
    pub maximum_java_protocol: u16,
}

impl EngineContractV1 {
    fn validate(&self) -> Result<(), SupervisorErrorV1> {
        if self.image_id.trim().is_empty()
            || self.java_build_id.trim().is_empty()
            || self.mod_build_id.trim().is_empty()
            || self.minimum_java_protocol > self.maximum_java_protocol
        {
            return Err(SupervisorErrorV1::InvalidEngineContract);
        }
        if self.supervisor_protocol != SUPERVISOR_PROTOCOL_V2 {
            return Err(SupervisorErrorV1::SupervisorProtocolMismatch);
        }
        Ok(())
    }

    fn supports(&self, runtime: &JavaRuntimeIdentityV1) -> bool {
        self.java_build_id == runtime.java_build_id
            && self.mod_build_id == runtime.mod_build_id
            && self.minimum_java_protocol <= runtime.protocol_version
            && runtime.protocol_version <= self.maximum_java_protocol
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineLeaseV1 {
    pub epoch: u64,
    pub contract: EngineContractV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedEngineV1 {
    engine_epoch: u64,
    contract: EngineContractV1,
}

impl PreparedEngineV1 {
    pub fn engine_epoch(&self) -> u64 {
        self.engine_epoch
    }

    pub fn contract(&self) -> &EngineContractV1 {
        &self.contract
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrepareEngineOutcomeV1 {
    Prepared(PreparedEngineV1),
    DeferredBusy {
        current_engine_epoch: u64,
        job_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JavaRuntimeIdentityV1 {
    pub process_id: u32,
    pub process_start_token: String,
    pub runtime_epoch: String,
    pub java_build_id: String,
    pub mod_build_id: String,
    pub protocol_version: u16,
}

impl JavaRuntimeIdentityV1 {
    fn validate(&self) -> Result<(), SupervisorErrorV1> {
        if self.process_id == 0
            || self.process_start_token.trim().is_empty()
            || self.runtime_epoch.trim().is_empty()
            || self.java_build_id.trim().is_empty()
            || self.mod_build_id.trim().is_empty()
        {
            return Err(SupervisorErrorV1::InvalidRuntimeIdentity);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StableBoundaryV1 {
    Neutral,
    Parked {
        frame_token: String,
        autosave_identity: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BusyOperationV1 {
    Exchange {
        source: StableBoundaryV1,
    },
    Recovery {
        dirty_reason: &'static str,
    },
    ParkedRevalidation {
        frame_token: String,
        autosave_identity: String,
    },
    /// Read-only capture from the primary parked runtime plus validation in a
    /// separate disposable runtime. The primary boundary must not be mutated
    /// by this operation.
    NativeRecoveryRoundTrip {
        frame_token: String,
        autosave_identity: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupervisorPhaseV1 {
    Idle,
    Neutral,
    Parked {
        frame_token: String,
        autosave_identity: String,
        validated_engine_epoch: Option<u64>,
    },
    Busy {
        job_id: String,
        engine_epoch: u64,
        operation: BusyOperationV1,
    },
    Dirty {
        reason: &'static str,
    },
    Retiring {
        reason: &'static str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JavaAfterRotationV1 {
    Absent,
    PreservedNeutral {
        process_id: u32,
    },
    ParkedRequiresRevalidation {
        process_id: u32,
        frame_token: String,
    },
    DirtyRequiresRecovery {
        process_id: u32,
    },
    RetireRequired {
        process_id: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RotationOutcomeV1 {
    Attached {
        engine_epoch: u64,
        java: JavaAfterRotationV1,
    },
    DeferredBusy {
        current_engine_epoch: u64,
        job_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExchangeSettlementV1 {
    Neutral,
    Parked {
        frame_token: String,
        autosave_identity: String,
    },
    Retire {
        reason: &'static str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoverySettlementV1 {
    Neutral,
    Dirty { reason: &'static str },
    Retire { reason: &'static str },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParkedRevalidationSettlementV1 {
    Verified,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeRecoveryRoundTripSettlementV1 {
    /// The primary parked runtime was revalidated after the read-only work.
    SourcePreserved,
    /// The primary runtime can no longer be proven to match its source
    /// boundary. A later Engine must recover it explicitly or retire it.
    Dirty {
        reason: &'static str,
    },
    Retire {
        reason: &'static str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupervisorErrorV1 {
    InvalidEngineContract,
    SupervisorProtocolMismatch,
    InvalidRuntimeIdentity,
    EngineRequired,
    EngineEpochMismatch,
    RuntimeNotIdle,
    RuntimeUnavailable,
    RuntimeNotParked,
    RuntimeNotBusy,
    RuntimeNotRetiring,
    RuntimeNotRecovering,
    RuntimeBusy,
    JavaProtocolUnsupported,
    ParkedFrameRequiresRevalidation,
    ParkedFrameAlreadyValidated,
    FrameTokenMismatch,
    JobIdentityMismatch,
    EmptyIdentity,
    PreparedEngineStale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupervisorModelV1 {
    next_engine_epoch: u64,
    engine: Option<EngineLeaseV1>,
    runtime: Option<JavaRuntimeIdentityV1>,
    phase: SupervisorPhaseV1,
}

impl Default for SupervisorModelV1 {
    fn default() -> Self {
        Self {
            next_engine_epoch: 1,
            engine: None,
            runtime: None,
            phase: SupervisorPhaseV1::Idle,
        }
    }
}

impl SupervisorModelV1 {
    pub fn engine(&self) -> Option<&EngineLeaseV1> {
        self.engine.as_ref()
    }

    pub fn runtime(&self) -> Option<&JavaRuntimeIdentityV1> {
        self.runtime.as_ref()
    }

    pub fn phase(&self) -> &SupervisorPhaseV1 {
        &self.phase
    }

    /// Reserves the next Engine epoch without changing the active Engine.
    /// A candidate process must bind this epoch before `attach_prepared_engine`
    /// commits the replacement.
    pub fn prepare_engine(
        &self,
        contract: EngineContractV1,
    ) -> Result<PrepareEngineOutcomeV1, SupervisorErrorV1> {
        contract.validate()?;
        if let SupervisorPhaseV1::Busy {
            job_id,
            engine_epoch,
            ..
        } = &self.phase
        {
            return Ok(PrepareEngineOutcomeV1::DeferredBusy {
                current_engine_epoch: *engine_epoch,
                job_id: job_id.clone(),
            });
        }
        Ok(PrepareEngineOutcomeV1::Prepared(PreparedEngineV1 {
            engine_epoch: self.next_engine_epoch,
            contract,
        }))
    }

    pub fn attach_prepared_engine(
        &mut self,
        prepared: PreparedEngineV1,
    ) -> Result<RotationOutcomeV1, SupervisorErrorV1> {
        if let SupervisorPhaseV1::Busy {
            job_id,
            engine_epoch,
            ..
        } = &self.phase
        {
            return Ok(RotationOutcomeV1::DeferredBusy {
                current_engine_epoch: *engine_epoch,
                job_id: job_id.clone(),
            });
        }
        if prepared.engine_epoch != self.next_engine_epoch {
            return Err(SupervisorErrorV1::PreparedEngineStale);
        }
        let engine_epoch = prepared.engine_epoch;
        self.next_engine_epoch += 1;
        self.engine = Some(EngineLeaseV1 {
            epoch: engine_epoch,
            contract: prepared.contract,
        });
        let java = self.java_after_rotation();
        Ok(RotationOutcomeV1::Attached { engine_epoch, java })
    }

    fn java_after_rotation(&mut self) -> JavaAfterRotationV1 {
        let Some(runtime) = &self.runtime else {
            return JavaAfterRotationV1::Absent;
        };
        let process_id = runtime.process_id;
        let engine = self.engine.as_ref().expect("engine was installed");
        if !engine.contract.supports(runtime) {
            self.phase = SupervisorPhaseV1::Retiring {
                reason: "java_protocol_incompatible",
            };
            return JavaAfterRotationV1::RetireRequired { process_id };
        }
        match &mut self.phase {
            SupervisorPhaseV1::Neutral => JavaAfterRotationV1::PreservedNeutral { process_id },
            SupervisorPhaseV1::Parked {
                frame_token,
                validated_engine_epoch,
                ..
            } => {
                *validated_engine_epoch = None;
                JavaAfterRotationV1::ParkedRequiresRevalidation {
                    process_id,
                    frame_token: frame_token.clone(),
                }
            }
            SupervisorPhaseV1::Dirty { .. } => {
                JavaAfterRotationV1::DirtyRequiresRecovery { process_id }
            }
            SupervisorPhaseV1::Retiring { .. } => {
                JavaAfterRotationV1::RetireRequired { process_id }
            }
            SupervisorPhaseV1::Idle | SupervisorPhaseV1::Busy { .. } => {
                unreachable!("runtime identity and supervisor phase disagree")
            }
        }
    }

    pub fn runtime_opened(
        &mut self,
        runtime: JavaRuntimeIdentityV1,
    ) -> Result<(), SupervisorErrorV1> {
        runtime.validate()?;
        if self.runtime.is_some() || self.phase != SupervisorPhaseV1::Idle {
            return Err(SupervisorErrorV1::RuntimeNotIdle);
        }
        let engine = self
            .engine
            .as_ref()
            .ok_or(SupervisorErrorV1::EngineRequired)?;
        if !engine.contract.supports(&runtime) {
            return Err(SupervisorErrorV1::JavaProtocolUnsupported);
        }
        self.runtime = Some(runtime);
        self.phase = SupervisorPhaseV1::Neutral;
        Ok(())
    }

    pub fn begin_exchange(
        &mut self,
        engine_epoch: u64,
        job_id: impl Into<String>,
        expected_frame_token: Option<&str>,
    ) -> Result<(), SupervisorErrorV1> {
        self.require_engine(engine_epoch)?;
        let job_id = job_id.into();
        if job_id.trim().is_empty() {
            return Err(SupervisorErrorV1::EmptyIdentity);
        }
        let source = match &self.phase {
            SupervisorPhaseV1::Neutral if expected_frame_token.is_none() => {
                StableBoundaryV1::Neutral
            }
            SupervisorPhaseV1::Neutral => return Err(SupervisorErrorV1::FrameTokenMismatch),
            SupervisorPhaseV1::Parked {
                frame_token,
                autosave_identity,
                validated_engine_epoch,
            } => {
                if *validated_engine_epoch != Some(engine_epoch) {
                    return Err(SupervisorErrorV1::ParkedFrameRequiresRevalidation);
                }
                if expected_frame_token != Some(frame_token.as_str()) {
                    return Err(SupervisorErrorV1::FrameTokenMismatch);
                }
                StableBoundaryV1::Parked {
                    frame_token: frame_token.clone(),
                    autosave_identity: autosave_identity.clone(),
                }
            }
            _ => return Err(SupervisorErrorV1::RuntimeUnavailable),
        };
        self.phase = SupervisorPhaseV1::Busy {
            job_id,
            engine_epoch,
            operation: BusyOperationV1::Exchange { source },
        };
        Ok(())
    }

    pub fn complete_exchange(
        &mut self,
        engine_epoch: u64,
        job_id: &str,
        settlement: ExchangeSettlementV1,
    ) -> Result<(), SupervisorErrorV1> {
        let SupervisorPhaseV1::Busy {
            job_id: expected_job,
            engine_epoch: expected_engine,
            operation: BusyOperationV1::Exchange { .. },
        } = &self.phase
        else {
            return Err(SupervisorErrorV1::RuntimeNotBusy);
        };
        if *expected_engine != engine_epoch {
            return Err(SupervisorErrorV1::EngineEpochMismatch);
        }
        if expected_job != job_id {
            return Err(SupervisorErrorV1::JobIdentityMismatch);
        }
        self.phase = match settlement {
            ExchangeSettlementV1::Neutral => SupervisorPhaseV1::Neutral,
            ExchangeSettlementV1::Parked {
                frame_token,
                autosave_identity,
            } => {
                if frame_token.trim().is_empty() || autosave_identity.trim().is_empty() {
                    return Err(SupervisorErrorV1::EmptyIdentity);
                }
                SupervisorPhaseV1::Parked {
                    frame_token,
                    autosave_identity,
                    validated_engine_epoch: Some(engine_epoch),
                }
            }
            ExchangeSettlementV1::Retire { reason } => {
                if reason.trim().is_empty() {
                    return Err(SupervisorErrorV1::EmptyIdentity);
                }
                SupervisorPhaseV1::Retiring { reason }
            }
        };
        Ok(())
    }

    pub fn begin_recovery(
        &mut self,
        engine_epoch: u64,
        job_id: impl Into<String>,
    ) -> Result<(), SupervisorErrorV1> {
        self.require_engine(engine_epoch)?;
        let job_id = job_id.into();
        if job_id.trim().is_empty() {
            return Err(SupervisorErrorV1::EmptyIdentity);
        }
        let SupervisorPhaseV1::Dirty { reason } = &self.phase else {
            return Err(SupervisorErrorV1::RuntimeNotRecovering);
        };
        self.phase = SupervisorPhaseV1::Busy {
            job_id,
            engine_epoch,
            operation: BusyOperationV1::Recovery {
                dirty_reason: reason,
            },
        };
        Ok(())
    }

    pub fn complete_recovery(
        &mut self,
        engine_epoch: u64,
        job_id: &str,
        settlement: RecoverySettlementV1,
    ) -> Result<(), SupervisorErrorV1> {
        let SupervisorPhaseV1::Busy {
            job_id: expected_job,
            engine_epoch: expected_engine,
            operation: BusyOperationV1::Recovery { .. },
        } = &self.phase
        else {
            return Err(SupervisorErrorV1::RuntimeNotRecovering);
        };
        if *expected_engine != engine_epoch {
            return Err(SupervisorErrorV1::EngineEpochMismatch);
        }
        if expected_job != job_id {
            return Err(SupervisorErrorV1::JobIdentityMismatch);
        }
        self.phase = match settlement {
            RecoverySettlementV1::Neutral => SupervisorPhaseV1::Neutral,
            RecoverySettlementV1::Dirty { reason } => {
                if reason.trim().is_empty() {
                    return Err(SupervisorErrorV1::EmptyIdentity);
                }
                SupervisorPhaseV1::Dirty { reason }
            }
            RecoverySettlementV1::Retire { reason } => {
                if reason.trim().is_empty() {
                    return Err(SupervisorErrorV1::EmptyIdentity);
                }
                SupervisorPhaseV1::Retiring { reason }
            }
        };
        Ok(())
    }

    pub fn begin_parked_revalidation(
        &mut self,
        engine_epoch: u64,
        job_id: impl Into<String>,
        expected_frame_token: &str,
    ) -> Result<(), SupervisorErrorV1> {
        self.require_engine(engine_epoch)?;
        let job_id = job_id.into();
        if job_id.trim().is_empty() {
            return Err(SupervisorErrorV1::EmptyIdentity);
        }
        let SupervisorPhaseV1::Parked {
            frame_token,
            autosave_identity,
            validated_engine_epoch,
        } = &self.phase
        else {
            return Err(SupervisorErrorV1::RuntimeNotParked);
        };
        if frame_token != expected_frame_token {
            return Err(SupervisorErrorV1::FrameTokenMismatch);
        }
        if validated_engine_epoch.is_some() {
            return Err(SupervisorErrorV1::ParkedFrameAlreadyValidated);
        }
        self.phase = SupervisorPhaseV1::Busy {
            job_id,
            engine_epoch,
            operation: BusyOperationV1::ParkedRevalidation {
                frame_token: frame_token.clone(),
                autosave_identity: autosave_identity.clone(),
            },
        };
        Ok(())
    }

    pub fn complete_parked_revalidation(
        &mut self,
        engine_epoch: u64,
        job_id: &str,
        settlement: ParkedRevalidationSettlementV1,
    ) -> Result<(), SupervisorErrorV1> {
        let SupervisorPhaseV1::Busy {
            job_id: expected_job,
            engine_epoch: expected_engine,
            operation:
                BusyOperationV1::ParkedRevalidation {
                    frame_token,
                    autosave_identity,
                },
        } = &self.phase
        else {
            return Err(SupervisorErrorV1::RuntimeNotParked);
        };
        if *expected_engine != engine_epoch {
            return Err(SupervisorErrorV1::EngineEpochMismatch);
        }
        if expected_job != job_id {
            return Err(SupervisorErrorV1::JobIdentityMismatch);
        }
        self.phase = SupervisorPhaseV1::Parked {
            frame_token: frame_token.clone(),
            autosave_identity: autosave_identity.clone(),
            validated_engine_epoch: match settlement {
                ParkedRevalidationSettlementV1::Verified => Some(engine_epoch),
                ParkedRevalidationSettlementV1::Rejected => None,
            },
        };
        Ok(())
    }

    pub fn begin_native_recovery_round_trip(
        &mut self,
        engine_epoch: u64,
        job_id: impl Into<String>,
        expected_frame_token: &str,
    ) -> Result<(), SupervisorErrorV1> {
        self.require_engine(engine_epoch)?;
        let job_id = job_id.into();
        if job_id.trim().is_empty() {
            return Err(SupervisorErrorV1::EmptyIdentity);
        }
        let SupervisorPhaseV1::Parked {
            frame_token,
            autosave_identity,
            validated_engine_epoch,
        } = &self.phase
        else {
            return Err(SupervisorErrorV1::RuntimeNotParked);
        };
        if *validated_engine_epoch != Some(engine_epoch) {
            return Err(SupervisorErrorV1::ParkedFrameRequiresRevalidation);
        }
        if frame_token != expected_frame_token {
            return Err(SupervisorErrorV1::FrameTokenMismatch);
        }
        self.phase = SupervisorPhaseV1::Busy {
            job_id,
            engine_epoch,
            operation: BusyOperationV1::NativeRecoveryRoundTrip {
                frame_token: frame_token.clone(),
                autosave_identity: autosave_identity.clone(),
            },
        };
        Ok(())
    }

    pub fn complete_native_recovery_round_trip(
        &mut self,
        engine_epoch: u64,
        job_id: &str,
        settlement: NativeRecoveryRoundTripSettlementV1,
    ) -> Result<(), SupervisorErrorV1> {
        let SupervisorPhaseV1::Busy {
            job_id: expected_job,
            engine_epoch: expected_engine,
            operation:
                BusyOperationV1::NativeRecoveryRoundTrip {
                    frame_token,
                    autosave_identity,
                },
        } = &self.phase
        else {
            return Err(SupervisorErrorV1::RuntimeNotBusy);
        };
        if *expected_engine != engine_epoch {
            return Err(SupervisorErrorV1::EngineEpochMismatch);
        }
        if expected_job != job_id {
            return Err(SupervisorErrorV1::JobIdentityMismatch);
        }
        self.phase = match settlement {
            NativeRecoveryRoundTripSettlementV1::SourcePreserved => SupervisorPhaseV1::Parked {
                frame_token: frame_token.clone(),
                autosave_identity: autosave_identity.clone(),
                validated_engine_epoch: Some(engine_epoch),
            },
            NativeRecoveryRoundTripSettlementV1::Dirty { reason } => {
                if reason.trim().is_empty() {
                    return Err(SupervisorErrorV1::EmptyIdentity);
                }
                SupervisorPhaseV1::Dirty { reason }
            }
            NativeRecoveryRoundTripSettlementV1::Retire { reason } => {
                if reason.trim().is_empty() {
                    return Err(SupervisorErrorV1::EmptyIdentity);
                }
                SupervisorPhaseV1::Retiring { reason }
            }
        };
        Ok(())
    }

    pub fn engine_disconnected(&mut self, engine_epoch: u64) -> Result<(), SupervisorErrorV1> {
        self.require_engine(engine_epoch)?;
        self.engine = None;
        match self.phase.clone() {
            SupervisorPhaseV1::Busy {
                engine_epoch: busy_engine,
                operation: BusyOperationV1::Exchange { .. },
                ..
            } if busy_engine == engine_epoch => {
                self.phase = SupervisorPhaseV1::Dirty {
                    reason: "engine_disconnected_during_exchange",
                };
            }
            SupervisorPhaseV1::Busy {
                engine_epoch: busy_engine,
                operation: BusyOperationV1::Recovery { .. },
                ..
            } if busy_engine == engine_epoch => {
                self.phase = SupervisorPhaseV1::Dirty {
                    reason: "engine_disconnected_during_recovery",
                };
            }
            SupervisorPhaseV1::Busy {
                engine_epoch: busy_engine,
                operation:
                    BusyOperationV1::ParkedRevalidation {
                        frame_token,
                        autosave_identity,
                    },
                ..
            } if busy_engine == engine_epoch => {
                self.phase = SupervisorPhaseV1::Parked {
                    frame_token,
                    autosave_identity,
                    validated_engine_epoch: None,
                };
            }
            SupervisorPhaseV1::Busy {
                engine_epoch: busy_engine,
                operation:
                    BusyOperationV1::NativeRecoveryRoundTrip {
                        frame_token,
                        autosave_identity,
                    },
                ..
            } if busy_engine == engine_epoch => {
                // The primary runtime is read-only throughout this operation.
                // A replacement Engine must still revalidate the boundary;
                // any disposable validator/capsule is owned outside this
                // model and must be aborted by its resource owner.
                self.phase = SupervisorPhaseV1::Parked {
                    frame_token,
                    autosave_identity,
                    validated_engine_epoch: None,
                };
            }
            SupervisorPhaseV1::Parked {
                frame_token,
                autosave_identity,
                ..
            } => {
                self.phase = SupervisorPhaseV1::Parked {
                    frame_token,
                    autosave_identity,
                    validated_engine_epoch: None,
                };
            }
            _ => {}
        }
        Ok(())
    }

    pub fn begin_retire(&mut self, reason: &'static str) -> Result<(), SupervisorErrorV1> {
        if reason.trim().is_empty() {
            return Err(SupervisorErrorV1::EmptyIdentity);
        }
        match self.phase {
            SupervisorPhaseV1::Idle => return Err(SupervisorErrorV1::RuntimeUnavailable),
            SupervisorPhaseV1::Busy { .. } => return Err(SupervisorErrorV1::RuntimeBusy),
            _ => self.phase = SupervisorPhaseV1::Retiring { reason },
        }
        Ok(())
    }

    pub fn complete_retirement(&mut self) -> Result<(), SupervisorErrorV1> {
        if !matches!(self.phase, SupervisorPhaseV1::Retiring { .. }) {
            return Err(SupervisorErrorV1::RuntimeNotRetiring);
        }
        self.runtime = None;
        self.phase = SupervisorPhaseV1::Idle;
        Ok(())
    }

    fn require_engine(&self, engine_epoch: u64) -> Result<(), SupervisorErrorV1> {
        let engine = self
            .engine
            .as_ref()
            .ok_or(SupervisorErrorV1::EngineRequired)?;
        if engine.epoch != engine_epoch {
            return Err(SupervisorErrorV1::EngineEpochMismatch);
        }
        Ok(())
    }
}
