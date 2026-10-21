use super::*;
use sts_core::state::selection::{SelectionResolution, SelectionScope};

fn selection(ids: &[u32]) -> ClientInput {
    ClientInput::SubmitSelection(SelectionResolution::card_lineages(
        SelectionScope::Hand,
        sts_core::test_support::scenario_lineages(ids.iter().copied()),
    ))
}

fn selection_root(single: bool) -> CombatDecisionRoot {
    let mut position = root().position().clone();
    position.combat.turn.energy = 0;
    position
        .combat
        .zones
        .hand
        .push(CombatCard::for_scenario(CardId::Wound, 33));
    position.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([11, 22, 33]),
        min_cards: if single { 1 } else { 2 },
        max_cards: if single { 1 } else { 2 },
        can_cancel: false,
        reason: HandSelectReason::Discard,
    });
    CombatDecisionRoot::new(position).unwrap()
}

fn snapshot(session: &TurnOptionGeneratorSession) -> Vec<(Vec<TurnOptionAction>, f64)> {
    session
        .completed_options()
        .iter()
        .map(|o| (o.actions().to_vec(), o.negative_log_policy()))
        .collect()
}

#[test]
fn selection_intent_edits_preserve_budget_results_and_each_remaining_order() {
    for single in [false, true] {
        for pause in 0..=16 {
            let stepper = TinyTurnStepper::with_selection();
            let root = selection_root(single);
            let family = &stepper
                .legal_action_surface(root.position())
                .selection_families[0];
            let mut cursor = SelectionTransactionCursor::new(family).unwrap();
            let members: Vec<_> = std::iter::from_fn(|| cursor.next_input()).collect();
            let mut session = TurnOptionGeneratorSession::new(root.clone(), config());
            let source = session.root_selection_intent().source;
            session
                .replace_root_selection_intent(&source, &members[members.len() - 1..], &stepper)
                .unwrap();
            session.advance(&stepper, CombatPlanningQuantum::deterministic(pause, 1024));
            let used = session.counters();
            let granted = session.granted_budget();
            let retained = snapshot(&session);
            let calls = stepper.calls.lock().unwrap().clone();
            for replacement in [&members[..1], &[][..], &members[1..2], &members[..1]] {
                session
                    .replace_root_selection_intent(&source, replacement, &stepper)
                    .unwrap();
                assert_eq!(session.root_selection_intent().members, replacement);
                assert_eq!(session.counters(), used);
                assert_eq!(session.granted_budget(), granted);
                assert_eq!(snapshot(&session), retained);
                assert_eq!(*stepper.calls.lock().unwrap(), calls);
            }
            finish(&mut session, &stepper);
            assert!(session.is_finished());
            for input in &members {
                assert_eq!(stepper.call_count(input), 1, "pause {pause}, {input:?}");
            }
            assert_eq!(&snapshot(&session)[..retained.len()], retained);
            let mut baseline = TurnOptionGeneratorSession::new(root.clone(), config());
            finish(&mut baseline, &TinyTurnStepper::with_selection());
            let states = |s: &TurnOptionGeneratorSession| {
                s.completed_options()
                    .iter()
                    .map(|o| o.exact_successor_hash().to_owned())
                    .collect::<std::collections::BTreeSet<_>>()
            };
            assert_eq!(states(&session), states(&baseline));
            let probabilities = |s: &TurnOptionGeneratorSession| {
                s.completed_options()
                    .iter()
                    .map(|o| (-o.negative_log_policy()).exp())
                    .sum::<f64>()
            };
            assert!((probabilities(&session) - probabilities(&baseline)).abs() < 1e-12);
            for option in session.completed_options() {
                replay_turn_option(&root, option, &stepper, ReplayLimits::deterministic(64))
                    .unwrap();
            }
        }
    }
}

