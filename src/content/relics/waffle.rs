use crate::state::core::EngineState;
use crate::state::run::RunState;

pub fn on_equip(run_state: &mut RunState) -> Option<EngineState> {
    let source = crate::state::selection::DomainEventSource::Relic(super::RelicId::Waffle);
    run_state.gain_max_hp_with_source(7, 7, source);
    run_state.heal_with_source(run_state.max_hp, source);
    None
}
