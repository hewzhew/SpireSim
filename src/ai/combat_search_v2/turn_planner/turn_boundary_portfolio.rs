use std::time::Instant;

use crate::sim::combat::CombatStepper;

use super::super::frontier::SearchNode;
use super::super::{CombatSearchPluginStack, CombatSearchV2Config};
use super::enumerate::enumerate_turn_plans;
use super::types::{TurnPlanBucket, TurnPlanStopReason, TurnPlanV1, TurnPlannerConfigV1};

const TURN_BOUNDARY_MAX_INNER_NODES: usize = 128;
const TURN_BOUNDARY_MAX_END_STATES: usize = 8;
const TURN_BOUNDARY_PER_PURPOSE_LIMIT: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) enum TurnBoundaryCandidatePurposeV1 {
    TerminalWin,
    Survival,
    Progress,
    Setup,
    Balanced,
    PendingChoice,
}

pub(in crate::ai::combat_search_v2) struct TurnBoundaryCandidateV1 {
    pub(in crate::ai::combat_search_v2) purpose: TurnBoundaryCandidatePurposeV1,
    pub(in crate::ai::combat_search_v2) plan: TurnPlanV1,
}

#[derive(Default)]
pub(in crate::ai::combat_search_v2) struct TurnBoundaryPortfolioV1 {
    pub(in crate::ai::combat_search_v2) candidates: Vec<TurnBoundaryCandidateV1>,
    pub(in crate::ai::combat_search_v2) inner_nodes_expanded: usize,
    pub(in crate::ai::combat_search_v2) inner_nodes_generated: usize,
    pub(in crate::ai::combat_search_v2) exact_state_skips: usize,
}

pub(in crate::ai::combat_search_v2) fn build_turn_boundary_portfolio(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    plugins: &CombatSearchPluginStack,
    remaining_global_nodes: usize,
    deadline: Option<Instant>,
) -> TurnBoundaryPortfolioV1 {
    if remaining_global_nodes == 0 {
        return TurnBoundaryPortfolioV1::default();
    }
    let turn_config = TurnPlannerConfigV1 {
        max_inner_nodes: remaining_global_nodes.min(TURN_BOUNDARY_MAX_INNER_NODES),
        max_partial_frontier_states: None,
        max_end_states: TURN_BOUNDARY_MAX_END_STATES,
        per_bucket_limit: TURN_BOUNDARY_PER_PURPOSE_LIMIT,
        potion_policy: plugins.potion.policy,
        max_potions_used: plugins.potion.max_potions_used,
        allowed_potion_slots: plugins.potion.allowed_potion_slots,
        allow_potion_discard: plugins.potion.allow_potion_discard,
        max_engine_steps_per_action: config.max_engine_steps_per_action,
        turn_plan_prior: config.turn_plan_prior.clone(),
        capture_step_trace: false,
    };
    let enumeration = enumerate_turn_plans(node, stepper, &turn_config, deadline);
    let candidates = enumeration
        .plans
        .into_iter()
        .filter_map(|plan| {
            let purpose = candidate_purpose(plan.bucket, plan.stop_reason, plan.actions.len())?;
            Some(TurnBoundaryCandidateV1 { purpose, plan })
        })
        .collect();

    TurnBoundaryPortfolioV1 {
        candidates,
        inner_nodes_expanded: enumeration.nodes_expanded,
        inner_nodes_generated: enumeration.nodes_generated,
        exact_state_skips: enumeration.exact_state_skips,
    }
}

fn candidate_purpose(
    bucket: TurnPlanBucket,
    stop_reason: TurnPlanStopReason,
    action_count: usize,
) -> Option<TurnBoundaryCandidatePurposeV1> {
    if action_count == 0 {
        return None;
    }
    match (bucket, stop_reason) {
        (TurnPlanBucket::TerminalWin, TurnPlanStopReason::Terminal) => {
            Some(TurnBoundaryCandidatePurposeV1::TerminalWin)
        }
        (TurnPlanBucket::TerminalLoss, _) => None,
        (_, TurnPlanStopReason::PendingChoice) => {
            Some(TurnBoundaryCandidatePurposeV1::PendingChoice)
        }
        (TurnPlanBucket::Survival, TurnPlanStopReason::NextTurn) => {
            Some(TurnBoundaryCandidatePurposeV1::Survival)
        }
        (TurnPlanBucket::Progress, TurnPlanStopReason::NextTurn) => {
            Some(TurnBoundaryCandidatePurposeV1::Progress)
        }
        (TurnPlanBucket::Setup, TurnPlanStopReason::NextTurn) => {
            Some(TurnBoundaryCandidatePurposeV1::Setup)
        }
        (TurnPlanBucket::Balanced, TurnPlanStopReason::NextTurn) => {
            Some(TurnBoundaryCandidatePurposeV1::Balanced)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_candidates_keep_supported_stable_boundaries() {
        assert_eq!(
            candidate_purpose(TurnPlanBucket::Progress, TurnPlanStopReason::NextTurn, 3),
            Some(TurnBoundaryCandidatePurposeV1::Progress)
        );
        assert_eq!(
            candidate_purpose(TurnPlanBucket::Balanced, TurnPlanStopReason::NextTurn, 1),
            Some(TurnBoundaryCandidatePurposeV1::Balanced)
        );
        assert_eq!(
            candidate_purpose(
                TurnPlanBucket::Boundary,
                TurnPlanStopReason::PendingChoice,
                1
            ),
            Some(TurnBoundaryCandidatePurposeV1::PendingChoice)
        );
    }

    #[test]
    fn macro_candidates_reject_losses_and_incomplete_work() {
        assert_eq!(
            candidate_purpose(
                TurnPlanBucket::TerminalLoss,
                TurnPlanStopReason::Terminal,
                2
            ),
            None
        );
        assert_eq!(
            candidate_purpose(
                TurnPlanBucket::Progress,
                TurnPlanStopReason::EngineStepLimit,
                2
            ),
            None
        );
        assert_eq!(
            candidate_purpose(TurnPlanBucket::Progress, TurnPlanStopReason::NextTurn, 0),
            None
        );
    }
}
