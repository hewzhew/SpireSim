use std::time::Instant;

use crate::sim::combat::CombatStepper;

use super::super::super::rollout_pending_choice::RolloutPendingChoiceProgress;
use super::super::super::rollout_profile::RolloutPerformanceCounters;
use super::super::super::turn_planner::{enumerate_turn_plans, TurnPlannerConfigV1};
use super::super::super::*;
use super::attribution::TurnBeamExtensionAttribution;
use super::selection::{
    best_estimate, finish_with_attribution, merge_prior_pending_choice_progress, select_beam,
    state_estimate,
};
use super::types::TurnBeamState;
use super::{
    TURN_BEAM_ACTION_REASON, TURN_BEAM_BOUNDARY_FALLBACK_REASON, TURN_BEAM_NO_PLAN_REASON,
};

pub(in crate::ai::combat_search_v2) fn turn_beam_extension_rollout_with_attribution(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    max_actions: usize,
    deadline: Option<Instant>,
    performance: &mut RolloutPerformanceCounters,
) -> (RolloutNodeEstimate, TurnBeamExtensionAttribution) {
    let root_action_count = node.actions.len();
    let beam_width = config.rollout_beam_width.max(1);
    let mut attribution = TurnBeamExtensionAttribution::default();
    let mut beam = vec![TurnBeamState {
        node: node.clone_for_rollout(),
        progress: RolloutPendingChoiceProgress::default(),
        last_action_reason: None,
        estimate_override: None,
        stalled_stop_reason: None,
    }];

    loop {
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            return finish_with_attribution(
                best_estimate(&beam, root_action_count, RolloutStopReason::Deadline),
                attribution,
            );
        }

        if let Some(winner) = beam
            .iter()
            .filter(|state| {
                terminal_label(&state.node.engine, &state.node.combat) == SearchTerminalLabel::Win
            })
            .max_by_key(|state| state.node.combat.entities.player.current_hp)
        {
            return finish_with_attribution(
                state_estimate(winner, root_action_count, RolloutStopReason::TerminalState),
                attribution,
            );
        }
        if beam.iter().all(|state| {
            terminal_label(&state.node.engine, &state.node.combat)
                != SearchTerminalLabel::Unresolved
        }) {
            return finish_with_attribution(
                best_estimate(&beam, root_action_count, RolloutStopReason::TerminalState),
                attribution,
            );
        }

        let max_simulated = beam
            .iter()
            .map(|state| state.node.actions.len().saturating_sub(root_action_count))
            .max()
            .unwrap_or_default();
        if max_simulated >= max_actions {
            return finish_with_attribution(
                best_estimate(&beam, root_action_count, RolloutStopReason::MaxActions),
                attribution,
            );
        }

        let mut next = Vec::new();
        let mut stalled = Vec::new();
        for state in &beam {
            let mut progress = state.progress;
            let phase_profile = combat_search_phase_profile(&state.node.engine, &state.node.combat);
            progress.observe_boundary(phase_profile.pending_choice);
            if phase_profile.pending_choice.high_fanout {
                stalled.push(TurnBeamState {
                    node: state.node.clone(),
                    progress,
                    last_action_reason: state.last_action_reason,
                    estimate_override: None,
                    stalled_stop_reason: Some(RolloutStopReason::HighFanoutPendingChoice),
                });
                continue;
            }
            if !matches!(state.node.engine, EngineState::CombatPlayerTurn) {
                let simulated = state.node.actions.len().saturating_sub(root_action_count);
                let remaining_actions = max_actions.saturating_sub(simulated);
                let mut estimate = super::super::conservative_no_potion_rollout(
                    &state.node,
                    stepper,
                    config,
                    remaining_actions,
                    deadline,
                    performance,
                );
                estimate.actions_simulated = estimate.actions_simulated.saturating_add(simulated);
                estimate.last_action_reason = Some(TURN_BEAM_BOUNDARY_FALLBACK_REASON);
                merge_prior_pending_choice_progress(&mut estimate, state.progress);
                stalled.push(TurnBeamState {
                    node: state.node.clone(),
                    progress,
                    last_action_reason: state.last_action_reason,
                    estimate_override: Some(estimate),
                    stalled_stop_reason: None,
                });
                continue;
            }

            let remaining_actions = max_actions
                .saturating_sub(state.node.actions.len().saturating_sub(root_action_count));
            if remaining_actions == 0 {
                stalled.push(TurnBeamState {
                    node: state.node.clone(),
                    progress,
                    last_action_reason: state.last_action_reason,
                    estimate_override: None,
                    stalled_stop_reason: Some(RolloutStopReason::MaxActions),
                });
                continue;
            }

            let turn_config = TurnPlannerConfigV1 {
                max_end_states: beam_width.saturating_mul(2),
                per_bucket_limit: beam_width,
                potion_policy: CombatSearchV2PotionPolicy::Never,
                max_engine_steps_per_action: config.max_engine_steps_per_action,
                capture_step_trace: false,
                ..TurnPlannerConfigV1::default()
            };
            let plans = enumerate_turn_plans(&state.node, stepper, &turn_config, deadline);
            attribution.observe_turn_plan_enumeration(&plans);
            if plans.plans.is_empty() {
                let mut no_plan = state.node.clone();
                no_plan.rollout_estimate = RolloutNodeEstimate::from_node(
                    &no_plan,
                    no_plan.actions.len().saturating_sub(root_action_count),
                    RolloutStopReason::NoLegalActions,
                    Some(TURN_BEAM_NO_PLAN_REASON),
                    progress,
                );
                stalled.push(TurnBeamState {
                    node: no_plan,
                    progress,
                    last_action_reason: Some(TURN_BEAM_NO_PLAN_REASON),
                    estimate_override: None,
                    stalled_stop_reason: Some(RolloutStopReason::NoLegalActions),
                });
            } else {
                let before = next.len();
                next.extend(
                    plans
                        .plans
                        .into_iter()
                        .filter(|plan| {
                            plan.end_node
                                .actions
                                .len()
                                .saturating_sub(root_action_count)
                                <= max_actions
                        })
                        .map(|plan| TurnBeamState {
                            node: plan.end_node,
                            progress,
                            last_action_reason: Some(TURN_BEAM_ACTION_REASON),
                            estimate_override: None,
                            stalled_stop_reason: None,
                        }),
                );
                if next.len() == before {
                    stalled.push(TurnBeamState {
                        node: state.node.clone(),
                        progress,
                        last_action_reason: state.last_action_reason,
                        estimate_override: None,
                        stalled_stop_reason: Some(RolloutStopReason::MaxActions),
                    });
                }
            }
        }

        if next.is_empty() {
            if !stalled.is_empty() {
                debug_assert!(stalled.iter().all(|state| {
                    state.estimate_override.is_some() || state.stalled_stop_reason.is_some()
                }));
                return finish_with_attribution(
                    best_estimate(
                        &stalled,
                        root_action_count,
                        RolloutStopReason::NoLegalActions,
                    ),
                    attribution,
                );
            }
            return finish_with_attribution(
                best_estimate(&beam, root_action_count, RolloutStopReason::NoLegalActions),
                attribution,
            );
        }

        beam = select_beam(next, beam_width);
    }
}
