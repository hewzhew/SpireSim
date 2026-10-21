use super::*;

fn finish_current_turn(session: &mut CombatDraftSession, stepper: &TinyTurnStepper) {
    let report = session
        .advance_options(stepper, CombatPlanningQuantum::deterministic(100, 100))
        .expect("an active draft turn should advance");
    assert_eq!(report.status, TurnOptionGenerationStatus::Complete);
}

fn handle_with_boundary(
    session: &CombatDraftSession,
    boundary: CompleteTurnOptionBoundary,
) -> CombatDraftOptionHandle {
    session
        .option_handles()
        .into_iter()
        .find(|handle| session.option(*handle).unwrap().boundary() == boundary)
        .expect("requested complete-turn boundary")
}

#[test]
fn draft_selects_one_exact_turn_and_exposes_the_next_player_turn() {
    let stepper = TinyTurnStepper::plain();
    let mut draft = CombatDraftSession::new(root(), config());
    finish_current_turn(&mut draft, &stepper);
    let parent_turn = draft.current_root().unwrap().turn_count();
    let handle = handle_with_boundary(&draft, CompleteTurnOptionBoundary::NextPlayerTurn);

    assert_eq!(handle.to_string(), format!("T{}", handle.ordinal()));
    assert_eq!(
        draft.choose(handle, &stepper).unwrap(),
        CompleteTurnOptionBoundary::NextPlayerTurn
    );

    assert_eq!(draft.selected_turn_count(), 1);
    assert_eq!(draft.current_root().unwrap().turn_count(), parent_turn + 1);
    assert_eq!(draft.terminal_boundary(), None);
}

#[test]
fn draft_imports_exact_prefix_without_search_and_replays_it_on_export() {
    let stepper = TinyTurnStepper::lethal_after_current_turn();
    let prefix =
        materialize_exact_action_line(&stepper, root().position(), &[ClientInput::EndTurn], 100)
            .unwrap();
    let mut draft =
        CombatDraftSession::from_exact_prefix(root(), config(), prefix.actions.clone(), &stepper)
            .unwrap();
    assert_eq!(
        draft.current_root().unwrap().position(),
        &prefix.final_position
    );
    assert!(draft.option_handles().is_empty());
    assert_eq!(
        draft.current_generation_counters(),
        Some(CombatPlanningCounters::default())
    );
    assert_eq!(draft.undo(), Err(CombatDraftError::NoPreviousTurn));
    assert!(draft.victory_witness(&stepper).unwrap().is_none());
    finish_current_turn(&mut draft, &stepper);
    let win = handle_with_boundary(&draft, CompleteTurnOptionBoundary::TerminalWin);
    draft.choose(win, &stepper).unwrap();
    let witness = draft.victory_witness(&stepper).unwrap().unwrap();
    assert_eq!(&witness.actions[..1], prefix.actions.as_slice());
    assert_eq!(draft.selected_turn_count(), 2);
    let replayed = replay_atomic_actions(&stepper, root().position(), &witness.actions, 100)
        .unwrap()
        .0;
    assert_eq!(replayed, witness.final_position);
    draft.undo().unwrap();
    assert_eq!(draft.undo(), Err(CombatDraftError::NoPreviousTurn));

    let mut corrupt = prefix.actions;
    corrupt[0].expected_successor_hash = "wrong successor".into();
    assert!(matches!(
        CombatDraftSession::from_exact_prefix(root(), config(), corrupt, &stepper),
        Err(CombatDraftError::PrefixReplay(_))
    ));
    let midturn = materialize_exact_action_line(&stepper, root().position(), &[PLAY], 100).unwrap();
    assert!(matches!(
        CombatDraftSession::from_exact_prefix(root(), config(), midturn.actions, &stepper),
        Err(CombatDraftError::PrefixIsNotTurnBoundary)
    ));
}

#[test]
fn undo_restores_the_retained_parent_generator_without_regeneration() {
    let stepper = TinyTurnStepper::plain();
    let mut draft = CombatDraftSession::new(root(), config());
    finish_current_turn(&mut draft, &stepper);
    let parent_handles = draft.option_handles();
    let parent_counters = draft.current_generation_counters().unwrap();
    let selected = handle_with_boundary(&draft, CompleteTurnOptionBoundary::NextPlayerTurn);

    draft.choose(selected, &stepper).unwrap();
    assert_eq!(
        draft.option(selected),
        Err(CombatDraftError::StaleOptionHandle)
    );
    draft.undo().unwrap();

    assert_eq!(draft.option_handles(), parent_handles);
    assert_eq!(draft.current_generation_counters(), Some(parent_counters));
    assert!(draft.option(selected).is_ok());
}

