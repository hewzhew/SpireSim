use super::*;

use crate::content::potions::{Potion, PotionId};
use crate::eval::run_control::{
    positive_ranked_run_policy_prior_v1, seed_oracle_run_explorer_from_session_v1,
    DecisionCandidateKey, OracleCombatWitnessOptionsV1, OracleRunCombatWitnessQualityPolicyV1,
    RunControlConfig, RunControlSession, RunPolicyCandidateV1, RunPolicyPriorFnV1,
    RunPolicyPriorV1,
};
use crate::runtime::combat::CombatCard;
use crate::state::core::{
    ActiveCombat, ClientInput, CombatContext, EngineState, RoomCombatContext,
    RunPendingChoiceReason, RunPendingChoiceState,
};
use crate::state::map::node::RoomType;
use crate::state::rewards::{RewardCard, RewardItem, RewardState};
use crate::state::selection::DomainEventSource;

fn parameterized_selection_analysis() -> OracleAnalysisSessionV1 {
    parameterized_selection_analysis_with_prior(None)
}

fn parameterized_selection_analysis_with_prior(
    decision_prior: Option<RunPolicyPriorFnV1>,
) -> OracleAnalysisSessionV1 {
    let mut run = RunControlSession::new(RunControlConfig::default());
    run.run_state.master_deck = (0..3)
        .map(|uuid| CombatCard::for_scenario(crate::content::cards::CardId::Strike, uuid))
        .collect();
    run.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 2,
        max_choices: 2,
        reason: RunPendingChoiceReason::PurgeNonBottled,
        source: DomainEventSource::Selection(RunPendingChoiceReason::PurgeNonBottled.into()),
        return_state: Box::new(EngineState::MapNavigation),
    });
    let combat_budgets =
        OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default());
    let explorer = seed_oracle_run_explorer_from_session_v1(
        run,
        RunProgressJournalV1::default(),
        &combat_budgets,
        None,
    )
    .expect("seed parameterized analysis");
    OracleAnalysisSessionV1::from_explorer(explorer, Some(0), combat_budgets, decision_prior, None)
        .expect("parameterized analysis")
}

fn card_reward_analysis() -> OracleAnalysisSessionV1 {
    let mut run = RunControlSession::new(RunControlConfig::default());
    run.run_state.act_num = 1;
    run.run_state.floor_num = 3;
    run.run_state.master_deck = (0..5)
        .map(|uuid| CombatCard::for_scenario(crate::content::cards::CardId::Strike, uuid))
        .collect();
    let cards = vec![
        RewardCard::new(crate::content::cards::CardId::PommelStrike, 0),
        RewardCard::new(crate::content::cards::CardId::SecondWind, 0),
    ];
    let mut reward = RewardState::new();
    reward.items = vec![RewardItem::Card {
        cards: cards.clone(),
    }];
    reward.pending_card_choice = Some(cards);
    reward.pending_card_reward_index = Some(0);
    run.engine_state = EngineState::RewardScreen(reward);

    let combat_budgets =
        OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default());
    let explorer = seed_oracle_run_explorer_from_session_v1(
        run,
        RunProgressJournalV1::default(),
        &combat_budgets,
        None,
    )
    .expect("seed card reward analysis");
    OracleAnalysisSessionV1::from_explorer(explorer, Some(0), combat_budgets, None, None)
        .expect("card reward analysis")
}

#[test]
fn card_reward_path_audit_batches_typed_boundaries_and_applied_identity() {
    let mut analysis = card_reward_analysis();
    let root = analysis
        .card_reward_path_audit(0)
        .expect("root card reward path audit");
    assert_eq!(root.target_node_id, 0);
    assert_eq!(root.boundaries.len(), 1);
    assert_eq!(root.boundaries[0].node_id, 0);
    assert_eq!(root.boundaries[0].floor, 3);
    assert_eq!(root.boundaries[0].deck.len(), 5);
    assert_eq!(
        root.boundaries[0].application,
        OracleAnalysisCardRewardApplicationV1::Uncommitted
    );
    let pommel = root.boundaries[0]
        .audit
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.candidate_key,
                DecisionCandidateKey::CardRewardPick {
                    card: crate::content::cards::CardId::PommelStrike,
                    ..
                }
            )
        })
        .expect("typed Pommel Strike candidate");
    assert!(matches!(
        pommel.acquisition,
        crate::eval::run_control::CardRewardPolicyAcquisitionV1::Card {
            card: crate::content::cards::CardId::PommelStrike,
            ..
        }
    ));

    let view = analysis.view_node(0).expect("root card reward view");
    let expected_cards = vec![
        RewardCard::new(crate::content::cards::CardId::PommelStrike, 0),
        RewardCard::new(crate::content::cards::CardId::SecondWind, 0),
    ];
    let reward = view.reward.as_ref().expect("typed reward state");
    assert_eq!(reward.pending_card_choice.as_ref(), Some(&expected_cards));
    assert!(matches!(
        reward.items.as_slice(),
        [RewardItem::Card { cards: offered }] if offered == &expected_cards
    ));
    let choice_ref = view
        .choices
        .iter()
        .find(|choice| choice.candidate_id == pommel.candidate_id)
        .expect("Pommel Strike retained choice")
        .choice_ref
        .clone();
    let child_node_id = analysis
        .try_choice(&choice_ref)
        .expect("apply Pommel Strike choice");
    let committed = analysis
        .card_reward_path_audit(child_node_id)
        .expect("committed card reward path audit");
    let OracleAnalysisCardRewardApplicationV1::Applied {
        candidate_id,
        current_owner_rank,
        ..
    } = &committed.boundaries[0].application
    else {
        panic!("committed card reward must retain applied identity")
    };
    assert_eq!(candidate_id, &pommel.candidate_id);
    assert_eq!(*current_owner_rank, pommel.owner_rank);
}

