use crate::{
    commit, diagnostic_lease, domain_manifest::RenderedManifestV1, lease_source, manifest_lease,
    preview_lease, search::SearchExecutionV1, search_lease, witness_continuation_lease,
    witness_preview_manifest,
};
use serde_json::Value;
use std::path::Path;
use sts_workbench_engine_protocol::{
    EngineActionPreviewCompletionV1, EngineActionPreviewResolutionV1, EngineDomainOperationV1,
    EngineResolvedActionV1, EngineViewManifestV1, EngineWitnessPreviewCompletionV1,
    EngineWitnessPreviewResolutionV1, ENGINE_ACTION_PREVIEW_COMPLETION_SCHEMA_V1,
    ENGINE_ACTION_PREVIEW_RESOLUTION_SCHEMA_V1, ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1,
    ENGINE_WITNESS_PREVIEW_RESOLUTION_SCHEMA_V1,
};

enum PendingViewExecutionV1 {
    Action(manifest_lease::ResolvedActionAliasesV1),
    Witness(search_lease::ResolvedWitnessAliasV1),
}

#[derive(Default)]
pub(super) struct ViewStateV1 {
    action_lease: Option<manifest_lease::ActionLeaseV1>,
    preview_lease: Option<preview_lease::PreviewLeaseV1>,
    witness_continuation_lease: Option<witness_continuation_lease::WitnessContinuationLeaseV1>,
    witness_lease: Option<search_lease::WitnessLeaseV1>,
    diagnostic_lease: Option<diagnostic_lease::DiagnosticLeaseV1>,
    pending: Option<PendingViewExecutionV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ViewStateErrorV1 {
    pub code: &'static str,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ObservationV1 {
    pub root_id: String,
    pub action_space_id: String,
    pub detail_handle: String,
    pub manifest: Option<RenderedManifestV1>,
}

impl ViewStateV1 {
    pub fn has_pending_execution(&self) -> bool {
        self.pending.is_some()
    }

    pub fn permits_while_pending(&self, operation: &EngineDomainOperationV1) -> bool {
        matches!(
            (&self.pending, operation),
            (
                Some(PendingViewExecutionV1::Action(_)),
                EngineDomainOperationV1::AcceptActionPreview { .. }
                    | EngineDomainOperationV1::AbortActionPreview {}
            ) | (
                Some(PendingViewExecutionV1::Witness(_)),
                EngineDomainOperationV1::AcceptWitnessPreview { .. }
                    | EngineDomainOperationV1::AbortWitnessPreview {}
            ) | (None, _)
        )
    }

    pub fn clear(&mut self) {
        self.action_lease = None;
        self.preview_lease = None;
        self.witness_continuation_lease = None;
        self.witness_lease = None;
        self.diagnostic_lease = None;
        self.pending = None;
    }

    pub fn accept_explicit_observation(
        &mut self,
        observed: ObservationV1,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        self.clear();
        self.accept_observation(observed, lease_source::LeaseSourceV1::ExplicitRoot)
    }

    pub fn accept_selected_observation(
        &mut self,
        observed: ObservationV1,
        selected: sts_paired_root::PairedHeadSelectionResolutionV1,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        self.clear();
        self.accept_observation(
            observed,
            lease_source::LeaseSourceV1::SelectedHead(Box::new(selected)),
        )
    }

    pub fn accept_preview_observation(
        &mut self,
        observed: ObservationV1,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        self.accept_observation(observed, lease_source::LeaseSourceV1::ExplicitRoot)
    }

    pub fn preview_successor(
        &self,
        repository: &Path,
        alias: &str,
    ) -> Result<String, ViewStateErrorV1> {
        if let Some(lease) = &self.preview_lease {
            return lease
                .resolve_successor(repository, alias)
                .map(|resolved| resolved.successor_root_id)
                .map_err(lease_error);
        }
        let lease = self
            .witness_continuation_lease
            .as_ref()
            .ok_or_else(preview_lease::preview_required)
            .map_err(lease_error)?;
        lease.resolve(repository, alias).map_err(lease_error)
    }

    pub fn inspect_alias(
        &self,
        repository: &Path,
        alias: &str,
    ) -> Result<String, ViewStateErrorV1> {
        self.diagnostic_lease
            .as_ref()
            .ok_or_else(diagnostic_lease::diagnostic_required)
            .and_then(|lease| lease.resolve(repository, alias).map(str::to_owned))
            .map_err(lease_error)
    }

