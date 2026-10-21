use crate::state::core::EngineState;
use crate::state::run::RunState;

pub fn on_equip(run_state: &mut RunState) -> Option<EngineState> {
    run_state.potions.push(None);
    run_state.potions.push(None);
    None
}