#[test]
fn selection_intent_validation_is_atomic_and_bound_to_exact_source() {
    let stepper = TinyTurnStepper::with_selection();
    let mut session = TurnOptionGeneratorSession::new(selection_root(false), config());
    let source = session.root_selection_intent().source;
    let valid = selection(&[33, 22]);
    session
        .replace_root_selection_intent(&source, &[valid.clone(), valid.clone()], &stepper)
        .unwrap();
    assert_eq!(session.root_selection_intent().members, vec![valid.clone()]);
    let bad_members = [
        ClientInput::EndTurn,
        selection(&[33, 99]),
        selection(&[33, 33]),
        selection(&[11]),
    ];
    for invalid in bad_members {
        assert_eq!(
            session.replace_root_selection_intent(&source, &[valid.clone(), invalid], &stepper),
            Err(SelectionIntentError::InvalidMember { index: 1 })
        );
    }
    assert_eq!(
        session.replace_root_selection_intent(&source, &vec![valid.clone(); 5], &stepper),
        Err(SelectionIntentError::TooManyMembers { limit: 4 })
    );
    let other =
        TurnOptionGeneratorSession::new(selection_root(true), config()).root_selection_intent();
    assert_eq!(
        session.replace_root_selection_intent(&other.source, &[], &stepper),
        Err(SelectionIntentError::StaleSource)
    );
    assert_eq!(session.root_selection_intent().members, vec![valid]);
    assert_eq!(session.counters(), CombatPlanningCounters::default());
    assert_eq!(session.granted_budget(), CombatPlanningCounters::default());
    assert!(stepper.calls.lock().unwrap().is_empty());
}

#[test]
fn selection_intent_split_grants_and_noop_edits_keep_the_same_schedule() {
    for pause in 0..12 {
        let run = |split: bool| {
            let stepper = TinyTurnStepper::with_selection();
            let mut session = TurnOptionGeneratorSession::with_policy(
                selection_root(false),
                config(),
                Arc::new(SharedGuidePolicy),
            );
            let source = session.root_selection_intent().source;
            session
                .replace_root_selection_intent(&source, &[selection(&[33, 22])], &stepper)
                .unwrap();
            session.advance(&stepper, CombatPlanningQuantum::deterministic(pause, 1024));
            session
                .replace_root_selection_intent(&source, &[selection(&[22, 11])], &stepper)
                .unwrap();
            if split {
                for _ in 0..100 {
                    let intent = session.root_selection_intent();
                    session
                        .replace_root_selection_intent(&intent.source, &intent.members, &stepper)
                        .unwrap();
                    session.advance(&stepper, CombatPlanningQuantum::deterministic(1, 0));
                }
            } else {
                session.advance(&stepper, CombatPlanningQuantum::deterministic(100, 0));
            }
            assert!(session.is_finished());
            let calls = stepper.calls.lock().unwrap().clone();
            (
                snapshot(&session),
                session.counters(),
                session.granted_budget(),
                calls,
            )
        };
        assert_eq!(run(false), run(true));
    }
}

#[test]
fn selection_intent_withdrawal_after_timeout_retries_without_duplicate_settlement() {
    let stepper = TinyTurnStepper::with_selection();
    let root = selection_root(false);
    let mut session = TurnOptionGeneratorSession::new(root.clone(), config());
    let source = session.root_selection_intent().source;
    let preferred = selection(&[33, 22]);
    session
        .replace_root_selection_intent(&source, std::slice::from_ref(&preferred), &stepper)
        .unwrap();
    stepper.timeout_next_selection.store(true, Ordering::SeqCst);
    let paused = session.advance(&stepper, CombatPlanningQuantum::deterministic(2, 8));
    assert_eq!(
        paused.status,
        TurnOptionGenerationStatus::Partial(GenerationInterruption::Deadline)
    );
    assert_eq!(stepper.call_count(&preferred), 1);
    session
        .replace_root_selection_intent(&source, &[], &stepper)
        .unwrap();
    session
        .replace_root_selection_intent(&source, &[selection(&[22, 11])], &stepper)
        .unwrap();
    finish(&mut session, &stepper);
    assert_eq!(stepper.call_count(&preferred), 2);
    assert_eq!(stepper.call_count(&selection(&[22, 11])), 1);
    for option in session.completed_options() {
        replay_turn_option(&root, option, &stepper, ReplayLimits::deterministic(64)).unwrap();
    }
}

