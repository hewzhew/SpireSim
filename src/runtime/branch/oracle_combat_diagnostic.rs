use serde_json::{json, Value};
use sts_combat_planner::TurnOptionAction;

use super::oracle_analysis_session::OracleAnalysisCombatProgressV1;
use crate::content::identity::gameplay_display_label_v1;
use crate::runtime::combat::{CombatCard, CombatState, MonsterEntity};
use crate::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper};
use crate::sim::combat_action::{combat_action_key, target_label};
use crate::state::core::ClientInput;

use super::OracleAnalysisWorkspaceV1;

/// Reconstructs the strategic facts captured when a verified run session
/// opens an exact combat. This is a read-only projection from typed state; it
/// does not consult the retired strategic owner or rank any action.
pub fn reconstruct_oracle_combat_context_trace_v1(
    session: &crate::eval::run_control::RunControlSession,
) -> Result<crate::eval::run_control::AtomicCombatSearchTraceSummaryV2, String> {
    let active_combat = session
        .active_combat
        .as_ref()
        .ok_or_else(|| "oracle combat context reconstruction requires active combat".to_string())?;
    let potion_continuation_context =
        crate::ai::potion_continuation_context_v1::potion_run_continuation_context_v1(
            &session.run_state,
            &active_combat.combat_state,
        );
    let potion_continuation_pressure =
        crate::ai::potion_continuation_pressure_v1::potion_continuation_pressure_v1(
            &session.run_state,
            &potion_continuation_context,
        );
    let combat_victory_continuation =
        crate::eval::run_control::CombatVictoryContinuationFactsV1::from_guaranteed_room_boss_full_heal(
            crate::eval::run_control::strategic_combat_victory_reaches_full_heal_v1(session),
        );
    let (entry_current_hp, entry_max_hp) = session.visible_player_hp();
    let strategic_hp_quality =
        crate::eval::run_control::CombatSearchStrategicHpQualityFactsV1::from_owner_limits(
            entry_current_hp,
            entry_max_hp,
            crate::eval::run_control::strategic_combat_survival_hp_loss_limit_v1(session),
            crate::eval::run_control::strategic_combat_quality_hp_loss_limit_v1(session),
        );

    Ok(crate::eval::run_control::AtomicCombatSearchTraceSummaryV2 {
        source: "reconstructed_exact_production_context".to_string(),
        potion_continuation_context: Some(potion_continuation_context),
        potion_continuation_pressure: Some(potion_continuation_pressure),
        combat_victory_continuation: Some(combat_victory_continuation),
        strategic_hp_quality: Some(strategic_hp_quality),
        ..crate::eval::run_control::AtomicCombatSearchTraceSummaryV2::default()
    })
}

pub fn oracle_live_combat_diagnostic_v1(
    workspace: &OracleAnalysisWorkspaceV1,
    node: usize,
    max_engine_steps_per_transition: usize,
) -> Result<Value, String> {
    let view = workspace.session.view_node(node)?;
    if view.encounter.is_none() {
        return Err(format!(
            "oracle node {node} is not at an active combat boundary"
        ));
    }
    let (generation_work, wall_ms) = if view.encounter.as_ref().is_some_and(|it| it.is_boss) {
        (
            workspace.budget.boss_generation_work,
            workspace.budget.boss_ms,
        )
    } else if view.encounter.as_ref().is_some_and(|it| it.is_elite) {
        (
            workspace.budget.elite_generation_work,
            workspace.budget.elite_ms,
        )
    } else {
        (
            workspace.budget.hallway_generation_work,
            workspace.budget.hallway_ms,
        )
    };
    let case = workspace.session.combat_case(
        node,
        workspace.seed,
        workspace.ascension,
        generation_work,
        wall_ms,
    )?;
    let progress_actions = view
        .combat
        .as_ref()
        .map(|progress| progress.deepest_progress_actions.as_slice())
        .unwrap_or_default();
    let survival_actions = view
        .combat
        .as_ref()
        .map(|progress| progress.deepest_survival_actions.as_slice())
        .unwrap_or_default();
    let root_policy = combat_policy_surface(&case.core.position, 32);
    let root_action_families = workspace
        .session
        .combat_root_action_families(node)
        .unwrap_or_default()
        .into_iter()
        .map(|family| {
            json!({
                "action": combat_action_label(&case.core.position, &family.first_action),
                "best_root_negative_log_policy": family.best_root_negative_log_policy,
                "completed_root_turn_options": family.completed_root_turn_options,
                "terminal_wins": family.terminal_wins,
                "terminal_losses": family.terminal_losses,
                "escapes": family.escapes,
                "unique_next_turn_successors": family.unique_next_turn_successors,
                "retained_next_turn_successors": family.retained_next_turn_successors,
                "reachable_exact_states": family.reachable_exact_states,
                "reachable_retained_states": family.reachable_retained_states,
                "reachable_generation_work": family.reachable_generation_work,
                "reachable_completed_turn_options": family.reachable_completed_turn_options,
                "max_player_turn": family.max_player_turn,
                "best_hp_at_max_turn": family.best_hp_at_max_turn,
                "lowest_enemy_hp_at_max_turn": family.lowest_enemy_hp_at_max_turn,
            })
        })
        .collect::<Vec<_>>();
    let deepest_progress_trace = replay_combat_path(
        case.core.position.clone(),
        progress_actions,
        max_engine_steps_per_transition,
    )?;
    let deepest_survival_trace = if survival_actions == progress_actions {
        json!({"same_as": "deepest_progress_trace"})
    } else {
        replay_combat_path(
            case.core.position.clone(),
            survival_actions,
            max_engine_steps_per_transition,
        )?
    };

    Ok(json!({
        "schema_name": "OracleLiveCombatDiagnosticV1",
        "schema_version": 1,
        "node": {
            "node_id": node,
            "act": view.act,
            "floor": view.floor,
            "hp": view.current_hp,
            "max_hp": view.max_hp,
            "boundary": view.boundary,
            "state_fingerprint": view.state_fingerprint,
        },
        "run": {
            "deck": case.core.position.combat.meta.master_deck_snapshot.iter().map(card_label).collect::<Vec<_>>(),
            "relics": case.core.position.combat.entities.player.relics.iter().map(|relic| relic.id.stable_id()).collect::<Vec<_>>(),
            "potions": case.core.position.combat.entities.potions.iter().map(|potion| potion.as_ref().map(|potion| potion.id.stable_id())).collect::<Vec<_>>(),
        },
        "root": combat_position_snapshot(&case.core.position),
        "root_policy": root_policy,
        "root_action_families": root_action_families,
        "search": compact_combat_progress(view.combat.as_ref()),
        "deepest_progress_trace": deepest_progress_trace,
        "deepest_survival_trace": deepest_survival_trace,
    }))
}