fn reject_child_decision_supply(
    _session: &RunControlSession,
    _legal: &[RunPolicyCandidateV1<'_>],
) -> Result<RunPolicyPriorV1, String> {
    Err("injected child decision-supply failure".to_string())
}

fn reverse_candidate_prior(
    _session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<RunPolicyPriorV1, String> {
    positive_ranked_run_policy_prior_v1(
        legal,
        legal
            .iter()
            .rev()
            .map(|candidate| candidate.candidate_id.to_string()),
    )
}

#[test]
fn current_choice_order_recomputes_direct_candidates_and_returns_exact_identity() {
    let mut run = RunControlSession::new(RunControlConfig::default());
    run.run_state.master_deck = (0..3)
        .map(|uuid| CombatCard::for_scenario(crate::content::cards::CardId::Strike, uuid))
        .collect();
    let cards = vec![
        RewardCard::new(crate::content::cards::CardId::PommelStrike, 0),
        RewardCard::new(crate::content::cards::CardId::SecondWind, 0),
    ];
    let mut reward = RewardState::new();
    reward.items = vec![RewardItem::Card {
        cards: cards.clone(),
    }];
    reward.pending_card_choice = Some(cards);
    reward.pending_card_reward_index = Some(0);
    run.engine_state = EngineState::RewardScreen(reward);
    let combat_budgets =
        OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default());
    let explorer = seed_oracle_run_explorer_from_session_v1(
        run,
        RunProgressJournalV1::default(),
        &combat_budgets,
        None,
    )
    .expect("seed direct-candidate analysis");
    let analysis = OracleAnalysisSessionV1::from_explorer(
        explorer,
        Some(0),
        combat_budgets,
        Some(reverse_candidate_prior),
        None,
    )
    .expect("direct-candidate analysis");
    let view = analysis.view_node(0).expect("materialized root view");
    let expected = view
        .choices
        .iter()
        .rev()
        .map(|choice| choice.choice_ref.clone())
        .collect::<Vec<_>>();

    assert_eq!(
        analysis
            .current_choice_order(0)
            .expect("current exact choice order"),
        expected
    );
    assert_ne!(
        view.choices.first().map(|choice| &choice.choice_ref),
        expected.first(),
        "the fixture must preserve a stale materialized rank to exercise recomputation"
    );
}

#[test]
fn current_choice_order_keeps_parameterized_selection_members_distinct_and_executable() {
    let mut analysis = parameterized_selection_analysis();
    let initial = analysis
        .current_choice_order(0)
        .expect("initial exact selection order");
    assert_eq!(initial.len(), 1);
    let first_ref = initial[0].clone();

    analysis
        .try_choice(&first_ref)
        .expect("execute first exact selection member");
    let expanded = analysis
        .current_choice_order(0)
        .expect("expanded exact selection order");
    assert_eq!(expanded.len(), 2);
    assert_eq!(expanded[0], first_ref);
    assert_ne!(expanded[0], expanded[1]);
    assert_eq!(
        expanded,
        analysis
            .current_choice_order(0)
            .expect("stable expanded exact selection order")
    );

    let parent = analysis.view_node(0).expect("expanded parent view");
    let selected = expanded
        .iter()
        .map(|choice_ref| {
            parent
                .choices
                .iter()
                .find(|choice| &choice.choice_ref == choice_ref)
                .expect("ordered exact member remains materialized")
        })
        .collect::<Vec<_>>();
    assert_eq!(selected[0].candidate_id, "select");
    assert_eq!(selected[1].candidate_id, "select");
    analysis
        .try_choice(&expanded[1])
        .expect("execute second exact selection member by unique choice ref");
}

fn combat_analysis(
    combat: crate::runtime::combat::CombatState,
    decision_prior: Option<RunPolicyPriorFnV1>,
) -> OracleAnalysisSessionV1 {
    combat_analysis_with_budgets(
        combat,
        decision_prior,
        OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default()),
    )
}

fn combat_analysis_with_budgets(
    combat: crate::runtime::combat::CombatState,
    decision_prior: Option<RunPolicyPriorFnV1>,
    combat_budgets: OracleRunCombatWitnessBudgetsV1,
) -> OracleAnalysisSessionV1 {
    let mut run = RunControlSession::new(RunControlConfig::default());
    run.engine_state = EngineState::CombatPlayerTurn;
    run.active_combat = Some(ActiveCombat::new(
        EngineState::CombatPlayerTurn,
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));
    let explorer = seed_oracle_run_explorer_from_session_v1(
        run,
        RunProgressJournalV1::default(),
        &combat_budgets,
        None,
    )
    .expect("seed combat analysis");
    OracleAnalysisSessionV1::from_explorer(explorer, Some(0), combat_budgets, decision_prior, None)
        .expect("combat analysis")
}

fn one_strike_combat_analysis(
    decision_prior: Option<RunPolicyPriorFnV1>,
) -> OracleAnalysisSessionV1 {
    combat_analysis(one_strike_combat(), decision_prior)
}

fn one_strike_combat() -> crate::runtime::combat::CombatState {
    let mut combat = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
    let plan = crate::content::monsters::roll_monster_turn_plan(
        &mut combat.rng.ai_rng,
        &monster,
        combat.meta.ascension_level,
        99,
        std::slice::from_ref(&monster),
        &[],
    );
    monster.set_planned_move_id(plan.move_id);
    monster.set_planned_steps(plan.steps);
    monster.set_planned_visible_spec(plan.visible_spec);
    monster.current_hp = 6;
    monster.max_hp = 6;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        1,
    )];
    combat
}

