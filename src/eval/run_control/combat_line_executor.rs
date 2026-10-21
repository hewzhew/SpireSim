use crate::ai::combat_search_v2::{
    CombatSearchV2ActionTrace, CombatSearchV2Config, CombatSearchV2Report,
    CombatSearchV2TurnSegmentReport,
};
use crate::sim::combat::{CombatPosition, CombatStepper, CombatTerminal, EngineCombatStepper};
use crate::state::core::ClientInput;

use super::atomic_combat_search_render::{
    render_complete_line_solver_application, render_search_application, render_segment_application,
};
use super::combat_candidate_line::{replay_candidate_line, CombatCandidateLine};
use super::combat_line_trace::{
    combat_automation_answer_claims_v1, combat_automation_opportunity_state_v1,
    combat_automation_step_state_v1, combat_line_performance_trace_annotation,
    combat_search_performance_trace_annotation, current_run_apply_status,
    CombatCandidateLinePerformance,
};
use super::combat_resolution::{
    RunCombatResolutionBoundaryV1, RunCombatResolutionKindV1, RunCombatResolutionV1,
};
use super::session::{RunControlSession, RunProgressOutcome};
use super::trace_annotation::{
    CombatAutomationActionV1, CombatAutomationTrajectoryRecordV1, CombatAutomationTrajectorySource,
};
use super::transition_report::{
    action_result_from_transition, render_action_result, ActionResult, ActionResultChange,
    CardSnapshot, RunVisibleSnapshot, TransitionAction,
};

pub(super) fn apply_selected_combat_candidate_line(
    session: &mut RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    report: &CombatSearchV2Report,
    mut selected_line: CombatCandidateLine,
    trajectory_source: CombatAutomationTrajectorySource,
    transition_label: String,
    line_performance: Option<CombatCandidateLinePerformance>,
) -> Result<RunProgressOutcome, String> {
    let replay =
        replay_candidate_line(start, selected_line.source, &selected_line.actions, config)?;
    if replay.line.terminal != CombatTerminal::Win {
        return Err(format!(
            "selected combat candidate line did not replay to win; source={} terminal={:?}",
            replay.line.source.label(),
            replay.line.terminal
        ));
    }
    selected_line = replay.line;
    let mut trial = session.clone();
    let master_deck = active_combat_master_deck(&trial);
    let resolution_before = RunCombatResolutionBoundaryV1::capture(&trial);
    let before_snapshot = RunVisibleSnapshot::capture(&trial);
    let applied = selected_line.actions.clone();
    trial.mark_current_combat_search_resolved();
    let automation_actions = apply_combat_action_traces(&mut trial, &applied)?;
    let after_snapshot = RunVisibleSnapshot::capture(&trial);
    let status = current_run_apply_status(&trial);
    let action_result = action_result_from_transition(
        TransitionAction {
            label: transition_label,
        },
        &before_snapshot,
        &after_snapshot,
        status,
    );
    let application = if line_performance.is_some() {
        render_complete_line_solver_application(
            report,
            &applied,
            &selected_line,
            replay.applied_count,
        )
    } else {
        render_search_application(report, &applied, &selected_line, replay.applied_count)
    };
    let message = format!(
        "{}\n{}\n{}",
        application,
        render_action_result(&action_result),
        super::render::render_run_control_state(&trial)
    );
    let answer_claims = combat_automation_answer_claims_v1(&master_deck, &automation_actions);
    let automation_record =
        CombatAutomationTrajectoryRecordV1::new(trajectory_source, automation_actions)
            .with_answer_claims(answer_claims);
    trial.remember_combat_automation_trajectory(automation_record.clone());
    let resolution_after = RunCombatResolutionBoundaryV1::capture(&trial);
    let resolution = RunCombatResolutionV1::new(
        RunCombatResolutionKindV1::CompleteVictory,
        resolution_before,
        automation_record.clone(),
        action_result.clone(),
        resolution_after,
    )?;
    let outcome = RunProgressOutcome::action(message, action_result)
        .with_trace_annotations(vec![
            automation_record.into_annotation(),
            combat_line_performance_trace_annotation(
                trajectory_source.label(),
                &trial,
                start,
                report,
                &selected_line,
                line_performance,
            ),
        ])
        .with_progress_step(super::RunProgressStepV1::CombatResolution(resolution));
    *session = trial;
    Ok(outcome)
}

