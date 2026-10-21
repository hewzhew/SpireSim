use super::types::{
    RewardCandidateEvidenceV1, RewardPolicyActionV1, RewardPolicyClassV1, RewardPolicyConfigV1,
};

pub(crate) fn low_agency_claim_action(
    candidate: &RewardCandidateEvidenceV1,
    config: &RewardPolicyConfigV1,
) -> Option<RewardPolicyActionV1> {
    match candidate.class {
        RewardPolicyClassV1::Gold if config.claim_gold => Some(claim(
            candidate,
            0.99,
            "gold reward has no choice tradeoff after it is visible",
        )),
        RewardPolicyClassV1::StolenGold if config.claim_gold => Some(claim(
            candidate,
            0.99,
            "stolen gold return has no choice tradeoff after it is visible",
        )),
        RewardPolicyClassV1::PotionWithEmptySlot if config.claim_potion_with_empty_slot => {
            Some(claim(
                candidate,
                0.88,
                "potion reward is claimed only because an empty potion slot is available",
            ))
        }
        RewardPolicyClassV1::RelicWithoutSapphireKeyConflict
            if config.claim_safe_relic_without_sapphire_key =>
        {
            Some(claim(
                candidate,
                0.85,
                "ordinary relic reward has no Sapphire Key conflict on this reward screen",
            ))
        }
        _ => None,
    }
}

fn claim(
    candidate: &RewardCandidateEvidenceV1,
    confidence: f32,
    reason: &'static str,
) -> RewardPolicyActionV1 {
    RewardPolicyActionV1::Claim {
        index: candidate.index,
        label: candidate.label.clone(),
        confidence,
        reason: reason.to_string(),
    }
}
