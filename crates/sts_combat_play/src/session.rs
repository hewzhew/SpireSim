use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sts_core::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_core::sim::combat::{
    combat_terminal, CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
};
use sts_core::sim::combat_case_contract::COMBAT_CASE_SCHEMA;
use sts_core::state::core::{ClientInput, EngineState};

pub const SESSION_SCHEMA: &str = "exact_combat_play_session_v1";
pub const MAX_ENGINE_STEPS: usize = 250;
pub const INTERACTIVE_MAX_INNER_NODES: usize = 256;
pub const INTERACTIVE_MAX_PARTIAL_FRONTIER_STATES: usize = 32;
pub const INTERACTIVE_MAX_END_STATES: usize = 24;

#[derive(Clone, Debug, Deserialize)]
struct CombatCaseRoot {
    schema: String,
    position: CombatPosition,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PlaySessionV1 {
    pub schema: String,
    pub case: PathBuf,
    pub root_exact_state_hash: String,
    pub allow_potions: bool,
    #[serde(default)]
    pub proposal_limits: PlayProposalLimitsV1,
    pub turns: Vec<ChosenTurnV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PlayProposalLimitsV1 {
    pub max_inner_nodes: usize,
    pub max_partial_frontier_states: usize,
    pub max_end_states: usize,
    pub per_bucket_limit: usize,
}

impl Default for PlayProposalLimitsV1 {
    fn default() -> Self {
        Self {
            max_inner_nodes: INTERACTIVE_MAX_INNER_NODES,
            max_partial_frontier_states: INTERACTIVE_MAX_PARTIAL_FRONTIER_STATES,
            max_end_states: INTERACTIVE_MAX_END_STATES,
            per_bucket_limit: INTERACTIVE_MAX_END_STATES,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChosenTurnV1 {
    pub from_turn: u32,
    pub plan_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub actions: Vec<ClientInput>,
    pub end_exact_state_hash: String,
}

pub fn replay_session(session: &PlaySessionV1) -> Result<CombatPosition, String> {
    replay_session_prefix(session, session.turns.len())
}

pub fn replay_session_prefix(
    session: &PlaySessionV1,
    retained_turn_count: usize,
) -> Result<CombatPosition, String> {
    if retained_turn_count > session.turns.len() {
        return Err(format!(
            "cannot replay {retained_turn_count} turns from a {}-turn session",
            session.turns.len()
        ));
    }
    let root = load_case_root(&session.case)?;
    let root_hash = exact_hash(&root);
    if root_hash != session.root_exact_state_hash {
        return Err(format!(
            "combat case root changed: session={}, current={root_hash}",
            session.root_exact_state_hash
        ));
    }
    let stepper = EngineCombatStepper;
    let mut position = root;
    for (index, turn) in session.turns.iter().take(retained_turn_count).enumerate() {
        if position.combat.turn.turn_count != turn.from_turn {
            return Err(format!(
                "session turn {index} drifted before replay: expected turn {}, reached {}",
                turn.from_turn, position.combat.turn.turn_count
            ));
        }
        for input in &turn.actions {
            position = apply_exact(&stepper, &position, input.clone())?;
        }
        let actual = exact_hash(&position);
        if actual != turn.end_exact_state_hash {
            return Err(format!(
                "session turn {index} exact successor drifted: expected={}, reached={actual}",
                turn.end_exact_state_hash
            ));
        }
    }
    Ok(position)
}

pub fn apply_exact(
    stepper: &EngineCombatStepper,
    position: &CombatPosition,
    input: ClientInput,
) -> Result<CombatPosition, String> {
    if !stepper.is_legal_action(position, &input)
        && stepper.choice_for_legal_input(position, &input).is_none()
    {
        return Err(format!("recorded action is no longer legal: {input:?}"));
    }
    let result = stepper.apply_to_stable(
        position,
        input,
        CombatStepLimits {
            max_engine_steps: MAX_ENGINE_STEPS,
            deadline: None,
        },
    );
    if result.truncated || result.timed_out {
        return Err("recorded exact transition exceeded its finite step limit".to_owned());
    }
    Ok(result.position)
}

pub fn load_case_root(path: &Path) -> Result<CombatPosition, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read combat case '{}': {error}", path.display()))?;
    let case: CombatCaseRoot = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to decode combat case '{}': {error}", path.display()))?;
    if case.schema != COMBAT_CASE_SCHEMA {
        return Err(format!(
            "expected {COMBAT_CASE_SCHEMA} at '{}', got '{}'",
            path.display(),
            case.schema
        ));
    }
    Ok(case.position)
}

pub fn load_session(path: &Path) -> Result<PlaySessionV1, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read play session '{}': {error}", path.display()))?;
    let session: PlaySessionV1 = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "failed to decode play session '{}': {error}",
            path.display()
        )
    })?;
    if session.schema != SESSION_SCHEMA {
        return Err(format!(
            "expected {SESSION_SCHEMA} at '{}', got '{}'",
            path.display(),
            session.schema
        ));
    }
    validate_proposal_limits(session.proposal_limits)?;
    Ok(session)
}