pub(super) fn apply_oracle_combat_witness(
    session: &mut RunControlSession,
    start: &CombatPosition,
    witness: &sts_combat_planner::OracleCombatWitness,
    trajectory_source: CombatAutomationTrajectorySource,
) -> Result<RunProgressOutcome, String> {
    if witness.actions.is_empty() {
        return Err("oracle combat witness contains no actions".to_string());
    }
    if witness.final_position.combat.runtime.combat_smoked {
        return Err(
            "oracle combat victory witness is a Smoke Bomb escape and must use the escape resolution channel"
                .to_string(),
        );
    }
    if session.current_active_combat_position()? != *start {
        return Err("oracle combat parent changed before witness commit".to_string());
    }

    let mut trial = session.clone();
    let master_deck = active_combat_master_deck(&trial);
    let resolution_before = RunCombatResolutionBoundaryV1::capture(&trial);
    let before_snapshot = RunVisibleSnapshot::capture(&trial);
    let mut automation_actions = Vec::with_capacity(witness.actions.len());
    trial.mark_current_combat_search_resolved();
    for (step_index, action) in witness.actions.iter().enumerate() {
        let position = trial.current_combat_position_for_actions().map_err(|error| {
            format!(
                "oracle witness lost its active combat before action {step_index}/{} ({:?}): {error}; witness inputs={:?}",
                witness.actions.len(),
                action.input,
                witness
                    .actions
                    .iter()
                    .map(|step| &step.input)
                    .collect::<Vec<_>>()
            )
        })?;
        let choice = EngineCombatStepper
            .choice_for_legal_input(&position, &action.input)
            .ok_or_else(|| format!("oracle witness action {step_index} is no longer legal"))?;
        let opportunity_before = combat_automation_opportunity_state_v1(&trial);
        let outcome = trial.apply_combat_resolution_input(action.input.clone())?;
        automation_actions.push(CombatAutomationActionV1 {
            step_index,
            action_key: choice.action_key,
            input: action.input.clone(),
            opportunity_before,
            drawn_cards: drawn_cards_from_action_result(outcome.action_result.as_ref()),
            combat_after: combat_automation_step_state_v1(&trial),
        });
    }

    let after_snapshot = RunVisibleSnapshot::capture(&trial);
    let status = current_run_apply_status(&trial);
    let action_result = action_result_from_transition(
        TransitionAction {
            label: format!(
                "oracle witness applied {} exact actions",
                witness.actions.len()
            ),
        },
        &before_snapshot,
        &after_snapshot,
        status,
    );
    let answer_claims = combat_automation_answer_claims_v1(&master_deck, &automation_actions);
    let automation_record =
        CombatAutomationTrajectoryRecordV1::new(trajectory_source, automation_actions)
            .with_answer_claims(answer_claims);
    trial.remember_combat_automation_trajectory(automation_record.clone());
    let resolution_after = RunCombatResolutionBoundaryV1::capture(&trial);
    let resolution = RunCombatResolutionV1::new(
        RunCombatResolutionKindV1::CompleteVictory,
        resolution_before,
        automation_record.clone(),
        action_result.clone(),
        resolution_after,
    )?;
    let message = format!(
        "Oracle combat witness verified and applied: actions={} replay_engine_steps={} policy_cost={:.6}\n{}\n{}",
        witness.actions.len(),
        witness.replay_engine_steps,
        witness.negative_log_policy,
        render_action_result(&action_result),
        super::render::render_run_control_state(&trial)
    );
    let outcome = RunProgressOutcome::action(message, action_result)
        .with_trace_annotations(vec![automation_record.into_annotation()])
        .with_progress_step(super::RunProgressStepV1::CombatResolution(resolution));
    *session = trial;
    Ok(outcome)
}

