use crate::domain_manifest::RenderedManifestV1;
use crate::lease_source::{LeaseScopeV1, LeaseSourceV1};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActionLeaseEntryV1 {
    action_id: Option<String>,
    available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ActionLeaseV1 {
    scope: LeaseScopeV1,
    entries: BTreeMap<String, ActionLeaseEntryV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ResolvedActionAliasesV1 {
    pub scope: LeaseScopeV1,
    pub actions: Vec<ResolvedActionAliasV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ResolvedActionAliasV1 {
    pub alias: String,
    pub action_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ActionLeaseErrorV1 {
    pub code: &'static str,
    pub detail: String,
}

impl ActionLeaseV1 {
    pub fn new(
        root_id: String,
        action_space_id: String,
        source: LeaseSourceV1,
        rendered: &RenderedManifestV1,
    ) -> Result<Self, String> {
        let mut entries = BTreeMap::new();
        for (ordinal, action) in rendered.actions.iter().enumerate() {
            let expected = format!("A{ordinal}");
            if action.alias != expected
                || action
                    .action_id
                    .as_deref()
                    .is_some_and(|action_id| action_id.trim().is_empty())
                || action.available && action.action_id.is_none()
                || entries
                    .insert(
                        action.alias.clone(),
                        ActionLeaseEntryV1 {
                            action_id: action.action_id.clone(),
                            available: action.available,
                        },
                    )
                    .is_some()
            {
                return Err("rendered manifest has an invalid action lease table".to_owned());
            }
        }
        Ok(Self {
            scope: LeaseScopeV1::new(root_id, action_space_id, source),
            entries,
        })
    }

    pub fn resolve(
        &self,
        repository: &Path,
        aliases: &[String],
    ) -> Result<ResolvedActionAliasesV1, ActionLeaseErrorV1> {
        if aliases.is_empty() || aliases.len() > 64 {
            return Err(action_error(
                "ActionUnknown",
                "preview requires between one and sixty-four A aliases",
            ));
        }
        let mut distinct = BTreeSet::new();
        if aliases
            .iter()
            .any(|alias| !is_action_alias(alias) || !distinct.insert(alias.as_str()))
        {
            return Err(action_error(
                "ActionUnknown",
                "preview requires distinct aliases from the current manifest",
            ));
        }
        self.scope
            .validate(repository)
            .map_err(|error| action_error("ActionExpired", error.detail))?;
        let mut actions = Vec::with_capacity(aliases.len());
        for alias in aliases {
            let entry = self.entries.get(alias).ok_or_else(|| {
                action_error(
                    "ActionUnknown",
                    format!("{alias} is absent from the current manifest"),
                )
            })?;
            if !entry.available {
                return Err(action_error(
                    "ActionUnavailable",
                    format!("{alias} is visible but not authorized for preview"),
                ));
            }
            let action_id = entry.action_id.as_ref().ok_or_else(|| {
                action_error(
                    "ActionUnavailable",
                    format!("{alias} has no executable action identity"),
                )
            })?;
            actions.push(ResolvedActionAliasV1 {
                alias: alias.clone(),
                action_id: action_id.clone(),
            });
        }
        Ok(ResolvedActionAliasesV1 {
            scope: self.scope.clone(),
            actions,
        })
    }

    pub fn resolve_scope(&self, repository: &Path) -> Result<LeaseScopeV1, ActionLeaseErrorV1> {
        self.scope
            .validate(repository)
            .map_err(|error| action_error("ActionExpired", error.detail))?;
        Ok(self.scope.clone())
    }
}

pub(super) fn observe_required() -> ActionLeaseErrorV1 {
    action_error(
        "ObserveRequired",
        "no current action lease; call observe before using an A alias",
    )
}

fn action_error(code: &'static str, detail: impl Into<String>) -> ActionLeaseErrorV1 {
    ActionLeaseErrorV1 {
        code,
        detail: detail.into(),
    }
}

fn is_action_alias(value: &str) -> bool {
    value
        .strip_prefix('A')
        .and_then(|ordinal| ordinal.parse::<usize>().ok())
        .is_some_and(|ordinal| value == format!("A{ordinal}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_manifest::{ActionLeaseEntryV1 as RenderedActionV1, RenderedManifestV1};
    use crate::test_support::paired_store_fixture;

    fn rendered(actions: &[(&str, &str, bool)]) -> RenderedManifestV1 {
        RenderedManifestV1 {
            text: "fixture".to_owned(),
            actions: actions
                .iter()
                .map(|(alias, action_id, available)| RenderedActionV1 {
                    alias: (*alias).to_owned(),
                    action_id: Some((*action_id).to_owned()),
                    available: *available,
                })
                .collect(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn explicit_lease_resolves_only_current_available_aliases() {
        let fixture = paired_store_fixture();
        let lease = ActionLeaseV1::new(
            fixture.source.root_id.clone(),
            fixture.source.root.preflight.action_space_id.clone(),
            LeaseSourceV1::ExplicitRoot,
            &rendered(&[("A0", "action-zero", true), ("A1", "action-one", false)]),
        )
        .unwrap();
        let resolved = lease
            .resolve(&fixture.repository, &["A0".to_owned()])
            .unwrap();
        assert_eq!(resolved.scope.root_id, fixture.source.root_id);
        assert_eq!(
            lease.resolve_scope(&fixture.repository).unwrap(),
            resolved.scope
        );
        assert_eq!(resolved.actions.len(), 1);
        assert_eq!(resolved.actions[0].alias, "A0");
        assert_eq!(resolved.actions[0].action_id, "action-zero");
        assert_eq!(
            lease
                .resolve(&fixture.repository, &["A2".to_owned()])
                .unwrap_err()
                .code,
            "ActionUnknown"
        );
        assert_eq!(
            lease
                .resolve(&fixture.repository, &["A1".to_owned()])
                .unwrap_err()
                .code,
            "ActionUnavailable"
        );
    }

    #[test]
    fn visible_unavailable_action_needs_no_fabricated_execution_identity() {
        let fixture = paired_store_fixture();
        let rendered = RenderedManifestV1 {
            text: "STS-MANIFEST 1\n\n[actions]\nA0 close unavailable=unknown gap=x\n".to_owned(),
            actions: vec![RenderedActionV1 {
                alias: "A0".to_owned(),
                action_id: None,
                available: false,
            }],
            diagnostics: Vec::new(),
        };
        let lease = ActionLeaseV1::new(
            fixture.source.root_id.clone(),
            fixture.source.root.preflight.action_space_id.clone(),
            LeaseSourceV1::ExplicitRoot,
            &rendered,
        )
        .unwrap();
        assert_eq!(
            lease
                .resolve(&fixture.repository, &["A0".to_owned()])
                .unwrap_err()
                .code,
            "ActionUnavailable"
        );

        let mut invalid = rendered;
        invalid.actions[0].available = true;
        assert!(ActionLeaseV1::new(
            fixture.source.root_id.clone(),
            fixture.source.root.preflight.action_space_id.clone(),
            LeaseSourceV1::ExplicitRoot,
            &invalid,
        )
        .is_err());
    }

    #[test]
    fn selected_lease_expires_when_the_head_advances() {
        let fixture = paired_store_fixture();
        let selected = fixture.store.resolve_head_selector("official").unwrap();
        let lease = ActionLeaseV1::new(
            fixture.source.root_id.clone(),
            fixture.source.root.preflight.action_space_id.clone(),
            LeaseSourceV1::SelectedHead(Box::new(selected)),
            &rendered(&[("A0", "action-zero", true)]),
        )
        .unwrap();
        fixture
            .store
            .commit_selected_receipt_idempotent("official", &fixture.first.receipt_id)
            .unwrap();
        assert_eq!(
            lease
                .resolve(&fixture.repository, &["A0".to_owned()])
                .unwrap_err()
                .code,
            "ActionExpired"
        );
    }
}