#[test]
fn multi_turn_win_exports_one_fixed_root_victory_witness() {
    let stepper = TinyTurnStepper::lethal_after_current_turn();
    let mut draft = CombatDraftSession::new(root(), config());
    finish_current_turn(&mut draft, &stepper);
    let first_turn = handle_with_boundary(&draft, CompleteTurnOptionBoundary::NextPlayerTurn);
    draft.choose(first_turn, &stepper).unwrap();
    finish_current_turn(&mut draft, &stepper);
    let winning = handle_with_boundary(&draft, CompleteTurnOptionBoundary::TerminalWin);

    draft.choose(winning, &stepper).unwrap();
    let witness = draft
        .victory_witness(&stepper)
        .unwrap()
        .expect("an exact terminal win should export a witness");

    assert_eq!(
        draft.terminal_boundary(),
        Some(CompleteTurnOptionBoundary::TerminalWin)
    );
    assert_eq!(draft.selected_turn_count(), 2);
    assert!(witness.actions.len() >= 2);
    assert!(stepper.is_exact_victory(&witness.final_position));
    assert_eq!(witness.actions.len(), draft.exact_line().len());
    draft.undo().unwrap();
    draft.undo().unwrap();
    assert!(draft.victory_witness(&stepper).unwrap().is_none());
    let sibling = draft
        .option_handles()
        .into_iter()
        .find(|h| *h != first_turn)
        .unwrap();
    draft.choose(sibling, &stepper).unwrap();
    finish_current_turn(&mut draft, &stepper);
    draft.undo().unwrap();
    draft.choose(first_turn, &stepper).unwrap();
    draft.choose(winning, &stepper).unwrap();
    let restored = draft.victory_witness(&stepper).unwrap().unwrap();
    assert_eq!(restored.actions, witness.actions);
    assert_eq!(restored.final_position, witness.final_position);
}

#[test]
fn terminal_loss_never_exports_a_victory_witness_and_can_be_undone() {
    let stepper = TinyTurnStepper {
        terminal_loss: true,
        ..TinyTurnStepper::plain()
    };
    let mut draft = CombatDraftSession::new(root(), config());
    finish_current_turn(&mut draft, &stepper);
    let losing = handle_with_boundary(&draft, CompleteTurnOptionBoundary::TerminalLoss);
    let parent_handles = draft.option_handles();

    draft.choose(losing, &stepper).unwrap();
    assert!(draft.victory_witness(&stepper).unwrap().is_none());
    assert!(draft.current_root().is_none());
    assert!(draft.option_handles().is_empty());

    draft.undo().unwrap();
    assert_eq!(draft.terminal_boundary(), None);
    assert_eq!(draft.option_handles(), parent_handles);
}

#[test]
fn smoke_escape_never_exports_a_victory_witness() {
    let stepper = TinyTurnStepper::escaping();
    let mut draft = CombatDraftSession::new(root(), config());
    finish_current_turn(&mut draft, &stepper);
    let escape = handle_with_boundary(&draft, CompleteTurnOptionBoundary::Escape);

    draft.choose(escape, &stepper).unwrap();

    assert_eq!(
        draft.terminal_boundary(),
        Some(CompleteTurnOptionBoundary::Escape)
    );
    assert!(draft.victory_witness(&stepper).unwrap().is_none());
}

#[test]
fn option_handles_bind_the_retained_frame_and_cannot_cross_sessions() {
    let stepper = TinyTurnStepper::plain();
    let mut draft = CombatDraftSession::new(root(), config());
    finish_current_turn(&mut draft, &stepper);
    assert_eq!(
        draft.option_handle(usize::MAX),
        Err(CombatDraftError::UnknownOption {
            ordinal: usize::MAX
        })
    );

    let parent = handle_with_boundary(&draft, CompleteTurnOptionBoundary::NextPlayerTurn);
    draft.choose(parent, &stepper).unwrap();
    finish_current_turn(&mut draft, &stepper);
    let child = draft.option_handles()[0];
    let child_frame = draft.current_frame_handle();
    draft.undo().unwrap();
    assert_eq!(
        draft.option(child),
        Err(CombatDraftError::StaleOptionHandle)
    );
    let mut other = CombatDraftSession::new(root(), config());
    finish_current_turn(&mut other, &stepper);
    assert_eq!(
        other.option(parent),
        Err(CombatDraftError::StaleOptionHandle)
    );
    draft.choose(parent, &stepper).unwrap();
    assert_eq!(draft.current_frame_handle(), child_frame);
    assert!(draft.option(child).is_ok());
}

