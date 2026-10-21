use crate::ai::combat_search_v2::{
    CombatSearchV2ActionTrace, CombatSearchV2Report, CombatSearchV2TurnSegmentReport,
};

use super::combat_candidate_line::CombatCandidateLine;
use super::view_model::client_input_hint;

pub(super) fn render_search_application(
    report: &CombatSearchV2Report,
    actions: &[CombatSearchV2ActionTrace],
    selected_line: &CombatCandidateLine,
    replay_applied_count: usize,
) -> String {
    let trajectory = report
        .best_win_trajectory
        .as_ref()
        .expect("caller only renders after selecting a complete trajectory");
    let mut lines = vec![
        "Search combat applied complete winning candidate.".to_string(),
        format!(
            "  coverage_status={:?} reliability={}",
            report.outcome.coverage_status, report.evidence_reliability.reliability
        ),
        render_search_policy_summary(report),
        render_search_diagnostics_summary(report),
        render_search_performance_summary(report),
        render_policy_evidence_summary(report),
        format!("  coverage_reason={}", report.outcome.coverage_reason),
        format!(
            "  selected_line source={} terminal={:?} final_hp={} hp_loss={} turns={} cards_played={} potions_used={} potions_discarded={} replay_applied={}",
            selected_line.source.label(),
            selected_line.terminal,
            selected_line.final_hp,
            selected_line.hp_loss,
            selected_line.turns,
            selected_line.cards_played,
            selected_line.potions_used,
            selected_line.potions_discarded,
            replay_applied_count
        ),
        format!(
            "  selected_assumptions={}",
            selected_line.assumption_labels().join(",")
        ),
        format!(
            "  original_final_hp={} original_hp_loss={} original_turns={} original_cards_played={} original_potions_used={} original_potions_discarded={}",
            trajectory.final_hp,
            trajectory.hp_loss,
            trajectory.turns,
            trajectory.cards_played,
            trajectory.potions_used,
            trajectory.potions_discarded
        ),
        format!(
            "  nodes_expanded={} nodes_generated={} nodes_to_first_win={:?}",
            report.stats.nodes_expanded,
            report.stats.nodes_generated,
            report.stats.nodes_to_first_win
        ),
        format!(
            "  rollout_policy={} rollouts={} rollout_wins={} rollout_skips={}",
            report.rollout.policy,
            report.rollout.evaluations,
            report.rollout.terminal_wins,
            report.rollout.budget_skips
        ),
        format!(
            "  action_count={} potion_policy={}",
            actions.len(),
            report.search_policy.potion_policy
        ),
    ];
    for action in actions.iter().take(12) {
        lines.push(format!(
            "    {} | {} | {}",
            action.step_index,
            client_input_hint(&action.input),
            action.action_key
        ));
    }
    if actions.len() > 12 {
        lines.push(format!("    ... {} more actions", actions.len() - 12));
    }
    lines.join("\n")
}

pub(super) fn render_complete_line_solver_application(
    report: &CombatSearchV2Report,
    actions: &[CombatSearchV2ActionTrace],
    selected_line: &CombatCandidateLine,
    replay_applied_count: usize,
) -> String {
    let mut lines = vec![
        format!(
            "Complete combat line fallback applied winning candidate. source={}",
            selected_line.source.label()
        ),
        format!(
            "  previous_search coverage_status={:?} reliability={}",
            report.outcome.coverage_status, report.evidence_reliability.reliability
        ),
        format!(
            "  selected_line source={} terminal={:?} final_hp={} hp_loss={} turns={} cards_played={} replay_applied={}",
            selected_line.source.label(),
            selected_line.terminal,
            selected_line.final_hp,
            selected_line.hp_loss,
            selected_line.turns,
            selected_line.cards_played,
            replay_applied_count
        ),
    ];
    for action in actions.iter().take(12) {
        lines.push(format!(
            "    {} | {} | {}",
            action.step_index,
            client_input_hint(&action.input),
            action.action_key
        ));
    }
    if actions.len() > 12 {
        lines.push(format!("    ... {} more actions", actions.len() - 12));
    }
    lines.join("\n")
}

pub(super) fn render_segment_application(
    search_report: &CombatSearchV2Report,
    segment_report: &CombatSearchV2TurnSegmentReport,
    rejection_result: &'static str,
) -> String {
    let trajectory = segment_report
        .selected
        .as_ref()
        .expect("caller only renders after selecting a segment");
    let mut lines = vec![
        "Search combat applied partial turn segment.".to_string(),
        format!("  behavior_label={}", segment_report.behavior_label),
        format!("  source={}", segment_report.source),
        format!("  original_search_result={rejection_result}"),
        format!(
            "  segment_bucket={} stop_reason={} candidate_count={} nodes_expanded={} nodes_generated={}",
            segment_report.selected_bucket.unwrap_or("unknown"),
            segment_report.selected_stop_reason.unwrap_or("unknown"),
            segment_report.candidate_count,
            segment_report.nodes_expanded,
            segment_report.nodes_generated
        ),
        format!(
            "  segment_terminal={:?} final_hp={} hp_loss={} turns={} cards_played={} potions_used={}",
            trajectory.terminal,
            trajectory.final_hp,
            trajectory.hp_loss,
            trajectory.turns,
            trajectory.cards_played,
            trajectory.potions_used
        ),
        format!(
            "  search_coverage={:?} reliability={}",
            search_report.outcome.coverage_status, search_report.evidence_reliability.reliability
        ),
        render_search_policy_summary(search_report),
        render_search_performance_summary(search_report),
        render_policy_evidence_summary(search_report),
        "  terminal_claim=none; this is an exact applied prefix, not a complete-win proof"
            .to_string(),
        format!("  action_count={}", trajectory.actions.len()),
    ];
    for action in trajectory.actions.iter().take(12) {
        lines.push(format!(
            "    {} | {} | {}",
            action.step_index,
            client_input_hint(&action.input),
            action.action_key
        ));
    }
    if trajectory.actions.len() > 12 {
        lines.push(format!(
            "    ... {} more actions",
            trajectory.actions.len() - 12
        ));
    }
    lines.join("\n")
}

