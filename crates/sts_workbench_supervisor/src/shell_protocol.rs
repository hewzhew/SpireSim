use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SHELL_REQUEST_SCHEMA_V1: &str = "sts-workbench-runtime-request-v1";
pub const SHELL_RESPONSE_SCHEMA_V1: &str = "sts-workbench-runtime-response-v1";
pub const ENGINE_PUBLIC_REQUEST_SCHEMA_V1: &str = "sts-workbench-engine-public-request-v1";

/// Opaque public operation forwarded by the stable shell to the replaceable
/// Engine. The shell validates only this envelope; the Engine owns the closed
/// public operation vocabulary and its field-level validation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnginePublicRequestV1 {
    pub schema: String,
    pub operation: Value,
}

impl EnginePublicRequestV1 {
    pub fn new(operation: Value) -> Self {
        Self {
            schema: ENGINE_PUBLIC_REQUEST_SCHEMA_V1.to_owned(),
            operation,
        }
    }
}

/// Stable outer request understood by the long-lived shell.
///
/// `operation` is intentionally generic: the shell may carry an opaque JSON
/// value while a replaceable Engine supplies the closed domain operation type.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShellRequestV1<Operation = Value> {
    pub schema: String,
    pub request_id: String,
    pub operation: Operation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellRouteV1 {
    ServiceStatus,
    Shutdown,
    Domain(Value),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellRoutingErrorV1 {
    pub code: &'static str,
    pub detail: &'static str,
}

impl ShellRequestV1<Value> {
    /// Separates lifecycle commands owned by the stable shell from opaque
    /// domain operations owned by the replaceable Engine.
    pub fn route(self) -> Result<(String, ShellRouteV1), ShellRoutingErrorV1> {
        if self.schema != SHELL_REQUEST_SCHEMA_V1 || self.request_id.trim().is_empty() {
            return Err(ShellRoutingErrorV1 {
                code: "unsupported_request",
                detail: "request schema or identity is invalid",
            });
        }
        let operation = self.operation.as_object().ok_or(ShellRoutingErrorV1 {
            code: "invalid_operation",
            detail: "operation must be a structured object",
        })?;
        let kind = operation
            .get("kind")
            .and_then(Value::as_str)
            .filter(|kind| !kind.trim().is_empty())
            .ok_or(ShellRoutingErrorV1 {
                code: "invalid_operation",
                detail: "operation kind is absent or invalid",
            })?;
        let route = match kind {
            "service_status" if operation.len() == 1 => ShellRouteV1::ServiceStatus,
            "shutdown" if operation.len() == 1 => ShellRouteV1::Shutdown,
            "service_status" | "shutdown" => {
                return Err(ShellRoutingErrorV1 {
                    code: "invalid_operation",
                    detail: "stable shell lifecycle operations do not accept domain fields",
                });
            }
            _ => ShellRouteV1::Domain(self.operation),
        };
        Ok((self.request_id, route))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShellResponseV1 {
    pub schema: String,
    pub request_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ShellErrorV1>,
    /// Short, already-redacted model-facing rendering owned by Rust. The MCP
    /// transport may forward this string but must not reinterpret its code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_manifest: Option<String>,
}

impl ShellResponseV1 {
    pub fn ready(request_id: impl Into<String>, result: Value) -> Self {
        Self {
            schema: SHELL_RESPONSE_SCHEMA_V1.to_owned(),
            request_id: request_id.into(),
            status: "ready".to_owned(),
            result: Some(result),
            error: None,
            error_manifest: None,
        }
    }

    pub fn rejected(
        request_id: impl Into<String>,
        code: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::rejected_with_error(
            request_id,
            ShellErrorV1 {
                code: code.into(),
                detail: detail.into(),
            },
        )
    }

    pub fn rejected_with_error(request_id: impl Into<String>, error: ShellErrorV1) -> Self {
        Self {
            schema: SHELL_RESPONSE_SCHEMA_V1.to_owned(),
            request_id: request_id.into(),
            status: "rejected".to_owned(),
            result: None,
            error: Some(error),
            error_manifest: None,
        }
    }

    pub fn with_error_manifest(mut self, manifest: impl Into<String>) -> Self {
        if self.status == "rejected" && self.error.is_some() && self.result.is_none() {
            self.error_manifest = Some(manifest.into());
        }
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShellErrorV1 {
    pub code: String,
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shell_can_carry_an_opaque_domain_operation_without_interpreting_it() {
        let encoded = json!({
            "schema": SHELL_REQUEST_SCHEMA_V1,
            "request_id": "request-1",
            "operation": {
                "kind": "future_domain_operation",
                "nested": {"typed": [1, 2, 3]},
            },
        });
        let request: ShellRequestV1 = serde_json::from_value(encoded.clone()).unwrap();

        assert_eq!(serde_json::to_value(request).unwrap(), encoded);
    }

    #[test]
    fn engine_public_envelope_keeps_the_operation_opaque() {
        let operation = json!({
            "kind": "future_engine_operation",
            "nested": {"typed": [1, 2, 3]},
        });
        let request = EnginePublicRequestV1::new(operation.clone());
        let encoded = serde_json::to_value(&request).unwrap();

        assert_eq!(encoded["schema"], ENGINE_PUBLIC_REQUEST_SCHEMA_V1);
        assert_eq!(encoded["operation"], operation);
        assert_eq!(
            serde_json::from_value::<EnginePublicRequestV1>(encoded).unwrap(),
            request
        );
    }

    #[test]
    fn shell_envelope_rejects_unknown_outer_fields() {
        let error = serde_json::from_value::<ShellRequestV1>(json!({
            "schema": SHELL_REQUEST_SCHEMA_V1,
            "request_id": "request-1",
            "operation": {"kind": "inspect"},
            "domain_leak": true,
        }))
        .unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn shell_routes_only_its_two_reserved_lifecycle_operations() {
        let status: ShellRequestV1 = serde_json::from_value(json!({
            "schema": SHELL_REQUEST_SCHEMA_V1,
            "request_id": "status-1",
            "operation": {"kind": "service_status"},
        }))
        .unwrap();
        assert_eq!(
            status.route().unwrap(),
            ("status-1".to_owned(), ShellRouteV1::ServiceStatus)
        );

        let domain: ShellRequestV1 = serde_json::from_value(json!({
            "schema": SHELL_REQUEST_SCHEMA_V1,
            "request_id": "observe-1",
            "operation": {"kind": "observe", "root_id": "opaque"},
        }))
        .unwrap();
        let (request_id, ShellRouteV1::Domain(operation)) = domain.route().unwrap() else {
            panic!("observe must remain an opaque domain operation")
        };
        assert_eq!(request_id, "observe-1");
        assert_eq!(operation["root_id"], "opaque");
    }

    #[test]
    fn reserved_lifecycle_operations_reject_smuggled_domain_fields() {
        let request: ShellRequestV1 = serde_json::from_value(json!({
            "schema": SHELL_REQUEST_SCHEMA_V1,
            "request_id": "shutdown-1",
            "operation": {"kind": "shutdown", "force": true},
        }))
        .unwrap();

        let error = request.route().unwrap_err();
        assert_eq!(error.code, "invalid_operation");
    }

    #[test]
    fn response_constructors_keep_ready_and_rejected_payloads_exclusive() {
        let ready = ShellResponseV1::ready("request-1", json!({"manifest": "short"}));
        assert_eq!(ready.status, "ready");
        assert!(ready.result.is_some());
        assert!(ready.error.is_none());
        assert!(ready.error_manifest.is_none());

        let rejected = ShellResponseV1::rejected("request-2", "ActionExpired", "diagnostic");
        assert_eq!(rejected.status, "rejected");
        assert!(rejected.result.is_none());
        assert!(rejected.error_manifest.is_none());
        assert_eq!(
            rejected.error.as_ref().map(|error| error.code.as_str()),
            Some("ActionExpired")
        );

        let rendered = rejected.with_error_manifest("STS-ERROR 1\ncode=ActionExpired\n");
        assert_eq!(
            rendered.error_manifest.as_deref(),
            Some("STS-ERROR 1\ncode=ActionExpired\n")
        );
    }
}
