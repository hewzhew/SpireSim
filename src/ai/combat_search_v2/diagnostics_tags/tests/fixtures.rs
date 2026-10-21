use super::*;

pub(super) fn branching() -> CombatSearchV2DiagnosticsBranching {
    CombatSearchV2DiagnosticsBranching {
        states_queried: 1,
        states_with_legal_actions: 1,
        legal_actions_total: 2,
        legal_actions_avg: 2.0,
        legal_actions_max: 2,
        nodes_generated_per_expanded: 0.0,
    }
}

pub(super) fn expansion() -> CombatSearchV2DiagnosticsExpansion {
    CombatSearchV2DiagnosticsExpansion {
        grouping_policy: "test",
        behavioral_effect: "diagnostic_only",
        states_observed: 0,
        total_atomic_actions: 0,
        total_fanout_groups: 0,
        fanout_groups_avg: 0.0,
        fanout_groups_max: 0,
        max_group_size: 0,
        action_kind_counts: Vec::new(),
        largest_groups: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn target_fanout() -> CombatSearchV2DiagnosticsTargetFanout {
    CombatSearchV2DiagnosticsTargetFanout {
        grouping_policy: "test",
        behavioral_effect: "diagnostic_only",
        states_observed: 0,
        targeted_actions_total: 0,
        target_fanout_groups_total: 0,
        multi_target_fanout_groups: 0,
        avg_targets_per_group: 0.0,
        max_targets_per_group: 0,
        lethal_target_groups: 0,
        unique_lethal_target_groups: 0,
        uniform_damage_groups: 0,
        max_target_hp_span: 0,
        group_kind_counts: Vec::new(),
        largest_target_fanouts: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn equivalence() -> CombatSearchV2DiagnosticsEquivalence {
    CombatSearchV2DiagnosticsEquivalence {
        equivalence_policy: "test",
        behavioral_effect: "diagnostic_only",
        states_observed: 0,
        states_compressed: 0,
        atomic_actions_in: 0,
        representative_actions_out: 0,
        actions_removed: 0,
        removed_action_ratio: 0.0,
        max_group_size: 0,
        group_kind_counts: Vec::new(),
        largest_groups: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn ordering() -> CombatSearchV2DiagnosticsOrdering {
    CombatSearchV2DiagnosticsOrdering {
        ordering_policy: "test",
        behavioral_effect: "diagnostic_only",
        states_observed: 0,
        states_reordered: 0,
        reordered_state_ratio: 0.0,
        total_actions_observed: 0,
        action_effect_actions: 0,
        attack_retaliation_actions: 0,
        attack_retaliation_trigger_count_hint: 0,
        attack_retaliation_raw_player_damage_hint: 0,
        attack_retaliation_player_block_loss_hint: 0,
        attack_retaliation_player_hp_loss_hint: 0,
        max_attack_retaliation_player_hp_loss_hint: 0,
        phase_action_hint_actions: 0,
        root_action_prior_scored_states: 0,
        root_action_prior_scored_actions: 0,
        max_position_shift: 0,
        avg_position_shift: 0.0,
        action_role_counts: Vec::new(),
        largest_reorders: Vec::new(),
        action_effect_samples: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn turn_branching() -> CombatSearchV2DiagnosticsTurnBranching {
    CombatSearchV2DiagnosticsTurnBranching {
        organization_policy: "test",
        behavioral_effect: "diagnostic_only",
        states_observed: 0,
        total_legal_actions: 0,
        total_generated_children: 0,
        generated_children_per_state: 0.0,
        same_turn_children: 0,
        next_turn_children: 0,
        pending_choice_children: 0,
        terminal_children: 0,
        other_children: 0,
        end_turn_children: 0,
        same_turn_child_ratio: 0.0,
        next_turn_child_ratio: 0.0,
        transition_counts: Vec::new(),
        largest_turn_fanouts: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn pending_choice() -> CombatSearchV2DiagnosticsPendingChoice {
    CombatSearchV2DiagnosticsPendingChoice {
        profiling_policy: "test",
        behavioral_effect: "diagnostic_only",
        rollout_contract_policy: "test",
        rollout_contract_behavioral_effect: "diagnostic_only",
        states_observed: 0,
        pending_choice_states: 0,
        expanded_pending_choice_states: 0,
        high_fanout_states: 0,
        max_candidate_count: 0,
        legal_actions_from_pending_choice: 0,
        max_legal_actions_from_pending_choice: 0,
        resolved_children: 0,
        still_pending_children: 0,
        truncated_children: 0,
        kind_counts: Vec::new(),
        ordering_role_counts: Vec::new(),
        largest_pending_choices: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn turn_prefix() -> CombatSearchV2DiagnosticsTurnPrefix {
    CombatSearchV2DiagnosticsTurnPrefix {
        tracking_policy: "test",
        behavioral_effect: "diagnostic_only",
        states_observed: 0,
        non_empty_prefix_states: 0,
        empty_prefix_states: 0,
        avg_prefix_length: 0.0,
        max_prefix_length: 0,
        max_legal_actions_after_non_empty_prefix: 0,
        total_cards_played_in_prefix: 0,
        total_potions_used_in_prefix: 0,
        total_potions_discarded_in_prefix: 0,
        total_other_actions_in_prefix: 0,
        prefix_length_counts: Vec::new(),
        prefix_kind_counts: Vec::new(),
        largest_prefix_fanouts: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn turn_sequence() -> CombatSearchV2DiagnosticsTurnSequence {
    CombatSearchV2DiagnosticsTurnSequence {
        grouping_policy: "test",
        behavioral_effect: "diagnostic_only",
        states_observed: 0,
        non_empty_prefix_states: 0,
        grouped_prefix_states: 0,
        unordered_sequence_groups: 0,
        groups_with_order_variants: 0,
        same_effect_order_variant_groups: 0,
        order_sensitive_groups: 0,
        max_ordered_variants_per_group: 0,
        max_effect_variants_per_group: 0,
        max_prefix_length: 0,
        max_legal_actions_after_prefix: 0,
        order_sensitive_divergence_histogram: Vec::new(),
        discard_order_shadow_audit: CombatSearchV2DiagnosticsDiscardOrderShadowAudit {
            audit_policy: "test",
            behavioral_effect: "diagnostic_only",
            candidate_groups: 0,
            candidate_states: 0,
            static_immediate_safe_groups: 0,
            static_immediate_safe_states: 0,
            exact_rollout_verified_groups: 0,
            proof_pruning_enabled: false,
            reveal_gate: StateAbstractionRevealGate::NextShuffle,
            one_step_exact_policy: "test",
            one_step_exact_stored_group_limit: 0,
            one_step_exact_sample_limit_groups: 0,
            one_step_exact_sample_limit_actions_per_group: 0,
            one_step_exact_checked_groups: 0,
            one_step_exact_sample_verified_groups: 0,
            one_step_exact_blocked_groups: 0,
            one_step_exact_checked_actions: 0,
            one_step_exact_verified_actions: 0,
            one_step_exact_blocked_actions: 0,
            sample_limit: 0,
            samples: Vec::new(),
            notes: Vec::new(),
        },
        largest_groups: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn turn_plan() -> CombatSearchV2DiagnosticsTurnPlan {
    CombatSearchV2DiagnosticsTurnPlan {
        planning_policy: "test",
        behavioral_effect: "diagnostic_only",
        root_states_observed: 0,
        total_plans: 0,
        max_plans_in_state: 0,
        total_inner_nodes_expanded: 0,
        total_inner_nodes_generated: 0,
        total_exact_state_skips: 0,
        total_truncated_children: 0,
        turn_plan_prior_scored_plans: 0,
        frontier_seeded_nodes: 0,
        frontier_seed_plans: Vec::new(),
        bucket_counts: Vec::new(),
        stop_reason_counts: Vec::new(),
        samples: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn card_identity() -> CombatSearchV2DiagnosticsCardIdentity {
    CombatSearchV2DiagnosticsCardIdentity {
        audit_policy: "test",
        behavioral_effect: "diagnostic_only",
        states_observed: 0,
        active_cards_observed: 0,
        action_payload_cards_observed: 0,
        action_payload_constructed_cards: 0,
        states_with_duplicate_active_lineage: 0,
        duplicate_active_lineage_observations: 0,
        states_with_lineage_card_id_conflict: 0,
        lineage_card_id_conflict_observations: 0,
        max_duplicate_group_size: 0,
        largest_duplicate_groups: Vec::new(),
        largest_conflict_groups: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn turn_local_dominance() -> CombatSearchV2DiagnosticsTurnLocalDominance {
    CombatSearchV2DiagnosticsTurnLocalDominance {
        pruning_policy: "test",
        behavioral_effect: "diagnostic_only",
        parent_states_observed: 0,
        enabled_parent_states: 0,
        eligible_child_states: 0,
        accepted_child_states: 0,
        pruned_child_states: 0,
        prune_ratio: 0.0,
        max_parent_dominance_buckets: 0,
        max_parent_resource_vectors: 0,
        max_bucket_width: 0,
        largest_parent_samples: Vec::new(),
        notes: Vec::new(),
    }
}

pub(super) fn pruning() -> CombatSearchV2DiagnosticsPruning {
    CombatSearchV2DiagnosticsPruning {
        transposition_prunes: 2,
        dominance_prunes: 0,
        turn_local_dominance_prunes: 0,
        terminal_wins: 1,
        terminal_losses: 0,
        unresolved_leaf_count: 0,
        max_actions_cut_count: 0,
        engine_step_limit_count: 0,
        potion_budget_cut_count: 0,
    }
}
