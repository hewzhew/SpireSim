use std::collections::BTreeMap;

use super::super::super::turn_planner::{TurnPlanBucket, TurnPlanEnumeration};
use super::super::super::*;

#[derive(Clone, Debug, Default)]
pub(in crate::ai::combat_search_v2) struct TurnBeamExtensionAttribution {
    pub(in crate::ai::combat_search_v2) turn_plan_calls: u64,
    pub(in crate::ai::combat_search_v2) turn_plan_inner_nodes_expanded: u64,
    pub(in crate::ai::combat_search_v2) turn_plan_inner_nodes_generated: u64,
    pub(in crate::ai::combat_search_v2) turn_plans_kept: u64,
    pub(in crate::ai::combat_search_v2) turn_plans_kept_by_bucket: BTreeMap<&'static str, u64>,
    pub(in crate::ai::combat_search_v2) terminal_candidates_kept: u64,
    pub(in crate::ai::combat_search_v2) best_pv_len: usize,
    pub(in crate::ai::combat_search_v2) best_pv_terminal: Option<SearchTerminalLabel>,
}

impl TurnBeamExtensionAttribution {
    pub(super) fn observe_turn_plan_enumeration(&mut self, enumeration: &TurnPlanEnumeration) {
        self.turn_plan_calls = self.turn_plan_calls.saturating_add(1);
        self.turn_plan_inner_nodes_expanded = self
            .turn_plan_inner_nodes_expanded
            .saturating_add(enumeration.nodes_expanded as u64);
        self.turn_plan_inner_nodes_generated = self
            .turn_plan_inner_nodes_generated
            .saturating_add(enumeration.nodes_generated as u64);
        self.turn_plans_kept = self
            .turn_plans_kept
            .saturating_add(enumeration.plans.len() as u64);
        for plan in &enumeration.plans {
            *self
                .turn_plans_kept_by_bucket
                .entry(plan.bucket.label())
                .or_default() += 1;
            if plan.bucket == TurnPlanBucket::TerminalWin {
                self.terminal_candidates_kept = self.terminal_candidates_kept.saturating_add(1);
            }
        }
    }

    pub(super) fn observe_best_estimate(&mut self, estimate: &RolloutNodeEstimate) {
        if self.best_pv_terminal.is_none() || estimate.actions_simulated > self.best_pv_len {
            self.best_pv_len = estimate.actions_simulated;
            self.best_pv_terminal = Some(estimate.terminal);
        }
    }
}
