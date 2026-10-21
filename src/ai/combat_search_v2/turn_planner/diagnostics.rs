use std::collections::BTreeMap;

use crate::ai::combat_search_v2::types::{
    CombatSearchV2DiagnosticsTurnPlan, CombatSearchV2DiagnosticsTurnPlanCount,
    CombatSearchV2DiagnosticsTurnPlanEntry, CombatSearchV2DiagnosticsTurnPlanSample,
};
use crate::sim::combat::CombatStepper;

use super::super::frontier::SearchNode;
use super::enumerate::enumerate_turn_plans;
use super::types::{
    TurnPlanBucket, TurnPlanEnumeration, TurnPlanStopReason, TurnPlanV1, TurnPlannerConfigV1,
};

const TURN_PLAN_DIAGNOSTIC_MAX_INNER_NODES: usize = 64;
const TURN_PLAN_DIAGNOSTIC_MAX_END_STATES: usize = 8;
const TURN_PLAN_DIAGNOSTIC_PER_BUCKET_LIMIT: usize = 2;
const TURN_PLAN_DIAGNOSTIC_ACTION_KEY_PREVIEW_LIMIT: usize = 6;

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct TurnPlanDiagnosticsCollector {
    root_states_observed: u64,
    total_plans: u64,
    max_plans_in_state: usize,
    total_inner_nodes_expanded: u64,
    total_inner_nodes_generated: u64,
    total_exact_state_skips: u64,
    total_truncated_children: u64,
    turn_plan_prior_scored_plans: u64,
    frontier_seeded_nodes: u64,
    frontier_seed_plans: Vec<TurnPlanDiagnosticEntry>,
    bucket_counts: BTreeMap<TurnPlanBucket, u64>,
    stop_reason_counts: BTreeMap<TurnPlanStopReason, u64>,
    samples: Vec<TurnPlanDiagnosticSample>,
}

#[derive(Clone, Debug)]
struct TurnPlanDiagnosticSample {
    observed_at_root_state: u64,
    plans: usize,
    inner_nodes_expanded: usize,
    inner_nodes_generated: usize,
    exact_state_skips: usize,
    truncated_children: usize,
    top_plans: Vec<TurnPlanDiagnosticEntry>,
}

#[derive(Clone, Debug)]
struct TurnPlanDiagnosticEntry {
    rank: usize,
    bucket: TurnPlanBucket,
    stop_reason: TurnPlanStopReason,
    outcome_class: &'static str,
    survival_bucket: &'static str,
    progress_bucket: &'static str,
    action_count: usize,
    final_hp: i32,
    risk_margin: i32,
    enemy_progress: i32,
    first_action_key: Option<String>,
    action_keys_preview: Vec<String>,
}

impl TurnPlanDiagnosticsCollector {
    pub(in crate::ai::combat_search_v2) fn observe_root(
        &mut self,
        root: &SearchNode,
        stepper: &impl CombatStepper,
    ) {
        if !matches!(
            root.engine,
            crate::state::core::EngineState::CombatPlayerTurn
        ) {
            return;
        }

        let config = TurnPlannerConfigV1 {
            max_inner_nodes: TURN_PLAN_DIAGNOSTIC_MAX_INNER_NODES,
            max_end_states: TURN_PLAN_DIAGNOSTIC_MAX_END_STATES,
            per_bucket_limit: TURN_PLAN_DIAGNOSTIC_PER_BUCKET_LIMIT,
            capture_step_trace: false,
            ..TurnPlannerConfigV1::default()
        };
        let enumeration = enumerate_turn_plans(root, stepper, &config, None);
        self.observe_enumeration(enumeration);
    }

    pub(in crate::ai::combat_search_v2) fn finish(&self) -> CombatSearchV2DiagnosticsTurnPlan {
        CombatSearchV2DiagnosticsTurnPlan {
            planning_policy: "turn_plan_v1_root_only_bounded_exact_step_enumeration",
            behavioral_effect: self.behavioral_effect_label(),
            root_states_observed: self.root_states_observed,
            total_plans: self.total_plans,
            max_plans_in_state: self.max_plans_in_state,
            total_inner_nodes_expanded: self.total_inner_nodes_expanded,
            total_inner_nodes_generated: self.total_inner_nodes_generated,
            total_exact_state_skips: self.total_exact_state_skips,
            total_truncated_children: self.total_truncated_children,
            turn_plan_prior_scored_plans: self.turn_plan_prior_scored_plans,
            frontier_seeded_nodes: self.frontier_seeded_nodes,
            frontier_seed_plans: self
                .frontier_seed_plans
                .iter()
                .map(diagnostic_entry_report)
                .collect(),
            bucket_counts: count_reports(&self.bucket_counts, TurnPlanBucket::label),
            stop_reason_counts: count_reports(&self.stop_reason_counts, TurnPlanStopReason::label),
            samples: self.sample_reports(),
            notes: vec![
                "turn plans are full-turn candidate observations, not committed search actions",
                "enumeration uses exact simulator stepping and exact state dedup inside the bounded diagnostic",
                "bucket selection preserves objective diversity before filling by estimate",
                "this diagnostic is currently root-only to avoid changing search behavior or wall-clock budget",
            ],
        }
    }