    pub fn accept_explicit_search(
        &mut self,
        repository: &Path,
        searched: SearchExecutionV1,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        self.clear();
        self.accept_search(
            repository,
            searched,
            lease_source::LeaseSourceV1::ExplicitRoot,
        )
    }

    pub fn accept_selected_search(
        &mut self,
        repository: &Path,
        searched: SearchExecutionV1,
        selected: sts_paired_root::PairedHeadSelectionResolutionV1,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        self.clear();
        self.accept_search(
            repository,
            searched,
            lease_source::LeaseSourceV1::SelectedHead(Box::new(selected)),
        )
    }

    pub fn current_search_scope(
        &self,
        repository: &Path,
    ) -> Result<lease_source::LeaseScopeV1, ViewStateErrorV1> {
        self.action_lease
            .as_ref()
            .ok_or_else(manifest_lease::observe_required)
            .and_then(|lease| lease.resolve_scope(repository))
            .map_err(lease_error)
    }

    pub fn accept_current_search(
        &mut self,
        repository: &Path,
        searched: SearchExecutionV1,
        source: lease_source::LeaseSourceV1,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        self.accept_search(repository, searched, source)
    }

    pub fn resolve_action_aliases(
        &mut self,
        repository: &Path,
        aliases: &[String],
    ) -> Result<EngineActionPreviewResolutionV1, ViewStateErrorV1> {
        self.preview_lease = None;
        self.witness_continuation_lease = None;
        self.witness_lease = None;
        self.diagnostic_lease = None;
        self.pending = None;
        let resolved = self
            .action_lease
            .as_ref()
            .ok_or_else(manifest_lease::observe_required)
            .and_then(|lease| lease.resolve(repository, aliases))
            .map_err(lease_error)?;
        let response = EngineActionPreviewResolutionV1 {
            schema: ENGINE_ACTION_PREVIEW_RESOLUTION_SCHEMA_V1.to_owned(),
            root_id: resolved.scope.root_id.clone(),
            action_space_id: resolved.scope.action_space_id.clone(),
            actions: resolved
                .actions
                .iter()
                .map(|action| EngineResolvedActionV1 {
                    alias: action.alias.clone(),
                    action_id: action.action_id.clone(),
                })
                .collect(),
        };
        self.pending = Some(PendingViewExecutionV1::Action(resolved));
        Ok(response)
    }

    pub fn accept_action_preview(
        &mut self,
        repository: &Path,
        completion: EngineActionPreviewCompletionV1,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        let resolved = match self.pending.take() {
            Some(PendingViewExecutionV1::Action(resolved)) => resolved,
            Some(other) => {
                self.pending = Some(other);
                return Err(view_error(
                    "PreviewResolutionMismatch",
                    "current Engine resolution belongs to witness preview",
                ));
            }
            None => {
                return Err(view_error(
                    "PreviewResolutionRequired",
                    "action preview completion has no current Engine resolution",
                ));
            }
        };
        if completion.schema != ENGINE_ACTION_PREVIEW_COMPLETION_SCHEMA_V1
            || completion.root_id != resolved.scope.root_id
            || completion.action_space_id != resolved.scope.action_space_id
        {
            return Err(view_error(
                "PreviewInvalid",
                "action preview completion changed its resolved scope",
            ));
        }
        let lease = preview_lease::PreviewLeaseV1::new(
            repository,
            resolved,
            &completion.summaries,
            completion.detail_handle,
        )
        .map_err(lease_error)?;
        let manifest = lease.text().to_owned();
        let diagnostic = diagnostic_lease::DiagnosticLeaseV1::new(
            lease.scope().clone(),
            lease.diagnostic_entries(),
        )
        .map_err(lease_error)?;
        self.preview_lease = Some(lease);
        self.witness_continuation_lease = None;
        self.witness_lease = None;
        self.diagnostic_lease = Some(diagnostic);
        Ok(EngineViewManifestV1::new(manifest))
    }

    pub fn abort_action_preview(&mut self) -> Result<(), ViewStateErrorV1> {
        match self.pending.take() {
            Some(PendingViewExecutionV1::Action(_)) => Ok(()),
            Some(other) => {
                self.pending = Some(other);
                Err(view_error(
                    "PreviewResolutionMismatch",
                    "current Engine resolution belongs to witness preview",
                ))
            }
            None => Err(view_error(
                "PreviewResolutionRequired",
                "no action preview resolution is pending",
            )),
        }
    }

