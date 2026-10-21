use crate::ai::combat_search_v2::{
    CombatSearchV2ActionAccessMechanicsFacts, CombatSearchV2ActionCardFacts,
    CombatSearchV2ActionDerivedMechanicsFacts, CombatSearchV2ActionDirectMechanicsFacts,
    CombatSearchV2ActionExactDeltaFacts, CombatSearchV2ActionFacts,
    CombatSearchV2ActionImmediateFacts, CombatSearchV2ActionMechanicsFacts,
    CombatSearchV2ActionReactiveMechanicsFacts, CombatSearchV2ActionResourceTimingFacts,
    CombatSearchV2ActionTargetFacts, CombatSearchV2ActionTrace, CombatSearchV2EnemySummary,
    CombatSearchV2StateSummary, CombatSearchV2TurnPlanProbeCandidateReport,
    CombatSearchV2TurnPlanProbeStepReport, SearchTerminalLabel,
};
use crate::content::cards::{CardTarget, CardType};
use crate::state::core::ClientInput;

use super::*;

#[test]
fn tactical_trace_summarizes_mechanical_turn_plan_deltas() {
    let plan = probe_plan(vec![
        probe_step(
            0,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
            Some(card_facts("Feel No Pain", "FeelNoPain", CardType::Power, 1)),
            exact_delta(0, 0, -1, -1, 0, 1, 0, -12, 0),
            mechanics_delta(0, 0, 0, 0, 0),
        ),
        probe_step(
            1,
            ClientInput::PlayCard {
                card_index: 1,
                target: Some(1),
            },
            Some(card_facts("True Grit", "TrueGrit", CardType::Skill, 1)),
            exact_delta(0, 9, -1, -1, 0, 0, 1, 0, 0),
            mechanics_delta(0, 0, 0, 0, 0),
        ),
        probe_step(
            2,
            ClientInput::UsePotion {
                potion_index: 0,
                target: Some(1),
            },
            None,
            exact_delta(-5, 0, 0, 0, 0, 0, 0, -20, 0),
            mechanics_delta(0, 0, 0, 0, 0),
        ),
    ]);

    let trace = tactical_trace_for_plan_report(&plan);

    assert_eq!(trace.action_count, 3);
    assert_eq!(trace.cards_played, 2);
    assert_eq!(trace.potions_used, 1);
    assert_eq!(trace.powers_played, 1);
    assert_eq!(trace.damage_done, 32);
    assert_eq!(trace.block_gained_proxy, 9);
    assert_eq!(trace.energy_spent_proxy, 2);
    assert_eq!(trace.exhaust_delta, 1);
    assert_eq!(trace.player_hp_lost, 5);
}

#[test]
fn selected_vs_best_target_reports_current_ordering_gap() {
    let first = lab_candidate(
        0,
        "first",
        tactical_plan_with_damage(0, "Strike", 6),
        target_with_complete_win(30, 10, 7, 0, 12, 10),
    );
    let best = lab_candidate(
        1,
        "best",
        tactical_plan_with_damage(1, "Bash", 9),
        target_with_complete_win(40, 0, 5, 0, 9, 8),
    );
    let candidates = vec![first, best];

    let comparison = selected_vs_best_target_report(&candidates).expect("comparison should exist");

    assert!(!comparison.same_plan);
    assert_eq!(comparison.current_first.plan_index, 0);
    assert_eq!(comparison.best_by_child_target.plan_index, 1);
    assert_eq!(
        comparison.delta_best_minus_current_first.final_hp_delta,
        Some(10)
    );
    assert_eq!(
        comparison.delta_best_minus_current_first.hp_loss_delta,
        Some(-10)
    );
    assert_eq!(
        comparison.delta_best_minus_current_first.turn_delta,
        Some(-2)
    );
    assert_eq!(
        comparison
            .delta_best_minus_current_first
            .nodes_expanded_delta,
        Some(-20)
    );
    assert_eq!(comparison.best_by_child_target.tactical.damage_done, 9);
}

