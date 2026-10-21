use sts_oracle_runtime::ai::combat_search_v2::CombatSearchV2Report;

use super::super::search_types::{SearchPerformanceReview, SearchRolloutPerformanceReview};

pub(super) fn performance_review(report: &CombatSearchV2Report) -> SearchPerformanceReview {
    SearchPerformanceReview {
        total_us: report.performance.total_elapsed_us,
        rollout_us: report.performance.rollout_estimate_elapsed_us,
        rollout_calls: report.performance.rollout_estimate_calls,
        root_rollout_calls: report.performance.root_rollout_estimate_calls,
        child_rollout_calls: report.performance.child_rollout_estimate_calls,
        deferred_child_rollout_calls: report.performance.deferred_child_rollout_estimate_calls,
        turn_plan_seed_rollout_calls: report.performance.turn_plan_seed_rollout_estimate_calls,
        rollout_evaluations: report.rollout.evaluations,
        rollout_budget_skips: report.rollout.budget_skips,
        rollout_max_evaluation_budget_skips: report.rollout.max_evaluation_budget_skips,
        rollout_deadline_budget_skips: report.rollout.deadline_budget_skips,
        deferred_child_rollout_admitted_signal: report
            .performance
            .deferred_child_rollout_admitted_signal,
        deferred_child_rollout_admitted_periodic: report
            .performance
            .deferred_child_rollout_admitted_periodic,
        deferred_child_rollout_skipped_low_signal: report
            .performance
            .deferred_child_rollout_skipped_low_signal,
        deferred_child_rollout_skipped_budget_share: report
            .performance
            .deferred_child_rollout_skipped_budget_share,
        turn_plan_seed_calls: report.performance.turn_plan_frontier_seed_calls,
        turn_plan_seed_inner_nodes_expanded: report
            .performance
            .turn_plan_frontier_seed_inner_nodes_expanded,
        turn_plan_seed_inner_nodes_generated: report
            .performance
            .turn_plan_frontier_seed_inner_nodes_generated,
        turn_plan_seed_exact_state_skips: report
            .performance
            .turn_plan_frontier_seed_exact_state_skips,
        turn_plan_seed_us: report.performance.turn_plan_frontier_seed_elapsed_us,
        turn_boundary_macro_calls: report.performance.turn_boundary_macro_calls,
        turn_boundary_macro_candidates: report.performance.turn_boundary_macro_candidates,
        turn_boundary_macro_inner_nodes_expanded: report
            .performance
            .turn_boundary_macro_inner_nodes_expanded,
        turn_boundary_macro_inner_nodes_generated: report
            .performance
            .turn_boundary_macro_inner_nodes_generated,
        turn_boundary_macro_exact_state_skips: report
            .performance
            .turn_boundary_macro_exact_state_skips,
        turn_boundary_macro_atomic_fallbacks: report
            .performance
            .turn_boundary_macro_atomic_fallbacks,
        turn_boundary_macro_us: report.performance.turn_boundary_macro_elapsed_us,
        engine_step_us: report.performance.engine_step_elapsed_us,
        frontier_pop_us: report.performance.frontier_pop_elapsed_us,
        expansion_us: report.performance.expansion_elapsed_us,
        child_bookkeeping_us: report.performance.child_bookkeeping_elapsed_us,
        rollout_profile: rollout_performance_review(report),
    }
}

fn rollout_performance_review(report: &CombatSearchV2Report) -> SearchRolloutPerformanceReview {
    SearchRolloutPerformanceReview {
        cache_queries: report.rollout.cache_queries,
        cache_hits: report.rollout.cache_hits,
        cache_misses: report.rollout.cache_misses,
        cache_lookup_us: report.rollout.performance.cache_lookup_us,
        policy_dispatch_us: report.rollout.performance.policy_dispatch_us,
        no_potion_iterations: report.rollout.performance.no_potion_iterations,
        no_potion_phase_profile_us: report.rollout.performance.no_potion_phase_profile_us,
        no_potion_legal_actions_us: report.rollout.performance.no_potion_legal_actions_us,
        no_potion_choose_action_us: report.rollout.performance.no_potion_choose_action_us,
        no_potion_choose_ordering_us: report.rollout.performance.no_potion_choose_ordering_us,
        no_potion_probe_us: report.rollout.performance.no_potion_probe_us,
        no_potion_probe_score_calls: report.rollout.performance.no_potion_probe_score_calls,
        no_potion_probe_actions_evaluated: report
            .rollout
            .performance
            .no_potion_probe_actions_evaluated,
        no_potion_probe_step_reuses: report.rollout.performance.no_potion_probe_step_reuses,
        no_potion_probe_engine_step_us: report.rollout.performance.no_potion_probe_engine_step_us,
        no_potion_probe_phase_profile_us: report
            .rollout
            .performance
            .no_potion_probe_phase_profile_us,
        no_potion_probe_action_facts_us: report.rollout.performance.no_potion_probe_action_facts_us,
        no_potion_engine_step_us: report.rollout.performance.no_potion_engine_step_us,
        no_potion_child_build_us: report.rollout.performance.no_potion_child_build_us,
    }
}
