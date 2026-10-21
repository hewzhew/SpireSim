mod adapter;
mod card_reward_manifest;
mod card_reward_manifest_adapter;
mod combat_manifest;
mod combat_manifest_adapter;
mod combat_reward_manifest;
mod combat_reward_manifest_adapter;
mod commit;
mod diagnostic_lease;
pub mod domain_manifest;
mod domain_manifest_adapter;
mod event_manifest;
mod event_manifest_adapter;
mod lease_source;
mod manifest_lease;
mod neow_manifest;
mod neow_manifest_adapter;
mod observation;
mod preview_lease;
mod public_shell;
mod scene_manifest_adapter;
mod search;
mod search_lease;
mod selection;
mod subprocess;
#[cfg(test)]
mod test_support;
mod view_state;
mod witness_continuation_lease;
mod witness_preview_manifest;

use serde_json::Value;
use std::fmt;
use std::path::PathBuf;
use sts_java_parity_projection::{
    project_natural_map_root, project_natural_neow_root, NativeRecoveryObservationScopeV1,
    NativeRecoveryObservationStoreV1, PAIRED_PROJECTION_MEDIA_TYPE_V1,
};
use sts_paired_root::{
    ComparisonStatusV1, DecisionProfileV1, NativeRecoveryCandidateBodyV2, ProjectionComparisonV1,
    ProjectionDomainV1, RecoveryCertificateBodyV2, RecoveryCertificateCandidateV2,
    StateRootActivationBodyV2, NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1,
};
pub(crate) use sts_workbench_artifact_store as artifact;
pub use sts_workbench_engine_protocol::{
    EngineActionAuthorizationV1, EngineActionPreviewCompletionV1, EngineActionPreviewResolutionV1,
    EngineDomainOperationV1, EngineDomainRequestV1,
    EngineNativeRecoveryCandidatePublicationStatusV1, EngineNativeRecoveryCandidatePublicationV1,
    EngineNativeRecoveryPreflightStatusV1, EngineNativeRecoveryPreflightV1,
    EngineNativeRecoveryPublicationStatusV1, EngineNativeRecoveryPublicationV1,
    EngineNativeResetStatusV1, EngineNativeResetV1, EngineParkedRevalidationStatusV1,
    EngineParkedRevalidationV1, EngineViewManifestV1, EngineWitnessPreviewCompletionV1,
    EngineWitnessPreviewResolutionV1, ENGINE_ACTION_AUTHORIZATION_SCHEMA_V1,
    ENGINE_DOMAIN_REQUEST_SCHEMA_V1, ENGINE_NATIVE_RECOVERY_CANDIDATE_PUBLICATION_SCHEMA_V1,
    ENGINE_NATIVE_RECOVERY_PREFLIGHT_SCHEMA_V1, ENGINE_NATIVE_RECOVERY_PUBLICATION_SCHEMA_V1,
    ENGINE_NATIVE_RESET_SCHEMA_V1, ENGINE_PARKED_REVALIDATION_SCHEMA_V1,
    ENGINE_VIEW_MANIFEST_SCHEMA_V1,
};
use sts_workbench_supervisor::{
    EngineOutcomeV1, EnginePublicRequestV1, NativeRecoveryCapsuleReaderV1, PhysicalRuntimeCallV1,
    PhysicalRuntimeOutcomeV1, PhysicalRuntimeResultV1, SupervisorCallOutcomeV1, SupervisorCallV1,
    ENGINE_PUBLIC_REQUEST_SCHEMA_V1, PHYSICAL_RUNTIME_RESULT_SCHEMA_V1,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineNativeRecoveryValidationV1 {
    pub engine_epoch: u64,
    pub job_id: String,
    pub target_root_id: String,
    pub captured_activation_id: String,
    pub restored_activation_id: String,
    pub restored_components: Vec<ProjectionComparisonV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineNativeRecoveryValidationErrorV1 {
    code: &'static str,
    detail: String,
}

impl EngineNativeRecoveryValidationErrorV1 {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for EngineNativeRecoveryValidationErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.detail)
    }
}

/// Consume one private fresh-validator observation and rebuild the Java half
/// of a supported natural autosave root. This is deliberately below the public
/// Engine operation surface: the callback job owns when it may be invoked.
pub fn validate_native_recovery_observation_v1(
    repository: impl Into<PathBuf>,
    target_root_id: &str,
    captured_activation_id: &str,
    observation_ref: &str,
    observation_scope: &NativeRecoveryObservationScopeV1,
) -> Result<EngineNativeRecoveryValidationV1, EngineNativeRecoveryValidationErrorV1> {
    let repository = repository.into();
    let store =
        sts_paired_root::StateRootStoreV2::new(repository.join(".oracle-lab/paired-root-store"))
            .with_operation_verification_cache();
    let candidate = store
        .prepare_recovery_certificate_candidate(target_root_id, captured_activation_id)
        .map_err(|_| {
            EngineNativeRecoveryValidationErrorV1::new(
                "native_recovery_candidate_ineligible",
                "native recovery candidate is ineligible",
            )
        })?;
    let target = store.load_root(&candidate.target_root_id).map_err(|_| {
        EngineNativeRecoveryValidationErrorV1::new(
            "native_recovery_target_unavailable",
            "native recovery target is unavailable",
        )
    })?;
    let observation = NativeRecoveryObservationStoreV1::new(&repository)
        .and_then(|store| store.load(observation_ref, observation_scope))
        .map_err(|error| {
            EngineNativeRecoveryValidationErrorV1::new(
                "native_recovery_observation_unavailable",
                error,
            )
        })?;
    validate_observation_versions(&candidate, &observation.observation.java_versions).map_err(
        |error| {
            EngineNativeRecoveryValidationErrorV1::new("native_recovery_version_diverged", error)
        },
    )?;
    validate_content_profile(&candidate.content_profile, &observation.observation.frame).map_err(
        |error| {
            EngineNativeRecoveryValidationErrorV1::new(
                "native_recovery_content_profile_diverged",
                error,
            )
        },
    )?;
    let projected = match target.root.decision_profile {
        DecisionProfileV1::MapNavigation => {
            project_natural_map_root(&observation.observation.frame, &target.root.card_bindings)
        }
        DecisionProfileV1::Neow => {
            project_natural_neow_root(&observation.observation.frame, &target.root.card_bindings)
        }
        _ => {
            return Err(EngineNativeRecoveryValidationErrorV1::new(
                "native_recovery_profile_unsupported",
                "native recovery does not support this natural decision profile",
            ))
        }
    }
    .map_err(|error| {
        EngineNativeRecoveryValidationErrorV1::new("native_recovery_projection_invalid", error)
    })?;
    validate_projection_set(&store, &candidate, &projected.components)?;

    let captured = store
        .load_activation(&candidate.captured_activation_id)
        .map_err(|_| {
            EngineNativeRecoveryValidationErrorV1::new(
                "native_recovery_capture_unavailable",
                "native recovery capture activation is unavailable",
            )
        })?;
    if captured.activation.java_runtime_epoch == projected.java_runtime_epoch {
        return Err(EngineNativeRecoveryValidationErrorV1::new(
            "native_recovery_epoch_not_fresh",
            "native recovery validation did not use a fresh Java runtime",
        ));
    }
    let frame_bytes = serde_json::to_vec(&observation.observation.frame).map_err(|error| {
        EngineNativeRecoveryValidationErrorV1::new(
            "native_recovery_activation_publish_failed",
            format!("failed to encode fresh Java frame: {error}"),
        )
    })?;
    let frame = store
        .put_bytes(PAIRED_PROJECTION_MEDIA_TYPE_V1, &frame_bytes)
        .map_err(|error| {
            EngineNativeRecoveryValidationErrorV1::new(
                "native_recovery_activation_publish_failed",
                error,
            )
        })?;
    let restored = store
        .create_activation(StateRootActivationBodyV2 {
            root_id: candidate.target_root_id.clone(),
            java_runtime_epoch: projected.java_runtime_epoch,
            java_frame: frame,
            action_space_id: target.root.preflight.action_space_id,
            card_leases: projected.card_leases,
        })
        .map_err(|error| {
            EngineNativeRecoveryValidationErrorV1::new(
                "native_recovery_activation_publish_failed",
                error,
            )
        })?;
    Ok(EngineNativeRecoveryValidationV1 {
        engine_epoch: observation_scope.engine_epoch,
        job_id: observation_scope.job_id.clone(),
        target_root_id: candidate.target_root_id,
        captured_activation_id: candidate.captured_activation_id,
        restored_activation_id: restored.activation_id,
        restored_components: candidate.expected_components,
    })
}

