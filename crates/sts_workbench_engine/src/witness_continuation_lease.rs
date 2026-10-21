use crate::lease_source::LeaseScopeV1;
use std::path::Path;

const WITNESS_CONTINUATION_ALIAS_V1: &str = "P0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WitnessContinuationLeaseV1 {
    scope: LeaseScopeV1,
    successor_root_id: String,
    transition_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WitnessContinuationLeaseErrorV1 {
    pub code: &'static str,
    pub detail: String,
}

impl WitnessContinuationLeaseV1 {
    pub fn new(
        repository: &Path,
        scope: LeaseScopeV1,
        successor_root_id: String,
        transition_count: u64,
    ) -> Result<Self, WitnessContinuationLeaseErrorV1> {
        let lease = Self {
            scope,
            successor_root_id,
            transition_count,
        };
        lease.validate(repository, "WitnessPreviewInvalid")?;
        Ok(lease)
    }

    pub fn resolve(
        &self,
        repository: &Path,
        alias: &str,
    ) -> Result<String, WitnessContinuationLeaseErrorV1> {
        if alias != WITNESS_CONTINUATION_ALIAS_V1 {
            return Err(continuation_error(
                "PreviewUnknown",
                format!("{alias} is absent from the current witness preview"),
            ));
        }
        self.validate(repository, "PreviewExpired")?;
        Ok(self.successor_root_id.clone())
    }

    fn validate(
        &self,
        repository: &Path,
        code: &'static str,
    ) -> Result<(), WitnessContinuationLeaseErrorV1> {
        self.scope
            .validate(repository)
            .map_err(|error| continuation_error(code, error.detail))?;
        let store = sts_paired_root::StateRootStoreV2::new(
            repository.join(".oracle-lab").join("paired-root-store"),
        )
        .with_operation_verification_cache();
        let ancestry = store
            .logical_ancestry(&self.successor_root_id)
            .map_err(|error| {
                continuation_error(
                    code,
                    format!("witness successor ancestry no longer validates: {error}"),
                )
            })?;
        let source_ordinal = ancestry
            .iter()
            .position(|root_id| root_id == &self.scope.root_id)
            .ok_or_else(|| {
                continuation_error(
                    code,
                    "witness successor does not descend from its source root",
                )
            })?;
        let actual_count = ancestry.len().saturating_sub(source_ordinal + 1);
        if self.transition_count == 0
            || u64::try_from(actual_count).ok() != Some(self.transition_count)
        {
            return Err(continuation_error(
                code,
                "witness successor ancestry does not match its Exact step count",
            ));
        }
        Ok(())
    }
}

pub(super) fn commit_unavailable() -> WitnessContinuationLeaseErrorV1 {
    continuation_error(
        "PreviewUnavailable",
        "a sequential witness continuation is observe-only and has no atomic receipt to commit",
    )
}

fn continuation_error(
    code: &'static str,
    detail: impl Into<String>,
) -> WitnessContinuationLeaseErrorV1 {
    WitnessContinuationLeaseErrorV1 {
        code,
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lease_source::LeaseSourceV1;
    use crate::test_support::{add_fixture_successor, paired_store_fixture};
    use sts_paired_root::PairedActionV1;

    fn two_step_successor(
        fixture: &crate::test_support::PairedStoreFixture,
    ) -> sts_paired_root::TransitionReceiptArtifactV1 {
        let first_root = fixture
            .store
            .load_root(&fixture.first.receipt.successor_root_id)
            .unwrap();
        add_fixture_successor(&fixture.store, &first_root, PairedActionV1::EndTurn)
    }

    #[test]
    fn exact_witness_path_issues_one_observe_only_continuation() {
        let fixture = paired_store_fixture();
        let second_step = two_step_successor(&fixture);
        let lease = WitnessContinuationLeaseV1::new(
            &fixture.repository,
            LeaseScopeV1::new(
                fixture.source.root_id.clone(),
                fixture.source.root.preflight.action_space_id.clone(),
                LeaseSourceV1::ExplicitRoot,
            ),
            second_step.receipt.successor_root_id.clone(),
            2,
        )
        .unwrap();

        assert_eq!(
            lease.resolve(&fixture.repository, "P0").unwrap(),
            second_step.receipt.successor_root_id
        );
        assert_eq!(
            lease.resolve(&fixture.repository, "P1").unwrap_err().code,
            "PreviewUnknown"
        );
        assert_eq!(commit_unavailable().code, "PreviewUnavailable");
    }

    #[test]
    fn continuation_rejects_a_wrong_source_or_step_count() {
        let fixture = paired_store_fixture();
        let second_step = two_step_successor(&fixture);
        let scope = LeaseScopeV1::new(
            fixture.source.root_id.clone(),
            fixture.source.root.preflight.action_space_id.clone(),
            LeaseSourceV1::ExplicitRoot,
        );
        assert_eq!(
            WitnessContinuationLeaseV1::new(
                &fixture.repository,
                scope.clone(),
                second_step.receipt.successor_root_id.clone(),
                1,
            )
            .unwrap_err()
            .code,
            "WitnessPreviewInvalid"
        );
        assert_eq!(
            WitnessContinuationLeaseV1::new(
                &fixture.repository,
                scope,
                fixture.source.root_id.clone(),
                2,
            )
            .unwrap_err()
            .code,
            "WitnessPreviewInvalid"
        );
    }
}
