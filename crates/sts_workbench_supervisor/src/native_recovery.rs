use crate::PhysicalRuntimeResultV1;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRecoveryCallbackPhaseV1 {
    AwaitingBegin,
    AwaitingSeal,
    Completed,
    Failed,
    Aborted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeRecoveryCallbackErrorV1 {
    InvalidJobIdentity,
    EngineEpochMismatch,
    JobIdentityMismatch,
    InvalidPhase,
    BeginFailed(String),
    SealFailed(String),
    InvalidOpaqueResult(&'static str),
}

impl fmt::Display for NativeRecoveryCallbackErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJobIdentity => write!(formatter, "invalid native recovery job identity"),
            Self::EngineEpochMismatch => write!(formatter, "native recovery Engine epoch mismatch"),
            Self::JobIdentityMismatch => write!(formatter, "native recovery job identity mismatch"),
            Self::InvalidPhase => write!(formatter, "native recovery callback is out of order"),
            Self::BeginFailed(detail) => {
                write!(formatter, "native recovery begin failed: {detail}")
            }
            Self::SealFailed(detail) => write!(formatter, "native recovery seal failed: {detail}"),
            Self::InvalidOpaqueResult(detail) => {
                write!(formatter, "native recovery returned {detail}")
            }
        }
    }
}

impl std::error::Error for NativeRecoveryCallbackErrorV1 {}

/// One Engine job's non-transferable authority to begin and then seal a native
/// recovery validation. Physical resources remain in the caller-owned
/// handlers; this type only closes ordering and job/epoch reuse.
#[derive(Debug)]
pub struct NativeRecoveryCallbackJobV1 {
    engine_epoch: u64,
    job_id: String,
    phase: NativeRecoveryCallbackPhaseV1,
}

impl NativeRecoveryCallbackJobV1 {
    pub fn new(
        engine_epoch: u64,
        job_id: impl Into<String>,
    ) -> Result<Self, NativeRecoveryCallbackErrorV1> {
        let job_id = job_id.into();
        if engine_epoch == 0 || job_id.trim().is_empty() {
            return Err(NativeRecoveryCallbackErrorV1::InvalidJobIdentity);
        }
        Ok(Self {
            engine_epoch,
            job_id,
            phase: NativeRecoveryCallbackPhaseV1::AwaitingBegin,
        })
    }

    pub fn phase(&self) -> NativeRecoveryCallbackPhaseV1 {
        self.phase
    }

    pub fn begin(
        &mut self,
        engine_epoch: u64,
        job_id: &str,
        capture_and_open_validator: impl FnOnce() -> Result<String, String>,
    ) -> Result<PhysicalRuntimeResultV1, NativeRecoveryCallbackErrorV1> {
        self.require_job(engine_epoch, job_id)?;
        if self.phase != NativeRecoveryCallbackPhaseV1::AwaitingBegin {
            return Err(NativeRecoveryCallbackErrorV1::InvalidPhase);
        }
        let observation_ref = match capture_and_open_validator() {
            Ok(reference) => reference,
            Err(error) => {
                self.phase = NativeRecoveryCallbackPhaseV1::Failed;
                return Err(NativeRecoveryCallbackErrorV1::BeginFailed(error));
            }
        };
        let result = PhysicalRuntimeResultV1::native_recovery_validator_ready(observation_ref);
        if let Err(error) = result.validate() {
            self.phase = NativeRecoveryCallbackPhaseV1::Failed;
            return Err(NativeRecoveryCallbackErrorV1::InvalidOpaqueResult(error));
        }
        self.phase = NativeRecoveryCallbackPhaseV1::AwaitingSeal;
        Ok(result)
    }

    pub fn seal(
        &mut self,
        engine_epoch: u64,
        job_id: &str,
        seal_recovery: impl FnOnce() -> Result<String, String>,
    ) -> Result<PhysicalRuntimeResultV1, NativeRecoveryCallbackErrorV1> {
        self.require_job(engine_epoch, job_id)?;
        if self.phase != NativeRecoveryCallbackPhaseV1::AwaitingSeal {
            return Err(NativeRecoveryCallbackErrorV1::InvalidPhase);
        }
        let recovery_ref = match seal_recovery() {
            Ok(reference) => reference,
            Err(error) => {
                self.phase = NativeRecoveryCallbackPhaseV1::Failed;
                return Err(NativeRecoveryCallbackErrorV1::SealFailed(error));
            }
        };
        let result = PhysicalRuntimeResultV1::native_recovery_sealed(recovery_ref);
        if let Err(error) = result.validate() {
            self.phase = NativeRecoveryCallbackPhaseV1::Failed;
            return Err(NativeRecoveryCallbackErrorV1::InvalidOpaqueResult(error));
        }
        self.phase = NativeRecoveryCallbackPhaseV1::Completed;
        Ok(result)
    }

    pub fn abort(&mut self) -> Result<(), NativeRecoveryCallbackErrorV1> {
        match self.phase {
            NativeRecoveryCallbackPhaseV1::AwaitingBegin
            | NativeRecoveryCallbackPhaseV1::AwaitingSeal
            | NativeRecoveryCallbackPhaseV1::Failed => {
                self.phase = NativeRecoveryCallbackPhaseV1::Aborted;
                Ok(())
            }
            NativeRecoveryCallbackPhaseV1::Completed | NativeRecoveryCallbackPhaseV1::Aborted => {
                Err(NativeRecoveryCallbackErrorV1::InvalidPhase)
            }
        }
    }