#[test]
fn draft_selection_intent_retains_option_handles_and_returns_with_undo() {
    let stepper = TinyTurnStepper::with_selection();
    let mut draft = CombatDraftSession::new(selection_root(false), config());
    let source = draft.root_selection_intent().unwrap().source;
    let preferred = selection(&[33, 22]);
    draft
        .replace_root_selection_intent(&source, std::slice::from_ref(&preferred), &stepper)
        .unwrap();
    draft
        .advance_options(&stepper, CombatPlanningQuantum::deterministic(100, 1024))
        .unwrap();
    let handles = draft.option_handles();
    let used = draft.current_generation_counters();
    let first = draft.option(handles[0]).unwrap().clone();
    draft
        .replace_root_selection_intent(&source, &[], &stepper)
        .unwrap();
    assert_eq!(draft.option(handles[0]).unwrap(), &first);
    draft
        .replace_root_selection_intent(&source, std::slice::from_ref(&preferred), &stepper)
        .unwrap();
    draft.choose(handles[0], &stepper).unwrap();
    assert!(draft.root_selection_intent().unwrap().members.is_empty());
    assert_eq!(
        draft.replace_root_selection_intent(&source, &[], &stepper),
        Err(CombatDraftError::SelectionIntent(
            SelectionIntentError::StaleSource
        ))
    );
    draft.undo().unwrap();
    assert_eq!(draft.option_handles(), handles);
    assert_eq!(draft.current_generation_counters(), used);
    assert_eq!(
        draft.root_selection_intent().unwrap().members,
        vec![preferred]
    );
}

#[test]
fn nested_selection_edits_reuse_the_tree_and_preserve_exact_coverage() {
    for single in [false, true] {
        for pause in 0..=14 {
            let stepper = if single {
                TinyTurnStepper::with_single_selection()
            } else {
                TinyTurnStepper::with_selection()
            };
            let root = root();
            let mut session = TurnOptionGeneratorSession::new(root.clone(), config());
            assert_eq!(session.selection_sources(&stepper, 0, 8).total, 0);
            // Root expansion, then the play opening a real retained choice.
            session.advance(&stepper, CombatPlanningQuantum::deterministic(2, 1024));
            let page = session.selection_sources(&stepper, 0, 8);
            assert_eq!(page.total, 1);
            let source = page.sources[0].clone();
            assert_eq!(source.action_count(), 1);
            assert_eq!(source.actions()[0].input, PLAY);
            let mut cursor = SelectionTransactionCursor::new(
                &stepper
                    .legal_action_surface(source.position())
                    .selection_families[0],
            )
            .unwrap();
            let members: Vec<_> = std::iter::from_fn(|| cursor.next_input()).collect();
            session
                .replace_selection_intent(&source, &members[1..], &stepper)
                .unwrap();
            session.advance(&stepper, CombatPlanningQuantum::deterministic(pause, 0));
            let retained = snapshot(&session);
            let used = session.counters();
            let granted = session.granted_budget();
            let calls = stepper.calls.lock().unwrap().clone();
            for replacement in [&members[..1], &[][..], &members[1..]] {
                session
                    .replace_selection_intent(&source, replacement, &stepper)
                    .unwrap();
                assert_eq!(session.counters(), used);
                assert_eq!(session.granted_budget(), granted);
                assert_eq!(snapshot(&session), retained);
                assert_eq!(*stepper.calls.lock().unwrap(), calls);
            }
            finish(&mut session, &stepper);
            assert!(session.is_finished());
            assert_eq!(session.selection_continuation_work(&source), Ok(0));
            assert_eq!(session.selection_sources(&stepper, 0, 8).total, 0);
            for member in &members {
                assert_eq!(stepper.call_count(member), 1, "pause={pause}");
            }
            let mut baseline = TurnOptionGeneratorSession::new(root.clone(), config());
            finish(&mut baseline, &stepper);
            let outcomes = |s: &TurnOptionGeneratorSession| {
                let mut outcomes: Vec<_> = s
                    .completed_options()
                    .iter()
                    .map(|o| {
                        (
                            o.exact_successor_hash().to_owned(),
                            o.negative_log_policy().to_bits(),
                        )
                    })
                    .collect();
                outcomes.sort();
                outcomes
            };
            assert_eq!(outcomes(&session), outcomes(&baseline));
            for option in session.completed_options() {
                replay_turn_option(&root, option, &stepper, ReplayLimits::deterministic(64))
                    .unwrap();
            }
            let mut independent = TurnOptionGeneratorSession::new(root.clone(), config());
            assert_eq!(
                independent.replace_selection_intent(&source, &members[..1], &stepper),
                Err(SelectionIntentError::StaleSource)
            );
        }
    }
}

