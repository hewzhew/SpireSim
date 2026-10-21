use crate::state::core::EngineState;
use crate::state::run::RunState;

pub fn on_equip(run_state: &mut RunState) -> Option<EngineState> {
    run_state.change_gold_with_source(
        300,
        crate::state::selection::DomainEventSource::Relic(super::RelicId::OldCoin),
    );
    None
}