#[test]
fn baseline_vs_best_guided_prefix_reports_search_outcome_delta() {
    let baseline = child_search_with_best_complete(target_with_complete_win(35, 8, 5, 0, 12, 12));
    let weaker = lab_candidate(
        0,
        "weaker",
        tactical_plan_with_damage(0, "Strike", 6),
        target_with_complete_win(30, 13, 5, 0, 12, 12),
    );
    let guided = lab_candidate(
        1,
        "guided",
        tactical_plan_with_damage(1, "Defend", 0),
        target_with_complete_win(41, 2, 6, 0, 15, 16),
    );
    let candidates = vec![weaker, guided];

    let comparison = search_vs_best_guided_prefix_report(
        &baseline,
        "reference_whole_combat_search",
        &candidates,
        None,
    )
    .expect("comparison should exist");

    assert_eq!(comparison.verdict, "guided_better");
    assert_eq!(comparison.verdict_basis, "final_hp");
    assert_eq!(
        comparison.guided_prefix_selection_basis,
        "root_composed_objective"
    );
    assert_eq!(comparison.baseline.final_hp, Some(35));
    assert_eq!(
        comparison.baseline.first_action_key.as_deref(),
        Some("test-first-action-12")
    );
    assert_eq!(comparison.best_guided_prefix.plan_index, 1);
    assert_eq!(
        comparison.delta_guided_minus_baseline.final_hp_delta,
        Some(6)
    );
    assert_eq!(
        comparison.delta_guided_minus_baseline.hp_loss_delta,
        Some(1)
    );
    assert_eq!(comparison.delta_guided_minus_baseline.turn_delta, Some(1));
    assert_eq!(
        comparison.delta_guided_minus_baseline.action_count_delta,
        Some(4)
    );
    assert_eq!(
        comparison
            .action_sequence_alignment
            .common_prefix_action_count,
        0
    );
    assert_eq!(
        comparison
            .action_sequence_alignment
            .baseline_next_action_key
            .as_deref(),
        Some("test-first-action-12")
    );
    assert_eq!(
        comparison
            .action_sequence_alignment
            .guided_next_action_key
            .as_deref(),
        Some("action-0")
    );
    assert_eq!(
        comparison.action_sequence_alignment.first_divergence_kind,
        "diverged"
    );
}

#[test]
fn baseline_comparison_selects_guided_prefix_by_root_objective_not_child_local_hp_loss() {
    let baseline = child_search_with_best_complete(target_with_complete_win(40, 10, 5, 0, 12, 12));
    let root_better = lab_candidate(
        0,
        "root-better",
        tactical_plan_with_damage(0, "Strike", 6),
        target_with_complete_win(40, 15, 4, 0, 10, 10),
    );
    let child_local_bait = lab_candidate(
        1,
        "child-local-bait",
        tactical_plan_with_damage(1, "Defend", 0),
        target_with_complete_win(40, 1, 8, 0, 18, 18),
    );
    let candidates = vec![root_better, child_local_bait];

    assert_eq!(
        compare_targets(&candidates[1].target, &candidates[0].target),
        Ordering::Greater,
        "child-local target ranking should prefer the bait candidate in this fixture"
    );

    let comparison = search_vs_best_guided_prefix_report(
        &baseline,
        "reference_whole_combat_search",
        &candidates,
        None,
    )
    .expect("comparison should exist");

    assert_eq!(
        comparison.guided_prefix_selection_basis,
        "root_composed_objective"
    );
    assert_eq!(comparison.best_guided_prefix.plan_index, 0);
    assert_eq!(comparison.verdict, "guided_better");
    assert_eq!(comparison.verdict_basis, "turns");
}

