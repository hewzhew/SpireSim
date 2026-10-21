use crate::content::relics::RelicId;
use crate::state::core::{EngineState, RunPendingChoiceReason, RunPendingChoiceState};
use crate::state::run::RunState;
use crate::state::selection::DomainEventSource;

pub fn on_equip(run_state: &mut RunState, return_state: EngineState) -> Option<EngineState> {
    if !run_state.master_deck.is_empty() {
        return Some(EngineState::RunPendingChoice(RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 1,
            max_choices: 1,
            reason: RunPendingChoiceReason::Duplicate,
            source: DomainEventSource::Relic(RelicId::DollysMirror),
            return_state: Box::new(return_state),
        }));
    }
    None
}
