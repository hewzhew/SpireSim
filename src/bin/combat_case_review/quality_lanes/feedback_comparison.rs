use super::super::search_types::SearchReview;
use super::types::{CombatSuccessFeedbackComparison, CombatSuccessFeedbackMetrics};

pub(super) fn compare_success_feedback(
    baseline: &CombatSuccessFeedbackMetrics,
    rerun: &SearchReview,
) -> CombatSuccessFeedbackComparison {
    let first_win_nodes_delta = match (baseline.nodes_to_first_win, rerun.nodes_to_first_win) {
        (Some(base), Some(next)) => Some(next as i64 - base as i64),
        _ => None,
    };
    CombatSuccessFeedbackComparison {
        rerun_found_win: rerun.complete_win,
        first_win_nodes_delta,
        terminal_wins_delta: rerun.terminal_wins as i64 - baseline.terminal_wins as i64,
        final_hp_delta: baseline
            .final_hp
            .zip(rerun.final_hp)
            .map(|(base, next)| next - base),
        hp_loss_delta: baseline
            .hp_loss
            .zip(rerun.hp_loss)
            .map(|(base, next)| next - base),
        potions_used_delta: baseline
            .potions_used
            .zip(rerun.potions_used)
            .map(|(base, next)| next as i32 - base as i32),
        easier_first_win: first_win_nodes_delta.map(|delta| delta < 0),
    }
}
