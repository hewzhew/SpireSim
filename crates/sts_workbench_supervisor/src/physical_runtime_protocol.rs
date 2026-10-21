use serde::{Deserialize, Serialize};

pub const PHYSICAL_RUNTIME_CALL_SCHEMA_V1: &str = "sts-workbench-physical-runtime-call-v1";
pub const PHYSICAL_RUNTIME_RESULT_SCHEMA_V1: &str = "sts-workbench-physical-runtime-result-v1";
const NATIVE_RECOVERY_OBSERVATION_REF_PREFIX_V1: &str = "workbench-native-recovery-observation-v1-";
const NATIVE_RECOVERY_CAPSULE_REF_PREFIX_V1: &str = "workbench-native-recovery-capsule-v1-";

/// Closed Engine-to-Supervisor vocabulary for operations whose authority
/// belongs to the physical runtime owner. It deliberately contains no scene,
/// action, artifact path, process ID, or generic JSON payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalRuntimeCallV1 {
    pub schema: String,
    pub operation: PhysicalRuntimeOperationV1,
}

impl PhysicalRuntimeCallV1 {
    pub fn validate_parked() -> Self {
        Self {
            schema: PHYSICAL_RUNTIME_CALL_SCHEMA_V1.to_owned(),
            operation: PhysicalRuntimeOperationV1::ValidateParked {},
        }
    }

    pub fn reset_to_native() -> Self {
        Self {
            schema: PHYSICAL_RUNTIME_CALL_SCHEMA_V1.to_owned(),
            operation: PhysicalRuntimeOperationV1::ResetToNative {},
        }
    }

    pub fn begin_native_recovery_validation() -> Self {
        Self {
            schema: PHYSICAL_RUNTIME_CALL_SCHEMA_V1.to_owned(),
            operation: PhysicalRuntimeOperationV1::BeginNativeRecoveryValidation {},
        }
    }

