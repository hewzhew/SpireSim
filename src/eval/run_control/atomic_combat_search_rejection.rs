use crate::ai::combat_search_v2::CombatSearchV2Report;
use crate::sim::combat::CombatPosition;

use super::atomic_combat_search_render::{
    render_policy_evidence_summary, render_search_diagnostics_summary,
    render_search_performance_summary, render_search_policy_summary,
};
use super::combat_line_adjudication::CombatLineAdjudicationV1;
use super::combat_line_trace::{
    attach_execution_adjudication, combat_search_performance_trace_annotation,
};
use super::session::{AtomicCombatSearchRejectionV2, RunControlSession, RunProgressOutcome};

pub(super) struct CombatSearchRejectionOutcome {
    pub(super) result: &'static str,
    pub(super) detail: Option<String>,
    pub(super) rejection: AtomicCombatSearchRejectionV2,
    pub(super) trace_source: &'static str,
    pub(super) execution_adjudication: Option<CombatLineAdjudicationV1>,
}

pub(super) fn build_atomic_combat_search_rejection_outcome(
    session: &RunControlSession,
    start: &CombatPosition,
    report: &CombatSearchV2Report,
    rejection: CombatSearchRejectionOutcome,
) -> RunProgressOutcome {
    let mut outcome = RunProgressOutcome::message(format!(
        "{}\n\n{}",
        render_search_rejection(report, rejection.result, rejection.detail),
        super::render::render_run_control_state(session)
    ))
    .with_atomic_combat_search_rejection(rejection.rejection);
    outcome
        .trace_annotations
        .push(combat_search_performance_trace_annotation(
            rejection.trace_source,
            session,
            start,
            report,
        ));
    if let Some(adjudication) = rejection.execution_adjudication {
        outcome = outcome.with_execution_adjudication(adjudication.clone());
        attach_execution_adjudication(&mut outcome.trace_annotations, &adjudication);
    }
    outcome
}

fn render_search_rejection(
    report: &CombatSearchV2Report,
    result: &'static str,
    detail: Option<String>,
) -> String {
    let mut lines = vec![
        "Search combat did not modify state.".to_string(),
        format!("  result={result}"),
    ];
    if let Some(detail) = detail {
        lines.push(format!("  detail={detail}"));
    }
    if let Some(candidate) = report.best_complete_trajectory.as_ref() {
        lines.push(format!(
            "  best_complete_candidate terminal={:?} final_hp={} hp_loss={} turns={} cards_played={} potions_used={} actions={}",
            candidate.terminal,
            candidate.final_hp,
            candidate.hp_loss,
            candidate.turns,
            candidate.cards_played,
            candidate.potions_used,
            candidate.actions.len()
        ));
    } else {
        lines.push("  best_complete_candidate=none".to_string());
    }
    lines.extend([
        format!("  coverage_status={:?}", report.outcome.coverage_status),
        render_search_policy_summary(report),
        render_search_diagnostics_summary(report),
        render_search_performance_summary(report),
        render_policy_evidence_summary(report),
        format!(
            "  complete_trajectory_found={}",
            report.outcome.complete_trajectory_found
        ),
        format!("  terminal_wins={}", report.stats.terminal_wins),
        format!("  nodes_expanded={}", report.stats.nodes_expanded),
        format!("  nodes_generated={}", report.stats.nodes_generated),
        format!(
            "  rollouts={} rollout_wins={} rollout_skips={}",
            report.rollout.evaluations, report.rollout.terminal_wins, report.rollout.budget_skips
        ),
        format!("  reliability={}", report.evidence_reliability.reliability),
        format!("  coverage_reason={}", report.outcome.coverage_reason),
    ]);
    lines.join("\n")
}