fn compact_combat_progress(combat: Option<&OracleAnalysisCombatProgressV1>) -> Value {
    let Some(combat) = combat else {
        return Value::Null;
    };
    json!({
        "historical_generation_work": combat.historical_generation_work,
        "current_search_generation_work": combat.current_search_generation_work,
        "generation_work": combat.generation_work,
        "local_generation_work": combat.local_generation_work,
        "discrepancy_generation_work": combat.discrepancy_generation_work,
        "exact_states": combat.exact_states,
        "local_exact_states": combat.local_exact_states,
        "discrepancy_exact_states": combat.discrepancy_exact_states,
        "completed_turn_options": combat.completed_turn_options,
        "retained_state_work": combat.retained_state_work,
        "local_retained_state_work": combat.local_retained_state_work,
        "discrepancy_retained_state_work": combat.discrepancy_retained_state_work,
        "max_player_turn": combat.max_player_turn,
        "deepest_progress": combat.deepest_progress_state,
        "deepest_survival": combat.deepest_survival_state,
        "incumbent_final_hp": combat.incumbent_final_hp,
        "incumbent_hp_loss": combat.incumbent_hp_loss,
        "incumbent_actions": combat.incumbent_action_count,
        "last_status": combat.last_status,
        "quantum_count": combat.quantum_count,
        "remaining_generation_work": combat.remaining_generation_work,
        "remaining_wall_ms": combat.remaining_wall_ms,
        "resume_kind": combat.resume_kind,
        "restart_count": combat.restart_count,
    })
}

pub fn combat_policy_surface(position: &CombatPosition, limit: usize) -> Value {
    const UNIFORM_EXPLORATION: f64 = 0.05;

    let stepper = EngineCombatStepper;
    let actions = stepper.atomic_actions(position);
    let weights =
        crate::ai::combat_search_v2::oracle_action_policy::oracle_atomic_action_policy_weights(
            position, &actions,
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
        "actions": ranked.into_iter().take(limit).map(|(_, _, value)| value).collect::<Vec<_>>(),
    })
}

