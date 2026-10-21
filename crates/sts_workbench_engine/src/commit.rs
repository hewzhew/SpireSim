use crate::artifact::publish_artifact;
use serde_json::{json, Value};
use std::path::Path;
use sts_paired_root::{PairedHeadCommitStatusV1, StateRootStoreV2};

const COMMIT_RESPONSE_SCHEMA_V1: &str = "sts-workbench-commit-response-v1";
const COMMIT_ARTIFACT_TYPE: &str = "paired_root_commit_detail";
const EXECUTION_SCOPE: &str = "artifact_pointer_only_no_runtime_execution";

pub(super) struct CommitOutcomeV1 {
    pub response: Value,
    pub committed_root_id: String,
}

pub(super) fn commit_selected_receipt(
    repository: &Path,
    selector_name: &str,
    receipt_id: &str,
) -> Result<CommitOutcomeV1, String> {
    let store = StateRootStoreV2::new(repository.join(".oracle-lab").join("paired-root-store"))
        .with_operation_verification_cache();
    let outcome = store.commit_selected_receipt_idempotent(selector_name, receipt_id)?;
    let status = match outcome.commit.status {
        PairedHeadCommitStatusV1::Committed => "committed",
        PairedHeadCommitStatusV1::AlreadyCommitted => "already_committed",
    };
    let committed_head = &outcome.commit.committed_head;
    let current_head = &outcome.commit.current_head;
    if outcome.selector.selector_name != selector_name
        || outcome.commit.receipt_id != receipt_id
        || committed_head.head_name != outcome.selector.head_name
        || current_head.head_name != outcome.selector.head_name
        || committed_head.committed_receipt_id.as_deref() != Some(receipt_id)
        || current_head.revision < committed_head.revision
        || matches!(outcome.commit.status, PairedHeadCommitStatusV1::Committed)
            && current_head != committed_head
    {
        return Err("paired-root commit returned inconsistent typed evidence".to_owned());
    }
    let detail = json!({
        "schema": "sts-workbench-commit-detail-v1",
        "status": status,
        "receipt_id": receipt_id,
        "committed_revision": committed_head.revision,
        "committed_root_id": committed_head.root_id,
        "current_revision": current_head.revision,
        "current_root_id": current_head.root_id,
        "execution_scope": EXECUTION_SCOPE,
    });
    let detail_handle = publish_artifact(repository, COMMIT_ARTIFACT_TYPE, &detail)?;
    let response = json!({
        "schema": COMMIT_RESPONSE_SCHEMA_V1,
        "status": status,
        "receipt_id": receipt_id,
        "committed_revision": committed_head.revision,
        "committed_root_id": committed_head.root_id,
        "current_revision": current_head.revision,
        "current_root_id": current_head.root_id,
        "execution_scope": EXECUTION_SCOPE,
        "detail_handle": detail_handle,
    });
    Ok(CommitOutcomeV1 {
        response,
        committed_root_id: committed_head.root_id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::paired_store_fixture;

    #[test]
    fn commit_is_receipt_only_and_idempotent() {
        let fixture = paired_store_fixture();
        let committed =
            commit_selected_receipt(&fixture.repository, "official", &fixture.first.receipt_id)
                .unwrap();
        assert_eq!(committed.response["schema"], COMMIT_RESPONSE_SCHEMA_V1);
        assert_eq!(committed.response["status"], "committed");
        assert_eq!(committed.response["execution_scope"], EXECUTION_SCOPE);

        let retried =
            commit_selected_receipt(&fixture.repository, "official", &fixture.first.receipt_id)
                .unwrap();
        assert_eq!(retried.response["status"], "already_committed");
        assert_eq!(retried.response["current_revision"], 1);

        let inspected = crate::artifact::inspect_artifact(
            &fixture.repository,
            committed.response["detail_handle"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(inspected["artifact_type"], COMMIT_ARTIFACT_TYPE);
    }

    #[test]
    fn stale_or_other_head_receipt_is_rejected() {
        let fixture = paired_store_fixture();
        commit_selected_receipt(&fixture.repository, "official", &fixture.first.receipt_id)
            .unwrap();
        assert!(commit_selected_receipt(
            &fixture.repository,
            "official",
            &fixture.second.receipt_id,
        )
        .is_err());

        let other = paired_store_fixture();
        let incompatible_root = other.second.receipt.successor_root_id.clone();
        other
            .store
            .create_head("other", &incompatible_root)
            .unwrap();
        other
            .store
            .select_head("official", Some(0), "other")
            .unwrap();
        assert!(
            commit_selected_receipt(&other.repository, "official", &other.first.receipt_id,)
                .is_err()
        );
        assert_eq!(other.store.load_head("other").unwrap().revision, 0);
    }
}
