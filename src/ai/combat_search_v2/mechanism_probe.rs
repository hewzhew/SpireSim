use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

use serde::Serialize;

use crate::content::cards::{get_card_definition, CardType};
use crate::content::powers::PowerId;
use crate::runtime::combat::CombatState;
use crate::sim::combat::EngineCombatStepper;
use crate::state::core::EngineState;

use super::frontier::SearchNode;
use super::turn_planner::{
    enumerate_turn_plans, TurnPlanBucket, TurnPlanStopReason, TurnPlannerConfigV1,
};
use super::{combat_search_exact_state_hash_v1, summarize_state, SearchTerminalLabel};

pub const COMBAT_MECHANISM_HORIZON_PROBE_SCHEMA_NAME: &str = "CombatMechanismHorizonProbe";
pub const COMBAT_MECHANISM_HORIZON_PROBE_SCHEMA_VERSION: u32 = 1;

/// Bounded finite-horizon diagnostic configuration. It deliberately exposes
/// no scalar objective: the caller receives a diverse set of exact endpoint
/// states and decides which capability question those endpoints can answer.
#[derive(Clone, Copy, Debug)]
pub struct CombatMechanismHorizonProbeConfigV1 {
    pub horizon_turns: u32,
    pub max_active_states_per_depth: usize,
    pub max_inner_nodes_per_turn: usize,
    pub max_end_states_per_turn: usize,
    pub per_bucket_limit: usize,
    pub max_engine_steps_per_action: usize,
}

