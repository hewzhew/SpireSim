use serde::Serialize;

use crate::sim::combat::{CombatPosition, CombatStepper, CombatTerminal, EngineCombatStepper};
use crate::sim::combat_action::CombatActionChoice;
use crate::state::core::ClientInput;

use super::super::{CombatSearchV2ActionTrace, CombatSearchV2StateSummary, SearchTerminalLabel};

#[derive(Clone, Debug, Serialize)]
pub struct CombatTurnPoolRescueReport {
    pub schema: &'static str,
    pub lanes: Vec<CombatTurnPoolRescueLineSummary>,
    pub best: Option<CombatTurnPoolRescueLineSummary>,
    pub nodes_expanded: u64,
    pub deadline_hit: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatTurnPoolOpeningReport {
    pub schema: &'static str,
    pub max_turns: usize,
    pub lanes: Vec<CombatTurnPoolOpeningLineReport>,
    pub best_cultist_cleanup: Option<CombatTurnPoolOpeningLineReport>,
    pub nodes_expanded: u64,
    pub nodes_generated: u64,
    pub deadline_hit: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatTurnPoolOpeningLineReport {
    pub lane: &'static str,
    pub terminal: SearchTerminalLabel,
    pub final_hp: i32,
    pub turns: u32,
    pub actions: Vec<CombatSearchV2ActionTrace>,
    pub potions_used: u32,
    pub powers_played: u32,
    pub cultists_alive: usize,
    pub total_cultist_hp: i32,
    pub end_state: CombatSearchV2StateSummary,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatTurnPoolRescueLineSummary {
    pub lane: &'static str,
    pub terminal: SearchTerminalLabel,
    pub final_hp: i32,
    pub total_enemy_hp: i32,
    pub living_enemy_count: usize,
    pub turns: u32,
    pub actions: usize,
    pub potions_used: u32,
    pub powers_played: u32,
}

#[derive(Clone, Debug)]
pub struct CombatTurnPoolRescueWin {
    pub summary: CombatTurnPoolRescueLineSummary,
    pub actions: Vec<CombatSearchV2ActionTrace>,
    pub nodes_expanded: u64,
    pub nodes_generated: u64,
    pub deadline_hit: bool,
}

impl CombatTurnPoolRescueWin {
    pub fn transition_summary(&self) -> String {
        format!(
            "turn_pool_rescue lane={} actions={} final_hp={} turns={} potions_used={} powers_played={} nodes={}/{} deadline_hit={}",
            self.summary.lane,
            self.actions.len(),
            self.summary.final_hp,
            self.summary.turns,
            self.summary.potions_used,
            self.summary.powers_played,
            self.nodes_expanded,
            self.nodes_generated,
            self.deadline_hit
        )
    }
}

pub(super) struct TurnPoolRun {
    pub(super) lanes: Vec<TurnPoolLaneNode>,
    pub(super) nodes_expanded: u64,
    pub(super) nodes_generated: u64,
    pub(super) deadline_hit: bool,
}

pub(super) struct TurnPoolLaneNode {
    pub(super) lane: TurnPoolLane,
    pub(super) node: TurnPoolNode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TurnPoolLane {
    Damage,
    Survival,
    Setup,
    PowerDelay,
    PotionBurst,
    CultistCleanup,
}

impl TurnPoolLane {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Damage => "damage",
            Self::Survival => "survival",
            Self::Setup => "setup",
            Self::PowerDelay => "power_delay",
            Self::PotionBurst => "potion_burst",
            Self::CultistCleanup => "cultist_cleanup",
        }
    }
}

#[derive(Clone)]
pub(super) struct TurnPoolNode {
    pub(super) position: CombatPosition,
    pub(super) actions: Vec<CombatSearchV2ActionTrace>,
    pub(super) terminal: SearchTerminalLabel,
    pub(super) potions_used: u32,
    pub(super) powers_played: u32,
}

impl TurnPoolNode {
    pub(super) fn root(position: CombatPosition, stepper: &EngineCombatStepper) -> Self {
        let terminal = search_terminal(stepper.terminal(&position));
        Self {
            position,
            actions: Vec::new(),
            terminal,
            potions_used: 0,
            powers_played: 0,
        }
    }

    pub(super) fn child(&self, position: CombatPosition, stepper: &EngineCombatStepper) -> Self {
        Self {
            terminal: search_terminal(stepper.terminal(&position)),
            position,
            actions: self.actions.clone(),
            potions_used: self.potions_used,
            powers_played: self.powers_played,
        }
    }

    pub(super) fn note_action(
        &mut self,
        action_id: usize,
        choice: CombatActionChoice,
        played_power: bool,
    ) {
        if matches!(choice.input, ClientInput::UsePotion { .. }) {
            self.potions_used = self.potions_used.saturating_add(1);
        }
        if played_power {
            self.powers_played = self.powers_played.saturating_add(1);
        }
        self.actions.push(CombatSearchV2ActionTrace {
            step_index: self.actions.len(),
            action_id,
            action_key: choice.action_key,
            action_debug: choice.action_debug,
            input: choice.input,
        });
    }
}

pub(super) struct TurnPoolExpandOutcome {
    pub(super) nodes: Vec<TurnPoolNode>,
    pub(super) nodes_expanded: u64,
    pub(super) nodes_generated: u64,
    pub(super) deadline_hit: bool,
}

fn search_terminal(terminal: CombatTerminal) -> SearchTerminalLabel {
    match terminal {
        CombatTerminal::Win => SearchTerminalLabel::Win,
        CombatTerminal::Loss => SearchTerminalLabel::Loss,
        CombatTerminal::Unresolved => SearchTerminalLabel::Unresolved,
    }
}
