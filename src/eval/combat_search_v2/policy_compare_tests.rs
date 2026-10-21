use super::*;

#[test]
fn comparison_summary_counts_verdicts_and_hp_delta() {
    let mut summary = CombatSearchV2RolloutPolicyComparisonSummary::default();
    observe_case(
        &mut summary,
        &CombatSearchV2RolloutPolicyComparisonCase {
            id: "case".to_string(),
            verdict: CombatSearchV2RolloutPolicyComparisonVerdict::RightBetter,
            right_minus_left_final_hp: Some(3),
            right_minus_left_hp_loss: Some(-3),
            right_minus_left_turns: Some(0),
            right_minus_left_cards_played: Some(1),
            right_minus_left_turn_plan_frontier_seeded_nodes: 0,
            left: run_summary(10),
            right: run_summary(13),
            first_action_diff: Some(CombatSearchV2RolloutPolicyFirstActionDiff {
                action_index: 0,
                left_action_id: None,
                left_action_key: None,
                left_action_debug: None,
                left_action_role: Some("end_turn"),
                right_action_id: None,
                right_action_key: None,
                right_action_debug: None,
                right_action_role: Some("damage_progress"),
                context: None,
            }),
        },
    );

    assert_eq!(summary.cases_compared, 1);
    assert_eq!(summary.right_better, 1);
    assert_eq!(summary.right_minus_left_final_hp_total, 3);
    assert_eq!(summary.right_minus_left_rollout_evaluations_total, 0);
    assert_eq!(summary.right_minus_left_rollout_terminal_wins_total, 0);
    assert_eq!(summary.right_minus_left_rollout_budget_skips_total, 0);
    assert_eq!(summary.first_action_diff_cases, 1);
    assert_eq!(run_summary(10).rollout_beam_width, 3);
    assert_eq!(run_summary(10).rollout_evaluations, 11);
    assert_eq!(run_summary(10).rollout_terminal_wins, 2);
    assert_eq!(run_summary(10).rollout_budget_skips, 4);
    assert_eq!(run_summary(10).rollout_turn_beam_extension_budget, 3);
    assert_eq!(run_summary(10).rollout_turn_beam_extensions, 2);
    assert_eq!(run_summary(10).rollout_turn_beam_extension_budget_skips, 5);
    assert_eq!(summary.first_diff_action_index_histogram.get("0"), Some(&1));
    assert_eq!(
        summary
            .right_better_right_role_histogram
            .get("damage_progress"),
        Some(&1)
    );
}

fn run_summary(final_hp: i32) -> CombatSearchV2RolloutPolicyComparisonRun {
    CombatSearchV2RolloutPolicyComparisonRun {
        policy: "test".to_string(),
        terminal: Some(SearchTerminalLabel::Win),
        coverage_status: SearchCoverageStatus::TimeBudgetLimited,
        complete_trajectory_found: true,
        final_hp: Some(final_hp),
        hp_loss: Some(0),
        turns: Some(1),
        potions_used: Some(0),
        cards_played: Some(1),
        action_count: Some(1),
        nodes_expanded: 1,
        nodes_generated: 1,
        nodes_to_first_win: Some(1),
        deadline_hit: true,
        node_budget_hit: false,
        turn_plan_frontier_seeded_nodes: 0,
        rollout_evaluations: 11,
        rollout_cache_hits: 7,
        rollout_budget_skips: 4,
        rollout_terminal_wins: 2,
        rollout_terminal_losses: 1,
        rollout_beam_width: 3,
        rollout_turn_beam_extension_budget: 3,
        rollout_turn_beam_extensions: 2,
        rollout_turn_beam_extension_budget_skips: 5,
    }
}

#[test]
fn comparison_summary_counts_turn_plan_frontier_seed_delta() {
    let mut summary = CombatSearchV2PolicyComparisonSummary::default();
    let mut left = run_summary(10);
    left.turn_plan_frontier_seeded_nodes = 0;
    let mut right = run_summary(10);
    right.turn_plan_frontier_seeded_nodes = 3;

    observe_case(
        &mut summary,
        &CombatSearchV2PolicyComparisonCase {
            id: "case".to_string(),
            verdict: CombatSearchV2PolicyComparisonVerdict::Tied,
            right_minus_left_final_hp: Some(0),
            right_minus_left_hp_loss: Some(0),
            right_minus_left_turns: Some(0),
            right_minus_left_cards_played: Some(0),
            right_minus_left_turn_plan_frontier_seeded_nodes: 3,
            left,
            right,
            first_action_diff: None,
        },
    );

    assert_eq!(
        summary.right_minus_left_turn_plan_frontier_seeded_nodes_total,
        3
    );
}

#[test]
fn comparison_summary_counts_rollout_budget_deltas() {
    let mut summary = CombatSearchV2PolicyComparisonSummary::default();
    let mut left = run_summary(10);
    left.rollout_evaluations = 10;
    left.rollout_terminal_wins = 1;
    left.rollout_budget_skips = 7;
    let mut right = run_summary(10);
    right.rollout_evaluations = 14;
    right.rollout_terminal_wins = 3;
    right.rollout_budget_skips = 2;

    observe_case(
        &mut summary,
        &CombatSearchV2PolicyComparisonCase {
            id: "case".to_string(),
            verdict: CombatSearchV2PolicyComparisonVerdict::Tied,
            right_minus_left_final_hp: Some(0),
            right_minus_left_hp_loss: Some(0),
            right_minus_left_turns: Some(0),
            right_minus_left_cards_played: Some(0),
            right_minus_left_turn_plan_frontier_seeded_nodes: 0,
            left,
            right,
            first_action_diff: None,
        },
    );

    assert_eq!(summary.right_minus_left_rollout_evaluations_total, 4);
    assert_eq!(summary.right_minus_left_rollout_terminal_wins_total, 2);
    assert_eq!(summary.right_minus_left_rollout_budget_skips_total, -5);
}