fn validate_observation_versions(
    candidate: &RecoveryCertificateCandidateV2,
    observed: &[sts_paired_root::VersionIdentityV1],
) -> Result<(), String> {
    for version in observed {
        let expected = candidate
            .versions
            .iter()
            .find(|expected| expected.component == version.component)
            .ok_or_else(|| "fresh Java version component is absent from target root".to_owned())?;
        if !expected.identity.eq_ignore_ascii_case(&version.identity) {
            return Err(format!(
                "fresh Java build differs from target component {}",
                version.component
            ));
        }
    }
    Ok(())
}

fn validate_content_profile(expected: &str, frame: &Value) -> Result<(), String> {
    if frame["parity_internal"]["content_profile"].as_str() != Some(expected) {
        return Err("fresh Java content profile differs from target root".to_owned());
    }
    Ok(())
}

fn validate_projection_set(
    store: &sts_paired_root::StateRootStoreV2,
    candidate: &RecoveryCertificateCandidateV2,
    projected: &[sts_java_parity_projection::JavaProjectionComponentV1],
) -> Result<(), EngineNativeRecoveryValidationErrorV1> {
    if projected.len() != candidate.expected_components.len() {
        return Err(EngineNativeRecoveryValidationErrorV1::new(
            "native_recovery_projection_domain_set_diverged",
            "fresh Java projection domain set differs from target root",
        ));
    }
    for expected in &candidate.expected_components {
        if expected.status != ComparisonStatusV1::Exact {
            return Err(EngineNativeRecoveryValidationErrorV1::new(
                "native_recovery_target_not_exact",
                "native recovery target contains a non-Exact component",
            ));
        }
        let fresh = projected
            .iter()
            .find(|fresh| fresh.domain == expected.domain)
            .ok_or_else(|| {
                EngineNativeRecoveryValidationErrorV1::new(
                    "native_recovery_projection_domain_set_diverged",
                    format!(
                        "fresh Java projection omitted target domain {:?}",
                        expected.domain
                    ),
                )
            })?;
        if fresh.schema != expected.projection_schema {
            return Err(EngineNativeRecoveryValidationErrorV1::new(
                "native_recovery_projection_schema_diverged",
                format!(
                    "fresh Java projection schema differs in {:?}",
                    expected.domain
                ),
            ));
        }
        let target: Value = store
            .load_exact_projection(&candidate.target_root_id, expected.domain)
            .map_err(|error| {
                EngineNativeRecoveryValidationErrorV1::new(
                    "native_recovery_projection_unavailable",
                    error,
                )
            })?;
        if fresh.value != target {
            return Err(EngineNativeRecoveryValidationErrorV1::new(
                projection_divergence_code(expected.domain),
                format!(
                    "fresh Java projection differs from target in {:?}",
                    expected.domain
                ),
            ));
        }
    }
    Ok(())
}

fn projection_divergence_code(domain: ProjectionDomainV1) -> &'static str {
    match domain {
        ProjectionDomainV1::RunScalars => "native_recovery_run_scalars_diverged",
        ProjectionDomainV1::Map => "native_recovery_map_diverged",
        ProjectionDomainV1::MasterDeck => "native_recovery_master_deck_diverged",
        ProjectionDomainV1::Relics => "native_recovery_relics_diverged",
        ProjectionDomainV1::Potions => "native_recovery_potions_diverged",
        ProjectionDomainV1::RngStreams => "native_recovery_rng_streams_diverged",
        ProjectionDomainV1::CardPools => "native_recovery_card_pools_diverged",
        ProjectionDomainV1::Room => "native_recovery_room_diverged",
        ProjectionDomainV1::Combat => "native_recovery_combat_diverged",
        ProjectionDomainV1::ActionSpace => "native_recovery_action_space_diverged",
        ProjectionDomainV1::CardBindings => "native_recovery_card_bindings_diverged",
        ProjectionDomainV1::RuntimeSettlement => "native_recovery_settlement_diverged",
    }
}

const PARKED_REVALIDATION_CALL_ID: &str = "validate-parked-runtime-1";
const NATIVE_RESET_CALL_ID: &str = "reset-to-native-1";

enum PendingEngineJobV1 {
    ParkedRevalidation {
        call_id: String,
    },
    NativeReset {
        call_id: String,
    },
    NativeRecoveryObservation {
        call_id: String,
        candidate: RecoveryCertificateCandidateV2,
        scope: NativeRecoveryObservationScopeV1,
    },
    NativeRecoverySeal {
        call_id: String,
        candidate: RecoveryCertificateCandidateV2,
        validation: EngineNativeRecoveryValidationV1,
    },
}

/// Replaceable domain state behind the stable Engine transport. The stdio
/// binary delegates complete Execute/Resume pairs here and does not interpret
/// pending jobs itself.
pub struct EngineRuntimeV1 {
    repository: PathBuf,
    bound_epoch: Option<u64>,
    pending_job: Option<PendingEngineJobV1>,
    operation_count: u64,
    view: view_state::ViewStateV1,
}

impl EngineRuntimeV1 {
    pub fn new(repository: impl Into<PathBuf>) -> Self {
        Self {
            repository: repository.into(),
            bound_epoch: None,
            pending_job: None,
            operation_count: 0,
            view: view_state::ViewStateV1::default(),
        }
    }

    pub fn bind_epoch(&mut self, engine_epoch: u64) -> Result<(), String> {
        if engine_epoch == 0 {
            return Err("Engine epoch must be non-zero".to_owned());
        }
        match self.bound_epoch {
            None => {
                self.bound_epoch = Some(engine_epoch);
                Ok(())
            }
            Some(bound) if bound == engine_epoch => Ok(()),
            Some(_) => Err("Engine runtime is already bound to another epoch".to_owned()),
        }
    }

