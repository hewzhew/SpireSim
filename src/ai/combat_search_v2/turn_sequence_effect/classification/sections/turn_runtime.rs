use super::super::super::super::state_abstraction::{
    StateAbstractionRevealGate, StateDivergenceKind,
};
use super::super::super::aggregate::TurnSequenceEffectAggregate;
use super::super::super::divergence::divergence;
use super::super::super::TurnSequenceDivergence;

pub(in crate::ai::combat_search_v2::turn_sequence_effect::classification) fn turn_runtime_divergence(
    aggregate: &TurnSequenceEffectAggregate,
) -> Option<TurnSequenceDivergence> {
    if aggregate.dominance_turn_keys.len() <= 1 {
        return None;
    }
    if aggregate.turn_draw_modifier_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::TurnDrawModifierDelta,
            Some("combat.turn.turn_start_draw_modifier"),
            StateAbstractionRevealGate::NextDraw,
        ));
    }
    if aggregate.turn_action_counter_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::TurnActionCounterDelta,
            Some("combat.turn.counters.cards_or_attacks_played_this_turn"),
            StateAbstractionRevealGate::NextLegalActionGeneration,
        ));
    }
    if aggregate.turn_played_card_history_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::TurnPlayedCardHistoryDelta,
            Some("combat.turn.counters.card_ids_played"),
            StateAbstractionRevealGate::NextLegalActionGeneration,
        ));
    }
    if aggregate.turn_discard_counter_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::TurnDiscardCounterDelta,
            Some("combat.turn.counters.cards_discarded_this_turn"),
            StateAbstractionRevealGate::NextLegalActionGeneration,
        ));
    }
    if aggregate.turn_orb_history_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::TurnOrbHistoryDelta,
            Some("combat.turn.counters.orb_or_mantra_history"),
            StateAbstractionRevealGate::NextLegalActionGeneration,
        ));
    }
    if aggregate.turn_combat_flag_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::TurnCombatFlagDelta,
            Some("combat.turn.counters.combat_flags"),
            StateAbstractionRevealGate::Unknown,
        ));
    }
    Some(divergence(
        StateDivergenceKind::TurnRuntimeDelta,
        Some("combat.turn"),
        StateAbstractionRevealGate::NextLegalActionGeneration,
    ))
}