#[test]
fn nested_intention_split_grants_match_and_preferred_descendants_get_service() {
    let run = |split| {
        let stepper = TinyTurnStepper::with_selection();
        let mut session = TurnOptionGeneratorSession::new(root(), config());
        session.advance(&stepper, CombatPlanningQuantum::deterministic(2, 1024));
        let source = session.selection_sources(&stepper, 0, 8).sources.remove(0);
        session
            .replace_selection_intent(&source, &[selection(&[22, 11])], &stepper)
            .unwrap();
        if split {
            for _ in 0..20 {
                session.advance(&stepper, CombatPlanningQuantum::deterministic(1, 0));
            }
        } else {
            session.advance(&stepper, CombatPlanningQuantum::deterministic(20, 0));
        }
        let calls = stepper.calls.lock().unwrap().clone();
        let selections: Vec<_> = calls
            .iter()
            .filter(|i| matches!(i, ClientInput::SubmitSelection(_)))
            .cloned()
            .collect();
        assert_eq!(selections[0], selection(&[22, 11]));
        (
            snapshot(&session),
            calls,
            session.counters(),
            session.granted_budget(),
        )
    };
    assert_eq!(run(false), run(true));
}

struct ForkedChoiceStepper(TinyTurnStepper);

impl CombatStepper for ForkedChoiceStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        if position.combat.turn.energy > 0
            && matches!(position.engine, EngineState::CombatPlayerTurn)
        {
            vec![
                PLAY,
                ClientInput::PlayCard {
                    card_index: 1,
                    target: None,
                },
                ClientInput::EndTurn,
            ]
        } else {
            self.0.atomic_actions(position)
        }
    }
    fn legal_action_surface(&self, position: &CombatPosition) -> CombatLegalActionSurfaceV2 {
        if matches!(position.engine, EngineState::PendingChoice(_)) {
            self.0.legal_action_surface(position)
        } else {
            CombatLegalActionSurfaceV2 {
                atomic_actions: self.atomic_actions(position),
                selection_families: vec![],
                indexed_choice: None,
            }
        }
    }
    fn supports_canonical_pending_choice_actions(&self) -> bool {
        true
    }
    fn is_legal_action(&self, position: &CombatPosition, input: &ClientInput) -> bool {
        self.atomic_actions(position).contains(input) || self.0.is_legal_action(position, input)
    }
    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        self.0.terminal(position)
    }
    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        limits: CombatStepLimits,
    ) -> CombatStepResult {
        if input
            == (ClientInput::PlayCard {
                card_index: 1,
                target: None,
            })
        {
            let mut result = self.0.apply_to_stable(position, PLAY, limits);
            result.position.combat.entities.player.block = 7;
            result
        } else {
            self.0.apply_to_stable(position, input, limits)
        }
    }
}

