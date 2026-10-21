//! Test-only oracle evidence. Never installs saves or authorizes live control.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

pub(crate) fn load(name: &str) -> Value {
    load_from("observed_a20h", name)
}

pub(crate) fn load_from(group: &str, name: &str) -> Value {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(group);
    let sources: Value =
        serde_json::from_slice(&std::fs::read(directory.join("sources.json")).unwrap()).unwrap();
    let bytes = std::fs::read(directory.join(name)).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        sources["files"][name]["sha256"].as_str().unwrap(),
        "recorded oracle bytes changed: {name}"
    );
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    if let Some(observation) = value.get("observation") {
        assert_eq!(observation["producer"], sources["files"][name]["producer"]);
    }
    value
}

pub(crate) fn observation(name: &str) -> crate::JavaRuntimeObservationV1 {
    observation_from("observed_a20h", name)
}

pub(crate) fn observation_from(group: &str, name: &str) -> crate::JavaRuntimeObservationV1 {
    let boundary: crate::RecordedJavaBoundaryV1 =
        serde_json::from_value(load_from(group, name)).unwrap();
    boundary.validate().unwrap();
    boundary.observation
}

pub(crate) fn validate_adjacent(
    before: &crate::JavaRuntimeObservationV1,
    action: &crate::RecordedJavaActionV1,
    after: &crate::JavaRuntimeObservationV1,
) -> bool {
    validate_adjacent_phase(
        before,
        action,
        after,
        crate::RecordedJavaActionPhaseV1::Committed,
    )
}

pub(crate) fn validate_adjacent_phase(
    before: &crate::JavaRuntimeObservationV1,
    action: &crate::RecordedJavaActionV1,
    after: &crate::JavaRuntimeObservationV1,
    phase: crate::RecordedJavaActionPhaseV1,
) -> bool {
    let witness = &after.frame["rng_call_witness"];
    action.validate().is_ok()
        && action.phase == phase
        && action.receipt.status == "completed"
        && before.producer == after.producer
        && before.frame["boundary"]["stable"] == true
        && after.frame["boundary"]["stable"] == true
        && action.receipt.based_on_frame_id == before.frame_id
        && action.receipt.successor_frame_id == Some(after.frame_id)
        && after.frame_id > before.frame_id
        && before.frame["frame_id"] == before.frame_id
        && after.frame["frame_id"] == after.frame_id
        && after.frame["previous_frame_id"] == before.frame_id
        && before
            .actions
            .iter()
            .any(|a| a.action_id == action.receipt.action_id)
        && witness["kind"] == "rng_call_witness_v1"
        && witness["schema"] == sts_parity_lockstep::NATURAL_RUN_RNG_CALL_LEDGER_SCHEMA_V1
        && witness["request_id"] == action.receipt.request_id
        && witness["action_id"] == action.receipt.action_id
        && witness["from_frame_id"] == before.frame_id
        && witness["to_frame_id"] == after.frame_id
        && witness["complete"] == true
        && witness["unsupported_reason"].is_null()
}
