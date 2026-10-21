use std::time::Instant;

use crate::sim::combat::{CombatPosition, CombatStepLimits, CombatStepResult, CombatStepper};

use super::super::oracle_action_policy::OracleCombatRolloutContractV1;
use super::super::phase_profile::combat_search_phase_profile;
use super::super::rollout_profile::RolloutPerformanceCounters;
use super::super::transition::terminal_label;
use super::super::*;
use super::score_types::{
    RolloutActionFactsProbeScore, RolloutActionProbeResult, RolloutActionProbeScore,
    RolloutTerminalProbeResult, RolloutTerminalProbeScore,
};

pub(super) fn probe_action(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    choice: &IndexedActionChoice,
    ordered_index: usize,
    performance: &mut RolloutPerformanceCounters,
    contract: OracleCombatRolloutContractV1,
) -> Option<RolloutActionProbeResult> {
    if deadline.is_some_and(|limit| Instant::now() >= limit) {
        return None;
    }
    performance.no_potion_probe_score_calls =
        performance.no_potion_probe_score_calls.saturating_add(1);
    let position = CombatPosition::new(node.engine.clone(), node.combat.clone());
    let engine_step_started = Instant::now();
    let step = stepper.apply_to_stable(
        &position,
        choice.choice.input.clone(),
        CombatStepLimits {
            max_engine_steps: config.max_engine_steps_per_action,
            deadline,
        },
    );
    performance.no_potion_probe_engine_step_elapsed_us = performance
        .no_potion_probe_engine_step_elapsed_us
        .saturating_add(engine_step_started.elapsed().as_micros());
    if step.truncated || step.timed_out {
        return None;
    }
    performance.no_potion_probe_actions_evaluated = performance
        .no_potion_probe_actions_evaluated
        .saturating_add(1);
    Some(RolloutActionProbeResult {
        score: rollout_action_probe_score(
            node,
            choice,
            ordered_index,
            &step,
            performance,
            contract,
        ),
        step,
    })
}

pub(super) fn probe_terminal_action(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    choice: &IndexedActionChoice,
    ordered_index: usize,
    performance: &mut RolloutPerformanceCounters,
    contract: OracleCombatRolloutContractV1,
) -> Option<RolloutTerminalProbeResult> {
    if deadline.is_some_and(|limit| Instant::now() >= limit) {
        return None;
    }
    performance.no_potion_probe_score_calls =
        performance.no_potion_probe_score_calls.saturating_add(1);
    let position = CombatPosition::new(node.engine.clone(), node.combat.clone());
    let engine_step_started = Instant::now();
    let step = stepper.apply_to_stable(
        &position,
        choice.choice.input.clone(),
        CombatStepLimits {
            max_engine_steps: config.max_engine_steps_per_action,
            deadline,
        },
    );
    performance.no_potion_probe_engine_step_elapsed_us = performance
        .no_potion_probe_engine_step_elapsed_us
        .saturating_add(engine_step_started.elapsed().as_micros());
    if step.truncated || step.timed_out {
        return None;
    }
    performance.no_potion_probe_actions_evaluated = performance
        .no_potion_probe_actions_evaluated
        .saturating_add(1);
    Some(RolloutTerminalProbeResult {
        score: RolloutTerminalProbeScore {
            terminal_rank: contract_terminal_rank(contract, &step.position),
            final_hp: step.position.combat.entities.player.current_hp,
            ordered_preference: -(ordered_index as i32),
        },
        step,
    })
}

