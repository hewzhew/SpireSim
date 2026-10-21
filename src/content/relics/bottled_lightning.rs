use crate::content::relics::RelicId;
use crate::state::core::{EngineState, RunPendingChoiceReason};
use crate::state::run::RunState;

pub struct BottledLightning;

impl BottledLightning {
    // Bottled Lightning allows the player to select a Skill card to become Innate.
    // In the combat engine, Innate cards are already resolved during initialization,
    // so this relic holds no active combat loop hooks.
}

pub fn on_equip(run_state: &RunState, return_state: EngineState) -> Option<EngineState> {
    super::bottle::on_equip(
        run_state,
        RunPendingChoiceReason::BottleLightning,
        RelicId::BottledLightning,
        return_state,
    )
}
