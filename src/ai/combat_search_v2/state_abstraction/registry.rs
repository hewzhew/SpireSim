use super::types::*;

pub fn boundary_spec(id: StateAbstractionBoundaryId) -> StateAbstractionBoundarySpec {
    match id {
        StateAbstractionBoundaryId::StarterBasicDuplicatePlayCardByTarget => {
            StateAbstractionBoundarySpec {
                id,
                name: "starter_basic_duplicate_play_card_by_target",
                scope: StateAbstractionBoundaryScope::LocalActionList,
                soundness: StateAbstractionSoundnessLevel::LocalActionEquivalent,
                allowed_consumers: vec![StateAbstractionConsumer::LocalActionDedup],
                ignored_fields: vec!["combat.card.lineage"],
                reveal_gates: vec![StateAbstractionRevealGate::CurrentActionResolution],
                audit_required: true,
                notes: "Deduplicates runtime-identical starter basic card plays to the same target inside one legal action list; it is not a global state merge.",
            }
        }
        StateAbstractionBoundaryId::PendingChoiceIdenticalRuntimeCard => {
            StateAbstractionBoundarySpec {
                id,
                name: "pending_choice_identical_runtime_card",
                scope: StateAbstractionBoundaryScope::LocalActionList,
                soundness: StateAbstractionSoundnessLevel::LocalActionEquivalent,
                allowed_consumers: vec![StateAbstractionConsumer::LocalActionDedup],
                ignored_fields: vec!["combat.card.lineage"],
                reveal_gates: vec![StateAbstractionRevealGate::CurrentActionResolution],
                audit_required: true,
                notes: "Deduplicates single-card pending grid/hand choices only when source scope and runtime card fields match; it is not a global state abstraction.",
            }
        }
        StateAbstractionBoundaryId::TurnSequenceOrderSensitive => {
            StateAbstractionBoundarySpec {
                id,
                name: "turn_sequence_order_sensitive",
                scope: StateAbstractionBoundaryScope::CombatSearchAnalysis,
                soundness: StateAbstractionSoundnessLevel::ReportOnly,
                allowed_consumers: vec![StateAbstractionConsumer::ReportOnly],
                ignored_fields: Vec::new(),
                reveal_gates: vec![StateAbstractionRevealGate::Unknown],
                audit_required: true,
                notes: "Observed turn-sequence variants are order-sensitive under the current exact/dominance key and must not prune exact branches.",
            }
        }
    }
}

pub fn registered_boundary_specs() -> Vec<StateAbstractionBoundarySpec> {
    vec![
        boundary_spec(StateAbstractionBoundaryId::StarterBasicDuplicatePlayCardByTarget),
        boundary_spec(StateAbstractionBoundaryId::PendingChoiceIdenticalRuntimeCard),
        boundary_spec(StateAbstractionBoundaryId::TurnSequenceOrderSensitive),
    ]
}
