pub fn on_equip() {
    // Note: Engine-level mechanics handled directly in `engine.rs` during `Action::PlayCard` reduction phase.
    // X cost calculation maps `-1` to `state.turn.energy` and then augments `+ 2` before `evaluate_card` overrides.
}

pub fn on_calculate_x_cost(amount: i32) -> i32 {
    amount + 2
}
