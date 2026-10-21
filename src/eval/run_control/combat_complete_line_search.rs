use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::ai::combat_search_v2::pending_choice_action_prefix::canonical_pending_choice_inputs;
use crate::ai::combat_search_v2::{
    filter_combat_search_legal_actions, CombatSearchV2ActionTrace, CombatSearchV2Config,
};
use crate::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use crate::sim::combat_action::CombatActionChoice;
use crate::state::core::ClientInput;

use super::combat_complete_line_scoring::{classify_lane, score_position, LineLane};

pub(super) const LINE_BEAM: usize = 128;

#[derive(Clone)]
pub(super) struct Line {
    pub(super) position: CombatPosition,
    pub(super) actions: Vec<CombatSearchV2ActionTrace>,
    pub(super) terminal: CombatTerminal,
    pub(super) score: i64,
    pub(super) lane: LineLane,
    pub(super) setup_seen: bool,
}

#[derive(Clone, Copy)]
pub(super) struct LineSearchConfig {
    pub(super) nodes: usize,
    pub(super) ms: u64,
    pub(super) beam: usize,
    pub(super) max_actions: usize,
}

pub(super) struct LineSearchRun {
    pub(super) best_win: Option<Line>,
    pub(super) nodes_expanded: usize,
    pub(super) nodes_generated: usize,
    pub(super) stop_reason: LineSearchStopReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LineSearchStopReason {
    FrontierEmpty,
    NodeBudget,
    GeneratedBudget,
    Deadline,
}

impl LineSearchStopReason {
    pub(super) fn label(self) -> &'static str {
        match self {
            LineSearchStopReason::FrontierEmpty => "frontier_empty",
            LineSearchStopReason::NodeBudget => "node_budget",
            LineSearchStopReason::GeneratedBudget => "generated_budget",
            LineSearchStopReason::Deadline => "deadline",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct LineSearchSeed {
    lane: LineLane,
    setup_seen: bool,
}

impl LineSearchSeed {
    pub(super) fn root() -> Self {
        Self {
            lane: LineLane::Root,
            setup_seen: false,
        }
    }

    pub(super) fn from_prefix(setup_seen: bool) -> Self {
        Self {
            lane: if setup_seen {
                LineLane::SetupPath
            } else {
                LineLane::Root
            },
            setup_seen,
        }
    }
}

pub(super) fn line_search_from(
    start_position: CombatPosition,
    initial_hp: i32,
    search: LineSearchConfig,
    config: &CombatSearchV2Config,
    stepper: &EngineCombatStepper,
    seed: LineSearchSeed,
) -> LineSearchRun {
    let deadline = Instant::now() + Duration::from_millis(search.ms);
    let mut frontier = vec![line_from(
        start_position,
        Vec::new(),
        initial_hp,
        seed.lane,
        seed.setup_seen,
        stepper,
    )];
    let mut best_win = None;
    let mut nodes_expanded = 0usize;
    let mut nodes_generated = 0usize;

    while !frontier.is_empty()
        && nodes_expanded < search.nodes
        && nodes_generated < search.nodes
        && Instant::now() < deadline
    {
        let mut next = Vec::new();
        for line in frontier.drain(..) {
            if nodes_expanded >= search.nodes
                || nodes_generated >= search.nodes
                || Instant::now() >= deadline
            {
                break;
            }
            if line.terminal != CombatTerminal::Unresolved
                || line.actions.len() >= search.max_actions
            {
                remember_win(&mut best_win, line);
                continue;
            }
            nodes_expanded += 1;
            for (action_id, choice) in
                ordered_line_choices(&line.position, config, stepper).enumerate()
            {
                if nodes_generated >= search.nodes || Instant::now() >= deadline {
                    break;
                }
                let step = stepper.apply_to_stable(
                    &line.position,
                    choice.input.clone(),
                    CombatStepLimits {
                        max_engine_steps: config.max_engine_steps_per_action,
                        deadline: Some(deadline),
                    },
                );
                if step.truncated || step.timed_out {
                    continue;
                }
                let mut actions = line.actions.clone();
                actions.push(CombatSearchV2ActionTrace {
                    step_index: actions.len(),
                    action_id,
                    action_key: choice.action_key,
                    action_debug: choice.action_debug,
                    input: choice.input.clone(),
                });
                let lane = classify_lane(&line.position, &step.position, &choice.input);
                let setup_seen = line.setup_seen || lane == LineLane::Setup;
                let child_lane = if lane == LineLane::Setup {
                    LineLane::Setup
                } else if line.setup_seen {
                    LineLane::SetupPath
                } else {
                    lane
                };
                let child = line_from(
                    step.position,
                    actions,
                    initial_hp,
                    child_lane,
                    setup_seen,
                    stepper,
                );
                nodes_generated += 1;
                match child.terminal {
                    CombatTerminal::Win => remember_win(&mut best_win, child),
                    CombatTerminal::Unresolved => next.push(child),
                    CombatTerminal::Loss => {}
                }
                if nodes_generated >= search.nodes || Instant::now() >= deadline {
                    break;
                }
            }
        }
        frontier = keep_lane_frontier(next, search.beam);
    }

    LineSearchRun {
        best_win,
        nodes_expanded,
        nodes_generated,
        stop_reason: line_search_stop_reason(&frontier, nodes_expanded, nodes_generated, search),
    }
}

fn ordered_line_choices<'a>(
    position: &'a CombatPosition,
    config: &CombatSearchV2Config,
    stepper: &'a EngineCombatStepper,
) -> Box<dyn Iterator<Item = CombatActionChoice> + 'a> {
    if let crate::state::core::EngineState::PendingChoice(choice) = &position.engine {
        if let Some(inputs) = canonical_pending_choice_inputs(choice) {
            return Box::new(inputs.filter_map(move |input| {
                stepper
                    .choice_for_legal_input(position, &input)
                    .filter(|choice| {
                        !matches!(
                            choice.input,
                            ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
                        )
                    })
            }));
        }
    }

