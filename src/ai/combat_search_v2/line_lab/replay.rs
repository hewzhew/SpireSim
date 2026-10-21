use crate::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use crate::state::core::ClientInput;

use super::super::{
    CombatSearchV2ActionTrace, CombatSearchV2TrajectoryReport, SearchTerminalLabel,
};
use super::types::{CombatLineLabLineSummary, PrefixReplay, ReplaySummary};

pub(super) fn replay_actions(
    start: &CombatPosition,
    actions: &[CombatSearchV2ActionTrace],
    stepper: &EngineCombatStepper,
) -> Option<ReplaySummary> {
    let mut position = start.clone();
    for action in actions {
        position = replay_one(&position, action, stepper)?;
    }
    Some(ReplaySummary {
        terminal: search_terminal(stepper.terminal(&position)),
        final_hp: position.combat.entities.player.current_hp,
        total_enemy_hp: total_enemy_hp(&position),
        living_enemy_count: living_enemy_count(&position),
    })
}

pub(super) fn replay_prefix(
    start: &CombatPosition,
    actions: &[CombatSearchV2ActionTrace],
    stepper: &EngineCombatStepper,
) -> Option<PrefixReplay> {
    let mut position = start.clone();
    let mut potions_used = 0;
    for action in actions {
        if matches!(action.input, ClientInput::UsePotion { .. }) {
            potions_used += 1;
        }
        position = replay_one(&position, action, stepper)?;
    }
    Some(PrefixReplay {
        position,
        replayed_actions: actions.len(),
        potions_used,
    })
}

pub(super) fn replay_one(
    position: &CombatPosition,
    action: &CombatSearchV2ActionTrace,
    stepper: &EngineCombatStepper,
) -> Option<CombatPosition> {
    let choice = stepper.choice_for_legal_input(position, &action.input)?;
    if choice.action_key != action.action_key {
        return None;
    }
    let step = stepper.apply_to_stable(
        position,
        choice.input,
        CombatStepLimits {
            max_engine_steps: 250,
            deadline: None,
        },
    );
    if step.truncated || step.timed_out {
        return None;
    }
    Some(step.position)
}

pub(super) fn line_summary(
    source: &'static str,
    trajectory: &CombatSearchV2TrajectoryReport,
) -> CombatLineLabLineSummary {
    CombatLineLabLineSummary {
        source,
        terminal: trajectory.terminal,
        final_hp: trajectory.final_hp,
        total_enemy_hp: trajectory.final_state.total_enemy_hp,
        living_enemy_count: trajectory.final_state.living_enemy_count,
        turns: trajectory.turns,
        actions: trajectory.actions.len(),
        potions_used: trajectory.potions_used,
    }
}

fn search_terminal(terminal: CombatTerminal) -> SearchTerminalLabel {
    match terminal {
        CombatTerminal::Win => SearchTerminalLabel::Win,
        CombatTerminal::Loss => SearchTerminalLabel::Loss,
        CombatTerminal::Unresolved => SearchTerminalLabel::Unresolved,
    }
}

fn living_enemy_count(position: &CombatPosition) -> usize {
    position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .count()
}

fn total_enemy_hp(position: &CombatPosition) -> i32 {
    position
        .combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster.current_hp.max(0) + monster.block.max(0))
        .sum()
}
