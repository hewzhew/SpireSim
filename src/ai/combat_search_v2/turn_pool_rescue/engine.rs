use std::time::{Duration, Instant};

use crate::ai::combat_search_v2::pending_choice_action_prefix::canonical_pending_choice_inputs;
use crate::content::cards::{get_card_definition, CardType};
use crate::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper};
use crate::sim::combat_action::CombatActionChoice;
use crate::state::core::ClientInput;

use super::super::{
    filter_combat_search_legal_actions_with_potion_plugin, CombatSearchPluginStack,
    CombatSearchV2Config, SearchTerminalLabel,
};
use super::ranking::{keep_diverse_lane_nodes, keep_lane_nodes, lane_rank};
use super::types::{
    TurnPoolExpandOutcome, TurnPoolLane, TurnPoolLaneNode, TurnPoolNode, TurnPoolRun,
};

const RESCUE_LANES: [TurnPoolLane; 5] = [
    TurnPoolLane::Damage,
    TurnPoolLane::Survival,
    TurnPoolLane::Setup,
    TurnPoolLane::PowerDelay,
    TurnPoolLane::PotionBurst,
];

const OPENING_DIAGNOSTIC_LANES: [TurnPoolLane; 6] = [
    TurnPoolLane::Damage,
    TurnPoolLane::Survival,
    TurnPoolLane::Setup,
    TurnPoolLane::PowerDelay,
    TurnPoolLane::PotionBurst,
    TurnPoolLane::CultistCleanup,
];

pub(super) fn run_turn_pool_nodes_v0(
    start: &CombatPosition,
    budget_ms: u64,
    config: Option<&CombatSearchV2Config>,
) -> TurnPoolRun {
    run_turn_pool_nodes(start, budget_ms, config, &RESCUE_LANES, 12)
}

pub(super) fn run_turn_pool_opening_nodes_v0(
    start: &CombatPosition,
    budget_ms: u64,
    max_turns: usize,
    config: Option<&CombatSearchV2Config>,
) -> TurnPoolRun {
    run_turn_pool_nodes(
        start,
        budget_ms,
        config,
        &OPENING_DIAGNOSTIC_LANES,
        max_turns.max(1),
    )
}

fn run_turn_pool_nodes(
    start: &CombatPosition,
    budget_ms: u64,
    config: Option<&CombatSearchV2Config>,
    lanes: &[TurnPoolLane],
    max_turns: usize,
) -> TurnPoolRun {
    const LANE_KEEP: usize = 3;
    const INNER_BEAM: usize = 12;
    const MAX_INNER_NODES: usize = 160;

    let stepper = EngineCombatStepper;
    let per_lane_ms = (budget_ms / lanes.len().max(1) as u64).max(500);
    let plugins = config.map(CombatSearchPluginStack::from_config);
    let mut total_nodes = 0u64;
    let mut total_generated = 0u64;
    let mut any_deadline_hit = false;
    let mut lane_results = Vec::new();

    for &lane in lanes {
        let deadline = Instant::now() + Duration::from_millis(per_lane_ms);
        let mut frontier = vec![TurnPoolNode::root(start.clone(), &stepper)];
        let mut lane_deadline_hit = false;
        for _ in 0..max_turns {
            if Instant::now() >= deadline {
                lane_deadline_hit = true;
                break;
            }
            let mut next_turn = Vec::new();
            for node in std::mem::take(&mut frontier) {
                if node.terminal != SearchTerminalLabel::Unresolved {
                    next_turn.push(node);
                    continue;
                }
                let outcome = expand_one_turn(
                    node,
                    lane,
                    &stepper,
                    deadline,
                    INNER_BEAM,
                    MAX_INNER_NODES,
                    config,
                    plugins.as_ref(),
                );
                total_nodes = total_nodes.saturating_add(outcome.nodes_expanded);
                total_generated = total_generated.saturating_add(outcome.nodes_generated);
                lane_deadline_hit |= outcome.deadline_hit;
                next_turn.extend(outcome.nodes);
                if lane_deadline_hit {
                    break;
                }
            }
            if next_turn.is_empty() {
                break;
            }
            keep_diverse_lane_nodes(&mut next_turn, lane, LANE_KEEP);
            let all_terminal = next_turn
                .iter()
                .all(|node| node.terminal != SearchTerminalLabel::Unresolved);
            frontier = next_turn;
            if all_terminal || lane_deadline_hit {
                break;
            }
        }
        any_deadline_hit |= lane_deadline_hit;
        if let Some(best) = frontier
            .into_iter()
            .max_by_key(|node| lane_rank(node, lane))
        {
            lane_results.push(TurnPoolLaneNode { lane, node: best });
        }
    }

    TurnPoolRun {
        lanes: lane_results,
        nodes_expanded: total_nodes,
        nodes_generated: total_generated,
        deadline_hit: any_deadline_hit,
    }
}

