use crate::ai::combat_search_v2::{
    filter_combat_search_legal_actions, CombatSearchV2ActionTrace, CombatSearchV2Config,
    CombatSearchV2TrajectoryReport, SearchTerminalLabel,
};
use crate::sim::combat::{
    combat_terminal, CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal,
    EngineCombatStepper,
};
use crate::state::core::ClientInput;

use super::view_model::client_input_hint;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CombatCandidateLineSource {
    SearchComplete,
    CompleteLineSolver,
    TurnPlanRescue,
    TurnPoolRescue,
}

impl CombatCandidateLineSource {
    pub(super) fn label(self) -> &'static str {
        match self {
            CombatCandidateLineSource::SearchComplete => "search_complete",
            CombatCandidateLineSource::CompleteLineSolver => "complete_line_solver",
            CombatCandidateLineSource::TurnPlanRescue => "turn_plan_rescue",
            CombatCandidateLineSource::TurnPoolRescue => "turn_pool_rescue",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CombatCandidateLineAssumption {
    ExactKnownDrawOrder,
}

impl CombatCandidateLineAssumption {
    pub(super) fn label(self) -> &'static str {
        match self {
            CombatCandidateLineAssumption::ExactKnownDrawOrder => "exact_known_draw_order",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct CombatCandidateLine {
    pub source: CombatCandidateLineSource,
    pub assumptions: Vec<CombatCandidateLineAssumption>,
    pub actions: Vec<CombatSearchV2ActionTrace>,
    pub terminal: CombatTerminal,
    pub final_hp: i32,
    pub hp_loss: i32,
    pub turns: u32,
    pub potions_used: u32,
    pub potions_discarded: u32,
    pub cards_played: u32,
}

pub(super) struct CombatLineReplayResult {
    pub line: CombatCandidateLine,
    pub applied_count: usize,
}

impl CombatCandidateLine {
    pub(super) fn try_from_search_trajectory(
        trajectory: &CombatSearchV2TrajectoryReport,
    ) -> Result<Self, String> {
        Ok(Self {
            source: CombatCandidateLineSource::SearchComplete,
            assumptions: default_assumptions(),
            actions: trajectory.actions.clone(),
            terminal: combat_terminal_from_search_label(trajectory.terminal)?,
            final_hp: trajectory.final_hp,
            hp_loss: trajectory.hp_loss,
            turns: trajectory.turns,
            potions_used: trajectory.potions_used,
            potions_discarded: trajectory.potions_discarded,
            cards_played: trajectory.cards_played,
        })
    }

    pub(super) fn from_position(
        source: CombatCandidateLineSource,
        actions: Vec<CombatSearchV2ActionTrace>,
        initial_hp: i32,
        position: &CombatPosition,
    ) -> Self {
        let final_hp = position.combat.entities.player.current_hp;
        Self {
            source,
            assumptions: default_assumptions(),
            turns: position.combat.turn.turn_count,
            terminal: combat_terminal(&position.engine, &position.combat),
            final_hp,
            hp_loss: (initial_hp - final_hp).max(0),
            potions_used: count_potion_uses(&actions),
            potions_discarded: count_potion_discards(&actions),
            cards_played: count_card_plays(&actions),
            actions,
        }
    }

    pub(super) fn assumption_labels(&self) -> Vec<&'static str> {
        self.assumptions
            .iter()
            .map(|assumption| assumption.label())
            .collect()
    }
}

pub(super) fn replay_candidate_line(
    start: &CombatPosition,
    source: CombatCandidateLineSource,
    actions: &[CombatSearchV2ActionTrace],
    config: &CombatSearchV2Config,
) -> Result<CombatLineReplayResult, String> {
    let stepper = EngineCombatStepper;
    let initial_hp = start.combat.entities.player.current_hp;
    let mut position = start.clone();
    let mut replayed = Vec::new();
    let mut potions_used = 0u32;
    for action in actions {
        let Some(candidate) = stepper.choice_for_legal_input(&position, &action.input) else {
            return Err(format!(
                "combat candidate line replay drift at step {}: expected {} ({})",
                action.step_index,
                action.action_key,
                client_input_hint(&action.input)
            ));
        };
        let choices = enforce_replay_potion_budget(
            filter_combat_search_legal_actions(
                vec![candidate],
                config.potion_policy,
                config.allowed_potion_slots,
                &position.combat,
            ),
            config,
            potions_used,
        );
        let Some(choice) = choices
            .into_iter()
            .find(|choice| choice.input == action.input && choice.action_key == action.action_key)
        else {
            return Err(format!(
                "combat candidate line replay drift at step {}: expected {} ({})",
                action.step_index,
                action.action_key,
                client_input_hint(&action.input)
            ));
        };
        let step = stepper.apply_to_stable(
            &position,
            choice.input.clone(),
            CombatStepLimits {
                max_engine_steps: config.max_engine_steps_per_action,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(format!(
                "combat candidate line replay stopped at step {} truncated={} timed_out={} engine_steps={}",
                action.step_index, step.truncated, step.timed_out, step.engine_steps
            ));
        }
        position = step.position;
        if matches!(choice.input, ClientInput::UsePotion { .. }) {
            potions_used = potions_used.saturating_add(1);
        }
        replayed.push(action.clone());
    }
    let applied_count = replayed.len();
    Ok(CombatLineReplayResult {
        line: CombatCandidateLine::from_position(source, replayed, initial_hp, &position),
        applied_count,
    })
}

pub(super) fn enforce_replay_potion_budget(
    choices: Vec<crate::sim::combat_action::CombatActionChoice>,
    config: &CombatSearchV2Config,
    potions_used: u32,
) -> Vec<crate::sim::combat_action::CombatActionChoice> {
    let Some(max_potions) = config.max_potions_used else {
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

fn default_assumptions() -> Vec<CombatCandidateLineAssumption> {
    vec![CombatCandidateLineAssumption::ExactKnownDrawOrder]
}

fn combat_terminal_from_search_label(
    terminal: SearchTerminalLabel,
) -> Result<CombatTerminal, String> {
    match terminal {
        SearchTerminalLabel::Win => Ok(CombatTerminal::Win),
        SearchTerminalLabel::Loss => Ok(CombatTerminal::Loss),
        SearchTerminalLabel::Unresolved => Ok(CombatTerminal::Unresolved),
        SearchTerminalLabel::Escape => {
            Err("combat candidate line rejects Smoke Bomb escape as a combat outcome".to_string())
        }
    }
}

fn count_card_plays(actions: &[CombatSearchV2ActionTrace]) -> u32 {
    actions
        .iter()
        .filter(|action| matches!(action.input, ClientInput::PlayCard { .. }))
        .count() as u32
}

fn count_potion_uses(actions: &[CombatSearchV2ActionTrace]) -> u32 {
    actions
        .iter()
        .filter(|action| matches!(action.input, ClientInput::UsePotion { .. }))
        .count() as u32
}

fn count_potion_discards(actions: &[CombatSearchV2ActionTrace]) -> u32 {
    actions
        .iter()
        .filter(|action| matches!(action.input, ClientInput::DiscardPotion(_)))
        .count() as u32
}

#[cfg(test)]
mod tests {
    use crate::ai::combat_search_v2::SearchTerminalLabel;

    use super::combat_terminal_from_search_label;

    #[test]
    fn combat_candidate_line_rejects_smoke_bomb_escape() {
        let error = combat_terminal_from_search_label(SearchTerminalLabel::Escape)
            .expect_err("escape must not become an executable combat outcome");

        assert!(error.contains("Smoke Bomb escape"));
    }
}
