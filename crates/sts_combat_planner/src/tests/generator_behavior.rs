use super::*;

#[test]
fn policy_guided_generator_emits_preferred_and_complete_sibling_options() {
    let stepper = TinyTurnStepper::lethal();
    let mut session =
        TurnOptionGeneratorSession::with_policy(root(), config(), Arc::new(PreferPlayPolicy));

    let report = session.advance(&stepper, CombatPlanningQuantum::deterministic(4, 8));

    assert_eq!(report.newly_completed_options, 2);
    assert_eq!(session.completed_options()[0].actions()[0].input, PLAY);
    assert!(session
        .completed_options()
        .iter()
        .any(|option| option.actions()[0].input == ClientInput::EndTurn));
    assert_eq!(session.retained_work_items(), 0);
    assert!(session.is_finished());
}

#[test]
fn shared_guide_publishes_and_services_the_best_partial_expansion() {
    let stepper = TinyTurnStepper::plain();
    let mut generator =
        TurnOptionGeneratorSession::with_policy(root(), config(), Arc::new(SharedGuidePolicy));

    assert_eq!(
        generator
            .best_retained_guide_promise_snapshot(SHARED_TEST_GUIDE)
            .expect("root guide promise")
            .rank,
        CombatStateGuideRank::new(vec![0])
    );

    generator.advance(&stepper, CombatPlanningQuantum::deterministic(1, 4));
    generator.prefer_lane(TurnOptionGeneratorPreferredLane::Guide(SHARED_TEST_GUIDE));
    generator.advance(&stepper, CombatPlanningQuantum::deterministic(1, 4));

    assert_eq!(
        generator
            .best_retained_guide_promise_snapshot(SHARED_TEST_GUIDE)
            .expect("partial-state guide promise")
            .rank,
        CombatStateGuideRank::new(vec![1]),
        "the resumable parent must publish its best retained partial state, not its stale root rank"
    );

    let guided_before = generator.guided_work_pops();
    generator.prefer_lane(TurnOptionGeneratorPreferredLane::Guide(SHARED_TEST_GUIDE));
    generator.advance(&stepper, CombatPlanningQuantum::deterministic(1, 4));
    assert_eq!(generator.guided_work_pops(), guided_before + 1);
}

#[test]
fn atomic_siblings_share_one_resumable_cursor_and_one_service_emits_one_edge() {
    let stepper = TinyTurnStepper::plain();
    let decision_root = root();
    let mut session = TurnOptionGeneratorSession::with_policy(
        decision_root.clone(),
        config(),
        Arc::new(PreferPlayPolicy),
    );

    let discovery = session.advance(&stepper, CombatPlanningQuantum::deterministic(1, 8));
    assert_eq!(discovery.after_diagnostics.applied_action_transitions, 0);
    assert_eq!(session.retained_work_items(), 1);
    assert_eq!(
        session.live_work_counts_at_exact_position(decision_root.position()),
        (0, 2, 0),
        "one cursor owns both concrete atomic siblings"
    );

    let first_edge = session.advance(&stepper, CombatPlanningQuantum::deterministic(1, 8));
    assert_eq!(first_edge.after_diagnostics.applied_action_transitions, 1);
    assert_eq!(
        first_edge
            .after_diagnostics
            .applied_action_transitions
            .saturating_sub(first_edge.before_diagnostics.applied_action_transitions),
        1,
        "one cursor service must not turn into eager all-successor expansion"
    );
}

#[test]
fn generator_publishes_a_reached_turn_boundary_without_rescheduling_it() {
    let stepper = TinyTurnStepper::plain();
    let mut position = root().position().clone();
    position.combat.turn.energy = 0;
    let root = CombatDecisionRoot::new(position).unwrap();
    let mut session = TurnOptionGeneratorSession::new(root, config());

    // One work item expands the root and one executes EndTurn. The resulting
    // next-player-turn state is already stable and must be published without
    // requiring a third agenda pop merely to recognize the boundary.
    let report = session.advance(&stepper, CombatPlanningQuantum::deterministic(2, 8));

    assert_eq!(report.after.generation_work, 2);
    assert_eq!(report.newly_completed_options, 1);
    assert_eq!(
        session.completed_options()[0].actions()[0].input,
        ClientInput::EndTurn
    );
}

