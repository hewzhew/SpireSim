use super::super::frontier::SearchNode;
use super::facts::combat_search_core_value_facts;
use crate::runtime::combat::CombatState;
use crate::state::core::EngineState;
use sts_combat_knowledge::CombatTacticalStateValueV1;

pub(in crate::ai::combat_search_v2) fn combat_search_state_value(
    node: &SearchNode,
) -> CombatTacticalStateValueV1 {
    combat_search_state_value_for_state(&node.engine, &node.combat)
}

pub(in crate::ai::combat_search_v2) fn combat_search_state_value_for_state(
    engine: &EngineState,
    combat: &CombatState,
) -> CombatTacticalStateValueV1 {
    let facts = combat_search_core_value_facts(engine, combat);
    CombatTacticalStateValueV1 {
        fewer_living_enemies: -(facts
            .phase_profile
            .enemy_phase
            .phase_adjusted_living_enemy_count as i32),
        phase_adjusted_enemy_effort_progress: -facts
            .phase_profile
            .enemy_phase
            .phase_adjusted_living_enemy_effort,
        enemy_effort_progress: -facts.phase_profile.enemy_phase.raw_living_enemy_effort,
        enemy_hp_progress: -facts.phase_profile.enemy_phase.raw_living_enemy_hp,
        split_debt_hp: -facts.phase_profile.enemy_phase.split_debt_hp,
        guardian_defensive_block: -facts.phase_profile.enemy_phase.guardian_defensive_block,
        guardian_mode_shift_pending: -(facts
            .phase_profile
            .enemy_mechanics
            .guardian_mode_shift_pending_count as i32),
        lagavulin_waking_pressure: -(facts.phase_profile.enemy_mechanics.lagavulin_waking_count
            as i32),
        gremlin_nob_enrage_pressure: -facts
            .phase_profile
            .enemy_mechanics
            .gremlin_nob_anger_amount_total,
        sentry_dazed_pressure: -(facts
            .phase_profile
            .enemy_mechanics
            .sentry_dazed_pressure_count as i32),
        hexaghost_opening_pressure: -(facts
            .phase_profile
            .enemy_mechanics
            .hexaghost_opening_pressure_count as i32),
        high_fanout_pending_choice: -i32::from(facts.phase_profile.pending_choice.high_fanout),
        pending_choice_estimated_action_fanout: -(facts
            .phase_profile
            .pending_choice
            .estimated_action_fanout as i32),
        survival_margin: facts.phase_profile.pressure.survival_margin,
        sustained_mitigation: facts.sustained_mitigation,
        player_hp: combat.entities.player.current_hp,
        player_block: combat.entities.player.block,
        hand_damage: facts.hand.damage,
        hand_block: facts.hand.block,
        hand_playable_cards: facts.hand.playable_cards,
        hand_low_cost: facts.hand.low_cost,
        next_draw_damage: facts.next_draw.damage,
        next_draw_block: facts.next_draw.block,
        next_draw_playable_cards: facts.next_draw.playable_cards,
        next_draw_low_cost: facts.next_draw.low_cost,
    }
}
