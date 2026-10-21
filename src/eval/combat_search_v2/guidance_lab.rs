use std::cmp::Ordering;

use serde::Serialize;

use crate::ai::combat_search_v2::{
    explain_combat_search_v2_initial_decision, run_combat_search_v2,
    CombatSearchV2DecisionCandidateReport, CombatSearchV2DecisionMicroscopeReport,
    CombatSearchV2OutcomeReport, CombatSearchV2Report, CombatSearchV2StateSummary,
    SearchTerminalLabel,
};
use crate::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};

use super::{
    CombatSearchV2BenchmarkInputKind, CombatSearchV2LoadedBenchmark, CombatSearchV2LoadedStart,
    CombatSearchV2RunOptions,
};

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchGuidanceLabV1Report {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub label_role: &'static str,
    pub policy_quality_claim: bool,
    pub input_label: String,
    pub root: CombatSearchGuidanceLabRootV1,
    pub candidates: Vec<CombatSearchGuidanceLabCandidateV1>,
    pub summary: CombatSearchGuidanceLabSummaryV1,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchGuidanceLabBenchmarkV1Report {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub label_role: &'static str,
    pub policy_quality_claim: bool,
    pub benchmark_name: String,
    pub requested_case_limit: Option<usize>,
    pub effective_case_limit: usize,
    pub summary: CombatSearchGuidanceLabBenchmarkSummaryV1,
    pub cases: Vec<CombatSearchGuidanceLabBenchmarkCaseV1>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchGuidanceLabBenchmarkSummaryV1 {
    pub cases_run: usize,
    pub cases_available: usize,
    pub candidate_count: usize,
    pub child_searches_run: usize,
    pub child_complete_wins: usize,
    pub cases_best_target_not_first_by_current_ordering: usize,
    pub cases_best_target_differs_from_best_complete_first_action: usize,
    pub cases_without_best_complete_first_action: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchGuidanceLabBenchmarkCaseV1 {
    pub id: String,
    pub input_kind: CombatSearchV2BenchmarkInputKind,
    pub input_path: String,
    pub lab: CombatSearchGuidanceLabV1Report,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchGuidanceLabRootV1 {
    pub microscope: CombatSearchV2DecisionMicroscopeReport,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchGuidanceLabCandidateV1 {
    pub original_action_id: usize,
    pub ordered_index: usize,
    pub action_key: String,
    pub action_debug: String,
    pub action_role: &'static str,
    pub selected_by_best_complete: bool,
    pub one_step_status: &'static str,
    pub one_step_terminal: SearchTerminalLabel,
    pub child_search: Option<CombatSearchGuidanceLabChildSearchV1>,
    pub target: CombatSearchGuidanceLabTargetV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchGuidanceLabChildSearchV1 {
    pub outcome: CombatSearchV2OutcomeReport,
    pub best_complete: Option<CombatSearchGuidanceLabTrajectoryV1>,
    pub best_frontier: Option<CombatSearchGuidanceLabTrajectoryV1>,
    pub final_state: Option<CombatSearchV2StateSummary>,
    pub nodes_expanded: u64,
    pub nodes_generated: u64,
    pub terminal_wins: u64,
    pub elapsed_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchGuidanceLabTrajectoryV1 {
    pub terminal: SearchTerminalLabel,
    pub estimated: bool,
    pub first_action_key: Option<String>,
    pub action_keys_preview: Vec<String>,
    pub final_hp: i32,
    pub hp_loss: i32,
    pub turns: u32,
    pub potions_used: u32,
    pub potions_discarded: u32,
    pub cards_played: u32,
    pub action_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchGuidanceLabTargetV1 {
    pub target_kind: &'static str,
    pub source: &'static str,
    pub terminal: SearchTerminalLabel,
    pub complete_win: bool,
    pub post_root_player_hp: i32,
    pub child_search_hp_loss: Option<i32>,
    pub final_hp: Option<i32>,
    pub nodes_expanded: Option<u64>,
    pub limitations: Vec<&'static str>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchGuidanceLabSummaryV1 {
    pub candidate_count: usize,
    pub child_searches_run: usize,
    pub child_complete_wins: usize,
    pub child_escapes: usize,
    pub child_losses: usize,
    pub child_unresolved: usize,
    pub best_target_ordered_index: Option<usize>,
    pub best_complete_first_action_ordered_index: Option<usize>,
    pub current_order_selected_rank: Option<usize>,
}

pub fn run_combat_search_guidance_lab_benchmark_v1(
    loaded: &CombatSearchV2LoadedBenchmark,
    root_options: CombatSearchV2RunOptions,
    child_options: CombatSearchV2RunOptions,
    max_cases: Option<usize>,
) -> CombatSearchGuidanceLabBenchmarkV1Report {
    let limit = max_cases.unwrap_or(4);
    let cases = loaded
        .cases
        .iter()
        .take(limit)
        .map(|case| CombatSearchGuidanceLabBenchmarkCaseV1 {
            id: case.id.clone(),
            input_kind: case.input.kind,
            input_path: case.input.path.display().to_string(),
            lab: run_combat_search_guidance_lab_v1(
                &case.start,
                root_options.clone(),
                child_options.clone(),
            ),
        })
        .collect::<Vec<_>>();
    let summary = summarize_benchmark(&cases, loaded.cases.len());
    CombatSearchGuidanceLabBenchmarkV1Report {
        schema_name: "CombatSearchGuidanceLabBenchmarkV1Report",
        schema_version: 2,
        label_role: "oracle_search_guidance_lab_not_human_policy",
        policy_quality_claim: false,
        benchmark_name: loaded.name.clone(),
        requested_case_limit: max_cases,
        effective_case_limit: limit,
        summary,
        cases,
        notes: vec![
            "offline batch lab only; does not alter combat search ordering",
            "default benchmark limit is deliberately small to avoid probe explosion",
            "increase --guidance-lab-max-cases only for explicit data collection runs",
        ],
    }
}

pub fn run_combat_search_guidance_lab_v1(
    loaded: &CombatSearchV2LoadedStart,
    root_options: CombatSearchV2RunOptions,
    child_options: CombatSearchV2RunOptions,
) -> CombatSearchGuidanceLabV1Report {
    let root_config =
        root_options.to_search_config_for_position(loaded.label.clone(), &loaded.position);
    let microscope = explain_combat_search_v2_initial_decision(
        &loaded.position.engine,
        &loaded.position.combat,
        root_config.clone(),
    );
    let stepper = EngineCombatStepper;
    let candidates = microscope
        .candidates
        .iter()
        .map(|candidate| candidate_guidance_report(loaded, candidate, &child_options, &stepper))
        .collect::<Vec<_>>();
    let summary = summarize_candidates(&candidates);

    CombatSearchGuidanceLabV1Report {
        schema_name: "CombatSearchGuidanceLabV1Report",
        schema_version: 2,
        label_role: "oracle_search_guidance_lab_not_human_policy",
        policy_quality_claim: false,
        input_label: loaded.label.clone(),
        root: CombatSearchGuidanceLabRootV1 { microscope },
        candidates,
        summary,
        notes: vec![
            "offline lab only; does not alter combat search ordering",
            "targets come from bounded child search after each root candidate",
            "labels are oracle-under-current-simulator-budget, not human-optimal actions",
            "candidate set is limited by the decision microscope candidate report limit",
        ],
    }
}

fn candidate_guidance_report(
    loaded: &CombatSearchV2LoadedStart,
    candidate: &CombatSearchV2DecisionCandidateReport,
    child_options: &CombatSearchV2RunOptions,
    stepper: &impl CombatStepper,
) -> CombatSearchGuidanceLabCandidateV1 {
    let one_step = stepper.apply_to_stable(
        &loaded.position,
        candidate.input.clone(),
        CombatStepLimits {
            max_engine_steps: child_options
                .max_engine_steps_per_action
                .unwrap_or_else(|| root_step_limit(child_options)),
            deadline: None,
        },
    );
    let child_search = if one_step.alive
        && !one_step.truncated
        && matches!(
            one_step.terminal,
            crate::sim::combat::CombatTerminal::Unresolved
        ) {
        let child_position = one_step.position.clone();
        let child_config = child_options.to_search_config_for_position(
            format!("{}:child:{}", loaded.label, candidate.action_key),
            &child_position,
        );
        let report =
            run_combat_search_v2(&child_position.engine, &child_position.combat, child_config);
        Some(child_search_report(&report))
    } else {
        None
    };
    let target = candidate_target(&one_step, child_search.as_ref());

    CombatSearchGuidanceLabCandidateV1 {
        original_action_id: candidate.original_action_id,
        ordered_index: candidate.ordered_index,
        action_key: candidate.action_key.clone(),
        action_debug: candidate.action_debug.clone(),
        action_role: candidate.action_role,
        selected_by_best_complete: candidate.selected_by_best_complete,
        one_step_status: candidate.one_step.status,
        one_step_terminal: candidate.one_step.terminal,
        child_search,
        target,
    }
}

fn root_step_limit(options: &CombatSearchV2RunOptions) -> usize {
    options.max_engine_steps_per_action.unwrap_or(250)
}

fn child_search_report(report: &CombatSearchV2Report) -> CombatSearchGuidanceLabChildSearchV1 {
    CombatSearchGuidanceLabChildSearchV1 {
        outcome: report.outcome.clone(),
        best_complete: report
            .best_complete_trajectory
            .as_ref()
            .map(trajectory_summary),
        best_frontier: report
            .best_frontier_trajectory
            .as_ref()
            .map(trajectory_summary),
        final_state: report
            .best_complete_trajectory
            .as_ref()
            .map(|trajectory| trajectory.final_state.clone()),
        nodes_expanded: report.stats.nodes_expanded,
        nodes_generated: report.stats.nodes_generated,
        terminal_wins: report.stats.terminal_wins,
        elapsed_ms: report.stats.elapsed_ms,
    }
}

fn trajectory_summary(
    trajectory: &crate::ai::combat_search_v2::CombatSearchV2TrajectoryReport,
) -> CombatSearchGuidanceLabTrajectoryV1 {
    CombatSearchGuidanceLabTrajectoryV1 {
        terminal: trajectory.terminal,
        estimated: trajectory.estimated,
        first_action_key: trajectory
            .actions
            .first()
            .map(|action| action.action_key.clone()),
        action_keys_preview: trajectory
            .actions
            .iter()
            .take(8)
            .map(|action| action.action_key.clone())
            .collect(),
        final_hp: trajectory.final_hp,
        hp_loss: trajectory.hp_loss,
        turns: trajectory.turns,
        potions_used: trajectory.potions_used,
        potions_discarded: trajectory.potions_discarded,
        cards_played: trajectory.cards_played,
        action_count: trajectory.actions.len(),
    }
}

fn candidate_target(
    one_step: &crate::sim::combat::CombatStepResult,
    child_search: Option<&CombatSearchGuidanceLabChildSearchV1>,
) -> CombatSearchGuidanceLabTargetV1 {
    match one_step.terminal {
        crate::sim::combat::CombatTerminal::Win => CombatSearchGuidanceLabTargetV1 {
            target_kind: "root_action_child_search_rank",
            source: "one_step_terminal",
            terminal: SearchTerminalLabel::Win,
            complete_win: true,
            post_root_player_hp: one_step.position.combat.entities.player.current_hp,
            child_search_hp_loss: Some(0),
            final_hp: Some(one_step.position.combat.entities.player.current_hp),
            nodes_expanded: Some(0),
            limitations: vec!["one_step_terminal_win_no_child_search_needed"],
        },
        crate::sim::combat::CombatTerminal::Loss => CombatSearchGuidanceLabTargetV1 {
            target_kind: "root_action_child_search_rank",
            source: "one_step_terminal",
            terminal: SearchTerminalLabel::Loss,
            complete_win: false,
            post_root_player_hp: one_step.position.combat.entities.player.current_hp,
            child_search_hp_loss: None,
            final_hp: Some(one_step.position.combat.entities.player.current_hp),
            nodes_expanded: Some(0),
            limitations: vec!["one_step_terminal_loss_no_child_search_run"],
        },
        crate::sim::combat::CombatTerminal::Unresolved => {
            if let Some(child) = child_search {
                if let Some(best) = child.best_complete.as_ref() {
                    CombatSearchGuidanceLabTargetV1 {
                        target_kind: "root_action_child_search_rank",
                        source: "bounded_child_search_best_complete",
                        terminal: best.terminal,
                        complete_win: best.terminal == SearchTerminalLabel::Win,
                        post_root_player_hp: one_step.position.combat.entities.player.current_hp,
                        child_search_hp_loss: Some(best.hp_loss),
                        final_hp: Some(best.final_hp),
                        nodes_expanded: Some(child.nodes_expanded),
                        limitations: vec![
                            "bounded_child_search_not_exhaustive",
                            "target_terms_are_diagnostic_not_policy",
                        ],
                    }
                } else {
                    CombatSearchGuidanceLabTargetV1 {
                        target_kind: "root_action_child_search_rank",
                        source: "bounded_child_search_no_complete",
                        terminal: SearchTerminalLabel::Unresolved,
                        complete_win: false,
                        post_root_player_hp: one_step.position.combat.entities.player.current_hp,
                        child_search_hp_loss: None,
                        final_hp: None,
                        nodes_expanded: Some(child.nodes_expanded),
                        limitations: vec![
                            "no_complete_child_candidate_under_budget",
                            "unresolved_does_not_prove_bad_action",
                        ],
                    }
                }
            } else {
                CombatSearchGuidanceLabTargetV1 {
                    target_kind: "root_action_child_search_rank",
                    source: "one_step_unsearched",
                    terminal: SearchTerminalLabel::Unresolved,
                    complete_win: false,
                    post_root_player_hp: one_step.position.combat.entities.player.current_hp,
                    child_search_hp_loss: None,
                    final_hp: None,
                    nodes_expanded: None,
                    limitations: vec!["one_step_truncated_or_dead_no_child_search"],
                }
            }
        }
    }
}

fn summarize_candidates(
    candidates: &[CombatSearchGuidanceLabCandidateV1],
) -> CombatSearchGuidanceLabSummaryV1 {
    let mut summary = CombatSearchGuidanceLabSummaryV1 {
        candidate_count: candidates.len(),
        ..CombatSearchGuidanceLabSummaryV1::default()
    };
    let mut ranked = candidates.iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        compare_targets(&right.target, &left.target)
            .then_with(|| left.ordered_index.cmp(&right.ordered_index))
    });
    summary.best_target_ordered_index = ranked.first().map(|candidate| candidate.ordered_index);
    for (rank, candidate) in ranked.iter().enumerate() {
        if candidate.selected_by_best_complete {
            summary.best_complete_first_action_ordered_index = Some(candidate.ordered_index);
            summary.current_order_selected_rank = Some(rank + 1);
            break;
        }
    }

    for candidate in candidates {
        if candidate.child_search.is_some() {
            summary.child_searches_run += 1;
        }
        match candidate.target.terminal {
            SearchTerminalLabel::Win if candidate.target.complete_win => {
                summary.child_complete_wins += 1;
            }
            SearchTerminalLabel::Loss => summary.child_losses += 1,
            SearchTerminalLabel::Escape => summary.child_escapes += 1,
            SearchTerminalLabel::Unresolved | SearchTerminalLabel::Win => {
                summary.child_unresolved += 1;
            }
        }
    }
    summary
}

fn compare_targets(
    left: &CombatSearchGuidanceLabTargetV1,
    right: &CombatSearchGuidanceLabTargetV1,
) -> Ordering {
    target_terminal_tier(left)
        .cmp(&target_terminal_tier(right))
        .then_with(|| {
            left.final_hp
                .unwrap_or(i32::MIN)
                .cmp(&right.final_hp.unwrap_or(i32::MIN))
        })
        .then_with(|| {
            right
                .child_search_hp_loss
                .unwrap_or(i32::MAX)
                .cmp(&left.child_search_hp_loss.unwrap_or(i32::MAX))
        })
        .then_with(|| {
            right
                .nodes_expanded
                .unwrap_or(u64::MAX)
                .cmp(&left.nodes_expanded.unwrap_or(u64::MAX))
        })
}

fn target_terminal_tier(target: &CombatSearchGuidanceLabTargetV1) -> u8 {
    match (target.complete_win, target.terminal) {
        (true, SearchTerminalLabel::Win) => 3,
        (false, SearchTerminalLabel::Win) => 2,
        (_, SearchTerminalLabel::Unresolved) => 1,
        (_, SearchTerminalLabel::Escape | SearchTerminalLabel::Loss) => 0,
    }
}

fn summarize_benchmark(
    cases: &[CombatSearchGuidanceLabBenchmarkCaseV1],
    cases_available: usize,
) -> CombatSearchGuidanceLabBenchmarkSummaryV1 {
    let mut summary = CombatSearchGuidanceLabBenchmarkSummaryV1 {
        cases_run: cases.len(),
        cases_available,
        ..CombatSearchGuidanceLabBenchmarkSummaryV1::default()
    };
    for case in cases {
        let lab = &case.lab.summary;
        summary.candidate_count += lab.candidate_count;
        summary.child_searches_run += lab.child_searches_run;
        summary.child_complete_wins += lab.child_complete_wins;
        if lab
            .best_target_ordered_index
            .is_some_and(|index| index != 0)
        {
            summary.cases_best_target_not_first_by_current_ordering += 1;
        }
        match lab.best_complete_first_action_ordered_index {
            Some(best_complete_index) => {
                if lab.best_target_ordered_index != Some(best_complete_index) {
                    summary.cases_best_target_differs_from_best_complete_first_action += 1;
                }
            }
            None => summary.cases_without_best_complete_first_action += 1,
        }
    }
    summary
}
