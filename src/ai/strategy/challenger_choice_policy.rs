use crate::ai::strategy::candidate_pressure_response::{
    CandidatePressureResponse, StrategyCommitmentKind,
};
use crate::ai::strategy::challenger_decision_context::ChallengerDecisionContext;
use crate::ai::strategy::challenger_policy_state::{ChallengerPolicyState, CommitmentStatus};
use crate::ai::strategy::decision_pipeline::CandidateLane;
use crate::ai::strategy::pressure_assessment::{PressureCoverage, PressureHypothesis};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyRepairCandidateView {
    pub choice_index: usize,
    pub lane: CandidateLane,
    pub raw_lane: CandidateLane,
    pub auto_allowed: bool,
    pub hard_filtered: bool,
    pub has_reject_cap: bool,
    pub inspect_only_reason: Option<String>,
    pub response: CandidatePressureResponse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicySelectionClass {
    OrdinaryChallenger,
    PressureRepair,
    CommitmentRepair,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyChoiceSelection {
    pub choice_index: usize,
    pub class: PolicySelectionClass,
    pub matched_pressure: Vec<PressureHypothesis>,
    pub matched_commitments: Vec<StrategyCommitmentKind>,
    pub overrode_reject: bool,
}

pub fn select_challenger_candidate(
    policy: &ChallengerPolicyState,
    context: &ChallengerDecisionContext,
    candidate: &PolicyRepairCandidateView,
) -> Option<PolicyChoiceSelection> {
    let matched_commitments = active_commitment_matches(policy, &candidate.response);
    let matched_active = open_pressure_matches(&policy.active_pressure, &candidate.response);
    let matched_current = open_pressure_matches(&context.current_pressure, &candidate.response);
    let matched_pressure = merge_matched_pressure(matched_active, matched_current);

    let ordinarily_eligible = candidate.lane != CandidateLane::Reject
        && (candidate.auto_allowed
            || (candidate.lane == CandidateLane::Probe
                && response_has_policy_meaning(&candidate.response)));
    let repair_eligible = candidate.lane == CandidateLane::Reject
        && !candidate.hard_filtered
        && if candidate.has_reject_cap {
            !matched_commitments.is_empty()
        } else {
            !matched_commitments.is_empty() || !matched_pressure.is_empty()
        };
    if !ordinarily_eligible && !repair_eligible {
        return None;
    }

    let class = if !matched_commitments.is_empty() {
        PolicySelectionClass::CommitmentRepair
    } else if !matched_pressure.is_empty() {
        PolicySelectionClass::PressureRepair
    } else {
        PolicySelectionClass::OrdinaryChallenger
    };
    Some(PolicyChoiceSelection {
        choice_index: candidate.choice_index,
        class,
        matched_pressure,
        matched_commitments,
        overrode_reject: candidate.lane == CandidateLane::Reject,
    })
}

pub fn select_challenger_repair_choice(
    policy: &ChallengerPolicyState,
    context: &ChallengerDecisionContext,
    candidates: &[PolicyRepairCandidateView],
) -> Option<PolicyChoiceSelection> {
    candidates
        .iter()
        .filter_map(|candidate| {
            select_challenger_candidate(policy, context, candidate).map(|selection| {
                let supports_commitment = !selection.matched_commitments.is_empty();
                let answers_active = policy.active_pressure.iter().any(|hypothesis| {
                    hypothesis.coverage == PressureCoverage::Open
                        && candidate.response.axes.contains(&hypothesis.axis)
                });
                let answers_current = context.current_pressure.iter().any(|hypothesis| {
                    hypothesis.coverage == PressureCoverage::Open
                        && candidate.response.axes.contains(&hypothesis.axis)
                });
                (
                    (
                        u8::from(!supports_commitment),
                        u8::from(!answers_active),
                        u8::from(!answers_current),
                        candidate.choice_index,
                    ),
                    selection,
                )
            })
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, selection)| selection)
}

pub fn seed_challenger_repair_policy(
    lane_id: u8,
    checkpoint_ref: impl Into<String>,
    context: &ChallengerDecisionContext,
    candidate: &PolicyRepairCandidateView,
) -> Option<(ChallengerPolicyState, PolicyChoiceSelection)> {
    let mut policy = ChallengerPolicyState::new(lane_id);
    policy.reconcile_context(context);
    let selection = select_challenger_candidate(&policy, context, candidate)?;
    if selection.matched_pressure.is_empty() && selection.matched_commitments.is_empty() {
        return None;
    }
    policy.merge_matched_pressure(&selection.matched_pressure);
    policy.record_divergence(checkpoint_ref, &candidate.response);
    policy.satisfy_supported_requirements(&candidate.response);
    Some((policy, selection))
}

fn active_commitment_matches(
    policy: &ChallengerPolicyState,
    response: &CandidatePressureResponse,
) -> Vec<StrategyCommitmentKind> {
    let mut matches = policy
        .commitments
        .iter()
        .filter(|commitment| {
            commitment.status == CommitmentStatus::Active
                && response.supports_commitments.contains(&commitment.kind)
        })
        .map(|commitment| commitment.kind)
        .collect::<Vec<_>>();
    matches.sort();
    matches.dedup();
    matches
}

fn open_pressure_matches(
    pressure: &[PressureHypothesis],
    response: &CandidatePressureResponse,
) -> Vec<PressureHypothesis> {
    pressure
        .iter()
        .filter(|hypothesis| {
            hypothesis.coverage == PressureCoverage::Open
                && response.axes.contains(&hypothesis.axis)
        })
        .cloned()
        .collect()
}

fn merge_matched_pressure(
    mut active: Vec<PressureHypothesis>,
    current: Vec<PressureHypothesis>,
) -> Vec<PressureHypothesis> {
    for hypothesis in current {
        if !active
            .iter()
            .any(|existing| existing.axis == hypothesis.axis)
        {
            active.push(hypothesis);
        }
    }
    active.sort_by_key(|hypothesis| hypothesis.axis);
    active
}

fn response_has_policy_meaning(response: &CandidatePressureResponse) -> bool {
    !response.axes.is_empty()
        || !response.supports_commitments.is_empty()
        || !response.opens_commitments.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::strategy::candidate_pressure_response::{
        CandidatePressureResponse, StrategyCommitmentKind,
    };
    use crate::ai::strategy::challenger_decision_context::open_inventory_pressure;
    use crate::ai::strategy::challenger_policy_state::ChallengerPolicyState;
    use crate::ai::strategy::decision_pipeline::CandidateLane;
    use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
    use crate::ai::strategy::deck_strategic_deficit::{
        DeckStrategicDeficitSummary, StrategicBurdenLevel, StrategicDeficitLevel,
    };
    use crate::ai::strategy::pressure_assessment::{
        EvidenceConfidence, PressureAxis, PressureCoverage, PressureHypothesis,
    };
    use crate::state::run::RunState;

    fn summary() -> DeckStrategicDeficitSummary {
        DeckStrategicDeficitSummary {
            frontload_damage: StrategicDeficitLevel::Adequate,
            aoe_or_minion_control: StrategicDeficitLevel::Adequate,
            block_or_mitigation: StrategicDeficitLevel::Adequate,
            boss_scaling_plan: StrategicDeficitLevel::Adequate,
            deck_access: StrategicDeficitLevel::Adequate,
            energy_or_playability: StrategicDeficitLevel::Adequate,
            deck_burden: StrategicBurdenLevel::Clean,
            too_many_low_impact_attacks: false,
            opening_hand_pollution: false,
            severe_curse_burden: false,
        }
    }

    fn decision_context(
        current_pressure: Vec<PressureHypothesis>,
        automatic_commitments: Vec<StrategyCommitmentKind>,
    ) -> ChallengerDecisionContext {
        let run = RunState::new(20, 0, false, "Ironclad");
        ChallengerDecisionContext {
            deck_plan: DeckPlanSnapshot::from_run_state(&run),
            gold: run.gold,
            current_pressure,
            automatic_commitments,
        }
    }

    fn context_from_summary(facts: DeckStrategicDeficitSummary) -> ChallengerDecisionContext {
        decision_context(open_inventory_pressure(facts), Vec::new())
    }

    fn context_with_open_axis(axis: PressureAxis) -> ChallengerDecisionContext {
        decision_context(
            vec![PressureHypothesis {
                axis,
                coverage: PressureCoverage::Open,
                confidence: EvidenceConfidence::Low,
                supporting_evidence: Vec::new(),
                contradicting_evidence: Vec::new(),
            }],
            Vec::new(),
        )
    }

    fn context_with_exhaust_commitment() -> ChallengerDecisionContext {
        decision_context(Vec::new(), vec![StrategyCommitmentKind::ExhaustEngine])
    }

    fn candidate(
        lane: CandidateLane,
        hard_filtered: bool,
        has_reject_cap: bool,
        response: CandidatePressureResponse,
    ) -> PolicyRepairCandidateView {
        PolicyRepairCandidateView {
            choice_index: 1,
            lane,
            raw_lane: lane,
            auto_allowed: lane != CandidateLane::Reject,
            hard_filtered,
            has_reject_cap,
            inspect_only_reason: (lane == CandidateLane::Reject)
                .then(|| "candidate score rejected".to_string()),
            response,
        }
    }

    #[test]
    fn static_adequacy_never_emits_covered_pressure() {
        let hypotheses = open_inventory_pressure(summary());

        assert!(hypotheses.is_empty());
    }

    #[test]
    fn missing_tempo_and_delay_remain_distinct_open_hypotheses() {
        let mut facts = summary();
        facts.frontload_damage = StrategicDeficitLevel::Missing;
        facts.block_or_mitigation = StrategicDeficitLevel::Thin;

        let hypotheses = open_inventory_pressure(facts);

        assert!(hypotheses
            .iter()
            .any(|item| item.axis == PressureAxis::ResolutionTempo));
        assert!(hypotheses
            .iter()
            .any(|item| item.axis == PressureAxis::DelayCapacity));
        assert!(hypotheses
            .iter()
            .all(|item| item.coverage == PressureCoverage::Open));
    }

    #[test]
    fn seed_keeps_only_pressure_axes_the_candidate_can_answer() {
        let mut facts = summary();
        facts.frontload_damage = StrategicDeficitLevel::Missing;
        facts.block_or_mitigation = StrategicDeficitLevel::Thin;
        let context = context_from_summary(facts);
        let candidate = candidate(
            CandidateLane::Probe,
            false,
            false,
            CandidatePressureResponse {
                axes: vec![PressureAxis::DelayCapacity],
                ..CandidatePressureResponse::default()
            },
        );

        let (state, selection) =
            seed_challenger_repair_policy(1, "branch-0/step-0", &context, &candidate)
                .expect("delay response should seed a challenger");

        assert_eq!(state.active_pressure.len(), 1);
        assert_eq!(state.active_pressure[0].axis, PressureAxis::DelayCapacity);
        assert_eq!(state.divergence_count, 1);
        assert_eq!(selection.class, PolicySelectionClass::PressureRepair);
    }

    #[test]
    fn hard_filtered_reject_never_becomes_policy_action() {
        let policy = ChallengerPolicyState::new(1);
        let context = context_with_open_axis(PressureAxis::GrowthHorizon);
        let rejected = candidate(
            CandidateLane::Reject,
            true,
            false,
            CandidatePressureResponse {
                axes: vec![PressureAxis::GrowthHorizon],
                ..CandidatePressureResponse::default()
            },
        );

        assert!(select_challenger_candidate(&policy, &context, &rejected).is_none());
    }

    #[test]
    fn scored_reject_can_answer_current_open_pressure() {
        let policy = ChallengerPolicyState::new(1);
        let context = context_with_open_axis(PressureAxis::GrowthHorizon);
        let rejected = candidate(
            CandidateLane::Reject,
            false,
            false,
            CandidatePressureResponse {
                axes: vec![PressureAxis::GrowthHorizon],
                ..CandidatePressureResponse::default()
            },
        );

        let selection = select_challenger_candidate(&policy, &context, &rejected)
            .expect("scored pressure repair should be eligible");

        assert_eq!(selection.class, PolicySelectionClass::PressureRepair);
        assert!(selection.overrode_reject);
    }

    #[test]
    fn cap_rejected_candidate_requires_direct_commitment_support() {
        let mut policy = ChallengerPolicyState::new(1);
        policy.reconcile_context(&context_with_exhaust_commitment());
        let broad_only = candidate(
            CandidateLane::Reject,
            false,
            true,
            CandidatePressureResponse {
                axes: vec![PressureAxis::ResolutionTempo],
                ..CandidatePressureResponse::default()
            },
        );
        let direct = candidate(
            CandidateLane::Reject,
            false,
            true,
            CandidatePressureResponse {
                supports_commitments: vec![StrategyCommitmentKind::ExhaustEngine],
                ..CandidatePressureResponse::default()
            },
        );

        assert!(select_challenger_candidate(
            &policy,
            &context_with_open_axis(PressureAxis::ResolutionTempo),
            &broad_only,
        )
        .is_none());
        assert_eq!(
            select_challenger_candidate(&policy, &context_with_exhaust_commitment(), &direct)
                .expect("direct commitment repair should pass")
                .class,
            PolicySelectionClass::CommitmentRepair,
        );
    }

    #[test]
    fn repair_choice_prefers_commitment_over_earlier_pressure_match() {
        let mut policy = ChallengerPolicyState::new(1);
        policy.reconcile_context(&context_with_exhaust_commitment());
        let mut pressure = candidate(
            CandidateLane::Probe,
            false,
            false,
            CandidatePressureResponse {
                axes: vec![PressureAxis::GrowthHorizon],
                ..CandidatePressureResponse::default()
            },
        );
        pressure.choice_index = 0;
        let commitment = candidate(
            CandidateLane::Probe,
            false,
            false,
            CandidatePressureResponse {
                supports_commitments: vec![StrategyCommitmentKind::ExhaustEngine],
                ..CandidatePressureResponse::default()
            },
        );

        let selection = select_challenger_repair_choice(
            &policy,
            &context_with_open_axis(PressureAxis::GrowthHorizon),
            &[pressure, commitment],
        )
        .expect("one repair candidate should be selected");

        assert_eq!(selection.choice_index, 1);
        assert_eq!(selection.class, PolicySelectionClass::CommitmentRepair);
    }
}
