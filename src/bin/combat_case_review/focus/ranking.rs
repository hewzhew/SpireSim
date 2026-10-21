use sts_oracle_runtime::ai::combat_search_v2::SearchTerminalLabel;

use super::super::search_types::{SearchDiagnosticProgressFacts, SearchReview};
use super::types::CombatReviewFocus;

pub(crate) fn review_focus(ladder: &[SearchReview]) -> Option<CombatReviewFocus> {
    let mut selected: Option<(&SearchReview, &SearchDiagnosticProgressFacts)> = None;
    for review in ladder {
        let Some(progress) = review.facts.diagnostic_progress.as_ref() else {
            continue;
        };
        if selected
            .map(|(_, current)| progress_is_better_focus(progress, current))
            .unwrap_or(true)
        {
            selected = Some((review, progress));
        }
    }
    selected.map(|(review, progress)| CombatReviewFocus {
        selected_review: review.label,
        reason: focus_reason(progress),
        progress: progress.clone(),
    })
}

pub(super) fn progress_is_better_focus(
    candidate: &SearchDiagnosticProgressFacts,
    current: &SearchDiagnosticProgressFacts,
) -> bool {
    match (
        candidate.terminal == SearchTerminalLabel::Win,
        current.terminal == SearchTerminalLabel::Win,
    ) {
        (true, false) => return true,
        (false, true) => return false,
        (true, true) => {
            return (candidate.final_hp, -(candidate.potions_used as i32))
                > (current.final_hp, -(current.potions_used as i32));
        }
        (false, false) => {}
    }

    (
        -(candidate.half_dead_enemy_count as i32),
        -candidate.total_enemy_hp,
        -(candidate.living_enemy_count as i32),
        candidate.turns as i32,
        candidate.final_hp,
        -(candidate.potions_used as i32),
    ) > (
        -(current.half_dead_enemy_count as i32),
        -current.total_enemy_hp,
        -(current.living_enemy_count as i32),
        current.turns as i32,
        current.final_hp,
        -(current.potions_used as i32),
    )
}

pub(super) fn focus_reason(progress: &SearchDiagnosticProgressFacts) -> &'static str {
    if progress.terminal == SearchTerminalLabel::Win {
        "complete_win_available"
    } else if progress.final_hp <= 0 && progress.half_dead_enemy_count > 0 {
        "phase_pending_enemy_player_died"
    } else {
        "closest_failure_progress_by_enemy_hp"
    }
}
