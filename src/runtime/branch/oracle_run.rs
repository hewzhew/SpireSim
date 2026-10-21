use std::fs;
use std::path::Path;
use std::time::{Instant, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ai::combat_witness_contract::CombatWitnessSatisfactionV1;
use crate::content::potions::Potion;
use crate::content::relics::RelicState;
use crate::eval::combat_case::{
    CombatCase, CombatCaseGap, CombatCaseRngSummary, CombatCaseRunSummary, CombatCaseSource,
    CombatCaseWitnessBudgetV1,
};
use crate::eval::combat_case_context::capture_oracle_analysis_combat_case_production_context_v2;
use crate::eval::run_control::{
    drive_oracle_run_explorer_v1, expand_oracle_neow_candidates_v1,
    seed_oracle_run_explorer_from_checkpoint_v1, seed_oracle_run_explorer_from_session_v1,
    seed_oracle_run_explorer_v1, uniform_run_policy_prior_v1, CombatAutomationActionV1,
    DecisionCandidateKey, NeowOracleExpansionV1, OracleCombatWitnessOptionsV1,
    OraclePendingCombatSummaryV1, OracleRunBranchV1, OracleRunCombatWitnessBudgetsV1,
    OracleRunCombatWitnessQualityPolicyV1, OracleRunExploreBudgetV1, OracleRunExploreResultV1,
    OracleRunExploreStopV1, OracleRunExplorerCheckpointV1, RewardAutomationConfig,
    RunControlConfig, RunControlSession, RunControlSessionCheckpointV1, RunDecisionAction,
    RunPolicyPriorIdentityV1, RunProgressJournalV1, RunProgressStepV1,
    StrategicProbeOwnerAuthorityV1, StrategicProbeSchedulingAuthorityV1,
};
use crate::runtime::combat::CombatCard;
use crate::sim::combat::CombatPosition;
use crate::state::core::EngineState;

pub const ORACLE_RUN_REPORT_SCHEMA_NAME: &str = "OracleRunReport";
pub const ORACLE_RUN_REPORT_SCHEMA_VERSION: u32 = 14;
pub const ORACLE_RUN_CONTINUATION_SCHEMA_NAME: &str = "OracleRunContinuation";
pub const ORACLE_RUN_CONTINUATION_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunBudget {
    pub max_work_items: usize,
    pub wall_ms: Option<u64>,
    pub hallway_generation_work: usize,
    pub hallway_ms: u64,
    pub elite_generation_work: usize,
    pub elite_ms: u64,
    pub boss_generation_work: usize,
    pub boss_ms: u64,
    pub combat_quantum_generation_work: usize,
    pub combat_quantum_ms: u64,
    #[serde(default = "default_combat_initial_divisor")]
    pub combat_initial_divisor: u32,
}

impl Default for OracleRunBudget {
    fn default() -> Self {
        Self {
            max_work_items: 2_048,
            wall_ms: None,
            hallway_generation_work: 250_000,
            hallway_ms: 5_000,
            elite_generation_work: 750_000,
            elite_ms: 15_000,
            boss_generation_work: 2_000_000,
            boss_ms: 30_000,
            combat_quantum_generation_work: 50_000,
            combat_quantum_ms: 1_000,
            combat_initial_divisor: 1,
        }
    }
}

const fn default_combat_initial_divisor() -> u32 {
    1
}

#[derive(Clone, Debug)]
pub struct OracleRunConfig {
    pub seed: u64,
    pub ascension: u8,
    pub budget: OracleRunBudget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunContinuationV1 {
    pub schema_name: String,
    pub schema_version: u32,
    /// `None` is accepted only while decoding a legacy artifact so callers can
    /// report an unsupported schema instead of a missing-field parse error.
    #[serde(default)]
    pub decision_prior: Option<RunPolicyPriorIdentityV1>,
    pub seed: u64,
    pub ascension: u8,
    pub journal: RunProgressJournalV1,
    pub session: RunControlSessionCheckpointV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explorer_frontier: Option<OracleRunExplorerCheckpointV1>,
}

impl OracleRunContinuationV1 {
    pub fn validate_supported_semantics(&self) -> Result<(), String> {
        if self.schema_name != ORACLE_RUN_CONTINUATION_SCHEMA_NAME
            || self.schema_version != ORACLE_RUN_CONTINUATION_SCHEMA_VERSION
        {
            return Err(format!(
                "unsupported oracle continuation {} version {}",
                self.schema_name, self.schema_version
            ));
        }
        if self.decision_prior != Some(RunPolicyPriorIdentityV1::UniformExactSurfaceV1) {
            return Err("unsupported oracle continuation decision prior".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleDecisionSummaryV1 {
    pub decision_step: u64,
    pub title: String,
    pub location: String,
    pub source: String,
    pub selected_label: String,
    pub candidate_labels: Vec<String>,
    pub action: RunDecisionAction,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleCombatResolutionSummaryV1 {
    pub combat_sequence: u64,
    pub title: String,
    pub location: String,
    pub kind: String,
    pub source: String,
    pub action_count: usize,
    pub final_hp: Option<i32>,
    pub actions: Vec<CombatAutomationActionV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleNeowReplayStepSummaryV1 {
    pub candidate_id: String,
    pub label: String,
    pub action: RunDecisionAction,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleCompletedNeowCandidateSummaryV1 {
    pub root_candidate_id: String,
    pub root_candidate_key: DecisionCandidateKey,
    pub root_label: String,
    pub replay: Vec<OracleNeowReplayStepSummaryV1>,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub deck: Vec<CombatCard>,
    pub relics: Vec<RelicState>,
    pub potions: Vec<Option<Potion>>,
    pub engine_state: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleUnresolvedNeowCandidateSummaryV1 {
    pub root_candidate_id: String,
    pub root_label: String,
    pub replay: Vec<OracleNeowReplayStepSummaryV1>,
    pub boundary: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleNeowFrontierSummaryV1 {
    pub completed: Vec<OracleCompletedNeowCandidateSummaryV1>,
    pub unresolved: Vec<OracleUnresolvedNeowCandidateSummaryV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleUnresolvedCombatSummaryV1 {
    pub branch_id: usize,
    pub witness_failure: String,
    pub evidence_kind: String,
    pub last_status: Option<String>,
    pub generation_work: u64,
    pub exact_states: usize,
    pub applied_action_transitions: usize,
    pub unique_successor_states: usize,
    pub duplicate_exact_successors: usize,
    pub completed_turn_options: usize,
    pub retained_state_work: usize,
    pub max_player_turn: u32,
    pub max_path_atomic_depth: usize,
    pub generation_gap_count: usize,
    pub incumbent_final_hp: Option<i32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunExplorerSummaryV1 {
    pub stop: OracleRunExploreStopV1,
    pub work_items: usize,
    pub combat_quanta: usize,
    pub decision_service_ms: u64,
    pub combat_service_ms: u64,
    pub combat_edge_probe_evaluations: usize,
    pub immediate_combat_edge_hints: usize,
    pub combat_edge_probe_authority: StrategicProbeSchedulingAuthorityV1,
    pub probe_owner_authority: StrategicProbeOwnerAuthorityV1,
    pub elapsed_ms: u64,
    pub materialized_branches: usize,
    pub pending_decisions: usize,
    pub pending_decision_discrepancy_counts: std::collections::BTreeMap<u64, usize>,
    pub pending_combats: Vec<OraclePendingCombatSummaryV1>,
    pub deferred_combats: usize,
    pub deferred_combat_effective_discrepancy_counts: std::collections::BTreeMap<u64, usize>,
    pub combat_search_restarts: usize,
    pub neow_root_progress: Vec<OracleNeowRootProgressSummaryV1>,
    pub exact_duplicates: usize,
    pub unresolved_combats: Vec<OracleUnresolvedCombatSummaryV1>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunBuildIdentityV1 {
    pub source_identity: super::SourceIdentity,
    pub cargo_profile: String,
    pub executable_len: Option<u64>,
    pub executable_modified_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleNeowRootProgressSummaryV1 {
    pub root_candidate_id: String,
    pub root_label: String,
    pub materialized_branches: usize,
    pub furthest_act: u8,
    pub furthest_floor: i32,
    pub furthest_branch_id: usize,
    pub pending_decisions: usize,
    pub pending_combats: usize,
    pub deferred_combats: usize,
    pub unresolved_combats: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunReportV1 {
    pub schema_name: String,
    pub schema_version: u32,
    pub seed: u64,
    pub ascension: u8,
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub keys: [bool; 3],
    pub deck: Vec<CombatCard>,
    pub relics: Vec<RelicState>,
    pub potions: Vec<Option<Potion>>,
    pub engine_state: String,
    pub elapsed_ms: u64,
    pub build_identity: OracleRunBuildIdentityV1,
    pub initial_neow_frontier: OracleNeowFrontierSummaryV1,
    pub explorer: OracleRunExplorerSummaryV1,
    pub committed_progress_steps: usize,
    pub decisions: Vec<OracleDecisionSummaryV1>,
    pub combat_resolutions: Vec<OracleCombatResolutionSummaryV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_unresolved_combat_case: Option<CombatCase>,
    /// Exact active combat at the selected bounded frontier. Kept out of the
    /// report payload; the CLI may persist it as a standalone diagnostic case.
    #[serde(skip_serializing)]
    pub selected_active_combat_case: Option<CombatCase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub victory_witness: Option<RunProgressJournalV1>,
    /// Mechanical exact-state continuation, intentionally omitted from the
    /// human report. The CLI can persist it separately on request.
    #[serde(skip_serializing)]
    pub continuation: OracleRunContinuationV1,
}

impl OracleRunReportV1 {
    pub fn victory(&self) -> bool {
        matches!(self.explorer.stop, OracleRunExploreStopV1::Victory { .. })
    }
}

pub fn run_oracle_run(config: OracleRunConfig) -> Result<OracleRunReportV1, String> {
    validate_config(&config)?;
    let started = Instant::now();
    let session = RunControlSession::new(RunControlConfig {
        seed: config.seed,
        ascension_level: config.ascension,
        final_act: false,
        reward_automation: oracle_reward_automation_config(),
        ..RunControlConfig::default()
    });
    let neow_expansion = expand_oracle_neow_candidates_v1(&session)
        .map_err(|error| format!("failed to materialize initial Neow frontier: {error}"))?;
    let initial_neow_frontier = oracle_neow_frontier_summary(neow_expansion.clone());
    let explorer = seed_oracle_run_explorer_v1(neow_expansion, Some(uniform_run_policy_prior_v1))
        .map_err(|error| format!("failed to seed oracle run explorer: {error}"))?;
    let explored = drive_oracle_run_explorer_v1(
        explorer,
        OracleRunExploreBudgetV1 {
            max_work_items: config.budget.max_work_items,
            wall_ms: config.budget.wall_ms,
            combat: oracle_run_combat_budgets_v1(&config),
            combat_quantum_generation_work: config.budget.combat_quantum_generation_work,
            combat_quantum_ms: Some(config.budget.combat_quantum_ms),
            decision_prior: Some(uniform_run_policy_prior_v1),
            decision_annotation: None,
            // The current endpoint lexicographic key has not been calibrated
            // against disjoint expert-reviewed/exact cases.  Keep the narrow
            // edge-order hook available for experiments, but do not let that
            // provisional key steer production oracle runs.
            combat_edge_order: None,
        },
    )
    .map_err(|error| format!("oracle run explorer failed: {error}"))?;

    finish_oracle_run_report(&config, &started, initial_neow_frontier, explored)
}

pub(super) fn oracle_reward_automation_config() -> RewardAutomationConfig {
    RewardAutomationConfig {
        claim_gold: true,
        claim_potion_with_empty_slot: true,
        claim_safe_relic_without_sapphire_key: true,
    }
}

pub fn run_oracle_run_from_continuation(
    config: OracleRunConfig,
    continuation: OracleRunContinuationV1,
) -> Result<OracleRunReportV1, String> {
    validate_config(&config)?;
    validate_continuation(&config, &continuation)?;
    let started = Instant::now();
    let combat_budgets = oracle_run_combat_budgets_v1(&config);
    let explorer = if let Some(frontier) = continuation.explorer_frontier {
        seed_oracle_run_explorer_from_checkpoint_v1(frontier, &combat_budgets)?
    } else {
        seed_oracle_run_explorer_from_session_v1(
            continuation.session.into_session()?,
            continuation.journal,
            &combat_budgets,
            Some(uniform_run_policy_prior_v1),
        )?
    };
    let explored = drive_oracle_run_explorer_v1(
        explorer,
        OracleRunExploreBudgetV1 {
            max_work_items: config.budget.max_work_items,
            wall_ms: config.budget.wall_ms,
            combat: combat_budgets,
            combat_quantum_generation_work: config.budget.combat_quantum_generation_work,
            combat_quantum_ms: Some(config.budget.combat_quantum_ms),
            decision_prior: Some(uniform_run_policy_prior_v1),
            decision_annotation: None,
            combat_edge_order: None,
        },
    )
    .map_err(|error| format!("continued oracle run explorer failed: {error}"))?;
    finish_oracle_run_report(
        &config,
        &started,
        OracleNeowFrontierSummaryV1 {
            completed: Vec::new(),
            unresolved: Vec::new(),
        },
        explored,
    )
}

pub fn save_oracle_run_continuation_v1(
    path: &Path,
    continuation: &OracleRunContinuationV1,
) -> Result<(), String> {
    continuation.validate_supported_semantics()?;
    let bytes = serde_json::to_vec(continuation)
        .map_err(|error| format!("failed to serialize oracle continuation: {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("failed to write {}: {error}", path.display()))
}

pub fn load_oracle_run_continuation_v1(path: &Path) -> Result<OracleRunContinuationV1, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let continuation: OracleRunContinuationV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    continuation.validate_supported_semantics()?;
    Ok(continuation)
}

fn finish_oracle_run_report(
    config: &OracleRunConfig,
    started: &Instant,
    initial_neow_frontier: OracleNeowFrontierSummaryV1,
    explored: OracleRunExploreResultV1,
) -> Result<OracleRunReportV1, String> {
    let explorer_frontier = if matches!(explored.stop, OracleRunExploreStopV1::Victory { .. }) {
        None
    } else {
        explored.explorer.frontier_checkpoint()?
    };
    let first_unresolved_combat_case = first_unresolved_combat_case(config, &explored)?;
    let selected = explored
        .witness()
        .or_else(|| explored.furthest_branch())
        .ok_or_else(|| "oracle run produced no materialized branch".to_string())?;
    let pending_combats = explored.explorer.pending_combat_summaries()?;
    let selected_active_combat_case = if let Some(pending) = pending_combats.first() {
        match explored
            .explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == pending.branch_id)
        {
            Some(branch) => combat_case_for_branch(
                config,
                branch,
                "bounded_run_stopped_with_active_combat".to_string(),
                pending.generation_work.min(usize::MAX as u64) as usize,
            )?,
            None => None,
        }
    } else {
        None
    };
    let journal = selected.journal.clone();
    let mut continuation_session = RunControlSessionCheckpointV1::from_session(&selected.session);
    continuation_session.clear_combat_diagnostics_for_external_checkpoint();
    let continuation = OracleRunContinuationV1 {
        schema_name: ORACLE_RUN_CONTINUATION_SCHEMA_NAME.to_string(),
        schema_version: ORACLE_RUN_CONTINUATION_SCHEMA_VERSION,
        decision_prior: Some(RunPolicyPriorIdentityV1::UniformExactSurfaceV1),
        seed: config.seed,
        ascension: config.ascension,
        journal: journal.clone(),
        session: continuation_session,
        explorer_frontier,
    };
    let victory_witness =
        matches!(explored.stop, OracleRunExploreStopV1::Victory { .. }).then_some(journal.clone());
    let deferred_combat_branch_ids = explored.explorer.deferred_combat_branch_ids();
    let neow_root_progress =
        oracle_neow_root_progress(&explored, &pending_combats, &deferred_combat_branch_ids);
    let explorer_summary = OracleRunExplorerSummaryV1 {
        stop: explored.stop,
        work_items: explored.work_items,
        combat_quanta: explored.combat_quanta,
        decision_service_ms: explored.decision_service_ms,
        combat_service_ms: explored.combat_service_ms,
        combat_edge_probe_evaluations: explored.combat_edge_probe_evaluations,
        immediate_combat_edge_hints: explored.immediate_combat_edge_hints,
        combat_edge_probe_authority:
            StrategicProbeSchedulingAuthorityV1::WithheldPendingHeldOutCalibration,
        probe_owner_authority: StrategicProbeOwnerAuthorityV1::NotGranted,
        elapsed_ms: explored.elapsed_ms,
        materialized_branches: explored.explorer.branches.len(),
        pending_decisions: explored.explorer.pending_decisions.len(),
        pending_decision_discrepancy_counts: explored
            .explorer
            .pending_decision_discrepancy_counts(),
        pending_combats,
        deferred_combats: explored.explorer.deferred_combat_count(),
        deferred_combat_effective_discrepancy_counts: explored
            .explorer
            .deferred_combat_effective_discrepancy_counts(),
        combat_search_restarts: explored.explorer.combat_search_restarts,
        neow_root_progress,
        exact_duplicates: explored.explorer.retired_exact_duplicates.len(),
        unresolved_combats: explored
            .explorer
            .unresolved_combats
            .iter()
            .map(|combat| OracleUnresolvedCombatSummaryV1 {
                branch_id: combat.branch_id,
                witness_failure: format!("{:?}", combat.failure),
                evidence_kind: combat.evidence_kind.as_str().to_string(),
                last_status: combat.last_status.clone(),
                generation_work: combat.generation_work,
                exact_states: combat.exact_states,
                applied_action_transitions: combat.applied_action_transitions,
                unique_successor_states: combat.unique_successor_states,
                duplicate_exact_successors: combat.duplicate_exact_successors,
                completed_turn_options: combat.completed_turn_options,
                retained_state_work: combat.retained_state_work,
                max_player_turn: combat.max_player_turn,
                max_path_atomic_depth: combat.max_path_atomic_depth,
                generation_gap_count: combat.generation_gap_count,
                incumbent_final_hp: combat.incumbent_final_hp,
            })
            .collect(),
    };

    Ok(OracleRunReportV1 {
        schema_name: ORACLE_RUN_REPORT_SCHEMA_NAME.to_string(),
        schema_version: ORACLE_RUN_REPORT_SCHEMA_VERSION,
        seed: config.seed,
        ascension: config.ascension,
        act: selected.session.run_state.act_num,
        floor: selected.session.run_state.floor_num,
        current_hp: selected.session.run_state.current_hp,
        max_hp: selected.session.run_state.max_hp,
        gold: selected.session.run_state.gold,
        keys: selected.session.run_state.keys,
        deck: selected.session.run_state.master_deck.clone(),
        relics: selected.session.run_state.relics.clone(),
        potions: selected.session.run_state.potions.clone(),
        engine_state: engine_state_name(&selected.session.engine_state).to_string(),
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        build_identity: current_oracle_build_identity(),
        initial_neow_frontier,
        explorer: explorer_summary,
        committed_progress_steps: journal.len(),
        decisions: decision_summaries(&journal),
        combat_resolutions: combat_resolution_summaries(&journal),
        first_unresolved_combat_case,
        selected_active_combat_case,
        victory_witness,
        continuation,
    })
}

fn current_oracle_build_identity() -> OracleRunBuildIdentityV1 {
    let executable = std::env::current_exe().ok();
    let metadata = executable
        .as_ref()
        .and_then(|path| std::fs::metadata(path).ok());
    OracleRunBuildIdentityV1 {
        source_identity: super::current_source_identity(),
        cargo_profile: executable
            .as_deref()
            .and_then(oracle_target_profile)
            .unwrap_or_else(|| env!("STS_CARGO_PROFILE").to_string()),
        executable_len: metadata.as_ref().map(std::fs::Metadata::len),
        executable_modified_unix_ms: metadata
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64),
    }
}

fn oracle_target_profile(executable: &Path) -> Option<String> {
    let mut directory = executable.parent()?;
    if directory.file_name().is_some_and(|name| name == "deps") {
        directory = directory.parent()?;
    }
    directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

fn oracle_neow_root_progress(
    explored: &OracleRunExploreResultV1,
    pending_combats: &[OraclePendingCombatSummaryV1],
    deferred_combat_branch_ids: &[usize],
) -> Vec<OracleNeowRootProgressSummaryV1> {
    let mut by_root = std::collections::BTreeMap::<String, OracleNeowRootProgressSummaryV1>::new();
    for branch in &explored.explorer.branches {
        let entry = by_root
            .entry(branch.neow_root_candidate_id.clone())
            .or_insert_with(|| OracleNeowRootProgressSummaryV1 {
                root_candidate_id: branch.neow_root_candidate_id.clone(),
                root_label: branch.neow_root_label.clone(),
                materialized_branches: 0,
                furthest_act: branch.session.run_state.act_num,
                furthest_floor: branch.session.run_state.floor_num,
                furthest_branch_id: branch.branch_id,
                pending_decisions: 0,
                pending_combats: 0,
                deferred_combats: 0,
                unresolved_combats: 0,
            });
        entry.materialized_branches = entry.materialized_branches.saturating_add(1);
        if (
            branch.session.run_state.act_num,
            branch.session.run_state.floor_num,
            branch.branch_id,
        ) > (
            entry.furthest_act,
            entry.furthest_floor,
            entry.furthest_branch_id,
        ) {
            entry.furthest_act = branch.session.run_state.act_num;
            entry.furthest_floor = branch.session.run_state.floor_num;
            entry.furthest_branch_id = branch.branch_id;
        }
    }
    for pending in &explored.explorer.pending_decisions {
        if let Some(entry) = by_root.get_mut(&pending.neow_root_candidate_id) {
            entry.pending_decisions = entry.pending_decisions.saturating_add(1);
        }
    }
    for pending in pending_combats {
        if let Some(branch) = explored
            .explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == pending.branch_id)
        {
            if let Some(entry) = by_root.get_mut(&branch.neow_root_candidate_id) {
                entry.pending_combats = entry.pending_combats.saturating_add(1);
            }
        }
    }
    for branch_id in deferred_combat_branch_ids {
        if let Some(branch) = explored
            .explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == *branch_id)
        {
            if let Some(entry) = by_root.get_mut(&branch.neow_root_candidate_id) {
                entry.deferred_combats = entry.deferred_combats.saturating_add(1);
            }
        }
    }
    for unresolved in &explored.explorer.unresolved_combats {
        if let Some(branch) = explored
            .explorer
            .branches
            .iter()
            .find(|branch| branch.branch_id == unresolved.branch_id)
        {
            if let Some(entry) = by_root.get_mut(&branch.neow_root_candidate_id) {
                entry.unresolved_combats = entry.unresolved_combats.saturating_add(1);
            }
        }
    }
    by_root.into_values().collect()
}

fn first_unresolved_combat_case(
    config: &OracleRunConfig,
    explored: &OracleRunExploreResultV1,
) -> Result<Option<CombatCase>, String> {
    let Some(unresolved) = explored.explorer.unresolved_combats.first() else {
        return Ok(None);
    };
    let Some(branch) = explored
        .explorer
        .branches
        .iter()
        .find(|branch| branch.branch_id == unresolved.branch_id)
    else {
        return Ok(None);
    };
    combat_case_for_branch(
        config,
        branch,
        format!("{:?}", unresolved.failure),
        unresolved.generation_work.min(usize::MAX as u64) as usize,
    )
}

fn combat_case_for_branch(
    config: &OracleRunConfig,
    branch: &OracleRunBranchV1,
    reason: String,
    generation_work: usize,
) -> Result<Option<CombatCase>, String> {
    let Some(active) = branch.session.active_combat.as_ref() else {
        return Ok(None);
    };
    let position = CombatPosition::new(active.engine_state.clone(), active.combat_state.clone());
    let wall_ms = if active.combat_state.meta.is_boss_fight {
        config.budget.boss_ms
    } else if active.combat_state.meta.is_elite_fight {
        config.budget.elite_ms
    } else {
        config.budget.hallway_ms
    };
    let mut case = CombatCase::new(
        CombatCaseSource {
            seed: config.seed,
            ascension: config.ascension,
            generation: branch.replay.len(),
            branch_id: branch.branch_id,
            parent_id: branch.parent_branch_id,
        },
        CombatCaseGap {
            boundary: format!(
                "Act {} Floor {} combat",
                branch.session.run_state.act_num, branch.session.run_state.floor_num
            ),
            reason,
            witness_budget: CombatCaseWitnessBudgetV1::TurnGraphPortfolioV1 {
                generation_work,
                wall_ms,
            },
        },
        CombatCaseRunSummary {
            act: branch.session.run_state.act_num,
            floor: branch.session.run_state.floor_num,
            hp: branch.session.run_state.current_hp,
            max_hp: branch.session.run_state.max_hp,
            gold: branch.session.run_state.gold,
            deck_size: branch.session.run_state.master_deck.len(),
            relic_count: branch.session.run_state.relics.len(),
            potion_slots: branch.session.run_state.potions.len(),
        },
        Vec::new(),
        None,
        Vec::new(),
        CombatCaseRngSummary::from_pool(&branch.session.run_state.rng_pool),
        position,
    );
    case.production_context = Some(capture_oracle_analysis_combat_case_production_context_v2(
        &case.core,
        &branch.session,
        &oracle_run_combat_budgets_v1(config),
    )?);
    Ok(Some(case))
}

fn oracle_neow_frontier_summary(expansion: NeowOracleExpansionV1) -> OracleNeowFrontierSummaryV1 {
    OracleNeowFrontierSummaryV1 {
        completed: expansion
            .completed
            .into_iter()
            .map(|candidate| OracleCompletedNeowCandidateSummaryV1 {
                root_candidate_id: candidate.root_candidate_id,
                root_candidate_key: candidate.root_candidate_key,
                root_label: candidate.root_label,
                replay: candidate
                    .replay
                    .into_iter()
                    .map(|step| OracleNeowReplayStepSummaryV1 {
                        candidate_id: step.candidate_id,
                        label: step.label,
                        action: step.action,
                    })
                    .collect(),
                current_hp: candidate.session.run_state.current_hp,
                max_hp: candidate.session.run_state.max_hp,
                gold: candidate.session.run_state.gold,
                deck: candidate.session.run_state.master_deck,
                relics: candidate.session.run_state.relics,
                potions: candidate.session.run_state.potions,
                engine_state: engine_state_name(&candidate.session.engine_state).to_string(),
            })
            .collect(),
        unresolved: expansion
            .unresolved
            .into_iter()
            .map(|candidate| OracleUnresolvedNeowCandidateSummaryV1 {
                root_candidate_id: candidate.root_candidate_id,
                root_label: candidate.root_label,
                replay: candidate
                    .replay
                    .into_iter()
                    .map(|step| OracleNeowReplayStepSummaryV1 {
                        candidate_id: step.candidate_id,
                        label: step.label,
                        action: step.action,
                    })
                    .collect(),
                boundary: candidate.boundary,
                reason: candidate.reason,
            })
            .collect(),
    }
}

fn combat_resolution_summaries(
    journal: &RunProgressJournalV1,
) -> Vec<OracleCombatResolutionSummaryV1> {
    journal
        .entries()
        .iter()
        .filter_map(RunProgressStepV1::as_combat_resolution)
        .map(|resolution| OracleCombatResolutionSummaryV1 {
            combat_sequence: resolution.before.combat_sequence,
            title: resolution.before.title.clone(),
            location: resolution.before.location.clone(),
            kind: format!("{:?}", resolution.kind),
            source: resolution.trajectory.source.to_string(),
            action_count: resolution.trajectory.action_count,
            final_hp: resolution
                .trajectory
                .actions
                .iter()
                .rev()
                .find_map(|action| action.combat_after.as_ref().map(|state| state.player_hp)),
            actions: resolution.trajectory.actions.clone(),
        })
        .collect()
}

fn decision_summaries(journal: &RunProgressJournalV1) -> Vec<OracleDecisionSummaryV1> {
    journal
        .entries()
        .iter()
        .filter_map(RunProgressStepV1::as_decision)
        .map(|transaction| {
            let selected_label = transaction
                .before
                .candidates
                .iter()
                .find(|candidate| candidate.candidate_id == transaction.selection.candidate_id)
                .map(|candidate| candidate.label.clone())
                .unwrap_or_else(|| "<selected candidate missing>".to_string());
            OracleDecisionSummaryV1 {
                decision_step: transaction.before.decision_step,
                title: transaction.before.title.clone(),
                location: transaction.before.location.clone(),
                source: format!("{:?}", transaction.selection.source),
                selected_label,
                candidate_labels: transaction
                    .before
                    .candidates
                    .iter()
                    .map(|candidate| candidate.label.clone())
                    .collect(),
                action: transaction.action.clone(),
            }
        })
        .collect()
}

fn validate_config(config: &OracleRunConfig) -> Result<(), String> {
    if config.budget.max_work_items == 0 {
        return Err("oracle run requires at least one work item".to_string());
    }
    if config.budget.combat_quantum_generation_work == 0 || config.budget.combat_quantum_ms == 0 {
        return Err("oracle run combat quantum must be positive".to_string());
    }
    if config.budget.combat_initial_divisor == 0 {
        return Err("oracle run combat initial divisor must be positive".to_string());
    }
    if config.ascension > 20 {
        return Err(format!(
            "oracle run ascension must be in 0..=20, got {}",
            config.ascension
        ));
    }
    Ok(())
}

fn validate_continuation(
    config: &OracleRunConfig,
    continuation: &OracleRunContinuationV1,
) -> Result<(), String> {
    continuation.validate_supported_semantics()?;
    if continuation.seed != config.seed || continuation.ascension != config.ascension {
        return Err(format!(
            "oracle continuation run mismatch: artifact is seed {} A{}, request is seed {} A{}",
            continuation.seed, continuation.ascension, config.seed, config.ascension
        ));
    }
    Ok(())
}

pub fn oracle_run_combat_budgets_v1(config: &OracleRunConfig) -> OracleRunCombatWitnessBudgetsV1 {
    OracleRunCombatWitnessBudgetsV1 {
        hallway: OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(config.budget.hallway_generation_work),
            wall_ms: Some(config.budget.hallway_ms),
            satisfaction: Some(CombatWitnessSatisfactionV1::ZeroLossOrBudget),
            ..OracleCombatWitnessOptionsV1::default()
        },
        elite: OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(config.budget.elite_generation_work),
            wall_ms: Some(config.budget.elite_ms),
            satisfaction: Some(CombatWitnessSatisfactionV1::ZeroLossOrBudget),
            ..OracleCombatWitnessOptionsV1::default()
        },
        boss: OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(config.budget.boss_generation_work),
            wall_ms: Some(config.budget.boss_ms),
            satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
            ..OracleCombatWitnessOptionsV1::default()
        },
        quality_policy: OracleRunCombatWitnessQualityPolicyV1::StrategicRun,
        initial_divisor: config.budget.combat_initial_divisor,
        guidance_bundle: None,
    }
}

fn engine_state_name(state: &EngineState) -> &'static str {
    match state {
        EngineState::MapNavigation => "map_navigation",
        EngineState::MapOverlay { .. } => "map_overlay",
        EngineState::CombatStart(_) => "combat_start",
        EngineState::CombatProcessing => "combat_processing",
        EngineState::CombatPlayerTurn => "combat_player_turn",
        EngineState::PendingChoice(_) => "pending_choice",
        EngineState::RewardScreen(_) => "reward_screen",
        EngineState::RewardOverlay { .. } => "reward_overlay",
        EngineState::EventRoom => "event_room",
        EngineState::Shop(_) => "shop",
        EngineState::Campfire => "campfire",
        EngineState::RunPendingChoice(_) => "run_pending_choice",
        EngineState::TreasureRoom(_) => "treasure_room",
        EngineState::BossRelicSelect(_) => "boss_relic_select",
        EngineState::GameOver(_) => "game_over",
        EngineState::DoubleBossComplete => "double_boss_complete",
        EngineState::Act3VictoryComplete => "act3_victory_complete",
        EngineState::TrueVictoryComplete => "true_victory_complete",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn continuation() -> OracleRunContinuationV1 {
        let session = RunControlSession::new(Default::default());
        OracleRunContinuationV1 {
            schema_name: ORACLE_RUN_CONTINUATION_SCHEMA_NAME.to_string(),
            schema_version: ORACLE_RUN_CONTINUATION_SCHEMA_VERSION,
            decision_prior: Some(RunPolicyPriorIdentityV1::UniformExactSurfaceV1),
            seed: 0,
            ascension: 0,
            journal: RunProgressJournalV1::default(),
            session: RunControlSessionCheckpointV1::from_session(&session),
            explorer_frontier: None,
        }
    }

    #[test]
    fn oracle_report_build_identity_names_the_profile_and_binary() {
        let identity = current_oracle_build_identity();

        assert!(!identity.cargo_profile.is_empty());
        assert!(identity.executable_len.is_some_and(|len| len > 0));
        assert!(identity.executable_modified_unix_ms.is_some());
        assert_eq!(
            oracle_target_profile(Path::new(r"D:\repo\target\fast-run\oracle_run.exe")),
            Some("fast-run".to_string())
        );
        assert_eq!(
            oracle_target_profile(Path::new(r"D:\repo\target\debug\deps\oracle_run-tests.exe")),
            Some("debug".to_string())
        );
    }

    #[test]
    fn oracle_boss_budget_uses_the_replayable_first_win_policy() {
        let config = OracleRunConfig {
            seed: 6,
            ascension: 0,
            budget: OracleRunBudget::default(),
        };

        let budgets = oracle_run_combat_budgets_v1(&config);

        assert_eq!(
            budgets.boss.satisfaction,
            Some(CombatWitnessSatisfactionV1::FirstCompleteWin)
        );
    }

    #[test]
    fn continuation_persists_and_requires_its_prior_identity() {
        let continuation = continuation();
        let json = serde_json::to_value(&continuation).expect("serialized continuation");
        assert_eq!(json["decision_prior"], "uniform_exact_surface_v1");
        continuation
            .validate_supported_semantics()
            .expect("matching continuation semantics");

        let mut legacy = continuation.clone();
        legacy.schema_version = 1;
        legacy.decision_prior = None;
        let legacy_error = legacy
            .validate_supported_semantics()
            .expect_err("legacy continuation must be rejected");
        assert!(legacy_error.contains("version 1"), "{legacy_error}");

        let mut missing_prior = continuation;
        missing_prior.decision_prior = None;
        let prior_error = missing_prior
            .validate_supported_semantics()
            .expect_err("V2 continuation without a named prior must be rejected");
        assert!(prior_error.contains("decision prior"), "{prior_error}");
    }
}