#[test]
fn only_complete_turn_options_are_public() {
    let stepper = TinyTurnStepper::plain();
    let mut session = TurnOptionGeneratorSession::new(root(), config());

    let first = session.advance(&stepper, CombatPlanningQuantum::deterministic(2, 4));
    assert!(matches!(
        first.status,
        TurnOptionGenerationStatus::Partial(GenerationInterruption::GenerationWorkBudget)
    ));
    assert!(session.completed_options().is_empty());

    let finished = finish(&mut session, &stepper);
    assert_eq!(finished.status, TurnOptionGenerationStatus::Complete);
    assert_eq!(session.completed_options().len(), 2);
    assert!(session.completed_options().iter().all(|option| {
        option.boundary() == CompleteTurnOptionBoundary::NextPlayerTurn
            && matches!(
                option.actions().last().map(|action| &action.input),
                Some(ClientInput::EndTurn)
            )
    }));
}

#[test]
fn split_quantum_matches_one_shot_without_replaying_transitions() {
    let split_stepper = TinyTurnStepper::plain();
    let mut split = TurnOptionGeneratorSession::new(root(), config());
    split.advance(&split_stepper, CombatPlanningQuantum::deterministic(2, 4));
    finish(&mut split, &split_stepper);

    let one_shot_stepper = TinyTurnStepper::plain();
    let mut one_shot = TurnOptionGeneratorSession::new(root(), config());
    finish(&mut one_shot, &one_shot_stepper);

    let split_options = split
        .completed_options()
        .iter()
        .map(|option| {
            (
                option.actions().to_vec(),
                option.exact_successor_hash().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let one_shot_options = one_shot
        .completed_options()
        .iter()
        .map(|option| {
            (
                option.actions().to_vec(),
                option.exact_successor_hash().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(split_options, one_shot_options);
    assert_eq!(split_stepper.call_count(&PLAY), 1);
    assert_eq!(split_stepper.call_count(&ClientInput::EndTurn), 2);
}

#[test]
fn generation_diagnostics_count_exact_successor_merges_without_changing_options() {
    let stepper = TinyTurnStepper::with_duplicate_unclassified_surface();
    let mut session = TurnOptionGeneratorSession::new(root(), config());

    let report = finish(&mut session, &stepper);

    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    assert_eq!(session.completed_options().len(), 2);
    assert_eq!(report.after_diagnostics.duplicate_exact_successors, 1);
    assert_eq!(report.after_diagnostics.applied_action_transitions, 4);
    assert_eq!(report.after_diagnostics.unique_successor_states, 3);
    assert_eq!(report.after_diagnostics.completed_turn_options, 2);
}

#[test]
fn engine_transition_waits_for_a_full_reservation() {
    let stepper = TinyTurnStepper::plain();
    let mut session = TurnOptionGeneratorSession::new(root(), config());

    let blocked = session.advance(&stepper, CombatPlanningQuantum::deterministic(10, 3));
    assert_eq!(
        blocked.status,
        TurnOptionGenerationStatus::Partial(GenerationInterruption::EngineStepBudget)
    );
    assert!(stepper.calls.lock().unwrap().is_empty());

    session.advance(&stepper, CombatPlanningQuantum::deterministic(0, 1));
    assert_eq!(stepper.call_count(&PLAY), 1);
}

#[test]
fn ordered_structured_selections_survive_complete_option_generation() {
    let stepper = TinyTurnStepper::with_selection();
    let mut session = TurnOptionGeneratorSession::new(root(), config());
    let report = finish(&mut session, &stepper);

    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    let submitted_orders = session
        .completed_options()
        .iter()
        .flat_map(|option| option.actions())
        .filter_map(|action| match &action.input {
            ClientInput::SubmitSelection(resolution) => Some(resolution.selected_card_lineages()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(submitted_orders.contains(&sts_core::test_support::scenario_lineages([11, 22])));
    assert!(submitted_orders.contains(&sts_core::test_support::scenario_lineages([22, 11])));
}

fn ordered_selection_root() -> CombatDecisionRoot {
    let mut position = root().position().clone();
    position.combat.turn.energy = 0;
    position.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([11, 22]),
        min_cards: 2,
        max_cards: 2,
        can_cancel: false,
        reason: HandSelectReason::Discard,
    });
    CombatDecisionRoot::new(position).unwrap()
}

struct SparseSelectionPolicy {
    source: CombatPosition,
    suggestions: Vec<ClientInput>,
}

impl CombatActionPolicy for SparseSelectionPolicy {
    fn weights(&self, _: &CombatPosition, choices: &[CombatPolicyChoice<'_>]) -> Vec<f64> {
        vec![1.0; choices.len()]
    }
    fn structured_selection_preferences(
        &self,
        position: &CombatPosition,
        _: &CombatSelectionActionFamilyV2,
    ) -> CombatSelectionPreferences {
        if position == &self.source {
            CombatSelectionPreferences::new(self.suggestions.iter().cloned())
        } else {
            CombatSelectionPreferences::default()
        }
    }
}

#[test]
fn sparse_selection_partitions_mass_without_replaying_duplicate_suggestions() {
    use sts_core::state::selection::{SelectionResolution, SelectionScope};
    let source = ordered_selection_root();
    let preferred = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
        SelectionScope::Hand,
        sts_core::test_support::scenario_lineages([22, 11]),
    ));
    let stale = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
        SelectionScope::Hand,
        sts_core::test_support::scenario_lineages([22, 99]),
    ));
    let policy = Arc::new(SparseSelectionPolicy {
        source: source.position().clone(),
        suggestions: vec![
            stale.clone(),
            preferred.clone(),
            preferred.clone(),
            ClientInput::EndTurn,
        ],
    });
    for epsilon in [0, 50_000, 1_000_000] {
        let settings = TurnOptionGeneratorConfig {
            uniform_exploration_ppm: epsilon,
            ..config()
        };
        let stepper = TinyTurnStepper::with_selection();
        let mut session =
            TurnOptionGeneratorSession::with_policy(source.clone(), settings, policy.clone());
        let completed = finish(&mut session, &stepper);
        assert_eq!(completed.status, TurnOptionGenerationStatus::Complete);
        assert_eq!(stepper.call_count(&preferred), 1);
        assert_eq!(stepper.call_count(&stale), 0);
        assert_eq!(session.completed_options().len(), 2);
        let probabilities: Vec<_> = session
            .completed_options()
            .iter()
            .map(|option| (-option.negative_log_policy()).exp())
            .collect();
        assert!(probabilities.iter().all(|p| *p > 0.0));
        assert!((probabilities.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        if epsilon < 1_000_000 {
            assert_eq!(session.completed_options()[0].actions()[0].input, preferred);
            assert!((probabilities[0] - (1.0 - epsilon.max(1) as f64 / 2_000_000.0)).abs() < 1e-12);
        }
        let mut split =
            TurnOptionGeneratorSession::with_policy(source.clone(), settings, policy.clone());
        let split_stepper = TinyTurnStepper::with_selection();
        let mut last = None;
        for i in 0..64 {
            last = Some(split.advance(
                &split_stepper,
                CombatPlanningQuantum::deterministic(1, if i == 0 { 1024 } else { 0 }),
            ));
            if split.is_finished() {
                break;
            }
        }
        assert!(split.is_finished());
        assert_eq!(completed.after, last.unwrap().after);
        for (a, b) in session
            .completed_options()
            .iter()
            .zip(split.completed_options())
        {
            assert_eq!(a.actions(), b.actions());
            assert_eq!(a.exact_successor_hash(), b.exact_successor_hash());
            replay_turn_option(&source, a, &stepper, ReplayLimits::deterministic(64)).unwrap();
        }
    }
}

#[test]
fn stale_or_invalid_sparse_selection_is_neutral() {
    let source = ordered_selection_root();
    let mut other = source.position().clone();
    other.combat.entities.player.current_hp -= 1;
    for (bound, suggestions) in [
        (source.position().clone(), vec![ClientInput::EndTurn]),
        (other, vec![ClientInput::Cancel]),
    ] {
        let policy = Arc::new(SparseSelectionPolicy {
            source: bound,
            suggestions,
        });
        let mut session = TurnOptionGeneratorSession::with_policy(source.clone(), config(), policy);
        let stepper = TinyTurnStepper::with_selection();
        let actual = finish(&mut session, &stepper);
        let mut reference = TurnOptionGeneratorSession::new(source.clone(), config());
        let expected = finish(&mut reference, &TinyTurnStepper::with_selection());
        assert_eq!(actual.after, expected.after);
        for (a, b) in session
            .completed_options()
            .iter()
            .zip(reference.completed_options())
        {
            assert_eq!(a.actions(), b.actions());
            assert_eq!(a.negative_log_policy(), b.negative_log_policy());
        }
    }
}

#[test]
fn sparse_selection_retries_the_preferred_member_without_losing_fallback() {
    use sts_core::state::selection::{SelectionResolution, SelectionScope};
    let source = ordered_selection_root();
    let preferred = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
        SelectionScope::Hand,
        sts_core::test_support::scenario_lineages([22, 11]),
    ));
    let policy = combat_plan_selection_timing_policy_v1(combat_plan_state_guide_policy_v1(
        Arc::new(SparseSelectionPolicy {
            source: source.position().clone(),
            suggestions: vec![preferred.clone()],
        }),
    ));
    let stepper = TinyTurnStepper::with_selection();
    stepper.timeout_next_selection.store(true, Ordering::SeqCst);
    let mut session = TurnOptionGeneratorSession::with_policy(source.clone(), config(), policy);
    let paused = session.advance(&stepper, CombatPlanningQuantum::deterministic(2, 8));
    assert_eq!(
        paused.status,
        TurnOptionGenerationStatus::Partial(GenerationInterruption::Deadline)
    );
    assert_eq!(stepper.call_count(&preferred), 1);
    assert!(session.completed_options().is_empty());
    finish(&mut session, &stepper);
    assert_eq!(stepper.call_count(&preferred), 2);
    assert_eq!(session.completed_options().len(), 2);
    assert!(
        (session
            .completed_options()
            .iter()
            .map(|o| (-o.negative_log_policy()).exp())
            .sum::<f64>()
            - 1.0)
            .abs()
            < 1e-12
    );
    for option in session.completed_options() {
        replay_turn_option(&source, option, &stepper, ReplayLimits::deterministic(64)).unwrap();
    }
}

#[test]
fn lazy_selection_settles_one_edge_after_reserving_engine_work() {
    let stepper = TinyTurnStepper::with_selection();
    let source = ordered_selection_root();
    let mut session = TurnOptionGeneratorSession::new(source.clone(), config());
    let blocked = session.advance(&stepper, CombatPlanningQuantum::deterministic(2, 3));
    assert_eq!(
        blocked.status,
        TurnOptionGenerationStatus::Partial(GenerationInterruption::EngineStepBudget)
    );
    assert!(stepper.calls.lock().unwrap().is_empty());
    assert_eq!(
        session.live_work_counts_at_exact_position(source.position()),
        (0, 0, 1)
    );

    let settled = session.advance(&stepper, CombatPlanningQuantum::deterministic(0, 1));
    assert_eq!(settled.after_diagnostics.applied_action_transitions, 1);
    assert_eq!(stepper.calls.lock().unwrap().len(), 1);
    assert_eq!(
        session.live_work_counts_at_exact_position(source.position()),
        (0, 0, 1)
    );
    assert!(session.completed_options().is_empty());

    let report = finish(&mut session, &stepper);
    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    assert_eq!(session.completed_options().len(), 2);
    let masses: Vec<_> = session
        .completed_options()
        .iter()
        .map(|option| (-option.negative_log_policy()).exp())
        .collect();
    assert!(masses.iter().all(|mass| (*mass - 0.5).abs() < 1e-12));
}

#[test]
fn timed_out_selection_retries_the_same_member_and_keeps_both_orders() {
    let stepper = TinyTurnStepper::with_selection();
    stepper.timeout_next_selection.store(true, Ordering::SeqCst);
    let source = ordered_selection_root();
    let mut session = TurnOptionGeneratorSession::new(source.clone(), config());
    let paused = session.advance(&stepper, CombatPlanningQuantum::deterministic(2, 8));
    assert_eq!(
        paused.status,
        TurnOptionGenerationStatus::Partial(GenerationInterruption::Deadline)
    );
    assert_eq!(paused.after_diagnostics.applied_action_transitions, 0);
    assert!(session.completed_options().is_empty());
    assert_eq!(
        session.live_work_counts_at_exact_position(source.position()),
        (0, 0, 1)
    );
    let retried = session.advance(&stepper, CombatPlanningQuantum::deterministic(1, 8));
    assert_eq!(retried.after_diagnostics.applied_action_transitions, 1);
    let calls = stepper.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], calls[1]);
    finish(&mut session, &stepper);
    let mut reference = TurnOptionGeneratorSession::new(source.clone(), config());
    finish(&mut reference, &TinyTurnStepper::with_selection());
    let hashes = |generator: &TurnOptionGeneratorSession| {
        generator
            .completed_options()
            .iter()
            .map(|option| option.exact_successor_hash().to_owned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(hashes(&session), hashes(&reference));
    for option in session.completed_options() {
        replay_turn_option(&source, option, &stepper, ReplayLimits::deterministic(64)).unwrap();
    }
}

#[test]
fn singleton_selection_member_policy_reorders_without_removing_siblings() {
    let stepper = TinyTurnStepper::with_single_selection();
    let mut uniform = TurnOptionGeneratorSession::new(root(), config());
    finish(&mut uniform, &stepper);
    let uniform_members = uniform
        .completed_options()
        .iter()
        .flat_map(|option| option.actions())
        .filter_map(|action| match &action.input {
            ClientInput::SubmitSelection(resolution) => Some(resolution.selected_card_lineages()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        uniform_members,
        vec![
            sts_core::test_support::scenario_lineages([11]),
            sts_core::test_support::scenario_lineages([22]),
        ]
    );

    let mut guided = TurnOptionGeneratorSession::with_policy(
        root(),
        config(),
        Arc::new(PreferSelection22Policy),
    );
    finish(&mut guided, &stepper);
    let guided_members = guided
        .completed_options()
        .iter()
        .flat_map(|option| option.actions())
        .filter_map(|action| match &action.input {
            ClientInput::SubmitSelection(resolution) => Some(resolution.selected_card_lineages()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        guided_members,
        vec![
            sts_core::test_support::scenario_lineages([22]),
            sts_core::test_support::scenario_lineages([11]),
        ]
    );
}

#[test]
fn real_engine_preserves_targeted_potion_inside_an_exact_option() {
    let mut combat = sts_core::test_support::blank_test_combat();
    let monster = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    let target = monster.id;
    combat.entities.monsters = vec![monster];
    combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 7))];
    combat.zones.hand.clear();
    let root = CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
        .unwrap();
    let stepper = EngineCombatStepper;
    let mut session = TurnOptionGeneratorSession::new(
        root.clone(),
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 256,
            ..TurnOptionGeneratorConfig::default()
        },
    );

    let report = session.advance(&stepper, CombatPlanningQuantum::deterministic(1_000, 8_192));
    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    let option = session
        .completed_options()
        .iter()
        .find(|option| {
            option.actions().iter().any(|action| {
                action.input
                    == ClientInput::UsePotion {
                        potion_index: 0,
                        target: Some(target),
                    }
            })
        })
        .expect("targeted Fire Potion should survive option generation");

    let replay = replay_turn_option(
        &root,
        option,
        &stepper,
        ReplayLimits::deterministic(option.engine_steps()),
    )
    .unwrap();
    assert_eq!(replay.position, *option.exact_successor());
    assert!(option.exact_successor().combat.entities.potions[0].is_none());
    assert!(
        option.exact_successor().combat.entities.monsters[0].current_hp
            < root.position().combat.entities.monsters[0].current_hp
    );
}

#[test]
fn real_engine_never_publishes_an_effectless_elixir_turn_option() {
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
    combat.entities.potions = vec![Some(Potion::new(PotionId::Elixir, 7))];
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 11)];
    let root = CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
        .unwrap();
    let stepper = EngineCombatStepper;
    let mut session = TurnOptionGeneratorSession::new(
        root,
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 256,
            ..TurnOptionGeneratorConfig::default()
        },
    );

    let report = session.advance(
        &stepper,
        CombatPlanningQuantum::deterministic(10_000, 65_536),
    );
    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    let elixir_options = session
        .completed_options()
        .iter()
        .filter(|option| {
            option.actions().iter().any(|action| {
                matches!(
                    action.input,
                    ClientInput::UsePotion {
                        potion_index: 0,
                        ..
                    }
                )
            })
        })
        .collect::<Vec<_>>();

    assert!(
        !elixir_options.is_empty(),
        "material Elixir lines must remain available"
    );
    for option in elixir_options {
        assert!(
            option
                .actions()
                .iter()
                .all(|action| !matches!(action.input, ClientInput::Cancel)),
            "automatic Elixir lines must not cancel after consuming the potion"
        );
        assert!(option.actions().iter().any(|action| {
            matches!(
                &action.input,
                ClientInput::SubmitSelection(resolution)
                    if !resolution.selected_card_lineages().is_empty()
            )
        }));
    }
}

#[test]
fn explicit_zero_potion_generator_phase_removes_potion_expenditure_inputs() {
    let mut combat = sts_core::test_support::blank_test_combat();
    let monster = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    combat.entities.monsters = vec![monster];
    combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 7))];
    combat.zones.hand.clear();
    let root = CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
        .unwrap();
    let stepper = EngineCombatStepper;
    let mut session = TurnOptionGeneratorSession::new(
        root,
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 256,
            allow_potion_expenditure: false,
            ..TurnOptionGeneratorConfig::default()
        },
    );

    let report = session.advance(&stepper, CombatPlanningQuantum::deterministic(1_000, 8_192));
    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    assert!(
        session
            .completed_options()
            .iter()
            .all(|option| option.actions().iter().all(|action| !matches!(
                action.input,
                ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
            ))),
        "a zero-potion phase must not generate potion use or discard lines"
    );
}

