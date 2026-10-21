use super::super::state_abstraction::{StateAbstractionRevealGate, StateDivergenceKind};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) struct TurnSequenceDivergence {
    pub kind: StateDivergenceKind,
    pub first_divergence_path: Option<&'static str>,
    pub guessed_reveal_gate: StateAbstractionRevealGate,
}

pub(super) fn divergence(
    kind: StateDivergenceKind,
    first_divergence_path: Option<&'static str>,
    guessed_reveal_gate: StateAbstractionRevealGate,
) -> TurnSequenceDivergence {
    TurnSequenceDivergence {
        kind,
        first_divergence_path,
        guessed_reveal_gate,
    }
}