#[test]
fn reference_turn_prefix_candidate_coverage_reports_exact_and_partial_matches() {
    let reference = vec![
        "defend".to_string(),
        "bash".to_string(),
        "combat/end_turn".to_string(),
        "next-turn-action".to_string(),
    ];
    let mut partial = lab_candidate(
        0,
        "partial",
        tactical_plan_with_damage(0, "Strike", 6),
        target_with_complete_win(40, 10, 5, 0, 12, 12),
    );
    partial.plan.action_keys = vec![
        "defend".to_string(),
        "strike".to_string(),
        "combat/end_turn".to_string(),
    ];
    let mut exact = lab_candidate(
        1,
        "exact",
        tactical_plan_with_damage(1, "Bash", 9),
        target_with_complete_win(40, 10, 5, 0, 12, 12),
    );
    exact.plan.action_keys = vec![
        "defend".to_string(),
        "bash".to_string(),
        "combat/end_turn".to_string(),
    ];

    let coverage = reference_turn_prefix_candidate_coverage(&reference, &[partial, exact], None);

    assert_eq!(coverage.reference_prefix_action_count, 3);
    assert_eq!(coverage.exact_match_plan_index, Some(1));
    assert_eq!(coverage.longest_prefix_match_plan_index, Some(1));
    assert_eq!(coverage.longest_prefix_match_action_count, 3);
}

#[test]
fn benchmark_summary_counts_guided_prefix_verdicts() {
    let mut summary = CombatTurnPlanGuidanceLabBenchmarkSummaryV1::default();
    record_guided_prefix_baseline_verdict_count(
        &mut summary,
        &lab_summary_with_guided_verdict("guided_better"),
    );
    record_guided_prefix_baseline_verdict_count(
        &mut summary,
        &lab_summary_with_guided_verdict("guided_tied"),
    );
    record_guided_prefix_baseline_verdict_count(
        &mut summary,
        &lab_summary_with_guided_verdict("guided_worse"),
    );
    record_guided_prefix_baseline_verdict_count(
        &mut summary,
        &CombatTurnPlanGuidanceLabSummaryV1::default(),
    );

    assert_eq!(summary.cases_guided_prefix_better_than_baseline, 1);
    assert_eq!(summary.cases_guided_prefix_tied_with_baseline, 1);
    assert_eq!(summary.cases_guided_prefix_worse_than_baseline, 1);
    assert_eq!(summary.cases_without_guided_prefix_baseline_comparison, 1);
}

fn lab_candidate(
    plan_index: usize,
    _action_key: &str,
    plan: CombatSearchV2TurnPlanProbeCandidateReport,
    child_best: CombatSearchGuidanceLabTrajectoryV1,
) -> CombatTurnPlanGuidanceLabCandidateV1 {
    CombatTurnPlanGuidanceLabCandidateV1 {
        tactical: tactical_trace_for_plan_report(&plan),
        plan,
        end_fingerprints: fingerprint_report(plan_index),
        child_search: Some(CombatSearchGuidanceLabChildSearchV1 {
            outcome: crate::ai::combat_search_v2::CombatSearchV2OutcomeReport {
                coverage_status: crate::ai::combat_search_v2::SearchCoverageStatus::Exhaustive,
                coverage_reason: "test".to_string(),
                complete_trajectory_found: true,
                complete_win_found: true,
                exhaustive: true,
            },
            best_complete: Some(child_best.clone()),
            best_frontier: Some(child_best.clone()),
            final_state: None,
            nodes_expanded: child_best.action_count as u64 * 10,
            nodes_generated: child_best.action_count as u64 * 20,
            terminal_wins: 1,
            elapsed_ms: 0,
        }),
        target: CombatSearchGuidanceLabTargetV1 {
            target_kind: "root_turn_plan_child_search_rank",
            source: "bounded_child_search_best_complete",
            terminal: SearchTerminalLabel::Win,
            complete_win: true,
            post_root_player_hp: 50,
            child_search_hp_loss: Some(child_best.hp_loss),
            final_hp: Some(child_best.final_hp),
            nodes_expanded: Some(child_best.action_count as u64 * 10),
            limitations: vec![],
        },
    }
}