    pub fn execute(&mut self, input: Value) -> EngineOutcomeV1 {
        if self.pending_job.is_some() {
            return rejected("engine_job_in_progress");
        }
        if input.get("schema").and_then(Value::as_str) == Some(ENGINE_PUBLIC_REQUEST_SCHEMA_V1) {
            let request = match serde_json::from_value::<EnginePublicRequestV1>(input) {
                Ok(request) if request.schema == ENGINE_PUBLIC_REQUEST_SCHEMA_V1 => request,
                _ => return rejected("invalid_engine_public_request"),
            };
            let parsed = match public_shell::parse_public_operation(request.operation) {
                Ok(parsed) => parsed,
                Err(()) => return rejected("invalid_public_operation"),
            };
            if self.view.has_pending_execution() {
                return rejected("view_execution_in_progress");
            }
            if parsed.requires_run_play
                && adapter::ensure_run_play_adapter_fresh(&self.repository).is_err()
            {
                return rejected("runtime_prerequisite_stale");
            }
            self.operation_count = self.operation_count.saturating_add(1);
            return self.execute_public_operation(parsed);
        }
        let request = match serde_json::from_value::<EngineDomainRequestV1>(input) {
            Ok(request) => request,
            Err(_) => return rejected("invalid_engine_operation"),
        };
        if request.schema != ENGINE_DOMAIN_REQUEST_SCHEMA_V1 {
            return rejected("engine_operation_schema_mismatch");
        }
        let operation = request.operation;
        if self.view.has_pending_execution() && !self.view.permits_while_pending(&operation) {
            return rejected("view_execution_in_progress");
        }
        self.operation_count = self.operation_count.saturating_add(1);
        match operation {
            EngineDomainOperationV1::AuthorizeActions {
                root_id,
                action_ids,
            } => {
                self.view.clear();
                match observation::require_verified_actions(
                    &self.repository,
                    &root_id,
                    &action_ids,
                    self.operation_count,
                ) {
                    Ok(scope) => EngineOutcomeV1::Completed {
                        output: serde_json::to_value(EngineActionAuthorizationV1 {
                            schema: ENGINE_ACTION_AUTHORIZATION_SCHEMA_V1.to_owned(),
                            root_id,
                            action_space_id: scope.action_space_id,
                        })
                        .expect("Engine action authorization serializes"),
                    },
                    Err(_) => rejected("preview_action_authorization_failed"),
                }
            }
            EngineDomainOperationV1::ResolveActionAliases { action_aliases } => {
                match self
                    .view
                    .resolve_action_aliases(&self.repository, &action_aliases)
                {
                    Ok(resolution) => completed(resolution),
                    Err(error) => rejected(error.code),
                }
            }
            EngineDomainOperationV1::AcceptActionPreview { completion } => {
                match self
                    .view
                    .accept_action_preview(&self.repository, completion)
                {
                    Ok(manifest) => completed(manifest),
                    Err(error) => rejected(error.code),
                }
            }
            EngineDomainOperationV1::AbortActionPreview {} => {
                match self.view.abort_action_preview() {
                    Ok(()) => {
                        completed(EngineViewManifestV1::new("STS-ABORT 1\nview=observation\n"))
                    }
                    Err(error) => rejected(error.code),
                }
            }
            EngineDomainOperationV1::ResolveWitnessAlias { witness_alias } => {
                match self
                    .view
                    .resolve_witness_alias(&self.repository, &witness_alias)
                {
                    Ok(resolution) => completed(resolution),
                    Err(error) => rejected(error.code),
                }
            }
            EngineDomainOperationV1::AcceptWitnessPreview { completion } => {
                match self
                    .view
                    .accept_witness_preview(&self.repository, completion)
                {
                    Ok(manifest) => completed(manifest),
                    Err(error) => rejected(error.code),
                }
            }
            EngineDomainOperationV1::AbortWitnessPreview {} => {
                match self.view.abort_witness_preview() {
                    Ok(()) => completed(EngineViewManifestV1::new("STS-ABORT 1\nview=search\n")),
                    Err(error) => rejected(error.code),
                }
            }
            EngineDomainOperationV1::ClearView {} => {
                self.view.clear();
                completed(EngineViewManifestV1::new("STS-CLEAR 1\nview=empty\n"))
            }
            EngineDomainOperationV1::RevalidateParkedRuntime {} => {
                self.pending_job = Some(PendingEngineJobV1::ParkedRevalidation {
                    call_id: PARKED_REVALIDATION_CALL_ID.to_owned(),
                });
                EngineOutcomeV1::SupervisorCall {
                    call: SupervisorCallV1 {
                        call_id: PARKED_REVALIDATION_CALL_ID.to_owned(),
                        input: serde_json::to_value(PhysicalRuntimeCallV1::validate_parked())
                            .expect("physical runtime call serializes"),
                    },
                }
            }
            EngineDomainOperationV1::ResetToNative {} => {
                self.pending_job = Some(PendingEngineJobV1::NativeReset {
                    call_id: NATIVE_RESET_CALL_ID.to_owned(),
                });
                EngineOutcomeV1::SupervisorCall {
                    call: SupervisorCallV1 {
                        call_id: NATIVE_RESET_CALL_ID.to_owned(),
                        input: serde_json::to_value(PhysicalRuntimeCallV1::reset_to_native())
                            .expect("physical runtime call serializes"),
                    },
                }
            }
            EngineDomainOperationV1::PreflightNativeRecovery {
                target_root_id,
                captured_activation_id,
            } => {
                let store = sts_paired_root::StateRootStoreV2::new(
                    self.repository.join(".oracle-lab/paired-root-store"),
                )
                .with_operation_verification_cache();
                match store.prepare_recovery_certificate_candidate(
                    &target_root_id,
                    &captured_activation_id,
                ) {
                    Ok(_) => EngineOutcomeV1::Completed {
                        output: serde_json::to_value(EngineNativeRecoveryPreflightV1::eligible())
                            .expect("native recovery preflight serializes"),
                    },
                    Err(_) => rejected("native_recovery_candidate_ineligible"),
                }
            }
            EngineDomainOperationV1::RetainNativeRecoveryCandidate {
                target_root_id,
                captured_activation_id,
                recovery_ref,
            } => self.retain_native_recovery_candidate(
                target_root_id,
                captured_activation_id,
                &recovery_ref,
            ),
            EngineDomainOperationV1::CreateNativeRecoveryCertificate {
                job_id,
                target_root_id,
                captured_activation_id,
            } => self.begin_native_recovery_certificate(
                job_id,
                target_root_id,
                captured_activation_id,
            ),
        }
    }

    fn execute_public_operation(
        &mut self,
        parsed: public_shell::ParsedPublicOperationV1,
    ) -> EngineOutcomeV1 {
        use public_shell::PublicEngineOperationV1 as Public;

        let outcome = match parsed.operation {
            Public::Inspect { handle } => {
                match sts_workbench_artifact_store::inspect_artifact(&self.repository, &handle) {
                    Ok(output) => EngineOutcomeV1::Completed { output },
                    Err(_) => rejected("inspect_artifact_failed"),
                }
            }
            Public::InspectAlias { diagnostic_alias } => {
                let handle = match self.view.inspect_alias(&self.repository, &diagnostic_alias) {
                    Ok(handle) => handle,
                    Err(error) => return rejected(error.code),
                };
                match sts_workbench_artifact_store::inspect_artifact(&self.repository, &handle) {
                    Ok(output) => EngineOutcomeV1::Completed { output },
                    Err(_) => rejected("inspect_diagnostic_alias_failed"),
                }
            }
            Public::Observe { root_id } => {
                self.view.clear();
                let observed = match self
                    .load_observation(&root_id, domain_manifest::ManifestAuthorityV1::ObserveOnly)
                {
                    Ok(observed) => observed,
                    Err(_) => return rejected("observe_explicit_root_failed"),
                };
                match self.view.accept_explicit_observation(observed) {
                    Ok(manifest) => completed(manifest),
                    Err(error) => rejected(error.code),
                }
            }
            Public::ObserveSelected { selector_name } => {
                self.view.clear();
                let observed = selection::run_at_selected_root_with_guard(
                    &self.repository,
                    &selector_name,
                    "observation",
                    |root_id| {
                        self.load_observation(
                            root_id,
                            domain_manifest::ManifestAuthorityV1::LabHead,
                        )
                    },
                );
                match observed {
                    Ok((observed, selected)) => {
                        match self.view.accept_selected_observation(observed, selected) {
                            Ok(manifest) => completed(manifest),
                            Err(error) => rejected(error.code),
                        }
                    }
                    Err(_) => rejected("observe_selected_root_failed"),
                }
            }
            Public::ObservePreview { preview_alias } => {
                let successor = match self
                    .view
                    .preview_successor(&self.repository, &preview_alias)
                {
                    Ok(successor) => successor,
                    Err(error) => return rejected(error.code),
                };
                let observed = match self.load_observation(
                    &successor,
                    domain_manifest::ManifestAuthorityV1::ObserveOnly,
                ) {
                    Ok(observed) => observed,
                    Err(_) => return rejected("observe_preview_successor_failed"),
                };
                match self.view.accept_preview_observation(observed) {
                    Ok(manifest) => completed(manifest),
                    Err(error) => rejected(error.code),
                }
            }
            Public::Search { root_id } => {
                self.view.clear();
                let searched = match self.load_search(&root_id) {
                    Ok(searched) => searched,
                    Err(_) => return rejected("search_explicit_root_failed"),
                };
                match self.view.accept_explicit_search(&self.repository, searched) {
                    Ok(manifest) => completed(manifest),
                    Err(error) => rejected(error.code),
                }
            }
            Public::SearchSelected { selector_name } => {
                self.view.clear();
                let searched = selection::run_at_selected_root_with_guard(
                    &self.repository,
                    &selector_name,
                    "search",
                    |root_id| self.load_search(root_id),
                );
                match searched {
                    Ok((searched, selected)) => {
                        match self
                            .view
                            .accept_selected_search(&self.repository, searched, selected)
                        {
                            Ok(manifest) => completed(manifest),
                            Err(error) => rejected(error.code),
                        }
                    }
                    Err(_) => rejected("search_selected_root_failed"),
                }
            }
            Public::SearchCurrent {} => {
                let scope = match self.view.current_search_scope(&self.repository) {
                    Ok(scope) => scope,
                    Err(error) => return rejected(error.code),
                };
                let searched = match self.load_search(&scope.root_id) {
                    Ok(searched) => searched,
                    Err(_) => return rejected("search_current_view_failed"),
                };
                match self
                    .view
                    .accept_current_search(&self.repository, searched, scope.source)
                {
                    Ok(manifest) => completed(manifest),
                    Err(error) => rejected(error.code),
                }
            }
            Public::Commit {
                selector_name,
                receipt_id,
            } => match self
                .view
                .commit_selected(&self.repository, &selector_name, &receipt_id)
            {
                Ok(output) => EngineOutcomeV1::Completed { output },
                Err(error) => rejected(error.code),
            },
            Public::CommitPreview { preview_alias } => {
                match self.view.commit_preview(&self.repository, &preview_alias) {
                    Ok(manifest) => completed(manifest),
                    Err(error) => rejected(error.code),
                }
            }
        };
        project_public_outcome(outcome, parsed.response)
    }

