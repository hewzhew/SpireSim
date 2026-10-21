use sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2Report;

use super::search_types::{SearchReview, SearchReviewFacts};

#[path = "search_review/performance.rs"]
mod performance;
#[path = "search_review/progress.rs"]
mod progress;
#[path = "search_review/progress_complete.rs"]
mod progress_complete;
#[path = "search_review/progress_rollout.rs"]
mod progress_rollout;

use performance::performance_review;
use progress::diagnostic_progress_facts;

pub(super) fn search_review(
    label: &'static str,
    nodes: usize,
    wall_ms: u64,
    report: &CombatSearchV2Report,
    action_preview_limit: usize,
) -> SearchReview {
    let best = report.best_win_trajectory.as_ref();
    SearchReview {
        label,
        nodes,
        wall_ms,
        expansion_policy: report.search_policy.expansion_policy,
        rollout_policy: report.search_policy.rollout_policy,
        turn_plan_policy: report.search_policy.turn_plan_policy,
        phase_guard_policy: report.search_policy.phase_guard_policy,
        setup_bias_policy: report.search_policy.action_prior_policy,
        child_rollout_policy: report.search_policy.child_rollout_policy,
        potion_policy: report.search_policy.potion_policy,
        max_potions_used: report.budget.max_potions_used,
        complete_win: best.is_some(),
        hp_loss: best.map(|trajectory| trajectory.hp_loss),
        final_hp: best.map(|trajectory| trajectory.final_hp),
        turns: best.map(|trajectory| trajectory.turns),
        potions_used: best.map(|trajectory| trajectory.potions_used),
        nodes_expanded: report.stats.nodes_expanded,
        nodes_generated: report.stats.nodes_generated,
        nodes_to_first_win: report.stats.nodes_to_first_win,
        terminal_wins: report.stats.terminal_wins,
        elapsed_ms: report.stats.elapsed_ms,
        deadline_hit: report.stats.deadline_hit,
        node_budget_hit: report.stats.node_budget_hit,
        performance: performance_review(report),
        facts: SearchReviewFacts {
            diagnostic_progress: diagnostic_progress_facts(report, action_preview_limit),
            turn_plan: (report.diagnostics.turn_plan.root_states_observed > 0
                || report.diagnostics.turn_plan.frontier_seeded_nodes > 0)
                .then(|| report.diagnostics.turn_plan.clone()),
        },
        candidate_adjudication_census: None,
        persistent_burden_cutpoint_probe: None,
    }
}
