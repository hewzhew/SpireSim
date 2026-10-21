use crate::state::core::EngineState;

use super::super::types::CombatEngineKey;
use super::pending_choice::pending_choice_key;

pub(super) fn engine_key(engine: &EngineState) -> CombatEngineKey {
    match engine {
        EngineState::DoubleBossComplete => CombatEngineKey::DoubleBossComplete,
        EngineState::Act3VictoryComplete => CombatEngineKey::Act3VictoryComplete,
        EngineState::TrueVictoryComplete => CombatEngineKey::TrueVictoryComplete,
        EngineState::CombatPlayerTurn => CombatEngineKey::CombatPlayerTurn,
        EngineState::CombatProcessing => CombatEngineKey::CombatProcessing,
        EngineState::PendingChoice(choice) => {
            CombatEngineKey::PendingChoice(pending_choice_key(choice))
        }
        EngineState::RewardScreen(_) => CombatEngineKey::RewardScreen(canonical_json(engine)),
        EngineState::RewardOverlay { .. } => CombatEngineKey::RewardOverlay(canonical_json(engine)),
        EngineState::TreasureRoom(_) => CombatEngineKey::TreasureRoom(canonical_json(engine)),
        EngineState::Campfire => CombatEngineKey::Campfire,
        EngineState::Shop(_) => CombatEngineKey::Shop(canonical_json(engine)),
        EngineState::MapNavigation => CombatEngineKey::MapNavigation,
        EngineState::MapOverlay { .. } => CombatEngineKey::MapOverlay(canonical_json(engine)),
        EngineState::EventRoom => CombatEngineKey::EventRoom,
        EngineState::CombatStart(_) => CombatEngineKey::CombatStart(canonical_json(engine)),
        EngineState::RunPendingChoice(_) => {
            CombatEngineKey::RunPendingChoice(canonical_json(engine))
        }
        EngineState::BossRelicSelect(_) => CombatEngineKey::BossRelicSelect(canonical_json(engine)),
        EngineState::GameOver(_) => CombatEngineKey::GameOver(canonical_json(engine)),
    }
}

fn canonical_json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("engine-state key should serialize deterministically")
}
