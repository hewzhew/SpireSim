use serde::{Deserialize, Serialize};

use crate::ai::strategy::candidate_pressure_response::{
    CandidatePressureResponse, StrategyCommitmentKind,
};
use crate::ai::strategy::challenger_decision_context::ChallengerDecisionContext;
use crate::ai::strategy::pressure_assessment::{PressureCoverage, PressureHypothesis};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitmentRequirement {
    RepeatableSupply,
    Source,
    Payoff,
    Deployability,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitmentStatus {
    Active,
    Completed,
    Abandoned,
    Expired,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitmentHorizon {
    DecisionBoundaries(u8),
    NextEliteOrBoss,
    CurrentActBoss,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyProgress {
    DecisionBoundary,
    EliteReached,
    BossReached,
    ActAdvanced,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StrategyCommitment {
    pub kind: StrategyCommitmentKind,
    pub status: CommitmentStatus,
    pub requirements: Vec<CommitmentRequirement>,
    pub horizon: CommitmentHorizon,
    pub burden_units: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChallengerPolicyState {
    pub lane_id: u8,
    pub active_pressure: Vec<PressureHypothesis>,
    pub commitments: Vec<StrategyCommitment>,
    pub divergence_count: u16,
    pub last_checkpoint_ref: Option<String>,
}

impl ChallengerPolicyState {
    pub fn new(lane_id: u8) -> Self {
        Self {
            lane_id,
            active_pressure: Vec::new(),
            commitments: Vec::new(),
            divergence_count: 0,
            last_checkpoint_ref: None,
        }
    }

    pub fn record_divergence(
        &mut self,
        checkpoint_ref: impl Into<String>,
        response: &CandidatePressureResponse,
    ) {
        self.divergence_count = self.divergence_count.saturating_add(1);
        self.last_checkpoint_ref = Some(checkpoint_ref.into());
        for kind in &response.opens_commitments {
            if !self.commitments.iter().any(|commitment| {
                commitment.kind == *kind && commitment.status == CommitmentStatus::Active
            }) {
                self.open_commitment(StrategyCommitment {
                    kind: *kind,
                    status: CommitmentStatus::Active,
                    requirements: default_requirements(*kind),
                    horizon: CommitmentHorizon::CurrentActBoss,
                    burden_units: 0,
                });
            }
        }
    }

    pub fn reconcile_context(&mut self, context: &ChallengerDecisionContext) {
        for &kind in &context.automatic_commitments {
            let already_known = self.commitments.iter().any(|commitment| {
                commitment.kind == kind
                    && matches!(
                        commitment.status,
                        CommitmentStatus::Active | CommitmentStatus::Completed
                    )
            });
            if !already_known {
                self.open_commitment(StrategyCommitment {
                    kind,
                    status: CommitmentStatus::Active,
                    requirements: default_requirements(kind),
                    horizon: CommitmentHorizon::CurrentActBoss,
                    burden_units: 0,
                });
            }
        }

        for remembered in &mut self.active_pressure {
            if let Some(current) = context
                .current_pressure
                .iter()
                .find(|current| current.axis == remembered.axis)
            {
                *remembered = merge_pressure_hypotheses(remembered.clone(), current.clone());
            } else if remembered.coverage == PressureCoverage::Open {
                remembered.coverage = PressureCoverage::PartiallyCovered;
            }
        }
    }

    pub fn merge_matched_pressure(&mut self, matched: &[PressureHypothesis]) {
        for hypothesis in matched {
            if let Some(existing) = self
                .active_pressure
                .iter_mut()
                .find(|existing| existing.axis == hypothesis.axis)
            {
                *existing = merge_pressure_hypotheses(existing.clone(), hypothesis.clone());
            } else {
                self.active_pressure.push(hypothesis.clone());
            }
        }
        self.active_pressure
            .sort_by_key(|hypothesis| hypothesis.axis);
    }

    pub fn satisfy_supported_requirements(&mut self, response: &CandidatePressureResponse) {
        for &kind in &response.supports_commitments {
            let requirement = match kind {
                StrategyCommitmentKind::ExhaustEngine => CommitmentRequirement::Payoff,
                StrategyCommitmentKind::StrengthScaling | StrategyCommitmentKind::BlockEngine => {
                    CommitmentRequirement::Source
                }
                StrategyCommitmentKind::UpgradeAccess => CommitmentRequirement::Deployability,
                StrategyCommitmentKind::SelfDamageEngine
                    if response.repeatable_self_damage_supply =>
                {
                    CommitmentRequirement::RepeatableSupply
                }
                StrategyCommitmentKind::SelfDamageEngine => continue,
            };
            self.observe_requirement(kind, requirement);
        }
    }

    pub fn open_commitment(&mut self, commitment: StrategyCommitment) {
        self.commitments.push(commitment);
    }

    pub fn candidate_supports_active_commitment(
        &self,
        response: &CandidatePressureResponse,
    ) -> bool {
        self.commitments.iter().any(|commitment| {
            commitment.status == CommitmentStatus::Active
                && response.supports_commitments.contains(&commitment.kind)
        })
    }

    pub fn observe_requirement(
        &mut self,
        kind: StrategyCommitmentKind,
        requirement: CommitmentRequirement,
    ) {
        if let Some(commitment) = self.commitments.iter_mut().find(|commitment| {
            commitment.kind == kind && commitment.status == CommitmentStatus::Active
        }) {
            commitment.requirements.retain(|item| *item != requirement);
            if commitment.requirements.is_empty() {
                commitment.status = CommitmentStatus::Completed;
            }
        }
    }

    pub fn advance(&mut self, progress: PolicyProgress) {
        for commitment in &mut self.commitments {
            if commitment.status != CommitmentStatus::Active {
                continue;
            }
            let expires = match (&mut commitment.horizon, progress) {
                (
                    CommitmentHorizon::DecisionBoundaries(remaining),
                    PolicyProgress::DecisionBoundary,
                ) => {
                    *remaining = remaining.saturating_sub(1);
                    *remaining == 0
                }
                (
                    CommitmentHorizon::NextEliteOrBoss,
                    PolicyProgress::EliteReached | PolicyProgress::BossReached,
                ) => true,
                (
                    CommitmentHorizon::CurrentActBoss,
                    PolicyProgress::BossReached | PolicyProgress::ActAdvanced,
                ) => true,
                _ => false,
            };
            if expires {
                commitment.status = CommitmentStatus::Expired;
            }
        }
    }
}

fn merge_pressure_hypotheses(
    mut left: PressureHypothesis,
    right: PressureHypothesis,
) -> PressureHypothesis {
    left.coverage = merge_coverage(left.coverage, right.coverage);
    left.confidence = left.confidence.max(right.confidence);
    for evidence in right.supporting_evidence {
        if !left.supporting_evidence.contains(&evidence) {
            left.supporting_evidence.push(evidence);
        }
    }
    for evidence in right.contradicting_evidence {
        if !left.contradicting_evidence.contains(&evidence) {
            left.contradicting_evidence.push(evidence);
        }
    }
    left
}

fn merge_coverage(left: PressureCoverage, right: PressureCoverage) -> PressureCoverage {
    use PressureCoverage::*;
    match (left, right) {
        (Open, _) | (_, Open) => Open,
        (PartiallyCovered, _) | (_, PartiallyCovered) => PartiallyCovered,
        (Unknown, _) | (_, Unknown) => Unknown,
        (Covered, Covered) => Covered,
    }
}

fn default_requirements(kind: StrategyCommitmentKind) -> Vec<CommitmentRequirement> {
    match kind {
        StrategyCommitmentKind::ExhaustEngine => vec![CommitmentRequirement::Payoff],
        StrategyCommitmentKind::SelfDamageEngine => {
            vec![CommitmentRequirement::RepeatableSupply]
        }
        StrategyCommitmentKind::StrengthScaling => vec![CommitmentRequirement::Source],
        StrategyCommitmentKind::BlockEngine => vec![CommitmentRequirement::Source],
        StrategyCommitmentKind::UpgradeAccess => vec![CommitmentRequirement::Deployability],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::strategy::candidate_pressure_response::CandidatePressureResponse;
    use crate::ai::strategy::challenger_decision_context::ChallengerDecisionContext;
    use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
    use crate::ai::strategy::pressure_assessment::{
        EvidenceConfidence, PressureAxis, PressureCoverage, PressureHypothesis,
    };
    use crate::state::run::RunState;

    fn context_with(
        current_pressure: Vec<PressureHypothesis>,
        automatic_commitments: Vec<StrategyCommitmentKind>,
    ) -> ChallengerDecisionContext {
        let run = RunState::new(10, 0, false, "Ironclad");
        ChallengerDecisionContext {
            deck_plan: DeckPlanSnapshot::from_run_state(&run),
            gold: run.gold,
            current_pressure,
            automatic_commitments,
        }
    }

    fn pressure(axis: PressureAxis) -> PressureHypothesis {
        PressureHypothesis {
            axis,
            coverage: PressureCoverage::Open,
            confidence: EvidenceConfidence::Low,
            supporting_evidence: Vec::new(),
            contradicting_evidence: Vec::new(),
        }
    }

    #[test]
    fn context_opens_automatic_commitment_once() {
        let mut state = ChallengerPolicyState::new(1);
        let context = context_with(Vec::new(), vec![StrategyCommitmentKind::ExhaustEngine]);

        state.reconcile_context(&context);
        state.reconcile_context(&context);

        assert_eq!(state.commitments.len(), 1);
        assert_eq!(
            state.commitments[0].requirements,
            vec![CommitmentRequirement::Payoff]
        );
        assert_eq!(
            state.commitments[0].horizon,
            CommitmentHorizon::CurrentActBoss
        );
    }

    #[test]
    fn missing_current_axis_becomes_partial_not_covered() {
        let mut state = ChallengerPolicyState::new(1);
        state
            .active_pressure
            .push(pressure(PressureAxis::DelayCapacity));

        state.reconcile_context(&context_with(Vec::new(), Vec::new()));

        assert_eq!(
            state.active_pressure[0].coverage,
            PressureCoverage::PartiallyCovered
        );
    }

    #[test]
    fn exhaust_support_completes_payoff_without_covering_pressure() {
        let mut state = ChallengerPolicyState::new(1);
        state.reconcile_context(&context_with(
            Vec::new(),
            vec![StrategyCommitmentKind::ExhaustEngine],
        ));
        state
            .active_pressure
            .push(pressure(PressureAxis::GrowthHorizon));
        let response = CandidatePressureResponse {
            supports_commitments: vec![StrategyCommitmentKind::ExhaustEngine],
            ..CandidatePressureResponse::default()
        };

        state.satisfy_supported_requirements(&response);

        assert_eq!(state.commitments[0].status, CommitmentStatus::Completed);
        assert_eq!(state.active_pressure[0].coverage, PressureCoverage::Open);
    }

    #[test]
    fn matched_pressure_becomes_persistent_without_duplicate_axes() {
        let mut state = ChallengerPolicyState::new(1);
        let matched = pressure(PressureAxis::MultiTargetControl);

        state.merge_matched_pressure(std::slice::from_ref(&matched));
        state.merge_matched_pressure(std::slice::from_ref(&matched));

        assert_eq!(state.active_pressure, vec![matched]);
    }

    #[test]
    fn challenger_remembers_multiple_sequential_divergences() {
        let mut state = ChallengerPolicyState::new(1);
        state.record_divergence("a2f19", &CandidatePressureResponse::default());
        state.record_divergence("a2f23", &CandidatePressureResponse::default());

        assert_eq!(state.divergence_count, 2);
        assert_eq!(state.last_checkpoint_ref.as_deref(), Some("a2f23"));
    }

    #[test]
    fn active_commitment_recognizes_later_support() {
        let mut state = ChallengerPolicyState::new(1);
        state.open_commitment(StrategyCommitment {
            kind: StrategyCommitmentKind::ExhaustEngine,
            status: CommitmentStatus::Active,
            requirements: vec![CommitmentRequirement::Payoff],
            horizon: CommitmentHorizon::DecisionBoundaries(3),
            burden_units: 1,
        });
        let response = CandidatePressureResponse {
            supports_commitments: vec![StrategyCommitmentKind::ExhaustEngine],
            ..CandidatePressureResponse::default()
        };

        assert!(state.candidate_supports_active_commitment(&response));
    }

    #[test]
    fn unsupported_commitment_expires_at_its_decision_horizon() {
        let mut state = ChallengerPolicyState::new(1);
        state.open_commitment(StrategyCommitment {
            kind: StrategyCommitmentKind::SelfDamageEngine,
            status: CommitmentStatus::Active,
            requirements: vec![CommitmentRequirement::RepeatableSupply],
            horizon: CommitmentHorizon::DecisionBoundaries(1),
            burden_units: 1,
        });

        state.advance(PolicyProgress::DecisionBoundary);

        assert_eq!(state.commitments[0].status, CommitmentStatus::Expired);
    }

    #[test]
    fn satisfying_last_requirement_completes_commitment() {
        let mut state = ChallengerPolicyState::new(1);
        state.open_commitment(StrategyCommitment {
            kind: StrategyCommitmentKind::SelfDamageEngine,
            status: CommitmentStatus::Active,
            requirements: vec![CommitmentRequirement::RepeatableSupply],
            horizon: CommitmentHorizon::CurrentActBoss,
            burden_units: 1,
        });

        state.observe_requirement(
            StrategyCommitmentKind::SelfDamageEngine,
            CommitmentRequirement::RepeatableSupply,
        );

        assert_eq!(state.commitments[0].status, CommitmentStatus::Completed);
    }

    #[test]
    fn challenger_policy_state_round_trips_through_json() {
        let mut state = ChallengerPolicyState::new(2);
        state.open_commitment(StrategyCommitment {
            kind: StrategyCommitmentKind::ExhaustEngine,
            status: CommitmentStatus::Active,
            requirements: vec![CommitmentRequirement::Payoff],
            horizon: CommitmentHorizon::CurrentActBoss,
            burden_units: 1,
        });

        let json = serde_json::to_string(&state).expect("policy state should serialize");
        let restored: ChallengerPolicyState =
            serde_json::from_str(&json).expect("policy state should deserialize");

        assert_eq!(restored, state);
    }
}