fn child_search_with_best_complete(
    best_complete: CombatSearchGuidanceLabTrajectoryV1,
) -> CombatSearchGuidanceLabChildSearchV1 {
    CombatSearchGuidanceLabChildSearchV1 {
        outcome: crate::ai::combat_search_v2::CombatSearchV2OutcomeReport {
            coverage_status: crate::ai::combat_search_v2::SearchCoverageStatus::Exhaustive,
            coverage_reason: "test".to_string(),
            complete_trajectory_found: true,
            complete_win_found: true,
            exhaustive: true,
        },
        best_complete: Some(best_complete.clone()),
        best_frontier: Some(best_complete.clone()),
        final_state: None,
        nodes_expanded: best_complete.action_count as u64 * 10,
        nodes_generated: best_complete.action_count as u64 * 20,
        terminal_wins: 1,
        elapsed_ms: 0,
    }
}

fn lab_summary_with_guided_verdict(verdict: &'static str) -> CombatTurnPlanGuidanceLabSummaryV1 {
    CombatTurnPlanGuidanceLabSummaryV1 {
        baseline_vs_best_guided_prefix: Some(CombatTurnPlanGuidanceBaselineComparisonV1 {
            verdict,
            verdict_basis: "test",
            guided_prefix_selection_basis: "root_composed_objective",
            reference_turn_prefix_candidate_coverage: CombatTurnPlanGuidanceCandidateCoverageV1 {
                comparison_scope: "reference_first_turn_prefix_vs_candidate_turn_plans",
                candidate_count: 0,
                preselection_candidate_count: 0,
                reference_prefix_action_count: 0,
                reference_prefix_action_keys: vec![],
                exact_match_plan_index: None,
                longest_prefix_match_plan_index: None,
                longest_prefix_match_action_count: 0,
                preselection_exact_match_rank: None,
                preselection_exact_match_selected_plan_index: None,
                preselection_exact_match_drop_reason: None,
                preselection_longest_prefix_rank: None,
                preselection_longest_prefix_action_count: 0,
                preselection_longest_prefix_drop_reason: None,
            },
            baseline: CombatTurnPlanGuidanceSearchSnapshotV1 {
                source: "baseline_whole_combat_search",
                terminal: SearchTerminalLabel::Win,
                complete_win: true,
                final_hp: Some(40),
                hp_loss: Some(0),
                turns: Some(1),
                potions_used: Some(0),
                cards_played: Some(1),
                action_count: Some(1),
                first_action_key: None,
                action_keys_preview: vec![],
                nodes_expanded: 1,
                nodes_generated: 1,
                terminal_wins: 1,
                elapsed_ms: 0,
            },
            best_guided_prefix: CombatTurnPlanGuidancePlanSnapshotV1 {
                plan_index: 0,
                first_action_key: None,
                action_keys_preview: vec![],
                target_source: "bounded_child_search_best_complete",
                terminal: SearchTerminalLabel::Win,
                complete_win: true,
                final_hp: Some(40),
                hp_loss: Some(0),
                turns: Some(1),
                potions_used: Some(0),
                cards_played: Some(1),
                action_count: Some(1),
                nodes_expanded: Some(1),
                tactical: CombatTurnPlanTacticalTraceV1::default(),
            },
            delta_guided_minus_baseline: CombatTurnPlanGuidanceOutcomeDeltaV1 {
                final_hp_delta: Some(0),
                hp_loss_delta: Some(0),
                turn_delta: Some(0),
                potions_used_delta: Some(0),
                cards_played_delta: Some(0),
                action_count_delta: Some(0),
                nodes_expanded_delta: Some(0),
            },
            action_sequence_alignment: CombatTurnPlanGuidanceActionSequenceAlignmentV1 {
                comparison_scope: "baseline_best_complete_preview_vs_guided_prefix",
                common_prefix_action_count: 0,
                baseline_action_count: Some(1),
                guided_prefix_action_count: 0,
                baseline_next_action_key: None,
                guided_next_action_key: None,
                first_divergence_kind: "identical_available_preview",
                baseline_action_keys_preview: vec![],
                guided_prefix_action_keys: vec![],
            },
        }),
        budgeted_root_vs_best_guided_prefix: Some(CombatTurnPlanGuidanceBaselineComparisonV1 {
            verdict,
            verdict_basis: "test",
            guided_prefix_selection_basis: "root_composed_objective",
            reference_turn_prefix_candidate_coverage: CombatTurnPlanGuidanceCandidateCoverageV1 {
                comparison_scope: "reference_first_turn_prefix_vs_candidate_turn_plans",
                candidate_count: 0,
                preselection_candidate_count: 0,
                reference_prefix_action_count: 0,
                reference_prefix_action_keys: vec![],
                exact_match_plan_index: None,
                longest_prefix_match_plan_index: None,
                longest_prefix_match_action_count: 0,
                preselection_exact_match_rank: None,
                preselection_exact_match_selected_plan_index: None,
                preselection_exact_match_drop_reason: None,
                preselection_longest_prefix_rank: None,
                preselection_longest_prefix_action_count: 0,
                preselection_longest_prefix_drop_reason: None,
            },
            baseline: CombatTurnPlanGuidanceSearchSnapshotV1 {
                source: "budgeted_root_same_budget_search",
                terminal: SearchTerminalLabel::Win,
                complete_win: true,
                final_hp: Some(40),
                hp_loss: Some(0),
                turns: Some(1),
                potions_used: Some(0),
                cards_played: Some(1),
                action_count: Some(1),
                first_action_key: None,
                action_keys_preview: vec![],
                nodes_expanded: 1,
                nodes_generated: 1,
                terminal_wins: 1,
                elapsed_ms: 0,
            },
            best_guided_prefix: CombatTurnPlanGuidancePlanSnapshotV1 {
                plan_index: 0,
                first_action_key: None,
                action_keys_preview: vec![],
                target_source: "bounded_child_search_best_complete",
                terminal: SearchTerminalLabel::Win,
                complete_win: true,
                final_hp: Some(40),
                hp_loss: Some(0),
                turns: Some(1),
                potions_used: Some(0),
                cards_played: Some(1),
                action_count: Some(1),
                nodes_expanded: Some(1),
                tactical: CombatTurnPlanTacticalTraceV1::default(),
            },
            delta_guided_minus_baseline: CombatTurnPlanGuidanceOutcomeDeltaV1 {
                final_hp_delta: Some(0),
                hp_loss_delta: Some(0),
                turn_delta: Some(0),
                potions_used_delta: Some(0),
                cards_played_delta: Some(0),
                action_count_delta: Some(0),
                nodes_expanded_delta: Some(0),
            },
            action_sequence_alignment: CombatTurnPlanGuidanceActionSequenceAlignmentV1 {
                comparison_scope: "baseline_best_complete_preview_vs_guided_prefix",
                common_prefix_action_count: 0,
                baseline_action_count: Some(1),
                guided_prefix_action_count: 0,
                baseline_next_action_key: None,
                guided_next_action_key: None,
                first_divergence_kind: "identical_available_preview",
                baseline_action_keys_preview: vec![],
                guided_prefix_action_keys: vec![],
            },
        }),
        ..CombatTurnPlanGuidanceLabSummaryV1::default()
    }
}

