use std::path::Path;
use sts_paired_root::{PairedHeadSelectionResolutionV1, StateRootStoreV2};

pub(super) fn run_at_selected_root_with_guard<F, T>(
    repository: &Path,
    selector_name: &str,
    operation_name: &str,
    operation: F,
) -> Result<(T, PairedHeadSelectionResolutionV1), String>
where
    F: FnOnce(&str) -> Result<T, String>,
{
    let store = StateRootStoreV2::new(repository.join(".oracle-lab").join("paired-root-store"))
        .with_operation_verification_cache();
    let before = store.resolve_head_selector(selector_name)?;
    let result = operation(&before.head.root_id)?;
    let after = store.resolve_head_selector(selector_name)?;
    if after != before {
        return Err(format!(
            "paired-root selector changed during {operation_name}; retry"
        ));
    }
    Ok((result, before))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::paired_store_fixture;
    use serde_json::json;

    #[test]
    fn stable_selection_returns_the_operation_result() {
        let fixture = paired_store_fixture();
        let (result, selected) = run_at_selected_root_with_guard(
            &fixture.repository,
            "official",
            "fixture read",
            |root_id| {
                assert_eq!(root_id, fixture.source.root_id);
                Ok(json!({"root_id": root_id}))
            },
        )
        .unwrap();
        assert_eq!(result["root_id"], fixture.source.root_id);
        assert_eq!(selected.head.root_id, fixture.source.root_id);
        assert_eq!(fixture.store.load_head("probe").unwrap().revision, 0);
    }

    #[test]
    fn head_advance_during_read_is_rejected() {
        let fixture = paired_store_fixture();
        let error = run_at_selected_root_with_guard(
            &fixture.repository,
            "official",
            "fixture read",
            |_| {
                fixture
                    .store
                    .commit_selected_receipt_idempotent("official", &fixture.first.receipt_id)
                    .unwrap();
                Ok(json!({"untrusted": true}))
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            "paired-root selector changed during fixture read; retry"
        );
        assert_eq!(fixture.store.load_head("probe").unwrap().revision, 1);
    }

    #[test]
    fn selector_switch_during_read_is_rejected() {
        let fixture = paired_store_fixture();
        fixture
            .store
            .create_head("other", &fixture.second.receipt.successor_root_id)
            .unwrap();
        let error = run_at_selected_root_with_guard(
            &fixture.repository,
            "official",
            "fixture read",
            |_| {
                fixture
                    .store
                    .select_head("official", Some(0), "other")
                    .unwrap();
                Ok(json!({"untrusted": true}))
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            "paired-root selector changed during fixture read; retry"
        );
        assert_eq!(
            fixture
                .store
                .resolve_head_selector("official")
                .unwrap()
                .selector
                .head_name,
            "other"
        );
    }
}
