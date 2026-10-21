use serde::Deserialize;
use serde_json::Value;

pub(super) const OBSERVE_RESPONSE_SCHEMA_V1: &str = "sts-workbench-manifest-response-v1";
pub(super) const SEARCH_RESPONSE_SCHEMA_V1: &str = "sts-workbench-search-manifest-response-v1";
pub(super) const COMMIT_RESPONSE_SCHEMA_V1: &str = "sts-workbench-preview-commit-response-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicResponseShapeV1 {
    PassThrough,
    Manifest(&'static str),
}

pub(super) struct ParsedPublicOperationV1 {
    pub operation: PublicEngineOperationV1,
    pub response: PublicResponseShapeV1,
    pub requires_run_play: bool,
}

/// Closed model-facing vocabulary owned by the replaceable Engine. This is
/// intentionally not part of the stable Supervisor protocol: adding a purely
/// semantic Engine operation must not require rebuilding the shell.
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum PublicEngineOperationV1 {
    Observe {
        root_id: String,
    },
    ObserveSelected {
        selector_name: String,
    },
    ObservePreview {
        preview_alias: String,
    },
    Inspect {
        handle: String,
    },
    InspectAlias {
        diagnostic_alias: String,
    },
    Search {
        root_id: String,
    },
    SearchSelected {
        selector_name: String,
    },
    SearchCurrent {},
    Commit {
        selector_name: String,
        receipt_id: String,
    },
    CommitPreview {
        preview_alias: String,
    },
}

pub(super) fn parse_public_operation(operation: Value) -> Result<ParsedPublicOperationV1, ()> {
    let operation = serde_json::from_value::<PublicEngineOperationV1>(operation).map_err(|_| ())?;
    let response = match &operation {
        PublicEngineOperationV1::Observe { .. }
        | PublicEngineOperationV1::ObserveSelected { .. }
        | PublicEngineOperationV1::ObservePreview { .. } => {
            PublicResponseShapeV1::Manifest(OBSERVE_RESPONSE_SCHEMA_V1)
        }
        PublicEngineOperationV1::Search { .. }
        | PublicEngineOperationV1::SearchSelected { .. }
        | PublicEngineOperationV1::SearchCurrent {} => {
            PublicResponseShapeV1::Manifest(SEARCH_RESPONSE_SCHEMA_V1)
        }
        PublicEngineOperationV1::CommitPreview { .. } => {
            PublicResponseShapeV1::Manifest(COMMIT_RESPONSE_SCHEMA_V1)
        }
        PublicEngineOperationV1::Inspect { .. }
        | PublicEngineOperationV1::InspectAlias { .. }
        | PublicEngineOperationV1::Commit { .. } => PublicResponseShapeV1::PassThrough,
    };
    let requires_run_play = matches!(
        &operation,
        PublicEngineOperationV1::Observe { .. }
            | PublicEngineOperationV1::ObserveSelected { .. }
            | PublicEngineOperationV1::ObservePreview { .. }
            | PublicEngineOperationV1::Search { .. }
            | PublicEngineOperationV1::SearchSelected { .. }
            | PublicEngineOperationV1::SearchCurrent {}
    );
    Ok(ParsedPublicOperationV1 {
        operation,
        response,
        requires_run_play,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn public_parser_accepts_only_model_facing_operations() {
        let parsed = parse_public_operation(json!({
            "kind": "observe_selected",
            "selector_name": "official",
        }))
        .unwrap();
        assert_eq!(
            parsed.operation,
            PublicEngineOperationV1::ObserveSelected {
                selector_name: "official".to_owned(),
            }
        );
        assert_eq!(
            parsed.response,
            PublicResponseShapeV1::Manifest(OBSERVE_RESPONSE_SCHEMA_V1)
        );
        assert!(parsed.requires_run_play);

        assert!(parse_public_operation(json!({
            "kind": "accept_action_preview",
            "completion": {},
        }))
        .is_err());
        assert!(parse_public_operation(json!({
            "kind": "inspect",
            "handle": "opaque",
            "smuggled": true,
        }))
        .is_err());
    }
}
