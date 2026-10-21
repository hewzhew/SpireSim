use std::time::Instant;

use super::super::oracle_action_policy::OracleCombatRolloutContractV1;
use super::super::rollout_pending_choice::{
    linear_pending_choice_actions, RolloutPendingChoiceProgress,
};
use super::super::rollout_profile::RolloutPerformanceCounters;
use super::super::*;

pub(in crate::ai::combat_search_v2) fn conservative_no_potion_rollout(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    max_actions: usize,
    deadline: Option<Instant>,
    performance: &mut RolloutPerformanceCounters,
) -> RolloutNodeEstimate {
    no_potion_rollout(
        CombatSearchRolloutPluginId::ConservativeNoPotion,
        node,
        stepper,
        config,
        max_actions,
        deadline,
        performance,
        OracleCombatRolloutContractV1::default(),
    )
}

pub(in crate::ai::combat_search_v2) fn phase_aware_no_potion_rollout(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    max_actions: usize,
    deadline: Option<Instant>,
    performance: &mut RolloutPerformanceCounters,
) -> RolloutNodeEstimate {
    no_potion_rollout(
        CombatSearchRolloutPluginId::PhaseAwareNoPotion,
        node,
        stepper,
        config,
        max_actions,
        deadline,
        performance,
        OracleCombatRolloutContractV1::default(),
    )
}

pub(in crate::ai::combat_search_v2) fn phase_aware_no_potion_rollout_for_contract(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    max_actions: usize,
    deadline: Option<Instant>,
    performance: &mut RolloutPerformanceCounters,
    contract: OracleCombatRolloutContractV1,
) -> RolloutNodeEstimate {
    no_potion_rollout(
        CombatSearchRolloutPluginId::PhaseAwareNoPotion,
        node,
        stepper,
        config,
        max_actions,
        deadline,
        performance,
        contract,
    )
}

