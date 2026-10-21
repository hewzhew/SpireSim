use sts_oracle_runtime::ai::combat_search_v2::{CombatSearchV2Config, CombatSearchV2Report};
use sts_oracle_runtime::eval::combat_case::CombatCase;
use sts_oracle_runtime::eval::run_control::{
    adjudicate_combat_case_candidates_v1, adjudicate_combat_case_line_v1,
    probe_combat_case_persistent_burden_cutpoints_v1, CombatCaseAdjudicationProbeV1,
    CombatCaseCandidateAdjudicationCensusV1, CombatCasePersistentBurdenCutpointProbeV1,
};

pub(super) struct ReviewAdjudicationRun {
    pub(super) source_review: &'static str,
    pub(super) config: CombatSearchV2Config,
    pub(super) report: CombatSearchV2Report,
}

pub(super) fn run_candidate_censuses(
    enabled: bool,
    runs: &[ReviewAdjudicationRun],
    case: Option<&CombatCase>,
) -> Option<Vec<CombatCaseCandidateAdjudicationCensusV1>> {
    if !enabled {
        return None;
    }
    Some(
        runs.iter()
            .map(|run| match case {
                Some(case) => adjudicate_combat_case_candidates_v1(
                    run.source_review,
                    case,
                    &run.config,
                    &run.report,
                ),
                None => CombatCaseCandidateAdjudicationCensusV1::ProjectionFailed {
                    source_review: run.source_review.to_string(),
                    retained_candidate_count: run.report.best_win_trajectory.iter().count()
                        + run.report.win_candidate_trajectories.len(),
                    error: "combat case unavailable".to_string(),
                },
            })
            .collect(),
    )
}

pub(super) fn run_persistent_burden_cutpoint_probes(
    enabled: bool,
    runs: &[ReviewAdjudicationRun],
    case: Option<&CombatCase>,
) -> Option<Vec<CombatCasePersistentBurdenCutpointProbeV1>> {
    if !enabled {
        return None;
    }
    Some(
        runs.iter()
            .map(|run| match case {
                Some(case) => probe_combat_case_persistent_burden_cutpoints_v1(
                    run.source_review,
                    case,
                    &run.config,
                    &run.report,
                ),
                None => CombatCasePersistentBurdenCutpointProbeV1::ProjectionFailed {
                    source_review: run.source_review.to_string(),
                    error: "combat case unavailable".to_string(),
                },
            })
            .collect(),
    )
}

pub(super) fn run_adjudication_probe(
    enabled: bool,
    runs: &[ReviewAdjudicationRun],
    focus_label: Option<&str>,
    case: Option<&CombatCase>,
) -> Option<CombatCaseAdjudicationProbeV1> {
    if !enabled {
        return None;
    }
    let run = focus_label
        .and_then(|label| runs.iter().find(|run| run.source_review == label))
        .or_else(|| runs.first());
    let Some(run) = run else {
        return Some(CombatCaseAdjudicationProbeV1::NoCompleteLine);
    };
    let Some(trajectory) = run.report.best_win_trajectory.as_ref() else {
        return Some(CombatCaseAdjudicationProbeV1::NoCompleteLine);
    };
    let Some(case) = case else {
        return Some(CombatCaseAdjudicationProbeV1::ProjectionFailed {
            source_review: run.source_review.to_string(),
            error: "combat case unavailable".to_string(),
        });
    };
    Some(adjudicate_combat_case_line_v1(
        run.source_review,
        case,
        &run.config,
        trajectory,
    ))
}

#[cfg(test)]
mod tests {
    use sts_oracle_runtime::eval::run_control::CombatCaseAdjudicationProbeV1;

    #[test]
    fn enabled_probe_without_complete_line_is_typed() {
        assert_eq!(
            super::run_adjudication_probe(true, &[], None, None),
            Some(CombatCaseAdjudicationProbeV1::NoCompleteLine)
        );
    }

    #[test]
    fn disabled_probe_is_absent_from_review_artifacts() {
        assert_eq!(super::run_adjudication_probe(false, &[], None, None), None);
    }

    #[test]
    fn disabled_candidate_census_is_absent() {
        assert_eq!(super::run_candidate_censuses(false, &[], None), None);
    }

    #[test]
    fn disabled_persistent_burden_probe_is_absent() {
        assert_eq!(
            super::run_persistent_burden_cutpoint_probes(false, &[], None),
            None
        );
    }
}
