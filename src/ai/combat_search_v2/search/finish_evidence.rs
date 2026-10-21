use super::super::*;

pub(super) fn evidence_reliability_report(
    invalid_card_identity_observed: bool,
    exhaustive: bool,
    action_surface_incomplete: bool,
) -> CombatSearchV2EvidenceReport {
    CombatSearchV2EvidenceReport {
        hidden_info_policy: "uses_only_the_supplied_engine_state; if that state contains hidden draw/rng truth, the report is engine-evidence rather than public-agent evidence",
        random_policy: "rng state is part of the transposition key; belief particles are not implemented in this first runner",
        estimate_policy: "unresolved frontier summaries are estimates/partial evidence and are never reported as terminal outcomes",
        reliability: if invalid_card_identity_observed {
            "invalid_input_or_rollout_state_duplicate_card_lineage_conflict_observed"
        } else if exhaustive {
            "exact_under_supplied_state_and_engine_semantics"
        } else {
            "partial_budgeted_evidence"
        },
        warnings: evidence_warnings(invalid_card_identity_observed, action_surface_incomplete),
    }
}

fn evidence_warnings(
    invalid_card_identity_observed: bool,
    action_surface_incomplete: bool,
) -> Vec<&'static str> {
    let mut warnings = vec![
        "unresolved_cannot_be_claimed_better_than_a_complete_baseline",
        "no_stepwise_human_action_agreement_objective",
        "no_llm_control_path",
        "combat_only_runner_does_not_validate_out_of_combat_strategy_quality",
        "default_potion_policy_disables_potions_until_a_real_potion_option_planner_exists",
    ];
    if invalid_card_identity_observed {
        warnings.push(
            "duplicate_active_card_lineage_with_conflicting_card_ids_observed_input_or_rollout_state_invalid_until_investigated",
        );
    }
    if action_surface_incomplete {
        warnings.push(
            "ordered_variants_of_multi_card_pending_choices_are_not_yet_covered_by_the_canonical_member_set_prefix_surface",
        );
    }
    warnings
}
