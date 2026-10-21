use crate::state::core::EngineState;
use crate::state::run::RunState;

pub fn on_equip(run_state: &mut RunState) -> Option<EngineState> {
    run_state.gain_max_hp_with_source(
        10,
        10,
        crate::state::selection::DomainEventSource::Relic(super::RelicId::Pear),
    );
    None
}
