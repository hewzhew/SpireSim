use serde::{Deserialize, Serialize};

use super::{CombatStartRequest, PendingChoice, RunPendingChoiceState};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum EngineState {
    CombatPlayerTurn,
    CombatProcessing,
    RewardScreen(crate::state::rewards::RewardState),
    /// Java-style reward overlay opened from another screen. Closing or
    /// exhausting it returns to the stashed screen; opening a card choice stays
    /// inside the overlay reward surface.
    RewardOverlay {
        reward_state: crate::state::rewards::RewardState,
        return_state: Box<EngineState>,
    },
    TreasureRoom(crate::state::rewards::TreasureChestState),
    Campfire,
    Shop(crate::state::shop::ShopState),
    MapNavigation,
    /// Java-style map overlay opened from another screen. Closing it returns to
    /// the stashed screen; selecting a map node commits travel and drops the
    /// overlay return path.
    MapOverlay {
        return_state: Box<EngineState>,
    },
    EventRoom,
    /// Request to construct a concrete active combat from RunState. This is a
    /// transient run boundary, not a capture/search combat decision boundary.
    CombatStart(CombatStartRequest),
    PendingChoice(PendingChoice),
    RunPendingChoice(RunPendingChoiceState),
    BossRelicSelect(crate::state::rewards::BossRelicChoiceState),
    GameOver(RunResult),
    /// The first A20 Act 3 boss is defeated; the original Proceed button
    /// has not yet entered the second boss room.
    DoubleBossComplete,
    /// The last Act 3 boss is defeated with all keys. Proceed enters the
    /// SpireHeart event; opening the door later constructs the final act.
    Act3VictoryComplete,
    /// The Heart is defeated; Proceed enters the original TrueVictoryRoom.
    TrueVictoryComplete,
}

impl EngineState {
    /// Payload-free protocol label for diagnostics and versioned summaries.
    /// Exact payload identity remains in the typed state or its dedicated key.
    pub const fn kind_v1(&self) -> &'static str {
        match self {
            Self::CombatPlayerTurn => "combat_player_turn",
            Self::CombatProcessing => "combat_processing",
            Self::RewardScreen(_) => "reward_screen",
            Self::RewardOverlay { .. } => "reward_overlay",
            Self::TreasureRoom(_) => "treasure_room",
            Self::Campfire => "campfire",
            Self::Shop(_) => "shop",
            Self::MapNavigation => "map_navigation",
            Self::MapOverlay { .. } => "map_overlay",
            Self::EventRoom => "event_room",
            Self::CombatStart(_) => "combat_start",
            Self::PendingChoice(_) => "combat_pending_choice",
            Self::RunPendingChoice(_) => "run_pending_choice",
            Self::BossRelicSelect(_) => "boss_relic_select",
            Self::DoubleBossComplete => "double_boss_complete",
            Self::Act3VictoryComplete => "act3_victory_complete",
            Self::TrueVictoryComplete => "true_victory_complete",
            Self::GameOver(_) => "game_over",
        }
    }

    pub fn map_overlay(return_state: EngineState) -> Self {
        Self::MapOverlay {
            return_state: Box::new(return_state),
        }
    }

    pub fn reward_overlay(
        reward_state: crate::state::rewards::RewardState,
        return_state: EngineState,
    ) -> Self {
        Self::RewardOverlay {
            reward_state,
            return_state: Box::new(return_state),
        }
    }

    pub fn is_map_surface(&self) -> bool {
        matches!(self, Self::MapNavigation | Self::MapOverlay { .. })
    }
}

/// Where to go after event combat finishes.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum PostCombatReturn {
    /// Return to the event dialog (e.g., Colosseum between fights).
    EventRoom,
    /// Standard: combat done -> rewards -> map navigation.
    MapNavigation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum RunResult {
    Victory,
    Defeat,
}

impl RunResult {
    pub const fn kind_v1(&self) -> &'static str {
        match self {
            Self::Victory => "victory",
            Self::Defeat => "defeat",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum TopLevelState {
    InCombat,
    OnMap,
    AtCampfire,
    InShop,
    OnRewardScreen,
    OnEvent,
}