pub fn replay_combat_path(
    mut position: CombatPosition,
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
            || !matches!(terminal, crate::sim::combat::CombatTerminal::Unresolved)
        {
            turns.push(json!({
                "turn": turn_number,
                "action_range": {"first": turn_start_action_index, "last": index + 1},
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
            "action_range": {"first": turn_start_action_index, "last": actions.len()},
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

pub fn combat_action_label(position: &CombatPosition, input: &ClientInput) -> String {
    match input {
        ClientInput::PlayCard { card_index, target } => position
            .combat
            .zones
            .hand
            .get(*card_index)
            .map(|card| {
                let target = compact_target_label(&position.combat, *target);
                if target == "none" {
                    format!("play {}", card_label(card))
                } else {
                    format!("play {} -> {target}", card_label(card))
                }
            })
            .unwrap_or_else(|| combat_action_key(&position.combat, input)),
        ClientInput::UsePotion {
            potion_index,
            target,
        } => {
            let potion = position
                .combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(Option::as_ref)
                .map(|potion| potion.id.stable_id().to_string())
                .unwrap_or_else(|| format!("slot {potion_index}"));
            let target = compact_target_label(&position.combat, *target);
            if target == "none" {
                format!("use {potion}")
            } else {
                format!("use {potion} -> {target}")
            }
        }
        ClientInput::EndTurn => "end turn".to_string(),
        ClientInput::SubmitSelection(resolution) => {
            let selected = resolution
                .selected_card_lineages()
                .into_iter()
                .map(|lineage| combat_card_lineage_label(&position.combat, lineage))
                .collect::<Vec<_>>()
                .join(", ");
            format!("select {selected}")
        }
        _ => combat_action_key(&position.combat, input),
    }
}

pub fn combat_position_snapshot(position: &CombatPosition) -> Value {
    let combat = &position.combat;
    let player = &combat.entities.player;
    json!({
        "turn": combat.turn.turn_count,
        "phase": format!("{:?}", combat.turn.current_phase),
        "player": {
            "hp": player.current_hp,
            "max_hp": player.max_hp,
            "block": player.block,
            "energy": combat.turn.energy,
            "powers": combat_power_labels(combat, player.id),
        },
        "hand": combat.zones.hand.iter().map(card_label).collect::<Vec<_>>().join(" | "),
        "piles": format!("draw {} / discard {} / exhaust {}", combat.zones.draw_pile.len(), combat.zones.discard_pile.len(), combat.zones.exhaust_pile.len()),
        "monsters": combat.entities.monsters.iter().map(|monster| monster_state_label(combat, monster)).collect::<Vec<_>>(),
    })
}

fn combat_turn_snapshot(position: &CombatPosition) -> Value {
    let combat = &position.combat;
    let player = &combat.entities.player;
    json!({
        "hp": player.current_hp,
        "block": player.block,
        "energy": combat.turn.energy,
        "player_powers": combat_power_labels(combat, player.id),
        "hand": combat.zones.hand.iter().map(card_label).collect::<Vec<_>>().join(" | "),
        "piles": format!("draw {} / discard {} / exhaust {}", combat.zones.draw_pile.len(), combat.zones.discard_pile.len(), combat.zones.exhaust_pile.len()),
        "monsters": combat.entities.monsters.iter().map(|monster| monster_state_label(combat, monster)).collect::<Vec<_>>(),
    })
}

fn compact_target_label(combat: &CombatState, target: Option<usize>) -> String {
    let Some(target) = target else {
        return "none".to_string();
    };
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target)
        .map(|monster| {
            let label = monster.enemy_id.get_name();
            format!("{label}[{}]", monster.slot)
        })
        .unwrap_or_else(|| target_label(combat, Some(target)))
}

fn combat_card_lineage_label(
    combat: &CombatState,
    lineage: crate::runtime::combat::CardLineageId,
) -> String {
    combat
        .zones
        .hand
        .iter()
        .chain(&combat.zones.draw_pile)
        .chain(&combat.zones.discard_pile)
        .chain(&combat.zones.exhaust_pile)
        .find(|card| card.lineage == lineage)
        .map(card_label)
        .unwrap_or_else(|| format!("card#{lineage}"))
}

fn combat_power_labels(combat: &CombatState, entity: crate::EntityId) -> Vec<String> {
    crate::content::powers::store::powers_for(combat, entity)
        .unwrap_or_default()
        .iter()
        .map(|power| {
            format!(
                "{}:{}",
                gameplay_display_label_v1(power.power_type),
                power.amount
            )
        })
        .collect()
}

fn monster_state_label(combat: &CombatState, monster: &MonsterEntity) -> String {
    let label = monster.enemy_id.get_name();
    if !monster.is_alive_for_action() {
        return format!("{label}[{}] dead", monster.slot);
    }
    let intent = monster
        .move_state
        .planned_visible_spec
        .as_ref()
        .map(|intent| format!("{intent:?}"))
        .unwrap_or_else(|| format!("move:{}", monster.planned_move_id()));
    let powers = combat_power_labels(combat, monster.id);
    let powers = if powers.is_empty() {
        String::new()
    } else {
        format!(" powers=[{}]", powers.join(", "))
    };
    format!(
        "{label}[{}] {}/{} block={} intent={intent}{powers}",
        monster.slot, monster.current_hp, monster.max_hp, monster.block
    )
}

fn card_label(card: &CombatCard) -> String {
    let upgrade = if card.upgrades == 0 {
        String::new()
    } else {
        format!("+{}", card.upgrades)
    };
    format!("{}{}", gameplay_display_label_v1(card.id), upgrade)
}