    let mut choices = legal_non_potion_choices(position, config, stepper);
    order_choices(&mut choices);
    Box::new(choices.into_iter())
}

fn line_search_stop_reason(
    frontier: &[Line],
    nodes_expanded: usize,
    nodes_generated: usize,
    search: LineSearchConfig,
) -> LineSearchStopReason {
    if frontier.is_empty() {
        LineSearchStopReason::FrontierEmpty
    } else if nodes_generated >= search.nodes {
        LineSearchStopReason::GeneratedBudget
    } else if nodes_expanded >= search.nodes {
        LineSearchStopReason::NodeBudget
    } else {
        LineSearchStopReason::Deadline
    }
}

pub(super) fn line_from(
    position: CombatPosition,
    actions: Vec<CombatSearchV2ActionTrace>,
    initial_hp: i32,
    lane: LineLane,
    setup_seen: bool,
    stepper: &EngineCombatStepper,
) -> Line {
    let terminal = stepper.terminal(&position);
    let score = score_position(&position, terminal, initial_hp, actions.len());
    Line {
        position,
        actions,
        terminal,
        score,
        lane,
        setup_seen,
    }
}

pub(super) fn legal_non_potion_choices(
    position: &CombatPosition,
    config: &CombatSearchV2Config,
    stepper: &EngineCombatStepper,
) -> Vec<CombatActionChoice> {
    filter_combat_search_legal_actions(
        stepper.atomic_action_choices(position),
        config.potion_policy,
        config.allowed_potion_slots,
        &position.combat,
    )
    .into_iter()
    .filter(|choice| {
        !matches!(
            choice.input,
            ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
        )
    })
    .collect()
}

pub(super) fn reindex_actions(
    mut actions: Vec<CombatSearchV2ActionTrace>,
) -> Vec<CombatSearchV2ActionTrace> {
    for (index, action) in actions.iter_mut().enumerate() {
        action.step_index = index;
    }
    actions
}

fn keep_lane_frontier(mut lines: Vec<Line>, beam: usize) -> Vec<Line> {
    lines.sort_by(|a, b| b.score.cmp(&a.score));
    let per_lane = (beam / 5).max(4);
    let mut kept = Vec::new();
    let mut counts: HashMap<LineLane, usize> = HashMap::new();
    let mut rest = Vec::new();
    for line in lines {
        let count = counts.entry(line.lane).or_default();
        if *count < per_lane && kept.len() < beam {
            *count += 1;
            kept.push(line);
        } else {
            rest.push(line);
        }
    }
    kept.extend(rest.into_iter().take(beam.saturating_sub(kept.len())));
    kept.sort_by(|a, b| b.score.cmp(&a.score));
    kept
}

fn remember_win(best: &mut Option<Line>, line: Line) {
    if line.terminal == CombatTerminal::Win
        && best
            .as_ref()
            .map(|current| line.score > current.score)
            .unwrap_or(true)
    {
        *best = Some(line);
    }
}

fn order_choices(choices: &mut [CombatActionChoice]) {
    choices.sort_by_key(|choice| match choice.input {
        ClientInput::PlayCard { .. } => 0,
        ClientInput::SubmitSelection(_) | ClientInput::SubmitDiscoverChoice(_) => 1,
        ClientInput::EndTurn => 2,
        _ => 3,
    });
}
