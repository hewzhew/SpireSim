//! Stable domain messages shared by the long-lived workbench runtime and the
//! replaceable Engine implementation.
//!
//! This crate deliberately contains no repository, Java, projection, or game
//! execution dependencies. Changing Engine implementation code must not make
//! the stable runtime binary stale; changing this wire contract must rebuild
//! both sides.

use serde::{Deserialize, Serialize};

pub const ENGINE_DOMAIN_REQUEST_SCHEMA_V1: &str = "sts-workbench-engine-domain-request-v1";
pub const ENGINE_PARKED_REVALIDATION_SCHEMA_V1: &str =
    "sts-workbench-engine-parked-revalidation-v1";
pub const ENGINE_NATIVE_RESET_SCHEMA_V1: &str = "sts-workbench-engine-native-reset-v1";
pub const ENGINE_NATIVE_RECOVERY_PREFLIGHT_SCHEMA_V1: &str =
    "sts-workbench-engine-native-recovery-preflight-v1";
pub const ENGINE_NATIVE_RECOVERY_PUBLICATION_SCHEMA_V1: &str =
    "sts-workbench-engine-native-recovery-publication-v1";
pub const ENGINE_NATIVE_RECOVERY_CANDIDATE_PUBLICATION_SCHEMA_V1: &str =
    "sts-workbench-engine-native-recovery-candidate-publication-v1";
pub const ENGINE_ACTION_AUTHORIZATION_SCHEMA_V1: &str =
    "sts-workbench-engine-action-authorization-v1";
pub const ENGINE_VIEW_MANIFEST_SCHEMA_V1: &str = "sts-workbench-engine-view-manifest-v1";
pub const ENGINE_ACTION_PREVIEW_RESOLUTION_SCHEMA_V1: &str =
    "sts-workbench-engine-action-preview-resolution-v1";
pub const ENGINE_ACTION_PREVIEW_COMPLETION_SCHEMA_V1: &str =
    "sts-workbench-engine-action-preview-completion-v1";
pub const ENGINE_WITNESS_PREVIEW_RESOLUTION_SCHEMA_V1: &str =
    "sts-workbench-engine-witness-preview-resolution-v1";
pub const ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1: &str =
    "sts-workbench-engine-witness-preview-completion-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineDomainRequestV1 {
    pub schema: String,
    pub operation: EngineDomainOperationV1,
}

