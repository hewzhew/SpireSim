use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::ai::combat_search_v2::{
    turn_plan_action_sequence_key, CombatSearchV2RootActionPrior, CombatSearchV2TurnPlanPrior,
};

#[derive(Debug, Deserialize)]
struct RootActionPriorHintRecordV0 {
    schema_name: Option<String>,
    root_exact_state_hash: Option<String>,
    #[serde(default)]
    action_prior_hints: Vec<RootActionPriorHintV0>,
}

#[derive(Debug, Deserialize)]
struct RootActionPriorHintV0 {
    action_key: String,
    score: f64,
}

#[derive(Debug, Deserialize)]
struct TurnPlanPriorHintRecordV0 {
    schema_name: Option<String>,
    root_exact_state_hash: Option<String>,
    #[serde(default)]
    turn_plan_prior_hints: Vec<TurnPlanPriorHintV0>,
}

#[derive(Debug, Deserialize)]
struct TurnPlanPriorHintV0 {
    action_keys: Vec<String>,
    score: f64,
}

pub fn load_combat_root_action_prior_hints_jsonl_v0(
    path: &Path,
) -> Result<CombatSearchV2RootActionPrior, String> {
    let payload = fs::read_to_string(path).map_err(|err| {
        format!(
            "failed to read root action prior hints from {}: {err}",
            path.display()
        )
    })?;
    parse_combat_root_action_prior_hints_jsonl_v0(&payload)
}

pub fn parse_combat_root_action_prior_hints_jsonl_v0(
    payload: &str,
) -> Result<CombatSearchV2RootActionPrior, String> {
    let mut scores_by_state: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut duplicate_hint_count = 0usize;
    for (line_index, line) in payload.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let record: RootActionPriorHintRecordV0 = serde_json::from_str(line).map_err(|err| {
            format!(
                "invalid CombatRootActionPriorHintV0 JSONL at line {}: {err}",
                line_index + 1
            )
        })?;
        if !matches!(
            record.schema_name.as_deref(),
            Some("CombatRootActionPriorHintV0")
        ) {
            continue;
        }
        let Some(state_hash) = record
            .root_exact_state_hash
            .filter(|state_hash| !state_hash.is_empty())
        else {
            continue;
        };
        let state_scores = scores_by_state.entry(state_hash).or_default();
        for hint in record.action_prior_hints {
            if hint.action_key.is_empty() || !hint.score.is_finite() {
                continue;
            }
            if let Some(existing) = state_scores.get_mut(&hint.action_key) {
                duplicate_hint_count = duplicate_hint_count.saturating_add(1);
                if hint.score > *existing {
                    *existing = hint.score;
                }
            } else {
                state_scores.insert(hint.action_key, hint.score);
            }
        }
    }
    Ok(
        CombatSearchV2RootActionPrior::from_scores_with_duplicate_count(
            scores_by_state,
            duplicate_hint_count,
        ),
    )
}

pub fn load_combat_turn_plan_prior_hints_jsonl_v0(
    path: &Path,
) -> Result<CombatSearchV2TurnPlanPrior, String> {
    let payload = fs::read_to_string(path).map_err(|err| {
        format!(
            "failed to read turn plan prior hints from {}: {err}",
            path.display()
        )
    })?;
    parse_combat_turn_plan_prior_hints_jsonl_v0(&payload)
}

