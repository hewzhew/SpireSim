use std::time::{Duration, Instant};

use crate::ai::combat_search_v2::{
    filter_combat_search_legal_actions, find_combat_turn_plan_rescue_win_v0,
    find_combat_turn_pool_rescue_win_v0, plan_combat_turn_segment_v1, CombatSearchV2ActionTrace,
    CombatSearchV2Config, CombatSearchV2Report,
};
use crate::content::potions::PotionId;
use crate::sim::combat::{
    combat_terminal, CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal,
    EngineCombatStepper,
};
use crate::sim::combat_legal_actions::engine_atomic_actions;

use super::combat_candidate_line::{replay_candidate_line, CombatCandidateLineSource};
use super::combat_line_executor::{
    apply_combat_turn_segment, apply_selected_combat_candidate_line,
    apply_smoke_bomb_survival_fallback,
};
use super::combat_line_trace::{millis_to_micros_u64, CombatCandidateLinePerformance};
use super::progress_options::{AtomicCombatSearchOptionsV2, RunControlCombatSegmentMode};
use super::session::{RunControlSession, RunProgressOutcome};
use super::trace_annotation::CombatAutomationTrajectorySource;

pub(super) fn try_apply_no_win_fallback(
    session: &mut RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    options: &AtomicCombatSearchOptionsV2,
    search_report: &CombatSearchV2Report,
    hp_loss_limit: Option<u32>,
) -> Result<Option<RunProgressOutcome>, String> {
    if let Some(outcome) = try_apply_complete_line_solver_after_no_win(
        session,
        start,
        config,
        search_report,
        hp_loss_limit,
    )? {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = try_apply_turn_plan_rescue_after_no_win(
        session,
        start,
        config,
        search_report,
        hp_loss_limit,
    )? {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = try_apply_line_lab_turn_pool_after_no_win(
        session,
        start,
        config,
        search_report,
        hp_loss_limit,
    )? {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = try_apply_turn_segment_after_rejection(
        session,
        start,
        config,
        options,
        search_report,
        "no_complete_winning_candidate",
    )? {
        return Ok(Some(outcome));
    }
    try_apply_smoke_bomb_survival_fallback_after_rejection(session, "no_complete_winning_candidate")
}

fn try_apply_turn_plan_rescue_after_no_win(
    session: &mut RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    search_report: &CombatSearchV2Report,
    hp_loss_limit: Option<u32>,
) -> Result<Option<RunProgressOutcome>, String> {
    let budget_ms = turn_plan_rescue_budget_ms(config);
    let rescue_started = Instant::now();
    let Some(rescue) = find_combat_turn_plan_rescue_win_v0(start, config, budget_ms, hp_loss_limit)
    else {
        return Ok(None);
    };
    let rescue_elapsed = rescue_started.elapsed();
    let replay = replay_candidate_line(
        start,
        CombatCandidateLineSource::TurnPlanRescue,
        &rescue.actions,
        config,
    )?;
    if replay.line.terminal != CombatTerminal::Win {
        return Ok(None);
    }
    if hp_loss_limit.is_some_and(|limit| replay.line.hp_loss > limit as i32) {
        return Ok(None);
    }
    let summary = format!(
        "{} per_plan_budget_ms={budget_ms}",
        rescue.transition_summary()
    );
    apply_selected_combat_candidate_line(
        session,
        start,
        config,
        search_report,
        replay.line,
        CombatAutomationTrajectorySource::TurnPlanRescue,
        summary,
        Some(CombatCandidateLinePerformance {
            nodes_expanded: rescue.nodes_expanded,
            nodes_generated: rescue.nodes_generated,
            total_us: duration_to_micros_u64(rescue_elapsed),
        }),
    )
    .map(Some)
}

fn try_apply_line_lab_turn_pool_after_no_win(
    session: &mut RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    search_report: &CombatSearchV2Report,
    hp_loss_limit: Option<u32>,
) -> Result<Option<RunProgressOutcome>, String> {
    let budget_ms = turn_pool_rescue_budget_ms(config);
    let rescue_started = Instant::now();
    let Some(rescue) = find_combat_turn_pool_rescue_win_v0(start, config, budget_ms) else {
        return Ok(None);
    };
    let rescue_elapsed = rescue_started.elapsed();
    let replay = replay_candidate_line(
        start,
        CombatCandidateLineSource::TurnPoolRescue,
        &rescue.actions,
        config,
    )?;
    if replay.line.terminal != CombatTerminal::Win {
        return Ok(None);
    }
    if hp_loss_limit.is_some_and(|limit| replay.line.hp_loss > limit as i32) {
        return Ok(None);
    }
    let summary = format!("{} budget_ms={budget_ms}", rescue.transition_summary());
    apply_selected_combat_candidate_line(
        session,
        start,
        config,
        search_report,
        replay.line,
        CombatAutomationTrajectorySource::TurnPoolRescue,
        summary,
        Some(CombatCandidateLinePerformance {
            nodes_expanded: rescue.nodes_expanded,
            nodes_generated: rescue.nodes_generated,
            total_us: duration_to_micros_u64(rescue_elapsed),
        }),
    )
    .map(Some)
}

fn turn_pool_rescue_budget_ms(config: &CombatSearchV2Config) -> u64 {
    let configured = config
        .wall_time
        .unwrap_or_else(|| std::time::Duration::from_millis(2_000))
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    configured.clamp(2_000, 5_000)
}

fn turn_plan_rescue_budget_ms(config: &CombatSearchV2Config) -> u64 {
    let configured = config
        .wall_time
        .unwrap_or_else(|| std::time::Duration::from_millis(2_000))
        .as_millis()
        .min(u128::from(u64::MAX)) as u64;
    configured.clamp(2_000, 10_000)
}

fn try_apply_complete_line_solver_after_no_win(
    session: &mut RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    search_report: &CombatSearchV2Report,
    hp_loss_limit: Option<u32>,
) -> Result<Option<RunProgressOutcome>, String> {
    let Some(solution) = super::combat_complete_line_solver::try_solve_complete_line(start, config)
    else {
        return Ok(None);
    };
    if hp_loss_limit.is_some_and(|limit| solution.line.hp_loss > limit as i32) {
        return Ok(None);
    }
    let summary = solution.transition_summary();
    apply_selected_combat_candidate_line(
        session,
        start,
        config,
        search_report,
        solution.line,
        CombatAutomationTrajectorySource::CompleteLineSolver,
        summary,
        Some(CombatCandidateLinePerformance {
            nodes_expanded: solution.nodes_expanded as u64,
            nodes_generated: solution.nodes_generated as u64,
            total_us: millis_to_micros_u64(solution.elapsed_ms),
        }),
    )
    .map(Some)
}

pub(super) fn try_apply_turn_segment_after_rejection(
    session: &mut RunControlSession,
    start: &CombatPosition,
    config: &CombatSearchV2Config,
    options: &AtomicCombatSearchOptionsV2,
    search_report: &CombatSearchV2Report,
    rejection_result: &'static str,
) -> Result<Option<RunProgressOutcome>, String> {
    if !segment_mode_allows_turn_segment(options.segment_mode, start) {
        return Ok(None);
    }

    let segment_report = plan_combat_turn_segment_v1(&start.engine, &start.combat, config);
    let Some(trajectory) = segment_report.selected.as_ref() else {
        return Ok(None);
    };
    verify_segment_trajectory_replays(start, &trajectory.actions, config)?;
    apply_combat_turn_segment(
        session,
        start,
        search_report,
        &segment_report,
        rejection_result,
    )
    .map(Some)
}

pub(super) fn try_apply_smoke_bomb_survival_fallback_after_rejection(
    session: &mut RunControlSession,
    rejection_result: &'static str,
) -> Result<Option<RunProgressOutcome>, String> {
    let Some(active) = session.active_combat.as_ref() else {
        return Ok(None);
    };
    if !matches!(
        active.engine_state,
        crate::state::core::EngineState::CombatPlayerTurn
    ) {
        // Potions are not directly usable while a PendingChoice owns input.
        // Avoid asking the legacy eager candidate surface to enumerate an
        // unrelated combinatorial selection merely to look for Smoke Bomb.
        return Ok(None);
    }
    let smoke_input = engine_atomic_actions(&active.engine_state, &active.combat_state)
        .into_iter()
        .find(|input| match input {
            crate::state::core::ClientInput::UsePotion { potion_index, .. } => active
                .combat_state
                .entities
                .potions
                .get(*potion_index)
                .and_then(|potion| potion.as_ref())
                .is_some_and(|potion| potion.id == PotionId::SmokeBomb),
            _ => false,
        });
    let Some(smoke_input) = smoke_input else {
        return Ok(None);
    };
    apply_smoke_bomb_survival_fallback(session, smoke_input, rejection_result).map(Some)
}

pub(super) fn segment_mode_allows_turn_segment(
    mode: Option<RunControlCombatSegmentMode>,
    start: &CombatPosition,
) -> bool {
    match mode {
        Some(RunControlCombatSegmentMode::TurnBoundary) => true,
        Some(RunControlCombatSegmentMode::NonBossTurnBoundary) => !start.combat.meta.is_boss_fight,
        None => false,
    }
}

fn duration_to_micros_u64(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

fn verify_segment_trajectory_replays(
    start: &CombatPosition,
    actions: &[CombatSearchV2ActionTrace],
    config: &CombatSearchV2Config,
) -> Result<(), String> {
    if actions.is_empty() {
        return Err("search-combat segment dry-run refused empty action list".to_string());
    }
    let stepper = EngineCombatStepper;
    let mut position = start.clone();
    for action in actions {
        let Some(candidate) = stepper.choice_for_legal_input(&position, &action.input) else {
            return Err(format!(
                "search-combat segment dry-run drift at step {}: expected {} ({})",
                action.step_index,
                action.action_key,
                super::view_model::client_input_hint(&action.input)
            ));
        };
        let choices = filter_combat_search_legal_actions(
            vec![candidate],
            config.potion_policy,
            config.allowed_potion_slots,
            &position.combat,
        );
        let Some(choice) = choices
            .into_iter()
            .find(|choice| choice.input == action.input && choice.action_key == action.action_key)
        else {
            return Err(format!(
                "search-combat segment dry-run drift at step {}: expected {} ({})",
                action.step_index,
                action.action_key,
                super::view_model::client_input_hint(&action.input)
            ));
        };
        let step = stepper.apply_to_stable(
            &position,
            choice.input.clone(),
            CombatStepLimits {
                max_engine_steps: config.max_engine_steps_per_action,
                deadline: None,
            },
        );
        if step.truncated {
            return Err(format!(
                "search-combat segment dry-run truncated at step {} after {} engine steps",
                action.step_index, step.engine_steps
            ));
        }
        position = step.position;
    }
    match combat_terminal(&position.engine, &position.combat) {
        CombatTerminal::Loss => Err("search-combat segment dry-run ended in loss".to_string()),
        CombatTerminal::Win | CombatTerminal::Unresolved => Ok(()),
    }
}
