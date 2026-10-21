use serde::Serialize;

use crate::ai::combat_search_v2::state_abstraction::{
    build_state_abstraction_gate_report, classify_state_abstraction_case,
    StateAbstractionCaseInput, StateAbstractionDivergenceInput, StateAbstractionGateReport,
};
use crate::ai::combat_search_v2::{SearchCoverageStatus, SearchTerminalLabel};

use super::benchmark::{
    CombatSearchV2BaselineVerdict, CombatSearchV2BenchmarkCaseReport,
    CombatSearchV2BenchmarkSummary,
};

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2BenchmarkGateReport {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub gate_name: &'static str,
    pub status: CombatSearchV2BenchmarkGateStatus,
    pub policy: &'static str,
    pub requirements: CombatSearchV2BenchmarkGateRequirements,
    pub summary: CombatSearchV2BenchmarkGateSummary,
    pub state_abstraction: StateAbstractionGateReport,
    pub priority_cases: Vec<CombatSearchV2BenchmarkGateCase>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatSearchV2BenchmarkGateStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2BenchmarkGateRequirements {
    pub require_complete_candidate_win_for_every_case: bool,
    pub require_no_complete_candidate_baseline_regression: bool,
    pub coverage_limits_are_reported_not_gate_warnings: bool,
    pub missing_baselines_are_reported_not_gate_warnings: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CombatSearchV2BenchmarkGateSummary {
    pub pass_cases: usize,
    pub warn_cases: usize,
    pub fail_cases: usize,
    pub missing_complete_trajectory: usize,
    pub non_winning_complete_candidate: usize,
    pub complete_candidate_baseline_regressions: usize,
    pub missing_baseline_cases: usize,
    pub deadline_cases: usize,
    pub node_budget_cases: usize,
    pub high_fanout_pending_choice_cases: usize,
    pub same_effect_turn_sequence_cases: usize,
    pub order_sensitive_turn_sequence_cases: usize,
    pub engine_step_limit_cases: usize,
    pub max_action_line_cut_cases: usize,
    pub potion_budget_cut_cases: usize,
    pub card_identity_warning_cases: usize,
    pub focus_counts: Vec<CombatSearchV2BenchmarkGateFocusCount>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2BenchmarkGateFocusCount {
    pub focus: &'static str,
    pub cases: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2BenchmarkGateCase {
    pub id: String,
    pub status: CombatSearchV2BenchmarkGateStatus,
    pub primary_focus: &'static str,
    pub reasons: Vec<&'static str>,
    pub metrics: CombatSearchV2BenchmarkGateCaseMetrics,
}

#[derive(Clone, Debug, Serialize)]
pub struct CombatSearchV2BenchmarkGateCaseMetrics {
    pub coverage_status: SearchCoverageStatus,
    pub best_complete_terminal: Option<SearchTerminalLabel>,
    pub best_complete_final_hp: Option<i32>,
    pub baseline_final_hp: Option<i32>,
    pub search_minus_baseline_final_hp: Option<i32>,
    pub search_minus_baseline_potions_used: Option<i32>,
    pub complete_candidate_verdict: Option<CombatSearchV2BaselineVerdict>,
    pub nodes_expanded: u64,
    pub nodes_generated: u64,
    pub nodes_to_first_win: Option<u64>,
    pub terminal_wins: u64,
    pub terminal_losses: u64,
    pub deadline_hit: bool,
    pub node_budget_hit: bool,
    pub unresolved_leaf_count: u64,
    pub max_actions_cut_count: u64,
    pub engine_step_limit_count: u64,
    pub potion_budget_cut_count: u64,
    pub high_fanout_pending_choice_states: u64,
    pub max_pending_choice_candidate_count: usize,
    pub max_legal_actions: usize,
    pub max_turn_generated_children: usize,
    pub same_effect_turn_sequence_groups: usize,
    pub order_sensitive_turn_sequence_groups: usize,
    pub duplicate_card_identity_states: u64,
    pub lineage_card_id_conflict_states: u64,
}

#[derive(Clone, Debug)]
struct CaseGateFacts<'a> {
    id: &'a str,
    coverage_status: SearchCoverageStatus,
    best_complete_terminal: Option<SearchTerminalLabel>,
    best_complete_final_hp: Option<i32>,
    best_complete_potions_used: Option<u32>,
    baseline_final_hp: Option<i32>,
    baseline_potions_used: Option<u32>,
    complete_candidate_verdict: Option<CombatSearchV2BaselineVerdict>,
    nodes_expanded: u64,
    nodes_generated: u64,
    nodes_to_first_win: Option<u64>,
    terminal_wins: u64,
    terminal_losses: u64,
    deadline_hit: bool,
    node_budget_hit: bool,
    unresolved_leaf_count: u64,
    max_actions_cut_count: u64,
    engine_step_limit_count: u64,
    potion_budget_cut_count: u64,
    high_fanout_pending_choice_states: u64,
    max_pending_choice_candidate_count: usize,
    max_legal_actions: usize,
    max_turn_generated_children: usize,
    same_effect_turn_sequence_groups: usize,
    order_sensitive_turn_sequence_groups: usize,
    duplicate_card_identity_states: u64,
    lineage_card_id_conflict_states: u64,
}

pub fn build_combat_search_v2_benchmark_gate_report(
    summary: &CombatSearchV2BenchmarkSummary,
    cases: &[CombatSearchV2BenchmarkCaseReport],
) -> CombatSearchV2BenchmarkGateReport {
    let mut gate_summary = CombatSearchV2BenchmarkGateSummary::default();
    let mut priority_cases = Vec::new();
    let mut state_abstraction_cases = Vec::new();
    for case in cases {
        let facts = case_gate_facts(case);
        if let Some(abstraction_case) = classify_state_abstraction_case(StateAbstractionCaseInput {
            case_id: facts.id,
            same_effect_turn_sequence_groups: facts.same_effect_turn_sequence_groups,
            order_sensitive_turn_sequence_groups: facts.order_sensitive_turn_sequence_groups,
            turn_sequence_divergence_histogram: case
                .diagnostics
                .turn_sequence
                .order_sensitive_divergence_histogram
                .iter()
                .map(|entry| StateAbstractionDivergenceInput {
                    kind: entry.kind,
                    first_divergence_path: entry.first_divergence_path,
                    guessed_reveal_gate: entry.guessed_reveal_gate,
                    groups: entry.groups,
                })
                .collect(),
        }) {
            state_abstraction_cases.push(abstraction_case);
        }
        let gate_case = assess_case_facts(facts);
        accumulate_gate_summary(&mut gate_summary, &gate_case);
        if gate_case.status != CombatSearchV2BenchmarkGateStatus::Pass {
            priority_cases.push(gate_case);
        }
    }

    sort_priority_cases(&mut priority_cases);
    gate_summary.focus_counts = focus_counts(&priority_cases);
    let state_abstraction = build_state_abstraction_gate_report(state_abstraction_cases);
    let status = if gate_summary.fail_cases > 0 {
        CombatSearchV2BenchmarkGateStatus::Fail
    } else if gate_summary.warn_cases > 0 {
        CombatSearchV2BenchmarkGateStatus::Warn
    } else {
        CombatSearchV2BenchmarkGateStatus::Pass
    };

    CombatSearchV2BenchmarkGateReport {
        schema_name: "CombatSearchV2BenchmarkGateReport",
        schema_version: 2,
        gate_name: "combat_search_benchmark_gate",
        status,
        policy: "fail on missing/non-winning complete candidate, complete-candidate baseline regression, or invalid card identity; coverage limits and diagnostics are reported without changing gate status",
        requirements: CombatSearchV2BenchmarkGateRequirements {
            require_complete_candidate_win_for_every_case: true,
            require_no_complete_candidate_baseline_regression: true,
            coverage_limits_are_reported_not_gate_warnings: true,
            missing_baselines_are_reported_not_gate_warnings: true,
        },
        summary: gate_summary,
        state_abstraction,
        priority_cases,
        notes: gate_notes(summary),
    }
}

fn case_gate_facts(case: &CombatSearchV2BenchmarkCaseReport) -> CaseGateFacts<'_> {
    let best = case.best_complete_trajectory.as_ref();
    let baseline = case.baseline.as_ref();
    CaseGateFacts {
        id: &case.id,
        coverage_status: case.outcome.coverage_status,
        best_complete_terminal: best.map(|trajectory| trajectory.terminal),
        best_complete_final_hp: best.map(|trajectory| trajectory.final_hp),
        best_complete_potions_used: best.map(|trajectory| trajectory.potions_used),
        baseline_final_hp: baseline.map(|baseline| baseline.final_hp),
        baseline_potions_used: baseline.map(|baseline| baseline.potions_used),
        complete_candidate_verdict: case
            .baseline_comparison
            .as_ref()
            .map(|comparison| comparison.verdict),
        nodes_expanded: case.stats.nodes_expanded,
        nodes_generated: case.stats.nodes_generated,
        nodes_to_first_win: case.stats.nodes_to_first_win,
        terminal_wins: case.stats.terminal_wins,
        terminal_losses: case.stats.terminal_losses,
        deadline_hit: case.stats.deadline_hit,
        node_budget_hit: case.stats.node_budget_hit,
        unresolved_leaf_count: case.diagnostics.pruning.unresolved_leaf_count,
        max_actions_cut_count: case.diagnostics.pruning.max_actions_cut_count,
        engine_step_limit_count: case.diagnostics.pruning.engine_step_limit_count,
        potion_budget_cut_count: case.diagnostics.pruning.potion_budget_cut_count,
        high_fanout_pending_choice_states: case.diagnostics.pending_choice.high_fanout_states,
        max_pending_choice_candidate_count: case.diagnostics.pending_choice.max_candidate_count,
        max_legal_actions: case.diagnostics.branching.legal_actions_max,
        max_turn_generated_children: case
            .diagnostics
            .turn_branching
            .largest_turn_fanouts
            .iter()
            .map(|sample| sample.generated_children)
            .max()
            .unwrap_or(0),
        same_effect_turn_sequence_groups: case
            .diagnostics
            .turn_sequence
            .same_effect_order_variant_groups,
        order_sensitive_turn_sequence_groups: case.diagnostics.turn_sequence.order_sensitive_groups,
        duplicate_card_identity_states: case
            .diagnostics
            .card_identity
            .states_with_duplicate_active_lineage,
        lineage_card_id_conflict_states: case
            .diagnostics
            .card_identity
            .states_with_lineage_card_id_conflict,
    }
}

fn assess_case_facts(facts: CaseGateFacts<'_>) -> CombatSearchV2BenchmarkGateCase {
    let mut status = CombatSearchV2BenchmarkGateStatus::Pass;
    let mut reasons = Vec::new();

    push_failure_if(
        &mut status,
        &mut reasons,
        facts.best_complete_terminal.is_none(),
        "missing_complete_trajectory",
    );
    push_failure_if(
        &mut status,
        &mut reasons,
        matches!(
            facts.best_complete_terminal,
            Some(SearchTerminalLabel::Loss | SearchTerminalLabel::Unresolved)
        ),
        "complete_candidate_not_win",
    );
    push_failure_if(
        &mut status,
        &mut reasons,
        facts.complete_candidate_verdict == Some(CombatSearchV2BaselineVerdict::BaselineBetter),
        "complete_candidate_worse_than_baseline",
    );

    push_failure_if(
        &mut status,
        &mut reasons,
        facts.lineage_card_id_conflict_states > 0,
        "invalid_card_identity_observed",
    );

    CombatSearchV2BenchmarkGateCase {
        id: facts.id.to_string(),
        status,
        primary_focus: primary_focus(&facts),
        reasons,
        metrics: CombatSearchV2BenchmarkGateCaseMetrics {
            coverage_status: facts.coverage_status,
            best_complete_terminal: facts.best_complete_terminal,
            best_complete_final_hp: facts.best_complete_final_hp,
            baseline_final_hp: facts.baseline_final_hp,
            search_minus_baseline_final_hp: optional_i32_delta(
                facts.best_complete_final_hp,
                facts.baseline_final_hp,
            ),
            search_minus_baseline_potions_used: optional_u32_delta(
                facts.best_complete_potions_used,
                facts.baseline_potions_used,
            ),
            complete_candidate_verdict: facts.complete_candidate_verdict,
            nodes_expanded: facts.nodes_expanded,
            nodes_generated: facts.nodes_generated,
            nodes_to_first_win: facts.nodes_to_first_win,
            terminal_wins: facts.terminal_wins,
            terminal_losses: facts.terminal_losses,
            deadline_hit: facts.deadline_hit,
            node_budget_hit: facts.node_budget_hit,
            unresolved_leaf_count: facts.unresolved_leaf_count,
            max_actions_cut_count: facts.max_actions_cut_count,
            engine_step_limit_count: facts.engine_step_limit_count,
            potion_budget_cut_count: facts.potion_budget_cut_count,
            high_fanout_pending_choice_states: facts.high_fanout_pending_choice_states,
            max_pending_choice_candidate_count: facts.max_pending_choice_candidate_count,
            max_legal_actions: facts.max_legal_actions,
            max_turn_generated_children: facts.max_turn_generated_children,
            same_effect_turn_sequence_groups: facts.same_effect_turn_sequence_groups,
            order_sensitive_turn_sequence_groups: facts.order_sensitive_turn_sequence_groups,
            duplicate_card_identity_states: facts.duplicate_card_identity_states,
            lineage_card_id_conflict_states: facts.lineage_card_id_conflict_states,
        },
    }
}

fn push_failure_if(
    status: &mut CombatSearchV2BenchmarkGateStatus,
    reasons: &mut Vec<&'static str>,
    condition: bool,
    reason: &'static str,
) {
    if condition {
        *status = CombatSearchV2BenchmarkGateStatus::Fail;
        reasons.push(reason);
    }
}

fn primary_focus(facts: &CaseGateFacts<'_>) -> &'static str {
    if facts.best_complete_terminal.is_none()
        || matches!(
            facts.best_complete_terminal,
            Some(SearchTerminalLabel::Loss | SearchTerminalLabel::Unresolved)
        )
    {
        if facts.high_fanout_pending_choice_states > 0 {
            "pending_choice"
        } else if facts.deadline_hit || facts.node_budget_hit {
            "budget_or_rollout"
        } else {
            "complete_trajectory"
        }
    } else if facts.complete_candidate_verdict
        == Some(CombatSearchV2BaselineVerdict::BaselineBetter)
    {
        "value_outcome"
    } else if facts.lineage_card_id_conflict_states > 0 {
        "state_integrity"
    } else if facts.high_fanout_pending_choice_states > 0 {
        "pending_choice"
    } else if facts.same_effect_turn_sequence_groups > 0 {
        "turn_sequence"
    } else if facts.order_sensitive_turn_sequence_groups > 0 {
        "state_abstraction_boundary"
    } else {
        "none"
    }
}

fn optional_i32_delta(left: Option<i32>, right: Option<i32>) -> Option<i32> {
    Some(left? - right?)
}

fn optional_u32_delta(left: Option<u32>, right: Option<u32>) -> Option<i32> {
    Some(left? as i32 - right? as i32)
}

fn accumulate_gate_summary(
    summary: &mut CombatSearchV2BenchmarkGateSummary,
    case: &CombatSearchV2BenchmarkGateCase,
) {
    match case.status {
        CombatSearchV2BenchmarkGateStatus::Pass => summary.pass_cases += 1,
        CombatSearchV2BenchmarkGateStatus::Warn => summary.warn_cases += 1,
        CombatSearchV2BenchmarkGateStatus::Fail => summary.fail_cases += 1,
    }
    count_reason(
        &mut summary.missing_complete_trajectory,
        &case.reasons,
        "missing_complete_trajectory",
    );
    count_reason(
        &mut summary.non_winning_complete_candidate,
        &case.reasons,
        "complete_candidate_not_win",
    );
    count_reason(
        &mut summary.complete_candidate_baseline_regressions,
        &case.reasons,
        "complete_candidate_worse_than_baseline",
    );
    if case.metrics.baseline_final_hp.is_none() {
        summary.missing_baseline_cases += 1;
    }
    if case.metrics.deadline_hit {
        summary.deadline_cases += 1;
    }
    if case.metrics.node_budget_hit {
        summary.node_budget_cases += 1;
    }
    if case.metrics.high_fanout_pending_choice_states > 0 {
        summary.high_fanout_pending_choice_cases += 1;
    }
    if case.metrics.same_effect_turn_sequence_groups > 0 {
        summary.same_effect_turn_sequence_cases += 1;
    }
    if case.metrics.same_effect_turn_sequence_groups == 0
        && case.metrics.order_sensitive_turn_sequence_groups > 0
    {
        summary.order_sensitive_turn_sequence_cases += 1;
    }
    if case.metrics.engine_step_limit_count > 0 {
        summary.engine_step_limit_cases += 1;
    }
    if case.metrics.max_actions_cut_count > 0 {
        summary.max_action_line_cut_cases += 1;
    }
    if case.metrics.potion_budget_cut_count > 0 {
        summary.potion_budget_cut_cases += 1;
    }
    if case.metrics.duplicate_card_identity_states > 0
        || case.metrics.lineage_card_id_conflict_states > 0
    {
        summary.card_identity_warning_cases += 1;
    }
}

fn count_reason(count: &mut usize, reasons: &[&'static str], reason: &'static str) {
    if reasons.contains(&reason) {
        *count += 1;
    }
}

fn sort_priority_cases(cases: &mut [CombatSearchV2BenchmarkGateCase]) {
    cases.sort_by(|left, right| {
        status_rank(right.status)
            .cmp(&status_rank(left.status))
            .then_with(|| focus_rank(left.primary_focus).cmp(&focus_rank(right.primary_focus)))
            .then_with(|| {
                right
                    .metrics
                    .nodes_generated
                    .cmp(&left.metrics.nodes_generated)
            })
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn status_rank(status: CombatSearchV2BenchmarkGateStatus) -> u8 {
    match status {
        CombatSearchV2BenchmarkGateStatus::Pass => 0,
        CombatSearchV2BenchmarkGateStatus::Warn => 1,
        CombatSearchV2BenchmarkGateStatus::Fail => 2,
    }
}

fn focus_rank(focus: &'static str) -> u8 {
    match focus {
        "complete_trajectory" => 0,
        "value_outcome" => 1,
        "state_integrity" => 2,
        "pending_choice" => 3,
        "turn_sequence" => 4,
        "state_abstraction_boundary" => 5,
        "budget_or_rollout" => 6,
        "baseline_coverage" => 7,
        _ => 8,
    }
}

fn focus_counts(
    cases: &[CombatSearchV2BenchmarkGateCase],
) -> Vec<CombatSearchV2BenchmarkGateFocusCount> {
    let mut counts: Vec<CombatSearchV2BenchmarkGateFocusCount> = Vec::new();
    for case in cases {
        match counts
            .iter_mut()
            .find(|count| count.focus == case.primary_focus)
        {
            Some(count) => count.cases += 1,
            None => counts.push(CombatSearchV2BenchmarkGateFocusCount {
                focus: case.primary_focus,
                cases: 1,
            }),
        }
    }
    counts.sort_by(|left, right| {
        right
            .cases
            .cmp(&left.cases)
            .then_with(|| focus_rank(left.focus).cmp(&focus_rank(right.focus)))
    });
    counts
}

fn gate_notes(summary: &CombatSearchV2BenchmarkSummary) -> Vec<&'static str> {
    let mut notes = vec![
        "gate compares whole-combat outcomes; it does not require stepwise action agreement",
        "coverage fields describe budget and frontier coverage; they are not pass/fail criteria by themselves",
    ];
    if summary.complete_candidate_missing > 0 {
        notes.push("missing complete candidates should be handled before tuning value preferences");
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_case_fails_missing_complete_candidate() {
        let case = assess_case_facts(CaseGateFacts {
            id: "case",
            coverage_status: SearchCoverageStatus::NodeBudgetLimited,
            best_complete_terminal: None,
            best_complete_final_hp: None,
            best_complete_potions_used: None,
            baseline_final_hp: Some(10),
            baseline_potions_used: Some(0),
            complete_candidate_verdict: None,
            nodes_expanded: 10,
            nodes_generated: 20,
            nodes_to_first_win: None,
            terminal_wins: 0,
            terminal_losses: 0,
            deadline_hit: false,
            node_budget_hit: true,
            unresolved_leaf_count: 1,
            max_actions_cut_count: 0,
            engine_step_limit_count: 0,
            potion_budget_cut_count: 0,
            high_fanout_pending_choice_states: 0,
            max_pending_choice_candidate_count: 0,
            max_legal_actions: 3,
            max_turn_generated_children: 3,
            same_effect_turn_sequence_groups: 0,
            order_sensitive_turn_sequence_groups: 0,
            duplicate_card_identity_states: 0,
            lineage_card_id_conflict_states: 0,
        });

        assert_eq!(case.status, CombatSearchV2BenchmarkGateStatus::Fail);
        assert!(case.reasons.contains(&"missing_complete_trajectory"));
        assert_eq!(case.primary_focus, "budget_or_rollout");
    }

    #[test]
    fn gate_case_passes_budget_limited_candidate_win_without_warning() {
        let case = assess_case_facts(CaseGateFacts {
            id: "case",
            coverage_status: SearchCoverageStatus::TimeBudgetLimited,
            best_complete_terminal: Some(SearchTerminalLabel::Win),
            best_complete_final_hp: Some(20),
            best_complete_potions_used: Some(0),
            baseline_final_hp: Some(15),
            baseline_potions_used: Some(0),
            complete_candidate_verdict: Some(CombatSearchV2BaselineVerdict::SearchBetter),
            nodes_expanded: 10,
            nodes_generated: 20,
            nodes_to_first_win: Some(5),
            terminal_wins: 1,
            terminal_losses: 0,
            deadline_hit: true,
            node_budget_hit: false,
            unresolved_leaf_count: 2,
            max_actions_cut_count: 0,
            engine_step_limit_count: 0,
            potion_budget_cut_count: 0,
            high_fanout_pending_choice_states: 0,
            max_pending_choice_candidate_count: 0,
            max_legal_actions: 5,
            max_turn_generated_children: 5,
            same_effect_turn_sequence_groups: 0,
            order_sensitive_turn_sequence_groups: 0,
            duplicate_card_identity_states: 0,
            lineage_card_id_conflict_states: 0,
        });

        assert_eq!(case.status, CombatSearchV2BenchmarkGateStatus::Pass);
        assert!(!case.reasons.contains(&"deadline_hit"));
        assert!(!case
            .reasons
            .contains(&"complete_candidate_worse_than_baseline"));
    }

    #[test]
    fn gate_focus_separates_safe_turn_sequence_candidates_from_order_sensitive_boundaries() {
        let mut facts = clean_warning_facts();
        facts.same_effect_turn_sequence_groups = 2;
        facts.order_sensitive_turn_sequence_groups = 4;

        let same_effect = assess_case_facts(facts.clone());

        assert_eq!(same_effect.primary_focus, "turn_sequence");
        assert_eq!(same_effect.status, CombatSearchV2BenchmarkGateStatus::Pass);
        assert!(same_effect.reasons.is_empty());

        facts.same_effect_turn_sequence_groups = 0;
        let order_sensitive = assess_case_facts(facts);

        assert_eq!(order_sensitive.primary_focus, "state_abstraction_boundary");
        assert_eq!(
            order_sensitive.status,
            CombatSearchV2BenchmarkGateStatus::Pass
        );
        assert!(order_sensitive.reasons.is_empty());
    }

    fn clean_warning_facts() -> CaseGateFacts<'static> {
        CaseGateFacts {
            id: "case",
            coverage_status: SearchCoverageStatus::TimeBudgetLimited,
            best_complete_terminal: Some(SearchTerminalLabel::Win),
            best_complete_final_hp: Some(20),
            best_complete_potions_used: Some(0),
            baseline_final_hp: Some(15),
            baseline_potions_used: Some(0),
            complete_candidate_verdict: Some(CombatSearchV2BaselineVerdict::SearchBetter),
            nodes_expanded: 10,
            nodes_generated: 20,
            nodes_to_first_win: Some(5),
            terminal_wins: 1,
            terminal_losses: 0,
            deadline_hit: true,
            node_budget_hit: false,
            unresolved_leaf_count: 2,
            max_actions_cut_count: 0,
            engine_step_limit_count: 0,
            potion_budget_cut_count: 0,
            high_fanout_pending_choice_states: 0,
            max_pending_choice_candidate_count: 0,
            max_legal_actions: 5,
            max_turn_generated_children: 5,
            same_effect_turn_sequence_groups: 0,
            order_sensitive_turn_sequence_groups: 0,
            duplicate_card_identity_states: 0,
            lineage_card_id_conflict_states: 0,
        }
    }
}
