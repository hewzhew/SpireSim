use serde::{Deserialize, Serialize};

use crate::state::core::ClientInput;

use super::SearchTerminalLabel;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CombatSearchV2OutcomeOrderKeyReport {
    pub terminal_rank: i32,
    pub run_hygiene: i32,
    pub persistent_adjusted_hp: i32,
    pub final_hp: i32,
    pub persistent_run_value: i32,
    pub potion_conservation: i32,
    pub faster_turns: i32,
    pub fewer_cards_played: i32,
    pub enemy_progress: i32,
    pub shorter_line: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2TrajectoryReport {
    pub terminal: SearchTerminalLabel,
    pub estimated: bool,
    pub outcome_order_key: CombatSearchV2OutcomeOrderKeyReport,
    pub actions: Vec<CombatSearchV2ActionTrace>,
    pub final_hp: i32,
    pub final_max_hp: i32,
    pub persistent_run_value: i32,
    pub final_block: i32,
    pub hp_loss: i32,
    pub turns: u32,
    pub potions_used: u32,
    pub potions_discarded: u32,
    pub cards_played: u32,
    pub enemy_final_state: Vec<CombatSearchV2EnemySummary>,
    pub final_state: CombatSearchV2StateSummary,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2ActionTrace {
    pub step_index: usize,
    pub action_id: usize,
    pub action_key: String,
    pub action_debug: String,
    pub input: ClientInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CombatSearchV2ActionPreview {
    pub action_key: String,
    pub input: ClientInput,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2EnemySummary {
    pub slot: usize,
    pub entity_id: usize,
    pub enemy_id: String,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub escaped: bool,
    pub dying: bool,
    pub half_dead: bool,
    pub phase: Option<&'static str>,
    pub planned_move_id: u8,
    pub visible_intent: String,
    pub visible_incoming_damage: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2StateSummary {
    pub engine_state: String,
    pub terminal: SearchTerminalLabel,
    pub player_hp: i32,
    pub player_block: i32,
    pub energy: u8,
    pub turn_count: u32,
    pub living_enemy_count: usize,
    pub total_enemy_hp: i32,
    pub visible_incoming_damage: i32,
    pub enemy_slots: Vec<CombatSearchV2EnemySummary>,
    pub hand_count: usize,
    pub draw_count: usize,
    pub discard_count: usize,
    pub exhaust_count: usize,
    pub limbo_count: usize,
    pub queued_cards_count: usize,
}