    pub fn resolve_witness_alias(
        &mut self,
        repository: &Path,
        alias: &str,
    ) -> Result<EngineWitnessPreviewResolutionV1, ViewStateErrorV1> {
        let resolved = self
            .witness_lease
            .as_ref()
            .ok_or_else(search_lease::search_required)
            .and_then(|lease| lease.resolve(repository, alias))
            .map_err(lease_error)?;
        self.action_lease = None;
        self.preview_lease = None;
        self.witness_continuation_lease = None;
        self.diagnostic_lease = None;
        self.pending = Some(PendingViewExecutionV1::Witness(resolved.clone()));
        Ok(EngineWitnessPreviewResolutionV1 {
            schema: ENGINE_WITNESS_PREVIEW_RESOLUTION_SCHEMA_V1.to_owned(),
            root_id: resolved.scope.root_id,
            action_space_id: resolved.scope.action_space_id,
            witness_handle: resolved.handle,
            plan_step_count: resolved.plan_step_count,
        })
    }

    pub fn accept_witness_preview(
        &mut self,
        repository: &Path,
        completion: EngineWitnessPreviewCompletionV1,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        let resolved = match self.pending.take() {
            Some(PendingViewExecutionV1::Witness(resolved)) => resolved,
            Some(other) => {
                self.pending = Some(other);
                return Err(view_error(
                    "WitnessResolutionMismatch",
                    "current Engine resolution belongs to action preview",
                ));
            }
            None => {
                return Err(view_error(
                    "WitnessResolutionRequired",
                    "witness preview completion has no current Engine resolution",
                ));
            }
        };
        if completion.schema != ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1
            || completion.root_id != resolved.scope.root_id
            || completion.action_space_id != resolved.scope.action_space_id
            || completion.witness_handle != resolved.handle
        {
            return Err(view_error(
                "WitnessPreviewInvalid",
                "witness preview completion changed its resolved scope",
            ));
        }
        let continuation = (completion.status
            == sts_workbench_engine_protocol::EngineWitnessPreviewStatusV1::Ready)
            .then(|| {
                witness_continuation_lease::WitnessContinuationLeaseV1::new(
                    repository,
                    resolved.scope.clone(),
                    completion.reached_root_id.clone(),
                    u64::try_from(completion.steps.len()).unwrap_or(u64::MAX),
                )
                .map_err(lease_error)
            })
            .transpose()?;
        let rendered =
            witness_preview_manifest::render_witness_preview_v1(repository, &resolved, &completion)
                .map_err(lease_error)?;
        let diagnostic = diagnostic_lease::DiagnosticLeaseV1::new(
            resolved.scope,
            [("D0".to_owned(), rendered.detail_handle)],
        )
        .map_err(lease_error)?;
        self.action_lease = None;
        self.preview_lease = None;
        self.witness_continuation_lease = continuation;
        self.diagnostic_lease = Some(diagnostic);
        Ok(EngineViewManifestV1::new(rendered.text))
    }

    pub fn abort_witness_preview(&mut self) -> Result<(), ViewStateErrorV1> {
        match self.pending.take() {
            Some(PendingViewExecutionV1::Witness(_)) => Ok(()),
            Some(other) => {
                self.pending = Some(other);
                Err(view_error(
                    "WitnessResolutionMismatch",
                    "current Engine resolution belongs to action preview",
                ))
            }
            None => Err(view_error(
                "WitnessResolutionRequired",
                "no witness preview resolution is pending",
            )),
        }
    }

    pub fn commit_preview(
        &mut self,
        repository: &Path,
        alias: &str,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        let lease = match self.preview_lease.as_ref() {
            Some(lease) => lease,
            None if self.witness_continuation_lease.is_some() => {
                return Err(lease_error(witness_continuation_lease::commit_unavailable()));
            }
            None => return Err(lease_error(preview_lease::preview_required())),
        };
        let resolved = lease
            .resolve_commit(repository, alias)
            .map_err(lease_error)?;
        let committed = commit::commit_selected_receipt(
            repository,
            &resolved.selector_name,
            &resolved.receipt_id,
        )
        .map_err(|detail| view_error("CommitPreviewFailed", detail))?;
        if committed.committed_root_id != resolved.successor_root_id {
            return Err(view_error(
                "CommitPreviewFailed",
                "committed root did not match the P lease successor",
            ));
        }
        self.clear();
        Ok(EngineViewManifestV1::new(format!(
            "STS-COMMIT 1\n{alias} committed\nnext=observe\n"
        )))
    }

