use super::super::super::super::state_abstraction::{
    StateAbstractionRevealGate, StateDivergenceKind,
};
use super::super::super::aggregate::TurnSequenceEffectAggregate;
use super::super::super::divergence::divergence;
use super::super::super::TurnSequenceDivergence;

pub(in crate::ai::combat_search_v2::turn_sequence_effect::classification) fn dominance_common_divergence(
    aggregate: &TurnSequenceEffectAggregate,
) -> Option<TurnSequenceDivergence> {
    if aggregate.dominance_monsters_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::MonsterRuntimeDelta,
            Some("combat.monsters.runtime_or_turn_plan"),
            StateAbstractionRevealGate::Unknown,
        ));
    }
    if aggregate.dominance_powers_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::ImmediatePublicDelta,
            Some("combat.powers"),
            StateAbstractionRevealGate::NextLegalActionGeneration,
        ));
    }
    if aggregate.dominance_runtime_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::CombatRuntimeHintDelta,
            Some("combat.runtime_hints"),
            StateAbstractionRevealGate::Unknown,
        ));
    }
    if aggregate.dominance_potions_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::PotionStateDelta,
            Some("combat.potions"),
            StateAbstractionRevealGate::NextLegalActionGeneration,
        ));
    }
    if aggregate.dominance_player_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::PlayerFutureDelta,
            Some("combat.player.future_relevant"),
            StateAbstractionRevealGate::Unknown,
        ));
    }
    if aggregate.dominance_queue_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::PendingQueueDelta,
            Some("combat.dominance.common.queue"),
            StateAbstractionRevealGate::CurrentActionResolution,
        ));
    }
    if aggregate.dominance_engine_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::EngineRuntimeDelta,
            Some("combat.engine_state"),
            StateAbstractionRevealGate::CurrentActionResolution,
        ));
    }
    if aggregate.dominance_meta_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::CombatMetaDelta,
            Some("combat.meta"),
            StateAbstractionRevealGate::Unknown,
        ));
    }
    None
}
