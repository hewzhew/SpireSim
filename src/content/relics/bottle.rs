use crate::content::relics::RelicId;
use crate::state::core::{
    run_pending_choice_allows_card_for_run, EngineState, RunPendingChoiceReason,
    RunPendingChoiceState,
};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub(crate) fn on_equip(
    run_state: &RunState,
    reason: RunPendingChoiceReason,
    relic_id: RelicId,
    return_state: EngineState,
) -> Option<EngineState> {
    let has_candidate = run_state
        .master_deck
        .iter()
        .any(|card| run_pending_choice_allows_card_for_run(&reason, card, run_state));
    if !has_candidate {
        return None;
    }

    Some(EngineState::RunPendingChoice(RunPendingChoiceState {
        selected: Vec::new(),
        min_choices: 1,
        max_choices: 1,
        reason,
        source: DomainEventSource::Relic(relic_id),
        return_state: Box::new(return_state),
    }))
}