fn no_potion_rollout(
    policy: CombatSearchRolloutPluginId,
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    max_actions: usize,
    deadline: Option<Instant>,
    performance: &mut RolloutPerformanceCounters,
    contract: OracleCombatRolloutContractV1,
) -> RolloutNodeEstimate {
    let mut rollout = node.clone_for_rollout();
    let mut last_action_reason = None;
    let mut pending_choice_progress = RolloutPendingChoiceProgress::default();
    for actions_simulated in 0..=max_actions {
        performance.no_potion_iterations = performance.no_potion_iterations.saturating_add(1);
        if terminal_label(&rollout.engine, &rollout.combat) != SearchTerminalLabel::Unresolved {
            return RolloutNodeEstimate::from_node(
                &rollout,
                actions_simulated,
                RolloutStopReason::TerminalState,
                last_action_reason,
                pending_choice_progress,
            );
        }
        if actions_simulated == max_actions {
            return RolloutNodeEstimate::from_node(
                &rollout,
                actions_simulated,
                RolloutStopReason::MaxActions,
                last_action_reason,
                pending_choice_progress,
            );
        }
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            return RolloutNodeEstimate::from_node(
                &rollout,
                actions_simulated,
                RolloutStopReason::Deadline,
                last_action_reason,
                pending_choice_progress,
            );
        }
        let phase_profile_started = Instant::now();
        let phase_profile = combat_search_phase_profile(&rollout.engine, &rollout.combat);
        performance.no_potion_phase_profile_elapsed_us = performance
            .no_potion_phase_profile_elapsed_us
            .saturating_add(phase_profile_started.elapsed().as_micros());
        pending_choice_progress.observe_boundary(phase_profile.pending_choice);
        let search_owned_structured_choice = stepper.supports_canonical_pending_choice_actions()
            && matches!(
                &rollout.engine,
                EngineState::PendingChoice(choice)
                    if PendingChoiceActionFamily::from_choice(choice).is_some()
            );
        if phase_profile.pending_choice.high_fanout {
            return RolloutNodeEstimate::from_node(
                &rollout,
                actions_simulated,
                RolloutStopReason::HighFanoutPendingChoice,
                last_action_reason,
                pending_choice_progress,
            );
        }

        let position = CombatPosition::new(rollout.engine.clone(), rollout.combat.clone());
        let structured_actions = search_owned_structured_choice
            .then(|| linear_pending_choice_actions(&position, stepper))
            .flatten();
        if search_owned_structured_choice && structured_actions.is_none() {
            return RolloutNodeEstimate::from_node(
                &rollout,
                actions_simulated,
                RolloutStopReason::StructuredPendingChoice,
                last_action_reason,
                pending_choice_progress,
            );
        }

        let legal_actions_started = Instant::now();
        let legal = filtered_rollout_legal_actions(
            policy,
            structured_actions.unwrap_or_else(|| stepper.atomic_action_choices(&position)),
            &rollout.combat,
        );
        performance.no_potion_legal_actions_elapsed_us = performance
            .no_potion_legal_actions_elapsed_us
            .saturating_add(legal_actions_started.elapsed().as_micros());
        if legal.is_empty() {
            return RolloutNodeEstimate::from_node(
                &rollout,
                actions_simulated,
                RolloutStopReason::NoLegalActions,
                last_action_reason,
                pending_choice_progress,
            );
        }

        let choose_action_started = Instant::now();
        let Some(selection) = choose_rollout_action(
            policy,
            &rollout,
            stepper,
            config,
            deadline,
            &rollout.engine,
            &rollout.combat,
            legal,
            performance,
            contract,
        ) else {
            performance.no_potion_choose_action_elapsed_us = performance
                .no_potion_choose_action_elapsed_us
                .saturating_add(choose_action_started.elapsed().as_micros());
            return RolloutNodeEstimate::from_node(
                &rollout,
                actions_simulated,
                RolloutStopReason::PolicyDeclined,
                last_action_reason,
                pending_choice_progress,
            );
        };
        performance.no_potion_choose_action_elapsed_us = performance
            .no_potion_choose_action_elapsed_us
            .saturating_add(choose_action_started.elapsed().as_micros());
        last_action_reason = Some(selection.reason);
        let cached_step = selection.cached_step;
        let choice = selection.choice;
        pending_choice_progress.note_simulated_action(phase_profile.pending_choice);

        let step = if let Some(step) = cached_step {
            performance.no_potion_probe_step_reuses =
                performance.no_potion_probe_step_reuses.saturating_add(1);
            step
        } else {
            let engine_step_started = Instant::now();
            let step = stepper.apply_to_stable(
                &position,
                choice.choice.input.clone(),
                CombatStepLimits {
                    max_engine_steps: config.max_engine_steps_per_action,
                    deadline,
                },
            );
            performance.no_potion_engine_step_elapsed_us = performance
                .no_potion_engine_step_elapsed_us
                .saturating_add(engine_step_started.elapsed().as_micros());
            step
        };
        let child_build_started = Instant::now();
        let mut child = rollout.clone_for_child(step.position.engine, step.position.combat);
        child.note_input(&choice.choice.input);
        child.push_action(CombatSearchV2ActionTrace {
            step_index: rollout.actions.len(),
            action_id: choice.original_action_id,
            action_key: choice.choice.action_key,
            action_debug: choice.choice.action_debug,
            input: choice.choice.input,
        });
        rollout = child;
        performance.no_potion_child_build_elapsed_us = performance
            .no_potion_child_build_elapsed_us
            .saturating_add(child_build_started.elapsed().as_micros());

        if step.truncated {
            return RolloutNodeEstimate::from_node(
                &rollout,
                actions_simulated + 1,
                RolloutStopReason::EngineStepLimit,
                last_action_reason,
                pending_choice_progress,
            );
        }
    }

    RolloutNodeEstimate::from_node(
        &rollout,
        max_actions,
        RolloutStopReason::MaxActions,
        last_action_reason,
        pending_choice_progress,
    )
}
