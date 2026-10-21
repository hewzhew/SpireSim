use crate::runtime::combat::CombatState;
use crate::sim::combat_action::CombatActionChoice;
use crate::state::core::EngineState;

use super::super::turn_plan_probe_report::{
    CombatSearchV2TurnPlanProbeActionMaskReport, CombatSearchV2TurnPlanProbeActionReport,
    CombatSearchV2TurnPlanProbeFirstActionSummaryReport,
};
use super::super::turn_planner::TurnPlanFirstActionSummaryV1;
use super::super::{
    compress_equivalent_actions, filtered_legal_actions, CombatSearchV2ActionTrace,
    CombatSearchV2PotionPolicy, IndexedActionChoice,
};
use super::selection_audit::bucket_count_report;

pub(super) fn root_action_mask_report(
    engine: &EngineState,
    combat: &CombatState,
    potion_policy: CombatSearchV2PotionPolicy,
    allowed_potion_slots: Option<u64>,
    allow_potion_discard: Option<bool>,
    legal_actions: Vec<CombatActionChoice>,
    preselection_first_actions: &[CombatSearchV2ActionTrace],
    preselection_first_action_summaries: &[TurnPlanFirstActionSummaryV1],
) -> CombatSearchV2TurnPlanProbeActionMaskReport {
    let candidate_eligible = filtered_legal_actions(
        legal_actions.clone(),
        potion_policy,
        allowed_potion_slots,
        allow_potion_discard,
        combat,
    );
    let equivalence = compress_equivalent_actions(engine, combat, candidate_eligible.clone());
    CombatSearchV2TurnPlanProbeActionMaskReport {
        data_role: "ObservedExact",
        availability: "RootOnly",
        complete_legal_mask: matches!(engine, EngineState::CombatPlayerTurn),
        legal_action_count: legal_actions.len(),
        candidate_eligible_action_count: candidate_eligible.len(),
        equivalence_representative_action_count: equivalence.choices.len(),
        preselection_first_action_count: preselection_first_actions.len(),
        potion_policy: potion_policy.label(),
        legal_actions: action_mask_entries(legal_actions),
        candidate_eligible_actions: action_mask_entries(candidate_eligible),
        equivalence_representative_actions: indexed_action_mask_entries(equivalence.choices),
        preselection_first_actions: action_trace_mask_entries(preselection_first_actions),
        preselection_first_action_summaries: first_action_summary_entries(
            preselection_first_action_summaries,
        ),
        notes: vec![
            "legal_actions is complete only for the CombatPlayerTurn boundary owned by this turn-plan probe",
            "candidate_eligible_actions applies the current combat search potion policy before turn-plan enumeration",
            "equivalence_representative_actions applies root action equivalence compression before turn-plan enumeration",
            "preselection_first_actions are first actions present before bucket selection truncates turn-plan candidates",
        ],
    }
}

fn first_action_summary_entries(
    summaries: &[TurnPlanFirstActionSummaryV1],
) -> Vec<CombatSearchV2TurnPlanProbeFirstActionSummaryReport> {
    summaries
        .iter()
        .map(
            |summary| CombatSearchV2TurnPlanProbeFirstActionSummaryReport {
                action: action_trace_mask_entry(&summary.action),
                plan_count: summary.plan_count,
                bucket_counts: bucket_count_report(&summary.bucket_counts),
            },
        )
        .collect()
}

fn action_mask_entries(
    actions: Vec<CombatActionChoice>,
) -> Vec<CombatSearchV2TurnPlanProbeActionReport> {
    actions
        .into_iter()
        .enumerate()
        .map(
            |(action_id, action)| CombatSearchV2TurnPlanProbeActionReport {
                action_id,
                action_key: action.action_key,
                action_debug: action.action_debug,
                input: action.input,
            },
        )
        .collect()
}

fn indexed_action_mask_entries(
    actions: Vec<IndexedActionChoice>,
) -> Vec<CombatSearchV2TurnPlanProbeActionReport> {
    actions
        .into_iter()
        .map(|action| CombatSearchV2TurnPlanProbeActionReport {
            action_id: action.original_action_id,
            action_key: action.choice.action_key,
            action_debug: action.choice.action_debug,
            input: action.choice.input,
        })
        .collect()
}

fn action_trace_mask_entries(
    actions: &[CombatSearchV2ActionTrace],
) -> Vec<CombatSearchV2TurnPlanProbeActionReport> {
    actions.iter().map(action_trace_mask_entry).collect()
}

fn action_trace_mask_entry(
    action: &CombatSearchV2ActionTrace,
) -> CombatSearchV2TurnPlanProbeActionReport {
    CombatSearchV2TurnPlanProbeActionReport {
        action_id: action.action_id,
        action_key: action.action_key.clone(),
        action_debug: action.action_debug.clone(),
        input: action.input.clone(),
    }
}
