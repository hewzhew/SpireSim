mod action_mask;
mod candidate_report;
mod prefix_session;
mod selection_audit;
mod types;

pub use prefix_session::{
    CombatSearchV2TurnPlanPrefixCandidateV1, CombatSearchV2TurnPlanPrefixLayerV1,
    CombatSearchV2TurnPlanPrefixSessionV1,
};
pub use types::{
    CombatSearchV2TurnPlanPartialCandidateAuditV1,
    CombatSearchV2TurnPlanPartialGuideConditionedAuditV1,
    CombatSearchV2TurnPlanPartialRetentionAuditV1, CombatSearchV2TurnPlanPartialRetentionLaneV1,
    CombatSearchV2TurnPlanProbeCandidate, CombatSearchV2TurnPlanProbeEnumeration,
    CombatSearchV2TurnPlanProbeGuideRankV1,
};

use crate::sim::combat::{CombatPosition, CombatStepper, EngineCombatStepper};

use std::time::Instant;

use super::frontier::SearchNode;
use super::turn_plan_probe_report::*;
use super::turn_planner::{
    enumerate_turn_plans, enumerate_turn_plans_across_pending_choices,
    enumerate_turn_plans_across_pending_choices_guided, TurnPlanEnumeration, TurnPlannerConfigV1,
};
use super::*;

const TURN_PLAN_PROBE_MAX_INNER_NODES: usize = 256;
const TURN_PLAN_PROBE_MAX_END_STATES: usize = 24;
const TURN_PLAN_PROBE_PER_BUCKET_LIMIT: usize = TURN_PLAN_PROBE_MAX_END_STATES;

pub fn enumerate_combat_search_v2_turn_plan_probe_candidates(
    engine: &EngineState,
    combat: &CombatState,
    config: &CombatSearchV2Config,
) -> CombatSearchV2TurnPlanProbeEnumeration {
    enumerate_probe_candidates(engine, combat, config, false)
}

/// Diagnostic-only variant that treats structured pending choices as part of
/// the same player turn. The production V2 turn planner retains its historical
/// boundary behavior; this variant exists only to measure what its bounded
/// partial beam can express once the missing selection transition is removed.
pub fn enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices(
    engine: &EngineState,
    combat: &CombatState,
    config: &CombatSearchV2Config,
) -> CombatSearchV2TurnPlanProbeEnumeration {
    enumerate_probe_candidates(engine, combat, config, true)
}

