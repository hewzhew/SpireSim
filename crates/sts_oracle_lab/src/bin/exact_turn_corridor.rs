use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2;
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::eval::combat_guidance_bundle::typed_combat_value_features_v1;
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_oracle_runtime::state::core::ClientInput;

const CORPUS_SCHEMA_NAME: &str = "CombatActionImitationCorpusManifestV1";
const CORPUS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusManifestV1 {
    schema_name: String,
    schema_version: u32,
    demonstrations: Vec<CorpusEntryV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusEntryV1 {
    id: String,
    case: PathBuf,
    actions: Vec<PathBuf>,
}

pub(super) struct LoadedDemonstrationV1 {
    pub(super) id: String,
    pub(super) case_path: PathBuf,
    pub(super) action_paths: Vec<PathBuf>,
    pub(super) position: CombatPosition,
    pub(super) actions: Vec<ClientInput>,
}

#[derive(Clone, Debug)]
pub(super) struct ExactTurnCorridor {
    pub(super) rank_by_exact_hash: HashMap<String, i32>,
    pub(super) typed_target_by_turn: HashMap<u32, (i32, Vec<i32>)>,
    pub(super) positions_by_rank: Vec<CombatPosition>,
    pub(super) transition_actions: Vec<Vec<ClientInput>>,
    pub(super) action_count: usize,
    pub(super) terminal_final_hp: i32,
}

pub(super) fn load(
    case_path: &Path,
    action_paths: &[PathBuf],
    max_engine_steps_per_transition: usize,
) -> Result<ExactTurnCorridor, String> {
    let case = load_combat_case(case_path)?;
    let actions = load_action_segments(action_paths)?;
    from_position_and_actions(case.core.position, actions, max_engine_steps_per_transition)
}

pub(super) fn from_position_and_actions(
    mut position: CombatPosition,
    actions: Vec<ClientInput>,
    max_engine_steps_per_transition: usize,
) -> Result<ExactTurnCorridor, String> {
    let stepper = EngineCombatStepper;
    let mut rank_by_exact_hash = HashMap::new();
    let mut typed_target_by_turn = HashMap::new();
    let initial_exact_hash = combat_exact_state_hash_v2(&position.engine, &position.combat);
    rank_by_exact_hash.insert(initial_exact_hash, 0);
    typed_target_by_turn.insert(
        position.combat.turn.turn_count,
        (0, typed_feature_components(&position)),
    );
    let mut next_turn_rank = 1i32;
    let mut positions_by_rank = vec![position.clone()];
    let mut transition_actions = Vec::new();
    let mut current_transition_actions = Vec::new();
    for (action_index, input) in actions.iter().enumerate() {
        if stepper.choice_for_legal_input(&position, input).is_none() {
            return Err(format!(
                "shadow corridor action {action_index} is not legal at turn {}: {input:?}",
                position.combat.turn.turn_count
            ));
        }
        let previous_turn = position.combat.turn.turn_count;
        current_transition_actions.push(input.clone());
        let step = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated {
            return Err(format!(
                "shadow corridor action {action_index} exceeded the engine-step limit"
            ));
        }
        position = step.position;
        if step.terminal == CombatTerminal::Unresolved
            && position.combat.turn.turn_count != previous_turn
        {
            transition_actions.push(std::mem::take(&mut current_transition_actions));
            positions_by_rank.push(position.clone());
            rank_by_exact_hash.insert(
                combat_exact_state_hash_v2(&position.engine, &position.combat),
                next_turn_rank,
            );
            typed_target_by_turn.insert(
                position.combat.turn.turn_count,
                (next_turn_rank, typed_feature_components(&position)),
            );
            next_turn_rank = next_turn_rank.saturating_add(1);
        }
    }
    if stepper.terminal(&position) != CombatTerminal::Win {
        return Err("shadow corridor action list is not an exact terminal win".to_string());
    }
    if !current_transition_actions.is_empty() {
        transition_actions.push(current_transition_actions);
    }
    if transition_actions.len() != positions_by_rank.len() {
        return Err(format!(
            "verified corridor has {} boundaries but {} outgoing turn segments",
            positions_by_rank.len(),
            transition_actions.len()
        ));
    }
    Ok(ExactTurnCorridor {
        rank_by_exact_hash,
        typed_target_by_turn,
        positions_by_rank,
        transition_actions,
        action_count: actions.len(),
        terminal_final_hp: position.combat.entities.player.current_hp,
    })
}

pub(super) fn load_action_segments(action_paths: &[PathBuf]) -> Result<Vec<ClientInput>, String> {
    let mut actions = Vec::new();
    for path in action_paths {
        let mut segment = serde_json::from_slice::<Vec<ClientInput>>(
            &std::fs::read(path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("invalid combat action segment {}: {error}", path.display()))?;
        actions.append(&mut segment);
    }
    Ok(actions)
}

pub(super) fn typed_feature_components(position: &CombatPosition) -> Vec<i32> {
    typed_combat_value_features_v1(position)
}

pub(super) fn load_corpus(manifest_path: &Path) -> Result<Vec<LoadedDemonstrationV1>, String> {
    let manifest = serde_json::from_slice::<CorpusManifestV1>(
        &std::fs::read(manifest_path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("invalid action imitation corpus manifest: {error}"))?;
    if manifest.schema_name != CORPUS_SCHEMA_NAME
        || manifest.schema_version != CORPUS_SCHEMA_VERSION
    {
        return Err("unsupported action imitation corpus manifest schema".to_string());
    }
    if manifest.demonstrations.is_empty() {
        return Err("action imitation corpus manifest has no demonstrations".to_string());
    }
    let base = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut seen_ids = HashSet::new();
    manifest
        .demonstrations
        .into_iter()
        .map(|entry| {
            if entry.id.trim().is_empty() || !seen_ids.insert(entry.id.clone()) {
                return Err(format!(
                    "action imitation corpus demonstration id is empty or duplicated: {:?}",
                    entry.id
                ));
            }
            if entry.actions.is_empty() {
                return Err(format!(
                    "action imitation corpus demonstration {:?} has no action segments",
                    entry.id
                ));
            }
            let case_path = resolve_manifest_path(base, &entry.case);
            let action_paths = entry
                .actions
                .iter()
                .map(|path| resolve_manifest_path(base, path))
                .collect::<Vec<_>>();
            let case = load_combat_case(&case_path)?;
            let actions = load_action_segments(&action_paths)?;
            Ok(LoadedDemonstrationV1 {
                id: entry.id,
                case_path,
                action_paths,
                position: case.core.position,
                actions,
            })
        })
        .collect()
}

fn resolve_manifest_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}
