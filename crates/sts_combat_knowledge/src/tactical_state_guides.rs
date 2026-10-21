//! Lexicographic tactical state value and independent guide lanes.
//!
//! The atomic owner may calculate some input facts while they still depend on
//! its private mechanics profiles. This module owns only their typed ordering
//! and guide projection; it cannot expand a state or accept a terminal.

use std::cmp::Ordering;

use sts_core::sim::combat::CombatPosition;

use super::tactical_state_facts::{
    combat_player_setup_summary_v1, combat_player_tactical_opportunity_summary_v1,
    combat_priority_owner_progress_v1, CombatPlayerSetupSummaryV1,
    CombatPlayerTacticalOpportunitySummaryV1,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CombatTacticalStateValueV1 {
    pub fewer_living_enemies: i32,
    pub phase_adjusted_enemy_effort_progress: i32,
    pub enemy_effort_progress: i32,
    pub enemy_hp_progress: i32,
    pub split_debt_hp: i32,
    pub guardian_defensive_block: i32,
    pub guardian_mode_shift_pending: i32,
    pub lagavulin_waking_pressure: i32,
    pub gremlin_nob_enrage_pressure: i32,
    pub sentry_dazed_pressure: i32,
    pub hexaghost_opening_pressure: i32,
    pub high_fanout_pending_choice: i32,
    pub pending_choice_estimated_action_fanout: i32,
    pub survival_margin: i32,
    pub sustained_mitigation: i32,
    pub player_hp: i32,
    pub player_block: i32,
    pub hand_damage: i32,
    pub hand_block: i32,
    pub hand_playable_cards: i32,
    pub hand_low_cost: i32,
    pub next_draw_damage: i32,
    pub next_draw_block: i32,
    pub next_draw_playable_cards: i32,
    pub next_draw_low_cost: i32,
}

impl Ord for CombatTacticalStateValueV1 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.fewer_living_enemies
            .cmp(&other.fewer_living_enemies)
            .then_with(|| {
                self.phase_adjusted_enemy_effort_progress
                    .cmp(&other.phase_adjusted_enemy_effort_progress)
            })
            .then_with(|| self.enemy_effort_progress.cmp(&other.enemy_effort_progress))
            .then_with(|| self.enemy_hp_progress.cmp(&other.enemy_hp_progress))
            .then_with(|| self.split_debt_hp.cmp(&other.split_debt_hp))
            .then_with(|| {
                self.guardian_defensive_block
                    .cmp(&other.guardian_defensive_block)
            })
            .then_with(|| {
                self.guardian_mode_shift_pending
                    .cmp(&other.guardian_mode_shift_pending)
            })
            .then_with(|| {
                self.lagavulin_waking_pressure
                    .cmp(&other.lagavulin_waking_pressure)
            })
            .then_with(|| {
                self.gremlin_nob_enrage_pressure
                    .cmp(&other.gremlin_nob_enrage_pressure)
            })
            .then_with(|| self.sentry_dazed_pressure.cmp(&other.sentry_dazed_pressure))
            .then_with(|| {
                self.hexaghost_opening_pressure
                    .cmp(&other.hexaghost_opening_pressure)
            })
            .then_with(|| {
                self.high_fanout_pending_choice
                    .cmp(&other.high_fanout_pending_choice)
            })
            .then_with(|| {
                self.pending_choice_estimated_action_fanout
                    .cmp(&other.pending_choice_estimated_action_fanout)
            })
            .then_with(|| self.survival_margin.cmp(&other.survival_margin))
            .then_with(|| self.sustained_mitigation.cmp(&other.sustained_mitigation))
            .then_with(|| self.player_hp.cmp(&other.player_hp))
            .then_with(|| self.player_block.cmp(&other.player_block))
            .then_with(|| self.hand_damage.cmp(&other.hand_damage))
            .then_with(|| self.hand_block.cmp(&other.hand_block))
            .then_with(|| self.hand_playable_cards.cmp(&other.hand_playable_cards))
            .then_with(|| self.hand_low_cost.cmp(&other.hand_low_cost))
            .then_with(|| self.next_draw_damage.cmp(&other.next_draw_damage))
            .then_with(|| self.next_draw_block.cmp(&other.next_draw_block))
            .then_with(|| {
                self.next_draw_playable_cards
                    .cmp(&other.next_draw_playable_cards)
            })
            .then_with(|| self.next_draw_low_cost.cmp(&other.next_draw_low_cost))
    }
}

impl PartialOrd for CombatTacticalStateValueV1 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CombatTacticalGuideBundleV1 {
    pub progress: Vec<i32>,
    pub survival: Vec<i32>,
    pub horizon: Vec<i32>,
    pub turn_generation: Vec<i32>,
    pub setup: Vec<i32>,
}

