use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sts_oracle_runtime::content::cards::{CardId, CardType};
use sts_oracle_runtime::sim::combat::CombatTerminal;
use sts_oracle_runtime::state::core::ClientInput;

use super::replay::previous_card_index;
use super::{
    ActionObservation, EvidenceRecord, MonsterObservation, PreviousCardBypassObservation,
    PreviousCardBypassStatus,
};

const QUERY_BATCH_SCHEMA_NAME: &str = "CombatEvidenceQueryBatchV1";
const QUERY_RESULTS_SCHEMA_NAME: &str = "CombatEvidenceQueryResultsV1";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CombatEvidenceQueryBatch {
    schema_name: String,
    schema_version: u32,
    queries: Vec<ActionTransitionQuery>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActionTransitionQuery {
    query_id: String,
    #[serde(default)]
    record: RecordFilter,
    #[serde(default)]
    current: ActionFilter,
    #[serde(default)]
    previous_card_same_turn: Option<ActionFilter>,
    #[serde(default)]
    bypass_previous_card: Option<BypassFilter>,
    #[serde(default = "default_max_matches")]
    max_matches: usize,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecordFilter {
    replay_exact: Option<bool>,
    final_terminal: Option<CombatTerminal>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActionFilter {
    card_id: Option<CardId>,
    card_type: Option<CardType>,
    terminal_after: Option<CombatTerminal>,
    turn: Option<IntConstraint>,
    /// Monster transition selected by the current query action's target.
    query_target: Option<MonsterTransitionFilter>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MonsterTransitionFilter {
    before: Option<MonsterFilter>,
    after: Option<MonsterFilter>,
    hp_delta: Option<IntConstraint>,
    block_delta: Option<IntConstraint>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MonsterFilter {
    hp: Option<IntConstraint>,
    block: Option<IntConstraint>,
    terminal_like: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct IntConstraint {
    eq: Option<i64>,
    gt: Option<i64>,
    ge: Option<i64>,
    lt: Option<i64>,
    le: Option<i64>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BypassFilter {
    status: Option<PreviousCardBypassStatus>,
    terminal_after: Option<CombatTerminal>,
    query_target_after: Option<MonsterFilter>,
}

#[derive(Debug, Serialize)]
pub(super) struct CombatEvidenceQueryResults {
    schema_name: &'static str,
    schema_version: u32,
    source_evidence_schemas: BTreeSet<String>,
    query_count: usize,
    results: Vec<ActionTransitionQueryResult>,
}

#[derive(Debug, Serialize)]
pub(super) struct CombatEvidenceQueryBatchSummary {
    query_count: usize,
    queries: Vec<ActionTransitionQuerySummary>,
}

#[derive(Debug, Serialize)]
struct ActionTransitionQuerySummary {
    query_id: String,
    matched_action_count: usize,
    independent_root_count: usize,
    returned_match_count: usize,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct ActionTransitionQueryResult {
    query_id: String,
    matched_action_count: usize,
    independent_root_count: usize,
    returned_match_count: usize,
    truncated: bool,
    matches: Vec<ActionTransitionMatch>,
}

#[derive(Debug, Serialize)]
struct ActionTransitionMatch {
    record_id: String,
    root_exact_state_hash: String,
    action_index: usize,
    current: ActionProjection,
    previous_card_same_turn: Option<ActionProjection>,
    bypass_previous_card: Option<BypassProjection>,
}

#[derive(Debug, Serialize)]
struct ActionProjection {
    index: usize,
    input: ClientInput,
    card_id: Option<CardId>,
    card_type: Option<CardType>,
    turn: u32,
    terminal_after: CombatTerminal,
    query_target_before: Option<MonsterObservation>,
    query_target_after: Option<MonsterObservation>,
}

#[derive(Debug, Serialize)]
struct BypassProjection {
    previous_action_index: Option<usize>,
    status: PreviousCardBypassStatus,
    terminal_after: Option<CombatTerminal>,
    query_target_after: Option<MonsterObservation>,
}

pub(super) fn read_query_batch(path: &Path) -> Result<CombatEvidenceQueryBatch, String> {
    let bytes = if path == Path::new("-") {
        let mut bytes = Vec::new();
        io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|error| format!("cannot read query batch from stdin: {error}"))?;
        bytes
    } else {
        fs::read(path)
            .map_err(|error| format!("cannot read query batch '{}': {error}", path.display()))?
    };
    let batch: CombatEvidenceQueryBatch = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "cannot decode {} query batch '{}': {error}",
            QUERY_BATCH_SCHEMA_NAME,
            path.display()
        )
    })?;
    validate_query_batch(&batch)?;
    Ok(batch)
}

pub(super) fn execute_query_batch(
    batch: &CombatEvidenceQueryBatch,
    records: &[EvidenceRecord],
) -> Result<CombatEvidenceQueryResults, String> {
    validate_query_batch(batch)?;
    let mut results = Vec::with_capacity(batch.queries.len());
    for query in &batch.queries {
        results.push(execute_query(query, records));
    }
    Ok(CombatEvidenceQueryResults {
        schema_name: QUERY_RESULTS_SCHEMA_NAME,
        schema_version: 1,
        source_evidence_schemas: records
            .iter()
            .map(|record| format!("{}@{}", record.schema_name, record.schema_version))
            .collect(),
        query_count: results.len(),
        results,
    })
}

impl CombatEvidenceQueryResults {
    pub(super) fn summary(&self) -> CombatEvidenceQueryBatchSummary {
        CombatEvidenceQueryBatchSummary {
            query_count: self.query_count,
            queries: self
                .results
                .iter()
                .map(|result| ActionTransitionQuerySummary {
                    query_id: result.query_id.clone(),
                    matched_action_count: result.matched_action_count,
                    independent_root_count: result.independent_root_count,
                    returned_match_count: result.returned_match_count,
                    truncated: result.truncated,
                })
                .collect(),
        }
    }
}

fn validate_query_batch(batch: &CombatEvidenceQueryBatch) -> Result<(), String> {
    if batch.schema_name != QUERY_BATCH_SCHEMA_NAME || batch.schema_version != 1 {
        return Err(format!(
            "expected {QUERY_BATCH_SCHEMA_NAME} schema_version 1, got {} schema_version {}",
            batch.schema_name, batch.schema_version
        ));
    }
    if batch.queries.is_empty() {
        return Err("query batch must contain at least one query".to_string());
    }
    if batch.queries.len() > 128 {
        return Err("query batch exceeds the 128-query bound".to_string());
    }
    let mut ids = BTreeSet::new();
    for query in &batch.queries {
        if query.query_id.trim().is_empty() {
            return Err("query_id must not be empty".to_string());
        }
        if !ids.insert(query.query_id.as_str()) {
            return Err(format!("duplicate query_id: {}", query.query_id));
        }
        if !(1..=1024).contains(&query.max_matches) {
            return Err(format!(
                "query '{}' max_matches must be between 1 and 1024",
                query.query_id
            ));
        }
    }
    Ok(())
}

fn execute_query(
    query: &ActionTransitionQuery,
    records: &[EvidenceRecord],
) -> ActionTransitionQueryResult {
    let mut matched_action_count = 0usize;
    let mut roots = BTreeSet::new();
    let mut matches = Vec::new();
    for record in records {
        if !record_matches(&query.record, record) {
            continue;
        }
        for (position, current) in record.actions.iter().enumerate() {
            let query_target = action_target(current);
            if !action_matches(&query.current, current, query_target) {
                continue;
            }
            let previous_index = previous_card_index(&record.actions, position);
            let previous = previous_index.and_then(|index| record.actions.get(index));
            if query
                .previous_card_same_turn
                .as_ref()
                .is_some_and(|filter| {
                    !previous.is_some_and(|action| action_matches(filter, action, query_target))
                })
            {
                continue;
            }
            if query.bypass_previous_card.as_ref().is_some_and(|filter| {
                !current
                    .previous_card_bypass
                    .as_ref()
                    .is_some_and(|bypass| bypass_matches(filter, bypass, query_target))
            }) {
                continue;
            }

            matched_action_count = matched_action_count.saturating_add(1);
            roots.insert(record.root_exact_state_hash.clone());
            if matches.len() < query.max_matches {
                matches.push(ActionTransitionMatch {
                    record_id: record.record_id.clone(),
                    root_exact_state_hash: record.root_exact_state_hash.clone(),
                    action_index: current.index,
                    current: project_action(current, query_target),
                    previous_card_same_turn: previous
                        .map(|action| project_action(action, query_target)),
                    bypass_previous_card: current
                        .previous_card_bypass
                        .as_ref()
                        .map(|bypass| project_bypass(bypass, query_target)),
                });
            }
        }
    }
    ActionTransitionQueryResult {
        query_id: query.query_id.clone(),
        matched_action_count,
        independent_root_count: roots.len(),
        returned_match_count: matches.len(),
        truncated: matches.len() < matched_action_count,
        matches,
    }
}

fn record_matches(filter: &RecordFilter, record: &EvidenceRecord) -> bool {
    filter
        .replay_exact
        .is_none_or(|expected| record.replay_exact == expected)
        && filter
            .final_terminal
            .is_none_or(|expected| record.final_terminal == expected)
}

fn action_matches(
    filter: &ActionFilter,
    action: &ActionObservation,
    query_target: Option<usize>,
) -> bool {
    filter
        .card_id
        .is_none_or(|expected| action.card.as_ref().map(|card| card.id) == Some(expected))
        && filter
            .card_type
            .is_none_or(|expected| action.card_type == Some(expected))
        && filter
            .terminal_after
            .is_none_or(|expected| action.terminal_after == expected)
        && filter
            .turn
            .as_ref()
            .is_none_or(|constraint| constraint.matches(i64::from(action.before.turn)))
        && filter.query_target.as_ref().is_none_or(|target_filter| {
            let Some(target) = query_target else {
                return false;
            };
            monster_transition_matches(
                target_filter,
                action.before.monster(target),
                action.after.monster(target),
            )
        })
}

fn bypass_matches(
    filter: &BypassFilter,
    bypass: &PreviousCardBypassObservation,
    query_target: Option<usize>,
) -> bool {
    filter
        .status
        .is_none_or(|expected| bypass.status == expected)
        && filter
            .terminal_after
            .is_none_or(|expected| bypass.terminal_after == Some(expected))
        && filter
            .query_target_after
            .as_ref()
            .is_none_or(|monster_filter| {
                let Some(target) = query_target else {
                    return false;
                };
                bypass
                    .target_after(target)
                    .is_some_and(|monster| monster_matches(monster_filter, monster))
            })
}

fn monster_transition_matches(
    filter: &MonsterTransitionFilter,
    before: Option<&MonsterObservation>,
    after: Option<&MonsterObservation>,
) -> bool {
    filter.before.as_ref().is_none_or(|monster_filter| {
        before.is_some_and(|monster| monster_matches(monster_filter, monster))
    }) && filter.after.as_ref().is_none_or(|monster_filter| {
        after.is_some_and(|monster| monster_matches(monster_filter, monster))
    }) && filter.hp_delta.as_ref().is_none_or(|constraint| {
        before
            .zip(after)
            .is_some_and(|(before, after)| constraint.matches(i64::from(after.hp - before.hp)))
    }) && filter.block_delta.as_ref().is_none_or(|constraint| {
        before.zip(after).is_some_and(|(before, after)| {
            constraint.matches(i64::from(after.block - before.block))
        })
    })
}

fn monster_matches(filter: &MonsterFilter, monster: &MonsterObservation) -> bool {
    filter
        .hp
        .as_ref()
        .is_none_or(|constraint| constraint.matches(i64::from(monster.hp)))
        && filter
            .block
            .as_ref()
            .is_none_or(|constraint| constraint.matches(i64::from(monster.block)))
        && filter
            .terminal_like
            .is_none_or(|expected| monster.terminal_like() == expected)
}

impl IntConstraint {
    fn matches(&self, value: i64) -> bool {
        self.eq.is_none_or(|expected| value == expected)
            && self.gt.is_none_or(|expected| value > expected)
            && self.ge.is_none_or(|expected| value >= expected)
            && self.lt.is_none_or(|expected| value < expected)
            && self.le.is_none_or(|expected| value <= expected)
    }
}

fn action_target(action: &ActionObservation) -> Option<usize> {
    match action.input {
        ClientInput::PlayCard { target, .. } => target,
        _ => None,
    }
}

fn project_action(action: &ActionObservation, query_target: Option<usize>) -> ActionProjection {
    ActionProjection {
        index: action.index,
        input: action.input.clone(),
        card_id: action.card.as_ref().map(|card| card.id),
        card_type: action.card_type,
        turn: action.before.turn,
        terminal_after: action.terminal_after,
        query_target_before: query_target
            .and_then(|target| action.before.monster(target))
            .cloned(),
        query_target_after: query_target
            .and_then(|target| action.after.monster(target))
            .cloned(),
    }
}

fn project_bypass(
    bypass: &PreviousCardBypassObservation,
    query_target: Option<usize>,
) -> BypassProjection {
    BypassProjection {
        previous_action_index: bypass.previous_action_index,
        status: bypass.status,
        terminal_after: bypass.terminal_after,
        query_target_after: query_target
            .and_then(|target| bypass.target_after(target))
            .cloned(),
    }
}

const fn default_max_matches() -> usize {
    64
}
