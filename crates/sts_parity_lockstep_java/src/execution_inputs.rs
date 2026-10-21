use crate::{JavaLockstepReceiptV1, JavaRuntimeObservationV1, RecordedJavaTransitionV1};
use sts_java_parity_protocol::ExecutionInputWitness;
use sts_parity_lockstep::{
    AdapterFailureKindV1, AdapterFailureV1, ExecutionInputsV1, EXECUTION_INPUTS_SCHEMA_V1,
};

pub(crate) fn validate_execution_input_transition(
    receipt: &JavaLockstepReceiptV1,
    successor: &JavaRuntimeObservationV1,
) -> Result<ExecutionInputsV1, AdapterFailureV1> {
    let fail = |message| AdapterFailureV1::new(AdapterFailureKindV1::Evidence, message);
    let raw = &successor.frame["execution_input_witness"];
    if raw.is_null() {
        // Old raw evidence remains useful only where Rust independently proves
        // this transition did not consume the newly observed environmental input.
        return Ok(ExecutionInputsV1::default());
    }
    let witness: ExecutionInputWitness = serde_json::from_value(raw.clone()).map_err(|error| {
        fail(format!(
            "Java execution-input witness is malformed: {error}"
        ))
    })?;
    if witness.kind != "execution_input_witness_v1"
        || witness.schema != EXECUTION_INPUTS_SCHEMA_V1
        || witness.from_frame_id != receipt.based_on_frame_id
        || witness.to_frame_id != successor.frame_id
        || Some(witness.to_frame_id) != receipt.successor_frame_id
        || successor.frame["previous_frame_id"].as_u64() != Some(witness.from_frame_id)
        || witness.request_id != receipt.request_id
        || witness.action_id != receipt.action_id
    {
        return Err(fail(
            "Java execution-input witness does not bind the executed action and adjacent frames"
                .to_owned(),
        ));
    }
    if !witness.complete
        || witness.unsupported_reason.is_some()
        || witness.inputs.secret_portal_playtime_bits.is_none()
    {
        return Err(fail(
            "Java execution-input witness is incomplete".to_owned(),
        ));
    }
    witness.inputs.validate().map_err(fail)?;
    Ok(witness.inputs)
}

/// Reused by continuous Act preflight. Raw evidence loading has already
/// validated the action/producer chain; this validates the same per-action
/// input witnesses as the live and offline Java adapter, including settlement.
pub fn execution_inputs_from_transitions_v1(
    transitions: &[RecordedJavaTransitionV1],
) -> Result<ExecutionInputsV1, String> {
    let mut inputs = ExecutionInputsV1::observed_empty();
    for transition in transitions {
        inputs.append(
            validate_execution_input_transition(
                &transition.action.receipt,
                &transition.after.observation,
            )
            .map_err(|failure| failure.message)?,
        )?;
    }
    Ok(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sts_parity_lockstep::ProducerIdentityV1;

    fn fixture() -> (JavaLockstepReceiptV1, JavaRuntimeObservationV1) {
        (
            JavaLockstepReceiptV1 {
                status: "completed".into(),
                request_id: "request".into(),
                based_on_frame_id: 7,
                action_id: "route".into(),
                successor_frame_id: Some(8),
            },
            JavaRuntimeObservationV1 {
                producer: ProducerIdentityV1 {
                    implementation: "test".into(),
                    version: "1".into(),
                    session_epoch: "epoch".into(),
                },
                frame_id: 8,
                actions: Vec::new(),
                frame: json!({ "previous_frame_id": 7,
                "execution_input_witness": { "kind": "execution_input_witness_v1", "schema": EXECUTION_INPUTS_SCHEMA_V1,
                    "from_frame_id": 7, "to_frame_id": 8, "request_id": "request", "action_id": "route", "complete": true,
                    "inputs": { "secret_portal_playtime_bits": [800.0f32.to_bits()] } } }),
            },
        )
    }

    #[test]
    fn execution_inputs_require_exact_action_and_adjacent_frame_binding() {
        let (receipt, observation) = fixture();
        assert_eq!(
            validate_execution_input_transition(&receipt, &observation)
                .unwrap()
                .secret_portal_playtime_bits,
            Some(vec![800.0f32.to_bits()])
        );
        for (key, value) in [
            ("from_frame_id", json!(6)),
            ("to_frame_id", json!(9)),
            ("request_id", json!("foreign")),
            ("action_id", json!("foreign")),
            ("complete", json!(false)),
            ("unsupported_reason", json!("missed read")),
            ("schema", json!("foreign")),
        ] {
            let mut wrong = observation.clone();
            wrong.frame["execution_input_witness"][key] = value;
            assert!(
                validate_execution_input_transition(&receipt, &wrong).is_err(),
                "{key}"
            );
        }
        let mut wrong = observation.clone();
        wrong.frame["previous_frame_id"] = json!(6);
        assert!(validate_execution_input_transition(&receipt, &wrong).is_err());
        wrong.frame = json!({});
        assert_eq!(
            validate_execution_input_transition(&receipt, &wrong).unwrap(),
            ExecutionInputsV1::default()
        );
        wrong = observation;
        wrong.frame["execution_input_witness"]["inputs"]["secret_portal_playtime_bits"] = json!([]);
        assert_eq!(
            validate_execution_input_transition(&receipt, &wrong).unwrap(),
            ExecutionInputsV1::observed_empty()
        );
        wrong.frame["execution_input_witness"]["inputs"]["secret_portal_playtime_bits"] =
            json!([f32::NAN.to_bits()]);
        assert!(validate_execution_input_transition(&receipt, &wrong).is_err());
    }
}
