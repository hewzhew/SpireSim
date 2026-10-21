use crate::lease_source::{LeaseScopeV1, LeaseSourceV1};
use crate::manifest_lease::ResolvedActionAliasesV1;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;
use sts_workbench_engine_protocol::{
    EnginePreviewMemberStatusV1 as PreviewMemberStatusV1,
    EnginePreviewMemberSummaryV1 as PreviewMemberSummaryV1,
};

const PREVIEW_MANIFEST_VERSION_V1: &str = "STS-PREVIEW 1";

#[derive(Clone, Debug, Eq, PartialEq)]
enum PreviewLeaseStatusV1 {
    Exact {
        receipt_id: String,
        successor_root_id: String,
    },
    Diverged,
    SuccessorBoundaryUnsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreviewLeaseEntryV1 {
    action_alias: String,
    action_id: String,
    status: PreviewLeaseStatusV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreviewLeaseV1 {
    scope: LeaseScopeV1,
    entries: BTreeMap<String, PreviewLeaseEntryV1>,
    diagnostics: BTreeMap<String, String>,
    text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ResolvedPreviewCommitV1 {
    pub selector_name: String,
    pub receipt_id: String,
    pub successor_root_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ResolvedPreviewSuccessorV1 {
    pub successor_root_id: String,
}

struct ResolvedExactPreviewV1 {
    receipt_id: String,
    successor_root_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreviewLeaseErrorV1 {
    pub code: &'static str,
    pub detail: String,
}

impl PreviewLeaseV1 {
    pub fn new(
        repository: &Path,
        resolved: ResolvedActionAliasesV1,
        summaries: &[PreviewMemberSummaryV1],
        detail_handle: String,
    ) -> Result<Self, PreviewLeaseErrorV1> {
        if resolved.actions.is_empty() || resolved.actions.len() != summaries.len() {
            return Err(preview_error(
                "PreviewInvalid",
                "preview result does not cover its resolved action aliases",
            ));
        }
        resolved
            .scope
            .validate(repository)
            .map_err(|error| preview_error("PreviewExpired", error.detail))?;
        let authority = match &resolved.scope.source {
            LeaseSourceV1::ExplicitRoot => "observe_only",
            LeaseSourceV1::SelectedHead(_) => "lab_head",
        };
        let mut text = format!(
            "{PREVIEW_MANIFEST_VERSION_V1}\nmode=independent_actions\nauthority={authority}\n\n[previews]\n"
        );
        let mut entries = BTreeMap::new();
        let mut diagnostics = BTreeMap::new();
        for (ordinal, (action, summary)) in
            resolved.actions.iter().zip(summaries.iter()).enumerate()
        {
            if action.action_id != summary.action_id {
                return Err(preview_error(
                    "PreviewInvalid",
                    "preview result action order changed after alias resolution",
                ));
            }
            let preview_alias = format!("P{ordinal}");
            let status = match &summary.status {
                PreviewMemberStatusV1::Exact {
                    receipt_id,
                    successor_root_id,
                } => {
                    if summary.total_count == 0 || summary.exact_count != summary.total_count {
                        return Err(preview_error(
                            "PreviewInvalid",
                            "Exact preview summary has incomplete parity evidence",
                        ));
                    }
                    let commit = match &resolved.scope.source {
                        LeaseSourceV1::ExplicitRoot => "observe_only",
                        LeaseSourceV1::SelectedHead(_) => "ready",
                    };
                    writeln!(
                        text,
                        "{preview_alias} action={} exact={}/{} continue=ready commit={commit}",
                        action.alias, summary.exact_count, summary.total_count
                    )
                    .unwrap();
                    PreviewLeaseStatusV1::Exact {
                        receipt_id: receipt_id.clone(),
                        successor_root_id: successor_root_id.clone(),
                    }
                }
                PreviewMemberStatusV1::Diverged { divergence_id } => {
                    if divergence_id.trim().is_empty() {
                        return Err(preview_error(
                            "PreviewInvalid",
                            "diverged preview omitted its durable identity",
                        ));
                    }
                    let diagnostic_alias = format!("D{}", diagnostics.len());
                    diagnostics.insert(diagnostic_alias.clone(), detail_handle.clone());
                    writeln!(
                        text,
                        "{preview_alias} action={} status=diverged inspect={diagnostic_alias} commit=unavailable",
                        action.alias
                    )
                    .unwrap();
                    PreviewLeaseStatusV1::Diverged
                }
                PreviewMemberStatusV1::SuccessorBoundaryUnsupported {} => {
                    let diagnostic_alias = format!("D{}", diagnostics.len());
                    diagnostics.insert(diagnostic_alias.clone(), detail_handle.clone());
                    writeln!(
                        text,
                        "{preview_alias} action={} status=successor_boundary_unsupported inspect={diagnostic_alias} commit=unavailable",
                        action.alias
                    )
                    .unwrap();
                    PreviewLeaseStatusV1::SuccessorBoundaryUnsupported
                }
            };
            if entries
                .insert(
                    preview_alias,
                    PreviewLeaseEntryV1 {
                        action_alias: action.alias.clone(),
                        action_id: action.action_id.clone(),
                        status,
                    },
                )
                .is_some()
            {
                return Err(preview_error(
                    "PreviewInvalid",
                    "preview lease generated duplicate aliases",
                ));
            }
        }
        Ok(Self {
            scope: resolved.scope,
            entries,
            diagnostics,
            text,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn scope(&self) -> &LeaseScopeV1 {
        &self.scope
    }

    pub fn diagnostic_entries(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.diagnostics
            .iter()
            .map(|(alias, handle)| (alias.clone(), handle.clone()))
    }

    pub fn resolve_commit(
        &self,
        repository: &Path,
        alias: &str,
    ) -> Result<ResolvedPreviewCommitV1, PreviewLeaseErrorV1> {
        let entry = self.resolve_entry(alias, "commit")?;
        let LeaseSourceV1::SelectedHead(expected) = &self.scope.source else {
            return Err(preview_error(
                "PreviewUnavailable",
                format!("{alias} came from an observe-only root and cannot move a head"),
            ));
        };
        let exact = self.resolve_exact(repository, alias, entry)?;
        Ok(ResolvedPreviewCommitV1 {
            selector_name: expected.selector.selector_name.clone(),
            receipt_id: exact.receipt_id,
            successor_root_id: exact.successor_root_id,
        })
    }

    pub fn resolve_successor(
        &self,
        repository: &Path,
        alias: &str,
    ) -> Result<ResolvedPreviewSuccessorV1, PreviewLeaseErrorV1> {
        let entry = self.resolve_entry(alias, "observe")?;
        let exact = self.resolve_exact(repository, alias, entry)?;
        Ok(ResolvedPreviewSuccessorV1 {
            successor_root_id: exact.successor_root_id,
        })
    }

    fn resolve_entry<'a>(
        &'a self,
        alias: &str,
        operation: &str,
    ) -> Result<&'a PreviewLeaseEntryV1, PreviewLeaseErrorV1> {
        if !is_alias(alias, 'P') {
            return Err(preview_error(
                "PreviewUnknown",
                format!("{operation} requires one P alias from the current preview"),
            ));
        }
        self.entries.get(alias).ok_or_else(|| {
            preview_error(
                "PreviewUnknown",
                format!("{alias} is absent from the current preview"),
            )
        })
    }

    fn resolve_exact(
        &self,
        repository: &Path,
        alias: &str,
        entry: &PreviewLeaseEntryV1,
    ) -> Result<ResolvedExactPreviewV1, PreviewLeaseErrorV1> {
        let PreviewLeaseStatusV1::Exact {
            receipt_id,
            successor_root_id,
        } = &entry.status
        else {
            return Err(preview_error(
                "PreviewUnavailable",
                format!("{alias} has no Exact successor"),
            ));
        };
        self.scope
            .validate(repository)
            .map_err(|error| preview_error("PreviewExpired", error.detail))?;
        let store = sts_paired_root::StateRootStoreV2::new(
            repository.join(".oracle-lab").join("paired-root-store"),
        )
        .with_operation_verification_cache();
        let receipt = store.load_receipt(receipt_id).map_err(|error| {
            preview_error(
                "PreviewExpired",
                format!("{alias} receipt no longer validates: {error}"),
            )
        })?;
        if receipt.receipt.source_root_id != self.scope.root_id
            || receipt.receipt.successor_root_id != *successor_root_id
            || receipt.receipt.action.source_root_id != self.scope.root_id
            || receipt.receipt.action.action_space_id != self.scope.action_space_id
            || receipt.receipt.action.handle_id != entry.action_id
        {
            return Err(preview_error(
                "PreviewExpired",
                format!("{alias} durable receipt no longer matches its preview lease"),
            ));
        }
        Ok(ResolvedExactPreviewV1 {
            receipt_id: receipt_id.clone(),
            successor_root_id: successor_root_id.clone(),
        })
    }
}

pub(super) fn preview_required() -> PreviewLeaseErrorV1 {
    preview_error(
        "PreviewRequired",
        "no current preview lease; preview an A alias before using a P alias",
    )
}

fn preview_error(code: &'static str, detail: impl Into<String>) -> PreviewLeaseErrorV1 {
    PreviewLeaseErrorV1 {
        code,
        detail: detail.into(),
    }
}

fn is_alias(value: &str, prefix: char) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|ordinal| ordinal.parse::<usize>().ok())
        .is_some_and(|ordinal| value == format!("{prefix}{ordinal}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lease_source::LeaseScopeV1;
    use crate::manifest_lease::{ResolvedActionAliasV1, ResolvedActionAliasesV1};
    use crate::test_support::paired_store_fixture;

    fn exact_summary(
        action_id: &str,
        receipt_id: &str,
        successor_root_id: &str,
    ) -> PreviewMemberSummaryV1 {
        PreviewMemberSummaryV1 {
            action_id: action_id.to_owned(),
            status: PreviewMemberStatusV1::Exact {
                receipt_id: receipt_id.to_owned(),
                successor_root_id: successor_root_id.to_owned(),
            },
            exact_count: 9,
            total_count: 9,
        }
    }

    #[test]
    fn selected_exact_preview_is_short_hash_free_and_commit_ready() {
        let fixture = paired_store_fixture();
        let selected = fixture.store.resolve_head_selector("official").unwrap();
        let action = &fixture.first.receipt.action;
        let resolved = ResolvedActionAliasesV1 {
            scope: LeaseScopeV1::new(
                fixture.source.root_id.clone(),
                action.action_space_id.clone(),
                LeaseSourceV1::SelectedHead(Box::new(selected)),
            ),
            actions: vec![ResolvedActionAliasV1 {
                alias: "A0".to_owned(),
                action_id: action.handle_id.clone(),
            }],
        };
        let lease = PreviewLeaseV1::new(
            &fixture.repository,
            resolved,
            &[exact_summary(
                &action.handle_id,
                &fixture.first.receipt_id,
                &fixture.first.receipt.successor_root_id,
            )],
            "workbench-artifact-v1-detail".to_owned(),
        )
        .unwrap();
        assert_eq!(
            lease.text(),
            "STS-PREVIEW 1\nmode=independent_actions\nauthority=lab_head\n\n[previews]\nP0 action=A0 exact=9/9 continue=ready commit=ready\n"
        );
        assert!(!lease.text().contains(&fixture.first.receipt_id));
        let commit = lease.resolve_commit(&fixture.repository, "P0").unwrap();
        assert_eq!(commit.selector_name, "official");
        assert_eq!(commit.receipt_id, fixture.first.receipt_id);
        assert_eq!(
            commit.successor_root_id,
            fixture.first.receipt.successor_root_id
        );
    }

    #[test]
    fn explicit_preview_cannot_acquire_head_commit_authority() {
        let fixture = paired_store_fixture();
        let action = &fixture.first.receipt.action;
        let lease = PreviewLeaseV1::new(
            &fixture.repository,
            ResolvedActionAliasesV1 {
                scope: LeaseScopeV1::new(
                    fixture.source.root_id.clone(),
                    action.action_space_id.clone(),
                    LeaseSourceV1::ExplicitRoot,
                ),
                actions: vec![ResolvedActionAliasV1 {
                    alias: "A0".to_owned(),
                    action_id: action.handle_id.clone(),
                }],
            },
            &[exact_summary(
                &action.handle_id,
                &fixture.first.receipt_id,
                &fixture.first.receipt.successor_root_id,
            )],
            "workbench-artifact-v1-detail".to_owned(),
        )
        .unwrap();
        assert!(lease.text().contains("commit=observe_only"));
        let successor = lease.resolve_successor(&fixture.repository, "P0").unwrap();
        assert_eq!(
            successor.successor_root_id,
            fixture.first.receipt.successor_root_id
        );
        let error = lease.resolve_commit(&fixture.repository, "P0").unwrap_err();
        assert_eq!(error.code, "PreviewUnavailable");
    }

    #[test]
    fn selected_preview_expires_when_the_observed_head_moves() {
        let fixture = paired_store_fixture();
        let selected = fixture.store.resolve_head_selector("official").unwrap();
        let action = &fixture.first.receipt.action;
        let lease = PreviewLeaseV1::new(
            &fixture.repository,
            ResolvedActionAliasesV1 {
                scope: LeaseScopeV1::new(
                    fixture.source.root_id.clone(),
                    action.action_space_id.clone(),
                    LeaseSourceV1::SelectedHead(Box::new(selected)),
                ),
                actions: vec![ResolvedActionAliasV1 {
                    alias: "A0".to_owned(),
                    action_id: action.handle_id.clone(),
                }],
            },
            &[exact_summary(
                &action.handle_id,
                &fixture.first.receipt_id,
                &fixture.first.receipt.successor_root_id,
            )],
            "workbench-artifact-v1-detail".to_owned(),
        )
        .unwrap();
        fixture
            .store
            .commit_selected_receipt_idempotent("official", &fixture.first.receipt_id)
            .unwrap();
        let error = lease.resolve_commit(&fixture.repository, "P0").unwrap_err();
        assert_eq!(error.code, "PreviewExpired");
    }

    #[test]
    fn divergence_has_only_a_short_diagnostic_alias() {
        let fixture = paired_store_fixture();
        let action = &fixture.first.receipt.action;
        let detail = "workbench-artifact-v1-detail";
        let lease = PreviewLeaseV1::new(
            &fixture.repository,
            ResolvedActionAliasesV1 {
                scope: LeaseScopeV1::new(
                    fixture.source.root_id.clone(),
                    action.action_space_id.clone(),
                    LeaseSourceV1::ExplicitRoot,
                ),
                actions: vec![ResolvedActionAliasV1 {
                    alias: "A0".to_owned(),
                    action_id: action.handle_id.clone(),
                }],
            },
            &[PreviewMemberSummaryV1 {
                action_id: action.handle_id.clone(),
                status: PreviewMemberStatusV1::Diverged {
                    divergence_id: "paired-divergence-v1-hidden".to_owned(),
                },
                exact_count: 3,
                total_count: 4,
            }],
            detail.to_owned(),
        )
        .unwrap();
        assert!(lease.text().contains("status=diverged inspect=D0"));
        assert!(!lease.text().contains("paired-divergence"));
        assert_eq!(
            lease.diagnostic_entries().collect::<Vec<_>>(),
            vec![("D0".to_owned(), detail.to_owned())]
        );
    }
}
