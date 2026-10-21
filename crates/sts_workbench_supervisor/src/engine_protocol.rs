use crate::EngineContractV1;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ENGINE_HELLO_SCHEMA_V1: &str = "sts-workbench-engine-hello-v1";
pub const ENGINE_REQUEST_SCHEMA_V1: &str = "sts-workbench-engine-request-v1";
pub const ENGINE_RESPONSE_SCHEMA_V1: &str = "sts-workbench-engine-response-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineHelloV1 {
    pub schema: String,
    pub contract: EngineContractV1,
}

impl EngineHelloV1 {
    pub fn new(contract: EngineContractV1) -> Self {
        Self {
            schema: ENGINE_HELLO_SCHEMA_V1.to_owned(),
            contract,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineRequestV1 {
    pub schema: String,
    pub request_id: String,
    pub engine_epoch: u64,
    pub operation: EngineOperationV1,
}

impl EngineRequestV1 {
    pub fn new(
        request_id: impl Into<String>,
        engine_epoch: u64,
        operation: EngineOperationV1,
    ) -> Self {
        Self {
            schema: ENGINE_REQUEST_SCHEMA_V1.to_owned(),
            request_id: request_id.into(),
            engine_epoch,
            operation,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EngineOperationV1 {
    Bind,
    Execute {
        input: Value,
    },
    ResumeSupervisorCall {
        call_id: String,
        outcome: SupervisorCallOutcomeV1,
    },
    Shutdown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineResponseV1 {
    pub schema: String,
    pub request_id: String,
    pub engine_epoch: u64,
    pub outcome: EngineOutcomeV1,
}

impl EngineResponseV1 {
    pub fn new(request_id: impl Into<String>, engine_epoch: u64, outcome: EngineOutcomeV1) -> Self {
        Self {
            schema: ENGINE_RESPONSE_SCHEMA_V1.to_owned(),
            request_id: request_id.into(),
            engine_epoch,
            outcome,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum EngineOutcomeV1 {
    Bound,
    Completed { output: Value },
    SupervisorCall { call: SupervisorCallV1 },
    Stopped,
    Rejected { code: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorCallV1 {
    pub call_id: String,
    pub input: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum SupervisorCallOutcomeV1 {
    Completed { output: Value },
    Rejected { code: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn execute_payload_remains_structured() {
        let request = EngineRequestV1::new(
            "request-1",
            7,
            EngineOperationV1::Execute {
                input: json!({"kind": "observe", "options": [1, 2]}),
            },
        );
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: EngineRequestV1 = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn unknown_envelope_fields_are_rejected() {
        let error = serde_json::from_value::<EngineRequestV1>(json!({
            "schema": ENGINE_REQUEST_SCHEMA_V1,
            "request_id": "request-1",
            "engine_epoch": 7,
            "operation": {"kind": "bind"},
            "unexpected": true
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn supervisor_callback_round_trips_without_interpreting_domain_values() {
        let response = EngineResponseV1::new(
            "request-1",
            7,
            EngineOutcomeV1::SupervisorCall {
                call: SupervisorCallV1 {
                    call_id: "java-1".to_owned(),
                    input: json!({"kind": "future_java_operation", "typed": [1, 2]}),
                },
            },
        );
        let encoded = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<EngineResponseV1>(&encoded).unwrap(),
            response
        );

        let resumed = EngineRequestV1::new(
            "request-2",
            7,
            EngineOperationV1::ResumeSupervisorCall {
                call_id: "java-1".to_owned(),
                outcome: SupervisorCallOutcomeV1::Completed {
                    output: json!({"frame": "opaque"}),
                },
            },
        );
        let encoded = serde_json::to_string(&resumed).unwrap();
        assert_eq!(
            serde_json::from_str::<EngineRequestV1>(&encoded).unwrap(),
            resumed
        );
    }
}
