use super::super::action_priority::ActionOrderingPriority;
use super::types::ActionOrderingEntry;

pub(in crate::ai::combat_search_v2::action_ordering) fn compare_action_ordering_entries(
    left: &ActionOrderingEntry,
    right: &ActionOrderingEntry,
) -> std::cmp::Ordering {
    compare_action_ordering_priorities(
        &left.priority,
        left.root_action_prior_score,
        &right.priority,
        right.root_action_prior_score,
    )
}

pub(in crate::ai::combat_search_v2) fn compare_action_ordering_priorities(
    left: &ActionOrderingPriority,
    left_root_action_prior_score: Option<f64>,
    right: &ActionOrderingPriority,
    right_root_action_prior_score: Option<f64>,
) -> std::cmp::Ordering {
    sts_combat_knowledge::compare_combat_tactical_action_ranks_v1(
        &left.tactical_rank_v1(),
        left_root_action_prior_score,
        &right.tactical_rank_v1(),
        right_root_action_prior_score,
    )
}