fn rollout_action_probe_score(
    node: &SearchNode,
    choice: &IndexedActionChoice,
    ordered_index: usize,
    step: &CombatStepResult,
    performance: &mut RolloutPerformanceCounters,
    contract: OracleCombatRolloutContractV1,
) -> RolloutActionProbeScore {
    let phase_profile_started = Instant::now();
    let phase_profile = combat_search_phase_profile(&step.position.engine, &step.position.combat);
    performance.no_potion_probe_phase_profile_elapsed_us = performance
        .no_potion_probe_phase_profile_elapsed_us
        .saturating_add(phase_profile_started.elapsed().as_micros());
    let mechanics_pressure = (phase_profile
        .enemy_mechanics
        .guardian_mode_shift_pending_count
        .saturating_add(phase_profile.enemy_mechanics.lagavulin_waking_count)
        .saturating_add(phase_profile.enemy_mechanics.sentry_dazed_pressure_count)
        .saturating_add(
            phase_profile
                .enemy_mechanics
                .hexaghost_opening_pressure_count,
        ) as i32)
        .saturating_add(
            phase_profile
                .enemy_mechanics
                .gremlin_nob_anger_amount_total
                .max(0),
        );
    let action_facts_started = Instant::now();
    let action_facts =
        summarize_action_facts_from_step(&node.engine, &node.combat, &choice.choice.input, step);
    performance.no_potion_probe_action_facts_elapsed_us = performance
        .no_potion_probe_action_facts_elapsed_us
        .saturating_add(action_facts_started.elapsed().as_micros());
    let action_facts_score = action_facts_probe_score(&action_facts);
    let player_block = step.position.combat.entities.player.block;
    let visible_hp_loss = (phase_profile.pressure.visible_incoming_damage - player_block).max(0);
    let recoverable_resource_urgency =
        super::super::action_priority::priority_for_input_with_plugins(
            &node.engine,
            &node.combat,
            &choice.choice.input,
            super::super::CombatSearchActionOrderingPlugins::default(),
        )
        .recoverable_resource_urgency;
    RolloutActionProbeScore {
        terminal_rank: contract_terminal_rank(contract, &step.position),
        recoverable_resource_urgency,
        final_hp: step.position.combat.entities.player.current_hp,
        survival_margin: phase_profile.pressure.survival_margin,
        visible_hp_loss,
        living_enemy_progress: -(living_enemy_count(&step.position.combat) as i32),
        phase_adjusted_enemy_progress: -phase_profile
            .enemy_phase
            .phase_adjusted_living_enemy_effort,
        split_debt_stability: -phase_profile.enemy_phase.split_debt_hp,
        mechanics_stability: -mechanics_pressure,
        action_sustained_mitigation: action_facts_score.sustained_mitigation,
        action_visible_mitigation: action_facts_score.visible_mitigation,
        action_debuff_setup: action_facts_score.debuff_setup,
        action_resource_timing: action_facts_score.resource_timing,
        action_progress_hint: action_facts_score.progress_hint,
        action_access_gain: action_facts_score.access_gain,
        action_reactive_safety: action_facts_score.reactive_safety,
        pending_choice_fanout: -(phase_profile.pending_choice.estimated_action_fanout as i32),
        ordered_preference: -(ordered_index as i32),
        nonterminal_upgrade_eligible: !matches!(choice.choice.input, ClientInput::EndTurn),
    }
}

fn contract_terminal_rank(
    contract: OracleCombatRolloutContractV1,
    position: &CombatPosition,
) -> i32 {
    let terminal = terminal_label(&position.engine, &position.combat);
    if terminal == SearchTerminalLabel::Win && !contract.accepts_terminal_position(position) {
        terminal_rank(SearchTerminalLabel::Unresolved)
    } else {
        terminal_rank(terminal)
    }
}

fn action_facts_probe_score(facts: &CombatSearchV2ActionFacts) -> RolloutActionFactsProbeScore {
    RolloutActionFactsProbeScore {
        sustained_mitigation: facts.mechanics.direct.persistent_enemy_strength_down,
        visible_mitigation: facts
            .mechanics
            .direct
            .temporary_enemy_strength_down
            .saturating_add(facts.mechanics.direct.visible_attack_mitigation_hint)
            .saturating_add(facts.mechanics.derived.enemy_weak),
        debuff_setup: facts.mechanics.derived.enemy_vulnerable,
        resource_timing: facts.mechanics.resource_timing.ordering_score,
        progress_hint: facts
            .immediate
            .target_progress_hint
            .max(facts.immediate.all_enemy_progress_hint)
            .saturating_add(facts.mechanics.reactive.enemy_damage),
        access_gain: facts
            .exact_one_step_delta
            .hand_delta
            .saturating_add(1)
            .max(0)
            .saturating_add(facts.exact_one_step_delta.energy_delta.max(0)),
        reactive_safety: -facts
            .mechanics
            .derived
            .enemy_strength_gain
            .saturating_add(facts.mechanics.derived.visible_attack_pressure_hint)
            .saturating_add(facts.mechanics.reactive.player_hp_loss)
            .saturating_add(facts.mechanics.reactive.bad_draw_cards)
            .saturating_add(i32::from(facts.mechanics.reactive.forced_turn_end)),
    }
}