fn fingerprint_report(plan_index: usize) -> CombatSearchV2InputFingerprintReport {
    CombatSearchV2InputFingerprintReport {
        boundary: crate::eval::fingerprint::DecisionBoundaryFingerprintV3 {
            engine_state: "CombatPlayerTurn".to_string(),
            decision_kind: "combat".to_string(),
            terminal: crate::sim::combat::CombatTerminal::Unresolved,
            stable_boundary: true,
            turn_count: 1,
        },
        public_observation_hash: format!("public-{plan_index}"),
        legal_input_language_hash: format!("language-{plan_index}"),
        action_enumeration_domain_hash: format!("domain-{plan_index}"),
        exact_state_hash: format!("hash-{plan_index}"),
        stable_outcome_hash: Some(format!("stable-{plan_index}")),
        rng_boundary_status: crate::eval::fingerprint::RngFingerprintStatus::Complete,
        rng_boundary_stream_count: 0,
        rng_boundary_digest: "empty".to_string(),
    }
}

fn target_with_complete_win(
    final_hp: i32,
    hp_loss: i32,
    turns: u32,
    potions_used: u32,
    cards_played: u32,
    action_count: usize,
) -> CombatSearchGuidanceLabTrajectoryV1 {
    CombatSearchGuidanceLabTrajectoryV1 {
        terminal: SearchTerminalLabel::Win,
        estimated: false,
        first_action_key: Some(format!("test-first-action-{action_count}")),
        action_keys_preview: vec![format!("test-first-action-{action_count}")],
        final_hp,
        hp_loss,
        turns,
        potions_used,
        potions_discarded: 0,
        cards_played,
        action_count,
    }
}

