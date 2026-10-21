use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureAxis {
    ResolutionTempo,
    DelayCapacity,
    MultiTargetControl,
    GrowthHorizon,
    Deployability,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureCoverage {
    Open,
    PartiallyCovered,
    Covered,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceConfidence {
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchCoverage {
    Complete,
    Limited,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureEvidenceSource {
    EncounterThreat,
    DeckCapability,
    ObservedOutcome,
    SearchCoverage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PressureEvidence {
    pub source: PressureEvidenceSource,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PressureHypothesis {
    pub axis: PressureAxis,
    pub coverage: PressureCoverage,
    pub confidence: EvidenceConfidence,
    pub supporting_evidence: Vec<PressureEvidence>,
    pub contradicting_evidence: Vec<PressureEvidence>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurvivalPressureContract {
    pub threat_turns: Option<u8>,
    pub resolution_turns: Option<u8>,
    pub finite_delay_turns: u8,
    pub repeatable_delay: bool,
    pub deployability: PressureCoverage,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OutcomePressureEvidence {
    pub hp_loss: u16,
    pub died: bool,
    pub search_coverage: SearchCoverage,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnresolvedOutcomePressure {
    pub unresolved: bool,
    pub attributed_axis: Option<PressureAxis>,
    pub search_coverage: SearchCoverage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PressureAssessment {
    pub overall: PressureCoverage,
    pub effective_horizon_turns: Option<u8>,
    pub hypotheses: Vec<PressureHypothesis>,
}

pub fn outcome_pressure_evidence(
    evidence: OutcomePressureEvidence,
) -> Option<UnresolvedOutcomePressure> {
    (evidence.hp_loss > 0 || evidence.died).then_some(UnresolvedOutcomePressure {
        unresolved: true,
        attributed_axis: None,
        search_coverage: evidence.search_coverage,
    })
}

pub fn assess_survival_pressure(contract: SurvivalPressureContract) -> PressureAssessment {
    let (Some(threat_turns), Some(resolution_turns)) =
        (contract.threat_turns, contract.resolution_turns)
    else {
        return PressureAssessment {
            overall: PressureCoverage::Unknown,
            effective_horizon_turns: contract.threat_turns,
            hypotheses: Vec::new(),
        };
    };

    let effective_horizon_turns = if contract.repeatable_delay {
        u8::MAX
    } else {
        threat_turns.saturating_add(contract.finite_delay_turns)
    };
    let resolves_in_time = resolution_turns <= effective_horizon_turns;
    let overall = if !resolves_in_time {
        PressureCoverage::Open
    } else if contract.deployability == PressureCoverage::Covered {
        PressureCoverage::Covered
    } else {
        PressureCoverage::PartiallyCovered
    };

    PressureAssessment {
        overall,
        effective_horizon_turns: Some(effective_horizon_turns),
        hypotheses: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hp_loss_opens_unresolved_pressure_without_assigning_an_axis() {
        let evidence = outcome_pressure_evidence(OutcomePressureEvidence {
            hp_loss: 12,
            died: false,
            search_coverage: SearchCoverage::Complete,
        })
        .expect("positive hp loss should be recorded");

        assert!(evidence.unresolved);
        assert_eq!(evidence.attributed_axis, None);
    }

    #[test]
    fn fast_resolution_covers_a_short_horizon_without_delay() {
        let assessment = assess_survival_pressure(SurvivalPressureContract {
            threat_turns: Some(3),
            resolution_turns: Some(2),
            finite_delay_turns: 0,
            repeatable_delay: false,
            deployability: PressureCoverage::Covered,
        });

        assert_eq!(assessment.overall, PressureCoverage::Covered);
        assert_eq!(assessment.effective_horizon_turns, Some(3));
    }

    #[test]
    fn finite_delay_cannot_cover_resolution_beyond_the_extended_horizon() {
        let assessment = assess_survival_pressure(SurvivalPressureContract {
            threat_turns: Some(2),
            resolution_turns: Some(5),
            finite_delay_turns: 2,
            repeatable_delay: false,
            deployability: PressureCoverage::Covered,
        });

        assert_eq!(assessment.overall, PressureCoverage::Open);
        assert_eq!(assessment.effective_horizon_turns, Some(4));
    }

    #[test]
    fn unknown_deployability_prevents_full_coverage() {
        let assessment = assess_survival_pressure(SurvivalPressureContract {
            threat_turns: Some(3),
            resolution_turns: Some(2),
            finite_delay_turns: 0,
            repeatable_delay: false,
            deployability: PressureCoverage::Unknown,
        });

        assert_eq!(assessment.overall, PressureCoverage::PartiallyCovered);
    }

    #[test]
    fn missing_horizon_or_resolution_stays_unknown() {
        let assessment = assess_survival_pressure(SurvivalPressureContract {
            threat_turns: None,
            resolution_turns: Some(2),
            finite_delay_turns: 1,
            repeatable_delay: false,
            deployability: PressureCoverage::Covered,
        });

        assert_eq!(assessment.overall, PressureCoverage::Unknown);
    }
}
