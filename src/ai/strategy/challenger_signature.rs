use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::ai::strategy::candidate_pressure_response::StrategyCommitmentKind;
use crate::ai::strategy::challenger_policy_state::{ChallengerPolicyState, CommitmentStatus};
use crate::ai::strategy::pressure_assessment::PressureAxis;

const MAX_CHALLENGERS: usize = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeckBurdenBand {
    Clean,
    Watch,
    Heavy,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployabilityBand {
    Thin,
    Adequate,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ChallengerSignature {
    pub pressure_axes: Vec<PressureAxis>,
    pub active_commitments: Vec<StrategyCommitmentKind>,
    pub burden: DeckBurdenBand,
    pub deployability: DeployabilityBand,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChallengerLaneSnapshot {
    pub policy: ChallengerPolicyState,
    pub burden: DeckBurdenBand,
    pub deployability: DeployabilityBand,
    pub evidence_rank: u16,
}

impl ChallengerLaneSnapshot {
    pub fn signature(&self) -> ChallengerSignature {
        let mut pressure_axes = self
            .policy
            .active_pressure
            .iter()
            .map(|hypothesis| hypothesis.axis)
            .collect::<Vec<_>>();
        pressure_axes.sort();
        pressure_axes.dedup();

        let mut active_commitments = self
            .policy
            .commitments
            .iter()
            .filter(|commitment| commitment.status == CommitmentStatus::Active)
            .map(|commitment| commitment.kind)
            .collect::<Vec<_>>();
        active_commitments.sort();
        active_commitments.dedup();

        ChallengerSignature {
            pressure_axes,
            active_commitments,
            burden: self.burden,
            deployability: self.deployability,
        }
    }
}

pub fn retain_distinct_challengers(
    lanes: Vec<ChallengerLaneSnapshot>,
) -> Vec<ChallengerLaneSnapshot> {
    let mut by_signature = BTreeMap::<ChallengerSignature, ChallengerLaneSnapshot>::new();
    for lane in lanes {
        let signature = lane.signature();
        let replace = match by_signature.get(&signature) {
            None => true,
            Some(existing) => lane.evidence_rank > existing.evidence_rank,
        };
        if replace {
            by_signature.insert(signature, lane);
        }
    }
    let mut retained = by_signature.into_values().collect::<Vec<_>>();
    retained.sort_by(|left, right| right.evidence_rank.cmp(&left.evidence_rank));
    retained.truncate(MAX_CHALLENGERS);
    retained
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::strategy::pressure_assessment::{
        EvidenceConfidence, PressureCoverage, PressureHypothesis,
    };

    fn lane(lane_id: u8, axis: PressureAxis, evidence_rank: u16) -> ChallengerLaneSnapshot {
        let mut policy = ChallengerPolicyState::new(lane_id);
        policy.active_pressure.push(PressureHypothesis {
            axis,
            coverage: PressureCoverage::Open,
            confidence: EvidenceConfidence::High,
            supporting_evidence: Vec::new(),
            contradicting_evidence: Vec::new(),
        });
        ChallengerLaneSnapshot {
            policy,
            burden: DeckBurdenBand::Heavy,
            deployability: DeployabilityBand::Thin,
            evidence_rank,
        }
    }

    #[test]
    fn equivalent_challengers_keep_the_stronger_evidence() {
        let retained = retain_distinct_challengers(vec![
            lane(1, PressureAxis::ResolutionTempo, 4),
            lane(2, PressureAxis::ResolutionTempo, 9),
        ]);

        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].policy.lane_id, 2);
    }

    #[test]
    fn distinct_pressure_hypotheses_survive_up_to_two_lanes() {
        let retained = retain_distinct_challengers(vec![
            lane(1, PressureAxis::ResolutionTempo, 9),
            lane(2, PressureAxis::DelayCapacity, 8),
            lane(3, PressureAxis::Deployability, 1),
        ]);

        assert_eq!(retained.len(), 2);
        assert!(retained.iter().any(|lane| lane.policy.lane_id == 1));
        assert!(retained.iter().any(|lane| lane.policy.lane_id == 2));
    }
}
