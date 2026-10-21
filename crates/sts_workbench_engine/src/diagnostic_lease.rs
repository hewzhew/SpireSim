use crate::lease_source::LeaseScopeV1;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DiagnosticLeaseV1 {
    scope: LeaseScopeV1,
    entries: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DiagnosticLeaseErrorV1 {
    pub code: &'static str,
    pub detail: String,
}

impl DiagnosticLeaseV1 {
    pub fn new(
        scope: LeaseScopeV1,
        entries: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, DiagnosticLeaseErrorV1> {
        let mut indexed = BTreeMap::new();
        for (alias, handle) in entries {
            if !is_alias(&alias, 'D')
                || handle.trim().is_empty()
                || indexed.insert(alias, handle).is_some()
            {
                return Err(diagnostic_error(
                    "DiagnosticInvalid",
                    "current view produced an invalid diagnostic alias table",
                ));
            }
        }
        Ok(Self {
            scope,
            entries: indexed,
        })
    }

    pub fn resolve(&self, repository: &Path, alias: &str) -> Result<&str, DiagnosticLeaseErrorV1> {
        if !is_alias(alias, 'D') {
            return Err(diagnostic_error(
                "DiagnosticUnknown",
                "inspect requires one D alias from the current view",
            ));
        }
        self.scope
            .validate(repository)
            .map_err(|error| diagnostic_error("DiagnosticExpired", error.detail))?;
        self.entries.get(alias).map(String::as_str).ok_or_else(|| {
            diagnostic_error(
                "DiagnosticUnknown",
                format!("{alias} is absent from the current view"),
            )
        })
    }
}

pub(super) fn diagnostic_required() -> DiagnosticLeaseErrorV1 {
    diagnostic_error(
        "InspectRequired",
        "no current diagnostic lease; observe, search, or preview before using a D alias",
    )
}

fn diagnostic_error(code: &'static str, detail: impl Into<String>) -> DiagnosticLeaseErrorV1 {
    DiagnosticLeaseErrorV1 {
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
    use crate::lease_source::LeaseSourceV1;
    use crate::test_support::paired_store_fixture;

    #[test]
    fn diagnostic_alias_is_scoped_to_the_current_source() {
        let fixture = paired_store_fixture();
        let selected = fixture.store.resolve_head_selector("official").unwrap();
        let lease = DiagnosticLeaseV1::new(
            LeaseScopeV1::new(
                fixture.source.root_id.clone(),
                fixture.source.root.preflight.action_space_id.clone(),
                LeaseSourceV1::SelectedHead(Box::new(selected)),
            ),
            [("D0".to_owned(), "detail-handle".to_owned())],
        )
        .unwrap();
        assert_eq!(
            lease.resolve(&fixture.repository, "D0").unwrap(),
            "detail-handle"
        );
        fixture
            .store
            .commit_selected_receipt_idempotent("official", &fixture.first.receipt_id)
            .unwrap();
        assert_eq!(
            lease.resolve(&fixture.repository, "D0").unwrap_err().code,
            "DiagnosticExpired"
        );
    }
}