#[test]
fn analysis_node_preserves_sentry_bolts_original_debuff_intent() {
    let mut combat = crate::test_support::blank_test_combat();
    let mut sentry = crate::test_support::test_monster(crate::content::monsters::EnemyId::Sentry);
    let plan = crate::content::monsters::roll_monster_turn_plan(
        &mut combat.rng.ai_rng,
        &sentry,
        combat.meta.ascension_level,
        99,
        std::slice::from_ref(&sentry),
        &[],
    );
    sentry.set_planned_move_id(plan.move_id);
    sentry.set_planned_steps(plan.steps);
    sentry.set_planned_visible_spec(plan.visible_spec);
    combat.entities.monsters = vec![sentry];

    let analysis = combat_analysis(combat, None);
    let view = analysis.view_cursor().expect("view sentry combat root");
    let intent = view.encounter.as_ref().expect("combat encounter").monsters[0]
        .intent
        .as_ref()
        .expect("Sentry Bolt has a typed intent");
    let MonsterMoveSpec::AddCard(add_card) = intent else {
        panic!("Sentry Bolt should project AddCard, got {intent:?}");
    };
    assert_eq!(add_card.card_id, crate::content::cards::CardId::Dazed);
    assert_eq!(add_card.amount, 2);
    assert_eq!(
        add_card.destination,
        crate::runtime::monster_move::CardDestination::Discard
    );
    assert_eq!(
        add_card.visible_strength,
        crate::runtime::monster_move::EffectStrength::Normal
    );
}

#[test]
fn restored_combat_node_accepts_exact_actions_without_resident_search() {
    let mut analysis = one_strike_combat_analysis(None);
    analysis.combat_jobs.clear();
    let checkpoint = analysis.checkpoint().expect("checkpoint combat root");
    let mut restored =
        OracleAnalysisSessionV1::restore(checkpoint, analysis.combat_budgets.clone(), None, None)
            .expect("restore combat root without tactical work");

    assert!(
        restored
            .view_cursor()
            .expect("restored combat root")
            .combat
            .is_none(),
        "the checkpoint deliberately contains no resident tactical search"
    );
    restored
        .accept_cursor_combat_actions(&[ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        }])
        .expect("exact actions do not require a pre-existing search session");

    assert_eq!(
        restored
            .view_cursor()
            .expect("accepted combat child")
            .boundary,
        OracleRunBoundaryV1::Reward
    );
}

fn smoke_bomb_combat_analysis(
    decision_prior: Option<RunPolicyPriorFnV1>,
) -> OracleAnalysisSessionV1 {
    let mut combat = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
    let plan = crate::content::monsters::roll_monster_turn_plan(
        &mut combat.rng.ai_rng,
        &monster,
        combat.meta.ascension_level,
        99,
        std::slice::from_ref(&monster),
        &[],
    );
    monster.set_planned_move_id(plan.move_id);
    monster.set_planned_steps(plan.steps);
    monster.set_planned_visible_spec(plan.visible_spec);
    combat.entities.monsters = vec![monster];
    combat.entities.potions = vec![Some(Potion::new(PotionId::SmokeBomb, 41))];
    combat_analysis(combat, decision_prior)
}

#[test]
fn failed_choice_does_not_widen_or_mutate_the_analysis_session() {
    let mut analysis = parameterized_selection_analysis();
    let work = analysis
        .explorer
        .pending_decisions
        .front_mut()
        .expect("one lazy selection member");
    let requested_ref = choice_ref(work);
    work.parent_state_fingerprint = "stale-parent-state".to_string();
    let before = serde_json::to_value(analysis.checkpoint().expect("checkpoint before failure"))
        .expect("serialize before checkpoint");

    let error = analysis
        .try_choice(&requested_ref)
        .expect_err("stale exact work must fail");

    assert!(error.contains("parent fingerprint changed"), "{error}");
    let after = serde_json::to_value(analysis.checkpoint().expect("checkpoint after failure"))
        .expect("serialize after checkpoint");
    assert_eq!(
        after, before,
        "a rejected user operation must not widen selection supply or mutate navigation"
    );
}

#[test]
fn successful_choice_commits_exactly_one_selection_widening_step() {
    let mut analysis = parameterized_selection_analysis();
    let original_ref = choice_ref(
        analysis
            .explorer
            .pending_decisions
            .front()
            .expect("one lazy selection member"),
    );

    let child_node_id = analysis
        .try_choice(&original_ref)
        .expect("exact selection materializes");

    assert_eq!(analysis.cursor_node_id(), child_node_id);
    let parent = analysis.view_node(0).expect("parent remains inspectable");
    assert_eq!(
        parent.choices.len(),
        2,
        "one successful service preserves the tried member and exposes one sibling"
    );
    assert!(parent
        .choices
        .iter()
        .any(|choice| choice.choice_ref == original_ref));
    assert_eq!(parent.children.len(), 1);
    assert_eq!(parent.children[0].child_node_id, child_node_id);
}

#[test]
fn child_supply_failure_does_not_leave_an_unlinked_materialized_branch() {
    let mut analysis =
        parameterized_selection_analysis_with_prior(Some(reject_child_decision_supply));
    let requested_ref = choice_ref(
        analysis
            .explorer
            .pending_decisions
            .front()
            .expect("one lazy selection member"),
    );
    let before = serde_json::to_value(analysis.checkpoint().expect("checkpoint before failure"))
        .expect("serialize before checkpoint");

    let error = analysis
        .try_choice(&requested_ref)
        .expect_err("injected child supply must fail");

    assert!(
        error.contains("injected child decision-supply failure"),
        "unexpected decision failure: {error}"
    );
    let after = serde_json::to_value(analysis.checkpoint().expect("checkpoint after failure"))
        .expect("serialize after checkpoint");
    assert_eq!(
        after, before,
        "a failed child supply must not leave an unlinked branch or consume the choice"
    );
}

#[test]
fn combat_child_supply_failure_does_not_leave_an_unlinked_materialized_branch() {
    let mut analysis = one_strike_combat_analysis(Some(reject_child_decision_supply));
    let before = serde_json::to_value(analysis.checkpoint().expect("checkpoint before failure"))
        .expect("serialize before checkpoint");

    let error = analysis
        .accept_cursor_combat_actions(&[ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        }])
        .expect_err("injected combat child supply must fail");

    assert!(
        error.contains("injected child decision-supply failure"),
        "unexpected combat failure: {error}"
    );
    let after = serde_json::to_value(analysis.checkpoint().expect("checkpoint after failure"))
        .expect("serialize after checkpoint");
    assert!(
        after == before,
        "failed combat child supply must preserve the branch, resident work, edges, and navigation"
    );
}

