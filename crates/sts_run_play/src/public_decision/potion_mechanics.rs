//! Candidate-bound potion descriptions, not inferred future actions.
use super::*;
use sts_oracle_learning::ai::card_semantics_v1::potion_acquisition_reading_v1;
use sts_oracle_learning::ai::planner_core::PlannerRewardDescriptor;
use sts_oracle_learning::content::potions::PotionId;

fn candidate_potion(action: &TaskStrategicActionV1) -> Option<PotionId> {
    match action {
        TaskStrategicActionV1::BuyPotion { potion, .. }
        | TaskStrategicActionV1::UseRunPotion { potion, .. }
        | TaskStrategicActionV1::DiscardRunPotion { potion, .. }
        | TaskStrategicActionV1::ClaimReward {
            reward: PlannerRewardDescriptor::Potion { potion },
            ..
        } => Some(*potion),
        _ => None,
    }
}

pub(super) fn project(decision: &TaskPublicDecisionV1) -> Value {
    let candidates: Vec<_> = decision
        .candidates
        .iter()
        .zip(&decision.information.candidate_surface.ordered_candidate_ids)
        .filter_map(|(candidate, id)| {
            let TaskPublicCandidateV1::Strategic { action } = candidate else {
                return None;
            };
            candidate_potion(action).map(|potion| {
                json!({
                    "candidate_id": id, "facts": potion_acquisition_reading_v1(potion),
                })
            })
        })
        .collect();
    json!({
        "schema": "sts-potion-acquisition-reading-v1",
        "public_snapshot_id": decision.information.snapshot_id,
        "basis": "simulator_definition_and_reviewed_effects",
        "coverage": "partial",
        "candidates": candidates,
    })
}
