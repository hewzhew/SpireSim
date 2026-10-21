//! Equal-work exact continuation of one selected turn-boundary successor.
//!
//! This is deliberately a thin laboratory adapter over the existing local
//! turn graph. It does not own candidate selection, persistence, or policy
//! training.

use serde::Serialize;
use std::time::Instant;
use sts_combat_planner::{
    CombatDecisionRoot, LocalTurnGraphWitnessConfig, LocalTurnGraphWitnessQuantum,
    LocalTurnGraphWitnessSession, OracleCombatWitnessSatisfaction, SharedCombatActionPolicy,
    TurnOptionGeneratorConfig,
};
use sts_oracle_runtime::sim::combat::{CombatPosition, EngineCombatStepper};

#[derive(Clone, Copy, Debug)]
pub(super) struct ExactBoundaryFollowupConfig {
    pub(super) generation_work: usize,
    pub(super) max_engine_steps_per_transition: usize,
    pub(super) max_turn_depth: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct ExactBoundaryFollowupReport {
    pub(super) elapsed_ms: u128,
    pub(super) status: String,
    pub(super) witness_found: bool,
    pub(super) final_hp: Option<i32>,
    pub(super) witness_action_count: Option<usize>,
    pub(super) selections: usize,
    pub(super) generation_work: usize,
    pub(super) engine_steps: usize,
    pub(super) exact_nodes: usize,
    pub(super) completed_turn_options: usize,
    pub(super) terminal_win_options: usize,
    pub(super) maximum_turn_depth: usize,
    pub(super) generation_gap_count: usize,
}

pub(super) fn run_exact_boundary_followup(
    position: CombatPosition,
    policy: SharedCombatActionPolicy,
    controls: ExactBoundaryFollowupConfig,
) -> Result<ExactBoundaryFollowupReport, String> {
    let started = Instant::now();
    let root = CombatDecisionRoot::new(position)
        .map_err(|error| format!("invalid exact boundary followup root: {error:?}"))?;
    let defaults = LocalTurnGraphWitnessConfig::default();
    let config = LocalTurnGraphWitnessConfig {
        generator: TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: controls.max_engine_steps_per_transition,
            ..defaults.generator
        },
        max_turn_depth: controls.max_turn_depth,
        satisfaction: OracleCombatWitnessSatisfaction::FirstWitness,
        ..defaults
    };
    let mut session = LocalTurnGraphWitnessSession::with_policy(root, config, policy);
    let report = session.advance(
        LocalTurnGraphWitnessQuantum {
            additional_selections: controls.generation_work.saturating_mul(4),
            additional_generation_work: controls.generation_work,
            additional_engine_steps: controls
                .generation_work
                .saturating_mul(controls.max_engine_steps_per_transition),
            deadline: None,
        },
        &EngineCombatStepper,
    );
    let witness = report.witness.as_ref();
    Ok(ExactBoundaryFollowupReport {
        elapsed_ms: started.elapsed().as_millis(),
        status: format!("{:?}", report.status),
        witness_found: witness.is_some(),
        final_hp: witness.map(|witness| witness.final_position.combat.entities.player.current_hp),
        witness_action_count: witness.map(|witness| witness.actions.len()),
        selections: report.counters.selections,
        generation_work: report.counters.generation_work,
        engine_steps: report.counters.engine_steps,
        exact_nodes: report.counters.exact_nodes,
        completed_turn_options: report.counters.completed_turn_options,
        terminal_win_options: report.counters.terminal_win_options,
        maximum_turn_depth: report.counters.maximum_turn_depth,
        generation_gap_count: report.generation_gaps.len(),
    })
}