    fn behavioral_effect_label(&self) -> &'static str {
        if self.frontier_seeded_nodes > 0 {
            "turn_plan_frontier_seed_exact_end_states_no_prune_no_terminal_claim"
        } else {
            "diagnostic_only_no_frontier_steering_no_prune_no_terminal_claim"
        }
    }

    fn observe_enumeration(&mut self, enumeration: TurnPlanEnumeration) {
        self.root_states_observed = self.root_states_observed.saturating_add(1);
        self.total_plans = self
            .total_plans
            .saturating_add(enumeration.plans.len() as u64);
        self.max_plans_in_state = self.max_plans_in_state.max(enumeration.plans.len());
        self.total_inner_nodes_expanded = self
            .total_inner_nodes_expanded
            .saturating_add(enumeration.nodes_expanded as u64);
        self.total_inner_nodes_generated = self
            .total_inner_nodes_generated
            .saturating_add(enumeration.nodes_generated as u64);
        self.total_exact_state_skips = self
            .total_exact_state_skips
            .saturating_add(enumeration.exact_state_skips as u64);
        self.total_truncated_children = self
            .total_truncated_children
            .saturating_add(enumeration.truncated_children as u64);
        self.turn_plan_prior_scored_plans = self
            .turn_plan_prior_scored_plans
            .saturating_add(enumeration.turn_plan_prior_scored_plans as u64);

        for plan in &enumeration.plans {
            *self.bucket_counts.entry(plan.bucket).or_default() += 1;
            *self.stop_reason_counts.entry(plan.stop_reason).or_default() += 1;
        }

        self.samples.push(TurnPlanDiagnosticSample {
            observed_at_root_state: self.root_states_observed,
            plans: enumeration.plans.len(),
            inner_nodes_expanded: enumeration.nodes_expanded,
            inner_nodes_generated: enumeration.nodes_generated,
            exact_state_skips: enumeration.exact_state_skips,
            truncated_children: enumeration.truncated_children,
            top_plans: plan_entries(&enumeration.plans),
        });
    }

    pub(in crate::ai::combat_search_v2) fn observe_frontier_seeded_plans(
        &mut self,
        plans: &[TurnPlanV1],
    ) {
        self.frontier_seeded_nodes = self
            .frontier_seeded_nodes
            .saturating_add(plans.len() as u64);
        self.frontier_seed_plans.extend(plan_entries(plans));
        self.frontier_seed_plans
            .truncate(TURN_PLAN_DIAGNOSTIC_MAX_END_STATES);
    }

    pub(in crate::ai::combat_search_v2) fn observe_prior_scored_plans(&mut self, plans: usize) {
        self.turn_plan_prior_scored_plans = self
            .turn_plan_prior_scored_plans
            .saturating_add(plans as u64);
    }

    fn sample_reports(&self) -> Vec<CombatSearchV2DiagnosticsTurnPlanSample> {
        self.samples
            .iter()
            .map(|sample| CombatSearchV2DiagnosticsTurnPlanSample {
                observed_at_root_state: sample.observed_at_root_state,
                plans: sample.plans,
                inner_nodes_expanded: sample.inner_nodes_expanded,
                inner_nodes_generated: sample.inner_nodes_generated,
                exact_state_skips: sample.exact_state_skips,
                truncated_children: sample.truncated_children,
                top_plans: sample
                    .top_plans
                    .iter()
                    .map(diagnostic_entry_report)
                    .collect(),
            })
            .collect()
    }
}

fn diagnostic_entry_report(
    entry: &TurnPlanDiagnosticEntry,
) -> CombatSearchV2DiagnosticsTurnPlanEntry {
    CombatSearchV2DiagnosticsTurnPlanEntry {
        rank: entry.rank,
        bucket: entry.bucket.label(),
        stop_reason: entry.stop_reason.label(),
        outcome_class: entry.outcome_class,
        survival_bucket: entry.survival_bucket,
        progress_bucket: entry.progress_bucket,
        action_count: entry.action_count,
        final_hp: entry.final_hp,
        risk_margin: entry.risk_margin,
        enemy_progress: entry.enemy_progress,
        first_action_key: entry.first_action_key.clone(),
        action_keys_preview: entry.action_keys_preview.clone(),
    }
}

fn plan_entries(plans: &[TurnPlanV1]) -> Vec<TurnPlanDiagnosticEntry> {
    plans
        .iter()
        .enumerate()
        .map(|(index, plan)| TurnPlanDiagnosticEntry {
            rank: index + 1,
            bucket: plan.bucket,
            stop_reason: plan.stop_reason,
            outcome_class: plan.eval.outcome_class().label(),
            survival_bucket: plan.eval.survival_bucket().label(),
            progress_bucket: plan.eval.progress_bucket().label(),
            action_count: plan.actions.len(),
            final_hp: plan.eval.final_hp(),
            risk_margin: plan.eval.risk_margin(),
            enemy_progress: plan.eval.enemy_progress(),
            first_action_key: plan.actions.first().map(|action| action.action_key.clone()),
            action_keys_preview: plan
                .actions
                .iter()
                .take(TURN_PLAN_DIAGNOSTIC_ACTION_KEY_PREVIEW_LIMIT)
                .map(|action| action.action_key.clone())
                .collect(),
        })
        .collect()
}

fn count_reports<T: Copy + Ord>(
    counts: &BTreeMap<T, u64>,
    label: fn(T) -> &'static str,
) -> Vec<CombatSearchV2DiagnosticsTurnPlanCount> {
    counts
        .iter()
        .map(|(key, plans)| CombatSearchV2DiagnosticsTurnPlanCount {
            label: label(*key).to_string(),
            plans: *plans,
        })
        .collect()
}
