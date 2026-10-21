use super::super::*;
use super::finish_coverage::{coverage_status_for_finished_search, coverage_status_reason};
use super::finish_evidence::evidence_reliability_report;
use super::finish_outcome::outcome_report;
use super::finish_policy::{budget_report, search_policy_report};
use super::finish_trajectories::trajectory_reports;
use super::loop_state::SearchLoopState;
use super::root_evidence::frontier_evidence_scan;
use std::time::Instant;

pub(super) struct SearchFinishInput {
    pub(super) config: CombatSearchV2Config,
    pub(super) policy_evidence: CombatSearchV2PolicyEvidenceReport,
    pub(super) loop_state: SearchLoopState,
    pub(super) quantum_history: Vec<CombatSearchV2QuantumEvidence>,
}

pub(super) fn finish_combat_search_report(input: SearchFinishInput) -> CombatSearchV2Report {
    let SearchFinishInput {
        config,
        policy_evidence,
        loop_state,
        quantum_history,
    } = input;
    let frontier_scan_started = Instant::now();
    let frontier_evidence = frontier_evidence_scan(&loop_state);
    let frontier_scan_elapsed_us = frontier_scan_started.elapsed().as_micros();
    let reportable_trajectories = loop_state.reportable_trajectories();
    let SearchLoopState {
        owns_engine_pending_choice_prefixes,
        stats,
        diagnostics,
        exact_transpositions,
        dominance,
        frontier,
        trajectories,
        rollout_cache,
        mut performance,
        unresolved_leaf_count,
        max_actions_cut_count,
        engine_step_limit_count,
        potion_budget_cut_count,
        exhausted,
        accepted_complete_candidate,
        ..
    } = loop_state;
    let exhaustive = !accepted_complete_candidate
        && !exhausted
        && frontier.is_empty()
        && unresolved_leaf_count == 0
        && max_actions_cut_count == 0
        && engine_step_limit_count == 0
        && potion_budget_cut_count == 0
        && !stats.action_surface_incomplete;
    let coverage_status =
        coverage_status_for_finished_search(&stats, exhaustive, accepted_complete_candidate);
    let coverage_reason = coverage_status_reason(coverage_status);
    let sample_states = frontier_evidence.sample_states;
    let frontier_work_items = frontier_evidence.work_item_count;
    let pending_choice_work_items = frontier_evidence.pending_choice_work_items;
    let diagnostics = diagnostics.finish(SearchDiagnosticsFinish {
        exact_transpositions: &exact_transpositions,
        dominance: &dominance,
        frontier_work_items,
        frontier_sample_count: sample_states.len(),
        stats: &stats,
        coverage_status,
        unresolved_leaf_count,
        max_actions_cut_count,
        engine_step_limit_count,
        potion_budget_cut_count,
    });
    let invalid_card_identity_observed = diagnostics
        .card_identity
        .states_with_lineage_card_id_conflict
        > 0;
    let action_surface_incomplete = stats.action_surface_incomplete;
    let search_policy = search_policy_report(&config, owns_engine_pending_choice_prefixes);
    let budget = budget_report(&config);
    let rollout = rollout_cache.finish(trajectories.best_frontier.as_ref());
    let trajectory_reports = trajectory_reports(reportable_trajectories);
    let storage_drop_started = Instant::now();
    frontier.drop_parallel();
    drop(exact_transpositions);
    drop(dominance);
    performance.report_frontier_scan_elapsed_us = frontier_scan_elapsed_us;
    performance.report_search_storage_drop_elapsed_us = storage_drop_started.elapsed().as_micros();

    CombatSearchV2Report {
        schema_name: COMBAT_SEARCH_V2_REPORT_SCHEMA_NAME,
        schema_version: COMBAT_SEARCH_V2_REPORT_SCHEMA_VERSION,
        input_label: config.input_label,
        information_boundary: "engine_state_snapshot_truth_v0",
        policy_evidence,
        search_policy,
        budget,
        outcome: outcome_report(
            coverage_status,
            coverage_reason,
            trajectory_reports.best_complete_trajectory.is_some(),
            trajectory_reports.best_win_trajectory.is_some(),
            exhaustive,
        ),
        best_complete_trajectory: trajectory_reports.best_complete_trajectory,
        best_win_trajectory: trajectory_reports.best_win_trajectory,
        win_candidate_trajectories: trajectory_reports.win_candidate_trajectories,
        best_frontier_trajectory: trajectory_reports.best_frontier_trajectory,
        frontier: CombatSearchV2FrontierReport {
            remaining_work_items: frontier_work_items,
            pending_choice_work_items,
            unresolved_leaf_count,
            max_actions_cut_count,
            engine_step_limit_count,
            potion_budget_cut_count,
            best_estimated_value: trajectory_reports.best_frontier_value,
            sample_states,
        },
        rollout,
        diagnostics,
        stats,
        performance,
        evidence_reliability: evidence_reliability_report(
            invalid_card_identity_observed,
            exhaustive,
            action_surface_incomplete,
        ),
        quantum_history,
        final_root_evidence: frontier_evidence.root_evidence,
    }
}
