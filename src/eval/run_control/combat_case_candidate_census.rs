use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::ai::combat_search_v2::{CombatSearchV2Config, CombatSearchV2Report};
use crate::content::cards::CardId;
use crate::eval::combat_case::CombatCase;

use super::combat_candidate_line::CombatCandidateLine;
use super::combat_case_adjudication::{
    adjudicate_observed_outcome, project_combat_case_session, COMBAT_CASE_PROJECTION_TRUST_V1,
};
use super::combat_case_retained_candidates::unique_retained_win_trajectories;
use super::combat_line_adjudication::{
    CombatLineAdjudicationV1, CombatLineCleanlinessV1, CombatLineObservedOutcomeV1,
    CombatLineRejectionReasonV1,
};
use super::combat_line_outcome::{evaluate_combat_candidate_line_outcome, prefer_accepted_outcome};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatCaseCandidateCensusConclusionV1 {
    CleanCandidatePresent,
    AllReplayedCandidatesDirty,
    IncompleteDueToReplayFailures,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatCaseCandidateReplayFailureV1 {
    pub retained_index: usize,
    pub action_count: usize,
    pub error: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatCaseCandidateOutcomeSummaryV1 {
    pub retained_index: usize,
    pub observed_outcome: CombatLineObservedOutcomeV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatCaseGainedCurseCountV1 {
    pub card: CardId,
    pub candidate_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CombatCaseCandidateAdjudicationCensusV1 {
    NoRetainedCandidates {
        source_review: String,
        retained_candidate_count: usize,
    },
    ProjectionFailed {
        source_review: String,
        retained_candidate_count: usize,
        error: String,
    },
    Adjudicated {
        source_review: String,
        projection_trust: String,
        retained_candidate_count: usize,
        unique_candidate_count: usize,
        replayed_candidate_count: usize,
        replay_failures: Vec<CombatCaseCandidateReplayFailureV1>,
        clean_accepted_count: usize,
        new_curse_rejected_count: usize,
        gained_curse_counts: Vec<CombatCaseGainedCurseCountV1>,
        best_clean_candidate: Option<CombatCaseCandidateOutcomeSummaryV1>,
        conclusion: CombatCaseCandidateCensusConclusionV1,
    },
}

impl CombatCaseCandidateAdjudicationCensusV1 {
    pub fn source_review(&self) -> &str {
        match self {
            Self::NoRetainedCandidates { source_review, .. }
            | Self::ProjectionFailed { source_review, .. }
            | Self::Adjudicated { source_review, .. } => source_review,
        }
    }
}

type CandidateEvaluation =
    Result<(usize, CombatLineObservedOutcomeV1), CombatCaseCandidateReplayFailureV1>;

pub fn adjudicate_combat_case_candidates_v1(
    source_review: impl Into<String>,
    case: &CombatCase,
    config: &CombatSearchV2Config,
    report: &CombatSearchV2Report,
) -> CombatCaseCandidateAdjudicationCensusV1 {
    let source_review = source_review.into();
    let retained = unique_retained_win_trajectories(report);
    let retained_candidate_count = retained.retained_candidate_count;
    if retained_candidate_count == 0 {
        return empty_census(source_review);
    }
    let unique_candidate_count = retained.trajectories.len();
    let session = match project_combat_case_session(case) {
        Ok(session) => session,
        Err(error) => {
            return CombatCaseCandidateAdjudicationCensusV1::ProjectionFailed {
                source_review,
                retained_candidate_count,
                error,
            };
        }
    };

    let mut evaluations = Vec::new();
    for retained in retained.trajectories {
        let action_count = retained.trajectory.actions.len();
        let evaluation = CombatCandidateLine::try_from_search_trajectory(retained.trajectory)
            .and_then(|line| {
                evaluate_combat_candidate_line_outcome(&session, &case.core.position, config, line)
            })
            .map(|evaluation| (retained.retained_index, evaluation.outcome))
            .map_err(|error| CombatCaseCandidateReplayFailureV1 {
                retained_index: retained.retained_index,
                action_count,
                error,
            });
        evaluations.push(evaluation);
    }

    summarize_evaluations(
        source_review,
        retained_candidate_count,
        unique_candidate_count,
        evaluations,
    )
}

fn empty_census(source_review: String) -> CombatCaseCandidateAdjudicationCensusV1 {
    CombatCaseCandidateAdjudicationCensusV1::NoRetainedCandidates {
        source_review,
        retained_candidate_count: 0,
    }
}

fn summarize_evaluations(
    source_review: String,
    retained_candidate_count: usize,
    unique_candidate_count: usize,
    evaluations: Vec<CandidateEvaluation>,
) -> CombatCaseCandidateAdjudicationCensusV1 {
    let mut replay_failures = Vec::new();
    let mut replayed_candidate_count = 0usize;
    let mut clean_accepted_count = 0usize;
    let mut new_curse_rejected_count = 0usize;
    let mut gained_curse_counts = HashMap::<CardId, usize>::new();
    let mut best_clean_candidate: Option<CombatCaseCandidateOutcomeSummaryV1> = None;

    for evaluation in evaluations {
        let (retained_index, outcome) = match evaluation {
            Ok(evaluation) => evaluation,
            Err(failure) => {
                replay_failures.push(failure);
                continue;
            }
        };
        replayed_candidate_count += 1;
        let adjudications = adjudicate_observed_outcome(outcome.clone());
        let clean_only = adjudications
            .into_iter()
            .find(|adjudication| {
                matches!(
                    adjudication,
                    CombatLineAdjudicationV1::Accepted {
                        policy: crate::ai::combat_search_v2::CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse,
                        ..
                    } | CombatLineAdjudicationV1::Rejected {
                        policy: crate::ai::combat_search_v2::CombatSearchAcceptancePluginId::CleanAcceptedLineNoNewCurse,
                        ..
                    }
                )
            })
            .expect("probe policies include clean-only adjudication");
        match clean_only {
            CombatLineAdjudicationV1::Accepted {
                cleanliness: CombatLineCleanlinessV1::Clean,
                ..
            } => {
                clean_accepted_count += 1;
                let candidate = CombatCaseCandidateOutcomeSummaryV1 {
                    retained_index,
                    observed_outcome: outcome,
                };
                let replace = best_clean_candidate
                    .as_ref()
                    .map(|best| {
                        prefer_accepted_outcome(&candidate.observed_outcome, &best.observed_outcome)
                    })
                    .unwrap_or(true);
                if replace {
                    best_clean_candidate = Some(candidate);
                }
            }
            CombatLineAdjudicationV1::Rejected {
                reason: CombatLineRejectionReasonV1::NewCurse { cards },
                ..
            } => {
                new_curse_rejected_count += 1;
                let unique_cards = cards
                    .into_iter()
                    .map(|card| card.id)
                    .collect::<HashSet<_>>();
                for card in unique_cards {
                    *gained_curse_counts.entry(card).or_default() += 1;
                }
            }
            other => panic!("unexpected clean-only adjudication: {other:?}"),
        }
    }

    let conclusion = if best_clean_candidate.is_some() {
        CombatCaseCandidateCensusConclusionV1::CleanCandidatePresent
    } else if !replay_failures.is_empty() {
        CombatCaseCandidateCensusConclusionV1::IncompleteDueToReplayFailures
    } else {
        CombatCaseCandidateCensusConclusionV1::AllReplayedCandidatesDirty
    };
    let mut gained_curse_counts = gained_curse_counts
        .into_iter()
        .map(|(card, candidate_count)| CombatCaseGainedCurseCountV1 {
            card,
            candidate_count,
        })
        .collect::<Vec<_>>();
    gained_curse_counts.sort_by_key(|entry| entry.card as i32);

    CombatCaseCandidateAdjudicationCensusV1::Adjudicated {
        source_review,
        projection_trust: COMBAT_CASE_PROJECTION_TRUST_V1.to_string(),
        retained_candidate_count,
        unique_candidate_count,
        replayed_candidate_count,
        replay_failures,
        clean_accepted_count,
        new_curse_rejected_count,
        gained_curse_counts,
        best_clean_candidate,
        conclusion,
    }
}

#[cfg(test)]
mod tests {
    use crate::content::cards::CardId;
    use crate::eval::run_control::combat_line_adjudication::CombatLineObservedOutcomeV1;
    use crate::eval::run_control::transition_report::CardSnapshot;
    use crate::sim::combat::CombatTerminal;

    use super::{
        empty_census, summarize_evaluations, CombatCaseCandidateAdjudicationCensusV1,
        CombatCaseCandidateCensusConclusionV1, CombatCaseCandidateReplayFailureV1,
    };

    fn clean_outcome(final_hp: i32) -> CombatLineObservedOutcomeV1 {
        CombatLineObservedOutcomeV1 {
            terminal: CombatTerminal::Win,
            final_hp,
            hp_loss: 44 - final_hp,
            potions_used: 0,
            action_count: 8,
            gold_delta: 0,
            ritual_dagger_growth: 0,
            gained_curses: Vec::new(),
        }
    }

    fn parasite_outcome(final_hp: i32) -> CombatLineObservedOutcomeV1 {
        CombatLineObservedOutcomeV1 {
            gained_curses: vec![CardSnapshot {
                id: CardId::Parasite,
                lineage: crate::test_support::scenario_lineage(9001),
                upgrades: 0,
            }],
            ..clean_outcome(final_hp)
        }
    }

    #[test]
    fn census_distinguishes_clean_and_dirty_candidate_outcomes() {
        let result = summarize_evaluations(
            "lane".to_string(),
            3,
            2,
            vec![Ok((0, clean_outcome(30))), Ok((2, parasite_outcome(44)))],
        );

        let CombatCaseCandidateAdjudicationCensusV1::Adjudicated {
            retained_candidate_count,
            unique_candidate_count,
            replayed_candidate_count,
            replay_failures,
            clean_accepted_count,
            new_curse_rejected_count,
            gained_curse_counts,
            best_clean_candidate,
            conclusion,
            ..
        } = result
        else {
            panic!("expected adjudicated census")
        };
        assert_eq!(retained_candidate_count, 3);
        assert_eq!(unique_candidate_count, 2);
        assert_eq!(replayed_candidate_count, 2);
        assert!(replay_failures.is_empty());
        assert_eq!(clean_accepted_count, 1);
        assert_eq!(new_curse_rejected_count, 1);
        assert_eq!(gained_curse_counts[0].card, CardId::Parasite);
        assert_eq!(gained_curse_counts[0].candidate_count, 1);
        assert_eq!(
            best_clean_candidate
                .expect("clean candidate")
                .retained_index,
            0
        );
        assert_eq!(
            conclusion,
            CombatCaseCandidateCensusConclusionV1::CleanCandidatePresent
        );
    }

    #[test]
    fn replay_failure_prevents_all_dirty_conclusion() {
        let result = summarize_evaluations(
            "lane".to_string(),
            2,
            2,
            vec![
                Ok((0, parasite_outcome(44))),
                Err(CombatCaseCandidateReplayFailureV1 {
                    retained_index: 1,
                    action_count: 7,
                    error: "drift".to_string(),
                }),
            ],
        );

        let CombatCaseCandidateAdjudicationCensusV1::Adjudicated { conclusion, .. } = result else {
            panic!("expected adjudicated census")
        };
        assert_eq!(
            conclusion,
            CombatCaseCandidateCensusConclusionV1::IncompleteDueToReplayFailures
        );
    }

    #[test]
    fn no_candidates_has_a_typed_conclusion() {
        assert!(matches!(
            empty_census("lane".to_string()),
            CombatCaseCandidateAdjudicationCensusV1::NoRetainedCandidates {
                source_review,
                retained_candidate_count: 0,
            } if source_review == "lane"
        ));
    }
}
