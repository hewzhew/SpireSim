use serde::Serialize;

use super::super::{CombatSearchV2StateSummary, SearchTerminalLabel};

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2FrontierReport {
    /// Queue entries waiting in the frontier. One residual pending-choice work
    /// item may own multiple not-yet-materialized prefixes. This is
    /// intentionally not an expensive exact-state census.
    pub remaining_work_items: usize,
    /// Search-only residual work that has not crossed the engine boundary.
    pub pending_choice_work_items: usize,
    pub unresolved_leaf_count: u64,
    pub max_actions_cut_count: u64,
    pub engine_step_limit_count: u64,
    pub potion_budget_cut_count: u64,
    pub best_estimated_value: Option<CombatSearchV2FrontierValueReport>,
    /// Cheap work-item samples; duplicate states and roots are allowed.
    pub sample_states: Vec<CombatSearchV2StateSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2FrontierValueReport {
    pub policy: &'static str,
    pub terminal: SearchTerminalLabel,
    pub player_hp: i32,
    pub player_block: i32,
    pub visible_incoming_damage: i32,
    pub survival_margin: i32,
    pub living_enemy_count: usize,
    pub total_enemy_hp: i32,
    pub total_enemy_block: i32,
    pub total_enemy_effort: i32,
    pub phase_adjusted_enemy_hp: i32,
    pub phase_adjusted_enemy_effort: i32,
    pub split_pending_count: usize,
    pub split_debt_hp: i32,
    pub awakened_rebirth_pending_count: usize,
    pub awakened_rebirth_debt_hp: i32,
    pub guardian_defensive_count: usize,
    pub guardian_defensive_block: i32,
    pub guardian_mode_shift_pending_count: usize,
    pub lagavulin_waking_count: usize,
    pub gremlin_nob_anger_amount_total: i32,
    pub sentry_dazed_pressure_count: usize,
    pub hexaghost_opening_pressure_count: usize,
    pub phase_profile: CombatSearchV2PhaseProfileReport,
    pub sustained_mitigation: i32,
    pub hand: CombatSearchV2CardPileValueReport,
    pub choker_capacity: CombatSearchV2ChokerCapacityReport,
    pub next_draw: CombatSearchV2CardPileValueReport,
    pub enemy_mechanics: CombatSearchV2EnemyMechanicsReport,
    pub potions_used: u32,
    pub potions_discarded: u32,
    pub cards_played: u32,
    pub actions_taken: usize,
    pub estimated: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2PhaseProfileReport {
    pub profiling_policy: &'static str,
    pub special_enemy_phase_count: usize,
    pub split_pending_count: usize,
    pub split_debt_hp: i32,
    pub awakened_rebirth_pending_count: usize,
    pub awakened_rebirth_debt_hp: i32,
    pub guardian_mode_shift_pending_count: usize,
    pub guardian_defensive_count: usize,
    pub lagavulin_sleeping_count: usize,
    pub lagavulin_waking_count: usize,
    pub pending_choice_present: bool,
    pub pending_choice_kind: Option<&'static str>,
    pub pending_choice_candidate_count: usize,
    pub pending_choice_estimated_action_fanout: usize,
    pub high_fanout_pending_choice: bool,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2CardPileValueReport {
    pub damage: i32,
    pub block: i32,
    pub playable_cards: i32,
    pub low_cost: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2ChokerCapacityReport {
    pub has_velvet_choker: bool,
    pub cards_played_this_turn: u8,
    pub remaining_slots: Option<u8>,
    pub affordable_hand_cards: u8,
    pub representable_affordable_cards: u8,
    pub stranded_affordable_cards: u8,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2EnemyMechanicsReport {
    pub profiling_policy: &'static str,
    pub tracked_monsters: usize,
    pub timed_threat_count: usize,
    pub timed_threat_min_owner_turns: Option<u32>,
    pub timed_threat_total_raw_damage: i32,
    pub finite_survival_damage_mitigation_target_count: usize,
    pub finite_survival_damage_mitigation_min_owner_turns: Option<u32>,
    pub attack_retaliation_target_count: usize,
    pub attack_retaliation_total_per_event: i32,
    pub attack_retaliation_visible_growth_target_count: usize,
    pub attack_retaliation_visible_growth_total: i32,
    pub split_pending_count: usize,
    pub guardian_open_count: usize,
    pub guardian_defensive_count: usize,
    pub guardian_mode_shift_pending_count: usize,
    pub guardian_min_mode_shift_remaining: Option<i32>,
    pub lagavulin_sleeping_count: usize,
    pub lagavulin_waking_count: usize,
    pub gremlin_nob_enrage_count: usize,
    pub gremlin_nob_anger_amount_total: i32,
    pub sentry_dazed_pressure_count: usize,
    pub fungi_beast_count: usize,
    pub healer_support_count: usize,
    pub hexaghost_opening_pressure_count: usize,
    pub bronze_automaton_count: usize,
    pub bronze_automaton_spawn_orbs_pending_count: usize,
    pub bronze_automaton_hyper_beam_pending_count: usize,
    pub bronze_orb_count: usize,
    pub bronze_orb_stasis_pending_count: usize,
    pub bronze_orb_stasis_card_count: usize,
    pub awakened_one_curiosity_count: usize,
    pub awakened_one_form_one_target: Option<usize>,
    pub awakened_one_form_one_hp_with_block: Option<i32>,
    pub awakened_one_positive_strength: Option<i32>,
    pub time_eater_count: usize,
    pub time_eater_time_warp_counter: Option<i32>,
    pub time_eater_cards_until_warp: Option<i32>,
    pub time_eater_haste_used: Option<bool>,
    pub time_eater_pending_haste_count: usize,
    pub time_eater_current_hp: Option<i32>,
    pub time_eater_half_hp: Option<i32>,
    pub notes: Vec<&'static str>,
}
