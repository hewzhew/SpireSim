use crate::ai::combat_search_v2::rollout_pending_choice::RolloutPendingChoiceProgress;
use crate::ai::combat_search_v2::turn_planner::types::{
    TurnPlanBucket, TurnPlanStopReason, TurnPlanV1,
};
use crate::ai::combat_search_v2::value::{combat_eval_from_rollout_estimate, CombatEvalV2};
use crate::ai::combat_search_v2::*;

use super::work::TurnPlanWorkNode;

pub(super) fn plan_from_node(
    work: TurnPlanWorkNode,
    root_action_len: usize,
    stop_reason: TurnPlanStopReason,
    root_eval: CombatEvalV2,
) -> TurnPlanV1 {
    let mut node = work.node;
    let pending_choice_progress = pending_choice_progress_for_plan(&node, stop_reason);
    let estimate = RolloutNodeEstimate::from_node(
        &node,
        node.actions.len().saturating_sub(root_action_len),
        rollout_stop_reason_for_turn_plan(stop_reason),
        None,
        pending_choice_progress,
    );
    let eval = combat_eval_from_rollout_estimate(&estimate);
    node.rollout_estimate = estimate;
    TurnPlanV1 {
        actions: node.actions[root_action_len..].to_vec(),
        action_facts: work.action_facts,
        step_states: work.step_states,
        end_node: node,
        stop_reason,
        bucket: TurnPlanBucket::from_root_and_eval(root_eval, eval, stop_reason),
        eval,
    }
}

pub(super) fn root_eval(root: &SearchNode) -> CombatEvalV2 {
    let estimate = RolloutNodeEstimate::from_node(
        root,
        0,
        RolloutStopReason::MaxActions,
        None,
        RolloutPendingChoiceProgress::default(),
    );
    combat_eval_from_rollout_estimate(&estimate)
}

pub(super) fn stop_reason_for_transition(transition: TurnBranchTransition) -> TurnPlanStopReason {
    if transition.is_terminal() {
        TurnPlanStopReason::Terminal
    } else if transition.is_next_turn() {
        TurnPlanStopReason::NextTurn
    } else if transition.is_pending_choice() {
        TurnPlanStopReason::PendingChoice
    } else {
        TurnPlanStopReason::OtherBoundary
    }
}

fn pending_choice_progress_for_plan(
    node: &SearchNode,
    stop_reason: TurnPlanStopReason,
) -> RolloutPendingChoiceProgress {
    let mut progress = RolloutPendingChoiceProgress::default();
    if stop_reason == TurnPlanStopReason::PendingChoice {
        progress.observe_boundary(
            combat_search_phase_profile(&node.engine, &node.combat).pending_choice,
        );
    }
    progress
}

fn rollout_stop_reason_for_turn_plan(stop_reason: TurnPlanStopReason) -> RolloutStopReason {
    match stop_reason {
        TurnPlanStopReason::Terminal => RolloutStopReason::TerminalState,
        TurnPlanStopReason::NextTurn => RolloutStopReason::MaxActions,
        TurnPlanStopReason::PendingChoice => RolloutStopReason::PolicyDeclined,
        TurnPlanStopReason::OtherBoundary => RolloutStopReason::PolicyDeclined,
        TurnPlanStopReason::NoLegalActions => RolloutStopReason::NoLegalActions,
        TurnPlanStopReason::EngineStepLimit => RolloutStopReason::EngineStepLimit,
    }
}