pub(super) fn render_search_policy_summary(report: &CombatSearchV2Report) -> String {
    format!(
        "  turn_plan_policy={} rollout_policy={}",
        report.search_policy.turn_plan_policy, report.rollout.policy
    )
}

pub(super) fn render_search_diagnostics_summary(report: &CombatSearchV2Report) -> String {
    format!(
        "  search_diagnostics=frontier_work_items={} pending_action_work={} unresolved_leaf={} max_actions_cut={} engine_step_cut={} potion_budget_cut={} turn_plan_observed={} turn_plan_seeded={} pending_states={} pending_high_fanout={} rollout_budget_skips={}",
        report.frontier.remaining_work_items,
        report.frontier.pending_choice_work_items,
        report.frontier.unresolved_leaf_count,
        report.frontier.max_actions_cut_count,
        report.frontier.engine_step_limit_count,
        report.frontier.potion_budget_cut_count,
        report.diagnostics.turn_plan.root_states_observed,
        report.diagnostics.turn_plan.frontier_seeded_nodes,
        report.diagnostics.pending_choice.pending_choice_states,
        report.diagnostics.pending_choice.high_fanout_states,
        report.rollout.budget_skips,
    )
}

pub(super) fn render_search_performance_summary(report: &CombatSearchV2Report) -> String {
    format!(
        "  search_performance=elapsed_ms={} total_us={} unattributed_us={} frontier_pop_calls={} frontier_pop_us={} pre_expand_us={} expansion_us={} child_bookkeeping_us={} engine_step_calls={} engine_step_us={} rollout_calls={} root_rollout_calls={} child_rollout_calls={} deferred_child_rollout_calls={} turn_plan_seed_rollout_calls={} deferred_child_nodes={} deferred_child_requeues={} rollout_cache=hits/queries/misses/inserts:{}/{}/{}/{} rollout_budget_skips={} max_eval_budget_skips={} deadline_budget_skips={} rollout_truncated={} rollout_terminal_wins={} rollout_inner_us=iters:{} cache_lookup:{} policy_total:{} phase:{} legal:{} choose:{} order:{} probe:{} probe_calls:{} probe_eval:{} probe_reuse:{} probe_engine:{} probe_phase:{} probe_facts:{} engine:{} build:{} terminal_child_rollout_skips={} terminal_turn_plan_seed_rollout_skips={} turn_local_dominance_rollout_skips={} rollout_us={} turn_plan_seed_calls={} turn_plan_seed_us={} shadow_audit_us={} root_turn_plan_diag_us={}",
        report.stats.elapsed_ms,
        report.performance.total_elapsed_us,
        report.performance.unattributed_elapsed_us,
        report.performance.frontier_pop_calls,
        report.performance.frontier_pop_elapsed_us,
        report.performance.pre_expand_elapsed_us,
        report.performance.expansion_elapsed_us,
        report.performance.child_bookkeeping_elapsed_us,
        report.performance.engine_step_calls,
        report.performance.engine_step_elapsed_us,
        report.performance.rollout_estimate_calls,
        report.performance.root_rollout_estimate_calls,
        report.performance.child_rollout_estimate_calls,
        report.performance.deferred_child_rollout_estimate_calls,
        report.performance.turn_plan_seed_rollout_estimate_calls,
        report.performance.deferred_child_rollout_nodes,
        report.performance.deferred_child_rollout_requeues,
        report.rollout.cache_hits,
        report.rollout.cache_queries,
        report.rollout.cache_misses,
        report.rollout.cache_inserts,
        report.rollout.budget_skips,
        report.rollout.max_evaluation_budget_skips,
        report.rollout.deadline_budget_skips,
        report.rollout.truncated_rollouts,
        report.rollout.terminal_wins,
        report.rollout.performance.no_potion_iterations,
        report.rollout.performance.cache_lookup_us,
        report.rollout.performance.policy_dispatch_us,
        report.rollout.performance.no_potion_phase_profile_us,
        report.rollout.performance.no_potion_legal_actions_us,
        report.rollout.performance.no_potion_choose_action_us,
        report.rollout.performance.no_potion_choose_ordering_us,
        report.rollout.performance.no_potion_probe_us,
        report.rollout.performance.no_potion_probe_score_calls,
        report.rollout.performance.no_potion_probe_actions_evaluated,
        report.rollout.performance.no_potion_probe_step_reuses,
        report.rollout.performance.no_potion_probe_engine_step_us,
        report.rollout.performance.no_potion_probe_phase_profile_us,
        report.rollout.performance.no_potion_probe_action_facts_us,
        report.rollout.performance.no_potion_engine_step_us,
        report.rollout.performance.no_potion_child_build_us,
        report.performance.terminal_child_rollout_skips,
        report.performance.terminal_turn_plan_seed_rollout_skips,
        report.performance.turn_local_dominance_rollout_skips,
        report.performance.rollout_estimate_elapsed_us,
        report.performance.turn_plan_frontier_seed_calls,
        report.performance.turn_plan_frontier_seed_elapsed_us,
        report.performance.shadow_audit_elapsed_us,
        report.performance.root_turn_plan_diagnostics_elapsed_us,
    )
}

pub(super) fn render_policy_evidence_summary(report: &CombatSearchV2Report) -> String {
    format!("  {}", report.policy_evidence.machine_summary())
}