    pub fn commit_selected(
        &mut self,
        repository: &Path,
        selector_name: &str,
        receipt_id: &str,
    ) -> Result<Value, ViewStateErrorV1> {
        let committed = commit::commit_selected_receipt(repository, selector_name, receipt_id)
            .map_err(|detail| view_error("CommitSelectedReceiptFailed", detail))?;
        self.clear();
        Ok(committed.response)
    }

    fn accept_observation(
        &mut self,
        observed: ObservationV1,
        source: lease_source::LeaseSourceV1,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        let ObservationV1 {
            root_id,
            action_space_id,
            detail_handle,
            manifest,
        } = observed;
        let scope = lease_source::LeaseScopeV1::new(
            root_id.clone(),
            action_space_id.clone(),
            source.clone(),
        );
        let rendered = manifest.ok_or_else(|| {
            view_error(
                "CurrentManifestUnavailable",
                "this historical root has no current Parity Lab manifest surface",
            )
        })?;
        let action_lease =
            manifest_lease::ActionLeaseV1::new(root_id, action_space_id, source, &rendered)
                .map_err(|detail| view_error("ObservationLeaseInvalid", detail))?;
        let diagnostic_lease = (!rendered.diagnostics.is_empty())
            .then(|| {
                diagnostic_lease::DiagnosticLeaseV1::new(
                    scope,
                    rendered
                        .diagnostics
                        .iter()
                        .map(|entry| (entry.alias.clone(), detail_handle.clone())),
                )
                .map_err(lease_error)
            })
            .transpose()?;
        let manifest = EngineViewManifestV1::new(rendered.text);
        self.action_lease = Some(action_lease);
        self.preview_lease = None;
        self.witness_continuation_lease = None;
        self.witness_lease = None;
        self.diagnostic_lease = diagnostic_lease;
        self.pending = None;
        Ok(manifest)
    }

