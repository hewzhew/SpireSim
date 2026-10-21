use crate::ai::combat_search_v2::{
    filter_combat_search_legal_actions, CombatSearchV2ActionTrace, CombatSearchV2Config,
};
use crate::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};

use super::combat_complete_line_scoring::played_power;
use super::combat_complete_line_search::{
    line_from, line_search_from, reindex_actions, Line, LineSearchConfig, LineSearchSeed,
    LineSearchStopReason, LINE_BEAM,
};

const REPAIR_CUTS: usize = 4;
const REPAIR_NODES: usize = 8_000;
const REPAIR_MS: u64 = 500;
const MIN_REPAIR_HP_LOSS: i32 = 8;
const MIN_REPAIR_ACTIONS: usize = 24;

#[derive(Clone, Copy)]
pub(super) struct LineRepairBudget {
    pub(super) nodes_per_cut: usize,
    pub(super) ms_per_cut: u64,
    pub(super) cuts: usize,
}

impl LineRepairBudget {
    pub(super) fn default_budget() -> Self {
        Self {
            nodes_per_cut: REPAIR_NODES,
            ms_per_cut: REPAIR_MS,
            cuts: REPAIR_CUTS,
        }
    }

    fn search(self, max_actions: usize) -> LineSearchConfig {
        LineSearchConfig {
            nodes: self.nodes_per_cut,
            ms: self.ms_per_cut,
            beam: LINE_BEAM,
            max_actions,
        }
    }
}

#[derive(Default)]
pub(super) struct RepairStats {
    pub(super) attempts: usize,
    pub(super) wins: usize,
    pub(super) improvements: usize,
    pub(super) nodes_expanded: usize,
    pub(super) nodes_generated: usize,
    pub(super) last_stop_reason: Option<LineSearchStopReason>,
}

struct ReplayedPrefix {
    position: CombatPosition,
    setup_seen: bool,
}

pub(super) fn repair_line_if_useful(
    root: &CombatPosition,
    mut best: Line,
    initial_hp: i32,
    budget: LineRepairBudget,
    config: &CombatSearchV2Config,
    stepper: &EngineCombatStepper,
) -> (Line, RepairStats) {
    let mut stats = RepairStats::default();
    if !should_repair_line(&best, initial_hp) {
        return (best, stats);
    }
    let repair_base = best.clone();
    for cut in repair_cut_points(repair_base.actions.len(), budget.cuts) {
        let cut = cut.min(repair_base.actions.len());
        let remaining_actions = config.max_actions_per_line.saturating_sub(cut);
        if remaining_actions == 0 {
            continue;
        }
        stats.attempts += 1;
        let Some(prefix) = replay_prefix(root, &repair_base.actions[..cut], config, stepper) else {
            continue;
        };
        let repair_run = line_search_from(
            prefix.position,
            initial_hp,
            budget.search(remaining_actions),
            config,
            stepper,
            LineSearchSeed::from_prefix(prefix.setup_seen),
        );
        stats.nodes_expanded += repair_run.nodes_expanded;
        stats.nodes_generated += repair_run.nodes_generated;
        stats.last_stop_reason = Some(repair_run.stop_reason);
        let Some(suffix_win) = repair_run.best_win else {
            continue;
        };
        stats.wins += 1;
        let candidate = splice_line(&repair_base, cut, suffix_win, initial_hp, stepper);
        if candidate.score > best.score {
            best = candidate;
            stats.improvements += 1;
        }
    }
    (best, stats)
}

fn should_repair_line(line: &Line, initial_hp: i32) -> bool {
    if line.terminal != CombatTerminal::Win {
        return false;
    }
    let hp_loss = (initial_hp - line.position.combat.entities.player.current_hp).max(0);
    hp_loss >= MIN_REPAIR_HP_LOSS || line.actions.len() >= MIN_REPAIR_ACTIONS
}

fn repair_cut_points(len: usize, limit: usize) -> Vec<usize> {
    let count = len.min(limit);
    (0..count).map(|index| index * len / count).collect()
}

fn replay_prefix(
    root: &CombatPosition,
    actions: &[CombatSearchV2ActionTrace],
    config: &CombatSearchV2Config,
    stepper: &EngineCombatStepper,
) -> Option<ReplayedPrefix> {
    let mut position = root.clone();
    let mut setup_seen = false;
    for action in actions {
        let candidate = stepper.choice_for_legal_input(&position, &action.input)?;
        let choice = filter_combat_search_legal_actions(
            vec![candidate],
            config.potion_policy,
            config.allowed_potion_slots,
            &position.combat,
        )
        .into_iter()
        .filter(|choice| {
            !matches!(
                choice.input,
                crate::state::core::ClientInput::UsePotion { .. }
                    | crate::state::core::ClientInput::DiscardPotion(_)
            )
        })
        .find(|choice| choice.action_key == action.action_key)?;
        setup_seen |= played_power(&position, &choice.input);
        let step = stepper.apply_to_stable(
            &position,
            choice.input,
            CombatStepLimits {
                max_engine_steps: config.max_engine_steps_per_action,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return None;
        }
        position = step.position;
        if stepper.terminal(&position) != CombatTerminal::Unresolved {
            break;
        }
    }
    Some(ReplayedPrefix {
        position,
        setup_seen,
    })
}

fn splice_line(
    prefix: &Line,
    cut: usize,
    suffix: Line,
    initial_hp: i32,
    stepper: &EngineCombatStepper,
) -> Line {
    let mut actions = prefix.actions[..cut].to_vec();
    actions.extend(suffix.actions);
    line_from(
        suffix.position,
        reindex_actions(actions),
        initial_hp,
        suffix.lane,
        prefix.setup_seen || suffix.setup_seen,
        stepper,
    )
}
