use super::*;
use crate::state::run::RunState;

fn click(state: &mut EngineState, run: &mut RunState, input: ClientInput) {
    let EngineState::RunPendingChoice(pending) = state.clone() else {
        panic!("expected pending choice")
    };
    tick_run_pending_choice_v1(state, run, &pending, Some(input)).unwrap();
}

#[test]
fn astrolabe_ordered_clicks_defer_all_mutation_and_match_atomic_submission() {
    let mut run = RunState::new(2026090709, 20, true, "Ironclad");
    let mut state =
        crate::content::relics::astrolabe::on_equip(&mut run, EngineState::MapNavigation).unwrap();
    let mut atomic_run = run.clone();
    let mut atomic_state = state.clone();
    let eligible = match &state {
        EngineState::RunPendingChoice(p) => p.selection_request(&run).targets,
        _ => unreachable!(),
    };
    let order = [
        eligible[2].card_lineage(),
        eligible[0].card_lineage(),
        eligible[1].card_lineage(),
    ];
    let before = run.clone();
    for (step, lineage) in order.iter().take(2).enumerate() {
        let index = run
            .master_deck
            .iter()
            .position(|c| c.lineage == *lineage)
            .unwrap();
        click(&mut state, &mut run, ClientInput::SelectCard(index));
        assert_eq!(
            run, before,
            "partial selection must not mutate deck, RNG, or events"
        );
        let EngineState::RunPendingChoice(pending) = &state else {
            panic!("selection settled early")
        };
        assert_eq!(pending.selected, order[..=step]);
        assert!(!pending
            .selection_request(&run)
            .targets
            .contains(&SelectionTargetRef::CardLineage(*lineage)));
        let selected_state = state.clone();
        click(&mut state, &mut run, ClientInput::SelectCard(index));
        assert_eq!(state, selected_state, "duplicate click must be rejected");
        click(&mut state, &mut run, ClientInput::SelectCard(usize::MAX));
        assert_eq!(state, selected_state, "invalid index must be rejected");
    }
    // Partial selection is checkpoint state, including order.
    state = serde_json::from_value(serde_json::to_value(&state).unwrap()).unwrap();
    let last = run
        .master_deck
        .iter()
        .position(|c| c.lineage == order[2])
        .unwrap();
    click(&mut state, &mut run, ClientInput::SelectCard(last));
    click(
        &mut atomic_state,
        &mut atomic_run,
        ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Deck,
            order,
        )),
    );
    assert_eq!(state, atomic_state);
    assert_eq!(run, atomic_run);
    let witnessed = run
        .emitted_events
        .iter()
        .filter_map(|e| match e {
            DomainEvent::CardTransformed { before, .. } => Some(before.lineage),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(witnessed, order);
}
