use super::*;
use crate::ai::combat_search_v2::state_abstraction::StateAbstractionRevealGate;

mod fixtures;

use fixtures::*;

#[test]
fn tags_surface_budget_and_active_diagnostics() {
    let stats = CombatSearchV2Stats {
        terminal_wins: 1,
        transposition_prunes: 2,
        ..CombatSearchV2Stats::default()
    };
    let mut branching = branching();
    branching.states_with_legal_actions = 0;
    branching.legal_actions_total = 0;
    branching.legal_actions_avg = 0.0;
    branching.legal_actions_max = 0;
    let mut expansion = expansion();
    expansion.states_observed = 1;
    expansion.max_group_size = 2;
    let mut target_fanout = target_fanout();
    target_fanout.states_observed = 1;
    target_fanout.multi_target_fanout_groups = 1;
    target_fanout.lethal_target_groups = 1;
    let mut equivalence = equivalence();
    equivalence.states_observed = 1;
    equivalence.actions_removed = 1;
    let mut ordering = ordering();
    ordering.states_observed = 1;
    ordering.states_reordered = 1;
    let mut turn_branching = turn_branching();
    turn_branching.states_observed = 1;
    turn_branching.same_turn_children = 1;
    turn_branching.next_turn_children = 1;
    turn_branching.pending_choice_children = 1;
    let mut pending_choice = pending_choice();
    pending_choice.states_observed = 3;
    pending_choice.pending_choice_states = 2;
    pending_choice.high_fanout_states = 1;
    pending_choice.expanded_pending_choice_states = 1;
    pending_choice.resolved_children = 1;
    pending_choice.still_pending_children = 1;
    pending_choice.truncated_children = 1;
    let mut turn_prefix = turn_prefix();
    turn_prefix.states_observed = 1;
    turn_prefix.non_empty_prefix_states = 1;
    turn_prefix.max_prefix_length = 3;
    let mut turn_sequence = turn_sequence();
    turn_sequence.states_observed = 1;
    turn_sequence.groups_with_order_variants = 1;
    turn_sequence.same_effect_order_variant_groups = 1;
    turn_sequence.order_sensitive_groups = 1;
    turn_sequence.discard_order_shadow_audit.candidate_groups = 1;
    let mut turn_plan = turn_plan();
    turn_plan.root_states_observed = 1;
    turn_plan.total_plans = 2;
    let mut card_identity = card_identity();
    card_identity.states_observed = 1;
    card_identity.states_with_duplicate_active_lineage = 1;
    card_identity.states_with_lineage_card_id_conflict = 1;
    card_identity.action_payload_constructed_cards = 1;
    let mut turn_local_dominance = turn_local_dominance();
    turn_local_dominance.parent_states_observed = 1;
    turn_local_dominance.pruned_child_states = 1;
    let mut pruning = pruning();
    pruning.turn_local_dominance_prunes = 1;
    pruning.unresolved_leaf_count = 1;

    let tags = diagnosis_tags(
        SearchCoverageStatus::NodeBudgetLimited,
        &stats,
        &branching,
        &expansion,
        &target_fanout,
        &equivalence,
        &ordering,
        &turn_branching,
        &pending_choice,
        &turn_prefix,
        &turn_sequence,
        &turn_plan,
        &card_identity,
        &turn_local_dominance,
        &pruning,
        4,
    );

    assert!(tags.contains(&"node_budget_limited_with_open_frontier"));
    assert!(tags.contains(&"terminal_wins_found"));
    assert!(tags.contains(&"transposition_pruning_active"));
    assert!(tags.contains(&"dominance_pruning_inactive"));
    assert!(tags.contains(&"turn_local_dominance_pruning_active"));
    assert!(tags.contains(&"unresolved_leaf_states"));
    assert!(tags.contains(&"no_legal_actions_observed"));
    assert!(tags.contains(&"action_expansion_diagnostics_active"));
    assert!(tags.contains(&"action_fanout_groups_observed"));
    assert!(tags.contains(&"target_fanout_diagnostics_active"));
    assert!(tags.contains(&"multi_target_fanout_observed"));
    assert!(tags.contains(&"lethal_target_fanout_observed"));
    assert!(tags.contains(&"action_equivalence_diagnostics_active"));
    assert!(tags.contains(&"equivalence_pruning_active"));
    assert!(tags.contains(&"duplicate_actions_compressed"));
    assert!(tags.contains(&"action_ordering_diagnostics_active"));
    assert!(tags.contains(&"action_ordering_reordered_legal_actions"));
    assert!(tags.contains(&"turn_branching_diagnostics_active"));
    assert!(tags.contains(&"same_turn_children_observed"));
    assert!(tags.contains(&"next_turn_children_observed"));
    assert!(tags.contains(&"pending_choice_children_observed"));
    assert!(tags.contains(&"pending_choice_profile_diagnostics_active"));
    assert!(tags.contains(&"pending_choice_states_observed"));
    assert!(tags.contains(&"high_fanout_pending_choices_observed"));
    assert!(tags.contains(&"pending_choice_contract_observed"));
    assert!(tags.contains(&"pending_choice_children_resolved"));
    assert!(tags.contains(&"pending_choice_children_remained_pending"));
    assert!(tags.contains(&"pending_choice_children_truncated"));
    assert!(tags.contains(&"turn_prefix_diagnostics_active"));
    assert!(tags.contains(&"non_empty_turn_prefix_observed"));
    assert!(tags.contains(&"long_turn_prefix_observed"));
    assert!(tags.contains(&"turn_sequence_diagnostics_active"));
    assert!(tags.contains(&"turn_sequence_order_variants_observed"));
    assert!(tags.contains(&"turn_sequence_same_effect_candidates_observed"));
    assert!(tags.contains(&"turn_sequence_order_sensitive_groups_observed"));
    assert!(tags.contains(&"discard_order_shadow_audit_candidates_observed"));
    assert!(tags.contains(&"turn_plan_diagnostics_active"));
    assert!(tags.contains(&"turn_plan_candidates_observed"));
    assert!(tags.contains(&"card_identity_diagnostics_active"));
    assert!(tags.contains(&"duplicate_active_card_lineage_observed"));
    assert!(tags.contains(&"card_lineage_id_conflict_observed"));
    assert!(tags.contains(&"card_payload_placeholders_observed"));
    assert!(tags.contains(&"turn_local_dominance_diagnostics_active"));
    assert!(tags.contains(&"turn_local_dominance_pruned_children"));
    assert!(tags.contains(&"frontier_remaining"));
}
