use std::path::Path;
use sts_paired_root::{PairedHeadSelectionResolutionV1, StateRootStoreV2};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum LeaseSourceV1 {
    ExplicitRoot,
    SelectedHead(Box<PairedHeadSelectionResolutionV1>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LeaseScopeV1 {
    pub root_id: String,
    pub action_space_id: String,
    pub source: LeaseSourceV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LeaseSourceErrorV1 {
    pub detail: String,
}

impl LeaseScopeV1 {
    pub fn new(root_id: String, action_space_id: String, source: LeaseSourceV1) -> Self {
        Self {
            root_id,
            action_space_id,
            source,
        }
    }

    pub fn validate(&self, repository: &Path) -> Result<(), LeaseSourceErrorV1> {
        let store = StateRootStoreV2::new(repository.join(".oracle-lab").join("paired-root-store"))
            .with_operation_verification_cache();
        let root = store.load_root(&self.root_id).map_err(|error| {
            lease_error(format!("lease source root no longer validates: {error}"))
        })?;
        if root.root.preflight.action_space_id != self.action_space_id {
            return Err(lease_error(
                "lease action space no longer matches its source root",
            ));
        }
        if let LeaseSourceV1::SelectedHead(expected) = &self.source {
            let current = store
                .resolve_head_selector(&expected.selector.selector_name)
                .map_err(|error| {
                    lease_error(format!("selected lease source no longer resolves: {error}"))
                })?;
            if current != **expected {
                return Err(lease_error("selected lease source changed after issuance"));
            }
        }
        Ok(())
    }
}

fn lease_error(detail: impl Into<String>) -> LeaseSourceErrorV1 {
    LeaseSourceErrorV1 {
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::paired_store_fixture;

    #[test]
    fn explicit_scope_validates_its_immutable_root_and_action_space() {
        let fixture = paired_store_fixture();
        let scope = LeaseScopeV1::new(
            fixture.source.root_id.clone(),
            fixture.source.root.preflight.action_space_id.clone(),
            LeaseSourceV1::ExplicitRoot,
        );
        scope.validate(&fixture.repository).unwrap();

        let wrong = LeaseScopeV1::new(
            fixture.source.root_id.clone(),
            "wrong-action-space".to_owned(),
            LeaseSourceV1::ExplicitRoot,
        );
        assert!(wrong
            .validate(&fixture.repository)
            .unwrap_err()
            .detail
            .contains("action space"));
    }

    #[test]
    fn selected_scope_expires_after_head_advance() {
        let fixture = paired_store_fixture();
        let selected = fixture.store.resolve_head_selector("official").unwrap();
        let scope = LeaseScopeV1::new(
            fixture.source.root_id.clone(),
            fixture.source.root.preflight.action_space_id.clone(),
            LeaseSourceV1::SelectedHead(Box::new(selected)),
        );
        fixture
            .store
            .commit_selected_receipt_idempotent("official", &fixture.first.receipt_id)
            .unwrap();
        assert!(scope
            .validate(&fixture.repository)
            .unwrap_err()
            .detail
            .contains("changed"));
    }
}
