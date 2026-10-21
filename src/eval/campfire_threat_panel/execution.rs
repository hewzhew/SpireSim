use serde::{Deserialize, Serialize};

use crate::ai::combat_search_v2::{
    run_combat_search_v2, SearchCoverageStatus, SearchTerminalLabel,
};
use crate::eval::campfire_evaluation::CampfireEvaluationBatch;
use crate::eval::campfire_survival_scenarios::{
    compile_aligned_campfire_survival_sample, CampfireSurvivalLens, CampfireSurvivalScenarioCell,
    CampfireSurvivalScenarioGapRecord, CampfireSurvivalScenarioSpec, CampfireSurvivalSubject,
};
use crate::eval::combat_lab_v1::{
    classify_combat_lab_outcome_v1, exact_replay_combat_search_trajectory_v1, profile_config_v1,
    CombatLabOutcomeClassV1, CombatLabReplayedCandidateV1,
};
use crate::eval::fingerprint::{hash_serializable, StateFingerprintV3};
use crate::state::run::RunState;

use super::{
    CampfireThreatEncounterV1, ResolvedCampfireThreatPanelSpecV1,
    CAMPFIRE_THREAT_PANEL_CELL_SCHEMA_VERSION,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampfireThreatPanelCellV1 {
    pub schema_version: u32,
    pub cell_key: String,
    pub contract_hash: String,
    pub context_fingerprint: String,
    pub subject: CampfireSurvivalSubject,
    pub lens: CampfireSurvivalLens,
    pub encounter: CampfireThreatEncounterV1,
    pub sample_index: u64,
    pub analysis_seed: u64,
    pub shuffle_seed: u64,
    pub profile_id: String,
    pub state_fingerprint: StateFingerprintV3,
    pub start_hp: i32,
    pub search_terminal: Option<SearchTerminalLabel>,
    pub coverage_status: SearchCoverageStatus,
    pub outcome_class: CombatLabOutcomeClassV1,
    pub replay_validated: bool,
    pub replayed_candidate: Option<CombatLabReplayedCandidateV1>,
    #[serde(default)]
    pub execution_reuse: Option<CampfireThreatPanelExecutionReuseV1>,
    pub expanded_nodes: u64,
    pub generated_nodes: u64,
    pub nodes_to_first_win: Option<u64>,
    pub node_budget_exhausted: bool,
    pub deadline_exhausted: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CampfireThreatPanelExecutionReuseV1 {
    IdenticalExactState { source_cell_key: String },
}

#[derive(Clone, Debug)]
pub struct CampfireThreatPanelCompiledSampleV1 {
    pub sample_index: u64,
    pub cells: Vec<(CampfireThreatEncounterV1, CampfireSurvivalScenarioCell)>,
    pub gaps: Vec<CampfireSurvivalScenarioGapRecord>,
}

pub fn compile_campfire_threat_panel_sample_v1(
    root: &RunState,
    evaluation: &CampfireEvaluationBatch,
    resolved: &ResolvedCampfireThreatPanelSpecV1,
    sample_index: u64,
) -> Result<CampfireThreatPanelCompiledSampleV1, String> {
    let mut cells = Vec::new();
    let mut gaps = Vec::new();
    for (encounter_index, encounter) in resolved.encounters.iter().enumerate() {
        let analysis_seed = derive_threat_analysis_seed_v1(
            resolved.spec.analysis_seed,
            encounter_index as u64,
            sample_index,
        );
        let sample = compile_aligned_campfire_survival_sample(
            root,
            evaluation,
            CampfireSurvivalScenarioSpec {
                encounter_id: encounter.encounter_id,
                room_type: encounter.room_type,
                analysis_seed,
                schedule: resolved.spec.schedule.clone(),
                sample_index,
                lenses: resolved.spec.lenses.clone(),
                include_unchanged_root: resolved.spec.include_unchanged_root,
            },
        )
        .map_err(|error| {
            format!(
                "failed to compile Campfire threat {:?} sample {sample_index}: {error:?}",
                encounter.encounter_id
            )
        })?;
        if gaps.is_empty() {
            gaps = sample.gaps.clone();
        } else if gaps != sample.gaps {
            return Err("Campfire threat candidate gaps changed across encounters".to_string());
        }
        cells.extend(
            sample
                .cells
                .into_iter()
                .map(|cell| (encounter.clone(), cell)),
        );
    }
    Ok(CampfireThreatPanelCompiledSampleV1 {
        sample_index,
        cells,
        gaps,
    })
}

pub fn execute_campfire_threat_panel_cell_v1(
    resolved: &ResolvedCampfireThreatPanelSpecV1,
    context_fingerprint: &str,
    encounter: &CampfireThreatEncounterV1,
    sample_index: u64,
    cell: &CampfireSurvivalScenarioCell,
) -> CampfireThreatPanelCellV1 {
    let config = profile_config_v1(
        &resolved.spec.experiment_id,
        &resolved.spec.profile,
        &resolved.spec.common_budget,
    );
    let report = run_combat_search_v2(&cell.start.engine, &cell.start.combat, config);
    let selected = report.best_complete_trajectory.as_ref();
    let search_terminal = selected.map(|trajectory| trajectory.terminal);
    let replay = selected.map(|trajectory| {
        exact_replay_combat_search_trajectory_v1(
            &cell.start,
            trajectory,
            resolved.spec.common_budget.max_engine_steps_per_action,
        )
    });
    let (replayed_candidate, error) = match replay {
        Some(Ok(candidate)) => (Some(candidate), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    let outcome_class = classify_combat_lab_outcome_v1(
        report.outcome.coverage_status,
        search_terminal,
        error.is_some(),
    );
    let cell_key = campfire_threat_panel_cell_key_v1(
        &resolved.contract_hash,
        context_fingerprint,
        cell.subject,
        cell.lens,
        encounter,
        sample_index,
        cell.analysis_seed,
        cell.shuffle_seed,
        &resolved.spec.profile.id,
    );

    CampfireThreatPanelCellV1 {
        schema_version: CAMPFIRE_THREAT_PANEL_CELL_SCHEMA_VERSION,
        cell_key,
        contract_hash: resolved.contract_hash.clone(),
        context_fingerprint: context_fingerprint.to_string(),
        subject: cell.subject,
        lens: cell.lens,
        encounter: encounter.clone(),
        sample_index,
        analysis_seed: cell.analysis_seed,
        shuffle_seed: cell.shuffle_seed,
        profile_id: resolved.spec.profile.id.clone(),
        state_fingerprint: cell.state_fingerprint.clone(),
        start_hp: cell.start.combat.entities.player.current_hp,
        search_terminal,
        coverage_status: report.outcome.coverage_status,
        outcome_class,
        replay_validated: replayed_candidate.is_some(),
        replayed_candidate,
        execution_reuse: None,
        expanded_nodes: report.stats.nodes_expanded,
        generated_nodes: report.stats.nodes_generated,
        nodes_to_first_win: report.stats.nodes_to_first_win,
        node_budget_exhausted: report.stats.node_budget_hit,
        deadline_exhausted: report.stats.deadline_hit,
        error,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn campfire_threat_panel_cell_key_v1(
    contract_hash: &str,
    context_fingerprint: &str,
    subject: CampfireSurvivalSubject,
    lens: CampfireSurvivalLens,
    encounter: &CampfireThreatEncounterV1,
    sample_index: u64,
    analysis_seed: u64,
    shuffle_seed: u64,
    profile_id: &str,
) -> String {
    #[derive(Serialize)]
    struct CellKeyInput<'a> {
        contract_hash: &'a str,
        context_fingerprint: &'a str,
        subject: CampfireSurvivalSubject,
        lens: CampfireSurvivalLens,
        encounter: &'a CampfireThreatEncounterV1,
        sample_index: u64,
        analysis_seed: u64,
        shuffle_seed: u64,
        profile_id: &'a str,
    }
    format!(
        "campfire_threat_panel_cell_v1:{}",
        hash_serializable(&CellKeyInput {
            contract_hash,
            context_fingerprint,
            subject,
            lens,
            encounter,
            sample_index,
            analysis_seed,
            shuffle_seed,
            profile_id,
        })
    )
}

pub fn derive_threat_analysis_seed_v1(base: u64, encounter_index: u64, sample_index: u64) -> u64 {
    const GOLDEN_GAMMA: u64 = 0x9E3779B97F4A7C15;
    let mut state = base
        .wrapping_add(GOLDEN_GAMMA.wrapping_mul(encounter_index.wrapping_add(1)))
        .wrapping_add(sample_index.rotate_left(32));
    state = (state ^ (state >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    state = (state ^ (state >> 27)).wrapping_mul(0x94D049BB133111EB);
    state ^ (state >> 31)
}