fn expand_one_turn(
    root: TurnPoolNode,
    lane: TurnPoolLane,
    stepper: &EngineCombatStepper,
    deadline: Instant,
    beam: usize,
    max_nodes: usize,
    config: Option<&CombatSearchV2Config>,
    plugins: Option<&CombatSearchPluginStack>,
) -> TurnPoolExpandOutcome {
    let start_turn = root.position.combat.turn.turn_count;
    let mut frontier = vec![root];
    let mut boundary = Vec::new();
    let mut nodes_expanded = 0u64;
    let mut nodes_generated = 0u64;
    let mut deadline_hit = false;
    let boundary_limit = beam.saturating_mul(4).max(beam);

    while !frontier.is_empty()
        && nodes_expanded < max_nodes as u64
        && nodes_generated < max_nodes as u64
    {
        if Instant::now() >= deadline {
            deadline_hit = true;
            break;
        }
        let mut next = Vec::new();
        for node in std::mem::take(&mut frontier) {
            if node.terminal != SearchTerminalLabel::Unresolved
                || node.position.combat.turn.turn_count > start_turn
            {
                boundary.push(node);
                continue;
            }
            nodes_expanded = nodes_expanded.saturating_add(1);
            let choices = turn_pool_choices(&node.position, stepper, plugins, node.potions_used);
            for (action_id, choice) in choices.enumerate() {
                if Instant::now() >= deadline {
                    deadline_hit = true;
                    break;
                }
                if nodes_generated >= max_nodes as u64 {
                    break;
                }
                let played_power = match choice.input {
                    ClientInput::PlayCard { card_index, .. } => {
                        is_power_in_hand(&node.position, card_index)
                    }
                    _ => false,
                };
                let step = stepper.apply_to_stable(
                    &node.position,
                    choice.input.clone(),
                    CombatStepLimits {
                        max_engine_steps: config
                            .map(|config| config.max_engine_steps_per_action)
                            .unwrap_or(250),
                        deadline: Some(deadline),
                    },
                );
                if step.truncated || step.timed_out {
                    deadline_hit |= step.timed_out;
                    continue;
                }
                let mut child = node.child(step.position, stepper);
                child.note_action(action_id, choice, played_power);
                nodes_generated = nodes_generated.saturating_add(1);
                if child.terminal != SearchTerminalLabel::Unresolved
                    || child.position.combat.turn.turn_count > start_turn
                {
                    boundary.push(child);
                    if boundary.len() >= boundary_limit {
                        break;
                    }
                } else {
                    next.push(child);
                }
            }
            if deadline_hit
                || nodes_expanded >= max_nodes as u64
                || boundary.len() >= boundary_limit
            {
                break;
            }
        }
        if boundary.len() >= boundary_limit {
            break;
        }
        if !next.is_empty() {
            keep_diverse_lane_nodes(&mut next, lane, beam);
        }
        frontier = next;
        if deadline_hit {
            break;
        }
    }

    if boundary.is_empty() {
        boundary = frontier;
    }
    keep_lane_nodes(&mut boundary, lane, beam);
    TurnPoolExpandOutcome {
        nodes: boundary,
        nodes_expanded,
        nodes_generated,
        deadline_hit,
    }
}

fn turn_pool_choices<'a>(
    position: &'a CombatPosition,
    stepper: &'a EngineCombatStepper,
    plugins: Option<&CombatSearchPluginStack>,
    potions_used: u32,
) -> Box<dyn Iterator<Item = CombatActionChoice> + 'a> {
    if let crate::state::core::EngineState::PendingChoice(choice) = &position.engine {
        if let Some(inputs) = canonical_pending_choice_inputs(choice) {
            return Box::new(
                inputs.filter_map(move |input| stepper.choice_for_legal_input(position, &input)),
            );
        }
    }

    let choices = match plugins {
        Some(plugins) => filter_combat_search_legal_actions_with_potion_plugin(
            stepper.atomic_action_choices(position),
            plugins.potion,
            &position.combat,
        ),
        None => stepper.atomic_action_choices(position),
    };
    Box::new(filter_turn_pool_potion_budget(choices, plugins, potions_used).into_iter())
}

fn filter_turn_pool_potion_budget(
    choices: Vec<CombatActionChoice>,
    plugins: Option<&CombatSearchPluginStack>,
    potions_used: u32,
) -> Vec<CombatActionChoice> {
    let Some(max_potions) = plugins.and_then(|plugins| plugins.potion.max_potions_used) else {
        return choices;
    };
    if potions_used < max_potions {
        return choices;
    }
    choices
        .into_iter()
        .filter(|choice| !matches!(choice.input, ClientInput::UsePotion { .. }))
        .collect()
}

fn is_power_in_hand(position: &CombatPosition, card_index: usize) -> bool {
    position
        .combat
        .zones
        .hand
        .get(card_index)
        .is_some_and(|card| get_card_definition(card.id).card_type == CardType::Power)
}

#[cfg(test)]
mod tests {
    use super::{TurnPoolLane, OPENING_DIAGNOSTIC_LANES, RESCUE_LANES};

    #[test]
    fn cultist_cleanup_lane_is_review_only() {
        assert!(!RESCUE_LANES.contains(&TurnPoolLane::CultistCleanup));
        assert!(OPENING_DIAGNOSTIC_LANES.contains(&TurnPoolLane::CultistCleanup));
    }
}
