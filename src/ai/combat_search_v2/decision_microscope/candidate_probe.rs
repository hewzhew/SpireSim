use crate::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper};
use crate::state::core::ClientInput;

use super::super::*;
use super::{
    CombatSearchV2ActionFactsReport, CombatSearchV2DecisionCandidateReport,
    CombatSearchV2DecisionOneStepReport,
};

pub(super) fn candidate_report(
    root: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    plugins: CombatSearchActionOrderingPlugins<'_>,
    choice: &IndexedActionChoice,
    ordered_index: usize,
    selected_action_key: Option<&str>,
) -> CombatSearchV2DecisionCandidateReport {
    let input = choice.choice.input.clone();
    let role = combat_search_action_ordering_role_label_for_state_with_plugins(
        &root.engine,
        &root.combat,
        &input,
        plugins,
    );
    let selected_by_best_complete = selected_action_key
        .map(|key| key == choice.choice.action_key)
        .unwrap_or(false);
    let step = stepper.apply_to_stable(
        &CombatPosition::new(root.engine.clone(), root.combat.clone()),
        input.clone(),
        CombatStepLimits {
            max_engine_steps: config.max_engine_steps_per_action,
            deadline: None,
        },
    );
    CombatSearchV2DecisionCandidateReport {
        original_action_id: choice.original_action_id,
        ordered_index,
        action_key: choice.choice.action_key.clone(),
        action_debug: choice.choice.action_debug.clone(),
        action_role: role,
        selected_by_best_complete,
        input: input.clone(),
        action_facts: action_facts_report(summarize_action_facts_from_step(
            &root.engine,
            &root.combat,
            &input,
            &step,
        )),
        one_step: one_step_report(root, &input, &step),
    }
}

fn action_facts_report(facts: CombatSearchV2ActionFacts) -> CombatSearchV2ActionFactsReport {
    CombatSearchV2ActionFactsReport {
        schema_name: "CombatSearchV2ActionFactsReport",
        schema_version: 1,
        evidence_policy:
            "static_card_definition_plus_simulator_one_step_delta_no_quality_label_no_teacher_claim",
        consumer_boundary:
            "diagnostic_report_wrapper; search_value_must_consume_CombatSearchV2ActionFacts_not_report_metadata",
        facts,
        notes: vec![
            "action facts describe current-state affordances and exact one-step consequences",
            "facts do not claim the action is good or optimal",
            "one-step deltas use the supplied exact engine state and may include hidden draw/rng truth from that state",
            "long-horizon value must consume pure facts separately and remain explicit about estimate boundaries",
        ],
    }
}

fn one_step_report(
    root: &SearchNode,
    input: &ClientInput,
    step: &crate::sim::combat::CombatStepResult,
) -> CombatSearchV2DecisionOneStepReport {
    let transition = (!step.truncated && !step.timed_out && step.alive).then(|| {
        classify_turn_branch_transition(
            &root.engine,
            &root.combat,
            input,
            &step.position.engine,
            &step.position.combat,
        )
    });
    let phase_profile = combat_search_phase_profile(&step.position.engine, &step.position.combat);
    let visible_hp_loss = (phase_profile.pressure.visible_incoming_damage
        - step.position.combat.entities.player.block)
        .max(0);

    CombatSearchV2DecisionOneStepReport {
        status: step_status(step),
        engine_steps: step.engine_steps,
        terminal: terminal_label(&step.position.engine, &step.position.combat),
        transition: transition.map(|transition| format!("{transition:?}")),
        turn_branch_priority_hint: transition.map(TurnBranchTransition::frontier_priority_hint),
        player_hp: step.position.combat.entities.player.current_hp,
        player_block: step.position.combat.entities.player.block,
        energy: step.position.combat.turn.energy,
        visible_incoming_damage: phase_profile.pressure.visible_incoming_damage,
        visible_hp_loss_if_turn_ends: visible_hp_loss,
        survival_margin: phase_profile.pressure.survival_margin,
        living_enemy_count: living_enemy_count(&step.position.combat),
        total_enemy_hp: phase_profile.enemy_phase.raw_living_enemy_hp,
        total_enemy_block: phase_profile.enemy_phase.raw_living_enemy_block,
        phase_adjusted_enemy_effort: phase_profile.enemy_phase.phase_adjusted_living_enemy_effort,
        split_debt_hp: phase_profile.enemy_phase.split_debt_hp,
        guardian_mode_shift_pending_count: phase_profile
            .enemy_mechanics
            .guardian_mode_shift_pending_count,
        lagavulin_waking_count: phase_profile.enemy_mechanics.lagavulin_waking_count,
        gremlin_nob_anger_amount_total: phase_profile
            .enemy_mechanics
            .gremlin_nob_anger_amount_total,
        sentry_dazed_pressure_count: phase_profile.enemy_mechanics.sentry_dazed_pressure_count,
        hexaghost_opening_pressure_count: phase_profile
            .enemy_mechanics
            .hexaghost_opening_pressure_count,
        pending_choice_present: phase_profile.pending_choice.present,
        pending_choice_estimated_action_fanout: phase_profile
            .pending_choice
            .estimated_action_fanout,
    }
}

fn step_status(step: &crate::sim::combat::CombatStepResult) -> &'static str {
    if step.timed_out {
        "timed_out"
    } else if step.truncated {
        "engine_step_limit"
    } else if !step.alive {
        "player_dead"
    } else {
        "stable"
    }
}
