//! Read-only labels, snapshots, and compact trace views for exact combat diagnostics.

use serde::Serialize;
use serde_json::{json, Value};
use sts_combat_planner::TurnOptionAction;
use sts_oracle_runtime::content::identity::gameplay_display_label_v1;
use sts_oracle_runtime::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
use sts_oracle_runtime::sim::combat_action::{combat_action_key, target_label};
use sts_oracle_runtime::state::core::ClientInput;

pub(super) fn combat_action_label(
    position: &sts_oracle_runtime::sim::combat::CombatPosition,
    input: &ClientInput,
) -> String {
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
                .map(|potion| gameplay_display_label_v1(potion.id).to_string())
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

pub(super) fn readable_turn_option_action_labels(
    root: &sts_oracle_runtime::sim::combat::CombatPosition,
    actions: &[TurnOptionAction],
    max_engine_steps_per_transition: usize,
) -> Result<Vec<String>, String> {
    let stepper = EngineCombatStepper;
    let mut position = root.clone();
    let mut labels = Vec::with_capacity(actions.len());
    for action in actions {
        labels.push(combat_action_label(&position, &action.input));
        let step = stepper.apply_to_stable(
            &position,
            action.input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(
                "generated option action could not be replayed while formatting".to_string(),
            );
        }
        position = step.position;
    }
    Ok(labels)
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct AtomicPolicyChoiceTraceV2 {
    pub(super) input: ClientInput,
    pub(super) action: String,
    pub(super) action_key: String,
    pub(super) raw_weight: f64,
    pub(super) is_selected: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct AtomicPolicyTraceStepV2 {
    pub(super) step: usize,
    pub(super) turn: u32,
    pub(super) action: String,
    pub(super) input: ClientInput,
    pub(super) action_key: String,
    pub(super) legal_action_count: usize,
    pub(super) ordinal_rank: Option<usize>,
    pub(super) raw_weight: Option<f64>,
    pub(super) probability: Option<f64>,
    pub(super) negative_log_probability: Option<f64>,
    pub(super) surface: &'static str,
    pub(super) top_choices: Vec<AtomicPolicyChoiceTraceV2>,
}

pub(super) fn target_atomic_policy_trace(
    initial: &sts_oracle_runtime::sim::combat::CombatPosition,
    target: &[ClientInput],
    max_engine_steps_per_transition: usize,
) -> Result<
    (
        Vec<AtomicPolicyTraceStepV2>,
        String,
        Vec<sts_oracle_runtime::sim::combat::CombatPosition>,
    ),
    String,
> {
    const UNIFORM_EXPLORATION: f64 = 0.05;

    let stepper = EngineCombatStepper;
    let mut position = initial.clone();
    let mut trace = Vec::with_capacity(target.len());
    let mut prefix_positions = Vec::with_capacity(target.len());
    for (step_index, input) in target.iter().enumerate() {
        let legal = stepper.atomic_actions(&position);
        let weights =
            sts_oracle_runtime::ai::combat_search_v2::oracle_action_policy::oracle_atomic_action_policy_weights(
                &position,
                &legal,
            );
        let target_index = legal.iter().position(|candidate| candidate == input);
        let exact_input_is_legal =
            target_index.is_some() || stepper.choice_for_legal_input(&position, input).is_some();
        let mut ranked_indices = (0..legal.len()).collect::<Vec<_>>();
        ranked_indices.sort_by(|left, right| {
            weights[*right]
                .total_cmp(&weights[*left])
                .then_with(|| left.cmp(right))
        });
        let top_choices = ranked_indices
            .into_iter()
            .take(3)
            .map(|index| AtomicPolicyChoiceTraceV2 {
                input: legal[index].clone(),
                action: combat_action_label(&position, &legal[index]),
                action_key: combat_action_key(&position.combat, &legal[index]),
                raw_weight: weights[index],
                is_selected: target_index == Some(index),
            })
            .collect::<Vec<_>>();
        let (ordinal_rank, raw_weight, probability, negative_log_probability) = target_index
            .and_then(|index| weights.get(index).copied().map(|weight| (index, weight)))
            .map_or((None, None, None, None), |(_, weight)| {
                let rank = 1 + weights
                    .iter()
                    .filter(|candidate| **candidate > weight)
                    .count();
                let total = weights.iter().sum::<f64>();
                let uniform = 1.0 / weights.len().max(1) as f64;
                let probability = ((1.0 - UNIFORM_EXPLORATION) * (weight / total)
                    + UNIFORM_EXPLORATION * uniform)
                    .max(f64::MIN_POSITIVE);
                (
                    Some(rank),
                    Some(weight),
                    Some(probability),
                    Some(-probability.ln()),
                )
            });
        trace.push(AtomicPolicyTraceStepV2 {
            step: step_index,
            turn: position.combat.turn.turn_count,
            action: combat_action_label(&position, input),
            input: input.clone(),
            action_key: combat_action_key(&position.combat, input),
            legal_action_count: legal.len(),
            ordinal_rank,
            raw_weight,
            probability,
            negative_log_probability,
            surface: if target_index.is_some() {
                "atomic"
            } else {
                "structured_selection"
            },
            top_choices,
        });
        if !exact_input_is_legal {
            return Err(format!(
                "target action {step_index} is not on the exact legal action surface: {input:?}"
            ));
        }
        let result = stepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if result.truncated {
            return Err(format!(
                "target action {step_index} exceeded the exact transition limit"
            ));
        }
        position = result.position;
        prefix_positions.push(position.clone());
    }
    Ok((
        trace,
        sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
            &position.engine,
            &position.combat,
        ),
        prefix_positions,
    ))
}

fn compact_target_label(
    combat: &sts_oracle_runtime::runtime::combat::CombatState,
    target: Option<usize>,
) -> String {
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
    combat: &sts_oracle_runtime::runtime::combat::CombatState,
    lineage: sts_oracle_runtime::runtime::combat::CardLineageId,
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

pub(super) fn combat_turn_snapshot(
    position: &sts_oracle_runtime::sim::combat::CombatPosition,
) -> Value {
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

pub(super) fn combat_position_snapshot(
    position: &sts_oracle_runtime::sim::combat::CombatPosition,
) -> Value {
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

fn combat_power_labels(
    combat: &sts_oracle_runtime::runtime::combat::CombatState,
    entity: sts_oracle_runtime::EntityId,
) -> Vec<String> {
    sts_oracle_runtime::content::powers::store::powers_for(combat, entity)
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

fn monster_state_label(
    combat: &sts_oracle_runtime::runtime::combat::CombatState,
    monster: &sts_oracle_runtime::runtime::combat::MonsterEntity,
) -> String {
    let label = monster.enemy_id.get_name();
    if monster.is_escaped {
        return format!("{label}[{}] escaped", monster.slot);
    }
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

fn card_label(card: &sts_oracle_runtime::runtime::combat::CombatCard) -> String {
    let upgrade = if card.upgrades == 0 {
        String::new()
    } else {
        format!("+{}", card.upgrades)
    };
    format!("{}{}", gameplay_display_label_v1(card.id), upgrade)
}

pub(super) fn compact_combat_trace(trace: Option<&Value>) -> Value {
    let Some(trace) = trace else {
        return Value::Null;
    };
    let turns = trace
        .get("turns")
        .and_then(Value::as_array)
        .map(|turns| {
            turns
                .iter()
                .map(|turn| {
                    let end = turn.get("end");
                    json!({
                        "turn": turn.get("turn"),
                        "action_range": turn.get("action_range"),
                        "start_hp": turn.get("start_hp"),
                        "actions": turn.get("actions"),
                        "end": {
                            "hp": end.and_then(|value| value.get("hp")),
                            "block": end.and_then(|value| value.get("block")),
                            "energy": end.and_then(|value| value.get("energy")),
                            "hand": end.and_then(|value| value.get("hand")),
                            "piles": end.and_then(|value| value.get("piles")),
                            "player_powers": end.and_then(|value| value.get("player_powers")),
                            "monsters": end.and_then(|value| value.get("monsters")),
                        },
                        "terminal": turn.get("terminal"),
                        "partial": turn.get("partial"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "action_count": trace.get("action_count"),
        "turns": turns,
        "terminal": trace.get("terminal"),
    })
}

pub(super) fn compact_local_corridor_report(report: Option<&Value>) -> Value {
    let Some(report) = report else {
        return Value::Null;
    };
    let states = report
        .get("states")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let reached = states
        .iter()
        .filter(|state| state.get("state").is_some_and(|value| !value.is_null()))
        .count();
    let first_missing = states
        .iter()
        .find(|state| state.get("state").is_none_or(Value::is_null));
    let furthest_reached_index = states
        .iter()
        .rposition(|state| state.get("state").is_some_and(|value| !value.is_null()));
    let furthest_reached = furthest_reached_index.and_then(|index| states.get(index));
    let incoming_to_furthest = furthest_reached_index
        .and_then(|index| index.checked_sub(1))
        .and_then(|index| states.get(index))
        .and_then(|state| state.get("outgoing_to_next"))
        .filter(|value| !value.is_null());
    json!({
        "authority": report.get("authority"),
        "changes_search_order": report.get("changes_search_order"),
        "action_count": report.get("action_count"),
        "exact_turn_states": report.get("exact_turn_states"),
        "reached_turn_states": reached,
        "first_missing_rank": first_missing
            .and_then(|state| state.get("corridor_rank")),
        "first_missing": first_missing,
        "incoming_to_furthest": incoming_to_furthest,
        "furthest_reached": furthest_reached,
        "terminal_final_hp": report.get("terminal_final_hp"),
    })
}
