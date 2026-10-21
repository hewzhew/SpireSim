use std::path::PathBuf;

use clap::Args;
use serde::Serialize;
use sts_oracle_runtime::content::monsters::EnemyId;
use sts_oracle_runtime::content::potions::Potion;
use sts_oracle_runtime::content::powers::PowerId;
use sts_oracle_runtime::content::relics::RelicState;
use sts_oracle_runtime::eval::combat_case::load_combat_case;
use sts_oracle_runtime::runtime::combat::{CombatCard, CombatPhase, Power};
use sts_oracle_runtime::sim::combat::{
    combat_terminal, CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal,
    EngineCombatStepper,
};
use sts_oracle_runtime::sim::combat_action::combat_action_key;
use sts_oracle_runtime::sim::combat_projection::project_monster_move_preview_in_combat;
use sts_oracle_runtime::state::core::{ClientInput, EngineState};

use super::exact_turn_corridor::load_action_segments;
use super::oracle_lab_runtime_identity;

#[derive(Debug, Args)]
pub(super) struct CombatCaseRouteCompareArgs {
    #[arg(long)]
    case: PathBuf,
    /// One or more consecutive exact action segments for neutral route A.
    #[arg(long = "route-a-actions", required = true)]
    route_a_actions: Vec<PathBuf>,
    /// Replay only the first N actions of route A.
    #[arg(long)]
    route_a_through: Option<usize>,
    /// One or more consecutive exact action segments for neutral route B.
    #[arg(long = "route-b-actions", required = true)]
    route_b_actions: Vec<PathBuf>,
    /// Replay only the first N actions of route B.
    #[arg(long)]
    route_b_through: Option<usize>,
    #[arg(long, default_value_t = 250)]
    max_engine_steps_per_transition: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct CombatCaseRouteCompareReportV1 {
    schema_name: &'static str,
    schema_version: u8,
    case: PathBuf,
    runtime: serde_json::Value,
    contract: RouteCompareContractV1,
    root_exact_state_hash: String,
    shared_prefix: SharedRoutePrefixV1,
    route_a: RouteReplayV1,
    route_b: RouteReplayV1,
    aligned_turn_boundaries: Vec<AlignedTurnBoundaryV1>,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct RouteCompareContractV1 {
    search: bool,
    ranking: bool,
    pruning: bool,
    policy_mutation: bool,
    comparison_semantics: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct SharedRoutePrefixV1 {
    action_count: usize,
    replay_hashes_match: bool,
    exact_state_hash_after_prefix: String,
    first_divergence: Option<RouteActionDivergenceV1>,
}

#[derive(Clone, Debug, Serialize)]
struct RouteActionDivergenceV1 {
    action_index: usize,
    route_a_input: Option<ClientInput>,
    route_b_input: Option<ClientInput>,
    route_a_action_key: Option<String>,
    route_b_action_key: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct RouteReplayV1 {
    action_paths: Vec<PathBuf>,
    supplied_action_count: usize,
    requested_through: Option<usize>,
    requested_action_count: usize,
    consumed_action_count: usize,
    unconsumed_requested_action_count: usize,
    invalid_action: Option<RouteInvalidActionV1>,
    truncated_transition: bool,
    timed_out_transition: bool,
    final_terminal: CombatTerminal,
    final_exact_state_hash: String,
    final_state: RouteStateSnapshotV1,
    actions: Vec<RouteActionRecordV1>,
    turn_boundaries: Vec<RouteTurnBoundaryV1>,
}

#[derive(Clone, Debug, Serialize)]
struct RouteInvalidActionV1 {
    action_index: usize,
    input: ClientInput,
    action_key: String,
    before_exact_state_hash: String,
}

#[derive(Clone, Debug, Serialize)]
struct RouteActionRecordV1 {
    action_index: usize,
    input: ClientInput,
    action_key: String,
    before_exact_state_hash: String,
    after_exact_state_hash: String,
    engine_steps: usize,
    truncated: bool,
    timed_out: bool,
    terminal: CombatTerminal,
}

#[derive(Clone, Debug, Serialize)]
struct RouteTurnBoundaryV1 {
    boundary_index: usize,
    consumed_action_count: usize,
    exact_state_hash: String,
    state: RouteStateSnapshotV1,
}

#[derive(Clone, Debug, Serialize)]
struct AlignedTurnBoundaryV1 {
    boundary_index: usize,
    route_a: Option<RouteTurnBoundaryV1>,
    route_b: Option<RouteTurnBoundaryV1>,
    route_b_minus_route_a: Option<RouteStateDeltaV1>,
}

#[derive(Clone, Debug, Serialize)]
struct RouteStateSnapshotV1 {
    engine_state: EngineState,
    terminal: CombatTerminal,
    turn_count: u32,
    phase: CombatPhase,
    energy: u8,
    player: RoutePlayerSnapshotV1,
    enemies: Vec<RouteEnemySnapshotV1>,
    potions: Vec<Option<Potion>>,
    zones: RouteCardZonesSnapshotV1,
    living_enemy_count: usize,
    total_enemy_hp: i32,
    total_enemy_block: i32,
    visible_incoming_damage: i32,
    visible_hp_loss: i32,
    visible_survival_margin: i32,
}

#[derive(Clone, Debug, Serialize)]
struct RoutePlayerSnapshotV1 {
    entity_id: usize,
    current_hp: i32,
    max_hp: i32,
    block: i32,
    relics: Vec<RelicState>,
    powers: Vec<Power>,
}

#[derive(Clone, Debug, Serialize)]
struct RouteEnemySnapshotV1 {
    entity_id: usize,
    enemy_id: EnemyId,
    slot: u8,
    logical_position: i32,
    alive_for_action: bool,
    current_hp: i32,
    max_hp: i32,
    block: i32,
    planned_move_id: u8,
    visible_damage_per_hit: Option<i32>,
    visible_hits: u8,
    visible_total_damage: Option<i32>,
    powers: Vec<Power>,
}

#[derive(Clone, Debug, Serialize)]
struct RouteCardZonesSnapshotV1 {
    draw_pile: Vec<CombatCard>,
    hand: Vec<CombatCard>,
    discard_pile: Vec<CombatCard>,
    exhaust_pile: Vec<CombatCard>,
    limbo: Vec<CombatCard>,
    queued_cards_count: usize,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct ScalarDeltaV1 {
    route_a: i32,
    route_b: i32,
    route_b_minus_route_a: i32,
}

#[derive(Clone, Debug, Serialize)]
struct RouteStateDeltaV1 {
    player_hp: ScalarDeltaV1,
    player_block: ScalarDeltaV1,
    energy: ScalarDeltaV1,
    living_enemy_count: ScalarDeltaV1,
    total_enemy_hp: ScalarDeltaV1,
    total_enemy_block: ScalarDeltaV1,
    visible_incoming_damage: ScalarDeltaV1,
    visible_hp_loss: ScalarDeltaV1,
    visible_survival_margin: ScalarDeltaV1,
    potion_count: ScalarDeltaV1,
    hand_count: ScalarDeltaV1,
    draw_count: ScalarDeltaV1,
    discard_count: ScalarDeltaV1,
    exhaust_count: ScalarDeltaV1,
    player_power_amount_deltas: Vec<RoutePowerAmountDeltaV1>,
    enemy_power_amount_deltas: Vec<RouteEnemyPowerAmountDeltaV1>,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct RoutePowerAmountDeltaV1 {
    power_id: PowerId,
    route_a: i32,
    route_b: i32,
    route_b_minus_route_a: i32,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct RouteEnemyPowerAmountDeltaV1 {
    slot: u8,
    power_id: PowerId,
    route_a: i32,
    route_b: i32,
    route_b_minus_route_a: i32,
}

pub(super) fn run(
    args: CombatCaseRouteCompareArgs,
) -> Result<CombatCaseRouteCompareReportV1, String> {
    let CombatCaseRouteCompareArgs {
        case,
        route_a_actions,
        route_a_through,
        route_b_actions,
        route_b_through,
        max_engine_steps_per_transition,
    } = args;
    let loaded = load_combat_case(&case)?;
    let root = loaded.core.position;
    let root_exact_state_hash = exact_state_hash(&root);
    let route_a_inputs = load_action_segments(&route_a_actions)?;
    let route_b_inputs = load_action_segments(&route_b_actions)?;
    let requested_a = requested_inputs(&route_a_inputs, route_a_through, "route-a")?;
    let requested_b = requested_inputs(&route_b_inputs, route_b_through, "route-b")?;
    let route_a = replay_route(
        &root,
        route_a_actions,
        route_a_inputs.len(),
        route_a_through,
        requested_a,
        max_engine_steps_per_transition,
    );
    let route_b = replay_route(
        &root,
        route_b_actions,
        route_b_inputs.len(),
        route_b_through,
        requested_b,
        max_engine_steps_per_transition,
    );
    let shared_prefix = shared_prefix(&root_exact_state_hash, &route_a, &route_b);
    let aligned_turn_boundaries = align_turn_boundaries(&route_a, &route_b);

    Ok(CombatCaseRouteCompareReportV1 {
        schema_name: "OracleCombatCaseRouteCompareV1",
        schema_version: 1,
        case,
        runtime: oracle_lab_runtime_identity(),
        contract: RouteCompareContractV1 {
            search: false,
            ranking: false,
            pruning: false,
            policy_mutation: false,
            comparison_semantics: "neutral_exact_replay_route_a_route_b",
        },
        root_exact_state_hash,
        shared_prefix,
        route_a,
        route_b,
        aligned_turn_boundaries,
    })
}

fn requested_inputs(
    inputs: &[ClientInput],
    through: Option<usize>,
    route: &str,
) -> Result<Vec<ClientInput>, String> {
    let requested = through.unwrap_or(inputs.len());
    if requested > inputs.len() {
        return Err(format!(
            "--{route}-through {requested} exceeds the {} supplied actions",
            inputs.len()
        ));
    }
    Ok(inputs[..requested].to_vec())
}

fn replay_route(
    root: &CombatPosition,
    action_paths: Vec<PathBuf>,
    supplied_action_count: usize,
    requested_through: Option<usize>,
    inputs: Vec<ClientInput>,
    max_engine_steps_per_transition: usize,
) -> RouteReplayV1 {
    let requested_action_count = inputs.len();
    let mut position = root.clone();
    let mut actions = Vec::new();
    let mut turn_boundaries = vec![turn_boundary(0, 0, &position)];
    let mut invalid_action = None;
    let mut truncated_transition = false;
    let mut timed_out_transition = false;

    for (action_index, input) in inputs.into_iter().enumerate() {
        let before_exact_state_hash = exact_state_hash(&position);
        let action_key = combat_action_key(&position.combat, &input);
        if EngineCombatStepper
            .choice_for_legal_input(&position, &input)
            .is_none()
        {
            invalid_action = Some(RouteInvalidActionV1 {
                action_index,
                input,
                action_key,
                before_exact_state_hash,
            });
            break;
        }
        let before_turn = position.combat.turn.turn_count;
        let step = EngineCombatStepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition.max(1),
                deadline: None,
            },
        );
        let after_exact_state_hash = exact_state_hash(&step.position);
        actions.push(RouteActionRecordV1 {
            action_index,
            input,
            action_key,
            before_exact_state_hash,
            after_exact_state_hash: after_exact_state_hash.clone(),
            engine_steps: step.engine_steps,
            truncated: step.truncated,
            timed_out: step.timed_out,
            terminal: step.terminal,
        });
        truncated_transition |= step.truncated;
        timed_out_transition |= step.timed_out;
        position = step.position;
        let entered_new_player_turn = matches!(position.engine, EngineState::CombatPlayerTurn)
            && position.combat.turn.turn_count != before_turn;
        if entered_new_player_turn || step.terminal != CombatTerminal::Unresolved {
            turn_boundaries.push(turn_boundary(
                turn_boundaries.len(),
                action_index.saturating_add(1),
                &position,
            ));
        }
        if step.truncated || step.timed_out || step.terminal != CombatTerminal::Unresolved {
            break;
        }
    }

    let consumed_action_count = actions.len();
    RouteReplayV1 {
        action_paths,
        supplied_action_count,
        requested_through,
        requested_action_count,
        consumed_action_count,
        unconsumed_requested_action_count: requested_action_count
            .saturating_sub(consumed_action_count),
        invalid_action,
        truncated_transition,
        timed_out_transition,
        final_terminal: combat_terminal(&position.engine, &position.combat),
        final_exact_state_hash: exact_state_hash(&position),
        final_state: state_snapshot(&position),
        actions,
        turn_boundaries,
    }
}

fn shared_prefix(
    root_exact_state_hash: &str,
    route_a: &RouteReplayV1,
    route_b: &RouteReplayV1,
) -> SharedRoutePrefixV1 {
    let action_count = route_a
        .actions
        .iter()
        .zip(&route_b.actions)
        .take_while(|(left, right)| left.input == right.input)
        .count();
    let replay_hashes_match = route_a
        .actions
        .iter()
        .zip(&route_b.actions)
        .take(action_count)
        .all(|(left, right)| left.after_exact_state_hash == right.after_exact_state_hash);
    let exact_state_hash_after_prefix = if action_count == 0 {
        root_exact_state_hash.to_string()
    } else {
        route_a.actions[action_count - 1]
            .after_exact_state_hash
            .clone()
    };
    let route_a_action = route_a.actions.get(action_count);
    let route_b_action = route_b.actions.get(action_count);
    let first_divergence =
        (route_a_action.is_some() || route_b_action.is_some()).then(|| RouteActionDivergenceV1 {
            action_index: action_count,
            route_a_input: route_a_action.map(|action| action.input.clone()),
            route_b_input: route_b_action.map(|action| action.input.clone()),
            route_a_action_key: route_a_action.map(|action| action.action_key.clone()),
            route_b_action_key: route_b_action.map(|action| action.action_key.clone()),
        });

    SharedRoutePrefixV1 {
        action_count,
        replay_hashes_match,
        exact_state_hash_after_prefix,
        first_divergence,
    }
}

fn align_turn_boundaries(
    route_a: &RouteReplayV1,
    route_b: &RouteReplayV1,
) -> Vec<AlignedTurnBoundaryV1> {
    let count = route_a
        .turn_boundaries
        .len()
        .max(route_b.turn_boundaries.len());
    (0..count)
        .map(|boundary_index| {
            let route_a_boundary = route_a.turn_boundaries.get(boundary_index).cloned();
            let route_b_boundary = route_b.turn_boundaries.get(boundary_index).cloned();
            let delta = route_a_boundary
                .as_ref()
                .zip(route_b_boundary.as_ref())
                .map(|(left, right)| state_delta(&left.state, &right.state));
            AlignedTurnBoundaryV1 {
                boundary_index,
                route_a: route_a_boundary,
                route_b: route_b_boundary,
                route_b_minus_route_a: delta,
            }
        })
        .collect()
}

fn turn_boundary(
    boundary_index: usize,
    consumed_action_count: usize,
    position: &CombatPosition,
) -> RouteTurnBoundaryV1 {
    RouteTurnBoundaryV1 {
        boundary_index,
        consumed_action_count,
        exact_state_hash: exact_state_hash(position),
        state: state_snapshot(position),
    }
}

fn state_snapshot(position: &CombatPosition) -> RouteStateSnapshotV1 {
    let combat = &position.combat;
    let player = &combat.entities.player;
    let enemies = combat
        .entities
        .monsters
        .iter()
        .map(|monster| {
            let preview = project_monster_move_preview_in_combat(combat, monster);
            RouteEnemySnapshotV1 {
                entity_id: monster.id,
                enemy_id: monster.enemy_id,
                slot: monster.slot,
                logical_position: monster.logical_position,
                alive_for_action: monster.is_alive_for_action(),
                current_hp: monster.current_hp,
                max_hp: monster.max_hp,
                block: monster.block,
                planned_move_id: monster.planned_move_id(),
                visible_damage_per_hit: preview.damage_per_hit,
                visible_hits: preview.hits,
                visible_total_damage: preview.total_damage,
                powers: entity_powers(combat, monster.id),
            }
        })
        .collect::<Vec<_>>();
    let visible_incoming_damage = enemies
        .iter()
        .filter(|enemy| enemy.alive_for_action)
        .filter_map(|enemy| enemy.visible_total_damage)
        .sum::<i32>();
    let visible_hp_loss = visible_incoming_damage.saturating_sub(player.block).max(0);
    let living_enemy_count = enemies
        .iter()
        .filter(|enemy| enemy.alive_for_action)
        .count();
    let total_enemy_hp = enemies
        .iter()
        .filter(|enemy| enemy.alive_for_action)
        .map(|enemy| enemy.current_hp.max(0))
        .sum();
    let total_enemy_block = enemies
        .iter()
        .filter(|enemy| enemy.alive_for_action)
        .map(|enemy| enemy.block.max(0))
        .sum();
    RouteStateSnapshotV1 {
        engine_state: position.engine.clone(),
        terminal: combat_terminal(&position.engine, combat),
        turn_count: combat.turn.turn_count,
        phase: combat.turn.current_phase,
        energy: combat.turn.energy,
        player: RoutePlayerSnapshotV1 {
            entity_id: player.id,
            current_hp: player.current_hp,
            max_hp: player.max_hp,
            block: player.block,
            relics: player.relics.clone(),
            powers: entity_powers(combat, player.id),
        },
        enemies,
        potions: combat.entities.potions.clone(),
        zones: RouteCardZonesSnapshotV1 {
            draw_pile: combat.zones.draw_pile.iter().cloned().collect(),
            hand: combat.zones.hand.clone(),
            discard_pile: combat.zones.discard_pile.iter().cloned().collect(),
            exhaust_pile: combat.zones.exhaust_pile.iter().cloned().collect(),
            limbo: combat.zones.limbo.clone(),
            queued_cards_count: combat.zones.queued_cards.len(),
        },
        living_enemy_count,
        total_enemy_hp,
        total_enemy_block,
        visible_incoming_damage,
        visible_hp_loss,
        visible_survival_margin: player.current_hp.saturating_sub(visible_hp_loss),
    }
}

fn entity_powers(
    combat: &sts_oracle_runtime::runtime::combat::CombatState,
    entity_id: usize,
) -> Vec<Power> {
    combat
        .entities
        .power_db
        .get(&entity_id)
        .cloned()
        .unwrap_or_default()
}

fn state_delta(left: &RouteStateSnapshotV1, right: &RouteStateSnapshotV1) -> RouteStateDeltaV1 {
    RouteStateDeltaV1 {
        player_hp: scalar_delta(left.player.current_hp, right.player.current_hp),
        player_block: scalar_delta(left.player.block, right.player.block),
        energy: scalar_delta(i32::from(left.energy), i32::from(right.energy)),
        living_enemy_count: scalar_delta(
            usize_to_i32(left.living_enemy_count),
            usize_to_i32(right.living_enemy_count),
        ),
        total_enemy_hp: scalar_delta(left.total_enemy_hp, right.total_enemy_hp),
        total_enemy_block: scalar_delta(left.total_enemy_block, right.total_enemy_block),
        visible_incoming_damage: scalar_delta(
            left.visible_incoming_damage,
            right.visible_incoming_damage,
        ),
        visible_hp_loss: scalar_delta(left.visible_hp_loss, right.visible_hp_loss),
        visible_survival_margin: scalar_delta(
            left.visible_survival_margin,
            right.visible_survival_margin,
        ),
        potion_count: scalar_delta(
            usize_to_i32(left.potions.iter().flatten().count()),
            usize_to_i32(right.potions.iter().flatten().count()),
        ),
        hand_count: scalar_delta(
            usize_to_i32(left.zones.hand.len()),
            usize_to_i32(right.zones.hand.len()),
        ),
        draw_count: scalar_delta(
            usize_to_i32(left.zones.draw_pile.len()),
            usize_to_i32(right.zones.draw_pile.len()),
        ),
        discard_count: scalar_delta(
            usize_to_i32(left.zones.discard_pile.len()),
            usize_to_i32(right.zones.discard_pile.len()),
        ),
        exhaust_count: scalar_delta(
            usize_to_i32(left.zones.exhaust_pile.len()),
            usize_to_i32(right.zones.exhaust_pile.len()),
        ),
        player_power_amount_deltas: power_amount_deltas(&left.player.powers, &right.player.powers),
        enemy_power_amount_deltas: enemy_power_amount_deltas(&left.enemies, &right.enemies),
    }
}

fn scalar_delta(route_a: i32, route_b: i32) -> ScalarDeltaV1 {
    ScalarDeltaV1 {
        route_a,
        route_b,
        route_b_minus_route_a: route_b.saturating_sub(route_a),
    }
}

fn power_amount_deltas(left: &[Power], right: &[Power]) -> Vec<RoutePowerAmountDeltaV1> {
    union_power_ids(left, right)
        .into_iter()
        .filter_map(|power_id| {
            let route_a = summed_power_amount(left, power_id);
            let route_b = summed_power_amount(right, power_id);
            (route_a != route_b).then_some(RoutePowerAmountDeltaV1 {
                power_id,
                route_a,
                route_b,
                route_b_minus_route_a: route_b.saturating_sub(route_a),
            })
        })
        .collect()
}

fn enemy_power_amount_deltas(
    left: &[RouteEnemySnapshotV1],
    right: &[RouteEnemySnapshotV1],
) -> Vec<RouteEnemyPowerAmountDeltaV1> {
    let mut slots = left.iter().map(|enemy| enemy.slot).collect::<Vec<_>>();
    for slot in right.iter().map(|enemy| enemy.slot) {
        if !slots.contains(&slot) {
            slots.push(slot);
        }
    }
    slots.sort_unstable();
    slots
        .into_iter()
        .flat_map(|slot| {
            let left_powers = left
                .iter()
                .find(|enemy| enemy.slot == slot)
                .map(|enemy| enemy.powers.as_slice())
                .unwrap_or_default();
            let right_powers = right
                .iter()
                .find(|enemy| enemy.slot == slot)
                .map(|enemy| enemy.powers.as_slice())
                .unwrap_or_default();
            power_amount_deltas(left_powers, right_powers)
                .into_iter()
                .map(move |delta| RouteEnemyPowerAmountDeltaV1 {
                    slot,
                    power_id: delta.power_id,
                    route_a: delta.route_a,
                    route_b: delta.route_b,
                    route_b_minus_route_a: delta.route_b_minus_route_a,
                })
        })
        .collect()
}

fn union_power_ids(left: &[Power], right: &[Power]) -> Vec<PowerId> {
    let mut ids = Vec::new();
    for power_id in left.iter().chain(right).map(|power| power.power_type) {
        if !ids.contains(&power_id) {
            ids.push(power_id);
        }
    }
    ids
}

fn summed_power_amount(powers: &[Power], power_id: PowerId) -> i32 {
    powers
        .iter()
        .filter(|power| power.power_type == power_id)
        .fold(0_i32, |sum, power| sum.saturating_add(power.amount))
}

fn usize_to_i32(value: usize) -> i32 {
    value.try_into().unwrap_or(i32::MAX)
}

fn exact_state_hash(position: &CombatPosition) -> String {
    sts_oracle_runtime::ai::combat_state_key::combat_exact_state_hash_v2(
        &position.engine,
        &position.combat,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_runtime::content::cards::CardId;
    use sts_oracle_runtime::content::monsters::EnemyId;
    use sts_oracle_runtime::runtime::combat::{CombatState, PowerPayload};
    use sts_oracle_runtime::state::core::EngineState;
    use sts_oracle_runtime::test_support::{blank_test_combat, test_monster};

    #[test]
    fn shared_prefix_reports_the_first_exact_action_divergence() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![test_monster(EnemyId::JawWorm)];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let route_a = replay_route(
            &root,
            Vec::new(),
            1,
            Some(1),
            vec![ClientInput::PlayCard {
                card_index: 0,
                target: None,
            }],
            250,
        );
        let route_b = replay_route(
            &root,
            Vec::new(),
            1,
            Some(1),
            vec![ClientInput::PlayCard {
                card_index: 1,
                target: None,
            }],
            250,
        );

        let prefix = shared_prefix(&exact_state_hash(&root), &route_a, &route_b);

        assert_eq!(prefix.action_count, 0);
        assert!(prefix.replay_hashes_match);
        assert_eq!(prefix.first_divergence.unwrap().action_index, 0);
    }

    #[test]
    fn state_delta_keeps_enemy_power_amount_changes_typed() {
        let mut left_combat = combat_with_one_monster();
        left_combat.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: -2,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );
        let right_combat = combat_with_one_monster();
        let left = state_snapshot(&CombatPosition::new(
            EngineState::CombatPlayerTurn,
            left_combat,
        ));
        let right = state_snapshot(&CombatPosition::new(
            EngineState::CombatPlayerTurn,
            right_combat,
        ));

        let delta = state_delta(&left, &right);

        assert_eq!(delta.enemy_power_amount_deltas.len(), 1);
        let strength = delta.enemy_power_amount_deltas[0];
        assert_eq!(strength.slot, 0);
        assert_eq!(strength.power_id, PowerId::Strength);
        assert_eq!(strength.route_a, -2);
        assert_eq!(strength.route_b, 0);
        assert_eq!(strength.route_b_minus_route_a, 2);
    }

    fn combat_with_one_monster() -> CombatState {
        let mut combat = blank_test_combat();
        let mut monster = test_monster(EnemyId::JawWorm);
        monster.id = 1;
        monster.slot = 0;
        combat.entities.monsters = vec![monster];
        combat
    }
}