pub fn parse_combat_turn_plan_prior_hints_jsonl_v0(
    payload: &str,
) -> Result<CombatSearchV2TurnPlanPrior, String> {
    let mut scores_by_state: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut duplicate_hint_count = 0usize;
    for (line_index, line) in payload.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let record: TurnPlanPriorHintRecordV0 = serde_json::from_str(line).map_err(|err| {
            format!(
                "invalid CombatTurnPlanPriorHintV0 JSONL at line {}: {err}",
                line_index + 1
            )
        })?;
        if !matches!(
            record.schema_name.as_deref(),
            Some("CombatTurnPlanPriorHintV0")
        ) {
            continue;
        }
        let Some(state_hash) = record
            .root_exact_state_hash
            .filter(|state_hash| !state_hash.is_empty())
        else {
            continue;
        };
        let state_scores = scores_by_state.entry(state_hash).or_default();
        for hint in record.turn_plan_prior_hints {
            if hint.action_keys.is_empty() || !hint.score.is_finite() {
                continue;
            }
            let plan_key = turn_plan_action_sequence_key(&hint.action_keys);
            if let Some(existing) = state_scores.get_mut(&plan_key) {
                duplicate_hint_count = duplicate_hint_count.saturating_add(1);
                if hint.score > *existing {
                    *existing = hint.score;
                }
            } else {
                state_scores.insert(plan_key, hint.score);
            }
        }
    }
    Ok(
        CombatSearchV2TurnPlanPrior::from_scores_with_duplicate_count(
            scores_by_state,
            duplicate_hint_count,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_root_action_prior_hints_by_exact_state_and_action_key() {
        let payload = r#"
{"schema_name":"CombatRootActionPriorHintV0","root_exact_state_hash":"state-a","action_prior_hints":[{"action_key":"combat/end_turn","score":0.25},{"action_key":"combat/play_card/hand:0/card:Bash+0#10/target:monster_slot:0","score":0.75}]}
{"schema_name":"OtherSchema","root_exact_state_hash":"ignored","action_prior_hints":[{"action_key":"combat/end_turn","score":1.0}]}
{"schema_name":"CombatRootActionPriorHintV0","root_exact_state_hash":"state-b","action_prior_hints":[{"action_key":"combat/end_turn","score":0.5}]}
"#;

        let prior = parse_combat_root_action_prior_hints_jsonl_v0(payload)
            .expect("prior hints should parse");

        assert_eq!(prior.score("state-a", "combat/end_turn"), Some(0.25));
        assert_eq!(
            prior.score(
                "state-a",
                "combat/play_card/hand:0/card:Bash+0#10/target:monster_slot:0"
            ),
            Some(0.75)
        );
        assert_eq!(prior.score("state-b", "combat/end_turn"), Some(0.5));
        assert_eq!(prior.score("ignored", "combat/end_turn"), None);
        assert_eq!(prior.score("state-a", "missing"), None);
        assert_eq!(prior.duplicate_hint_count(), 0);
    }

    #[test]
    fn duplicate_state_action_hints_keep_highest_score_and_report_count() {
        let payload = r#"
{"schema_name":"CombatRootActionPriorHintV0","root_exact_state_hash":"state-a","action_prior_hints":[{"action_key":"combat/end_turn","score":0.25}]}
{"schema_name":"CombatRootActionPriorHintV0","root_exact_state_hash":"state-a","action_prior_hints":[{"action_key":"combat/end_turn","score":0.10},{"action_key":"combat/play_card/bash","score":0.50}]}
{"schema_name":"CombatRootActionPriorHintV0","root_exact_state_hash":"state-a","action_prior_hints":[{"action_key":"combat/end_turn","score":0.75}]}
"#;

        let prior = parse_combat_root_action_prior_hints_jsonl_v0(payload)
            .expect("prior hints should parse");

        assert_eq!(prior.score("state-a", "combat/end_turn"), Some(0.75));
        assert_eq!(prior.score("state-a", "combat/play_card/bash"), Some(0.50));
        assert_eq!(prior.duplicate_hint_count(), 2);
    }

    #[test]
    fn parses_turn_plan_prior_hints_by_exact_state_and_plan_actions() {
        let payload = r#"
{"schema_name":"CombatTurnPlanPriorHintV0","root_exact_state_hash":"state-a","turn_plan_prior_hints":[{"action_keys":["combat/play_card/bash","combat/end_turn"],"score":0.75},{"action_keys":["combat/end_turn"],"score":0.25}]}
{"schema_name":"OtherSchema","root_exact_state_hash":"ignored","turn_plan_prior_hints":[{"action_keys":["combat/end_turn"],"score":1.0}]}
{"schema_name":"CombatTurnPlanPriorHintV0","root_exact_state_hash":"state-b","turn_plan_prior_hints":[{"action_keys":["combat/end_turn"],"score":0.5}]}
"#;

        let prior = parse_combat_turn_plan_prior_hints_jsonl_v0(payload)
            .expect("turn plan prior hints should parse");

        assert_eq!(
            prior.score_for_action_keys(
                "state-a",
                &[
                    "combat/play_card/bash".to_string(),
                    "combat/end_turn".to_string()
                ]
            ),
            Some(0.75)
        );
        assert_eq!(
            prior.score_for_action_keys("state-a", &["combat/end_turn".to_string()]),
            Some(0.25)
        );
        assert_eq!(
            prior.score_for_action_keys("state-b", &["combat/end_turn".to_string()]),
            Some(0.5)
        );
        assert_eq!(
            prior.score_for_action_keys("ignored", &["combat/end_turn".to_string()]),
            None
        );
        assert_eq!(prior.duplicate_hint_count(), 0);
    }

    #[test]
    fn duplicate_turn_plan_hints_keep_highest_score_and_report_count() {
        let payload = r#"
{"schema_name":"CombatTurnPlanPriorHintV0","root_exact_state_hash":"state-a","turn_plan_prior_hints":[{"action_keys":["combat/end_turn"],"score":0.25}]}
{"schema_name":"CombatTurnPlanPriorHintV0","root_exact_state_hash":"state-a","turn_plan_prior_hints":[{"action_keys":["combat/end_turn"],"score":0.10},{"action_keys":["combat/play_card/bash"],"score":0.50}]}
{"schema_name":"CombatTurnPlanPriorHintV0","root_exact_state_hash":"state-a","turn_plan_prior_hints":[{"action_keys":["combat/end_turn"],"score":0.75}]}
"#;

        let prior = parse_combat_turn_plan_prior_hints_jsonl_v0(payload)
            .expect("turn plan prior hints should parse");

        assert_eq!(
            prior.score_for_action_keys("state-a", &["combat/end_turn".to_string()]),
            Some(0.75)
        );
        assert_eq!(
            prior.score_for_action_keys("state-a", &["combat/play_card/bash".to_string()]),
            Some(0.50)
        );
        assert_eq!(prior.duplicate_hint_count(), 2);
    }
}
