use super::*;

pub(super) fn diagnosis_tags(
    coverage_status: SearchCoverageStatus,
    stats: &CombatSearchV2Stats,
    branching: &CombatSearchV2DiagnosticsBranching,
    expansion: &CombatSearchV2DiagnosticsExpansion,
    target_fanout: &CombatSearchV2DiagnosticsTargetFanout,
    equivalence: &CombatSearchV2DiagnosticsEquivalence,
    ordering: &CombatSearchV2DiagnosticsOrdering,
    turn_branching: &CombatSearchV2DiagnosticsTurnBranching,
    pending_choice: &CombatSearchV2DiagnosticsPendingChoice,
    turn_prefix: &CombatSearchV2DiagnosticsTurnPrefix,
    turn_sequence: &CombatSearchV2DiagnosticsTurnSequence,
    turn_plan: &CombatSearchV2DiagnosticsTurnPlan,
    card_identity: &CombatSearchV2DiagnosticsCardIdentity,
    turn_local_dominance: &CombatSearchV2DiagnosticsTurnLocalDominance,
    pruning: &CombatSearchV2DiagnosticsPruning,
    frontier_work_items: usize,
) -> Vec<&'static str> {
    let mut tags = Vec::new();

    match coverage_status {
        SearchCoverageStatus::Exhaustive => tags.push("frontier_exhausted"),
        SearchCoverageStatus::AcceptedCompleteCandidate => tags.push("accepted_complete_candidate"),
        SearchCoverageStatus::NodeBudgetLimited => {
            if frontier_work_items > 0 {
                tags.push("node_budget_limited_with_open_frontier");
            } else {
                tags.push("node_budget_limited");
            }
        }
        SearchCoverageStatus::ActionPrefixBudgetLimited => {
            if frontier_work_items > 0 {
                tags.push("action_prefix_budget_limited_with_open_frontier");
            } else {
                tags.push("action_prefix_budget_limited");
            }
        }
        SearchCoverageStatus::ActionSurfaceIncomplete => {
            tags.push("pending_choice_action_surface_incomplete")
        }
        SearchCoverageStatus::TimeBudgetLimited => {
            if frontier_work_items > 0 {
                tags.push("time_budget_limited_with_open_frontier");
            } else {
                tags.push("time_budget_limited");
            }
        }
        SearchCoverageStatus::FrontierOpen => tags.push("frontier_open"),
    }

    if stats.terminal_wins > 0 {
        tags.push("terminal_wins_found");
    } else {
        tags.push("no_terminal_wins_found");
    }
    if stats.terminal_losses > 0 {
        tags.push("terminal_losses_found");
    }
    if stats.transposition_prunes > 0 {
        tags.push("transposition_pruning_active");
    } else {
        tags.push("transposition_pruning_inactive");
    }
    if stats.dominance_prunes > 0 {
        tags.push("dominance_pruning_active");
    } else {
        tags.push("dominance_pruning_inactive");
    }
    if pruning.engine_step_limit_count > 0 {
        tags.push("engine_step_limit_truncated_children");
    }
    if pruning.potion_budget_cut_count > 0 {
        tags.push("potion_budget_cutoffs");
    }
    if pruning.turn_local_dominance_prunes > 0 {
        tags.push("turn_local_dominance_pruning_active");
    }
    if pruning.max_actions_cut_count > 0 {
        tags.push("max_actions_per_line_cutoffs");
    }
    if pruning.unresolved_leaf_count > 0 {
        tags.push("unresolved_leaf_states");
    }
    if branching.states_queried > 0 && branching.legal_actions_max == 0 {
        tags.push("no_legal_actions_observed");
    }
    if expansion.states_observed > 0 {
        tags.push("action_expansion_diagnostics_active");
    }
    if expansion.max_group_size > 1 {
        tags.push("action_fanout_groups_observed");
    }
    if target_fanout.states_observed > 0 {
        tags.push("target_fanout_diagnostics_active");
    }
    if target_fanout.multi_target_fanout_groups > 0 {
        tags.push("multi_target_fanout_observed");
    }
    if target_fanout.lethal_target_groups > 0 {
        tags.push("lethal_target_fanout_observed");
    }
    if equivalence.states_observed > 0 {
        tags.push("action_equivalence_diagnostics_active");
    }
    if equivalence.actions_removed > 0 {
        tags.push("equivalence_pruning_active");
        tags.push("duplicate_actions_compressed");
    }
    if ordering.states_observed > 0 {
        tags.push("action_ordering_diagnostics_active");
    }
    if ordering.states_reordered > 0 {
        tags.push("action_ordering_reordered_legal_actions");
    }
    if turn_branching.states_observed > 0 {
        tags.push("turn_branching_diagnostics_active");
    }
    if turn_branching.same_turn_children > 0 {
        tags.push("same_turn_children_observed");
    }
    if turn_branching.next_turn_children > 0 {
        tags.push("next_turn_children_observed");
    }
    if turn_branching.pending_choice_children > 0 {
        tags.push("pending_choice_children_observed");
    }
    if pending_choice.states_observed > 0 {
        tags.push("pending_choice_profile_diagnostics_active");
    }
    if pending_choice.pending_choice_states > 0 {
        tags.push("pending_choice_states_observed");
    }
    if pending_choice.high_fanout_states > 0 {
        tags.push("high_fanout_pending_choices_observed");
    }
    if pending_choice.expanded_pending_choice_states > 0 {
        tags.push("pending_choice_contract_observed");
    }
    if pending_choice.resolved_children > 0 {
        tags.push("pending_choice_children_resolved");
    }
    if pending_choice.still_pending_children > 0 {
        tags.push("pending_choice_children_remained_pending");
    }
    if pending_choice.truncated_children > 0 {
        tags.push("pending_choice_children_truncated");
    }
    if turn_prefix.states_observed > 0 {
        tags.push("turn_prefix_diagnostics_active");
    }
    if turn_prefix.non_empty_prefix_states > 0 {
        tags.push("non_empty_turn_prefix_observed");
    }
    if turn_prefix.max_prefix_length >= 3 {
        tags.push("long_turn_prefix_observed");
    }
    if turn_sequence.states_observed > 0 {
        tags.push("turn_sequence_diagnostics_active");
    }
    if turn_sequence.groups_with_order_variants > 0 {
        tags.push("turn_sequence_order_variants_observed");
    }
    if turn_sequence.same_effect_order_variant_groups > 0 {
        tags.push("turn_sequence_same_effect_candidates_observed");
    }
    if turn_sequence.order_sensitive_groups > 0 {
        tags.push("turn_sequence_order_sensitive_groups_observed");
    }
    if turn_sequence.discard_order_shadow_audit.candidate_groups > 0 {
        tags.push("discard_order_shadow_audit_candidates_observed");
    }
    if turn_plan.root_states_observed > 0 {
        tags.push("turn_plan_diagnostics_active");
    }
    if turn_plan.total_plans > 0 {
        tags.push("turn_plan_candidates_observed");
    }
    if turn_plan.frontier_seeded_nodes > 0 {
        tags.push("turn_plan_frontier_seeded");
    }
    if card_identity.states_observed > 0 {
        tags.push("card_identity_diagnostics_active");
    }
    if card_identity.states_with_duplicate_active_lineage > 0 {
        tags.push("duplicate_active_card_lineage_observed");
    }
    if card_identity.states_with_lineage_card_id_conflict > 0 {
        tags.push("card_lineage_id_conflict_observed");
    }
    if card_identity.action_payload_constructed_cards > 0 {
        tags.push("card_payload_placeholders_observed");
    }
    if turn_local_dominance.parent_states_observed > 0 {
        tags.push("turn_local_dominance_diagnostics_active");
    }
    if turn_local_dominance.pruned_child_states > 0 {
        tags.push("turn_local_dominance_pruned_children");
    }
    if frontier_work_items > 0 {
        tags.push("frontier_remaining");
    }

    tags
}

#[cfg(test)]
mod tests;
