use super::super::enemy_mechanics_profile::enemy_mechanics_profile_report;
use super::super::frontier::SearchNode;
use super::super::phase_profile::combat_search_phase_profile_report;
use super::super::transition::terminal_label;
use super::super::{
    CombatSearchV2CardPileValueReport, CombatSearchV2ChokerCapacityReport,
    CombatSearchV2FrontierValueReport,
};
use super::facts::combat_search_core_value_facts;
use sts_combat_knowledge::{CombatTacticalCardPileValueV1, CombatTacticalChokerCapacityV1};

pub(in crate::ai::combat_search_v2) const COMBAT_SEARCH_FRONTIER_VALUE_POLICY: &str =
    "frontier_value_v3_visible_pressure_phase_adjusted_enemy_effort_mechanics_pressure_hand_next_draw_resources_no_terminal_claim";

pub(in crate::ai::combat_search_v2) fn combat_search_frontier_value_report(
    node: &SearchNode,
) -> CombatSearchV2FrontierValueReport {
    let facts = combat_search_core_value_facts(&node.engine, &node.combat);
    CombatSearchV2FrontierValueReport {
        policy: COMBAT_SEARCH_FRONTIER_VALUE_POLICY,
        terminal: terminal_label(&node.engine, &node.combat),
        player_hp: node.combat.entities.player.current_hp,
        player_block: node.combat.entities.player.block,
        visible_incoming_damage: facts.phase_profile.pressure.visible_incoming_damage,
        survival_margin: facts.phase_profile.pressure.survival_margin,
        living_enemy_count: facts.living_enemy_count,
        total_enemy_hp: facts.phase_profile.enemy_phase.raw_living_enemy_hp,
        total_enemy_block: facts.phase_profile.enemy_phase.raw_living_enemy_block,
        total_enemy_effort: facts.phase_profile.enemy_phase.raw_living_enemy_effort,
        phase_adjusted_enemy_hp: facts
            .phase_profile
            .enemy_phase
            .phase_adjusted_living_enemy_hp,
        phase_adjusted_enemy_effort: facts
            .phase_profile
            .enemy_phase
            .phase_adjusted_living_enemy_effort,
        split_pending_count: facts.phase_profile.enemy_phase.split_pending_count,
        split_debt_hp: facts.phase_profile.enemy_phase.split_debt_hp,
        awakened_rebirth_pending_count: facts
            .phase_profile
            .enemy_phase
            .awakened_rebirth_pending_count,
        awakened_rebirth_debt_hp: facts.phase_profile.enemy_phase.awakened_rebirth_debt_hp,
        guardian_defensive_count: facts.phase_profile.enemy_phase.guardian_defensive_count,
        guardian_defensive_block: facts.phase_profile.enemy_phase.guardian_defensive_block,
        guardian_mode_shift_pending_count: facts
            .phase_profile
            .enemy_mechanics
            .guardian_mode_shift_pending_count,
        lagavulin_waking_count: facts.phase_profile.enemy_mechanics.lagavulin_waking_count,
        gremlin_nob_anger_amount_total: facts
            .phase_profile
            .enemy_mechanics
            .gremlin_nob_anger_amount_total,
        sentry_dazed_pressure_count: facts
            .phase_profile
            .enemy_mechanics
            .sentry_dazed_pressure_count,
        hexaghost_opening_pressure_count: facts
            .phase_profile
            .enemy_mechanics
            .hexaghost_opening_pressure_count,
        phase_profile: combat_search_phase_profile_report(facts.phase_profile),
        sustained_mitigation: facts.sustained_mitigation,
        hand: card_pile_value_report(facts.hand),
        choker_capacity: choker_capacity_report(facts.choker_capacity),
        next_draw: card_pile_value_report(facts.next_draw),
        enemy_mechanics: enemy_mechanics_profile_report(facts.phase_profile.enemy_mechanics),
        potions_used: node.potions_used,
        potions_discarded: node.potions_discarded,
        cards_played: node.cards_played,
        actions_taken: node.actions.len(),
        estimated: true,
    }
}

fn card_pile_value_report(
    value: CombatTacticalCardPileValueV1,
) -> CombatSearchV2CardPileValueReport {
    CombatSearchV2CardPileValueReport {
        damage: value.damage,
        block: value.block,
        playable_cards: value.playable_cards,
        low_cost: value.low_cost,
    }
}

fn choker_capacity_report(
    capacity: CombatTacticalChokerCapacityV1,
) -> CombatSearchV2ChokerCapacityReport {
    CombatSearchV2ChokerCapacityReport {
        has_velvet_choker: capacity.has_velvet_choker,
        cards_played_this_turn: capacity.cards_played_this_turn,
        remaining_slots: capacity.remaining_slots,
        affordable_hand_cards: capacity.affordable_hand_cards,
        representable_affordable_cards: capacity.representable_affordable_cards,
        stranded_affordable_cards: capacity.stranded_affordable_cards,
    }
}
