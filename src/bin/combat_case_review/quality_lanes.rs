#[path = "quality_lanes/feedback.rs"]
mod feedback;
#[path = "quality_lanes/feedback_comparison.rs"]
mod feedback_comparison;
#[path = "quality_lanes/feedback_progress.rs"]
mod feedback_progress;
#[path = "quality_lanes/feedback_selection.rs"]
mod feedback_selection;
#[cfg(test)]
#[path = "quality_lanes/feedback_tests.rs"]
mod feedback_tests;
#[path = "quality_lanes/quality.rs"]
mod quality;
#[path = "quality_lanes/specs.rs"]
mod specs;
#[path = "quality_lanes/types.rs"]
mod types;

use sts_oracle_runtime::eval::combat_case::CombatCase;

pub(crate) use quality::{combat_line_quality, compare_quality, witness_line_from_trajectory};
pub(crate) use specs::quality_lane_specs;
pub(crate) use types::CombatLineQuality;
pub(super) use types::CombatQualityLaneReview;

use super::options::ReviewOptions;
use super::search_runner::run_config_search;
use feedback::run_success_feedback_rerun;
use feedback_selection::CombatFeedbackSourcePicker;
use types::CombatQualityLaneResult;

pub(super) fn run_quality_lanes(
    options: &ReviewOptions,
    case: &CombatCase,
) -> CombatQualityLaneReview {
    let specs = quality_lane_specs();
    let lane_count = specs.len().max(1);
    let total_nodes = options
        .quality_lane_total_nodes
        .unwrap_or(options.slow_nodes)
        .max(1);
    let total_wall_ms = options
        .quality_lane_total_ms
        .unwrap_or(options.slow_ms)
        .max(1);
    let per_lane_nodes = (total_nodes / lane_count).max(1);
    let per_lane_wall_ms = (total_wall_ms / lane_count as u64).max(1);
    let mut lanes = Vec::new();
    let mut feedback_sources = CombatFeedbackSourcePicker::default();
    for lane in specs {
        let (review, report) = run_config_search(
            lane.label,
            case,
            lane.config(per_lane_nodes, per_lane_wall_ms),
            options.action_preview_limit,
        );
        let quality = combat_line_quality(&report);
        if let (Some(quality), Some(trajectory)) =
            (quality.as_ref(), report.best_win_trajectory.as_ref())
        {
            feedback_sources.consider_complete_win(lane, &review, quality, trajectory);
        } else {
            feedback_sources.consider_estimated_rollout(lane, &review);
        }
        lanes.push(CombatQualityLaneResult {
            lane: lane.label,
            intent: lane.intent,
            review,
            quality,
        });
    }
    let selected_lane = lanes
        .iter()
        .enumerate()
        .filter_map(|(index, lane)| lane.quality.as_ref().map(|quality| (index, quality)))
        .max_by(|(_, left), (_, right)| compare_quality(left, right))
        .map(|(index, _)| lanes[index].lane);
    let success_feedback_rerun = feedback_sources.into_source().and_then(|source| {
        run_success_feedback_rerun(
            case,
            source,
            per_lane_nodes,
            per_lane_wall_ms,
            options.action_preview_limit,
        )
    });

    CombatQualityLaneReview {
        schema: "combat_quality_lane_review_v0",
        contract: "case_level_experiment_only_same_total_budget_split_across_lanes_no_runner_policy_change",
        total_nodes,
        total_wall_ms,
        per_lane_nodes,
        per_lane_wall_ms,
        selected_lane,
        selected_reason: if selected_lane.is_some() {
            "best_complete_win_by_persistent_adjusted_hp_then_potion_conservation"
        } else {
            "no_lane_found_complete_win"
        },
        success_feedback_rerun,
        lanes,
    }
}