#[test]
fn exact_combat_actions_register_the_materialized_child_decision_supply() {
    let mut analysis = one_strike_combat_analysis(None);

    let child_node_id = analysis
        .accept_cursor_combat_actions(&[ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        }])
        .expect("exact one-Strike witness");
    let child = analysis.view_cursor().expect("combat child view");

    assert_eq!(child.node_id, child_node_id);
    assert_eq!(child.boundary, OracleRunBoundaryV1::Reward);
    assert!(
        !child.choices.is_empty(),
        "a committed combat child must expose its legal reward decisions"
    );
}

#[test]
fn smoke_bomb_child_supply_failure_does_not_leave_an_unlinked_materialized_branch() {
    let mut analysis = smoke_bomb_combat_analysis(Some(reject_child_decision_supply));
    let before = serde_json::to_value(analysis.checkpoint().expect("checkpoint before failure"))
        .expect("serialize before checkpoint");

    let error = analysis
        .accept_cursor_smoke_bomb_escape()
        .expect_err("injected Smoke Bomb child supply must fail");

    assert!(
        error.contains("injected child decision-supply failure"),
        "unexpected Smoke Bomb failure: {error}"
    );
    let after = serde_json::to_value(analysis.checkpoint().expect("checkpoint after failure"))
        .expect("serialize after checkpoint");
    assert_eq!(
        after, before,
        "failed Smoke Bomb child supply must preserve the branch, resident work, edges, and navigation"
    );
}

#[test]
fn smoke_bomb_escape_registers_the_materialized_child_decision_supply() {
    let mut analysis = smoke_bomb_combat_analysis(None);

    let child_node_id = analysis
        .accept_cursor_smoke_bomb_escape()
        .expect("exact Smoke Bomb escape");
    let child = analysis.view_cursor().expect("escaped child view");

    assert_eq!(child.node_id, child_node_id);
    assert_eq!(child.boundary, OracleRunBoundaryV1::Reward);
    assert!(
        !child.choices.is_empty(),
        "a committed escape child must expose its legal reward decisions"
    );
}

fn strategic_combat_budgets(
    options: OracleCombatWitnessOptionsV1,
) -> OracleRunCombatWitnessBudgetsV1 {
    let mut budgets = OracleRunCombatWitnessBudgetsV1::uniform(options);
    budgets.quality_policy = OracleRunCombatWitnessQualityPolicyV1::StrategicRun;
    budgets
}

fn potion_equipped_one_strike_combat() -> crate::runtime::combat::CombatState {
    let mut combat = one_strike_combat();
    combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 51))];
    combat
}

fn establish_current_combat_witness(analysis: &mut OracleAnalysisSessionV1) {
    let work = &mut analysis
        .combat_jobs
        .get_mut(&0)
        .expect("resident combat")
        .work;
    for _ in 0..4 {
        if work.has_verified_witness() {
            break;
        }
        let _ = work.advance_improving_incumbent(
            &OracleCombatWitnessQuantumV1 {
                label: "establish-test-incumbent",
                additional_generation_work: 32,
                soft_wall_ms: None,
            },
            None,
        );
    }
    assert!(
        work.has_verified_witness(),
        "the exact planner should establish the deterministic one-Strike witness"
    );
}

#[test]
fn strategic_nonboss_analysis_starts_with_an_exact_no_potion_stage() {
    let analysis = combat_analysis_with_budgets(
        potion_equipped_one_strike_combat(),
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1::default()),
    );

    let combat = analysis
        .view_cursor()
        .expect("strategic combat view")
        .combat
        .expect("resident combat progress");

    assert_eq!(combat.search_stage, 0);
    assert_eq!(combat.max_potions_used, Some(0));
}

#[test]
fn strategic_no_potion_witness_that_meets_quality_materializes_without_rescue() {
    let mut analysis = combat_analysis_with_budgets(
        potion_equipped_one_strike_combat(),
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(64),
            ..OracleCombatWitnessOptionsV1::default()
        }),
    );

    let report = analysis
        .advance_cursor(OracleAnalysisAdvanceRequestV1 {
            max_quanta: 4,
            quantum_generation_work: 16,
            quantum_ms: None,
            wall_ms: None,
            improve_incumbent: true,
        })
        .expect("serve exact conserving combat");

    assert!(
        matches!(
            report.status,
            OracleAnalysisAdvanceStatusV1::BoundaryReached { .. }
        ),
        "a zero-loss no-potion witness should satisfy strategic quality: {:?}",
        report.status
    );
    assert_eq!(
        report.combat.expect("final combat progress").search_stage,
        0
    );
    assert_eq!(analysis.explorer.combat_search_restarts, 0);
}