#[test]
fn semantic_potion_generator_keeps_use_but_omits_discard() {
    let mut combat = sts_core::test_support::blank_test_combat();
    let mut monster = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    monster.current_hp = 20;
    monster.max_hp = 20;
    combat.entities.monsters = vec![monster];
    combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 7))];
    combat.zones.hand.clear();
    let root = CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
        .unwrap();
    let stepper = EngineCombatStepper;
    let mut session = TurnOptionGeneratorSession::new(
        root,
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 256,
            allow_potion_discard: false,
            ..TurnOptionGeneratorConfig::default()
        },
    );

    let report = session.advance(&stepper, CombatPlanningQuantum::deterministic(1_000, 8_192));
    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    let inputs = session
        .completed_options()
        .iter()
        .flat_map(|option| option.actions())
        .map(|action| &action.input)
        .collect::<Vec<_>>();
    assert!(inputs
        .iter()
        .any(|input| matches!(input, ClientInput::UsePotion { .. })));
    assert!(inputs
        .iter()
        .all(|input| !matches!(input, ClientInput::DiscardPotion(_))));
}

#[test]
fn potion_slot_mask_filters_inputs_without_mutating_the_exact_root() {
    let mut combat = sts_core::test_support::blank_test_combat();
    let monster = sts_core::test_support::planned_monster(EnemyId::TheGuardian, 1);
    combat.entities.monsters = vec![monster];
    combat.entities.potions = vec![
        Some(Potion::new(PotionId::EnergyPotion, 7)),
        Some(Potion::new(PotionId::EnergyPotion, 8)),
    ];
    combat.zones.hand.clear();
    let root = CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
        .unwrap();
    let root_potions = root.position().combat.entities.potions.clone();
    let stepper = EngineCombatStepper;
    let mut session = TurnOptionGeneratorSession::new(
        root.clone(),
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 256,
            allowed_potion_slots: Some(1_u64 << 1),
            ..TurnOptionGeneratorConfig::default()
        },
    );

    let report = session.advance(
        &stepper,
        CombatPlanningQuantum::deterministic(4_000, 32_768),
    );
    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    let potion_inputs = session
        .completed_options()
        .iter()
        .flat_map(|option| option.actions())
        .filter_map(|action| match action.input {
            ClientInput::UsePotion { potion_index, .. } => Some(potion_index),
            ClientInput::DiscardPotion(slot) => Some(slot),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !potion_inputs.is_empty(),
        "the allowed potion slot should remain searchable"
    );
    assert!(
        potion_inputs.iter().all(|slot| *slot == 1),
        "the disallowed slot leaked into generated options: {potion_inputs:?}"
    );
    assert_eq!(
        root.position().combat.entities.potions,
        root_potions,
        "slot filtering must not alter the exact root inventory"
    );
}