    fn begin_native_recovery_certificate(
        &mut self,
        job_id: String,
        target_root_id: String,
        captured_activation_id: String,
    ) -> EngineOutcomeV1 {
        let Some(engine_epoch) = self.bound_epoch else {
            return rejected("engine_epoch_unbound");
        };
        let store = sts_paired_root::StateRootStoreV2::new(self.paired_store_root())
            .with_operation_verification_cache();
        let candidate = match store
            .prepare_recovery_certificate_candidate(&target_root_id, &captured_activation_id)
        {
            Ok(candidate) => candidate,
            Err(_) => return rejected("native_recovery_candidate_ineligible"),
        };
        let scope = match NativeRecoveryObservationScopeV1::new(engine_epoch, job_id) {
            Ok(scope) => scope,
            Err(_) => return rejected("native_recovery_job_identity_invalid"),
        };
        let call_id = format!("{}-begin", scope.job_id);
        self.pending_job = Some(PendingEngineJobV1::NativeRecoveryObservation {
            call_id: call_id.clone(),
            candidate,
            scope,
        });
        EngineOutcomeV1::SupervisorCall {
            call: SupervisorCallV1 {
                call_id,
                input: serde_json::to_value(
                    PhysicalRuntimeCallV1::begin_native_recovery_validation(),
                )
                .expect("physical runtime call serializes"),
            },
        }
    }

    fn retain_native_recovery_candidate(
        &self,
        target_root_id: String,
        captured_activation_id: String,
        recovery_ref: &str,
    ) -> EngineOutcomeV1 {
        let store = sts_paired_root::StateRootStoreV2::new(self.paired_store_root())
            .with_operation_verification_cache();
        let prepared = match store
            .prepare_recovery_certificate_candidate(&target_root_id, &captured_activation_id)
        {
            Ok(candidate) => candidate,
            Err(_) => return rejected("native_recovery_candidate_ineligible"),
        };
        let capsule = match NativeRecoveryCapsuleReaderV1::new(&self.paired_store_root())
            .and_then(|reader| reader.load_capsule(recovery_ref))
        {
            Ok(capsule) => capsule,
            Err(_) => return rejected("native_recovery_capsule_invalid"),
        };
        let expected_directories = NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        if capsule.relative_directories != expected_directories {
            return rejected("native_recovery_capsule_scope_mismatch");
        }
        let native_autosave_sha256 = match store.native_storage_autosave_identity(&capsule.bundle) {
            Ok(identity) => identity,
            Err(_) => return rejected("native_recovery_capsule_payload_invalid"),
        };
        let candidate = NativeRecoveryCandidateBodyV2 {
            target_root_id: prepared.target_root_id,
            captured_activation_id: prepared.captured_activation_id,
            versions: prepared.versions,
            content_profile: prepared.content_profile,
            recovery_ref: capsule.recovery_ref,
            native_storage_bundle: capsule.bundle,
            native_autosave_sha256,
        };
        if store.create_native_recovery_candidate(candidate).is_err() {
            return rejected("native_recovery_candidate_publication_failed");
        }
        completed(EngineNativeRecoveryCandidatePublicationV1::retained())
    }

    pub fn resume_supervisor_call(
        &mut self,
        call_id: &str,
        outcome: SupervisorCallOutcomeV1,
    ) -> EngineOutcomeV1 {
        let Some(pending) = self.pending_job.take() else {
            return rejected("unexpected_supervisor_resume");
        };
        let expected_call_id = match &pending {
            PendingEngineJobV1::ParkedRevalidation { call_id }
            | PendingEngineJobV1::NativeReset { call_id }
            | PendingEngineJobV1::NativeRecoveryObservation { call_id, .. }
            | PendingEngineJobV1::NativeRecoverySeal { call_id, .. } => call_id,
        };
        if call_id != expected_call_id {
            return rejected("supervisor_call_mismatch");
        }
        let output = match outcome {
            SupervisorCallOutcomeV1::Completed { output } => output,
            SupervisorCallOutcomeV1::Rejected { .. } => {
                return rejected("physical_runtime_call_rejected");
            }
        };
        let result = match serde_json::from_value::<PhysicalRuntimeResultV1>(output) {
            Ok(result) if result.schema == PHYSICAL_RUNTIME_RESULT_SCHEMA_V1 => result,
            Ok(_) => return rejected("physical_runtime_result_mismatch"),
            Err(_) => return rejected("invalid_physical_runtime_result"),
        };
        match (pending, result.outcome) {
            (
                PendingEngineJobV1::ParkedRevalidation { .. },
                PhysicalRuntimeOutcomeV1::ParkedValidated {},
            ) => EngineOutcomeV1::Completed {
                output: serde_json::to_value(EngineParkedRevalidationV1::validated())
                    .expect("Engine parked revalidation serializes"),
            },
            (PendingEngineJobV1::NativeReset { .. }, PhysicalRuntimeOutcomeV1::NativeReset {}) => {
                EngineOutcomeV1::Completed {
                    output: serde_json::to_value(EngineNativeResetV1::reset())
                        .expect("Engine native reset serializes"),
                }
            }
            (
                PendingEngineJobV1::NativeRecoveryObservation {
                    candidate, scope, ..
                },
                PhysicalRuntimeOutcomeV1::NativeRecoveryValidatorReady { observation_ref },
            ) => {
                let validation = match validate_native_recovery_observation_v1(
                    &self.repository,
                    &candidate.target_root_id,
                    &candidate.captured_activation_id,
                    &observation_ref,
                    &scope,
                ) {
                    Ok(validation) => validation,
                    Err(error) => return rejected(error.code()),
                };
                let call_id = format!("{}-seal", scope.job_id);
                self.pending_job = Some(PendingEngineJobV1::NativeRecoverySeal {
                    call_id: call_id.clone(),
                    candidate,
                    validation,
                });
                EngineOutcomeV1::SupervisorCall {
                    call: SupervisorCallV1 {
                        call_id,
                        input: serde_json::to_value(
                            PhysicalRuntimeCallV1::seal_native_recovery_validation(),
                        )
                        .expect("physical runtime call serializes"),
                    },
                }
            }
            (
                PendingEngineJobV1::NativeRecoverySeal {
                    candidate,
                    validation,
                    ..
                },
                PhysicalRuntimeOutcomeV1::NativeRecoverySealed { recovery_ref },
            ) => self.publish_native_recovery_certificate(candidate, validation, &recovery_ref),
            _ => rejected("physical_runtime_result_mismatch"),
        }
    }

    fn publish_native_recovery_certificate(
        &self,
        candidate: RecoveryCertificateCandidateV2,
        validation: EngineNativeRecoveryValidationV1,
        recovery_ref: &str,
    ) -> EngineOutcomeV1 {
        if self.bound_epoch != Some(validation.engine_epoch) || validation.job_id.is_empty() {
            return rejected("native_recovery_job_scope_mismatch");
        }
        let capsule = match NativeRecoveryCapsuleReaderV1::new(&self.paired_store_root())
            .and_then(|reader| reader.load_capsule(recovery_ref))
        {
            Ok(capsule) => capsule,
            Err(_) => return rejected("native_recovery_capsule_invalid"),
        };
        let expected_directories = NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        if capsule.relative_directories != expected_directories {
            return rejected("native_recovery_capsule_scope_mismatch");
        }
        if validation.target_root_id != candidate.target_root_id
            || validation.captured_activation_id != candidate.captured_activation_id
            || validation.restored_components != candidate.expected_components
        {
            return rejected("native_recovery_validation_scope_mismatch");
        }
        let store = sts_paired_root::StateRootStoreV2::new(self.paired_store_root())
            .with_operation_verification_cache();
        let java_recovery = match store
            .native_storage_recovery_payload(&capsule.bundle, &candidate.content_profile)
        {
            Ok(payload) => payload,
            Err(_) => return rejected("native_recovery_capsule_payload_invalid"),
        };
        let certificate = RecoveryCertificateBodyV2 {
            target_root_id: candidate.target_root_id,
            versions: candidate.versions,
            java_recovery,
            captured_activation_id: candidate.captured_activation_id,
            restored_activation_id: validation.restored_activation_id,
            restored_components: validation.restored_components,
        };
        if store.create_recovery_certificate(certificate).is_err() {
            return rejected("native_recovery_certificate_publication_failed");
        }
        EngineOutcomeV1::Completed {
            output: serde_json::to_value(EngineNativeRecoveryPublicationV1::published())
                .expect("native recovery publication serializes"),
        }
    }