fn tactical_plan_with_damage(
    plan_index: usize,
    card_name: &'static str,
    damage: i32,
) -> CombatSearchV2TurnPlanProbeCandidateReport {
    probe_plan_with_index(
        plan_index,
        vec![probe_step(
            0,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
            Some(card_facts(card_name, card_name, CardType::Attack, 1)),
            exact_delta(0, 0, -1, -1, 0, 1, 0, -damage, 0),
            mechanics_delta(0, 0, 0, 0, 0),
        )],
    )
}

fn probe_plan(
    steps: Vec<CombatSearchV2TurnPlanProbeStepReport>,
) -> CombatSearchV2TurnPlanProbeCandidateReport {
    probe_plan_with_index(0, steps)
}

fn probe_plan_with_index(
    plan_index: usize,
    steps: Vec<CombatSearchV2TurnPlanProbeStepReport>,
) -> CombatSearchV2TurnPlanProbeCandidateReport {
    CombatSearchV2TurnPlanProbeCandidateReport {
        plan_index,
        bucket: "balanced",
        stop_reason: "next_turn",
        outcome_class: "unresolved",
        survival_bucket: "safe",
        progress_bucket: "race_even",
        action_count: steps.len(),
        first_action_key: steps.first().map(|step| step.action.action_key.clone()),
        action_keys: steps
            .iter()
            .map(|step| step.action.action_key.clone())
            .collect(),
        actions: steps.iter().map(|step| step.action.clone()).collect(),
        action_facts: steps.iter().map(|step| step.action_facts.clone()).collect(),
        steps,
        eval_final_hp: 50,
        eval_risk_margin: 0,
        eval_enemy_progress: 0,
        end_state: state_summary(50, 0, 3, 100),
    }
}

fn probe_step(
    step_index: usize,
    input: ClientInput,
    card: Option<CombatSearchV2ActionCardFacts>,
    exact_one_step_delta: CombatSearchV2ActionExactDeltaFacts,
    mechanics: CombatSearchV2ActionMechanicsFacts,
) -> CombatSearchV2TurnPlanProbeStepReport {
    CombatSearchV2TurnPlanProbeStepReport {
        step_index,
        action: CombatSearchV2ActionTrace {
            step_index,
            action_id: step_index,
            action_key: format!("action-{step_index}"),
            action_debug: format!("action {step_index}"),
            input,
        },
        action_facts: CombatSearchV2ActionFacts {
            action_kind: "test",
            card,
            target: Some(CombatSearchV2ActionTargetFacts {
                target_slot: 0,
                entity_id: 1,
                enemy_id: "Cultist".to_string(),
                hp: 100,
                block: 0,
                visible_incoming_damage: 6,
                vulnerable: 0,
                weak: 0,
                strength: 0,
                timed_enemy_threat: None,
                attack_retaliation: None,
            }),
            pending_choice_selection: None,
            immediate: CombatSearchV2ActionImmediateFacts::default(),
            mechanics,
            exact_one_step_delta,
        },
        exact_state_hash_kind: "exact",
        state_before_exact_state_hash: format!("before-{step_index}"),
        state_after_exact_state_hash: format!("after-{step_index}"),
        state_before: state_summary(50, 0, 3, 100),
        state_after: state_summary(50, 0, 2, 100),
        engine_steps: 1,
    }
}

