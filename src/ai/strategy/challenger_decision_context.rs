use crate::ai::boss_mechanics_v1::{
    boss_mechanic_pressure_profile_v1, BossMechanicMissingAnswerV1,
};
use crate::ai::strategy::candidate_pressure_response::StrategyCommitmentKind;
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::deck_strategic_deficit::{
    DeckStrategicDeficitSummary, StrategicDeficitLevel,
};
use crate::ai::strategy::pressure_assessment::{
    EvidenceConfidence, PressureAxis, PressureCoverage, PressureEvidence, PressureEvidenceSource,
    PressureHypothesis,
};
use crate::state::run::RunState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChallengerDecisionContext {
    pub deck_plan: DeckPlanSnapshot,
    pub gold: i32,
    pub current_pressure: Vec<PressureHypothesis>,
    pub automatic_commitments: Vec<StrategyCommitmentKind>,
}

pub fn challenger_decision_context(run_state: &RunState) -> ChallengerDecisionContext {
    let deck_plan = DeckPlanSnapshot::from_run_state(run_state);
    let mut current_pressure = open_inventory_pressure(deck_plan.strategic_deficit);
    if let Some(boss) = run_state.boss_key {
        let profile = boss_mechanic_pressure_profile_v1(run_state, boss);
        for answer in profile.missing_answers {
            for axis in axes_for_missing_answer(answer) {
                merge_open_hypothesis(
                    &mut current_pressure,
                    open_hypothesis(
                        axis,
                        EvidenceConfidence::Medium,
                        PressureEvidenceSource::EncounterThreat,
                        answer.label(),
                    ),
                );
            }
        }
    }
    current_pressure.sort_by_key(|hypothesis| hypothesis.axis);

    let mut automatic_commitments = Vec::new();
    if deck_plan.roles.exhaust_stream_units >= 2 && deck_plan.roles.exhaust_payoff_units == 0 {
        automatic_commitments.push(StrategyCommitmentKind::ExhaustEngine);
    }

    ChallengerDecisionContext {
        deck_plan,
        gold: run_state.gold,
        current_pressure,
        automatic_commitments,
    }
}

pub fn open_inventory_pressure(facts: DeckStrategicDeficitSummary) -> Vec<PressureHypothesis> {
    let mut hypotheses = Vec::new();
    push_static_if_open(
        &mut hypotheses,
        facts.frontload_damage,
        PressureAxis::ResolutionTempo,
        "frontload inventory is missing or thin",
    );
    push_static_if_open(
        &mut hypotheses,
        facts.aoe_or_minion_control,
        PressureAxis::MultiTargetControl,
        "multi-target inventory is missing or thin",
    );
    push_static_if_open(
        &mut hypotheses,
        facts.block_or_mitigation,
        PressureAxis::DelayCapacity,
        "delay inventory is missing or thin",
    );
    push_static_if_open(
        &mut hypotheses,
        facts.boss_scaling_plan,
        PressureAxis::GrowthHorizon,
        "scaling inventory is missing or thin",
    );
    if is_open(facts.deck_access) || is_open(facts.energy_or_playability) {
        merge_open_hypothesis(
            &mut hypotheses,
            open_hypothesis(
                PressureAxis::Deployability,
                EvidenceConfidence::Low,
                PressureEvidenceSource::DeckCapability,
                "access or playability inventory is missing or thin",
            ),
        );
    }
    hypotheses.sort_by_key(|hypothesis| hypothesis.axis);
    hypotheses
}

fn axes_for_missing_answer(answer: BossMechanicMissingAnswerV1) -> Vec<PressureAxis> {
    use BossMechanicMissingAnswerV1::*;
    match answer {
        DarkEchoBlockPlan | ExecuteBlockPlan => vec![PressureAxis::DelayCapacity],
        HasteBurstOrSetupPlan | ChampTransitionBurst => {
            vec![PressureAxis::ResolutionTempo, PressureAxis::GrowthHorizon]
        }
        SplitTimingPlan | FocusedKillOrderPlan => vec![PressureAxis::ResolutionTempo],
        PostSplitDamagePlan | CollectorMinionPlan => vec![PressureAxis::MultiTargetControl],
        Block50OrKillBeforeBeam => vec![PressureAxis::DelayCapacity, PressureAxis::ResolutionTempo],
        StasisRecoveryPlan => vec![PressureAxis::Deployability],
        PhasePowerPlan | TimeWarpCounterPlan | ArtifactStripPlan | TurnFourDebuffPlan => Vec::new(),
    }
}

