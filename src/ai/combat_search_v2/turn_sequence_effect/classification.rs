use super::super::state_abstraction::StateAbstractionRevealGate;
use super::aggregate::TurnSequenceEffectAggregate;
use super::divergence::divergence;
use super::TurnSequenceDivergence;

mod sections;
use sections::{
    dominance_common_divergence, identity_or_resource_divergence, pending_or_rng_divergence,
    terminal_or_public_divergence, turn_runtime_divergence,
};

impl TurnSequenceEffectAggregate {
    pub(in crate::ai::combat_search_v2) fn classify(&self) -> TurnSequenceDivergence {
        if let Some(divergence) = terminal_or_public_divergence(self) {
            return divergence;
        }
        if let Some(divergence) = pending_or_rng_divergence(self) {
            return divergence;
        }
        if let Some(divergence) = turn_runtime_divergence(self) {
            return divergence;
        }
        if let Some(divergence) = dominance_common_divergence(self) {
            return divergence;
        }
        if let Some(divergence) = identity_or_resource_divergence(self) {
            return divergence;
        }
        divergence(
            super::super::state_abstraction::StateDivergenceKind::Unknown,
            None,
            StateAbstractionRevealGate::Unknown,
        )
    }
}