impl EngineDomainRequestV1 {
    pub fn authorize_actions(root_id: impl Into<String>, action_ids: Vec<String>) -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::AuthorizeActions {
                root_id: root_id.into(),
                action_ids,
            },
        }
    }

    pub fn resolve_action_aliases(action_aliases: Vec<String>) -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::ResolveActionAliases { action_aliases },
        }
    }

    pub fn accept_action_preview(completion: EngineActionPreviewCompletionV1) -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::AcceptActionPreview { completion },
        }
    }

    pub fn abort_action_preview() -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::AbortActionPreview {},
        }
    }

    pub fn resolve_witness_alias(witness_alias: impl Into<String>) -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::ResolveWitnessAlias {
                witness_alias: witness_alias.into(),
            },
        }
    }

    pub fn accept_witness_preview(completion: EngineWitnessPreviewCompletionV1) -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::AcceptWitnessPreview { completion },
        }
    }

    pub fn abort_witness_preview() -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::AbortWitnessPreview {},
        }
    }

    pub fn clear_view() -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::ClearView {},
        }
    }

    pub fn revalidate_parked_runtime() -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::RevalidateParkedRuntime {},
        }
    }

    pub fn reset_to_native() -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::ResetToNative {},
        }
    }

    pub fn preflight_native_recovery(
        target_root_id: impl Into<String>,
        captured_activation_id: impl Into<String>,
    ) -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::PreflightNativeRecovery {
                target_root_id: target_root_id.into(),
                captured_activation_id: captured_activation_id.into(),
            },
        }
    }

    pub fn create_native_recovery_certificate(
        job_id: impl Into<String>,
        target_root_id: impl Into<String>,
        captured_activation_id: impl Into<String>,
    ) -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::CreateNativeRecoveryCertificate {
                job_id: job_id.into(),
                target_root_id: target_root_id.into(),
                captured_activation_id: captured_activation_id.into(),
            },
        }
    }

    pub fn retain_native_recovery_candidate(
        target_root_id: impl Into<String>,
        captured_activation_id: impl Into<String>,
        recovery_ref: impl Into<String>,
    ) -> Self {
        Self {
            schema: ENGINE_DOMAIN_REQUEST_SCHEMA_V1.to_owned(),
            operation: EngineDomainOperationV1::RetainNativeRecoveryCandidate {
                target_root_id: target_root_id.into(),
                captured_activation_id: captured_activation_id.into(),
                recovery_ref: recovery_ref.into(),
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EngineDomainOperationV1 {
    AuthorizeActions {
        root_id: String,
        action_ids: Vec<String>,
    },
    ResolveActionAliases {
        action_aliases: Vec<String>,
    },
    AcceptActionPreview {
        completion: EngineActionPreviewCompletionV1,
    },
    AbortActionPreview {},
    ResolveWitnessAlias {
        witness_alias: String,
    },
    AcceptWitnessPreview {
        completion: EngineWitnessPreviewCompletionV1,
    },
    AbortWitnessPreview {},
    ClearView {},
    RevalidateParkedRuntime {},
    ResetToNative {},
    PreflightNativeRecovery {
        target_root_id: String,
        captured_activation_id: String,
    },
    RetainNativeRecoveryCandidate {
        target_root_id: String,
        captured_activation_id: String,
        recovery_ref: String,
    },
    CreateNativeRecoveryCertificate {
        job_id: String,
        target_root_id: String,
        captured_activation_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineViewManifestV1 {
    pub schema: String,
    pub manifest: String,
}

impl EngineViewManifestV1 {
    pub fn new(manifest: impl Into<String>) -> Self {
        Self {
            schema: ENGINE_VIEW_MANIFEST_SCHEMA_V1.to_owned(),
            manifest: manifest.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineResolvedActionV1 {
    pub alias: String,
    pub action_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineActionPreviewResolutionV1 {
    pub schema: String,
    pub root_id: String,
    pub action_space_id: String,
    pub actions: Vec<EngineResolvedActionV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EnginePreviewMemberStatusV1 {
    Exact {
        receipt_id: String,
        successor_root_id: String,
    },
    Diverged {
        divergence_id: String,
    },
    SuccessorBoundaryUnsupported {},
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnginePreviewMemberSummaryV1 {
    pub action_id: String,
    pub status: EnginePreviewMemberStatusV1,
    pub exact_count: u64,
    pub total_count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineActionPreviewCompletionV1 {
    pub schema: String,
    pub root_id: String,
    pub action_space_id: String,
    pub summaries: Vec<EnginePreviewMemberSummaryV1>,
    pub detail_handle: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineWitnessPreviewResolutionV1 {
    pub schema: String,
    pub root_id: String,
    pub action_space_id: String,
    pub witness_handle: String,
    pub plan_step_count: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineWitnessPreviewStatusV1 {
    Ready,
    Diverged,
    IntentUnavailable,
    SuccessorBoundaryUnsupported,
}

impl EngineWitnessPreviewStatusV1 {
    pub fn token(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Diverged => "diverged",
            Self::IntentUnavailable => "intent_unavailable",
            Self::SuccessorBoundaryUnsupported => "successor_boundary_unsupported",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineWitnessPreviewStepStatusV1 {
    Ready,
    Diverged,
    SuccessorBoundaryUnsupported,
}

impl EngineWitnessPreviewStepStatusV1 {
    pub fn token(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Diverged => "diverged",
            Self::SuccessorBoundaryUnsupported => "successor_boundary_unsupported",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineWitnessPreviewStepV1 {
    pub status: EngineWitnessPreviewStepStatusV1,
    pub exact_count: u64,
    pub total_count: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineWitnessPreviewCompletionV1 {
    pub schema: String,
    pub root_id: String,
    pub action_space_id: String,
    pub witness_handle: String,
    pub reached_root_id: String,
    pub status: EngineWitnessPreviewStatusV1,
    pub steps: Vec<EngineWitnessPreviewStepV1>,
    pub stopped_step_ordinal: Option<u64>,
    pub stop_reason: Option<String>,
    pub detail_handle: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineActionAuthorizationV1 {
    pub schema: String,
    pub root_id: String,
    pub action_space_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineNativeRecoveryPreflightV1 {
    pub schema: String,
    pub status: EngineNativeRecoveryPreflightStatusV1,
}

impl EngineNativeRecoveryPreflightV1 {
    pub fn eligible() -> Self {
        Self {
            schema: ENGINE_NATIVE_RECOVERY_PREFLIGHT_SCHEMA_V1.to_owned(),
            status: EngineNativeRecoveryPreflightStatusV1::Eligible,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineNativeRecoveryPreflightStatusV1 {
    Eligible,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineNativeRecoveryPublicationV1 {
    pub schema: String,
    pub status: EngineNativeRecoveryPublicationStatusV1,
}

impl EngineNativeRecoveryPublicationV1 {
    pub fn published() -> Self {
        Self {
            schema: ENGINE_NATIVE_RECOVERY_PUBLICATION_SCHEMA_V1.to_owned(),
            status: EngineNativeRecoveryPublicationStatusV1::Published,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineNativeRecoveryPublicationStatusV1 {
    Published,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineNativeRecoveryCandidatePublicationV1 {
    pub schema: String,
    pub status: EngineNativeRecoveryCandidatePublicationStatusV1,
}

impl EngineNativeRecoveryCandidatePublicationV1 {
    pub fn retained() -> Self {
        Self {
            schema: ENGINE_NATIVE_RECOVERY_CANDIDATE_PUBLICATION_SCHEMA_V1.to_owned(),
            status: EngineNativeRecoveryCandidatePublicationStatusV1::Retained,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineNativeRecoveryCandidatePublicationStatusV1 {
    Retained,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineParkedRevalidationV1 {
    pub schema: String,
    pub status: EngineParkedRevalidationStatusV1,
}

impl EngineParkedRevalidationV1 {
    pub fn validated() -> Self {
        Self {
            schema: ENGINE_PARKED_REVALIDATION_SCHEMA_V1.to_owned(),
            status: EngineParkedRevalidationStatusV1::Validated,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineParkedRevalidationStatusV1 {
    Validated,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineNativeResetV1 {
    pub schema: String,
    pub status: EngineNativeResetStatusV1,
}

impl EngineNativeResetV1 {
    pub fn reset() -> Self {
        Self {
            schema: ENGINE_NATIVE_RESET_SCHEMA_V1.to_owned(),
            status: EngineNativeResetStatusV1::Reset,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineNativeResetStatusV1 {
    Reset,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requests_round_trip_and_reject_smuggled_fields() {
        let action_completion = EngineActionPreviewCompletionV1 {
            schema: ENGINE_ACTION_PREVIEW_COMPLETION_SCHEMA_V1.to_owned(),
            root_id: "root".to_owned(),
            action_space_id: "action-space".to_owned(),
            summaries: vec![EnginePreviewMemberSummaryV1 {
                action_id: "action-a".to_owned(),
                status: EnginePreviewMemberStatusV1::SuccessorBoundaryUnsupported {},
                exact_count: 0,
                total_count: 1,
            }],
            detail_handle: "detail".to_owned(),
        };
        let witness_completion = EngineWitnessPreviewCompletionV1 {
            schema: ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1.to_owned(),
            root_id: "root".to_owned(),
            action_space_id: "action-space".to_owned(),
            witness_handle: "witness".to_owned(),
            reached_root_id: "successor".to_owned(),
            status: EngineWitnessPreviewStatusV1::Ready,
            steps: vec![EngineWitnessPreviewStepV1 {
                status: EngineWitnessPreviewStepStatusV1::Ready,
                exact_count: 9,
                total_count: 9,
            }],
            stopped_step_ordinal: None,
            stop_reason: None,
            detail_handle: "detail".to_owned(),
        };
        let requests = [
            EngineDomainRequestV1::authorize_actions(
                "root",
                vec!["action-a".to_owned(), "action-b".to_owned()],
            ),
            EngineDomainRequestV1::resolve_action_aliases(vec!["A0".to_owned()]),
            EngineDomainRequestV1::accept_action_preview(action_completion),
            EngineDomainRequestV1::abort_action_preview(),
            EngineDomainRequestV1::resolve_witness_alias("W0"),
            EngineDomainRequestV1::accept_witness_preview(witness_completion),
            EngineDomainRequestV1::abort_witness_preview(),
            EngineDomainRequestV1::clear_view(),
            EngineDomainRequestV1::revalidate_parked_runtime(),
            EngineDomainRequestV1::reset_to_native(),
            EngineDomainRequestV1::preflight_native_recovery("root", "activation"),
            EngineDomainRequestV1::retain_native_recovery_candidate(
                "root",
                "activation",
                "recovery-ref",
            ),
            EngineDomainRequestV1::create_native_recovery_certificate("job", "root", "activation"),
        ];
        for request in requests {
            let encoded = serde_json::to_value(&request).unwrap();
            assert_eq!(
                serde_json::from_value::<EngineDomainRequestV1>(encoded).unwrap(),
                request
            );
        }

        let error = serde_json::from_value::<EngineDomainRequestV1>(json!({
            "schema": ENGINE_DOMAIN_REQUEST_SCHEMA_V1,
            "operation": {
                "kind": "reset_to_native",
                "path": "smuggled",
            },
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));

        let error = serde_json::from_value::<EngineDomainRequestV1>(json!({
            "schema": ENGINE_DOMAIN_REQUEST_SCHEMA_V1,
            "operation": {
                "kind": "clear_view",
                "root_id": "smuggled",
            },
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn results_round_trip_with_closed_statuses() {
        let parked = EngineParkedRevalidationV1::validated();
        let encoded = serde_json::to_value(&parked).unwrap();
        assert_eq!(encoded["status"], "validated");
        assert_eq!(
            serde_json::from_value::<EngineParkedRevalidationV1>(encoded).unwrap(),
            parked
        );

        let reset = EngineNativeResetV1::reset();
        let encoded = serde_json::to_value(&reset).unwrap();
        assert_eq!(encoded["status"], "reset");
        assert_eq!(
            serde_json::from_value::<EngineNativeResetV1>(encoded).unwrap(),
            reset
        );

        let preflight = EngineNativeRecoveryPreflightV1::eligible();
        let encoded = serde_json::to_value(&preflight).unwrap();
        assert_eq!(encoded["status"], "eligible");
        assert_eq!(
            serde_json::from_value::<EngineNativeRecoveryPreflightV1>(encoded).unwrap(),
            preflight
        );

        let publication = EngineNativeRecoveryPublicationV1::published();
        let encoded = serde_json::to_value(&publication).unwrap();
        assert_eq!(encoded["status"], "published");
        assert_eq!(
            serde_json::from_value::<EngineNativeRecoveryPublicationV1>(encoded).unwrap(),
            publication
        );

        let candidate = EngineNativeRecoveryCandidatePublicationV1::retained();
        let encoded = serde_json::to_value(&candidate).unwrap();
        assert_eq!(encoded["status"], "retained");
        assert_eq!(
            serde_json::from_value::<EngineNativeRecoveryCandidatePublicationV1>(encoded).unwrap(),
            candidate
        );

        assert!(serde_json::from_value::<EngineNativeResetV1>(json!({
            "schema": ENGINE_NATIVE_RESET_SCHEMA_V1,
            "status": "future_status",
        }))
        .is_err());

        let authorization = EngineActionAuthorizationV1 {
            schema: ENGINE_ACTION_AUTHORIZATION_SCHEMA_V1.to_owned(),
            root_id: "root".to_owned(),
            action_space_id: "action-space".to_owned(),
        };
        let encoded = serde_json::to_value(&authorization).unwrap();
        assert_eq!(
            serde_json::from_value::<EngineActionAuthorizationV1>(encoded).unwrap(),
            authorization
        );

        let manifest = EngineViewManifestV1::new("STS-MANIFEST 1\n");
        let encoded = serde_json::to_value(&manifest).unwrap();
        assert_eq!(
            serde_json::from_value::<EngineViewManifestV1>(encoded).unwrap(),
            manifest
        );
    }
}
