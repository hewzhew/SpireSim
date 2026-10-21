#![allow(dead_code)]
// TurnPlan V1 is intentionally staged: this module is tested independently
// before it is allowed to steer the main combat search frontier.

mod diagnostics;
mod enumerate;
mod frontier_seed;
mod turn_boundary_portfolio;
mod types;

pub(in crate::ai::combat_search_v2) use diagnostics::TurnPlanDiagnosticsCollector;
pub(in crate::ai::combat_search_v2) use enumerate::{
    enumerate_turn_plans, enumerate_turn_plans_across_pending_choices,
    enumerate_turn_plans_across_pending_choices_guided, TurnPlanLayerSessionV1,
    TurnPlanPartialLayerV1,
};
pub(in crate::ai::combat_search_v2) use frontier_seed::turn_plan_frontier_seed;
pub(in crate::ai::combat_search_v2) use turn_boundary_portfolio::build_turn_boundary_portfolio;
pub(in crate::ai::combat_search_v2) use types::{
    TurnPlanBucket, TurnPlanCandidateSelectionAuditV1, TurnPlanCoverageGroupAuditV1,
    TurnPlanCoverageKeyV1, TurnPlanCoverageSignatureV1, TurnPlanEnumeration,
    TurnPlanFirstActionSummaryV1, TurnPlanSelectionAuditV1, TurnPlanStopReason, TurnPlanV1,
    TurnPlannerConfigV1,
};

#[cfg(test)]
mod tests;
