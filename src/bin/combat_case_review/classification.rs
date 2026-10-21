use serde::Serialize;
use sts_oracle_runtime::ai::combat_search_v2::SearchTerminalLabel;
use sts_oracle_runtime::eval::run_control::AtomicCombatSearchTraceSummaryV2;

use super::focus::CombatReviewFocus;
use super::search_types::{SearchDiagnosticProgressFacts, SearchReview};

#[derive(Serialize)]
pub(super) struct CombatGapReviewClassification {
    pub(super) kind: &'static str,
    reason: &'static str,
    basis_review: Option<&'static str>,
}

pub(super) fn classify_gap_review(
    saved_atomic_combat_search: Option<&AtomicCombatSearchTraceSummaryV2>,
    ladder: &[SearchReview],
    focus: Option<&CombatReviewFocus>,
) -> CombatGapReviewClassification {
    if saved_atomic_combat_search
        .is_some_and(|search| search.complete_win_found || search.best_win.is_some())
    {
        return classification(
            "SavedCompleteWinRejectedByPolicy",
            "saved_complete_win_present_in_case",
            Some("saved_atomic_combat_search"),
        );
    }
    if ladder.is_empty() {
        return classification("NotReviewed", "ladder_not_requested", None);
    }
    if let Some(review) = ladder.iter().find(|review| review.complete_win) {
        return if review.potions_used.unwrap_or(0) > 0 {
            classification(
                "PotionRescueWon",
                "win_found_using_potions",
                Some(review.label),
            )
        } else {
            classification(
                "SearchMissWonWithReview",
                "win_found_with_review_budget",
                Some(review.label),
            )
        };
    }
    let review = ladder
        .last()
        .expect("non-empty ladder was checked before classification");
    if search_starved_by_rollout(review) {
        return classification(
            "SearchStarvedByRollout",
            "rollout_pct_high_and_nodes_low",
            Some(review.label),
        );
    }
    if review.deadline_hit && review.nodes_expanded < 1_000 {
        return classification(
            "TimeoutNoConclusion",
            "deadline_hit_with_too_few_exact_nodes",
            Some(review.label),
        );
    }
    if let Some(focus) = focus {
        if is_exact_near_miss_loss(&focus.progress) {
            return classification(
                "NearMissNoWinAfterReview",
                "exact_loss_reached_single_enemy_with_low_remaining_hp",
                Some(focus.selected_review),
            );
        }
    }
    classification(
        "StillNoWinAfterReview",
        "no_win_after_review_budget",
        Some(review.label),
    )
}

fn is_exact_near_miss_loss(progress: &SearchDiagnosticProgressFacts) -> bool {
    progress.source == "best_complete"
        && progress.terminal == SearchTerminalLabel::Loss
        && !progress.estimated
        && progress.living_enemy_count == 1
        && progress.total_enemy_hp <= 10
}

fn classification(
    kind: &'static str,
    reason: &'static str,
    basis_review: Option<&'static str>,
) -> CombatGapReviewClassification {
    CombatGapReviewClassification {
        kind,
        reason,
        basis_review,
    }
}

fn search_starved_by_rollout(review: &SearchReview) -> bool {
    review.nodes_expanded < 500 && rollout_pct(review) >= 75.0
}

fn rollout_pct(review: &SearchReview) -> f64 {
    if review.performance.total_us == 0 {
        return 0.0;
    }
    100.0 * review.performance.rollout_us as f64 / review.performance.total_us as f64
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use sts_oracle_runtime::eval::run_control::AtomicCombatSearchTraceSummaryV2;

    use super::*;

    fn saved_atomic_combat_search(
        complete_win_found: bool,
        include_best_win: bool,
    ) -> AtomicCombatSearchTraceSummaryV2 {
        let best_win = include_best_win.then(|| {
            json!({
                "terminal": SearchTerminalLabel::Win,
                "final_hp": 13,
                "hp_loss": 32,
                "turns": 6,
                "cards_played": 23,
                "potions_used": 1,
                "potions_discarded": 0,
                "action_count": 32
            })
        });
        serde_json::from_value(json!({
            "source": "search_combat_rejected",
            "act": 2,
            "floor": 23,
            "turn": 0,
            "combat_kind": "elite",
            "enemies": ["Book of Stabbing"],
            "coverage_status": "TimeBudgetLimited",
            "complete_trajectory_found": true,
            "complete_win_found": complete_win_found,
            "best_win": best_win,
            "deadline_hit": true,
            "nodes_expanded": 3544,
            "terminal_wins": 84,
            "total_us": 5_067_038
        }))
        .expect("valid saved search summary")
    }

    #[test]
    fn missing_ladder_is_not_reviewed() {
        let classification = classify_gap_review(None, &[], None);

        assert_eq!(classification.kind, "NotReviewed");
        assert_eq!(classification.reason, "ladder_not_requested");
        assert_eq!(classification.basis_review, None);
    }

    #[test]
    fn saved_complete_win_precedes_missing_ladder_win() {
        let saved = saved_atomic_combat_search(true, true);

        let classification = classify_gap_review(Some(&saved), &[], None);

        assert_eq!(classification.kind, "SavedCompleteWinRejectedByPolicy");
        assert_eq!(classification.reason, "saved_complete_win_present_in_case");
        assert_eq!(
            classification.basis_review,
            Some("saved_atomic_combat_search")
        );
    }

    #[test]
    fn legacy_best_win_proves_saved_complete_win() {
        let saved = saved_atomic_combat_search(false, true);

        let classification = classify_gap_review(Some(&saved), &[], None);

        assert_eq!(classification.kind, "SavedCompleteWinRejectedByPolicy");
    }

    #[test]
    fn saved_atomic_combat_search_without_win_preserves_existing_classification() {
        let saved = saved_atomic_combat_search(false, false);

        let classification = classify_gap_review(Some(&saved), &[], None);

        assert_eq!(classification.kind, "NotReviewed");
        assert_eq!(classification.reason, "ladder_not_requested");
    }
}
