use std::time::{Duration, Instant};

use crate::sim::combat::{CombatStepper, CombatTerminal, EngineCombatStepper};

use super::super::{
    OracleAnalysisSessionV1, OracleCombatWitnessAdvanceV1, OracleCombatWitnessQuantumV1,
    OracleResidentCombatWitnessJobV1,
};
use super::view::exact_hash;
use super::{
    OracleAnalysisCombatScratchSearchExitV1, OracleAnalysisCombatScratchSearchReportV1,
    OracleAnalysisCombatScratchSearchRequestV1, OracleAnalysisCombatScratchViewV1,
};

impl OracleAnalysisSessionV1 {
    pub fn search_combat_scratch(
        &mut self,
        request: OracleAnalysisCombatScratchSearchRequestV1,
        selection_offset: usize,
        selection_limit: usize,
    ) -> Result<
        (
            OracleAnalysisCombatScratchSearchReportV1,
            OracleAnalysisCombatScratchViewV1,
        ),
        String,
    > {
        let report = self.search_combat_line_lab(request)?;
        let view = self.combat_scratch_view(selection_offset, selection_limit)?;
        Ok((report, view))
    }

    pub fn search_combat_line_lab(
        &mut self,
        request: OracleAnalysisCombatScratchSearchRequestV1,
    ) -> Result<OracleAnalysisCombatScratchSearchReportV1, String> {
        if request.max_quanta == 0
            || request.quantum_generation_work == 0
            || request.quantum_ms == 0
            || request.wall_ms == 0
        {
            return Err(
                "combat scratch search max_quanta, quantum_generation_work, quantum_ms, and wall_ms must be positive"
                    .to_string(),
            );
        }
        let (run_node_id, source_scratch_node_id, position) = {
            let scratch = self.combat_scratch.as_ref().ok_or_else(|| {
                "oracle analysis workspace has no active combat scratch".to_string()
            })?;
            (
                scratch.run_node_id,
                scratch.cursor_scratch_node_id,
                scratch.current_position()?.clone(),
            )
        };
        if EngineCombatStepper.terminal(&position) != CombatTerminal::Unresolved {
            return Err("combat scratch search requires an unresolved cursor".to_string());
        }

        let mut trial = self.require_branch(run_node_id)?.session.clone();
        trial.engine_state = position.engine.clone();
        let active = trial.active_combat.as_mut().ok_or_else(|| {
            format!("oracle analysis node {run_node_id} has no active combat state")
        })?;
        active.engine_state = position.engine.clone();
        active.combat_state = position.combat.clone();

        let total_work = request
            .max_quanta
            .saturating_mul(request.quantum_generation_work);
        let mut options = self.combat_budgets.for_session(&trial);
        options.max_generation_work = Some(total_work);
        options.wall_ms = Some(request.wall_ms);
        options.satisfaction =
            Some(crate::ai::combat_witness_contract::CombatWitnessSatisfactionV1::FirstCompleteWin);
        options.potion_policy =
            Some(crate::ai::combat_witness_contract::CombatWitnessPotionPolicyV1::Never);
        options.max_potions_used = Some(0);
        options.allowed_potion_slots = Some(0);
        let mut work = OracleResidentCombatWitnessJobV1::new(
            &trial,
            options,
            self.combat_budgets.guidance_bundle.as_deref(),
        )?;
        let started = Instant::now();
        let deadline = started.checked_add(Duration::from_millis(request.wall_ms));
        let quantum = OracleCombatWitnessQuantumV1 {
            label: "combat-scratch-descendant-search",
            additional_generation_work: request.quantum_generation_work,
            soft_wall_ms: Some(request.quantum_ms),
        };
        let mut quanta_served = 0usize;
        let mut last_advance = None;
        while !work.has_verified_witness() && quanta_served < request.max_quanta {
            let advance = work.advance(&quantum, deadline);
            quanta_served = quanta_served.saturating_add(1);
            let stop = !matches!(advance, OracleCombatWitnessAdvanceV1::Pending);
            last_advance = Some(advance);
            if stop {
                break;
            }
        }

        let witness_inputs = work.verified_witness_inputs();
        let progress = work.evidence();
        let (exit, appended_action_count, first_appended, terminal_node) =
            if let Some(inputs) = witness_inputs {
                let appended_action_count = inputs.len();
                let (first, terminal) = self
                    .combat_scratch
                    .as_mut()
                    .expect("scratch remained active during in-memory search")
                    .append_inputs_atomically(&inputs)?;
                (
                    OracleAnalysisCombatScratchSearchExitV1::WitnessAdded,
                    appended_action_count,
                    first,
                    terminal,
                )
            } else {
                let exit = match last_advance {
                    Some(OracleCombatWitnessAdvanceV1::ReadyToFinish) => {
                        OracleAnalysisCombatScratchSearchExitV1::PortfolioCompleteWithoutWitness
                    }
                    Some(OracleCombatWitnessAdvanceV1::AllowanceExhausted) => {
                        OracleAnalysisCombatScratchSearchExitV1::AllowanceExhausted
                    }
                    Some(OracleCombatWitnessAdvanceV1::GlobalDeadlineReached) => {
                        OracleAnalysisCombatScratchSearchExitV1::DeadlineReached
                    }
                    Some(OracleCombatWitnessAdvanceV1::Pending) | None => {
                        OracleAnalysisCombatScratchSearchExitV1::QuantumLimitReached
                    }
                };
                (exit, 0, None, None)
            };
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let report = OracleAnalysisCombatScratchSearchReportV1 {
            run_node_id,
            source_scratch_node_id,
            search_root_exact_state_hash: exact_hash(&position),
            exit,
            quanta_served,
            elapsed_ms,
            generation_work: progress.generation_work,
            exact_states: progress.exact_states,
            completed_turn_options: progress.completed_turn_options,
            max_player_turn: progress.max_player_turn,
            last_status: progress.last_status,
            additional_potions_allowed: 0,
            appended_action_count,
            first_appended_scratch_node_id: first_appended,
            terminal_scratch_node_id: terminal_node,
        };
        Ok(report)
    }
}
