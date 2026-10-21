use std::collections::BTreeMap;

use super::super::turn_plan_probe_report::{
    CombatSearchV2TurnPlanProbeCandidateSelectionAuditReport,
    CombatSearchV2TurnPlanProbeCoverageGroupAuditReport,
    CombatSearchV2TurnPlanProbeCoverageKeyReport,
    CombatSearchV2TurnPlanProbeCoverageSignatureReport,
    CombatSearchV2TurnPlanProbeSelectionAuditReport,
};
use super::super::turn_planner::{
    TurnPlanBucket, TurnPlanCandidateSelectionAuditV1, TurnPlanCoverageGroupAuditV1,
    TurnPlanCoverageKeyV1, TurnPlanCoverageSignatureV1, TurnPlanSelectionAuditV1, TurnPlanV1,
};

pub(super) fn selection_audit_report(
    audit: &TurnPlanSelectionAuditV1,
) -> CombatSearchV2TurnPlanProbeSelectionAuditReport {
    CombatSearchV2TurnPlanProbeSelectionAuditReport {
        data_role: "turn_plan_candidate_selection_audit",
        behavioral_effect: "diagnostic_only_no_candidate_reordering_no_budget_change",
        candidates: audit
            .candidates
            .iter()
            .map(candidate_selection_audit_report)
            .collect(),
        coverage_groups: audit
            .coverage_groups
            .iter()
            .map(coverage_group_audit_report)
            .collect(),
    }
}

pub(super) fn bucket_count_report(
    counts: &BTreeMap<TurnPlanBucket, usize>,
) -> BTreeMap<&'static str, usize> {
    counts
        .iter()
        .map(|(bucket, count)| (bucket.label(), *count))
        .collect()
}

pub(super) fn selected_bucket_count_report(plans: &[TurnPlanV1]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::<TurnPlanBucket, usize>::new();
    for plan in plans {
        *counts.entry(plan.bucket).or_default() += 1;
    }
    bucket_count_report(&counts)
}

fn candidate_selection_audit_report(
    candidate: &TurnPlanCandidateSelectionAuditV1,
) -> CombatSearchV2TurnPlanProbeCandidateSelectionAuditReport {
    CombatSearchV2TurnPlanProbeCandidateSelectionAuditReport {
        preselection_rank: candidate.preselection_rank,
        selected_plan_index: candidate.selected_plan_index,
        outcome: candidate.outcome.label(),
        drop_reason: candidate.drop_reason.map(|reason| reason.label()),
        bucket: candidate.bucket.label(),
        action_keys: candidate.action_keys.clone(),
        coverage_key: coverage_key_report(candidate.coverage_key),
        coverage_signature: coverage_signature_report(candidate.coverage_signature),
    }
}

fn coverage_group_audit_report(
    group: &TurnPlanCoverageGroupAuditV1,
) -> CombatSearchV2TurnPlanProbeCoverageGroupAuditReport {
    CombatSearchV2TurnPlanProbeCoverageGroupAuditReport {
        bucket: group.key.bucket.label(),
        coverage_key: coverage_key_report(group.key.coverage),
        preselection_count: group.preselection_count,
        selected_count: group.selected_count,
        bucket_cap_dropped_count: group.bucket_cap_dropped_count,
        max_end_states_dropped_count: group.max_end_states_dropped_count,
    }
}

fn coverage_key_report(key: TurnPlanCoverageKeyV1) -> CombatSearchV2TurnPlanProbeCoverageKeyReport {
    CombatSearchV2TurnPlanProbeCoverageKeyReport {
        damage: key.damage.label(),
        block: key.block.label(),
        debuff: key.debuff.label(),
        setup: key.setup.label(),
        resource: key.resource.label(),
        risk: key.risk.label(),
    }
}

fn coverage_signature_report(
    signature: TurnPlanCoverageSignatureV1,
) -> CombatSearchV2TurnPlanProbeCoverageSignatureReport {
    CombatSearchV2TurnPlanProbeCoverageSignatureReport {
        action_count: signature.action_count,
        cards_played: signature.cards_played,
        attacks_played: signature.attacks_played,
        skills_played: signature.skills_played,
        powers_played: signature.powers_played,
        potions_used: signature.potions_used,
        damage_done: signature.damage_done,
        block_gained_proxy: signature.block_gained_proxy,
        enemy_vulnerable_added: signature.enemy_vulnerable_added,
        enemy_weak_added: signature.enemy_weak_added,
        enemy_strength_down_added: signature.enemy_strength_down_added,
        player_strength_gain: signature.player_strength_gain,
        player_temporary_strength_gain: signature.player_temporary_strength_gain,
        energy_spent_proxy: signature.energy_spent_proxy,
        hand_delta: signature.hand_delta,
        draw_delta: signature.draw_delta,
        discard_delta: signature.discard_delta,
        exhaust_delta: signature.exhaust_delta,
        queued_cards_delta: signature.queued_cards_delta,
        player_hp_lost: signature.player_hp_lost,
        reactive_player_hp_loss: signature.reactive_player_hp_loss,
        reactive_forced_turn_end_actions: signature.reactive_forced_turn_end_actions,
        pending_choice_steps: signature.pending_choice_steps,
    }
}