    fn accept_search(
        &mut self,
        repository: &Path,
        searched: SearchExecutionV1,
        source: lease_source::LeaseSourceV1,
    ) -> Result<EngineViewManifestV1, ViewStateErrorV1> {
        let lease = search_lease::WitnessLeaseV1::new(repository, source, &searched)
            .map_err(lease_error)?;
        let manifest = lease.text().to_owned();
        let diagnostic = diagnostic_lease::DiagnosticLeaseV1::new(
            lease.scope().clone(),
            [("D0".to_owned(), lease.detail_handle().to_owned())],
        )
        .map_err(lease_error)?;
        self.action_lease = None;
        self.preview_lease = None;
        self.witness_continuation_lease = None;
        self.witness_lease = Some(lease);
        self.diagnostic_lease = Some(diagnostic);
        self.pending = None;
        Ok(EngineViewManifestV1::new(manifest))
    }
}

fn lease_error<Error>(error: Error) -> ViewStateErrorV1
where
    Error: IntoViewStateErrorV1,
{
    error.into_view_state_error()
}

trait IntoViewStateErrorV1 {
    fn into_view_state_error(self) -> ViewStateErrorV1;
}

macro_rules! lease_error_conversion {
    ($error:ty) => {
        impl IntoViewStateErrorV1 for $error {
            fn into_view_state_error(self) -> ViewStateErrorV1 {
                ViewStateErrorV1 {
                    code: self.code,
                    detail: self.detail,
                }
            }
        }
    };
}

lease_error_conversion!(manifest_lease::ActionLeaseErrorV1);
lease_error_conversion!(preview_lease::PreviewLeaseErrorV1);
lease_error_conversion!(witness_continuation_lease::WitnessContinuationLeaseErrorV1);
lease_error_conversion!(search_lease::WitnessLeaseErrorV1);
lease_error_conversion!(diagnostic_lease::DiagnosticLeaseErrorV1);
lease_error_conversion!(witness_preview_manifest::WitnessPreviewRenderErrorV1);

fn view_error(code: &'static str, detail: impl Into<String>) -> ViewStateErrorV1 {
    ViewStateErrorV1 {
        code,
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_manifest::ActionLeaseEntryV1;
    use crate::search::{SearchStatusV1, SearchTranslationStatusV1, SearchWitnessSummaryV1};
    use crate::test_support::{paired_store_fixture, PairedStoreFixture};
    use sts_workbench_engine_protocol::{
        EngineDomainRequestV1, EnginePreviewMemberStatusV1, EnginePreviewMemberSummaryV1,
        EngineWitnessPreviewStatusV1, EngineWitnessPreviewStepStatusV1, EngineWitnessPreviewStepV1,
    };

    fn artifact(byte: char) -> String {
        format!(
            "workbench-artifact-v1-{}",
            std::iter::repeat_n(byte, 64).collect::<String>()
        )
    }

    fn observed(fixture: &PairedStoreFixture) -> ObservationV1 {
        ObservationV1 {
            root_id: fixture.source.root_id.clone(),
            action_space_id: fixture.source.root.preflight.action_space_id.clone(),
            detail_handle: artifact('d'),
            manifest: Some(RenderedManifestV1 {
                text: "STS-MANIFEST 1\n\n[actions]\nA0 first\nA1 second\n".to_owned(),
                actions: vec![
                    ActionLeaseEntryV1 {
                        alias: "A0".to_owned(),
                        action_id: Some(fixture.first.receipt.action.handle_id.clone()),
                        available: true,
                    },
                    ActionLeaseEntryV1 {
                        alias: "A1".to_owned(),
                        action_id: Some(fixture.second.receipt.action.handle_id.clone()),
                        available: true,
                    },
                ],
                diagnostics: vec![],
            }),
        }
    }

    fn exact_summary(
        receipt: &sts_paired_root::TransitionReceiptArtifactV1,
    ) -> EnginePreviewMemberSummaryV1 {
        EnginePreviewMemberSummaryV1 {
            action_id: receipt.receipt.action.handle_id.clone(),
            status: EnginePreviewMemberStatusV1::Exact {
                receipt_id: receipt.receipt_id.clone(),
                successor_root_id: receipt.receipt.successor_root_id.clone(),
            },
            exact_count: 9,
            total_count: 9,
        }
    }

    fn action_completion(
        fixture: &PairedStoreFixture,
        summaries: Vec<EnginePreviewMemberSummaryV1>,
    ) -> EngineActionPreviewCompletionV1 {
        EngineActionPreviewCompletionV1 {
            schema: ENGINE_ACTION_PREVIEW_COMPLETION_SCHEMA_V1.to_owned(),
            root_id: fixture.source.root_id.clone(),
            action_space_id: fixture.source.root.preflight.action_space_id.clone(),
            summaries,
            detail_handle: artifact('p'),
        }
    }

    fn searched(fixture: &PairedStoreFixture) -> SearchExecutionV1 {
        SearchExecutionV1 {
            root_id: fixture.source.root_id.clone(),
            action_space_id: fixture.source.root.preflight.action_space_id.clone(),
            status: SearchStatusV1::Witness,
            summary_exit: "satisfied".to_owned(),
            lane_count: 1,
            bounded_frontier_count: 0,
            witnesses: vec![SearchWitnessSummaryV1 {
                witness_handle: artifact('a'),
                plan_step_count: 1,
                translation: SearchTranslationStatusV1::Complete,
                final_hp: 80,
                total_potions_consumed: 0,
                satisfies_requested_quality: true,
            }],
            detail_handle: artifact('b'),
        }
    }

    #[test]
    fn empty_or_cleared_view_has_no_alias_authority() {
        let fixture = paired_store_fixture();
        let mut view = ViewStateV1::default();
        assert_eq!(
            view.current_search_scope(&fixture.repository)
                .unwrap_err()
                .code,
            "ObserveRequired"
        );
        assert_eq!(
            view.preview_successor(&fixture.repository, "P0")
                .unwrap_err()
                .code,
            "PreviewRequired"
        );
        assert_eq!(
            view.resolve_witness_alias(&fixture.repository, "W0")
                .unwrap_err()
                .code,
            "SearchRequired"
        );
        assert_eq!(
            view.inspect_alias(&fixture.repository, "D0")
                .unwrap_err()
                .code,
            "InspectRequired"
        );

        view.accept_explicit_observation(observed(&fixture))
            .unwrap();
        view.clear();
        assert_eq!(
            view.resolve_action_aliases(&fixture.repository, &["A0".to_owned()])
                .unwrap_err()
                .code,
            "ObserveRequired"
        );
        assert!(!ViewStateV1::default().has_pending_execution());
    }

    #[test]
    fn pending_execution_accepts_only_its_matching_completion_or_abort() {
        let fixture = paired_store_fixture();
        let mut view = ViewStateV1::default();
        view.accept_explicit_observation(observed(&fixture))
            .unwrap();
        view.resolve_action_aliases(&fixture.repository, &["A0".to_owned()])
            .unwrap();
        assert!(view.permits_while_pending(
            &EngineDomainRequestV1::accept_action_preview(action_completion(
                &fixture,
                vec![exact_summary(&fixture.first)],
            ))
            .operation
        ));
        assert!(
            !view.permits_while_pending(&EngineDomainRequestV1::abort_witness_preview().operation)
        );
        assert!(!view.permits_while_pending(&EngineDomainRequestV1::clear_view().operation));

        let wrong_completion = EngineWitnessPreviewCompletionV1 {
            schema: ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1.to_owned(),
            root_id: fixture.source.root_id.clone(),
            action_space_id: fixture.source.root.preflight.action_space_id.clone(),
            witness_handle: artifact('w'),
            reached_root_id: fixture.source.root_id.clone(),
            status: EngineWitnessPreviewStatusV1::Ready,
            steps: vec![],
            stopped_step_ordinal: None,
            stop_reason: None,
            detail_handle: artifact('d'),
        };
        assert_eq!(
            view.accept_witness_preview(&fixture.repository, wrong_completion)
                .unwrap_err()
                .code,
            "WitnessResolutionMismatch"
        );
        assert!(view.has_pending_execution());
        view.abort_action_preview().unwrap();
        assert!(!view.has_pending_execution());
    }

    #[test]
    fn action_completion_is_ordered_and_failure_signs_no_preview_or_diagnostic() {
        let fixture = paired_store_fixture();
        let mut view = ViewStateV1::default();
        view.accept_explicit_observation(observed(&fixture))
            .unwrap();
        view.resolve_action_aliases(&fixture.repository, &["A0".to_owned(), "A1".to_owned()])
            .unwrap();
        let error = view
            .accept_action_preview(
                &fixture.repository,
                action_completion(
                    &fixture,
                    vec![
                        exact_summary(&fixture.second),
                        exact_summary(&fixture.first),
                    ],
                ),
            )
            .unwrap_err();
        assert_eq!(error.code, "PreviewInvalid");
        assert_eq!(
            view.preview_successor(&fixture.repository, "P0")
                .unwrap_err()
                .code,
            "PreviewRequired"
        );
        assert_eq!(
            view.inspect_alias(&fixture.repository, "D0")
                .unwrap_err()
                .code,
            "InspectRequired"
        );
    }

    #[test]
    fn exact_preview_preserves_observe_only_and_selected_commit_boundaries() {
        let fixture = paired_store_fixture();
        let mut explicit = ViewStateV1::default();
        explicit
            .accept_explicit_observation(observed(&fixture))
            .unwrap();
        explicit
            .resolve_action_aliases(&fixture.repository, &["A0".to_owned()])
            .unwrap();
        explicit
            .accept_action_preview(
                &fixture.repository,
                action_completion(&fixture, vec![exact_summary(&fixture.first)]),
            )
            .unwrap();
        assert_eq!(
            explicit
                .preview_successor(&fixture.repository, "P0")
                .unwrap(),
            fixture.first.receipt.successor_root_id
        );
        assert_eq!(
            explicit
                .commit_preview(&fixture.repository, "P0")
                .unwrap_err()
                .code,
            "PreviewUnavailable"
        );

        let mut selected = ViewStateV1::default();
        selected
            .accept_selected_observation(
                observed(&fixture),
                fixture.store.resolve_head_selector("official").unwrap(),
            )
            .unwrap();
        selected
            .resolve_action_aliases(&fixture.repository, &["A0".to_owned()])
            .unwrap();
        selected
            .accept_action_preview(
                &fixture.repository,
                action_completion(&fixture, vec![exact_summary(&fixture.first)]),
            )
            .unwrap();
        let committed = selected.commit_preview(&fixture.repository, "P0").unwrap();
        assert!(committed.manifest.contains("P0 committed"));
        assert_eq!(
            fixture
                .store
                .resolve_head_selector("official")
                .unwrap()
                .head
                .root_id,
            fixture.first.receipt.successor_root_id
        );
        assert_eq!(
            selected
                .preview_successor(&fixture.repository, "P0")
                .unwrap_err()
                .code,
            "PreviewRequired"
        );
    }

    #[test]
    fn divergence_signs_only_diagnostic_authority() {
        let fixture = paired_store_fixture();
        let mut view = ViewStateV1::default();
        view.accept_explicit_observation(observed(&fixture))
            .unwrap();
        view.resolve_action_aliases(&fixture.repository, &["A0".to_owned()])
            .unwrap();
        let action_id = fixture.first.receipt.action.handle_id.clone();
        let completion = action_completion(
            &fixture,
            vec![EnginePreviewMemberSummaryV1 {
                action_id,
                status: EnginePreviewMemberStatusV1::Diverged {
                    divergence_id: "paired-divergence-v1-fixture".to_owned(),
                },
                exact_count: 8,
                total_count: 9,
            }],
        );
        let manifest = view
            .accept_action_preview(&fixture.repository, completion)
            .unwrap();
        assert!(manifest.manifest.contains("status=diverged inspect=D0"));
        assert_eq!(
            view.preview_successor(&fixture.repository, "P0")
                .unwrap_err()
                .code,
            "PreviewUnavailable"
        );
        assert_eq!(
            view.inspect_alias(&fixture.repository, "D0").unwrap(),
            artifact('p')
        );
    }

    #[test]
    fn witness_completion_retains_witness_and_replaces_diagnostic_view() {
        let fixture = paired_store_fixture();
        let mut view = ViewStateV1::default();
        let search = searched(&fixture);
        view.accept_explicit_search(&fixture.repository, search.clone())
            .unwrap();
        let resolved = view
            .resolve_witness_alias(&fixture.repository, "W0")
            .unwrap();
        let completion = EngineWitnessPreviewCompletionV1 {
            schema: ENGINE_WITNESS_PREVIEW_COMPLETION_SCHEMA_V1.to_owned(),
            root_id: resolved.root_id,
            action_space_id: resolved.action_space_id,
            witness_handle: resolved.witness_handle,
            reached_root_id: fixture.first.receipt.successor_root_id.clone(),
            status: EngineWitnessPreviewStatusV1::Ready,
            steps: vec![EngineWitnessPreviewStepV1 {
                status: EngineWitnessPreviewStepStatusV1::Ready,
                exact_count: 9,
                total_count: 9,
            }],
            stopped_step_ordinal: None,
            stop_reason: None,
            detail_handle: artifact('c'),
        };
        let manifest = view
            .accept_witness_preview(&fixture.repository, completion)
            .unwrap();
        assert!(manifest
            .manifest
            .contains("W0 status=ready exact_steps=1/1"));
        assert!(manifest.manifest.contains("continue=P0"));
        assert_eq!(
            view.preview_successor(&fixture.repository, "P0").unwrap(),
            fixture.first.receipt.successor_root_id
        );
        assert_eq!(
            view.commit_preview(&fixture.repository, "P0")
                .unwrap_err()
                .code,
            "PreviewUnavailable"
        );
        assert_eq!(
            view.inspect_alias(&fixture.repository, "D0").unwrap(),
            artifact('c')
        );
        assert_eq!(
            view.resolve_witness_alias(&fixture.repository, "W0")
                .unwrap()
                .witness_handle,
            search.witnesses[0].witness_handle
        );
        view.abort_witness_preview().unwrap();
    }

    #[test]
    fn selected_head_drift_expires_the_whole_view_before_resolution() {
        let fixture = paired_store_fixture();
        let mut view = ViewStateV1::default();
        view.accept_selected_observation(
            observed(&fixture),
            fixture.store.resolve_head_selector("official").unwrap(),
        )
        .unwrap();
        fixture
            .store
            .commit_selected_receipt_idempotent("official", &fixture.first.receipt_id)
            .unwrap();
        assert_eq!(
            view.resolve_action_aliases(&fixture.repository, &["A0".to_owned()])
                .unwrap_err()
                .code,
            "ActionExpired"
        );
        assert!(!view.has_pending_execution());
    }
}
