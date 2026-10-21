mod compiler;
mod render;
mod types;

#[cfg(test)]
mod tests;

pub use compiler::{
    best_duplicate_target_for_shop_v1, compile_deck_mutation_decision_v1,
    compile_direct_deck_mutation_plan_candidate_v1, deck_mutation_target_class_for_card_v1,
    deck_removal_target_snapshots_v1,
};
pub use render::render_compiled_deck_mutation_decision_v1;
pub use types::{
    AllowedDeckMutationConsumersV1, CompiledDeckMutationDecisionV1, DeckMutationCardSnapshotV1,
    DeckMutationCommitmentModeV1, DeckMutationCompilerOutputV1, DeckMutationCompilerRequestV1,
    DeckMutationKindV1, DeckMutationPlanCandidateV1, DeckMutationPlanRoleV1,
    DeckMutationPlanStepV1, DeckMutationRemovalBenefitTierV1, DeckMutationRemovalBenefitV1,
    DeckMutationTargetClassV1, DeckMutationTargetLossTierV1, DeckMutationTargetLossV1,
    DeckMutationTransformProfileV1, DuplicateStackBehaviorV1, DuplicateTargetEvaluationV1,
    DuplicateTargetRoleV1, TransformRandomAdditionBandV1, TransformVarianceRiskV1,
};