fn card_facts(
    name: &'static str,
    card_id: &str,
    card_type: CardType,
    cost_for_turn: i32,
) -> CombatSearchV2ActionCardFacts {
    CombatSearchV2ActionCardFacts {
        hand_index: 0,
        lineage: crate::test_support::scenario_lineage(1),
        card_id: card_id.to_string(),
        name,
        upgraded: false,
        card_type,
        definition_target: CardTarget::Enemy,
        effective_target: CardTarget::Enemy,
        cost_for_turn,
        base_cost: cost_for_turn as i8,
        evaluated_damage: 0,
        evaluated_block: 0,
        evaluated_magic: 0,
        exhaust: false,
        ethereal: false,
        innate: false,
    }
}

fn exact_delta(
    player_hp_delta: i32,
    player_block_delta: i32,
    energy_delta: i32,
    hand_delta: i32,
    draw_delta: i32,
    discard_delta: i32,
    exhaust_delta: i32,
    total_enemy_hp_delta: i32,
    total_enemy_block_delta: i32,
) -> CombatSearchV2ActionExactDeltaFacts {
    CombatSearchV2ActionExactDeltaFacts {
        status: "ok",
        terminal: SearchTerminalLabel::Unresolved,
        engine_steps: 1,
        player_hp_delta,
        player_block_delta,
        player_strength_delta: 0,
        energy_delta,
        hand_delta,
        draw_delta,
        discard_delta,
        exhaust_delta,
        limbo_delta: 0,
        queued_cards_delta: 0,
        total_enemy_hp_delta,
        total_enemy_block_delta,
        pending_choice_present: false,
        pending_choice_estimated_action_fanout: 0,
    }
}

fn mechanics_delta(
    visible_attack_mitigation_hint: i32,
    player_strength_gain: i32,
    player_temporary_strength_gain: i32,
    reactive_player_hp_loss: i32,
    reactive_bad_draw_cards: i32,
) -> CombatSearchV2ActionMechanicsFacts {
    CombatSearchV2ActionMechanicsFacts {
        direct: CombatSearchV2ActionDirectMechanicsFacts {
            visible_attack_mitigation_hint,
            player_strength_gain,
            player_temporary_strength_gain,
            ..CombatSearchV2ActionDirectMechanicsFacts::default()
        },
        reactive: CombatSearchV2ActionReactiveMechanicsFacts {
            player_hp_loss: reactive_player_hp_loss,
            bad_draw_cards: reactive_bad_draw_cards,
            ..CombatSearchV2ActionReactiveMechanicsFacts::default()
        },
        access: CombatSearchV2ActionAccessMechanicsFacts::default(),
        resource_timing: CombatSearchV2ActionResourceTimingFacts::default(),
        derived: CombatSearchV2ActionDerivedMechanicsFacts {
            mitigation_score: visible_attack_mitigation_hint,
            reactive_risk_score: reactive_player_hp_loss + reactive_bad_draw_cards,
            net_mitigation_score: visible_attack_mitigation_hint
                - reactive_player_hp_loss
                - reactive_bad_draw_cards,
            ..CombatSearchV2ActionDerivedMechanicsFacts::default()
        },
    }
}

fn state_summary(
    player_hp: i32,
    player_block: i32,
    energy: u8,
    total_enemy_hp: i32,
) -> CombatSearchV2StateSummary {
    CombatSearchV2StateSummary {
        engine_state: "CombatPlayerTurn".to_string(),
        terminal: SearchTerminalLabel::Unresolved,
        player_hp,
        player_block,
        energy,
        turn_count: 1,
        living_enemy_count: 1,
        total_enemy_hp,
        visible_incoming_damage: 6,
        enemy_slots: vec![CombatSearchV2EnemySummary {
            slot: 0,
            entity_id: 1,
            enemy_id: "Cultist".to_string(),
            hp: total_enemy_hp,
            max_hp: 100,
            block: 0,
            alive: true,
            escaped: false,
            dying: false,
            half_dead: false,
            phase: None,
            planned_move_id: 0,
            visible_intent: "attack".to_string(),
            visible_incoming_damage: 6,
        }],
        hand_count: 5,
        draw_count: 5,
        discard_count: 0,
        exhaust_count: 0,
        limbo_count: 0,
        queued_cards_count: 0,
    }
}
