pub const MECHANICS: super::mechanics::RelicMechanics = super::mechanics::RelicMechanics {
    text_id: crate::content::relics::RelicId::Calipers,
};

pub fn on_calculate_block_retained(block: i32) -> i32 {
    (block - 15).max(0)
}

pub fn on_equip() {
    // Note: Engine-level mechanics handled directly via `state.entities.player.block = (state.entities.player.block - 15).max(0);` in `engine.rs`
    // within `Action::EndTurnTrigger`, zeroing out block gracefully without full wipe.
}