#[test]
fn identical_choice_tables_keep_distinct_retained_sources_and_focus() {
    let stepper = ForkedChoiceStepper(TinyTurnStepper::with_selection());
    let mut session = TurnOptionGeneratorSession::new(root(), config());
    session.advance(&stepper, CombatPlanningQuantum::deterministic(3, 1024));
    let sources = session.selection_sources(&stepper, 0, 16).sources;
    assert_eq!(sources.len(), 2);
    assert_ne!(sources[0], sources[1]);
    assert_eq!(
        stepper
            .legal_action_surface(sources[0].position())
            .selection_families,
        stepper
            .legal_action_surface(sources[1].position())
            .selection_families
    );
    assert_ne!(sources[0].actions(), sources[1].actions());
    session
        .replace_selection_intent(&sources[1], &[selection(&[22, 11])], &stepper)
        .unwrap();
    let before = session.counters();
    session
        .replace_selection_intent(&sources[0], &[selection(&[11, 22])], &stepper)
        .unwrap();
    assert_eq!(session.selection_intent().source, sources[0]);
    assert_eq!(session.counters(), before);
    session
        .replace_selection_intent(&sources[1], &[selection(&[22, 11])], &stepper)
        .unwrap();
    session.advance(&stepper, CombatPlanningQuantum::deterministic(100, 100));
    // Each exact origin settles both orders. Merely sharing the visible table
    // must neither merge the source nor suppress an edge in its sibling.
    assert_eq!(stepper.0.call_count(&selection(&[11, 22])), 2);
    assert_eq!(stepper.0.call_count(&selection(&[22, 11])), 2);
    for option in session.completed_options() {
        replay_turn_option(&root(), option, &stepper, ReplayLimits::deterministic(64)).unwrap();
    }
}

#[test]
fn selection_outcomes_bind_the_full_prefix_and_keep_every_observed_order() {
    let stepper = ForkedChoiceStepper(TinyTurnStepper::with_selection());
    let mut draft = CombatDraftSession::new(root(), config());
    draft
        .advance_options(&stepper, CombatPlanningQuantum::deterministic(3, 1024))
        .unwrap();
    let sources = draft.selection_sources(&stepper, 0, 16).unwrap().sources;
    assert_eq!(sources.len(), 2);
    assert_eq!(
        draft.selection_outcomes(&sources[0], 0, 8).unwrap().total,
        0
    );
    draft
        .advance_options(&stepper, CombatPlanningQuantum::deterministic(100, 1024))
        .unwrap();
    let before = draft.current_generation_counters();
    let calls = stepper.0.calls.lock().unwrap().clone();
    let mut all = std::collections::HashSet::new();
    for source in &sources {
        let page = draft.selection_outcomes(source, 0, 16).unwrap();
        assert_eq!(page.total, 2);
        let mut paged = Vec::new();
        for offset in 0..page.total {
            paged.extend(
                draft
                    .selection_outcomes(source, offset, 1)
                    .unwrap()
                    .outcomes,
            );
        }
        assert_eq!(page.outcomes, paged);
        assert!(draft
            .selection_outcomes(source, usize::MAX, 16)
            .unwrap()
            .outcomes
            .is_empty());
        for outcome in &page.outcomes {
            assert!(all.insert(outcome.handle));
            let option = draft.option(outcome.handle).unwrap();
            assert_eq!(option.actions()[0], source.actions()[0]);
            assert_eq!(
                outcome.selection,
                option.actions()[source.action_count()].input
            );
            let mut wrong_prefix = option.actions().to_vec();
            wrong_prefix[0].expected_successor_hash = "unrelated exact state".into();
            assert!(source.selected_input_in(&wrong_prefix).is_none());
            wrong_prefix = option.actions().to_vec();
            wrong_prefix[0].input = ClientInput::EndTurn;
            assert!(source.selected_input_in(&wrong_prefix).is_none());
        }
        draft
            .replace_selection_intent(source, &[selection(&[22, 11])], &stepper)
            .unwrap();
        let prioritized = draft.selection_outcomes(source, 0, 16).unwrap();
        assert_eq!(prioritized.outcomes[0].selection, selection(&[22, 11]));
        assert_eq!(
            draft.selection_outcomes(source, 0, 1).unwrap().outcomes,
            prioritized.outcomes[..1]
        );
        let mut generation_order = prioritized;
        generation_order
            .outcomes
            .sort_by_key(|outcome| outcome.handle.ordinal());
        assert_eq!(generation_order, page);
        draft
            .replace_selection_intent(source, &[], &stepper)
            .unwrap();
        assert_eq!(draft.selection_outcomes(source, 0, 16).unwrap(), page);
    }
    assert_eq!(draft.current_generation_counters(), before);
    assert_eq!(*stepper.0.calls.lock().unwrap(), calls);
    let page = draft.selection_outcomes(&sources[0], 0, 16).unwrap();
    draft.choose(page.outcomes[0].handle, &stepper).unwrap();
    assert_eq!(
        draft.selection_outcomes(&sources[0], 0, 16),
        Err(CombatDraftError::SelectionIntent(
            SelectionIntentError::StaleSource
        ))
    );
    draft.undo().unwrap();
    assert_eq!(draft.selection_outcomes(&sources[0], 0, 16).unwrap(), page);
}

