use super::super::super::super::state_abstraction::{
    StateAbstractionRevealGate, StateDivergenceKind,
};
use super::super::super::aggregate::TurnSequenceEffectAggregate;
use super::super::super::divergence::divergence;
use super::super::super::TurnSequenceDivergence;

pub(in crate::ai::combat_search_v2::turn_sequence_effect::classification) fn terminal_or_public_divergence(
    aggregate: &TurnSequenceEffectAggregate,
) -> Option<TurnSequenceDivergence> {
    if aggregate.terminal_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::TerminalDelta,
            Some("combat.terminal_label"),
            StateAbstractionRevealGate::CombatEnd,
        ));
    }
    if aggregate.legal_action_count_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::LegalActionDelta,
            Some("combat.legal_actions.count"),
            StateAbstractionRevealGate::NextLegalActionGeneration,
        ));
    }
    if aggregate.public_state_keys.len() > 1 || aggregate.resource_public_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::ImmediatePublicDelta,
            Some("combat.public_state"),
            StateAbstractionRevealGate::NextLegalActionGeneration,
        ));
    }
    if aggregate.hand_public_order_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::HandOrderDelta,
            Some("combat.zones.hand"),
            StateAbstractionRevealGate::NextLegalActionGeneration,
        ));
    }
    if aggregate.draw_public_order_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::DrawPileOrderDelta,
            Some("combat.zones.draw_pile"),
            StateAbstractionRevealGate::NextDraw,
        ));
    }
    if aggregate.discard_public_order_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::DiscardOrderDelta,
            Some("combat.zones.discard_pile"),
            StateAbstractionRevealGate::NextShuffle,
        ));
    }
    if aggregate.exhaust_public_order_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::ExhaustOrderDelta,
            Some("combat.zones.exhaust_pile"),
            StateAbstractionRevealGate::NextCardSelection,
        ));
    }
    if aggregate.limbo_public_order_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::PendingQueueDelta,
            Some("combat.zones.limbo"),
            StateAbstractionRevealGate::CurrentActionResolution,
        ));
    }
    None
}
