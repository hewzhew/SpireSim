use super::super::*;

pub(super) fn search_policy_report(
    config: &CombatSearchV2Config,
    owns_engine_pending_choice_prefixes: bool,
) -> CombatSearchV2PolicyReport {
    let plugins = CombatSearchPluginStack::from_config(config);
    CombatSearchV2PolicyReport {
        kind: match plugins.expansion {
            CombatSearchExpansionPluginId::AtomicActions => {
                "best_first_atomic_action_graph_search_v2"
            }
            CombatSearchExpansionPluginId::HierarchicalTurnBoundary => {
                "best_first_hierarchical_turn_boundary_graph_search_v1"
            }
        },
        terminal_policy: "whole_combat_terminal_only",
        expansion_order: "conservative_duplicate_action_equivalence_then_semantic_turn_action_ordering_with_one_step_retaliation_protection_frontier_continuation_then_frontier_value_v1",
        expansion_policy: plugins.expansion.label(),
        pending_choice_action_surface: if owns_engine_pending_choice_prefixes {
            "canonical_member_set_prefix_with_explicit_order_variant_gap_v2"
        } else {
            "stepper_eager_candidate_actions"
        },
        action_prior_policy: plugins.action_prior.label(),
        phase_guard_policy: plugins.phase_guard.label(),
        frontier_value: COMBAT_SEARCH_FRONTIER_VALUE_POLICY,
        turn_branching: "turn_transition_classification_with_late_frontier_tie_break",
        turn_plan_policy: CombatSearchV2TurnPlanPolicy::from(plugins.turn_plan).label(),
        potion_policy: plugins.potion.policy.label(),
        transposition_table: "exact_runtime_state_key_with_resource_coverage",
        dominance_pruning:
            "global_dominance_bucket_resource_vector_plus_same_parent_same_turn_sibling_coverage",
        rollout_value:
            "combat_eval_v3_first_play_urgency_between_simulated_loss_and_unresolved_estimate_used_for_frontier_priority_only_not_terminal_claims",
        rollout_policy: CombatSearchV2RolloutPolicy::from(plugins.rollout).label(),
        child_rollout_policy: CombatSearchV2ChildRolloutPolicy::from(plugins.child_rollout).label(),
        llm_authority: "none",
    }
}

pub(super) fn budget_report(config: &CombatSearchV2Config) -> CombatSearchV2BudgetReport {
    let plugins = CombatSearchPluginStack::from_config(config);
    CombatSearchV2BudgetReport {
        max_nodes: config.max_nodes,
        max_pending_choice_prefixes: config.max_nodes,
        max_actions_per_line: config.max_actions_per_line,
        max_engine_steps_per_action: config.max_engine_steps_per_action,
        wall_time_ms: config.wall_time.map(|duration| duration.as_millis()),
        satisfaction: config.satisfaction.label(),
        satisfaction_hp_loss_at_most: config.satisfaction.hp_loss_limit(),
        max_potions_used: plugins.potion.max_potions_used,
        rollout_max_evaluations: config.rollout_max_evaluations,
        rollout_max_actions: config.rollout_max_actions,
        rollout_beam_width: config.rollout_beam_width,
    }
}