fn is_open(level: StrategicDeficitLevel) -> bool {
    matches!(
        level,
        StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
    )
}

fn push_static_if_open(
    hypotheses: &mut Vec<PressureHypothesis>,
    level: StrategicDeficitLevel,
    axis: PressureAxis,
    label: &'static str,
) {
    if is_open(level) {
        merge_open_hypothesis(
            hypotheses,
            open_hypothesis(
                axis,
                EvidenceConfidence::Low,
                PressureEvidenceSource::DeckCapability,
                label,
            ),
        );
    }
}

fn open_hypothesis(
    axis: PressureAxis,
    confidence: EvidenceConfidence,
    source: PressureEvidenceSource,
    label: &'static str,
) -> PressureHypothesis {
    PressureHypothesis {
        axis,
        coverage: PressureCoverage::Open,
        confidence,
        supporting_evidence: vec![PressureEvidence {
            source,
            label: label.to_string(),
        }],
        contradicting_evidence: Vec::new(),
    }
}

fn merge_open_hypothesis(hypotheses: &mut Vec<PressureHypothesis>, incoming: PressureHypothesis) {
    let Some(existing) = hypotheses
        .iter_mut()
        .find(|hypothesis| hypothesis.axis == incoming.axis)
    else {
        hypotheses.push(incoming);
        return;
    };
    existing.coverage = PressureCoverage::Open;
    existing.confidence = existing.confidence.max(incoming.confidence);
    for evidence in incoming.supporting_evidence {
        if !existing.supporting_evidence.contains(&evidence) {
            existing.supporting_evidence.push(evidence);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::strategy::candidate_pressure_response::StrategyCommitmentKind;
    use crate::ai::strategy::pressure_assessment::{PressureAxis, PressureEvidenceSource};
    use crate::content::cards::CardId;
    use crate::content::monsters::factory::EncounterId;
    use crate::runtime::combat::CombatCard;
    use crate::state::run::RunState;

    fn replace_deck(run: &mut RunState, cards: &[CardId]) {
        run.master_deck = cards
            .iter()
            .enumerate()
            .map(|(index, &card)| CombatCard::for_scenario(card, 70_000 + index as u32))
            .collect();
    }

    #[test]
    fn two_exhaust_sources_without_payoff_open_one_payoff_commitment() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        replace_deck(&mut run, &[CardId::TrueGrit, CardId::BurningPact]);

        let context = challenger_decision_context(&run);

        assert_eq!(
            context.automatic_commitments,
            vec![StrategyCommitmentKind::ExhaustEngine]
        );
    }

    #[test]
    fn one_incidental_exhaust_source_does_not_open_commitment() {
        let mut run = RunState::new(2, 0, false, "Ironclad");
        replace_deck(&mut run, &[CardId::TrueGrit, CardId::Strike]);

        let context = challenger_decision_context(&run);

        assert!(context.automatic_commitments.is_empty());
    }

    #[test]
    fn automaton_block_or_kill_answer_opens_both_safe_response_axes() {
        let mut run = RunState::new(3, 0, false, "Ironclad");
        run.act_num = 2;
        run.boss_key = Some(EncounterId::Automaton);
        replace_deck(&mut run, &[CardId::Strike, CardId::Defend, CardId::Bash]);

        let context = challenger_decision_context(&run);

        assert!(context.current_pressure.iter().any(|hypothesis| {
            hypothesis.axis == PressureAxis::DelayCapacity
                && hypothesis.supporting_evidence.iter().any(|evidence| {
                    evidence.source == PressureEvidenceSource::EncounterThreat
                        && evidence.label == "block50_or_kill_before_beam"
                })
        }));
        assert!(context.current_pressure.iter().any(|hypothesis| {
            hypothesis.axis == PressureAxis::ResolutionTempo
                && hypothesis.supporting_evidence.iter().any(|evidence| {
                    evidence.source == PressureEvidenceSource::EncounterThreat
                        && evidence.label == "block50_or_kill_before_beam"
                })
        }));
    }
}