pub fn combat_tactical_guide_bundle_v1(
    position: &CombatPosition,
    value: &CombatTacticalStateValueV1,
) -> CombatTacticalGuideBundleV1 {
    let setup = combat_player_setup_summary_v1(&position.combat);
    let tactical = combat_player_tactical_opportunity_summary_v1(position, value.survival_margin);
    CombatTacticalGuideBundleV1 {
        progress: progress_guide_components(position, value),
        survival: survival_guide_components(position, value),
        horizon: horizon_guide_components(position, value),
        turn_generation: turn_generation_guide_components(position, value, setup),
        setup: setup_guide_components(position, value, setup, tactical),
    }
}

fn progress_guide_components(
    position: &CombatPosition,
    value: &CombatTacticalStateValueV1,
) -> Vec<i32> {
    let priority = combat_priority_owner_progress_v1(&position.combat);
    vec![
        value.fewer_living_enemies,
        priority.completed_targets,
        priority.focused_damage,
        priority.total_damage,
        value.phase_adjusted_enemy_effort_progress,
        value.enemy_effort_progress,
        value.enemy_hp_progress,
        value.split_debt_hp,
        value.guardian_defensive_block,
        value.guardian_mode_shift_pending,
        value.lagavulin_waking_pressure,
        value.gremlin_nob_enrage_pressure,
        value.sentry_dazed_pressure,
        value.hexaghost_opening_pressure,
        value.high_fanout_pending_choice,
        value.pending_choice_estimated_action_fanout,
        value.survival_margin,
        value.sustained_mitigation,
        value.player_hp,
        value.player_block,
        value.hand_damage,
        value.hand_block,
        value.hand_playable_cards,
        value.hand_low_cost,
        value.next_draw_damage,
        value.next_draw_block,
        value.next_draw_playable_cards,
        value.next_draw_low_cost,
    ]
}

fn survival_guide_components(
    position: &CombatPosition,
    value: &CombatTacticalStateValueV1,
) -> Vec<i32> {
    let priority = combat_priority_owner_progress_v1(&position.combat);
    vec![
        value.survival_margin,
        value.player_hp,
        value.fewer_living_enemies,
        priority.completed_targets,
        priority.focused_damage,
        priority.total_damage,
        value.phase_adjusted_enemy_effort_progress,
        value.enemy_effort_progress,
        value.enemy_hp_progress,
        value.sustained_mitigation,
        value.player_block,
        value.guardian_defensive_block,
        value.guardian_mode_shift_pending,
        value.split_debt_hp,
        value.hand_block,
        value.next_draw_block,
        value.hand_damage,
        value.next_draw_damage,
    ]
}

fn horizon_guide_components(
    position: &CombatPosition,
    value: &CombatTacticalStateValueV1,
) -> Vec<i32> {
    let priority = combat_priority_owner_progress_v1(&position.combat);
    vec![
        i32::try_from(position.combat.turn.turn_count).unwrap_or(i32::MAX),
        value.fewer_living_enemies,
        priority.completed_targets,
        priority.focused_damage,
        priority.total_damage,
        value.phase_adjusted_enemy_effort_progress,
        value.enemy_effort_progress,
        value.enemy_hp_progress,
        value.survival_margin,
        value.player_hp,
    ]
}

fn turn_generation_guide_components(
    position: &CombatPosition,
    value: &CombatTacticalStateValueV1,
    setup: CombatPlayerSetupSummaryV1,
) -> Vec<i32> {
    let priority = combat_priority_owner_progress_v1(&position.combat);
    vec![
        i32::from(position.combat.turn.counters.cards_played_this_turn),
        priority.completed_targets,
        priority.focused_damage,
        priority.total_damage,
        setup.exhaust_engine_connected,
        setup.status_access_engine_connected,
        setup.active_power_count,
        setup.active_power_mass,
        value.sustained_mitigation,
        value.fewer_living_enemies,
        value.phase_adjusted_enemy_effort_progress,
        value.enemy_effort_progress,
        value.enemy_hp_progress,
        i32::from(position.combat.turn.energy),
        value.hand_playable_cards,
        value.player_hp,
        value.player_block,
    ]
}

fn setup_guide_components(
    position: &CombatPosition,
    value: &CombatTacticalStateValueV1,
    setup: CombatPlayerSetupSummaryV1,
    tactical: CombatPlayerTacticalOpportunitySummaryV1,
) -> Vec<i32> {
    let priority = combat_priority_owner_progress_v1(&position.combat);
    vec![
        tactical.runic_cube_emergency_draw_conversion,
        tactical.dark_embrace_wound_access_conversion,
        setup.exhaust_engine_connected,
        setup.status_access_engine_connected,
        setup.exhaust_payoff_engine_count,
        setup.recurring_output_count,
        setup.recurring_output_mass,
        value.fewer_living_enemies,
        priority.completed_targets,
        priority.focused_damage,
        priority.total_damage,
        value.phase_adjusted_enemy_effort_progress,
        value.enemy_effort_progress,
        value.enemy_hp_progress,
        setup.exhaust_engine_fuel,
        setup.exhaust_payoff_engine_fuel,
        value.player_hp,
        value.survival_margin,
        setup.active_power_count,
        setup.active_power_mass,
        value.hand_block,
        value.hand_damage,
        i32::try_from(position.combat.turn.turn_count).unwrap_or(i32::MAX),
    ]
}
