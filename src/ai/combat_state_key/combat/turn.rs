use crate::runtime::combat::{CombatPhase, CombatState, EphemeralCounters};

use super::super::types::{CombatPhaseKey, CombatTurnCountersKey, CombatTurnKey};

pub(super) fn turn_key(combat: &CombatState) -> CombatTurnKey {
    let turn = &combat.turn;
    CombatTurnKey {
        turn_count: turn.turn_count,
        phase: phase_key(turn.current_phase),
        energy: turn.energy,
        turn_start_draw_modifier: turn.turn_start_draw_modifier,
        counters: turn_counters_key(&turn.counters),
    }
}

pub(crate) fn phase_key(phase: CombatPhase) -> CombatPhaseKey {
    match phase {
        CombatPhase::PlayerTurn => CombatPhaseKey::PlayerTurn,
        CombatPhase::MonsterTurn => CombatPhaseKey::MonsterTurn,
        CombatPhase::TurnTransition => CombatPhaseKey::TurnTransition,
    }
}

fn turn_counters_key(counters: &EphemeralCounters) -> CombatTurnCountersKey {
    CombatTurnCountersKey {
        cards_played_this_turn: counters.cards_played_this_turn,
        attacks_played_this_turn: counters.attacks_played_this_turn,
        cards_discarded_this_turn: counters.cards_discarded_this_turn,
        card_ids_played_this_turn: counters.card_ids_played_this_turn.clone(),
        card_ids_played_this_combat: counters.card_ids_played_this_combat.clone(),
        orbs_channeled_this_turn: counters.orbs_channeled_this_turn.clone(),
        orbs_channeled_this_combat: counters.orbs_channeled_this_combat.clone(),
        mantra_gained_this_combat: counters.mantra_gained_this_combat,
        times_damaged_this_combat: counters.times_damaged_this_combat,
        victory_triggered: counters.victory_triggered,
        discovery_cost_for_turn: counters.discovery_cost_for_turn,
        early_end_turn_pending: counters.early_end_turn_pending,
        skip_monster_turn_pending: counters.skip_monster_turn_pending,
        end_turn_hand_discarded: counters.end_turn_hand_discarded,
        player_escaping: counters.player_escaping,
        escape_pending_reward: counters.escape_pending_reward,
    }
}
