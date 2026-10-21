use blake2::{Blake2b512, Digest};
use serde::{Deserialize, Serialize};

use crate::ai::combat_search_v2::{
    replay_combat_search_witness_line_v1, run_combat_search_v2, CombatSearchV2ActionPreview,
    CombatSearchV2OutcomeOrderKeyReport, CombatSearchV2Report, CombatSearchV2Stats,
    CombatSearchV2TrajectoryReport, CombatSearchV2WitnessLine, CombatSearchV2WitnessReplayV1,
    SearchCoverageStatus, SearchTerminalLabel,
};
use crate::eval::fingerprint::StateFingerprintV3;
use crate::sim::combat::CombatPosition;
use crate::state::core::ClientInput;
use crate::state::DomainCardSnapshot;

use super::{
    profile_config_v1, CombatLabCompiledSampleV1, ResolvedCombatLabProfileV1,
    ResolvedCombatLabSpecV1,
};

pub const COMBAT_LAB_CELL_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLabOutcomeClassV1 {
    ResolvedWin,
    ResolvedLoss,
    CoverageLimited,
    ExecutionError,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLabCellErrorStageV1 {
    SampleConstruction,
    Search,
    ExactReplay,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabCellErrorV1 {
    pub stage: CombatLabCellErrorStageV1,
    pub code: String,
    pub message: String,
    pub halt_experiment: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabReplayedCandidateV1 {
    pub terminal: SearchTerminalLabel,
    pub outcome_order_key: CombatSearchV2OutcomeOrderKeyReport,
    pub final_hp: i32,
    pub hp_loss: i32,
    pub turns: u32,
    pub actions: usize,
    pub cards_played: u32,
    pub potions_used: u32,
    pub draw_history: Vec<DomainCardSnapshot>,
    pub action_history: Vec<ClientInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CombatLabCellRecordV1 {
    pub schema_version: u32,
    pub cell_key: String,
    pub experiment_hash: String,
    pub sample_index: u64,
    pub shuffle_seed: u64,
    pub profile_id: String,
    pub profile_hash: String,
    pub budget_hash: String,
    pub initial_state_fingerprint: Option<StateFingerprintV3>,
    pub non_shuffle_rng_hash: Option<String>,
    pub shuffle_rng_hash: Option<String>,
    pub search_terminal: Option<SearchTerminalLabel>,
    pub coverage_status: Option<SearchCoverageStatus>,
    pub outcome_class: CombatLabOutcomeClassV1,
    pub outcome_order_key: Option<CombatSearchV2OutcomeOrderKeyReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replayed_candidate: Option<CombatLabReplayedCandidateV1>,
    pub replay_validated: bool,
    pub start_hp: Option<i32>,
    pub final_hp: Option<i32>,
    pub hp_loss: Option<i32>,
    pub turns: Option<u32>,
    pub actions: Option<usize>,
    pub cards_played: Option<u32>,
    pub potions_used: Option<u32>,
    pub draw_history: Vec<DomainCardSnapshot>,
    pub action_history: Vec<ClientInput>,
    pub expanded_nodes: u64,
    pub generated_nodes: u64,
    pub nodes_to_first_win: Option<u64>,
    pub node_budget_exhausted: bool,
    pub deadline_exhausted: bool,
    pub error: Option<CombatLabCellErrorV1>,
}

#[derive(Serialize)]
struct CombatLabCellKeyInputV1<'a> {
    experiment_hash: &'a str,
    sample_index: u64,
    shuffle_seed: u64,
    profile_id: &'a str,
    profile_hash: &'a str,
    budget_hash: &'a str,
}

pub fn combat_lab_cell_key_v1(
    experiment_hash: &str,
    sample_index: u64,
    shuffle_seed: u64,
    profile_id: &str,
    profile_hash: &str,
    budget_hash: &str,
) -> String {
    let input = CombatLabCellKeyInputV1 {
        experiment_hash,
        sample_index,
        shuffle_seed,
        profile_id,
        profile_hash,
        budget_hash,
    };
    let bytes = serde_json::to_vec(&input).expect("combat laboratory cell key should serialize");
    let mut hasher = Blake2b512::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let hash = digest[..32]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("combat_lab_cell_v1:{hash}")
}

pub fn execute_combat_lab_cell_v1(
    resolved: &ResolvedCombatLabSpecV1,
    sample: &CombatLabCompiledSampleV1,
    profile: &ResolvedCombatLabProfileV1,
) -> CombatLabCellRecordV1 {
    let config = profile_config_v1(
        &resolved.experiment_id,
        &profile.spec,
        &resolved.common_budget,
    );
    let report = run_combat_search_v2(&sample.start.engine, &sample.start.combat, config);
    combat_lab_cell_record_from_search_report_v1(resolved, sample, profile, &report)
}

pub fn combat_lab_cell_record_from_search_report_v1(
    resolved: &ResolvedCombatLabSpecV1,
    sample: &CombatLabCompiledSampleV1,
    profile: &ResolvedCombatLabProfileV1,
    report: &CombatSearchV2Report,
) -> CombatLabCellRecordV1 {
    combat_lab_cell_record_from_trajectory_with_replayer_v1(
        resolved,
        sample,
        profile,
        report.outcome.coverage_status,
        report.best_complete_trajectory.as_ref(),
        &report.stats,
        replay_combat_search_witness_line_v1,
    )
}

pub fn classify_combat_lab_outcome_v1(
    coverage_status: SearchCoverageStatus,
    selected_terminal: Option<SearchTerminalLabel>,
    replay_failed: bool,
) -> CombatLabOutcomeClassV1 {
    if replay_failed {
        return CombatLabOutcomeClassV1::ExecutionError;
    }
    if matches!(
        coverage_status,
        SearchCoverageStatus::NodeBudgetLimited
            | SearchCoverageStatus::ActionPrefixBudgetLimited
            | SearchCoverageStatus::ActionSurfaceIncomplete
            | SearchCoverageStatus::TimeBudgetLimited
            | SearchCoverageStatus::FrontierOpen
    ) || selected_terminal.is_none()
        || selected_terminal == Some(SearchTerminalLabel::Unresolved)
    {
        return CombatLabOutcomeClassV1::CoverageLimited;
    }
    match selected_terminal.expect("checked complete terminal") {
        SearchTerminalLabel::Win => CombatLabOutcomeClassV1::ResolvedWin,
        SearchTerminalLabel::Escape => CombatLabOutcomeClassV1::ResolvedLoss,
        SearchTerminalLabel::Loss => CombatLabOutcomeClassV1::ResolvedLoss,
        SearchTerminalLabel::Unresolved => CombatLabOutcomeClassV1::CoverageLimited,
    }
}

pub fn exact_replay_combat_search_trajectory_v1(
    start: &CombatPosition,
    trajectory: &CombatSearchV2TrajectoryReport,
    max_engine_steps_per_action: usize,
) -> Result<CombatLabReplayedCandidateV1, String> {
    exact_replay_combat_search_trajectory_with_replayer_v1(
        start,
        trajectory,
        max_engine_steps_per_action,
        replay_combat_search_witness_line_v1,
    )
}

fn exact_replay_combat_search_trajectory_with_replayer_v1<F>(
    start: &CombatPosition,
    trajectory: &CombatSearchV2TrajectoryReport,
    max_engine_steps_per_action: usize,
    replayer: F,
) -> Result<CombatLabReplayedCandidateV1, String>
where
    F: FnOnce(
        &CombatPosition,
        &CombatSearchV2WitnessLine,
        usize,
    ) -> Result<CombatSearchV2WitnessReplayV1, String>,
{
    if trajectory.estimated {
        return Err("selected complete trajectory is estimated".to_string());
    }
    let witness = witness_line_from_trajectory(trajectory);
    let evidence = replayer(start, &witness, max_engine_steps_per_action)?;
    let replay_final_hp = evidence
        .steps
        .last()
        .map(|step| step.player_hp)
        .unwrap_or(start.combat.entities.player.current_hp);
    if replay_final_hp != trajectory.final_hp {
        return Err(format!(
            "replayed final HP mismatch: expected {}, replayed {replay_final_hp}",
            trajectory.final_hp
        ));
    }

    Ok(CombatLabReplayedCandidateV1 {
        terminal: trajectory.terminal,
        outcome_order_key: trajectory.outcome_order_key,
        final_hp: trajectory.final_hp,
        hp_loss: trajectory.hp_loss,
        turns: trajectory.turns,
        actions: evidence.replayed_actions,
        cards_played: trajectory.cards_played,
        potions_used: trajectory.potions_used,
        draw_history: evidence
            .steps
            .iter()
            .flat_map(|step| step.drawn_cards.iter().cloned())
            .collect(),
        action_history: evidence
            .steps
            .iter()
            .map(|step| step.action.clone())
            .collect(),
    })
}

pub(super) fn combat_lab_cell_record_from_trajectory_with_replayer_v1<F>(
    resolved: &ResolvedCombatLabSpecV1,
    sample: &CombatLabCompiledSampleV1,
    profile: &ResolvedCombatLabProfileV1,
    coverage_status: SearchCoverageStatus,
    selected: Option<&CombatSearchV2TrajectoryReport>,
    stats: &CombatSearchV2Stats,
    replayer: F,
) -> CombatLabCellRecordV1
where
    F: FnOnce(
        &CombatPosition,
        &CombatSearchV2WitnessLine,
        usize,
    ) -> Result<CombatSearchV2WitnessReplayV1, String>,
{
    let selected_terminal = selected.map(|trajectory| trajectory.terminal);
    let mut replay_error = None;
    let replayed_candidate =
        selected.and_then(
            |trajectory| match exact_replay_combat_search_trajectory_with_replayer_v1(
                &sample.start,
                trajectory,
                resolved.common_budget.max_engine_steps_per_action,
                replayer,
            ) {
                Ok(candidate) => Some(candidate),
                Err(error) => {
                    replay_error = Some(error);
                    None
                }
            },
        );

    let outcome_class =
        classify_combat_lab_outcome_v1(coverage_status, selected_terminal, replay_error.is_some());
    let replay_validated = replayed_candidate.is_some();
    let resolved_trajectory = matches!(
        outcome_class,
        CombatLabOutcomeClassV1::ResolvedWin | CombatLabOutcomeClassV1::ResolvedLoss
    )
    .then_some(selected)
    .flatten();
    let draw_history = resolved_trajectory
        .and(replayed_candidate.as_ref())
        .map(|candidate| candidate.draw_history.clone())
        .unwrap_or_default();
    let action_history = resolved_trajectory
        .and(replayed_candidate.as_ref())
        .map(|candidate| candidate.action_history.clone())
        .unwrap_or_default();
    let replayed_actions = resolved_trajectory
        .and(replayed_candidate.as_ref())
        .map(|candidate| candidate.actions);
    let error = replay_error.map(|message| CombatLabCellErrorV1 {
        stage: CombatLabCellErrorStageV1::ExactReplay,
        code: "exact_replay_invariant_mismatch".to_string(),
        message: format!("combat laboratory exact replay invariant failure: {message}"),
        halt_experiment: true,
    });
    let start_hp = sample.start.combat.entities.player.current_hp;

    CombatLabCellRecordV1 {
        schema_version: COMBAT_LAB_CELL_SCHEMA_VERSION,
        cell_key: combat_lab_cell_key_v1(
            &resolved.experiment_hash,
            sample.sample_index,
            sample.shuffle_seed,
            &profile.spec.id,
            &profile.profile_hash,
            &resolved.budget_hash,
        ),
        experiment_hash: resolved.experiment_hash.clone(),
        sample_index: sample.sample_index,
        shuffle_seed: sample.shuffle_seed,
        profile_id: profile.spec.id.clone(),
        profile_hash: profile.profile_hash.clone(),
        budget_hash: resolved.budget_hash.clone(),
        initial_state_fingerprint: Some(sample.state_fingerprint.clone()),
        non_shuffle_rng_hash: Some(sample.non_shuffle_rng_hash.clone()),
        shuffle_rng_hash: Some(sample.shuffle_rng_hash.clone()),
        search_terminal: Some(selected_terminal.unwrap_or(SearchTerminalLabel::Unresolved)),
        coverage_status: Some(coverage_status),
        outcome_class,
        outcome_order_key: resolved_trajectory.map(|trajectory| trajectory.outcome_order_key),
        replayed_candidate,
        replay_validated,
        start_hp: Some(start_hp),
        final_hp: resolved_trajectory.map(|trajectory| trajectory.final_hp),
        hp_loss: resolved_trajectory.map(|trajectory| trajectory.hp_loss),
        turns: resolved_trajectory.map(|trajectory| trajectory.turns),
        actions: replayed_actions,
        cards_played: resolved_trajectory.map(|trajectory| trajectory.cards_played),
        potions_used: resolved_trajectory.map(|trajectory| trajectory.potions_used),
        draw_history,
        action_history,
        expanded_nodes: stats.nodes_expanded,
        generated_nodes: stats.nodes_generated,
        nodes_to_first_win: stats.nodes_to_first_win,
        node_budget_exhausted: stats.node_budget_hit,
        deadline_exhausted: stats.deadline_hit,
        error,
    }
}

fn witness_line_from_trajectory(
    trajectory: &CombatSearchV2TrajectoryReport,
) -> CombatSearchV2WitnessLine {
    CombatSearchV2WitnessLine {
        source: "combat_lab_v1_best_complete_trajectory",
        terminal: trajectory.terminal,
        final_hp: trajectory.final_hp,
        total_enemy_hp: trajectory.final_state.total_enemy_hp,
        action_count: Some(trajectory.actions.len()),
        actions: trajectory
            .actions
            .iter()
            .map(|action| CombatSearchV2ActionPreview {
                action_key: action.action_key.clone(),
                input: action.input.clone(),
            })
            .collect(),
    }
}
