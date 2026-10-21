use super::super::*;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct ActionEquivalenceKey {
    pub(super) kind: ActionEquivalenceKind,
    pub(super) signature: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ActionEquivalenceKind {
    StarterBasicPlayCard,
    SingleCardPendingChoiceSelection,
}

pub(super) fn equivalence_key_for_choice(
    engine: &EngineState,
    combat: &CombatState,
    choice: &CombatActionChoice,
) -> Option<ActionEquivalenceKey> {
    let shared = crate::sim::combat_action_equivalence::combat_action_equivalence_key_v1(
        engine,
        combat,
        &choice.input,
    )?;
    let kind = match shared.kind {
        crate::sim::combat_action_equivalence::CombatActionEquivalenceKindV1::StarterBasicPlayCard => {
            ActionEquivalenceKind::StarterBasicPlayCard
        }
        crate::sim::combat_action_equivalence::CombatActionEquivalenceKindV1::SingleCardPendingChoiceSelection => {
            ActionEquivalenceKind::SingleCardPendingChoiceSelection
        }
    };
    Some(ActionEquivalenceKey {
        kind,
        signature: shared.signature,
    })
}
