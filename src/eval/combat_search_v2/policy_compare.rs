use std::collections::BTreeMap;

use serde::Serialize;

use crate::ai::combat_search_v2::{
    compare_outcome_metrics, CombatSearchV2OutcomeMetrics, CombatSearchV2RolloutPolicy,
    CombatSearchV2TrajectoryReport, CombatSearchV2TurnPlanPolicy, SearchCoverageStatus,
    SearchTerminalLabel,
};

use super::benchmark::{
    run_combat_search_v2_benchmark, CombatSearchV2BenchmarkCaseReport,
    CombatSearchV2BenchmarkReport, CombatSearchV2LoadedBenchmarkCase,
};
use super::rollout_compare_attribution::{
    first_action_diff, CombatSearchV2RolloutPolicyFirstActionDiff,
};
use super::{CombatSearchV2LoadedBenchmark, CombatSearchV2RunOptions};

pub type CombatSearchV2RolloutPolicyComparisonReport = CombatSearchV2PolicyComparisonReport;
pub type CombatSearchV2RolloutPolicyComparisonSummary = CombatSearchV2PolicyComparisonSummary;
pub type CombatSearchV2RolloutPolicyComparisonCase = CombatSearchV2PolicyComparisonCase;
pub type CombatSearchV2RolloutPolicyComparisonVerdict = CombatSearchV2PolicyComparisonVerdict;
pub type CombatSearchV2RolloutPolicyComparisonRun = CombatSearchV2PolicyComparisonRun;

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2PolicyComparisonReport {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub benchmark_name: String,
    pub case_count: usize,
    pub comparison_kind: &'static str,
    pub left_policy: String,
    pub right_policy: String,
    pub summary: CombatSearchV2PolicyComparisonSummary,
    pub notes: Vec<&'static str>,
    pub cases: Vec<CombatSearchV2PolicyComparisonCase>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2PolicyComparisonSummary {
    pub cases_compared: usize,
    pub right_better: usize,
    pub left_better: usize,
    pub tied: usize,
    pub right_only_complete: usize,
    pub left_only_complete: usize,
    pub both_inconclusive: usize,
    pub first_action_diff_cases: usize,
    pub right_minus_left_final_hp_total: i32,
    pub right_minus_left_nodes_expanded_total: i64,
    pub right_minus_left_nodes_generated_total: i64,
    pub right_minus_left_turn_plan_frontier_seeded_nodes_total: i64,
    pub right_minus_left_rollout_evaluations_total: i64,
    pub right_minus_left_rollout_terminal_wins_total: i64,
    pub right_minus_left_rollout_budget_skips_total: i64,
    pub first_diff_action_index_histogram: BTreeMap<String, usize>,
    pub right_better_right_role_histogram: BTreeMap<String, usize>,
    pub left_better_right_role_histogram: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2PolicyComparisonCase {
    pub id: String,
    pub verdict: CombatSearchV2PolicyComparisonVerdict,
    pub right_minus_left_final_hp: Option<i32>,
    pub right_minus_left_hp_loss: Option<i32>,
    pub right_minus_left_turns: Option<i32>,
    pub right_minus_left_cards_played: Option<i32>,
    pub right_minus_left_turn_plan_frontier_seeded_nodes: i64,
    pub left: CombatSearchV2PolicyComparisonRun,
    pub right: CombatSearchV2PolicyComparisonRun,
    pub first_action_diff: Option<CombatSearchV2RolloutPolicyFirstActionDiff>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2PolicyComparisonVerdict {
    RightBetter,
    LeftBetter,
    Tied,
    RightOnlyComplete,
    LeftOnlyComplete,
    BothInconclusive,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2PolicyComparisonRun {
    pub policy: String,
    pub terminal: Option<SearchTerminalLabel>,
    pub coverage_status: SearchCoverageStatus,
    pub complete_trajectory_found: bool,
    pub final_hp: Option<i32>,
    pub hp_loss: Option<i32>,
    pub turns: Option<u32>,
    pub potions_used: Option<u32>,
    pub cards_played: Option<u32>,
    pub action_count: Option<usize>,
    pub nodes_expanded: u64,
    pub nodes_generated: u64,
    pub nodes_to_first_win: Option<u64>,
    pub deadline_hit: bool,
    pub node_budget_hit: bool,
    pub turn_plan_frontier_seeded_nodes: u64,
    pub rollout_evaluations: u64,
    pub rollout_cache_hits: u64,
    pub rollout_budget_skips: u64,
    pub rollout_terminal_wins: u64,
    pub rollout_terminal_losses: u64,
    pub rollout_beam_width: usize,
    pub rollout_turn_beam_extension_budget: usize,
    pub rollout_turn_beam_extensions: u64,
    pub rollout_turn_beam_extension_budget_skips: u64,
}

pub fn compare_combat_search_v2_rollout_policies(
    loaded: &CombatSearchV2LoadedBenchmark,
    options: CombatSearchV2RunOptions,
    left_policy: CombatSearchV2RolloutPolicy,
    right_policy: CombatSearchV2RolloutPolicy,
) -> CombatSearchV2PolicyComparisonReport {
    let mut left_options = options.clone();
    left_options.rollout_policy = Some(left_policy);
    let left = run_combat_search_v2_benchmark(loaded, left_options);

    let mut right_options = options.clone();
    right_options.rollout_policy = Some(right_policy);
    let right = run_combat_search_v2_benchmark(loaded, right_options);

    build_comparison_report(
        loaded,
        &options,
        "rollout_policy",
        left_policy.label(),
        right_policy.label(),
        &left,
        &right,
        vec![
            "comparison uses complete candidate trajectories, not stepwise action agreement",
            "first_action_diff identifies where selected best complete trajectories diverge",
            "rollout policy affects frontier priority; action diffs are consequences, not direct rollout action labels",
            "turn_beam_no_potion is a rollout estimate policy; it does not change exact replay validation",
            "first_action_diff context is reconstructed by exact replay of the common prefix and is diagnostic only",
        ],
    )
}

pub fn compare_combat_search_v2_turn_plan_policies(
    loaded: &CombatSearchV2LoadedBenchmark,
    options: CombatSearchV2RunOptions,
    left_policy: CombatSearchV2TurnPlanPolicy,
    right_policy: CombatSearchV2TurnPlanPolicy,
) -> CombatSearchV2PolicyComparisonReport {
    let mut left_options = options.clone();
    left_options.turn_plan_policy = Some(left_policy);
    let left = run_combat_search_v2_benchmark(loaded, left_options);

    let mut right_options = options.clone();
    right_options.turn_plan_policy = Some(right_policy);
    let right = run_combat_search_v2_benchmark(loaded, right_options);

    build_comparison_report(
        loaded,
        &options,
        "turn_plan_policy",
        left_policy.label(),
        right_policy.label(),
        &left,
        &right,
        vec![
            "comparison uses complete candidate trajectories, not stepwise action agreement",
            "turn_plan_policy=root_frontier_seed seeds exact root turn-plan end states into frontier",
            "seeded turn-plan states do not prune exact states and cannot create terminal outcome records without exact replay",
            "first_action_diff context is reconstructed by exact replay of the common prefix and is diagnostic only",
        ],
    )
}

fn build_comparison_report(
    loaded: &CombatSearchV2LoadedBenchmark,
    options: &CombatSearchV2RunOptions,
    comparison_kind: &'static str,
    left_policy: &'static str,
    right_policy: &'static str,
    left: &CombatSearchV2BenchmarkReport,
    right: &CombatSearchV2BenchmarkReport,
    notes: Vec<&'static str>,
) -> CombatSearchV2PolicyComparisonReport {
    let mut summary = CombatSearchV2PolicyComparisonSummary::default();
    let cases = loaded
        .cases
        .iter()
        .zip(left.cases.iter())
        .zip(right.cases.iter())
        .map(|((loaded_case, left_case), right_case)| {
            debug_assert_eq!(loaded_case.id, left_case.id);
            debug_assert_eq!(left_case.id, right_case.id);
            let case = compare_case(
                loaded_case,
                options,
                left_policy,
                right_policy,
                left_case,
                right_case,
            );
            observe_case(&mut summary, &case);
            case
        })
        .collect::<Vec<_>>();

    CombatSearchV2PolicyComparisonReport {
        schema_name: "CombatSearchV2PolicyComparisonReport",
        schema_version: 1,
        benchmark_name: loaded.name.clone(),
        case_count: cases.len(),
        comparison_kind,
        left_policy: left_policy.to_string(),
        right_policy: right_policy.to_string(),
        summary,
        notes,
        cases,
    }
}

fn compare_case(
    loaded: &CombatSearchV2LoadedBenchmarkCase,
    options: &CombatSearchV2RunOptions,
    left_policy: &'static str,
    right_policy: &'static str,
    left: &CombatSearchV2BenchmarkCaseReport,
    right: &CombatSearchV2BenchmarkCaseReport,
) -> CombatSearchV2PolicyComparisonCase {
    let left_trajectory = left.best_complete_trajectory.as_ref();
    let right_trajectory = right.best_complete_trajectory.as_ref();
    let verdict = compare_trajectories(left_trajectory, right_trajectory);
    CombatSearchV2PolicyComparisonCase {
        id: left.id.clone(),
        verdict,
        right_minus_left_final_hp: delta_i32(
            left_trajectory.map(|trajectory| trajectory.final_hp),
            right_trajectory.map(|trajectory| trajectory.final_hp),
        ),
        right_minus_left_hp_loss: delta_i32(
            left_trajectory.map(|trajectory| trajectory.hp_loss),
            right_trajectory.map(|trajectory| trajectory.hp_loss),
        ),
        right_minus_left_turns: delta_u32(
            left_trajectory.map(|trajectory| trajectory.turns),
            right_trajectory.map(|trajectory| trajectory.turns),
        ),
        right_minus_left_cards_played: delta_u32(
            left_trajectory.map(|trajectory| trajectory.cards_played),
            right_trajectory.map(|trajectory| trajectory.cards_played),
        ),
        right_minus_left_turn_plan_frontier_seeded_nodes: right
            .diagnostics
            .turn_plan
            .frontier_seeded_nodes as i64
            - left.diagnostics.turn_plan.frontier_seeded_nodes as i64,
        left: summarize_run(left_policy, left),
        right: summarize_run(right_policy, right),
        first_action_diff: first_action_diff(
            loaded,
            options,
            left_trajectory.map(|trajectory| trajectory.actions.as_slice()),
            right_trajectory.map(|trajectory| trajectory.actions.as_slice()),
        ),
    }
}

fn compare_trajectories(
    left: Option<&CombatSearchV2TrajectoryReport>,
    right: Option<&CombatSearchV2TrajectoryReport>,
) -> CombatSearchV2PolicyComparisonVerdict {
    match (left.filter(is_resolved), right.filter(is_resolved)) {
        (Some(left), Some(right)) => match compare_outcome_metrics(
            CombatSearchV2OutcomeMetrics::from_trajectory(right),
            CombatSearchV2OutcomeMetrics::from_trajectory(left),
        ) {
            std::cmp::Ordering::Greater => CombatSearchV2PolicyComparisonVerdict::RightBetter,
            std::cmp::Ordering::Equal => CombatSearchV2PolicyComparisonVerdict::Tied,
            std::cmp::Ordering::Less => CombatSearchV2PolicyComparisonVerdict::LeftBetter,
        },
        (None, Some(_)) => CombatSearchV2PolicyComparisonVerdict::RightOnlyComplete,
        (Some(_), None) => CombatSearchV2PolicyComparisonVerdict::LeftOnlyComplete,
        (None, None) => CombatSearchV2PolicyComparisonVerdict::BothInconclusive,
    }
}

fn is_resolved(trajectory: &&CombatSearchV2TrajectoryReport) -> bool {
    trajectory.terminal != SearchTerminalLabel::Unresolved
}

fn summarize_run(
    policy: &'static str,
    case: &CombatSearchV2BenchmarkCaseReport,
) -> CombatSearchV2PolicyComparisonRun {
    let trajectory = case.best_complete_trajectory.as_ref();
    CombatSearchV2PolicyComparisonRun {
        policy: policy.to_string(),
        terminal: trajectory.map(|trajectory| trajectory.terminal),
        coverage_status: case.outcome.coverage_status,
        complete_trajectory_found: case.outcome.complete_trajectory_found,
        final_hp: trajectory.map(|trajectory| trajectory.final_hp),
        hp_loss: trajectory.map(|trajectory| trajectory.hp_loss),
        turns: trajectory.map(|trajectory| trajectory.turns),
        potions_used: trajectory.map(|trajectory| trajectory.potions_used),
        cards_played: trajectory.map(|trajectory| trajectory.cards_played),
        action_count: trajectory.map(|trajectory| trajectory.actions.len()),
        nodes_expanded: case.stats.nodes_expanded,
        nodes_generated: case.stats.nodes_generated,
        nodes_to_first_win: case.stats.nodes_to_first_win,
        deadline_hit: case.stats.deadline_hit,
        node_budget_hit: case.stats.node_budget_hit,
        turn_plan_frontier_seeded_nodes: case.diagnostics.turn_plan.frontier_seeded_nodes,
        rollout_evaluations: case.rollout.evaluations,
        rollout_cache_hits: case.rollout.cache_hits,
        rollout_budget_skips: case.rollout.budget_skips,
        rollout_terminal_wins: case.rollout.terminal_wins,
        rollout_terminal_losses: case.rollout.terminal_losses,
        rollout_beam_width: case.rollout.beam_width,
        rollout_turn_beam_extension_budget: case.rollout.turn_beam_extension_budget,
        rollout_turn_beam_extensions: case.rollout.turn_beam_extensions,
        rollout_turn_beam_extension_budget_skips: case.rollout.turn_beam_extension_budget_skips,
    }
}

fn observe_case(
    summary: &mut CombatSearchV2PolicyComparisonSummary,
    case: &CombatSearchV2PolicyComparisonCase,
) {
    summary.cases_compared += 1;
    match case.verdict {
        CombatSearchV2PolicyComparisonVerdict::RightBetter => summary.right_better += 1,
        CombatSearchV2PolicyComparisonVerdict::LeftBetter => summary.left_better += 1,
        CombatSearchV2PolicyComparisonVerdict::Tied => summary.tied += 1,
        CombatSearchV2PolicyComparisonVerdict::RightOnlyComplete => {
            summary.right_only_complete += 1
        }
        CombatSearchV2PolicyComparisonVerdict::LeftOnlyComplete => summary.left_only_complete += 1,
        CombatSearchV2PolicyComparisonVerdict::BothInconclusive => summary.both_inconclusive += 1,
    }
    if let Some(delta) = case.right_minus_left_final_hp {
        summary.right_minus_left_final_hp_total += delta;
    }
    summary.right_minus_left_nodes_expanded_total +=
        case.right.nodes_expanded as i64 - case.left.nodes_expanded as i64;
    summary.right_minus_left_nodes_generated_total +=
        case.right.nodes_generated as i64 - case.left.nodes_generated as i64;
    summary.right_minus_left_turn_plan_frontier_seeded_nodes_total +=
        case.right_minus_left_turn_plan_frontier_seeded_nodes;
    summary.right_minus_left_rollout_evaluations_total +=
        case.right.rollout_evaluations as i64 - case.left.rollout_evaluations as i64;
    summary.right_minus_left_rollout_terminal_wins_total +=
        case.right.rollout_terminal_wins as i64 - case.left.rollout_terminal_wins as i64;
    summary.right_minus_left_rollout_budget_skips_total +=
        case.right.rollout_budget_skips as i64 - case.left.rollout_budget_skips as i64;
    if case.first_action_diff.is_some() {
        summary.first_action_diff_cases += 1;
    }
    if let Some(diff) = case.first_action_diff.as_ref() {
        increment_histogram(
            &mut summary.first_diff_action_index_histogram,
            diff.action_index.to_string(),
        );
        match case.verdict {
            CombatSearchV2PolicyComparisonVerdict::RightBetter
            | CombatSearchV2PolicyComparisonVerdict::RightOnlyComplete => {
                if let Some(role) = diff.right_action_role {
                    increment_histogram(&mut summary.right_better_right_role_histogram, role);
                }
            }
            CombatSearchV2PolicyComparisonVerdict::LeftBetter
            | CombatSearchV2PolicyComparisonVerdict::LeftOnlyComplete => {
                if let Some(role) = diff.right_action_role {
                    increment_histogram(&mut summary.left_better_right_role_histogram, role);
                }
            }
            CombatSearchV2PolicyComparisonVerdict::Tied
            | CombatSearchV2PolicyComparisonVerdict::BothInconclusive => {}
        }
    }
}

fn increment_histogram(histogram: &mut BTreeMap<String, usize>, key: impl Into<String>) {
    *histogram.entry(key.into()).or_default() += 1;
}

fn delta_i32(left: Option<i32>, right: Option<i32>) -> Option<i32> {
    Some(right? - left?)
}

fn delta_u32(left: Option<u32>, right: Option<u32>) -> Option<i32> {
    Some(right? as i32 - left? as i32)
}

#[cfg(test)]
#[path = "policy_compare_tests.rs"]
mod tests;
