pub const MECHANICS: &str = "At the start of your turn, lose 15 Block instead of all Block, keeping any positive remainder. Enemy attacks and other Block expenditures still consume it. If Barricade is active, it preserves all remaining Block and Calipers does not remove 15.";

pub fn on_calculate_block_retained(block: i32) -> i32 {
    (block - 15).max(0)
}

pub fn on_equip() {
    // Note: Engine-level mechanics handled directly via `state.entities.player.block = (state.entities.player.block - 15).max(0);` in `engine.rs`
    // within `Action::EndTurnTrigger`, zeroing out block gracefully without full wipe.
}