impl Default for CombatMechanismHorizonProbeConfigV1 {
    fn default() -> Self {
        Self {
            horizon_turns: 2,
            max_active_states_per_depth: 64,
            max_inner_nodes_per_turn: 512,
            max_end_states_per_turn: 24,
            per_bucket_limit: 4,
            max_engine_steps_per_action: 250,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatMechanismHorizonProbeReportV1 {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub evidence_policy: &'static str,
    pub requested_horizon_turns: u32,
    pub elapsed_us: u128,
    pub initial: CombatMechanismEndpointStateV1,
    pub depths: Vec<CombatMechanismDepthReportV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatMechanismDepthReportV1 {
    pub depth_turns: u32,
    pub source_state_count: usize,
    pub turn_planner_nodes_expanded: usize,
    pub turn_planner_nodes_generated: usize,
    pub turn_planner_preselection_endpoints: usize,
    pub turn_planner_selected_endpoints: usize,
    pub turn_planner_exact_state_skips: usize,
    pub engine_truncated_children: usize,
    pub duplicate_endpoint_states: usize,
    pub non_turn_boundary_endpoints: usize,
    pub state_cap_dropped: usize,
    pub endpoints: Vec<CombatMechanismEndpointV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatMechanismEndpointV1 {
    pub stop_reason: &'static str,
    pub diversity_bucket: &'static str,
    pub action_count: usize,
    pub action_keys: Vec<String>,
    pub hp_loss: i32,
    pub total_enemy_hp_delta: i32,
    pub living_enemy_delta: i32,
    pub status_or_curse_delta: i32,
    pub state: CombatMechanismEndpointStateV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatMechanismEndpointStateV1 {
    pub exact_state_hash: String,
    pub terminal: SearchTerminalLabel,
    pub turn: u32,
    pub player_hp: i32,
    pub player_max_hp: i32,
    pub player_block: i32,
    pub energy: u8,
    pub living_enemy_count: usize,
    pub total_enemy_hp: i32,
    pub visible_incoming_damage: i32,
    pub hand_count: usize,
    pub draw_count: usize,
    pub discard_count: usize,
    pub exhaust_count: usize,
    pub status_or_curse_count: usize,
    pub powers: Vec<CombatMechanismPowerObservationV1>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatMechanismPowerObservationV1 {
    pub entity_id: usize,
    pub power: PowerId,
    pub amount: i32,
}

#[derive(Clone)]
struct HorizonCandidate {
    node: SearchNode,
    stop_reason: TurnPlanStopReason,
    bucket: TurnPlanBucket,
}

pub fn run_combat_mechanism_horizon_probe_v1(
    engine: &EngineState,
    combat: &CombatState,
    config: CombatMechanismHorizonProbeConfigV1,
) -> CombatMechanismHorizonProbeReportV1 {
    let started = Instant::now();
    let initial = endpoint_state(engine, combat);
    let initial_hp = initial.player_hp;
    let initial_enemy_hp = initial.total_enemy_hp;
    let initial_living_enemies = initial.living_enemy_count;
    let initial_pollution = initial.status_or_curse_count;
    let mut active = vec![SearchNode::root(engine.clone(), combat.clone())];
    let mut depths = Vec::new();

    for depth_turns in 1..=config.horizon_turns {
        if active.is_empty() {
            break;
        }
        let source_state_count = active.len();
        let mut report = CombatMechanismDepthReportV1 {
            depth_turns,
            source_state_count,
            turn_planner_nodes_expanded: 0,
            turn_planner_nodes_generated: 0,
            turn_planner_preselection_endpoints: 0,
            turn_planner_selected_endpoints: 0,
            turn_planner_exact_state_skips: 0,
            engine_truncated_children: 0,
            duplicate_endpoint_states: 0,
            non_turn_boundary_endpoints: 0,
            state_cap_dropped: 0,
            endpoints: Vec::new(),
        };
        let mut candidates = Vec::new();

        for source in std::mem::take(&mut active) {
            let enumeration = enumerate_turn_plans(
                &source,
                &EngineCombatStepper,
                &TurnPlannerConfigV1 {
                    max_inner_nodes: config.max_inner_nodes_per_turn,
                    max_partial_frontier_states: None,
                    max_end_states: config.max_end_states_per_turn,
                    per_bucket_limit: config.per_bucket_limit,
                    potion_policy: super::CombatSearchV2PotionPolicy::Never,
                    max_potions_used: Some(0),
                    allowed_potion_slots: None,
                    allow_potion_discard: None,
                    max_engine_steps_per_action: config.max_engine_steps_per_action,
                    turn_plan_prior: None,
                    capture_step_trace: false,
                },
                None,
            );
            report.turn_planner_nodes_expanded = report
                .turn_planner_nodes_expanded
                .saturating_add(enumeration.nodes_expanded);
            report.turn_planner_nodes_generated = report
                .turn_planner_nodes_generated
                .saturating_add(enumeration.nodes_generated);
            report.turn_planner_preselection_endpoints = report
                .turn_planner_preselection_endpoints
                .saturating_add(enumeration.preselection_plan_count);
            report.turn_planner_selected_endpoints = report
                .turn_planner_selected_endpoints
                .saturating_add(enumeration.plans.len());
            report.turn_planner_exact_state_skips = report
                .turn_planner_exact_state_skips
                .saturating_add(enumeration.exact_state_skips);
            report.engine_truncated_children = report
                .engine_truncated_children
                .saturating_add(enumeration.truncated_children);

            candidates.extend(enumeration.plans.into_iter().map(|plan| HorizonCandidate {
                node: plan.end_node,
                stop_reason: plan.stop_reason,
                bucket: plan.bucket,
            }));
        }

        let mut seen = HashSet::new();
        candidates.retain(|candidate| {
            let inserted = seen.insert(combat_search_exact_state_hash_v1(
                &candidate.node.engine,
                &candidate.node.combat,
            ));
            if !inserted {
                report.duplicate_endpoint_states =
                    report.duplicate_endpoint_states.saturating_add(1);
            }
            inserted
        });
        let before_cap = candidates.len();
        candidates = stratified_cap(candidates, config.max_active_states_per_depth);
        report.state_cap_dropped = before_cap.saturating_sub(candidates.len());

        report.endpoints = candidates
            .iter()
            .map(|candidate| {
                endpoint_report(
                    candidate,
                    initial_hp,
                    initial_enemy_hp,
                    initial_living_enemies,
                    initial_pollution,
                )
            })
            .collect();

        if depth_turns < config.horizon_turns {
            active = candidates
                .into_iter()
                .filter_map(|candidate| {
                    if candidate.stop_reason == TurnPlanStopReason::NextTurn
                        && super::terminal_label(&candidate.node.engine, &candidate.node.combat)
                            == SearchTerminalLabel::Unresolved
                    {
                        Some(candidate.node)
                    } else {
                        report.non_turn_boundary_endpoints =
                            report.non_turn_boundary_endpoints.saturating_add(1);
                        None
                    }
                })
                .collect();
        }
        depths.push(report);
    }

    CombatMechanismHorizonProbeReportV1 {
        schema_name: COMBAT_MECHANISM_HORIZON_PROBE_SCHEMA_NAME,
        schema_version: COMBAT_MECHANISM_HORIZON_PROBE_SCHEMA_VERSION,
        evidence_policy:
            "bounded_exact_transitions_diverse_endpoint_surface_no_scalar_or_whole_combat_claim",
        requested_horizon_turns: config.horizon_turns,
        elapsed_us: started.elapsed().as_micros(),
        initial,
        depths,
    }
}

fn stratified_cap(candidates: Vec<HorizonCandidate>, cap: usize) -> Vec<HorizonCandidate> {
    if candidates.len() <= cap {
        return candidates;
    }
    let mut groups = BTreeMap::<TurnPlanBucket, Vec<HorizonCandidate>>::new();
    for candidate in candidates {
        groups.entry(candidate.bucket).or_default().push(candidate);
    }
    let mut selected = Vec::with_capacity(cap);
    let mut ordinal = 0usize;
    while selected.len() < cap {
        let before = selected.len();
        for group in groups.values() {
            if selected.len() >= cap {
                break;
            }
            if let Some(candidate) = group.get(ordinal) {
                selected.push(candidate.clone());
            }
        }
        if selected.len() == before {
            break;
        }
        ordinal = ordinal.saturating_add(1);
    }
    selected
}

fn endpoint_report(
    candidate: &HorizonCandidate,
    initial_hp: i32,
    initial_enemy_hp: i32,
    initial_living_enemies: usize,
    initial_pollution: usize,
) -> CombatMechanismEndpointV1 {
    let state = endpoint_state(&candidate.node.engine, &candidate.node.combat);
    CombatMechanismEndpointV1 {
        stop_reason: candidate.stop_reason.label(),
        diversity_bucket: candidate.bucket.label(),
        action_count: candidate.node.actions.len(),
        action_keys: candidate
            .node
            .actions
            .iter()
            .map(|action| action.action_key.clone())
            .collect(),
        hp_loss: initial_hp.saturating_sub(state.player_hp),
        total_enemy_hp_delta: initial_enemy_hp.saturating_sub(state.total_enemy_hp),
        living_enemy_delta: (initial_living_enemies as i32)
            .saturating_sub(state.living_enemy_count as i32),
        status_or_curse_delta: (state.status_or_curse_count as i32)
            .saturating_sub(initial_pollution as i32),
        state,
    }
}

fn endpoint_state(engine: &EngineState, combat: &CombatState) -> CombatMechanismEndpointStateV1 {
    let summary = summarize_state(engine, combat);
    let mut powers = combat
        .entities
        .power_db
        .iter()
        .flat_map(|(entity_id, powers)| {
            powers
                .iter()
                .map(|power| CombatMechanismPowerObservationV1 {
                    entity_id: *entity_id,
                    power: power.power_type,
                    amount: power.amount,
                })
        })
        .collect::<Vec<_>>();
    powers.sort_by_key(|power| (power.entity_id, power.power.stable_ordinal()));

    CombatMechanismEndpointStateV1 {
        exact_state_hash: combat_search_exact_state_hash_v1(engine, combat),
        terminal: summary.terminal,
        turn: summary.turn_count,
        player_hp: summary.player_hp,
        player_max_hp: combat.entities.player.max_hp,
        player_block: summary.player_block,
        energy: summary.energy,
        living_enemy_count: summary.living_enemy_count,
        total_enemy_hp: summary.total_enemy_hp,
        visible_incoming_damage: summary.visible_incoming_damage,
        hand_count: summary.hand_count,
        draw_count: summary.draw_count,
        discard_count: summary.discard_count,
        exhaust_count: summary.exhaust_count,
        status_or_curse_count: status_or_curse_count(combat),
        powers,
    }
}

fn status_or_curse_count(combat: &CombatState) -> usize {
    combat
        .zones
        .hand
        .iter()
        .chain(&combat.zones.draw_pile)
        .chain(&combat.zones.discard_pile)
        .chain(&combat.zones.exhaust_pile)
        .filter(|card| {
            matches!(
                get_card_definition(card.id).card_type,
                CardType::Status | CardType::Curse
            )
        })
        .count()
}
