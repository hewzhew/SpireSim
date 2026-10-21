use sts_oracle_runtime::ai::combat_search_v2::SearchTerminalLabel;
use sts_oracle_runtime::eval::combat_case::CombatCase;

use super::super::classification::CombatGapReviewClassification;
use super::super::search_types::{SearchDiagnosticProgressFacts, SearchReview};

pub(super) struct CombatStrategicSignalContext {
    pub(super) no_exact_win: bool,
    pub(super) no_win_after_review: bool,
    pub(super) exact_loss: bool,
    pub(super) rollout_win: bool,
    pub(super) low_hp_start: bool,
}

impl CombatStrategicSignalContext {
    pub(super) fn new(
        case: &CombatCase,
        classification: &CombatGapReviewClassification,
        progress: Option<&SearchDiagnosticProgressFacts>,
        ladder: &[SearchReview],
    ) -> Self {
        Self {
            no_exact_win: !ladder.iter().any(|review| review.complete_win),
            no_win_after_review: matches!(
                classification.kind,
                "StillNoWinAfterReview" | "NearMissNoWinAfterReview" | "SearchStarvedByRollout"
            ),
            exact_loss: progress.is_some_and(|progress| {
                progress.source == "best_complete" && progress.terminal == SearchTerminalLabel::Loss
            }),
            rollout_win: progress.is_some_and(|progress| {
                progress.source == "rollout_frontier"
                    && progress.terminal == SearchTerminalLabel::Win
            }),
            low_hp_start: case.core.run.hp * 100 <= case.core.run.max_hp * 20,
        }
    }
}
