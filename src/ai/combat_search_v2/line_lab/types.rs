use serde::Serialize;

use crate::sim::combat::CombatPosition;

pub use super::super::turn_pool_rescue::{
    CombatTurnPoolRescueLineSummary as CombatLineLabTurnPoolLineSummary,
    CombatTurnPoolRescueReport as CombatLineLabTurnPoolReport,
};
use super::super::SearchTerminalLabel;

#[derive(Clone, Debug, Serialize)]
pub struct CombatLineLabReport {
    pub schema: &'static str,
    pub baseline: Option<CombatLineLabLineSummary>,
    pub cuts: Vec<CombatLineLabCutReport>,
    pub best_repair: Option<CombatLineLabCutReport>,
    pub turn_pool: Option<CombatLineLabTurnPoolReport>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatLineLabLineSummary {
    pub source: &'static str,
    pub terminal: SearchTerminalLabel,
    pub final_hp: i32,
    pub total_enemy_hp: i32,
    pub living_enemy_count: usize,
    pub turns: u32,
    pub actions: usize,
    pub potions_used: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatLineLabCutReport {
    pub cut_kind: &'static str,
    pub cut_action_index: usize,
    pub prefix_replayed_actions: usize,
    pub terminal: Option<SearchTerminalLabel>,
    pub final_hp: Option<i32>,
    pub total_enemy_hp: Option<i32>,
    pub living_enemy_count: Option<usize>,
    pub turns: Option<u32>,
    pub suffix_actions: Option<usize>,
    pub total_potions_used: Option<u32>,
    pub baseline_suffix_replay_ok: Option<bool>,
    pub baseline_suffix_terminal: Option<SearchTerminalLabel>,
    pub baseline_suffix_final_hp: Option<i32>,
    pub baseline_suffix_total_enemy_hp: Option<i32>,
    pub repair_action_edit_distance: Option<usize>,
    pub delta_enemy_hp: Option<i32>,
    pub delta_player_hp: Option<i32>,
    pub nodes_expanded: Option<u64>,
    pub deadline_hit: Option<bool>,
    pub failed_reason: Option<&'static str>,
}

#[derive(Clone, Copy)]
pub(super) struct CutPoint {
    pub(super) kind: &'static str,
    pub(super) action_index: usize,
}

pub(super) struct PrefixReplay {
    pub(super) position: CombatPosition,
    pub(super) replayed_actions: usize,
    pub(super) potions_used: u32,
}

pub(super) struct ReplaySummary {
    pub(super) terminal: SearchTerminalLabel,
    pub(super) final_hp: i32,
    pub(super) total_enemy_hp: i32,
    pub(super) living_enemy_count: usize,
}
