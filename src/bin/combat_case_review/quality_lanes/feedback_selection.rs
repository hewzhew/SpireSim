use sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2TrajectoryReport;

use super::super::search_types::SearchReview;
use super::feedback::CombatSuccessFeedbackSource;
use super::feedback_progress::{
    estimated_rollout_feedback_rank, estimated_rollout_feedback_witness,
};
use super::quality::{compare_quality, witness_line_from_trajectory};
use super::specs::QualityLaneSpec;
use super::types::{CombatLineQuality, CombatSuccessFeedbackMetrics};

type EstimatedFeedbackRank = (i32, i32, i32, i32);

#[derive(Default)]
pub(super) struct CombatFeedbackSourcePicker {
    complete: Option<(CombatLineQuality, CombatSuccessFeedbackSource)>,
    estimated: Option<(EstimatedFeedbackRank, CombatSuccessFeedbackSource)>,
}

impl CombatFeedbackSourcePicker {
    pub(super) fn consider_complete_win(
        &mut self,
        lane: QualityLaneSpec,
        review: &SearchReview,
        quality: &CombatLineQuality,
        trajectory: &CombatSearchV2TrajectoryReport,
    ) {
        if self
            .complete
            .as_ref()
            .is_some_and(|(current, _)| compare_quality(quality, current).is_lt())
        {
            return;
        }
        self.complete = Some((
            quality.clone(),
            CombatSuccessFeedbackSource {
                spec: lane,
                baseline: CombatSuccessFeedbackMetrics::from_review(review),
                witness: witness_line_from_trajectory(lane.label, trajectory),
                source_kind: "complete_win",
            },
        ));
    }

    pub(super) fn consider_estimated_rollout(
        &mut self,
        lane: QualityLaneSpec,
        review: &SearchReview,
    ) {
        let Some(progress) = review.facts.diagnostic_progress.as_ref() else {
            return;
        };
        let Some(witness) = estimated_rollout_feedback_witness(lane.label, progress) else {
            return;
        };
        let rank = estimated_rollout_feedback_rank(progress);
        if self
            .estimated
            .as_ref()
            .is_some_and(|(current, _)| rank <= *current)
        {
            return;
        }
        self.estimated = Some((
            rank,
            CombatSuccessFeedbackSource {
                spec: lane,
                baseline: CombatSuccessFeedbackMetrics::from_review(review),
                witness,
                source_kind: "estimated_rollout_frontier",
            },
        ));
    }

    pub(super) fn into_source(self) -> Option<CombatSuccessFeedbackSource> {
        self.complete
            .map(|(_, source)| source)
            .or_else(|| self.estimated.map(|(_, source)| source))
    }
}