pub(super) fn apply_combat_turn_segment(
    session: &mut RunControlSession,
    start: &CombatPosition,
    search_report: &CombatSearchV2Report,
    segment_report: &CombatSearchV2TurnSegmentReport,
    rejection_result: &'static str,
) -> Result<RunProgressOutcome, String> {
    let trajectory = segment_report
        .selected
        .as_ref()
        .expect("caller only applies after selecting a segment");
    let mut trial = session.clone();
    let master_deck = active_combat_master_deck(&trial);
    let resolution_before = RunCombatResolutionBoundaryV1::capture(&trial);
    let before_snapshot = RunVisibleSnapshot::capture(&trial);
    let applied = trajectory.actions.clone();
    trial.mark_current_combat_search_resolved();
    let automation_actions = apply_combat_action_traces(&mut trial, &applied)?;
    let after_snapshot = RunVisibleSnapshot::capture(&trial);
    let status = current_run_apply_status(&trial);
    let transition_label = format!(
        "search-combat segment applied {} actions (partial turn; not terminal claim)",
        applied.len()
    );
    let action_result = action_result_from_transition(
        TransitionAction {
            label: transition_label,
        },
        &before_snapshot,
        &after_snapshot,
        status,
    );
    let message = format!(
        "{}\n{}\n{}",
        render_segment_application(search_report, segment_report, rejection_result),
        render_action_result(&action_result),
        super::render::render_run_control_state(&trial)
    );
    let answer_claims = combat_automation_answer_claims_v1(&master_deck, &automation_actions);
    let automation_record = CombatAutomationTrajectoryRecordV1::new(
        CombatAutomationTrajectorySource::SearchCombatTurnSegment,
        automation_actions,
    )
    .with_answer_claims(answer_claims);
    trial.remember_combat_automation_trajectory(automation_record.clone());
    let resolution_after = RunCombatResolutionBoundaryV1::capture(&trial);
    let resolution = RunCombatResolutionV1::new(
        RunCombatResolutionKindV1::TurnSegment,
        resolution_before,
        automation_record.clone(),
        action_result.clone(),
        resolution_after,
    )?;
    let outcome = RunProgressOutcome::action(message, action_result)
        .with_trace_annotations(vec![
            automation_record.into_annotation(),
            combat_search_performance_trace_annotation(
                "search_combat_turn_segment",
                &trial,
                start,
                search_report,
            ),
        ])
        .with_progress_step(super::RunProgressStepV1::CombatResolution(resolution));
    *session = trial;
    Ok(outcome)
}

