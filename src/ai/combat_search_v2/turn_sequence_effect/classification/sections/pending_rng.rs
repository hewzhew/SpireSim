use super::super::super::super::state_abstraction::{
    StateAbstractionRevealGate, StateDivergenceKind,
};
use super::super::super::aggregate::TurnSequenceEffectAggregate;
use super::super::super::divergence::divergence;
use super::super::super::TurnSequenceDivergence;

pub(in crate::ai::combat_search_v2::turn_sequence_effect::classification) fn pending_or_rng_divergence(
    aggregate: &TurnSequenceEffectAggregate,
) -> Option<TurnSequenceDivergence> {
    if aggregate.pending_queue_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::PendingQueueDelta,
            Some("combat.engine.action_queue_or_queued_cards"),
            StateAbstractionRevealGate::CurrentActionResolution,
        ));
    }
    if aggregate.rng_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::RngStateDelta,
            Some("combat.rng"),
            StateAbstractionRevealGate::NextRandomCall,
        ));
    }
    if aggregate.dominance_rng_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::RngStateDelta,
            Some("combat.dominance.common.rng"),
            StateAbstractionRevealGate::NextRandomCall,
        ));
    }
    None
}