#[test]
fn exhausted_stage_probe_retains_a_satisfying_incumbent_until_explicit_acceptance() {
    let mut analysis = combat_analysis_with_budgets(
        potion_equipped_one_strike_combat(),
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(64),
            ..OracleCombatWitnessOptionsV1::default()
        }),
    );
    establish_current_combat_witness(&mut analysis);
    let before = analysis
        .view_cursor()
        .expect("initial combat view")
        .combat
        .expect("initial combat progress");
    assert!(before.incumbent_satisfies_satisfaction == Some(true));
    let branch_count = analysis.explorer.branches.len();
    let edge_count = analysis.edges.len();

    let report = analysis
        .probe_cursor_combat_stage(OracleAnalysisCombatProbeRequestV1 {
            generation_work: 3,
            quantum_generation_work: 1,
            wall_ms: 1_000,
        })
        .expect("probe the current no-potion stage");
    let progress = report.combat;

    assert_eq!(report.stop, OracleAnalysisCombatProbeStopV1::StageExhausted);
    assert_eq!(report.generation_work_requested, 3);
    assert_eq!(report.generation_work_consumed, 1);
    assert_eq!(report.quanta_served, 1);
    assert_eq!(analysis.cursor_node_id(), 0);
    assert_eq!(analysis.explorer.branches.len(), branch_count);
    assert_eq!(analysis.edges.len(), edge_count);
    assert_eq!(
        analysis.view_cursor().expect("retained combat").boundary,
        OracleRunBoundaryV1::Combat
    );
    assert!(analysis.combat_jobs.contains_key(&0));
    assert_eq!(progress.search_stage, 0);
    assert_eq!(progress.max_potions_used, before.max_potions_used);
    assert_eq!(progress.allowed_potion_slots, before.allowed_potion_slots);
    assert_eq!(
        progress.stage_trace.last().map(|stage| stage.exit),
        Some(OracleAnalysisCombatStageExitV1::ProbeStageExhausted)
    );

    let resumed = analysis
        .probe_cursor_combat_stage(OracleAnalysisCombatProbeRequestV1 {
            generation_work: 2,
            quantum_generation_work: 1,
            wall_ms: 1_000,
        })
        .expect("resume the same current-stage frontier");
    assert_eq!(
        resumed.stop,
        OracleAnalysisCombatProbeStopV1::StageExhausted
    );
    assert_eq!(resumed.generation_work_consumed, 0);
    assert_eq!(
        resumed.combat.resume_kind,
        OracleCombatSearchResumeKindV1::SearchResumeExact
    );
    assert_eq!(resumed.combat.search_stage, 0);
    assert_eq!(resumed.combat.restart_count, 0);
    assert_eq!(analysis.explorer.branches.len(), branch_count);
    assert_eq!(analysis.edges.len(), edge_count);

    let child_node_id = analysis
        .accept_cursor_combat_incumbent()
        .expect("explicitly accept the retained incumbent");
    assert_eq!(analysis.cursor_node_id(), child_node_id);
    assert_eq!(
        analysis.view_cursor().expect("accepted child").boundary,
        OracleRunBoundaryV1::Reward
    );
    assert!(!analysis.combat_jobs.contains_key(&0));
}

#[test]
fn current_stage_probe_rejects_zero_work_without_mutation() {
    let mut analysis = one_strike_combat_analysis(None);
    let branch_count = analysis.explorer.branches.len();
    let error = analysis
        .probe_cursor_combat_stage(OracleAnalysisCombatProbeRequestV1 {
            generation_work: 0,
            ..OracleAnalysisCombatProbeRequestV1::default()
        })
        .expect_err("zero-work probes must fail");

    assert!(error.contains("requires positive work"));
    assert_eq!(analysis.explorer.branches.len(), branch_count);
    assert_eq!(analysis.cursor_node_id(), 0);
}

#[test]
fn strategic_advance_keeps_a_below_reserve_win_resident_until_explicit_acceptance() {
    // This deliberately crosses witness replay, analysis advance, and child
    // materialization: a lower-level predicate test cannot catch an unsafe
    // fallback being committed by the public staged-advance transaction.
    let mut combat = one_strike_combat();
    combat.entities.player.current_hp = 7;
    combat.entities.player.max_hp = 80;
    combat.turn.energy = 0;
    combat.zones.hand = vec![
        CombatCard::for_scenario(crate::content::cards::CardId::Offering, 2),
        CombatCard::for_scenario(crate::content::cards::CardId::Strike, 1),
    ];
    let mut analysis = combat_analysis_with_budgets(
        combat,
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(0),
            ..OracleCombatWitnessOptionsV1::default()
        }),
    );
    let actions = [
        ClientInput::PlayCard {
            card_index: 0,
            target: None,
        },
        ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        },
    ];
    analysis
        .combat_jobs
        .get_mut(&0)
        .expect("resident low-HP combat")
        .work
        .verify_and_restore_action_witness(&actions)
        .expect("exact Offering win");

    assert!(!analysis
        .cursor_combat_incumbent_preserves_survival_floor()
        .expect("typed survival-floor check"));
    let report = analysis
        .advance_cursor(OracleAnalysisAdvanceRequestV1 {
            max_quanta: 1,
            quantum_generation_work: 1,
            quantum_ms: None,
            wall_ms: None,
            improve_incumbent: false,
        })
        .expect("bounded strategic advance");

    assert!(matches!(
        report.status,
        OracleAnalysisAdvanceStatusV1::BudgetUnknown
    ));
    assert_eq!(
        report
            .combat
            .as_ref()
            .and_then(|combat| combat.incumbent_final_hp),
        Some(1)
    );
    assert_eq!(
        analysis
            .view_cursor()
            .expect("resident combat view")
            .boundary,
        OracleRunBoundaryV1::Combat,
        "an unsafe fallback must not materialize during ordinary advance"
    );

    analysis
        .accept_cursor_combat_incumbent()
        .expect("explicit analyst acceptance");
    let accepted = analysis.view_cursor().expect("explicitly accepted child");
    assert_eq!(accepted.boundary, OracleRunBoundaryV1::Reward);
    assert_eq!(accepted.current_hp, 1);
}

#[test]
fn bounded_no_potion_unknown_enters_the_full_potion_stage() {
    let mut combat = potion_equipped_one_strike_combat();
    combat.zones.hand.clear();
    combat.entities.monsters[0].current_hp = 200;
    combat.entities.monsters[0].max_hp = 200;
    let mut analysis = combat_analysis_with_budgets(
        combat,
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(2),
            ..OracleCombatWitnessOptionsV1::default()
        }),
    );

    let report = analysis
        .advance_cursor(OracleAnalysisAdvanceRequestV1 {
            max_quanta: 2,
            quantum_generation_work: 1,
            quantum_ms: None,
            wall_ms: None,
            improve_incumbent: true,
        })
        .expect("serve conserving challenge and rescue");
    let combat = report.combat.expect("resident rescue progress");

    assert_eq!(combat.search_stage, 1);
    assert_ne!(combat.max_potions_used, Some(0));
    assert_eq!(combat.allowed_potion_slots, Some(1));
    assert_eq!(combat.stage_trace.len(), 2);
    assert_eq!(combat.stage_trace[0].stage, 0);
    assert_eq!(
        combat.stage_trace[0].exit,
        OracleAnalysisCombatStageExitV1::PromotedForReservedQuantum
    );
    assert_eq!(combat.stage_trace[1].stage, 1);
    assert_eq!(
        combat.stage_trace[1].exit,
        OracleAnalysisCombatStageExitV1::SearchPending
    );
    assert_eq!(analysis.explorer.combat_search_restarts, 1);
}

