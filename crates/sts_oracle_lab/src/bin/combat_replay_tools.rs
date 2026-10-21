//! Exact descendant replay, readable path reconstruction, and case export tools.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sts_combat_planner::{
    LocalTurnGraphStateSnapshot, LocalTurnGraphWitnessSession, TurnOptionAction,
};
use sts_oracle_runtime::eval::combat_case::{save_combat_case, CombatCase};
use sts_oracle_runtime::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
};
use sts_oracle_runtime::state::core::ClientInput;

use super::combat_trace_view::{combat_action_label, combat_turn_snapshot};

pub(super) fn save_combat_inputs(
    output: &Path,
    inputs: impl IntoIterator<Item = ClientInput>,
) -> Result<(), String> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let inputs = inputs.into_iter().collect::<Vec<_>>();
    let bytes = serde_json::to_vec_pretty(&inputs).map_err(|error| error.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())
}

pub(super) fn replay_combat_inputs(
    mut position: CombatPosition,
    inputs: &[ClientInput],
    max_engine_steps_per_transition: usize,
) -> Result<CombatPosition, String> {
    for (index, input) in inputs.iter().enumerate() {
        if !EngineCombatStepper.is_legal_action(&position, input) {
            return Err(format!("combat replay action {index} is not legal"));
        }
        let step = EngineCombatStepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(format!(
                "combat replay action {index} did not reach a stable state"
            ));
        }
        position = step.position;
    }
    Ok(position)
}

fn combat_policy_surface(
    position: &sts_oracle_runtime::sim::combat::CombatPosition,
    limit: usize,
) -> Value {
    const UNIFORM_EXPLORATION: f64 = 0.05;

    let stepper = EngineCombatStepper;
    let actions = stepper.atomic_actions(position);
    let weights =
        sts_oracle_runtime::ai::combat_search_v2::oracle_action_policy::oracle_atomic_action_policy_weights(
            position,
            &actions,
        );
    let total = weights.iter().sum::<f64>();
    let uniform = 1.0 / actions.len().max(1) as f64;
    let mut ranked = actions
        .iter()
        .zip(&weights)
        .enumerate()
        .map(|(surface_index, (input, weight))| {
            let ordinal_rank = 1 + weights
                .iter()
                .filter(|candidate| **candidate > *weight)
                .count();
            let probability = if total > 0.0 {
                ((1.0 - UNIFORM_EXPLORATION) * (*weight / total) + UNIFORM_EXPLORATION * uniform)
                    .max(f64::MIN_POSITIVE)
            } else {
                uniform
            };
            (
                *weight,
                surface_index,
                json!({
                    "rank": ordinal_rank,
                    "surface_index": surface_index,
                    "action": combat_action_label(position, input),
                    "weight": weight,
                    "probability": probability,
                }),
            )
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
    });
    let shown = ranked.len().min(limit);
    json!({
        "action_count": ranked.len(),
        "shown": shown,
        "truncated": ranked.len() > shown,
        "actions": ranked
            .into_iter()
            .take(limit)
            .map(|(_, _, value)| value)
            .collect::<Vec<_>>(),
    })
}

pub(super) fn export_descendant_combat_case(
    base: &CombatCase,
    actions: &[TurnOptionAction],
    output: &Path,
    max_engine_steps_per_transition: usize,
    reason: &str,
) -> Result<PathBuf, String> {
    let position = replay_descendant_position(
        base.core.position.clone(),
        actions,
        max_engine_steps_per_transition,
    )?;

    let mut exported = base.clone();
    exported.core.position = position;
    exported.refresh_derived_summaries_and_clear_production_context();
    exported.core.gap.boundary = format!(
        "{} + {} exact descendant actions",
        exported.core.gap.boundary,
        actions.len()
    );
    exported.core.gap.reason = reason.to_string();
    exported.atomic_combat_search_attempts.clear();
    exported.failed_atomic_combat_search = None;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    save_combat_case(output, &exported)?;

    let stem = output
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("deepest");
    let action_output = output.with_file_name(format!("{stem}.prefix.actions.json"));
    save_combat_inputs(
        &action_output,
        actions.iter().map(|action| action.input.clone()),
    )?;
    Ok(action_output)
}

pub(super) fn local_graph_state_snapshot_for_path(
    session: &LocalTurnGraphWitnessSession,
    root: sts_oracle_runtime::sim::combat::CombatPosition,
    actions: &[TurnOptionAction],
    max_engine_steps_per_transition: usize,
) -> Result<Option<LocalTurnGraphStateSnapshot>, String> {
    let position = replay_descendant_position(root, actions, max_engine_steps_per_transition)?;
    let exact_state_hash = sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
        &position.engine,
        &position.combat,
    );
    Ok(session.state_snapshot_by_exact_hash(&exact_state_hash))
}