#[test]
fn revisiting_a_child_restores_its_source_and_advice_without_crossing_siblings() {
    let stepper = TinyTurnStepper::with_selection();
    let mut draft = CombatDraftSession::new(root(), config());
    draft
        .advance_options(&stepper, CombatPlanningQuantum::deterministic(100, 1024))
        .unwrap();
    let parent_options = draft.option_handles();
    let branch = parent_options
        .iter()
        .copied()
        .find(|handle| draft.option(*handle).unwrap().actions().len() == 1)
        .unwrap();
    let sibling = parent_options
        .into_iter()
        .find(|handle| *handle != branch)
        .unwrap();
    draft.choose(branch, &stepper).unwrap();
    let source = (0..16)
        .find_map(|_| {
            draft
                .advance_options(&stepper, CombatPlanningQuantum::deterministic(1, 1024))
                .unwrap();
            draft
                .selection_sources(&stepper, 0, 8)
                .unwrap()
                .sources
                .first()
                .cloned()
        })
        .expect("observe the intermediate selection before its work is consumed");
    draft
        .replace_selection_intent(&source, &[selection(&[22, 11])], &stepper)
        .unwrap();
    draft
        .advance_options(&stepper, CombatPlanningQuantum::deterministic(100, 1024))
        .unwrap();
    let intent = draft.selection_intent().unwrap();
    let outcomes = draft.selection_outcomes(&source, 0, 16).unwrap();
    let counters = draft.current_generation_counters();
    draft.undo().unwrap();
    draft.choose(sibling, &stepper).unwrap();
    assert!(matches!(
        draft.validate_selection_source(&source),
        Err(CombatDraftError::SelectionIntent(
            SelectionIntentError::StaleSource
        ))
    ));
    assert!(draft.selection_intent().unwrap().members.is_empty());
    draft.undo().unwrap();
    draft.choose(branch, &stepper).unwrap();
    let restored = draft.selection_intent().unwrap();
    assert_eq!(restored.source, intent.source);
    assert_eq!(restored.members, intent.members);
    assert_eq!(draft.selection_outcomes(&source, 0, 16).unwrap(), outcomes);
    assert_eq!(draft.current_generation_counters(), counters);
}