#[test]
fn new_identity_stage_keeps_its_configured_share_of_a_larger_advance_request() {
    let mut combat = potion_equipped_one_strike_combat();
    combat.zones.hand.clear();
    combat.entities.monsters[0].current_hp = 200;
    combat.entities.monsters[0].max_hp = 200;
    combat
        .entities
        .potions
        .push(Some(Potion::new(PotionId::ExplosivePotion, 52)));
    let mut analysis = combat_analysis_with_budgets(
        combat,
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(9),
            ..OracleCombatWitnessOptionsV1::default()
        }),
    );

    let report = analysis
        .advance_cursor(OracleAnalysisAdvanceRequestV1 {
            max_quanta: 1,
            quantum_generation_work: 1_000,
            quantum_ms: None,
            wall_ms: None,
            improve_incumbent: true,
        })
        .expect("serve the configured no-potion share");
    let job = analysis.combat_jobs.get(&0).expect("first potion stage");

    assert!(matches!(
        report.status,
        OracleAnalysisAdvanceStatusV1::SearchPending
    ));
    assert_eq!(job.stage, 1);
    assert_eq!(job.work.allowed_potion_slots(), Some(0b01));
    assert_eq!(job.work.remaining_generation_work(), 5);
    let progress = report.combat.expect("identity-stage progress");
    assert_eq!(progress.stage_trace.len(), 2);
    assert_eq!(
        progress.stage_trace[0].exit,
        OracleAnalysisCombatStageExitV1::PromotedAfterAllowanceExhausted
    );
    assert_eq!(
        progress.stage_trace[1].exit,
        OracleAnalysisCombatStageExitV1::SearchPending
    );
    assert!(analysis
        .combat_budgets
        .has_later_stage(&analysis.require_branch(0).unwrap().session, 1));
}

#[test]
fn typed_fire_potion_rescue_materializes_when_no_potion_win_exists() {
    let mut combat = one_strike_combat();
    combat.zones.hand.clear();
    combat.entities.monsters[0].current_hp = 20;
    combat.entities.monsters[0].max_hp = 20;
    combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 52))];
    let mut analysis = combat_analysis_with_budgets(
        combat,
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(256),
            ..OracleCombatWitnessOptionsV1::default()
        }),
    );

    let report = analysis
        .advance_cursor(OracleAnalysisAdvanceRequestV1 {
            max_quanta: 8,
            quantum_generation_work: 32,
            quantum_ms: None,
            wall_ms: None,
            improve_incumbent: true,
        })
        .expect("serve exact Fire Potion rescue");

    assert!(
        matches!(
            report.status,
            OracleAnalysisAdvanceStatusV1::BoundaryReached { .. }
        ),
        "the typed survival lane should admit the exact lethal potion: {:?}",
        report.status
    );
    let progress = report.combat.expect("final rescue progress");
    assert_eq!(progress.search_stage, 1);
    assert_eq!(progress.allowed_potion_slots, Some(1));
    assert_eq!(progress.incumbent_potions_used, Some(1));
    assert_eq!(progress.incumbent_potion_slots, Some(1));
    assert_eq!(
        progress.stage_trace.last().map(|stage| stage.exit),
        Some(OracleAnalysisCombatStageExitV1::BoundaryReached)
    );
    assert!(
        analysis
            .view_cursor()
            .expect("rescued child")
            .potions
            .first()
            .is_some_and(Option::is_none),
        "the materialized exact child must record the Fire Potion expenditure"
    );
}

#[test]
fn common_strength_potion_can_rescue_a_verified_but_low_quality_win() {
    let mut combat = one_strike_combat();
    combat.entities.player.current_hp = 40;
    combat.entities.player.max_hp = 40;
    combat.entities.monsters[0].current_hp = 8;
    combat.entities.monsters[0].max_hp = 8;
    combat.zones.draw_pile = vec![CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        2,
    )]
    .into();
    combat.entities.potions = vec![Some(Potion::new(PotionId::StrengthPotion, 53))];
    let mut analysis = combat_analysis_with_budgets(
        combat,
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(512),
            ..OracleCombatWitnessOptionsV1::default()
        }),
    );

    let first = analysis
        .advance_cursor(OracleAnalysisAdvanceRequestV1 {
            max_quanta: 16,
            quantum_generation_work: 32,
            quantum_ms: None,
            wall_ms: None,
            improve_incumbent: true,
        })
        .expect("enter exact Strength Potion quality rescue");

    let report = if matches!(
        &first.status,
        OracleAnalysisAdvanceStatusV1::BoundaryReached { .. }
    ) {
        first
    } else {
        assert!(
            matches!(&first.status, OracleAnalysisAdvanceStatusV1::SearchPending),
            "the Strength Potion rescue should remain pending or finish early: {:?}",
            first.status
        );
        let pending = first.combat.expect("pending Strength rescue progress");
        assert_eq!(pending.search_stage, 1);
        assert_eq!(pending.allowed_potion_slots, Some(1));

        analysis
            .advance_cursor(OracleAnalysisAdvanceRequestV1 {
                max_quanta: 16,
                quantum_generation_work: 32,
                quantum_ms: None,
                wall_ms: None,
                improve_incumbent: true,
            })
            .expect("finish exact Strength Potion quality rescue")
    };

    assert!(
        matches!(
            report.status,
            OracleAnalysisAdvanceStatusV1::BoundaryReached { .. }
        ),
        "the common deterministic rescue should improve the verified line: {:?}",
        report.status
    );
    let progress = report.combat.expect("final Strength rescue progress");
    assert_eq!(progress.search_stage, 1);
    assert_eq!(progress.allowed_potion_slots, Some(1));
    assert_eq!(progress.incumbent_potions_used, Some(1));
    assert_eq!(progress.incumbent_potion_slots, Some(1));
    let final_stage = progress.stage_trace.last().expect("final Strength stage");
    assert_eq!(final_stage.local_candidate_final_hp, Some(40));
    assert_eq!(final_stage.local_candidate_potions_used, Some(1));
    assert_eq!(final_stage.local_candidate_potion_slots, Some(1));
    assert_eq!(
        final_stage.local_candidate_satisfies_satisfaction,
        Some(true)
    );
    assert_eq!(
        final_stage.local_candidate_disposition,
        Some(OracleCombatWitnessCandidateDispositionV1::SelectedIncumbent)
    );
    assert_eq!(
        analysis.view_cursor().expect("rescued child").current_hp,
        40,
        "Strength should kill before the otherwise unavoidable Jaw Worm hit"
    );
}