pub fn save_session(path: &Path, session: &PlaySessionV1) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create play-session directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let payload = serde_json::to_vec_pretty(session)
        .map_err(|error| format!("failed to encode play session: {error}"))?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, payload).map_err(|error| {
        format!(
            "failed to write play session '{}': {error}",
            temporary.display()
        )
    })?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| {
            format!(
                "failed to replace play session '{}': {error}",
                path.display()
            )
        })?;
    }
    fs::rename(&temporary, path).map_err(|error| {
        format!(
            "failed to publish play session '{}': {error}",
            path.display()
        )
    })
}

pub fn ensure_player_turn(position: &CombatPosition) -> Result<(), String> {
    if matches!(position.engine, EngineState::CombatPlayerTurn) {
        Ok(())
    } else {
        Err(format!(
            "exact position is not a player-turn boundary: engine={:?}, terminal={:?}",
            position.engine,
            combat_terminal(&position.engine, &position.combat)
        ))
    }
}

pub fn validate_proposal_limits(limits: PlayProposalLimitsV1) -> Result<(), String> {
    for (name, value) in [
        ("max_inner_nodes", limits.max_inner_nodes),
        (
            "max_partial_frontier_states",
            limits.max_partial_frontier_states,
        ),
        ("max_end_states", limits.max_end_states),
        ("per_bucket_limit", limits.per_bucket_limit),
    ] {
        if value == 0 {
            return Err(format!("play proposal limit {name} must be positive"));
        }
    }
    Ok(())
}

pub fn exact_hash(position: &CombatPosition) -> String {
    combat_exact_state_hash_v2(&position.engine, &position.combat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fresh_test_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sts-combat-play-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn session_round_trip_preserves_exact_root_identity() {
        let directory = fresh_test_directory("round-trip");
        fs::create_dir(&directory).expect("fresh test directory");
        let case_path = directory.join("root.case.json");
        let session_path = directory.join("play.session.json");
        let root = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            sts_core::test_support::blank_test_combat(),
        );
        fs::write(
            &case_path,
            serde_json::to_vec(&serde_json::json!({
                "schema": COMBAT_CASE_SCHEMA,
                "position": root,
            }))
            .expect("encode combat case"),
        )
        .expect("write combat case");
        let session = PlaySessionV1 {
            schema: SESSION_SCHEMA.to_owned(),
            case: case_path,
            root_exact_state_hash: exact_hash(&root),
            allow_potions: false,
            proposal_limits: PlayProposalLimitsV1::default(),
            turns: Vec::new(),
        };

        save_session(&session_path, &session).expect("save session");
        let loaded = load_session(&session_path).expect("load session");
        let replayed = replay_session(&loaded).expect("replay exact root");

        assert_eq!(exact_hash(&replayed), exact_hash(&root));
        fs::remove_dir_all(directory).expect("remove isolated test directory");
    }

    #[test]
    fn replay_rejects_changed_case_identity_before_actions() {
        let directory = fresh_test_directory("identity-mismatch");
        fs::create_dir(&directory).expect("fresh test directory");
        let case_path = directory.join("root.case.json");
        let root = CombatPosition::new(
            EngineState::CombatPlayerTurn,
            sts_core::test_support::blank_test_combat(),
        );
        fs::write(
            &case_path,
            serde_json::to_vec(&serde_json::json!({
                "schema": COMBAT_CASE_SCHEMA,
                "position": root,
            }))
            .expect("encode combat case"),
        )
        .expect("write combat case");
        let session = PlaySessionV1 {
            schema: SESSION_SCHEMA.to_owned(),
            case: case_path,
            root_exact_state_hash: "not-the-root".to_owned(),
            allow_potions: false,
            proposal_limits: PlayProposalLimitsV1::default(),
            turns: Vec::new(),
        };

        let error = replay_session(&session).expect_err("identity mismatch must fail closed");

        assert!(error.contains("combat case root changed"));
        fs::remove_dir_all(directory).expect("remove isolated test directory");
    }
}
