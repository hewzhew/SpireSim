use super::super::super::super::state_abstraction::{
    StateAbstractionRevealGate, StateDivergenceKind,
};
use super::super::super::aggregate::TurnSequenceEffectAggregate;
use super::super::super::divergence::divergence;
use super::super::super::TurnSequenceDivergence;

pub(in crate::ai::combat_search_v2::turn_sequence_effect::classification) fn identity_or_resource_divergence(
    aggregate: &TurnSequenceEffectAggregate,
) -> Option<TurnSequenceDivergence> {
    if aggregate.hand_identity_order_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::CardLineageDelta,
            Some("combat.zones.hand.lineage_order"),
            StateAbstractionRevealGate::NextLegalActionGeneration,
        ));
    }
    if aggregate.draw_identity_order_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::CardLineageDelta,
            Some("combat.zones.draw_pile.lineage_order"),
            StateAbstractionRevealGate::NextDraw,
        ));
    }
    if aggregate.discard_identity_order_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::CardLineageDelta,
            Some("combat.zones.discard_pile.lineage_order"),
            StateAbstractionRevealGate::NextShuffle,
        ));
    }
    if aggregate.exhaust_identity_order_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::CardLineageDelta,
            Some("combat.zones.exhaust_pile.lineage_order"),
            StateAbstractionRevealGate::NextCardSelection,
        ));
    }
    if aggregate.limbo_identity_order_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::CardLineageDelta,
            Some("combat.zones.limbo.lineage_order"),
            StateAbstractionRevealGate::CurrentActionResolution,
        ));
    }
    if aggregate.dominance_zones_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::ZoneRuntimeDelta,
            Some("combat.zones.runtime"),
            StateAbstractionRevealGate::Unknown,
        ));
    }
    if aggregate.resource_cost_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::Unknown,
            Some("search.resource_vector.costs"),
            StateAbstractionRevealGate::Unknown,
        ));
    }
    if aggregate.dominance_keys.len() > 1 {
        return Some(divergence(
            StateDivergenceKind::Unknown,
            Some("combat.dominance_key"),
            StateAbstractionRevealGate::Unknown,
        ));
    }
    None
}