#[test]
fn boss_uses_stages_but_explicit_potion_overrides_remain_literal() {
    let mut boss_combat = potion_equipped_one_strike_combat();
    boss_combat.meta.is_boss_fight = true;
    let boss = combat_analysis_with_budgets(
        boss_combat,
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1::default()),
    );
    let boss_progress = boss
        .view_cursor()
        .expect("boss combat view")
        .combat
        .expect("boss combat progress");
    assert_eq!(boss_progress.search_stage, 0);
    assert_eq!(boss_progress.max_potions_used, Some(0));
    assert!(boss
        .combat_budgets
        .has_later_stage(&boss.require_branch(0).expect("boss branch").session, 0));

    let overridden = combat_analysis_with_budgets(
        potion_equipped_one_strike_combat(),
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
            max_potions_used: Some(1),
            ..OracleCombatWitnessOptionsV1::default()
        }),
    );
    let overridden_progress = overridden
        .view_cursor()
        .expect("overridden combat view")
        .combat
        .expect("overridden combat progress");
    assert_eq!(overridden_progress.search_stage, 0);
    assert_eq!(overridden_progress.max_potions_used, Some(1));
    assert!(!overridden.combat_budgets.has_later_stage(
        &overridden
            .require_branch(0)
            .expect("overridden branch")
            .session,
        0
    ));
}

#[test]
fn boss_single_identity_rescue_finishes_before_multi_potion_fallback() {
    let mut combat = one_strike_combat();
    combat.meta.is_boss_fight = true;
    combat.zones.hand.clear();
    combat.entities.monsters[0].current_hp = 20;
    combat.entities.monsters[0].max_hp = 20;
    combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 52))];
    let mut analysis = combat_analysis_with_budgets(
        combat,
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(256),
            ..OracleCombatWitnessOptionsV1::default()
        }),
    );

    let report = analysis
        .advance_cursor(OracleAnalysisAdvanceRequestV1 {
            max_quanta: 8,
            quantum_generation_work: 32,
            quantum_ms: None,
            wall_ms: None,
            improve_incumbent: true,
        })
        .expect("finish exact one-potion Boss rescue");
    let progress = report.combat.expect("final Boss rescue progress");

    assert!(matches!(
        report.status,
        OracleAnalysisAdvanceStatusV1::BoundaryReached { .. }
    ));
    assert_eq!(progress.search_stage, 1);
    assert_eq!(progress.max_potions_used, Some(1));
    assert_eq!(progress.allowed_potion_slots, Some(1));
    assert_eq!(progress.incumbent_potions_used, Some(1));
    assert_eq!(
        analysis.explorer.combat_search_restarts, 1,
        "the verified single-identity rescue must commit before the multi-potion fallback"
    );
}

#[test]
fn boss_multi_potion_fallback_remains_available_after_single_slot_misses() {
    let mut combat = one_strike_combat();
    combat.meta.is_boss_fight = true;
    combat.zones.hand.clear();
    combat.entities.monsters[0].current_hp = 30;
    combat.entities.monsters[0].max_hp = 30;
    combat.entities.potions = vec![
        Some(Potion::new(PotionId::FirePotion, 52)),
        Some(Potion::new(PotionId::FirePotion, 53)),
    ];
    let mut analysis = combat_analysis_with_budgets(
        combat,
        None,
        strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(2_048),
            ..OracleCombatWitnessOptionsV1::default()
        }),
    );

    let report = analysis
        .advance_cursor(OracleAnalysisAdvanceRequestV1 {
            max_quanta: 10_000,
            quantum_generation_work: 64,
            quantum_ms: None,
            wall_ms: None,
            improve_incumbent: true,
        })
        .expect("finish exact two-potion Boss fallback");
    let progress = report.combat.expect("final multi-potion progress");

    assert!(
        matches!(
            report.status,
            OracleAnalysisAdvanceStatusV1::BoundaryReached { .. }
        ),
        "multi-potion fallback did not materialize: {:?}, stage={}, remaining_generation_work={}, max_potions={:?}, allowed_slots={:?}",
        report.status,
        progress.search_stage,
        progress.remaining_generation_work,
        progress.max_potions_used,
        progress.allowed_potion_slots
    );
    assert_eq!(progress.search_stage, 3);
    assert_eq!(progress.max_potions_used, Some(2));
    assert_eq!(progress.allowed_potion_slots, Some(0b11));
    assert_eq!(progress.incumbent_potions_used, Some(2));
    assert_eq!(progress.incumbent_potion_slots, Some(0b11));
    assert_eq!(analysis.explorer.combat_search_restarts, 3);
}