    fn require_job(
        &self,
        engine_epoch: u64,
        job_id: &str,
    ) -> Result<(), NativeRecoveryCallbackErrorV1> {
        if self.engine_epoch != engine_epoch {
            return Err(NativeRecoveryCallbackErrorV1::EngineEpochMismatch);
        }
        if self.job_id != job_id {
            return Err(NativeRecoveryCallbackErrorV1::JobIdentityMismatch);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PhysicalRuntimeOutcomeV1;
    use std::cell::Cell;

    const JOB_ID: &str = "anchor-job-1";

    #[test]
    fn begin_then_seal_is_the_only_successful_sequence() {
        let mut job = NativeRecoveryCallbackJobV1::new(7, JOB_ID).unwrap();
        assert_eq!(
            job.seal(7, JOB_ID, || Ok(recovery_ref('b'))),
            Err(NativeRecoveryCallbackErrorV1::InvalidPhase)
        );

        let begin = job.begin(7, JOB_ID, || Ok(observation_ref('a'))).unwrap();
        assert!(matches!(
            begin.outcome,
            PhysicalRuntimeOutcomeV1::NativeRecoveryValidatorReady { .. }
        ));
        assert_eq!(job.phase(), NativeRecoveryCallbackPhaseV1::AwaitingSeal);
        assert_eq!(
            job.begin(7, JOB_ID, || Ok(observation_ref('c'))),
            Err(NativeRecoveryCallbackErrorV1::InvalidPhase)
        );

        let sealed = job.seal(7, JOB_ID, || Ok(recovery_ref('d'))).unwrap();
        assert!(matches!(
            sealed.outcome,
            PhysicalRuntimeOutcomeV1::NativeRecoverySealed { .. }
        ));
        assert_eq!(job.phase(), NativeRecoveryCallbackPhaseV1::Completed);
        assert_eq!(
            job.seal(7, JOB_ID, || Ok(recovery_ref('e'))),
            Err(NativeRecoveryCallbackErrorV1::InvalidPhase)
        );
    }

    #[test]
    fn another_job_or_engine_epoch_cannot_consume_the_capability() {
        let mut job = NativeRecoveryCallbackJobV1::new(7, JOB_ID).unwrap();
        let handler_calls = Cell::new(0);
        assert_eq!(
            job.begin(8, JOB_ID, || {
                handler_calls.set(handler_calls.get() + 1);
                Ok(observation_ref('a'))
            }),
            Err(NativeRecoveryCallbackErrorV1::EngineEpochMismatch)
        );
        assert_eq!(
            job.begin(7, "anchor-job-2", || {
                handler_calls.set(handler_calls.get() + 1);
                Ok(observation_ref('a'))
            }),
            Err(NativeRecoveryCallbackErrorV1::JobIdentityMismatch)
        );
        assert_eq!(handler_calls.get(), 0);
        assert_eq!(job.phase(), NativeRecoveryCallbackPhaseV1::AwaitingBegin);
        job.begin(7, JOB_ID, || Ok(observation_ref('a'))).unwrap();
    }

    #[test]
    fn handler_or_ref_failure_poison_the_current_job() {
        let mut failed_capture = NativeRecoveryCallbackJobV1::new(7, JOB_ID).unwrap();
        assert_eq!(
            failed_capture.begin(7, JOB_ID, || Err("copy failed".to_owned())),
            Err(NativeRecoveryCallbackErrorV1::BeginFailed(
                "copy failed".to_owned()
            ))
        );
        assert_eq!(
            failed_capture.phase(),
            NativeRecoveryCallbackPhaseV1::Failed
        );
        failed_capture.abort().unwrap();

        let mut invalid_ref = NativeRecoveryCallbackJobV1::new(7, JOB_ID).unwrap();
        assert!(matches!(
            invalid_ref.begin(7, JOB_ID, || Ok("C:\\leaked.json".to_owned())),
            Err(NativeRecoveryCallbackErrorV1::InvalidOpaqueResult(_))
        ));
        assert_eq!(invalid_ref.phase(), NativeRecoveryCallbackPhaseV1::Failed);
    }

    #[test]
    fn seal_failure_and_abort_permanently_close_the_job() {
        let mut failed_seal = NativeRecoveryCallbackJobV1::new(7, JOB_ID).unwrap();
        failed_seal
            .begin(7, JOB_ID, || Ok(observation_ref('a')))
            .unwrap();
        assert_eq!(
            failed_seal.seal(7, JOB_ID, || Err("publish failed".to_owned())),
            Err(NativeRecoveryCallbackErrorV1::SealFailed(
                "publish failed".to_owned()
            ))
        );
        assert_eq!(failed_seal.phase(), NativeRecoveryCallbackPhaseV1::Failed);
        failed_seal.abort().unwrap();
        assert_eq!(failed_seal.phase(), NativeRecoveryCallbackPhaseV1::Aborted);
        assert_eq!(
            failed_seal.begin(7, JOB_ID, || Ok(observation_ref('b'))),
            Err(NativeRecoveryCallbackErrorV1::InvalidPhase)
        );
        assert_eq!(
            failed_seal.seal(7, JOB_ID, || Ok(recovery_ref('c'))),
            Err(NativeRecoveryCallbackErrorV1::InvalidPhase)
        );
    }

    fn observation_ref(fill: char) -> String {
        format!(
            "workbench-native-recovery-observation-v1-{}",
            fill.to_string().repeat(64)
        )
    }

    fn recovery_ref(fill: char) -> String {
        format!(
            "workbench-native-recovery-capsule-v1-{}",
            fill.to_string().repeat(64)
        )
    }
}