    fn load_observation(
        &self,
        root_id: &str,
        authority: domain_manifest::ManifestAuthorityV1,
    ) -> Result<view_state::ObservationV1, String> {
        let observed = observation::observe_explicit_root(
            &self.repository,
            root_id,
            self.operation_count,
            authority,
        )?;
        Ok(view_state::ObservationV1 {
            root_id: observed.root_id,
            action_space_id: observed.action_space_id,
            detail_handle: observed.detail_handle,
            manifest: observed.manifest,
        })
    }

    fn load_search(&self, root_id: &str) -> Result<search::SearchExecutionV1, String> {
        search::search_explicit_root(&self.repository, root_id, self.operation_count)
    }

    fn paired_store_root(&self) -> PathBuf {
        self.repository.join(".oracle-lab/paired-root-store")
    }
}

fn rejected(code: &str) -> EngineOutcomeV1 {
    EngineOutcomeV1::Rejected {
        code: code.to_owned(),
    }
}

fn completed(value: impl serde::Serialize) -> EngineOutcomeV1 {
    EngineOutcomeV1::Completed {
        output: serde_json::to_value(value).expect("Engine domain response serializes"),
    }
}

fn project_public_outcome(
    outcome: EngineOutcomeV1,
    shape: public_shell::PublicResponseShapeV1,
) -> EngineOutcomeV1 {
    let EngineOutcomeV1::Completed { output } = outcome else {
        return outcome;
    };
    match shape {
        public_shell::PublicResponseShapeV1::PassThrough => EngineOutcomeV1::Completed { output },
        public_shell::PublicResponseShapeV1::Manifest(schema) => {
            let manifest = match serde_json::from_value::<EngineViewManifestV1>(output) {
                Ok(manifest) if manifest.schema == ENGINE_VIEW_MANIFEST_SCHEMA_V1 => manifest,
                _ => return rejected("invalid_public_manifest_projection"),
            };
            EngineOutcomeV1::Completed {
                output: serde_json::json!({
                    "schema": schema,
                    "manifest": manifest.manifest,
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use sts_java_parity_projection::{
        project_natural_map_root, project_natural_neow_root, JavaStableDecisionProjectionV1,
        NativeRecoveryObservationStoreV1,
    };
    use sts_paired_root::{
        card_binding_id, CardBindingEvidenceV1, CrossRuntimeCardBindingV1, RootPreflightV1,
        RootProvenanceV1, RustCardLineageV1, RustStateSnapshotV2, StateRootActivationArtifactV2,
        StateRootArtifactV2, StateRootBodyV2, StateRootStoreV2, VersionIdentityV1,
        WorldProvenanceV1,
    };
    use uuid::Uuid;

    #[cfg(windows)]
    #[derive(Debug)]
    struct FakeCapsuleValidatorV1;

    #[cfg(windows)]
    impl sts_workbench_supervisor::NativeRecoveryValidatorV1 for FakeCapsuleValidatorV1 {
        fn shutdown(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    struct NativeRecoveryFixture {
        repository: PathBuf,
        root: StateRootArtifactV2,
        captured: StateRootActivationArtifactV2,
    }

    impl NativeRecoveryFixture {
        fn new() -> Self {
            let captured_frame = map_frame("java-captured", "java-card-captured", 99);
            Self::new_for(
                DecisionProfileV1::MapNavigation,
                captured_frame,
                project_natural_map_root,
            )
        }

        fn new_neow() -> Self {
            let captured_frame = completed_neow_frame("java-captured", "java-card-captured", 199);
            Self::new_for(
                DecisionProfileV1::Neow,
                captured_frame,
                project_natural_neow_root,
            )
        }

        fn new_for<Project>(
            profile: DecisionProfileV1,
            captured_frame: Value,
            project: Project,
        ) -> Self
        where
            Project: FnOnce(
                &Value,
                &[CrossRuntimeCardBindingV1],
            ) -> Result<JavaStableDecisionProjectionV1, String>,
        {
            let repository =
                std::env::temp_dir().join(format!("sts-engine-native-recovery-{}", Uuid::new_v4()));
            fs::create_dir_all(&repository).unwrap();
            let store = paired_store(&repository);
            let evidence = store.put_bytes("application/json", b"{}").unwrap();
            let mut binding = CrossRuntimeCardBindingV1 {
                binding_id: String::new(),
                rust_lineage: RustCardLineageV1::MasterDeck {
                    acquisition_ordinal: 0,
                },
                card_id: "Strike_R".to_owned(),
                upgrades: 0,
                evidence: CardBindingEvidenceV1::MasterDeckObservation {
                    java_master_deck_ordinal: 0,
                    rust_master_deck_ordinal: 0,
                    java_projection: evidence.clone(),
                    rust_projection: evidence,
                },
            };
            binding.binding_id = card_binding_id(&binding).unwrap();
            let projection = project(&captured_frame, &[binding.clone()]).unwrap();
            let components = projection
                .components
                .into_iter()
                .map(|component| {
                    let bytes = serde_json::to_vec(&component.value).unwrap();
                    let blob = store
                        .put_bytes(PAIRED_PROJECTION_MEDIA_TYPE_V1, &bytes)
                        .unwrap();
                    ProjectionComparisonV1 {
                        domain: component.domain,
                        projection_schema: component.schema.to_owned(),
                        java: blob.clone(),
                        rust: blob,
                        status: ComparisonStatusV1::Exact,
                        first_difference: None,
                    }
                })
                .collect();
            let payload = repository.join("payload");
            fs::create_dir_all(&payload).unwrap();
            let bundle = store.put_directory(&payload).unwrap();
            let root = store
                .create_root(StateRootBodyV2 {
                    provenance: RootProvenanceV1::Probe,
                    world_provenance: WorldProvenanceV1::Natural,
                    parent: None,
                    decision_profile: profile,
                    versions: root_versions(),
                    content_profile: "all_unlocked".to_owned(),
                    rust_snapshot: RustStateSnapshotV2 {
                        bundle,
                        entrypoint: "exact_checkpoint".to_owned(),
                        source_identity: Vec::new(),
                    },
                    preflight: RootPreflightV1 {
                        action_space_id: "action-space:native-recovery-fixture".to_owned(),
                        components,
                        uncovered: Vec::new(),
                    },
                    card_bindings: vec![binding.clone()],
                    capabilities: Vec::new(),
                })
                .unwrap();
            let frame = store
                .put_bytes(
                    PAIRED_PROJECTION_MEDIA_TYPE_V1,
                    &serde_json::to_vec(&captured_frame).unwrap(),
                )
                .unwrap();
            let captured = store
                .create_activation(StateRootActivationBodyV2 {
                    root_id: root.root_id.clone(),
                    java_runtime_epoch: projection.java_runtime_epoch,
                    java_frame: frame,
                    action_space_id: root.root.preflight.action_space_id.clone(),
                    card_leases: vec![sts_paired_root::JavaCardLeaseV1 {
                        binding_id: binding.binding_id,
                        java_card_uuid: "java-card-captured".to_owned(),
                    }],
                })
                .unwrap();
            Self {
                repository,
                root,
                captured,
            }
        }

        fn publish(&self, scope: &NativeRecoveryObservationScopeV1, frame: Value) -> String {
            NativeRecoveryObservationStoreV1::new(&self.repository)
                .unwrap()
                .publish(scope.clone(), java_versions(), frame)
                .unwrap()
                .observation_ref
        }

        #[cfg(windows)]
        fn seal_capsule(&self, relative_directories: &[&str], job_id: &str) -> String {
            use sts_workbench_supervisor::{
                NativeRecoveryCapsuleJobV1, NativeRecoveryCapsuleRegistryV1,
                NativeRecoveryValidatorOpenV1, OwnedStorageRegistryV1, PhysicalRuntimeOutcomeV1,
                RepositoryLifecycleOwnerV1,
            };

            let game = self.repository.join("game");
            fs::create_dir_all(&game).unwrap();
            let owner = RepositoryLifecycleOwnerV1::acquire(&self.repository).unwrap();
            let storage_registry = OwnedStorageRegistryV1::new(&owner, &game);
            let source = storage_registry.create().unwrap();
            for relative in relative_directories {
                fs::create_dir_all(source.root().join(relative)).unwrap();
            }
            if relative_directories.contains(&"preferences") {
                fs::write(source.root().join("preferences/prefs"), b"prefs").unwrap();
            }
            if relative_directories.contains(&"saves") {
                fs::write(source.root().join("saves/IRONCLAD.autosave"), b"save").unwrap();
            }
            let scope = source
                .bind_native_recovery_scope(relative_directories)
                .unwrap();
            let capsules = NativeRecoveryCapsuleRegistryV1::new(
                &owner,
                storage_registry,
                &self.repository.join(".oracle-lab/paired-root-store"),
            )
            .unwrap();
            let mut job = NativeRecoveryCapsuleJobV1::new(capsules, scope, 7, job_id).unwrap();
            job.begin(
                7,
                job_id,
                || Ok(()),
                |_| {
                    Ok(NativeRecoveryValidatorOpenV1::new(
                        FakeCapsuleValidatorV1,
                        format!(
                            "workbench-native-recovery-observation-v1-{}",
                            "a".repeat(64)
                        ),
                    ))
                },
            )
            .unwrap();
            match job.seal(7, job_id, || Ok(())).unwrap().outcome {
                PhysicalRuntimeOutcomeV1::NativeRecoverySealed { recovery_ref } => recovery_ref,
                other => panic!("unexpected capsule result: {other:?}"),
            }
        }
    }

    impl Drop for NativeRecoveryFixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.repository).unwrap();
        }
    }

    fn paired_store(repository: &Path) -> StateRootStoreV2 {
        StateRootStoreV2::new(repository.join(".oracle-lab/paired-root-store"))
    }

    fn root_versions() -> Vec<VersionIdentityV1> {
        vec![
            VersionIdentityV1 {
                component: "communicationmod_jar".to_owned(),
                identity: "mod-jar".to_owned(),
            },
            VersionIdentityV1 {
                component: "communicationmod_source".to_owned(),
                identity: "mod-source".to_owned(),
            },
            VersionIdentityV1 {
                component: "run_play_workbench".to_owned(),
                identity: "workbench".to_owned(),
            },
            VersionIdentityV1 {
                component: "rust_rng_semantics".to_owned(),
                identity: "rng".to_owned(),
            },
            VersionIdentityV1 {
                component: "rust_simulator_semantics".to_owned(),
                identity: "simulator".to_owned(),
            },
            VersionIdentityV1 {
                component: "slay_the_spire_jar".to_owned(),
                identity: "game-jar".to_owned(),
            },
        ]
    }

    fn java_versions() -> Vec<VersionIdentityV1> {
        root_versions()
            .into_iter()
            .filter(|version| {
                matches!(
                    version.component.as_str(),
                    "communicationmod_jar" | "communicationmod_source" | "slay_the_spire_jar"
                )
            })
            .collect()
    }

    fn map_frame(runtime_epoch: &str, card_uuid: &str, gold: i64) -> Value {
        serde_json::json!({
            "kind": "state_frame",
            "profile": sts_java_parity_protocol::RUN_CONTROL_PROFILE,
            "protocol": {
                "name": "communicationmod.parity",
                "major": 1,
                "minor": sts_java_parity_protocol::PROTOCOL_MINOR,
                "schema_id": "communicationmod.parity-frame.v1"
            },
            "producer_session_id": runtime_epoch,
            "connection_id": "connection-1",
            "capture_id": "capture-1",
            "frame_id": 7,
            "boundary": {
                "kind": "run_decision",
                "stable": true,
                "evidence": {"screen_type": "MAP", "room_phase": "COMPLETE"}
            },
            "public_base": {
                "run": {
                    "ascension": 20,
                    "character": "IRONCLAD",
                    "act": 1,
                    "floor": 0,
                    "current_hp": 80,
                    "max_hp": 80,
                    "gold": gold,
                    "final_act_available": true,
                    "has_ruby_key": false,
                    "has_sapphire_key": false,
                    "has_emerald_key": false
                },
                "decision": {"kind": "map"},
                "map": {
                    "current_x": -1,
                    "current_y": -1,
                    "boss": "TheGuardian",
                    "nodes": [{
                        "x": 0,
                        "y": 0,
                        "room_type": "com.megacrit.cardcrawl.rooms.MonsterRoom",
                        "has_emerald_key": false,
                        "edges": [{"destination_x": 3, "destination_y": 16}]
                    }]
                }
            },
            "parity_internal": {
                "run_seed": "-1",
                "content_profile": "all_unlocked",
                "master_deck": [{
                    "ordinal": 0,
                    "card_uuid": card_uuid,
                    "card_id": "Strike_R",
                    "upgrades": 0
                }],
                "relics": [{"index": 0, "relic_id": "Burning Blood", "counter": -1}],
                "potions": [{"slot_index": 0, "empty": true}],
                "card_pools": {
                    "common": ["Anger"],
                    "uncommon": ["Battle Trance"],
                    "rare": ["Feed"],
                    "colorless_combat": ["Finesse"]
                }
            },
            "rng_internal": {
                "streams": [{
                    "owner": "com.megacrit.cardcrawl.dungeons.AbstractDungeon.mapRng",
                    "scope": "run",
                    "state_hex": ["00", "01"],
                    "counter": 9
                }],
                "run_streams": [{
                    "owner": "com.megacrit.cardcrawl.neow.NeowEvent.rng",
                    "scope": "run",
                    "state_hex": ["02", "03"],
                    "counter": 1
                }]
            },
            "action_space": {"actions": [{
                "action_id": "map_node:0:0",
                "kind": "choose_map_node",
                "map_x": 0,
                "map_y": 0,
                "map_room_type": "com.megacrit.cardcrawl.rooms.MonsterRoom",
                "parameter_schema": null
            }]}
        })
    }

    fn completed_neow_frame(runtime_epoch: &str, card_uuid: &str, gold: i64) -> Value {
        let mut frame = map_frame(runtime_epoch, card_uuid, gold);
        frame["boundary"]["evidence"] = serde_json::json!({
            "screen_type": "EVENT",
            "room_phase": "EVENT",
        });
        frame["public_base"]["decision"] = serde_json::json!({
            "kind": "event",
            "event_id": "Neow",
            "event_screen_index": 99,
        });
        frame["action_space"]["actions"] = serde_json::json!([{
            "action_id": "event_option:Neow:index:99:0",
            "kind": "choose_event_option",
            "event_id": "Neow",
            "event_screen_index": 99,
            "option_index": 0,
        }]);
        frame
    }

    #[test]
    fn public_inspect_envelope_round_trips_without_entering_the_shared_domain_protocol() {
        let request = EnginePublicRequestV1::new(serde_json::json!({
            "kind": "inspect",
            "handle": "workbench-artifact-v1-example",
        }));
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(encoded["schema"], ENGINE_PUBLIC_REQUEST_SCHEMA_V1);
        assert_eq!(encoded["operation"]["kind"], "inspect");
        assert_eq!(
            serde_json::from_value::<EnginePublicRequestV1>(encoded).unwrap(),
            request
        );
    }

    #[test]
    fn public_manifest_projection_preserves_the_existing_shell_schema() {
        let outcome = project_public_outcome(
            completed(EngineViewManifestV1::new("STS-MANIFEST 1\nscene=fixture\n")),
            public_shell::PublicResponseShapeV1::Manifest(public_shell::OBSERVE_RESPONSE_SCHEMA_V1),
        );
        let EngineOutcomeV1::Completed { output } = outcome else {
            panic!("valid public manifest must remain completed")
        };
        assert_eq!(output["schema"], "sts-workbench-manifest-response-v1");
        assert_eq!(output["manifest"], "STS-MANIFEST 1\nscene=fixture\n");
        assert!(output.as_object().is_some_and(|object| object.len() == 2));
    }

    #[test]
    fn parked_revalidation_request_and_result_are_closed() {
        let request = EngineDomainRequestV1::revalidate_parked_runtime();
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "schema": ENGINE_DOMAIN_REQUEST_SCHEMA_V1,
                "operation": {"kind": "revalidate_parked_runtime"},
            })
        );
        assert_eq!(
            serde_json::from_value::<EngineDomainRequestV1>(encoded).unwrap(),
            request
        );
        let error = serde_json::from_value::<EngineDomainRequestV1>(serde_json::json!({
            "schema": ENGINE_DOMAIN_REQUEST_SCHEMA_V1,
            "operation": {
                "kind": "revalidate_parked_runtime",
                "frame": "smuggled",
            },
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));

        let result = EngineParkedRevalidationV1::validated();
        let encoded = serde_json::to_value(&result).unwrap();
        assert_eq!(
            serde_json::from_value::<EngineParkedRevalidationV1>(encoded).unwrap(),
            result
        );
    }

    #[test]
    fn native_reset_is_one_closed_callback() {
        let mut runtime = EngineRuntimeV1::new("unused-repository");
        let request = serde_json::to_value(EngineDomainRequestV1::reset_to_native()).unwrap();
        let EngineOutcomeV1::SupervisorCall { call } = runtime.execute(request.clone()) else {
            panic!("reset must request one physical callback")
        };
        assert_eq!(call.call_id, NATIVE_RESET_CALL_ID);
        assert_eq!(
            serde_json::from_value::<PhysicalRuntimeCallV1>(call.input)
                .unwrap()
                .operation,
            sts_workbench_supervisor::PhysicalRuntimeOperationV1::ResetToNative {}
        );
        assert!(matches!(
            runtime.execute(request.clone()),
            EngineOutcomeV1::Rejected { code } if code == "engine_job_in_progress"
        ));
        let output = runtime.resume_supervisor_call(
            &call.call_id,
            SupervisorCallOutcomeV1::Completed {
                output: serde_json::to_value(PhysicalRuntimeResultV1::native_reset()).unwrap(),
            },
        );
        let EngineOutcomeV1::Completed { output } = output else {
            panic!("reset callback must complete the Engine job")
        };
        assert_eq!(
            serde_json::from_value::<EngineNativeResetV1>(output).unwrap(),
            EngineNativeResetV1::reset()
        );
        assert!(matches!(
            runtime.execute(request),
            EngineOutcomeV1::SupervisorCall { .. }
        ));
    }

    #[test]
    fn runtime_owns_pending_callback_and_clears_rejected_result() {
        let mut runtime = EngineRuntimeV1::new("unused-repository");
        let request =
            serde_json::to_value(EngineDomainRequestV1::revalidate_parked_runtime()).unwrap();
        let EngineOutcomeV1::SupervisorCall { call } = runtime.execute(request.clone()) else {
            panic!("revalidation must request one physical callback")
        };
        assert_eq!(call.call_id, PARKED_REVALIDATION_CALL_ID);
        assert!(matches!(
            runtime.execute(request.clone()),
            EngineOutcomeV1::Rejected { code } if code == "engine_job_in_progress"
        ));
        assert!(matches!(
            runtime.resume_supervisor_call(
                &call.call_id,
                SupervisorCallOutcomeV1::Completed {
                    output: serde_json::json!({"invalid": true}),
                },
            ),
            EngineOutcomeV1::Rejected { code } if code == "invalid_physical_runtime_result"
        ));
        assert!(matches!(
            runtime.execute(request),
            EngineOutcomeV1::SupervisorCall { .. }
        ));
    }

    #[test]
    fn unknown_internal_domain_fields_are_rejected() {
        let error = serde_json::from_value::<EngineDomainRequestV1>(serde_json::json!({
            "schema": ENGINE_DOMAIN_REQUEST_SCHEMA_V1,
            "operation": {
                "kind": "authorize_actions",
                "root_id": "root",
                "action_ids": [],
                "unexpected": true,
            }
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn public_operations_and_internal_authorization_fail_closed() {
        let mut runtime = EngineRuntimeV1::new("missing-repository");
        let missing_root = "invalid-root";

        let observe = serde_json::to_value(EnginePublicRequestV1::new(serde_json::json!({
            "kind": "observe",
            "root_id": missing_root,
        })))
        .unwrap();
        assert!(matches!(
            runtime.execute(observe),
            EngineOutcomeV1::Rejected { code } if code == "runtime_prerequisite_stale"
        ));

        let authorize = serde_json::to_value(EngineDomainRequestV1::authorize_actions(
            missing_root,
            vec!["not-a-paired-action".to_owned()],
        ))
        .unwrap();
        assert!(matches!(
            runtime.execute(authorize),
            EngineOutcomeV1::Rejected { code }
                if code == "preview_action_authorization_failed"
        ));

        let search = serde_json::to_value(EnginePublicRequestV1::new(serde_json::json!({
            "kind": "search",
            "root_id": missing_root,
        })))
        .unwrap();
        assert!(matches!(
            runtime.execute(search),
            EngineOutcomeV1::Rejected { code } if code == "runtime_prerequisite_stale"
        ));
    }

    #[test]
    fn native_recovery_preflight_is_closed_and_read_only() {
        let request = EngineDomainRequestV1::preflight_native_recovery(
            format!("state-root-v2-{}", "a".repeat(64)),
            format!("state-root-activation-v2-{}", "b".repeat(64)),
        );
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(
            serde_json::from_value::<EngineDomainRequestV1>(encoded.clone()).unwrap(),
            request
        );
        let mut runtime = EngineRuntimeV1::new("missing-repository");
        assert!(matches!(
            runtime.execute(encoded),
            EngineOutcomeV1::Rejected { code }
                if code == "native_recovery_candidate_ineligible"
        ));

        let error = serde_json::from_value::<EngineDomainRequestV1>(serde_json::json!({
            "schema": ENGINE_DOMAIN_REQUEST_SCHEMA_V1,
            "operation": {
                "kind": "preflight_native_recovery",
                "target_root_id": format!("state-root-v2-{}", "a".repeat(64)),
                "captured_activation_id": format!("state-root-activation-v2-{}", "b".repeat(64)),
                "path": "smuggled",
            },
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[cfg(windows)]
    #[test]
    fn native_recovery_candidate_publication_does_not_grant_a_certificate() {
        let fixture = NativeRecoveryFixture::new();
        let recovery_ref =
            fixture.seal_capsule(NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1, "candidate-capsule");
        let mut runtime = EngineRuntimeV1::new(&fixture.repository);
        runtime.bind_epoch(7).unwrap();
        let request =
            serde_json::to_value(EngineDomainRequestV1::retain_native_recovery_candidate(
                &fixture.root.root_id,
                &fixture.captured.activation_id,
                recovery_ref,
            ))
            .unwrap();

        let EngineOutcomeV1::Completed { output } = runtime.execute(request) else {
            panic!("retained native candidate must publish")
        };
        assert_eq!(
            serde_json::from_value::<EngineNativeRecoveryCandidatePublicationV1>(output).unwrap(),
            EngineNativeRecoveryCandidatePublicationV1::retained()
        );
        let store = paired_store(&fixture.repository);
        assert_eq!(
            store
                .native_recovery_candidate_ids(&fixture.root.root_id)
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .recovery_plan(&fixture.root.root_id)
            .unwrap_err()
            .contains("no fresh-JVM recovery certificate"));
    }

    #[test]
    fn native_recovery_validation_rebinds_one_fresh_activation() {
        let fixture = NativeRecoveryFixture::new();
        let scope = NativeRecoveryObservationScopeV1::new(7, "recovery-job").unwrap();
        let observation_ref =
            fixture.publish(&scope, map_frame("java-restored", "java-card-restored", 99));

        let validated = validate_native_recovery_observation_v1(
            &fixture.repository,
            &fixture.root.root_id,
            &fixture.captured.activation_id,
            &observation_ref,
            &scope,
        )
        .unwrap();

        assert_eq!(validated.target_root_id, fixture.root.root_id);
        assert_eq!(
            validated.captured_activation_id,
            fixture.captured.activation_id
        );
        assert_eq!(validated.restored_components.len(), 10);
        let restored = paired_store(&fixture.repository)
            .load_activation(&validated.restored_activation_id)
            .unwrap();
        assert_eq!(
            restored.activation.java_runtime_epoch,
            "java-restored:connection-1:capture-1"
        );
        assert_eq!(
            restored.activation.card_leases[0].java_card_uuid,
            "java-card-restored"
        );
    }

    #[test]
    fn native_recovery_validation_accepts_the_post_neow_autosave_root() {
        let fixture = NativeRecoveryFixture::new_neow();
        let scope = NativeRecoveryObservationScopeV1::new(7, "recovery-job").unwrap();
        let observation_ref = fixture.publish(
            &scope,
            completed_neow_frame("java-restored", "java-card-restored", 199),
        );

        let validated = validate_native_recovery_observation_v1(
            &fixture.repository,
            &fixture.root.root_id,
            &fixture.captured.activation_id,
            &observation_ref,
            &scope,
        )
        .unwrap();

        assert_eq!(validated.target_root_id, fixture.root.root_id);
        assert_eq!(validated.restored_components.len(), 11);
        let restored = paired_store(&fixture.repository)
            .load_activation(&validated.restored_activation_id)
            .unwrap();
        assert_eq!(
            restored.activation.java_runtime_epoch,
            "java-restored:connection-1:capture-1"
        );
        assert_eq!(
            restored.activation.card_leases[0].java_card_uuid,
            "java-card-restored"
        );
    }

    #[test]
    fn native_recovery_validation_rejects_projection_divergence() {
        let fixture = NativeRecoveryFixture::new();
        let scope = NativeRecoveryObservationScopeV1::new(7, "recovery-job").unwrap();
        let observation_ref = fixture.publish(
            &scope,
            map_frame("java-restored", "java-card-restored", 100),
        );

        let error = validate_native_recovery_observation_v1(
            &fixture.repository,
            &fixture.root.root_id,
            &fixture.captured.activation_id,
            &observation_ref,
            &scope,
        )
        .unwrap_err();

        assert_eq!(error.code(), "native_recovery_run_scalars_diverged");
        assert!(error.to_string().contains("RunScalars"));
    }

    #[test]
    fn native_recovery_validation_reports_rng_divergence() {
        let fixture = NativeRecoveryFixture::new();
        let scope = NativeRecoveryObservationScopeV1::new(7, "recovery-job").unwrap();
        let mut frame = map_frame("java-restored", "java-card-restored", 99);
        frame["rng_internal"]["streams"][0]["state_hex"] = serde_json::json!(["00", "02"]);
        let observation_ref = fixture.publish(&scope, frame);

        let error = validate_native_recovery_observation_v1(
            &fixture.repository,
            &fixture.root.root_id,
            &fixture.captured.activation_id,
            &observation_ref,
            &scope,
        )
        .unwrap_err();

        assert_eq!(error.code(), "native_recovery_rng_streams_diverged");
        assert!(error.to_string().contains("RngStreams"));
    }

    #[test]
    fn native_recovery_validation_rejects_another_job_scope() {
        let fixture = NativeRecoveryFixture::new();
        let scope = NativeRecoveryObservationScopeV1::new(7, "recovery-job").unwrap();
        let observation_ref =
            fixture.publish(&scope, map_frame("java-restored", "java-card-restored", 99));
        let other_scope = NativeRecoveryObservationScopeV1::new(7, "other-job").unwrap();

        let error = validate_native_recovery_observation_v1(
            &fixture.repository,
            &fixture.root.root_id,
            &fixture.captured.activation_id,
            &observation_ref,
            &other_scope,
        )
        .unwrap_err();

        assert!(error.to_string().contains("identity is invalid"));
    }

    #[test]
    fn native_recovery_validation_requires_a_fresh_java_epoch() {
        let fixture = NativeRecoveryFixture::new();
        let scope = NativeRecoveryObservationScopeV1::new(7, "recovery-job").unwrap();
        let observation_ref =
            fixture.publish(&scope, map_frame("java-captured", "java-card-restored", 99));

        let error = validate_native_recovery_observation_v1(
            &fixture.repository,
            &fixture.root.root_id,
            &fixture.captured.activation_id,
            &observation_ref,
            &scope,
        )
        .unwrap_err();

        assert!(error.to_string().contains("fresh Java runtime"));
    }

    #[test]
    fn native_recovery_job_does_not_seal_a_diverged_observation() {
        let fixture = NativeRecoveryFixture::new();
        let scope = NativeRecoveryObservationScopeV1::new(7, "native-recovery-7-1").unwrap();
        let observation_ref = fixture.publish(
            &scope,
            map_frame("java-restored", "java-card-restored", 100),
        );
        let mut runtime = EngineRuntimeV1::new(&fixture.repository);
        runtime.bind_epoch(7).unwrap();
        let request =
            serde_json::to_value(EngineDomainRequestV1::create_native_recovery_certificate(
                "native-recovery-7-1",
                &fixture.root.root_id,
                &fixture.captured.activation_id,
            ))
            .unwrap();
        let EngineOutcomeV1::SupervisorCall { call } = runtime.execute(request.clone()) else {
            panic!("native recovery must begin with a physical callback")
        };
        assert_eq!(call.call_id, "native-recovery-7-1-begin");
        assert_eq!(
            serde_json::from_value::<PhysicalRuntimeCallV1>(call.input)
                .unwrap()
                .operation,
            sts_workbench_supervisor::PhysicalRuntimeOperationV1::BeginNativeRecoveryValidation {}
        );

        assert!(matches!(
            runtime.resume_supervisor_call(
                &call.call_id,
                SupervisorCallOutcomeV1::Completed {
                    output: serde_json::to_value(
                        PhysicalRuntimeResultV1::native_recovery_validator_ready(observation_ref),
                    )
                    .unwrap(),
                },
            ),
            EngineOutcomeV1::Rejected { code }
                if code == "native_recovery_run_scalars_diverged"
        ));
        assert!(matches!(
            runtime.execute(request),
            EngineOutcomeV1::SupervisorCall { .. }
        ));
    }

    #[cfg(windows)]
    #[test]
    fn native_recovery_job_publishes_one_storage_anchor_after_seal() {
        let fixture = NativeRecoveryFixture::new();
        let job_id = "native-recovery-7-1";
        let scope = NativeRecoveryObservationScopeV1::new(7, job_id).unwrap();
        let observation_ref =
            fixture.publish(&scope, map_frame("java-restored", "java-card-restored", 99));
        let recovery_ref = fixture.seal_capsule(
            sts_paired_root::NATIVE_STORAGE_RECOVERY_DIRECTORIES_V1,
            job_id,
        );
        let mut runtime = EngineRuntimeV1::new(&fixture.repository);
        runtime.bind_epoch(7).unwrap();
        let request =
            serde_json::to_value(EngineDomainRequestV1::create_native_recovery_certificate(
                job_id,
                &fixture.root.root_id,
                &fixture.captured.activation_id,
            ))
            .unwrap();

        let EngineOutcomeV1::SupervisorCall { call: begin } = runtime.execute(request) else {
            panic!("native recovery must request begin")
        };
        let EngineOutcomeV1::SupervisorCall { call: seal } = runtime.resume_supervisor_call(
            &begin.call_id,
            SupervisorCallOutcomeV1::Completed {
                output: serde_json::to_value(
                    PhysicalRuntimeResultV1::native_recovery_validator_ready(observation_ref),
                )
                .unwrap(),
            },
        ) else {
            panic!("Exact observation must request seal")
        };
        assert_eq!(seal.call_id, "native-recovery-7-1-seal");
        assert_eq!(
            serde_json::from_value::<PhysicalRuntimeCallV1>(seal.input)
                .unwrap()
                .operation,
            sts_workbench_supervisor::PhysicalRuntimeOperationV1::SealNativeRecoveryValidation {}
        );
        let EngineOutcomeV1::Completed { output } = runtime.resume_supervisor_call(
            &seal.call_id,
            SupervisorCallOutcomeV1::Completed {
                output: serde_json::to_value(PhysicalRuntimeResultV1::native_recovery_sealed(
                    recovery_ref,
                ))
                .unwrap(),
            },
        ) else {
            panic!("sealed exact recovery must publish a certificate")
        };
        assert_eq!(
            serde_json::from_value::<EngineNativeRecoveryPublicationV1>(output).unwrap(),
            EngineNativeRecoveryPublicationV1::published()
        );

        let store = paired_store(&fixture.repository);
        let plan = store.recovery_plan(&fixture.root.root_id).unwrap();
        let certificate = store
            .load_recovery_certificate(&fixture.root.root_id, &plan.recovery_certificate_id)
            .unwrap();
        assert_eq!(
            certificate.certificate.java_recovery.entrypoint,
            sts_paired_root::NATIVE_STORAGE_CONTINUE_ENTRYPOINT_V1
        );
        assert_eq!(
            certificate.certificate.java_recovery.source_identity.len(),
            2
        );
        assert!(certificate
            .certificate
            .java_recovery
            .source_identity
            .iter()
            .all(|claim| claim.kind != "checkpoint_manifest_sha256"));
        let restored = store
            .load_activation(&certificate.certificate.restored_activation_id)
            .unwrap();
        assert_eq!(restored.activation.root_id, fixture.root.root_id);
        assert_eq!(
            restored.activation.java_runtime_epoch,
            "java-restored:connection-1:capture-1"
        );
    }
}