fn replay_descendant_position(
    mut position: sts_oracle_runtime::sim::combat::CombatPosition,
    actions: &[TurnOptionAction],
    max_engine_steps_per_transition: usize,
) -> Result<sts_oracle_runtime::sim::combat::CombatPosition, String> {
    let stepper = EngineCombatStepper;
    for (index, action) in actions.iter().enumerate() {
        if stepper
            .choice_for_legal_input(&position, &action.input)
            .is_none()
        {
            return Err(format!(
                "deepest-case action {index} is not legal at turn {}: {:?}",
                position.combat.turn.turn_count, action.input
            ));
        }
        let result = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if result.truncated {
            return Err(format!(
                "deepest-case action {index} exceeded {max_engine_steps_per_transition} engine steps"
            ));
        }
        position = result.position;
    }
    Ok(position)
}

pub(super) fn replay_combat_path(
    mut position: sts_oracle_runtime::sim::combat::CombatPosition,
    actions: &[TurnOptionAction],
    max_engine_steps_per_transition: usize,
) -> Result<Value, String> {
    let stepper = EngineCombatStepper;
    let mut turns = Vec::new();
    let mut turn_number = position.combat.turn.turn_count;
    let mut turn_start_hp = position.combat.entities.player.current_hp;
    let mut turn_start_policy = combat_policy_surface(&position, 12);
    let mut turn_start_action_index = 1usize;
    let mut turn_actions = Vec::new();
    let mut terminal = stepper.terminal(&position);

    for (index, action) in actions.iter().enumerate() {
        let action_key = combat_action_label(&position, &action.input);
        if stepper
            .choice_for_legal_input(&position, &action.input)
            .is_none()
        {
            return Err(format!(
                "diagnostic path action {index} is not legal at turn {}: {action_key}",
                position.combat.turn.turn_count
            ));
        }
        let result = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if result.truncated {
            return Err(format!(
                "diagnostic path action {index} exceeded {max_engine_steps_per_transition} engine steps: {action_key}"
            ));
        }
        turn_actions.push(action_key);
        position = result.position;
        terminal = result.terminal;
        let next_turn = position.combat.turn.turn_count;
        if next_turn != turn_number
            || !matches!(
                terminal,
                sts_oracle_runtime::sim::combat::CombatTerminal::Unresolved
            )
        {
            turns.push(json!({
                "turn": turn_number,
                "action_range": {
                    "first": turn_start_action_index,
                    "last": index + 1,
                },
                "start_hp": turn_start_hp,
                "start_policy": turn_start_policy,
                "actions": turn_actions,
                "end": combat_turn_snapshot(&position),
                "terminal": format!("{terminal:?}"),
            }));
            turn_number = next_turn;
            turn_start_hp = position.combat.entities.player.current_hp;
            turn_start_policy = combat_policy_surface(&position, 12);
            turn_start_action_index = index + 2;
            turn_actions = Vec::new();
        }
    }
    if !turn_actions.is_empty() {
        turns.push(json!({
            "turn": turn_number,
            "action_range": {
                "first": turn_start_action_index,
                "last": actions.len(),
            },
            "start_hp": turn_start_hp,
            "start_policy": turn_start_policy,
            "actions": turn_actions,
            "end": combat_turn_snapshot(&position),
            "terminal": format!("{terminal:?}"),
            "partial": true,
        }));
    }

    Ok(json!({
        "action_count": actions.len(),
        "turns": turns,
        "terminal": format!("{terminal:?}"),
    }))
}

#[cfg(test)]
mod tests {
    use super::save_combat_inputs;

    #[test]
    fn combat_input_artifact_owner_creates_parent_and_writes_json_array() {
        let root = std::env::temp_dir().join(format!(
            "sts-oracle-lab-combat-inputs-{}",
            std::process::id()
        ));
        let output = root.join("nested").join("actions.json");
        let _ = std::fs::remove_dir_all(&root);

        save_combat_inputs(
            &output,
            std::iter::empty::<sts_oracle_runtime::state::core::ClientInput>(),
        )
        .expect("save empty combat input artifact");

        let text = std::fs::read_to_string(&output).expect("read combat input artifact");
        assert_eq!(text.trim(), "[]");
        assert!(
            save_combat_inputs(
                &output,
                std::iter::empty::<sts_oracle_runtime::state::core::ClientInput>(),
            )
            .is_err(),
            "combat input exports must not overwrite an existing artifact"
        );
        std::fs::remove_dir_all(root).expect("remove combat input artifact fixture");
    }
}