    pub fn seal_native_recovery_validation() -> Self {
        Self {
            schema: PHYSICAL_RUNTIME_CALL_SCHEMA_V1.to_owned(),
            operation: PhysicalRuntimeOperationV1::SealNativeRecoveryValidation {},
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PhysicalRuntimeOperationV1 {
    ValidateParked {},
    ResetToNative {},
    /// Capture the already-authorized parked storage and open a disposable
    /// fresh validator. The active job supplies all authority out of band.
    BeginNativeRecoveryValidation {},
    /// Seal the current job's staged recovery capsule after the Engine has
    /// independently accepted the validator observation as Exact.
    SealNativeRecoveryValidation {},
}

/// A successful callback result carries no physical identity back into the
/// replaceable Engine. Exact process/frame/autosave values remain owned by the
/// Supervisor state transition that authorized the call.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalRuntimeResultV1 {
    pub schema: String,
    pub outcome: PhysicalRuntimeOutcomeV1,
}

impl PhysicalRuntimeResultV1 {
    pub fn parked_validated() -> Self {
        Self {
            schema: PHYSICAL_RUNTIME_RESULT_SCHEMA_V1.to_owned(),
            outcome: PhysicalRuntimeOutcomeV1::ParkedValidated {},
        }
    }

    pub fn native_reset() -> Self {
        Self {
            schema: PHYSICAL_RUNTIME_RESULT_SCHEMA_V1.to_owned(),
            outcome: PhysicalRuntimeOutcomeV1::NativeReset {},
        }
    }

    pub fn native_recovery_validator_ready(observation_ref: impl Into<String>) -> Self {
        Self {
            schema: PHYSICAL_RUNTIME_RESULT_SCHEMA_V1.to_owned(),
            outcome: PhysicalRuntimeOutcomeV1::NativeRecoveryValidatorReady {
                observation_ref: observation_ref.into(),
            },
        }
    }

    pub fn native_recovery_sealed(recovery_ref: impl Into<String>) -> Self {
        Self {
            schema: PHYSICAL_RUNTIME_RESULT_SCHEMA_V1.to_owned(),
            outcome: PhysicalRuntimeOutcomeV1::NativeRecoverySealed {
                recovery_ref: recovery_ref.into(),
            },
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema != PHYSICAL_RUNTIME_RESULT_SCHEMA_V1 {
            return Err("physical runtime result schema mismatch");
        }
        match &self.outcome {
            PhysicalRuntimeOutcomeV1::ParkedValidated {}
            | PhysicalRuntimeOutcomeV1::NativeReset {} => Ok(()),
            PhysicalRuntimeOutcomeV1::NativeRecoveryValidatorReady { observation_ref }
                if valid_opaque_ref(observation_ref, NATIVE_RECOVERY_OBSERVATION_REF_PREFIX_V1) =>
            {
                Ok(())
            }
            PhysicalRuntimeOutcomeV1::NativeRecoverySealed { recovery_ref }
                if valid_opaque_ref(recovery_ref, NATIVE_RECOVERY_CAPSULE_REF_PREFIX_V1) =>
            {
                Ok(())
            }
            PhysicalRuntimeOutcomeV1::NativeRecoveryValidatorReady { .. } => {
                Err("invalid native recovery observation ref")
            }
            PhysicalRuntimeOutcomeV1::NativeRecoverySealed { .. } => {
                Err("invalid native recovery capsule ref")
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum PhysicalRuntimeOutcomeV1 {
    ParkedValidated {},
    NativeReset {},
    NativeRecoveryValidatorReady { observation_ref: String },
    NativeRecoverySealed { recovery_ref: String },
}

fn valid_opaque_ref(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 64
            && suffix
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parked_validation_has_no_identity_or_escape_payload() {
        let call = serde_json::to_value(PhysicalRuntimeCallV1::validate_parked()).unwrap();
        assert_eq!(
            call,
            json!({
                "schema": PHYSICAL_RUNTIME_CALL_SCHEMA_V1,
                "operation": {"kind": "validate_parked"},
            })
        );
        let result = serde_json::to_value(PhysicalRuntimeResultV1::parked_validated()).unwrap();
        assert_eq!(
            result,
            json!({
                "schema": PHYSICAL_RUNTIME_RESULT_SCHEMA_V1,
                "outcome": {"status": "parked_validated"},
            })
        );
    }

    #[test]
    fn native_reset_has_no_identity_or_escape_payload() {
        let call = serde_json::to_value(PhysicalRuntimeCallV1::reset_to_native()).unwrap();
        assert_eq!(
            call,
            json!({
                "schema": PHYSICAL_RUNTIME_CALL_SCHEMA_V1,
                "operation": {"kind": "reset_to_native"},
            })
        );
        let result = serde_json::to_value(PhysicalRuntimeResultV1::native_reset()).unwrap();
        assert_eq!(
            result,
            json!({
                "schema": PHYSICAL_RUNTIME_RESULT_SCHEMA_V1,
                "outcome": {"status": "native_reset"},
            })
        );
    }

    #[test]
    fn physical_callback_rejects_unknown_fields() {
        let error = serde_json::from_value::<PhysicalRuntimeCallV1>(json!({
            "schema": PHYSICAL_RUNTIME_CALL_SCHEMA_V1,
            "operation": {"kind": "validate_parked", "frame": "smuggled"},
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));

        let error = serde_json::from_value::<PhysicalRuntimeCallV1>(json!({
            "schema": PHYSICAL_RUNTIME_CALL_SCHEMA_V1,
            "operation": {"kind": "reset_to_native", "root": "smuggled"},
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));

        let error = serde_json::from_value::<PhysicalRuntimeResultV1>(json!({
            "schema": PHYSICAL_RUNTIME_RESULT_SCHEMA_V1,
            "outcome": {"status": "parked_validated", "frame": "smuggled"},
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn native_recovery_callbacks_expose_only_canonical_opaque_refs() {
        assert_eq!(
            serde_json::to_value(PhysicalRuntimeCallV1::begin_native_recovery_validation())
                .unwrap(),
            json!({
                "schema": PHYSICAL_RUNTIME_CALL_SCHEMA_V1,
                "operation": {"kind": "begin_native_recovery_validation"},
            })
        );
        assert_eq!(
            serde_json::to_value(PhysicalRuntimeCallV1::seal_native_recovery_validation()).unwrap(),
            json!({
                "schema": PHYSICAL_RUNTIME_CALL_SCHEMA_V1,
                "operation": {"kind": "seal_native_recovery_validation"},
            })
        );

        let observation = PhysicalRuntimeResultV1::native_recovery_validator_ready(format!(
            "{NATIVE_RECOVERY_OBSERVATION_REF_PREFIX_V1}{}",
            "a".repeat(64)
        ));
        observation.validate().unwrap();
        let sealed = PhysicalRuntimeResultV1::native_recovery_sealed(format!(
            "{NATIVE_RECOVERY_CAPSULE_REF_PREFIX_V1}{}",
            "b".repeat(64)
        ));
        sealed.validate().unwrap();

        let invalid = PhysicalRuntimeResultV1::native_recovery_validator_ready(
            "C:\\leaked\\validator-frame.json",
        );
        assert_eq!(
            invalid.validate(),
            Err("invalid native recovery observation ref")
        );
    }
}