#[test]
fn sibling_and_nested_branches_resume_exact_work_and_failed_replay_keeps_them() {
    let stepper = TinyTurnStepper::plain();
    let mut draft = CombatDraftSession::new(root(), config());
    finish_current_turn(&mut draft, &stepper);
    let mut branches = draft.option_handles();
    branches.sort_by_key(|handle| draft.option(*handle).unwrap().actions().len());
    assert_eq!(branches.len(), 2);
    let parent_counters = draft.current_generation_counters();
    let parent_frame = draft.current_frame_handle();

    draft.choose(branches[0], &stepper).unwrap();
    draft
        .advance_options(&stepper, CombatPlanningQuantum::deterministic(1, 100))
        .unwrap();
    let first_handles = draft.option_handles();
    let first_counters = draft.current_generation_counters();
    let first_frame = draft.current_frame_handle();
    draft.undo().unwrap();
    assert!(draft.exact_line().is_empty());

    let calls = stepper.calls.lock().unwrap().len();
    let first = draft.option_continuation(branches[0]).unwrap().unwrap();
    assert_eq!(first.frame, first_frame.unwrap());
    assert_eq!(Some(first.counters), first_counters);
    assert_eq!(first.available_option_count, first_handles.len());
    assert_eq!(stepper.calls.lock().unwrap().len(), calls);
    assert!(draft.option_continuation(branches[1]).unwrap().is_none());

    draft.choose(branches[1], &stepper).unwrap();
    assert_eq!(draft.selected_turn_count(), 1);
    assert_ne!(draft.current_frame_handle(), first_frame);
    finish_current_turn(&mut draft, &stepper);
    let second_handles = draft.option_handles();
    let second_counters = draft.current_generation_counters();
    draft.undo().unwrap();
    assert_eq!(draft.current_generation_counters(), parent_counters);
    assert_eq!(draft.current_frame_handle(), parent_frame);

    // The second route plays a card; a changed successor must fail before
    // activation, even when its child has already been retained.
    let played = branches
        .iter()
        .copied()
        .find(|handle| {
            draft
                .option(*handle)
                .unwrap()
                .actions()
                .iter()
                .any(|action| action.input == PLAY)
        })
        .unwrap();
    stepper.successor_salt.store(1, Ordering::SeqCst);
    assert!(matches!(
        draft.choose(played, &stepper),
        Err(CombatDraftError::TurnReplay(_))
    ));
    assert_eq!(draft.current_frame_handle(), parent_frame);
    assert!(draft.exact_line().is_empty());
    assert_eq!(draft.option_continuation(branches[0]).unwrap(), Some(first));
    stepper.successor_salt.store(0, Ordering::SeqCst);

    let replay_calls = draft.option(branches[0]).unwrap().actions().len();
    let calls = stepper.calls.lock().unwrap().len();
    draft.choose(branches[0], &stepper).unwrap();
    assert_eq!(stepper.calls.lock().unwrap().len() - calls, replay_calls);
    assert_eq!(draft.option_handles(), first_handles);
    assert_eq!(draft.current_generation_counters(), first_counters);
    let next = draft
        .advance_options(&stepper, CombatPlanningQuantum::deterministic(100, 100))
        .unwrap();
    assert_eq!(Some(next.before), first_counters);
    assert!(next.after.generation_work > next.before.generation_work);
    let completed_first_handles = draft.option_handles();
    draft.choose(completed_first_handles[0], &stepper).unwrap();
    finish_current_turn(&mut draft, &stepper);
    let grandchild_handles = draft.option_handles();
    let grandchild_counters = draft.current_generation_counters();
    draft.undo().unwrap();
    draft.undo().unwrap();
    draft.choose(branches[1], &stepper).unwrap();
    assert_eq!(draft.option_handles(), second_handles);
    assert_eq!(draft.current_generation_counters(), second_counters);
    assert_eq!(
        draft.option(completed_first_handles[0]),
        Err(CombatDraftError::StaleOptionHandle)
    );
    draft.undo().unwrap();
    draft.choose(branches[0], &stepper).unwrap();
    assert_eq!(draft.option_handles(), completed_first_handles);
    draft.choose(completed_first_handles[0], &stepper).unwrap();
    assert_eq!(draft.option_handles(), grandchild_handles);
    assert_eq!(draft.current_generation_counters(), grandchild_counters);
}

#[test]
fn zero_quantum_returns_immediately_without_hidden_search_loop() {
    let stepper = TinyTurnStepper::plain();
    let mut draft = CombatDraftSession::new(root(), config());

    let report = draft
        .advance_options(&stepper, CombatPlanningQuantum::deterministic(0, 0))
        .unwrap();

    assert_eq!(
        report.status,
        TurnOptionGenerationStatus::Partial(GenerationInterruption::GenerationWorkBudget)
    );
    assert!(draft.option_handles().is_empty());
    assert_eq!(
        draft.current_option_coverage(),
        Some(CombatDraftOptionCoverage::Open {
            mechanics_gap_count: 0
        })
    );
    assert!(stepper.calls.lock().unwrap().is_empty());
}
