mod attribution;
mod extension;
mod selection;
mod types;

use std::time::Instant;

use crate::sim::combat::CombatStepper;

use super::super::rollout_profile::RolloutPerformanceCounters;
#[cfg(test)]
use super::super::value::combat_eval_from_rollout_estimate;
use super::super::*;

pub(in crate::ai::combat_search_v2) use attribution::TurnBeamExtensionAttribution;
pub(in crate::ai::combat_search_v2) use extension::turn_beam_extension_rollout_with_attribution;

const TURN_BEAM_ACTION_REASON: &str = "turn_beam_no_potion_selected_turn_plan_end_state";
const TURN_BEAM_NO_PLAN_REASON: &str = "turn_beam_no_potion_no_turn_plan_available";
const TURN_BEAM_CONSERVATIVE_ANCHOR_REASON: &str = "turn_beam_no_potion_conservative_anchor";
const TURN_BEAM_BOUNDARY_FALLBACK_REASON: &str =
    "turn_beam_no_potion_conservative_boundary_fallback";

#[cfg(test)]
pub(in crate::ai::combat_search_v2) fn turn_beam_no_potion_rollout(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    max_actions: usize,
    deadline: Option<Instant>,
) -> RolloutNodeEstimate {
    let mut performance = RolloutPerformanceCounters::default();
    if !matches!(node.engine, EngineState::CombatPlayerTurn) {
        return turn_beam_extension_rollout(
            node,
            stepper,
            config,
            max_actions,
            deadline,
            &mut performance,
        );
    }
    let anchor = turn_beam_conservative_anchor_rollout(
        node,
        stepper,
        config,
        max_actions,
        deadline,
        &mut performance,
    );
    if anchor.terminal == SearchTerminalLabel::Win {
        return anchor;
    }
    let beam = turn_beam_extension_rollout(
        node,
        stepper,
        config,
        max_actions,
        deadline,
        &mut performance,
    );
    better_estimate(beam, anchor)
}

pub(in crate::ai::combat_search_v2) fn turn_beam_conservative_anchor_rollout(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    max_actions: usize,
    deadline: Option<Instant>,
    performance: &mut RolloutPerformanceCounters,
) -> RolloutNodeEstimate {
    let mut estimate = super::conservative_no_potion_rollout(
        node,
        stepper,
        config,
        max_actions,
        deadline,
        performance,
    );
    estimate.last_action_reason = Some(TURN_BEAM_CONSERVATIVE_ANCHOR_REASON);
    estimate
}

#[cfg(test)]
fn turn_beam_extension_rollout(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    max_actions: usize,
    deadline: Option<Instant>,
    performance: &mut RolloutPerformanceCounters,
) -> RolloutNodeEstimate {
    turn_beam_extension_rollout_with_attribution(
        node,
        stepper,
        config,
        max_actions,
        deadline,
        performance,
    )
    .0
}

#[cfg(test)]
fn better_estimate(left: RolloutNodeEstimate, right: RolloutNodeEstimate) -> RolloutNodeEstimate {
    let left_eval = combat_eval_from_rollout_estimate(&left);
    let right_eval = combat_eval_from_rollout_estimate(&right);
    if right_eval > left_eval {
        right
    } else {
        left
    }
}
