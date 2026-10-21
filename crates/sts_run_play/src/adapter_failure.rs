use serde::Serialize;
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub(crate) const RUN_PLAY_ADAPTER_FAILURE_SCHEMA_V1: &str = "sts-run-play-adapter-failure-v1";
pub(crate) const RUN_PLAY_ADAPTER_FAILED_CODE: &str = "run_play_adapter_failed";
pub(crate) const PAIRED_ROOT_VERSION_DRIFT_CODE: &str = "paired_root_version_drift";
pub(crate) const PERSISTENT_MASTER_DECK_DIVERGED_CODE: &str = "persistent_master_deck_diverged";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunPlayAdapterFailureV1 {
    pub schema: String,
    pub code: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<RunPlayAdapterFailureDetailsV1>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum RunPlayAdapterFailureDetailsV1 {
    PersistentMasterDeckDivergence {
        original_game_card_count: usize,
        simulator_card_count: usize,
        first_different_ordinal: Option<usize>,
        original_game: Option<Value>,
        simulator: Option<Value>,
    },
}

impl RunPlayAdapterFailureV1 {
    pub(crate) fn generic(detail: impl Into<String>) -> Self {
        Self::new(RUN_PLAY_ADAPTER_FAILED_CODE, detail)
    }

    pub(crate) fn from_error_detail(detail: impl Into<String>) -> Self {
        let detail = detail.into();
        if detail.starts_with("paired-root version drift for ") {
            return Self::new(PAIRED_ROOT_VERSION_DRIFT_CODE, detail);
        }
        Self::generic(detail)
    }

    pub(crate) fn persistent_master_deck_diverged(
        original_game: &Value,
        simulator: &Value,
    ) -> Self {
        let original_cards = original_game.as_array().map(Vec::as_slice).unwrap_or(&[]);
        let simulator_cards = simulator.as_array().map(Vec::as_slice).unwrap_or(&[]);
        let first_different_ordinal = (0..original_cards.len().max(simulator_cards.len()))
            .find(|ordinal| original_cards.get(*ordinal) != simulator_cards.get(*ordinal));
        Self {
            schema: RUN_PLAY_ADAPTER_FAILURE_SCHEMA_V1.to_owned(),
            code: PERSISTENT_MASTER_DECK_DIVERGED_CODE.to_owned(),
            detail: "persistent master deck diverged before identity binding".to_owned(),
            details: Some(
                RunPlayAdapterFailureDetailsV1::PersistentMasterDeckDivergence {
                    original_game_card_count: original_cards.len(),
                    simulator_card_count: simulator_cards.len(),
                    first_different_ordinal,
                    original_game: first_different_ordinal
                        .and_then(|ordinal| original_cards.get(ordinal).cloned()),
                    simulator: first_different_ordinal
                        .and_then(|ordinal| simulator_cards.get(ordinal).cloned()),
                },
            ),
        }
    }

    pub(crate) fn new(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            schema: RUN_PLAY_ADAPTER_FAILURE_SCHEMA_V1.to_owned(),
            code: code.into(),
            detail: detail.into(),
            details: None,
        }
    }
}

impl From<String> for RunPlayAdapterFailureV1 {
    fn from(detail: String) -> Self {
        Self::generic(detail)
    }
}

pub(crate) fn write_failure_report_if_absent(
    path: &Path,
    failure: &RunPlayAdapterFailureV1,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(failure)
        .map_err(|error| format!("failed to encode run-play adapter failure: {error}"))?;
    bytes.push(b'\n');
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => {
            return Err(format!(
                "failed to create run-play adapter failure report {}: {error}",
                path.display()
            ))
        }
    };
    file.write_all(&bytes).map_err(|error| {
        format!(
            "failed to write run-play adapter failure report {}: {error}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_deck_failure_has_a_stable_code_and_private_detail() {
        let original_game = serde_json::json!([
            {"ordinal": 0, "card_id": "Strike_R", "upgrades": 0},
            {"ordinal": 1, "card_id": "Regret", "upgrades": 0}
        ]);
        let simulator = serde_json::json!([
            {"ordinal": 0, "card_id": "Strike_R", "upgrades": 0}
        ]);
        let failure =
            RunPlayAdapterFailureV1::persistent_master_deck_diverged(&original_game, &simulator);

        assert_eq!(
            serde_json::to_value(&failure).unwrap(),
            serde_json::json!({
                "schema": RUN_PLAY_ADAPTER_FAILURE_SCHEMA_V1,
                "code": PERSISTENT_MASTER_DECK_DIVERGED_CODE,
                "detail": "persistent master deck diverged before identity binding",
                "details": {
                    "kind": "persistent_master_deck_divergence",
                    "original_game_card_count": 2,
                    "simulator_card_count": 1,
                    "first_different_ordinal": 1,
                    "original_game": {"ordinal": 1, "card_id": "Regret", "upgrades": 0},
                    "simulator": null,
                },
            })
        );
    }

    #[test]
    fn version_drift_is_a_closed_retry_forbidden_cause() {
        let failure = RunPlayAdapterFailureV1::from_error_detail(
            "paired-root version drift for communicationmod_jar: stored=old active=new",
        );

        assert_eq!(failure.code, PAIRED_ROOT_VERSION_DRIFT_CODE);
        assert_eq!(
            failure.detail,
            "paired-root version drift for communicationmod_jar: stored=old active=new"
        );
        assert!(failure.details.is_none());
    }
}
