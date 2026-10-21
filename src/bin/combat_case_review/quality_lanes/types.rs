use serde::Serialize;
use sts_oracle_runtime::ai::combat_search_v2::SearchTerminalLabel;

use super::super::search_types::SearchReview;

#[derive(Serialize)]
pub(crate) struct CombatQualityLaneReview {
    pub(super) schema: &'static str,
    pub(super) contract: &'static str,
    pub(super) total_nodes: usize,
    pub(super) total_wall_ms: u64,
    pub(super) per_lane_nodes: usize,
    pub(super) per_lane_wall_ms: u64,
    pub(super) selected_lane: Option<&'static str>,
    pub(super) selected_reason: &'static str,
    pub(super) success_feedback_rerun: Option<CombatSuccessFeedbackRerun>,
    pub(super) lanes: Vec<CombatQualityLaneResult>,
}

#[derive(Serialize)]
pub(super) struct CombatQualityLaneResult {
    pub(super) lane: &'static str,
    pub(super) intent: &'static str,
    pub(super) review: SearchReview,
    pub(super) quality: Option<CombatLineQuality>,
}

#[derive(Serialize)]
pub(super) struct CombatSuccessFeedbackRerun {
    pub(super) schema: &'static str,
    pub(super) contract: &'static str,
    pub(super) source_kind: &'static str,
    pub(super) source_lane: &'static str,
    pub(super) witness_action_count: usize,
    pub(super) prior_states: usize,
    pub(super) duplicate_prior_hints: usize,
    pub(super) baseline: CombatSuccessFeedbackMetrics,
    pub(super) rerun: SearchReview,
    pub(super) comparison: CombatSuccessFeedbackComparison,
}

#[derive(Clone, Serialize)]
pub(super) struct CombatSuccessFeedbackMetrics {
    pub(super) complete_win: bool,
    pub(super) nodes_to_first_win: Option<u64>,
    pub(super) terminal_wins: u64,
    pub(super) final_hp: Option<i32>,
    pub(super) hp_loss: Option<i32>,
    pub(super) potions_used: Option<u32>,
    pub(super) nodes_expanded: u64,
    pub(super) nodes_generated: u64,
    pub(super) elapsed_ms: u128,
}

#[derive(Serialize)]
pub(super) struct CombatSuccessFeedbackComparison {
    pub(super) rerun_found_win: bool,
    pub(super) first_win_nodes_delta: Option<i64>,
    pub(super) terminal_wins_delta: i64,
    pub(super) final_hp_delta: Option<i32>,
    pub(super) hp_loss_delta: Option<i32>,
    pub(super) potions_used_delta: Option<i32>,
    pub(super) easier_first_win: Option<bool>,
}

#[derive(Clone, Serialize)]
pub(crate) struct CombatLineQuality {
    pub(super) terminal: SearchTerminalLabel,
    pub(super) hp_loss: i32,
    pub(super) final_hp: i32,
    pub(super) persistent_run_value: i32,
    pub(super) persistent_adjusted_hp: i32,
    pub(super) potions_used: u32,
    pub(super) turns: u32,
    pub(super) cards_played: u32,
    pub(super) action_count: usize,
}

impl CombatSuccessFeedbackMetrics {
    pub(super) fn from_review(review: &SearchReview) -> Self {
        Self {
            complete_win: review.complete_win,
            nodes_to_first_win: review.nodes_to_first_win,
            terminal_wins: review.terminal_wins,
            final_hp: review.final_hp,
            hp_loss: review.hp_loss,
            potions_used: review.potions_used,
            nodes_expanded: review.nodes_expanded,
            nodes_generated: review.nodes_generated,
            elapsed_ms: review.elapsed_ms,
        }
    }
}
