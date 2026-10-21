use crate::{
    adapter::{create_operation_directory, read_json, run_typed_adapter},
    artifact::publish_artifact,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

const SEARCH_REPORT_SCHEMA_V1: &str = "sts-paired-root-search-report-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SearchTranslationStatusV1 {
    Complete,
    MechanicsGap,
}

impl SearchTranslationStatusV1 {
    pub fn token(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::MechanicsGap => "mechanics_gap",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SearchStatusV1 {
    Witness,
    BudgetUnknown,
}

impl SearchStatusV1 {
    pub fn token(self) -> &'static str {
        match self {
            Self::Witness => "witness",
            Self::BudgetUnknown => "budget_unknown",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SearchWitnessSummaryV1 {
    pub witness_handle: String,
    pub plan_step_count: u64,
    pub translation: SearchTranslationStatusV1,
    pub final_hp: i64,
    pub total_potions_consumed: u64,
    pub satisfies_requested_quality: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SearchExecutionV1 {
    pub root_id: String,
    pub action_space_id: String,
    pub status: SearchStatusV1,
    pub summary_exit: String,
    pub lane_count: u64,
    pub bounded_frontier_count: u64,
    pub witnesses: Vec<SearchWitnessSummaryV1>,
    pub detail_handle: String,
}

pub(super) fn search_explicit_root(
    repository: &Path,
    root_id: &str,
    request_count: u64,
) -> Result<SearchExecutionV1, String> {
    if !content_id_has_shape(root_id, "state-root-v2") {
        return Err("explicit search requires a typed PairedRoot identity".to_owned());
    }
    let attempt = create_operation_directory(repository, "search", request_count)?;
    let report_path = attempt.join("search.json");
    let arguments = vec![
        "paired-root-search".to_owned(),
        "--root".to_owned(),
        root_id.to_owned(),
        "--store".to_owned(),
        path_text(&repository.join(".oracle-lab").join("paired-root-store")),
        "--report".to_owned(),
        path_text(&report_path),
    ];
    run_typed_adapter(
        repository,
        &attempt,
        "paired-root-search",
        &arguments,
        Duration::from_secs(30),
    )?;
    let document: Value = read_json(&report_path)?;
    project_search(repository, root_id, document)
}

fn project_search(
    repository: &Path,
    root_id: &str,
    document: Value,
) -> Result<SearchExecutionV1, String> {
    let report = document
        .as_object()
        .ok_or_else(|| "paired-root search report is not a JSON object".to_owned())?;
    let status = required_string(report.get("status"), "search status")?;
    let projected_status = match status {
        "witness" => SearchStatusV1::Witness,
        "budget_unknown" => SearchStatusV1::BudgetUnknown,
        _ => return Err("paired-root search returned an invalid status".to_owned()),
    };
    let action_space_id = required_string(report.get("action_space_id"), "action space")?;
    let summary_exit = required_string(report.get("summary_exit"), "summary exit")?;
    let lanes = required_array(report.get("lanes"), "search lanes")?;
    let witnesses = required_array(report.get("witnesses"), "search witnesses")?;
    let bounded_frontier =
        required_array(report.get("bounded_frontier"), "search bounded frontier")?;
    let checkpoint = report
        .get("rust_checkpoint")
        .and_then(Value::as_object)
        .ok_or_else(|| "paired-root search omitted its Rust checkpoint".to_owned())?;
    let checkpoint_sha256 = required_string(checkpoint.get("sha256"), "checkpoint hash")?;
    let checkpoint_bytes = checkpoint
        .get("bytes")
        .and_then(Value::as_u64)
        .filter(|bytes| *bytes > 0)
        .ok_or_else(|| "paired-root search checkpoint length is invalid".to_owned())?;
    let policy = report
        .get("policy")
        .filter(|value| value.is_object())
        .ok_or_else(|| "paired-root search omitted its typed policy".to_owned())?;
    let _ = (checkpoint_bytes, policy);
    if report.get("schema").and_then(Value::as_str) != Some(SEARCH_REPORT_SCHEMA_V1)
        || report.get("root_id").and_then(Value::as_str) != Some(root_id)
        || !content_id_has_shape(action_space_id, "paired-action-space-v1")
        || !is_sha256(checkpoint_sha256)
        || lanes.is_empty()
        || status == "witness" && witnesses.is_empty()
        || status == "budget_unknown" && !witnesses.is_empty()
    {
        return Err("paired-root search returned invalid scoped evidence".to_owned());
    }

    let mut witness_summaries = Vec::with_capacity(witnesses.len());
    let mut seen = BTreeSet::new();
    for witness in witnesses {
        let witness_object = witness
            .as_object()
            .ok_or_else(|| "paired-root search witness is not an object".to_owned())?;
        let witness_id = required_string(witness_object.get("witness_id"), "witness identity")?;
        if !content_id_has_shape(witness_id, "paired-search-witness-v1") || !seen.insert(witness_id)
        {
            return Err("paired-root search returned an invalid witness identity".to_owned());
        }
        let plan_step_count = witness_object
            .get("plan_step_count")
            .and_then(Value::as_u64)
            .filter(|count| *count > 0)
            .ok_or_else(|| "paired-root search witness has no plan steps".to_owned())?;
        let translation = witness_object
            .get("translation")
            .and_then(Value::as_object)
            .ok_or_else(|| "paired-root search witness omitted its translation".to_owned())?;
        let translation_status = required_string(translation.get("status"), "translation status")?;
        let projected_translation = match translation_status {
            "complete" => SearchTranslationStatusV1::Complete,
            "mechanics_gap" => SearchTranslationStatusV1::MechanicsGap,
            _ => return Err("paired-root search witness has an invalid translation".to_owned()),
        };
        let next_action_handle_id = witness_object
            .get("next_action_handle_id")
            .and_then(Value::as_str);
        match translation_status {
            "complete" => {
                let intents = required_array(translation.get("intents"), "search intents")?;
                if intents.len() != usize::try_from(plan_step_count).unwrap_or(usize::MAX)
                    || !next_action_handle_id
                        .is_some_and(|value| content_id_has_shape(value, "paired-action-v1"))
                {
                    return Err(
                        "complete search witness omitted typed execution evidence".to_owned()
                    );
                }
            }
            "mechanics_gap" => {
                let step_ordinal = translation
                    .get("step_ordinal")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| "search mechanics gap omitted its step".to_owned())?;
                if step_ordinal >= plan_step_count
                    || required_string(translation.get("action_kind"), "gap action")?.is_empty()
                    || required_string(translation.get("reason"), "gap reason")?.is_empty()
                    || next_action_handle_id.is_some()
                {
                    return Err("search mechanics gap is not scoped to a witness step".to_owned());
                }
            }
            _ => unreachable!("translation status was validated above"),
        }
        let outcome = witness_object
            .get("outcome")
            .and_then(Value::as_object)
            .ok_or_else(|| "paired-root search witness omitted its outcome".to_owned())?;
        let final_hp = outcome
            .get("final_hp")
            .and_then(Value::as_i64)
            .ok_or_else(|| "paired-root search outcome omitted final HP".to_owned())?;
        let total_potions_consumed = outcome
            .get("total_potions_consumed")
            .and_then(Value::as_u64)
            .ok_or_else(|| "paired-root search outcome has invalid potion usage".to_owned())?;
        let satisfies_requested_quality = witness_object
            .get("satisfies_requested_quality")
            .and_then(Value::as_bool)
            .ok_or_else(|| "paired-root search witness omitted its quality result".to_owned())?;
        let witness_document = json!({
            "schema": "sts-workbench-search-witness-v1",
            "root_id": root_id,
            "action_space_id": action_space_id,
            "rust_checkpoint": report.get("rust_checkpoint"),
            "policy": report.get("policy"),
            "witness": witness
        });
        let witness_handle =
            publish_artifact(repository, "paired_root_search_witness", &witness_document)?;
        witness_summaries.push(SearchWitnessSummaryV1 {
            witness_handle: witness_handle.clone(),
            plan_step_count,
            translation: projected_translation,
            final_hp,
            total_potions_consumed,
            satisfies_requested_quality,
        });
    }
    let detail_handle = publish_artifact(repository, "paired_root_search_detail", &document)?;
    Ok(SearchExecutionV1 {
        root_id: root_id.to_owned(),
        action_space_id: action_space_id.to_owned(),
        status: projected_status,
        summary_exit: summary_exit.to_owned(),
        lane_count: u64::try_from(lanes.len()).unwrap_or(u64::MAX),
        bounded_frontier_count: u64::try_from(bounded_frontier.len()).unwrap_or(u64::MAX),
        witnesses: witness_summaries,
        detail_handle,
    })
}

fn required_string<'a>(value: Option<&'a Value>, label: &str) -> Result<&'a str, String> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("paired-root search omitted {label}"))
}

fn required_array<'a>(value: Option<&'a Value>, label: &str) -> Result<&'a Vec<Value>, String> {
    value
        .and_then(Value::as_array)
        .ok_or_else(|| format!("paired-root search omitted {label}"))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn content_id_has_shape(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|tail| tail.strip_prefix('-'))
        .is_some_and(is_sha256)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