#[test]
fn finite_potion_generator_limit_prunes_over_budget_prefixes() {
    let mut combat = sts_core::test_support::blank_test_combat();
    let monster = sts_core::test_support::planned_monster(EnemyId::TheGuardian, 1);
    combat.entities.monsters = vec![monster];
    combat.entities.potions = vec![
        Some(Potion::new(PotionId::EnergyPotion, 7)),
        Some(Potion::new(PotionId::EnergyPotion, 8)),
    ];
    combat.zones.hand.clear();
    let root = CombatDecisionRoot::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
        .unwrap();
    let stepper = EngineCombatStepper;
    let mut session = TurnOptionGeneratorSession::with_policy_and_potion_limit(
        root,
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 256,
            ..TurnOptionGeneratorConfig::default()
        },
        Arc::new(PreferPlayPolicy),
        Some(1),
    );

    let report = session.advance(
        &stepper,
        CombatPlanningQuantum::deterministic(4_000, 32_768),
    );
    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
    let potion_expenditures = session
        .completed_options()
        .iter()
        .map(|option| {
            option
                .actions()
                .iter()
                .filter(|action| {
                    matches!(
                        action.input,
                        ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
                    )
                })
                .count()
        })
        .collect::<Vec<_>>();
    assert!(
        potion_expenditures.contains(&1),
        "the finite allowance must retain legal one-potion turn lines"
    );
    assert!(
        potion_expenditures.iter().all(|count| *count <= 1),
        "the generator must not spend work completing over-budget turn lines"
    );
}