pub(super) fn apply_smoke_bomb_survival_fallback(
    session: &mut RunControlSession,
    smoke_input: ClientInput,
    rejection_result: &'static str,
) -> Result<RunProgressOutcome, String> {
    let mut trial = session.clone();
    let master_deck = active_combat_master_deck(&trial);
    let resolution_before = RunCombatResolutionBoundaryV1::capture(&trial);
    let before_snapshot = RunVisibleSnapshot::capture(&trial);
    let mut automation_actions = Vec::new();
    trial.mark_current_combat_search_resolved();
    let opportunity_before = combat_automation_opportunity_state_v1(&trial);
    let outcome = trial.apply_combat_resolution_input(smoke_input.clone())?;
    automation_actions.push(CombatAutomationActionV1 {
        step_index: 0,
        action_key: "combat/use_smoke_bomb_survival".to_string(),
        input: smoke_input,
        opportunity_before,
        drawn_cards: drawn_cards_from_action_result(outcome.action_result.as_ref()),
        combat_after: combat_automation_step_state_v1(&trial),
    });
    if active_combat_is_waiting_for_smoke_escape_turn_end(&trial) {
        let opportunity_before = combat_automation_opportunity_state_v1(&trial);
        let end_turn_outcome = trial.apply_combat_resolution_input(ClientInput::EndTurn)?;
        automation_actions.push(CombatAutomationActionV1 {
            step_index: 1,
            action_key: "combat/end_turn_after_smoke_bomb".to_string(),
            input: ClientInput::EndTurn,
            opportunity_before,
            drawn_cards: drawn_cards_from_action_result(end_turn_outcome.action_result.as_ref()),
            combat_after: combat_automation_step_state_v1(&trial),
        });
    }
    let after_snapshot = RunVisibleSnapshot::capture(&trial);
    let status = current_run_apply_status(&trial);
    let transition_label = format!(
        "Smoke Bomb survival fallback after {rejection_result} (not a combat victory claim)"
    );
    let action_result = action_result_from_transition(
        TransitionAction {
            label: transition_label,
        },
        &before_snapshot,
        &after_snapshot,
        status,
    );
    let message = format!(
        "Search combat did not find a complete win; used Smoke Bomb as a survival fallback after {rejection_result}.\n{}\n{}",
        render_action_result(&action_result),
        super::render::render_run_control_state(&trial)
    );
    let answer_claims = combat_automation_answer_claims_v1(&master_deck, &automation_actions);
    let automation_record = CombatAutomationTrajectoryRecordV1::new(
        CombatAutomationTrajectorySource::SearchCombatSmokeBombSurvival,
        automation_actions,
    )
    .with_answer_claims(answer_claims);
    trial.remember_combat_automation_trajectory(automation_record.clone());
    let resolution_after = RunCombatResolutionBoundaryV1::capture(&trial);
    let resolution = RunCombatResolutionV1::new(
        RunCombatResolutionKindV1::SmokeBombEscape,
        resolution_before,
        automation_record.clone(),
        action_result.clone(),
        resolution_after,
    )?;
    let outcome = RunProgressOutcome::action(message, action_result)
        .with_trace_annotations(vec![automation_record.into_annotation()])
        .with_progress_step(super::RunProgressStepV1::CombatResolution(resolution));
    *session = trial;
    Ok(outcome)
}

fn active_combat_is_waiting_for_smoke_escape_turn_end(session: &RunControlSession) -> bool {
    session
        .active_combat
        .as_ref()
        .is_some_and(|active| active.combat_state.turn.counters.player_escaping)
}

fn active_combat_master_deck(
    session: &RunControlSession,
) -> Vec<crate::runtime::combat::CombatCard> {
    session
        .active_combat
        .as_ref()
        .map(|active| active.combat_state.meta.master_deck_snapshot.to_vec())
        .unwrap_or_default()
}

fn apply_combat_action_traces(
    session: &mut RunControlSession,
    actions: &[CombatSearchV2ActionTrace],
) -> Result<Vec<CombatAutomationActionV1>, String> {
    actions
        .iter()
        .map(|action| {
            let opportunity_before = combat_automation_opportunity_state_v1(session);
            let outcome = session.apply_combat_resolution_input(action.input.clone())?;
            Ok(CombatAutomationActionV1 {
                step_index: action.step_index,
                action_key: action.action_key.clone(),
                input: action.input.clone(),
                opportunity_before,
                drawn_cards: drawn_cards_from_action_result(outcome.action_result.as_ref()),
                combat_after: combat_automation_step_state_v1(session),
            })
        })
        .collect()
}

pub(super) fn drawn_cards_from_action_result(
    action_result: Option<&ActionResult>,
) -> Vec<CardSnapshot> {
    action_result
        .into_iter()
        .flat_map(|result| result.changes.iter())
        .filter_map(|change| match change {
            ActionResultChange::CombatCardDrawn { card } => Some(card.clone()),
            _ => None,
        })
        .collect()
}
