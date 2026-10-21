//! Definitions for explicitly visible relics. Visibility does not grant an action.
use super::*;
use sts_oracle_learning::ai::card_semantics_v1::relic_acquisition_reading_v1;
use sts_oracle_learning::ai::planner_core::{PlannerDecisionContext, PlannerRewardDescriptor};
use sts_oracle_learning::content::relics::RelicId;

fn candidate_relic(action: &TaskStrategicActionV1) -> Option<RelicId> {
    match action {
        TaskStrategicActionV1::BuyRelic { relic, .. }
        | TaskStrategicActionV1::TakeBossRelic { relic, .. }
        | TaskStrategicActionV1::ClaimReward {
            reward: PlannerRewardDescriptor::Relic { relic },
            ..
        } => Some(*relic),
        _ => None,
    }
}

pub(super) fn project(decision: &TaskPublicDecisionV1) -> Value {
    let mut relics = Vec::new();
    let mut include = |relic: RelicId| {
        if !relics.contains(&relic) {
            relics.push(relic);
        }
    };
    if let TaskPublicObservationV1::Strategic { observation } = &decision.observation {
        for owned in &observation.relics {
            include(owned.relic);
        }
        match &observation.context {
            PlannerDecisionContext::Shop { inventory, .. } => {
                for offer in &inventory.relics {
                    include(offer.relic);
                }
            }
            PlannerDecisionContext::Reward { pending_rewards } => {
                for reward in pending_rewards {
                    if let PlannerRewardDescriptor::Relic { relic } = reward {
                        include(*relic);
                    }
                }
            }
            _ => {}
        }
    }
    for candidate in &decision.candidates {
        if let TaskPublicCandidateV1::Strategic { action } = candidate {
            if let Some(relic) = candidate_relic(action) {
                include(relic);
            }
        }
    }
    json!({
        "schema": "sts-visible-relic-reading-v1",
        "public_snapshot_id": decision.information.snapshot_id,
        "basis": "simulator_definition_and_reviewed_effects", "coverage": "partial",
        "definitions": relics.into_iter().map(relic_acquisition_reading_v1).collect::<Vec<_>>(),
    })
}
