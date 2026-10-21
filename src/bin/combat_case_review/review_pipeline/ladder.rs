use sts_oracle_runtime::ai::combat_search_v2::{
    CombatSearchProfile, CombatSearchV2Report, CombatSearchV2TrajectoryReport,
};
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::super::adjudication_probe::ReviewAdjudicationRun;
use super::super::options::ReviewOptions;
use super::super::search_runner::{
    review_all_potions_profile, review_no_potion_profile, run_config_search,
};
use super::super::search_types::SearchReview;

pub(super) struct ReviewLadderRun {
    pub(super) reviews: Vec<SearchReview>,
    pub(super) line_lab_parent: Option<CombatSearchV2TrajectoryReport>,
    pub(super) adjudication_runs: Vec<ReviewAdjudicationRun>,
}

struct LadderProfileRun {
    source_review: &'static str,
    review: SearchReview,
    config: sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2Config,
    report: CombatSearchV2Report,
}

pub(super) fn run_review_ladder(options: &ReviewOptions, case: &CombatCase) -> ReviewLadderRun {
    if !options.ladder {
        return ReviewLadderRun {
            reviews: Vec::new(),
            line_lab_parent: None,
            adjudication_runs: Vec::new(),
        };
    }

    let fast_profile = review_no_potion_profile(
        "fast_no_potion_diagnostic",
        options.fast_nodes,
        options.fast_ms,
        options,
    );
    let fast = run_ladder_profile(case, fast_profile, options);

    let slow_profile = review_all_potions_profile(
        "slow_potion_diagnostic",
        options.slow_nodes,
        options.slow_ms,
        options,
    );
    let slow = run_ladder_profile(case, slow_profile, options);

    let line_lab_parent = slow.report.best_complete_trajectory.clone();
    let LadderProfileRun {
        source_review: fast_source_review,
        review: fast_review,
        config: fast_config,
        report: fast_report,
    } = fast;
    let LadderProfileRun {
        source_review: slow_source_review,
        review: slow_review,
        config: slow_config,
        report: slow_report,
    } = slow;
    let reviews = vec![fast_review, slow_review];
    let adjudication_runs = vec![
        ReviewAdjudicationRun {
            source_review: fast_source_review,
            config: fast_config,
            report: fast_report,
        },
        ReviewAdjudicationRun {
            source_review: slow_source_review,
            config: slow_config,
            report: slow_report,
        },
    ];

    ReviewLadderRun {
        reviews,
        line_lab_parent,
        adjudication_runs,
    }
}

fn run_ladder_profile(
    case: &CombatCase,
    profile: CombatSearchProfile,
    options: &ReviewOptions,
) -> LadderProfileRun {
    let label = profile.label;
    let mut config = profile.to_config();
    if let Some(max_actions) = options.rollout_max_actions {
        config.rollout_max_actions = max_actions;
    }
    if let Some(max_evaluations) = options.rollout_max_evaluations {
        config.rollout_max_evaluations = max_evaluations;
    }
    let (review, report) =
        run_config_search(label, case, config.clone(), options.action_preview_limit);
    LadderProfileRun {
        source_review: label,
        review,
        config,
        report,
    }
}