#[test]
fn analysis_checkpoint_restores_the_resident_combat_stage() {
    let mut combat = potion_equipped_one_strike_combat();
    combat.zones.hand.clear();
    combat.entities.monsters[0].current_hp = 200;
    combat.entities.monsters[0].max_hp = 200;
    let budgets = strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
        max_generation_work: Some(2),
        ..OracleCombatWitnessOptionsV1::default()
    });
    let mut analysis = combat_analysis_with_budgets(combat, None, budgets.clone());
    assert!(analysis
        .promote_combat_job_if_needed(0)
        .expect("promote resident combat"));

    let checkpoint = analysis.checkpoint().expect("analysis checkpoint");
    assert_eq!(checkpoint.combat_jobs[0].stage, 1);
    assert_eq!(checkpoint.combat_jobs[0].completed_stage_trace.len(), 1);
    let restored = OracleAnalysisSessionV1::restore(checkpoint, budgets, None, None)
        .expect("restore analysis");
    let restored_progress = restored
        .view_cursor()
        .expect("restored combat view")
        .combat
        .expect("restored combat progress");

    assert_eq!(restored_progress.search_stage, 1);
    assert_ne!(restored_progress.max_potions_used, Some(0));
    assert_eq!(restored_progress.allowed_potion_slots, Some(1));
    assert_eq!(restored_progress.stage_trace.len(), 2);
    assert_eq!(restored_progress.stage_trace[0].stage, 0);
    assert_eq!(
        restored_progress.stage_trace[0].exit,
        OracleAnalysisCombatStageExitV1::PromotedForReservedQuantum
    );
    assert_eq!(restored_progress.stage_trace[1].stage, 1);
    assert_eq!(
        restored_progress.stage_trace[1].exit,
        OracleAnalysisCombatStageExitV1::Active
    );
    assert!(restored_progress.stage_trace.iter().all(|stage| stage
        .historical_generation_work_at_entry
        <= restored_progress.generation_work));
    assert_eq!(restored_progress.restart_count, 2);
}

#[test]
fn quality_gated_potion_replacement_survives_checkpoint_restore() {
    let mut combat = one_strike_combat();
    combat.entities.player.current_hp = 40;
    combat.entities.player.max_hp = 40;
    combat.entities.monsters[0].current_hp = 8;
    combat.entities.monsters[0].max_hp = 8;
    combat.zones.draw_pile = vec![CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        2,
    )]
    .into();
    combat.entities.potions = vec![Some(Potion::new(PotionId::SkillPotion, 60))];
    let budgets = strategic_combat_budgets(OracleCombatWitnessOptionsV1 {
        max_generation_work: Some(512),
        ..OracleCombatWitnessOptionsV1::default()
    });
    let mut analysis = combat_analysis_with_budgets(combat, None, budgets.clone());

    for _ in 0..16 {
        let work = &mut analysis
            .combat_jobs
            .get_mut(&0)
            .expect("resident conserving search")
            .work;
        if work.has_verified_witness() {
            break;
        }
        let _ = work.advance_improving_incumbent(
            &OracleCombatWitnessQuantumV1 {
                label: "quality-gate-checkpoint",
                additional_generation_work: 32,
                soft_wall_ms: None,
            },
            None,
        );
    }
    let conserving_work = &analysis
        .combat_jobs
        .get(&0)
        .expect("resident conserving search")
        .work;
    assert!(conserving_work.has_verified_witness());
    assert!(!conserving_work.has_refinement_ending_witness());
    assert!(analysis
        .promote_combat_job_if_needed(0)
        .expect("promote verified low-quality incumbent"));
    let checkpoint = analysis.checkpoint().expect("quality-gated checkpoint");
    assert_eq!(checkpoint.combat_jobs[0].stage, 1);
    assert!(
        checkpoint.combat_jobs[0]
            .work
            .potion_spend_requires_satisfaction
    );
    assert_eq!(checkpoint.combat_jobs[0].work.allowed_potion_slots, Some(1));

    let restored = OracleAnalysisSessionV1::restore(checkpoint, budgets, None, None)
        .expect("restore quality-gated analysis");
    let restored_checkpoint = restored
        .checkpoint()
        .expect("restored quality-gated checkpoint");
    assert!(
        restored_checkpoint.combat_jobs[0]
            .work
            .potion_spend_requires_satisfaction
    );
    assert_eq!(
        restored_checkpoint.combat_jobs[0].work.allowed_potion_slots,
        Some(1)
    );
}

#[test]
fn quality_gated_rescue_can_inspect_flexible_potions_without_admitting_passive_escape() {
    let mut combat = one_strike_combat();
    combat.entities.potions = vec![
        Some(Potion::new(PotionId::BlockPotion, 61)),
        Some(Potion::new(PotionId::SkillPotion, 62)),
        Some(Potion::new(PotionId::FairyPotion, 63)),
        Some(Potion::new(PotionId::SmokeBomb, 64)),
    ];
    let budgets = strategic_combat_budgets(OracleCombatWitnessOptionsV1::default());
    let mut analysis = combat_analysis_with_budgets(combat, None, budgets.clone());
    establish_current_combat_witness(&mut analysis);
    let branch = analysis.require_branch(0).expect("combat branch");
    let mut prior = analysis
        .combat_jobs
        .get(&0)
        .expect("resident combat")
        .work
        .checkpoint();
    assert!(
        prior.incumbent.is_some(),
        "the deterministic one-Strike policy should provide a verified fallback"
    );
    let incumbent = prior.incumbent.clone();

    let improve = budgets.for_session_stage_with_prior(&branch.session, 1, &prior);
    assert_eq!(improve.max_potions_used, Some(1));
    assert_eq!(
        improve.allowed_potion_slots,
        Some(1_u64 << 0),
        "each rescue stage must keep one concrete potion identity"
    );
    let improve_flexible = budgets.for_session_stage_with_prior(&branch.session, 2, &prior);
    assert_eq!(improve_flexible.max_potions_used, Some(1));
    assert_eq!(
        improve_flexible.allowed_potion_slots,
        Some(1_u64 << 1),
        "the next stage must independently inspect the flexible Skill Potion"
    );
    assert!(budgets.has_later_stage(&branch.session, 1));
    assert!(!budgets.has_later_stage(&branch.session, 2));

    prior.incumbent = None;
    let survival = budgets.for_session_stage_with_prior(&branch.session, 1, &prior);
    assert_eq!(survival.max_potions_used, Some(1));
    assert_eq!(
        survival.allowed_potion_slots,
        Some(1_u64 << 0),
        "finding any win still inspects one active identity at a time"
    );

    prior.max_potions_used = survival.max_potions_used;
    prior.allowed_potion_slots = survival.allowed_potion_slots;
    prior.incumbent = incumbent;
    let restored = budgets.for_session_stage_restore(&branch.session, 1, &prior);
    assert_eq!(
        restored.allowed_potion_slots, survival.allowed_potion_slots,
        "checkpoint restore must keep the original survival lane even after that lane has an incumbent"
    );
}