pub fn enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices_guided(
    engine: &EngineState,
    combat: &CombatState,
    config: &CombatSearchV2Config,
    partial_state_guides: &dyn Fn(&CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
) -> CombatSearchV2TurnPlanProbeEnumeration {
    enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices_guided_with_deadline(
        engine,
        combat,
        config,
        None,
        partial_state_guides,
    )
}

/// Deadline-aware counterpart used by bounded callers which must be able to
/// recover control from one unusually wide same-turn enumeration.
pub fn enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices_guided_with_deadline(
    engine: &EngineState,
    combat: &CombatState,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    partial_state_guides: &dyn Fn(&CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
) -> CombatSearchV2TurnPlanProbeEnumeration {
    enumerate_probe_candidates_guided(engine, combat, config, deadline, partial_state_guides)
}

fn enumerate_probe_candidates_guided(
    engine: &EngineState,
    combat: &CombatState,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    partial_state_guides: &dyn Fn(&CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
) -> CombatSearchV2TurnPlanProbeEnumeration {
    enumerate_probe_candidates_with_guides(
        engine,
        combat,
        config,
        true,
        deadline,
        Some(partial_state_guides),
    )
}

fn enumerate_probe_candidates(
    engine: &EngineState,
    combat: &CombatState,
    config: &CombatSearchV2Config,
    continue_pending_choices: bool,
) -> CombatSearchV2TurnPlanProbeEnumeration {
    enumerate_probe_candidates_with_guides(
        engine,
        combat,
        config,
        continue_pending_choices,
        None,
        None,
    )
}

fn enumerate_probe_candidates_with_guides(
    engine: &EngineState,
    combat: &CombatState,
    config: &CombatSearchV2Config,
    continue_pending_choices: bool,
    deadline: Option<Instant>,
    partial_state_guides: Option<
        &dyn Fn(&CombatPosition) -> Vec<CombatSearchV2TurnPlanProbeGuideRankV1>,
    >,
) -> CombatSearchV2TurnPlanProbeEnumeration {
    let root = SearchNode::root(engine.clone(), combat.clone());
    let plugins = CombatSearchPluginStack::from_config(config);
    let turn_config = turn_planner_config(config, &plugins);
    let enumeration = if let Some(guides) = partial_state_guides {
        enumerate_turn_plans_across_pending_choices_guided(
            &root,
            &EngineCombatStepper,
            &turn_config,
            deadline,
            guides,
        )
    } else if continue_pending_choices {
        enumerate_turn_plans_across_pending_choices(
            &root,
            &EngineCombatStepper,
            &turn_config,
            deadline,
        )
    } else {
        enumerate_turn_plans(&root, &EngineCombatStepper, &turn_config, deadline)
    };
    build_probe_enumeration(
        engine,
        combat,
        config,
        &root,
        &turn_config,
        enumeration,
        "turn_plan_v1_root_only_bounded_exact_step_enumeration",
    )
}

fn build_probe_enumeration(
    engine: &EngineState,
    combat: &CombatState,
    config: &CombatSearchV2Config,
    root: &SearchNode,
    turn_config: &TurnPlannerConfigV1,
    enumeration: TurnPlanEnumeration,
    planning_policy: &'static str,
) -> CombatSearchV2TurnPlanProbeEnumeration {
    let position = CombatPosition::new(engine.clone(), combat.clone());
    let legal_action_choices = if matches!(engine, EngineState::CombatPlayerTurn) {
        EngineCombatStepper.atomic_action_choices(&position)
    } else {
        Vec::new()
    };
    let root_action_mask = action_mask::root_action_mask_report(
        engine,
        combat,
        turn_config.potion_policy,
        turn_config.allowed_potion_slots,
        turn_config.allow_potion_discard,
        legal_action_choices,
        &enumeration.preselection_first_actions,
        &enumeration.preselection_first_action_summaries,
    );
    let candidates = enumeration
        .plans
        .iter()
        .enumerate()
        .map(candidate_report::candidate_report)
        .collect::<Vec<_>>();
    let root_report = CombatSearchV2TurnPlanProbeRootReport {
        schema_name: "CombatSearchV2TurnPlanProbeRootReport",
        schema_version: 3,
        question: "which_exact_same_turn_plan_should_receive_followup_search_budget",
        behavioral_scope: "diagnostic_only_no_prune_no_policy_change_no_artifact_promotion",
        input_label: config.input_label.clone(),
        config: CombatSearchV2TurnPlanProbeConfigReport {
            max_inner_nodes: turn_config.max_inner_nodes,
            max_end_states: turn_config.max_end_states,
            per_bucket_limit: turn_config.per_bucket_limit,
            potion_policy: turn_config.potion_policy.label(),
            allowed_potion_slots: turn_config.allowed_potion_slots,
            allow_potion_discard: turn_config.allow_potion_discard,
            max_engine_steps_per_action: turn_config.max_engine_steps_per_action,
        },
        initial_context: CombatSearchV2DecisionContext {
            state: summarize_state(engine, combat),
            phase_profile: combat_search_phase_profile_report(combat_search_phase_profile(
                engine, combat,
            )),
            frontier_value: combat_search_frontier_value_report(root),
        },
        root_action_mask,
        enumeration: CombatSearchV2TurnPlanProbeEnumerationReport {
            planning_policy,
            plans: enumeration.plans.len(),
            preselection_plans: enumeration.preselection_plan_count,
            preselection_first_action_count: enumeration.preselection_first_actions.len(),
            preselection_bucket_counts: selection_audit::bucket_count_report(
                &enumeration.preselection_bucket_counts,
            ),
            selected_bucket_counts: selection_audit::selected_bucket_count_report(
                &enumeration.plans,
            ),
            nodes_expanded: enumeration.nodes_expanded,
            nodes_generated: enumeration.nodes_generated,
            engine_steps: enumeration.engine_steps,
            exact_state_skips: enumeration.exact_state_skips,
            truncated_children: enumeration.truncated_children,
        },
        selection_audit: selection_audit::selection_audit_report(&enumeration.selection_audit),
        candidates: candidates
            .iter()
            .map(|candidate| candidate.report.clone())
            .collect(),
        notes: vec![
            "turn-plan probes are exact same-turn candidates ending at a stable boundary",
            "plan enumeration preserves bucket diversity; plan_index is not an optimality claim",
            "followup child search is owned by eval labs, not by this root probe helper",
        ],
    };
    CombatSearchV2TurnPlanProbeEnumeration {
        report: root_report,
        candidates,
    }
}

fn turn_planner_config(
    config: &CombatSearchV2Config,
    plugins: &CombatSearchPluginStack,
) -> TurnPlannerConfigV1 {
    TurnPlannerConfigV1 {
        max_inner_nodes: config
            .turn_plan_probe_max_inner_nodes
            .unwrap_or(TURN_PLAN_PROBE_MAX_INNER_NODES),
        max_partial_frontier_states: config.turn_plan_probe_max_partial_frontier_states,
        max_end_states: config
            .turn_plan_probe_max_end_states
            .unwrap_or(TURN_PLAN_PROBE_MAX_END_STATES),
        per_bucket_limit: config
            .turn_plan_probe_per_bucket_limit
            .unwrap_or(TURN_PLAN_PROBE_PER_BUCKET_LIMIT),
        potion_policy: plugins.potion.policy,
        max_potions_used: plugins.potion.max_potions_used,
        allowed_potion_slots: plugins.potion.allowed_potion_slots,
        allow_potion_discard: plugins.potion.allow_potion_discard,
        max_engine_steps_per_action: config.max_engine_steps_per_action,
        turn_plan_prior: config.turn_plan_prior.clone(),
        capture_step_trace: true,
    }
}
