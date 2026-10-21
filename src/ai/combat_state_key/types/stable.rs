use super::super::StableFrontierScope;
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub struct StableOutcomeKey {
    scope: StableFrontierScope,
    engine: StableEngineKey,
    payload: StableOutcomePayload,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum StableEngineKey {
    CombatReady,
    CombatProcessing,
    PendingChoice(StablePendingChoiceKey),
    Reward(StableRewardKey),
    RewardOverlay {
        reward: StableRewardKey,
        return_state: StableRunPendingReturnKey,
    },
    TreasureRoom(StableTreasureChestKey),
    Campfire,
    Shop(StableShopKey),
    MapNavigation,
    MapOverlay(StableRunPendingReturnKey),
    EventRoom,
    CombatStart(String),
    RunPendingChoice(StableRunPendingChoiceKey),
    BossRelic(StableBossRelicKey),
    GameOver(&'static str),
    DoubleBossComplete,
    Act3VictoryComplete,
    TrueVictoryComplete,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum StableOutcomePayload {
    Combat {
        meta: StableMetaKey,
        turn: StableTurnKey,
        player: StableCombatPlayerKey,
        zones: StableZonesKey,
        monsters: Vec<String>,
        powers: Vec<String>,
        rng: CombatRngPoolKey,
    },
    PostCombat {
        player: StablePostcombatPlayerKey,
        meta: StableMetaKey,
        runtime: StablePostcombatRuntimeKey,
        rng: CombatRngPoolKey,
    },
    GameOver,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct StableTurnKey {
    pub turn_count: u32,
    pub current_phase: CombatPhaseKey,
    pub energy: u8,
    pub turn_start_draw_modifier: i32,
    pub cards_played_this_turn: u8,
    pub attacks_played_this_turn: u8,
    pub times_damaged_this_combat: u8,
    pub victory_triggered: bool,
    pub discovery_cost_for_turn: Option<u8>,
    pub early_end_turn_pending: bool,
    pub player_escaping: bool,
    pub escape_pending_reward: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct StableCombatPlayerKey {
    pub max_hp: i32,
    pub facing_left: bool,
    pub orbs: String,
    pub max_orbs: u8,
    pub stance: String,
    pub relics: String,
    pub energy_master: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct StableZonesKey {
    pub draw: Vec<String>,
    pub hand: Vec<String>,
    pub discard: Vec<String>,
    pub exhaust: Vec<String>,
    pub limbo: Vec<String>,
}

impl StableOutcomeKey {
    pub(in crate::ai::combat_state_key) fn new(
        scope: StableFrontierScope,
        engine: StableEngineKey,
        payload: StableOutcomePayload,
    ) -> Self {
        Self {
            scope,
            engine,
            payload,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        match &self.payload {
            StableOutcomePayload::Combat {
                meta,
                turn,
                player,
                zones,
                monsters,
                powers,
                rng,
            } => format!(
                "scope={:?};engine={};meta={};turn={};player={};zones={};monsters=[{}];powers=[{}];rng={}",
                self.scope,
                self.engine.diagnostic_string(),
                meta.diagnostic_string(),
                turn.diagnostic_string(),
                player.diagnostic_string(),
                zones.diagnostic_string(),
                monsters.join("|"),
                powers.join("|"),
                serde_json::to_string(rng).expect("stable RNG key should serialize"),
            ),
            StableOutcomePayload::PostCombat {
                player,
                meta,
                runtime,
                rng,
            } => format!(
                "scope={:?};engine={};player={};meta={};runtime={};rng={}",
                self.scope,
                self.engine.diagnostic_string(),
                player.diagnostic_string(),
                meta.diagnostic_string(),
                runtime.diagnostic_string(),
                serde_json::to_string(rng).expect("stable RNG key should serialize"),
            ),
            StableOutcomePayload::GameOver => format!(
                "scope={:?};engine={}",
                self.scope,
                self.engine.diagnostic_string(),
            ),
        }
    }
}

impl StableEngineKey {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        match self {
            StableEngineKey::DoubleBossComplete => "double_boss_complete".to_owned(),
            StableEngineKey::Act3VictoryComplete => "act3_victory_complete".to_owned(),
            StableEngineKey::TrueVictoryComplete => "true_victory_complete".to_owned(),
            StableEngineKey::CombatReady => "player_turn_ready".to_string(),
            StableEngineKey::CombatProcessing => "combat_processing".to_string(),
            StableEngineKey::PendingChoice(value) => {
                format!("pending_choice:{}", value.diagnostic_string())
            }
            StableEngineKey::Reward(value) => format!("reward:{}", value.diagnostic_string()),
            StableEngineKey::RewardOverlay {
                reward,
                return_state,
            } => format!(
                "reward_overlay:{}:return{}",
                reward.diagnostic_string(),
                return_state.diagnostic_string()
            ),
            StableEngineKey::TreasureRoom(value) => {
                format!("treasure:{}", value.diagnostic_string())
            }
            StableEngineKey::Campfire => "campfire".to_string(),
            StableEngineKey::Shop(value) => format!("shop:{}", value.diagnostic_string()),
            StableEngineKey::MapNavigation => "map_navigation".to_string(),
            StableEngineKey::MapOverlay(value) => {
                format!("map_overlay:return{}", value.diagnostic_string())
            }
            StableEngineKey::EventRoom => "event_room".to_string(),
            StableEngineKey::CombatStart(value) => format!("combat_start:{value}"),
            StableEngineKey::RunPendingChoice(value) => {
                format!("run_choice:{}", value.diagnostic_string())
            }
            StableEngineKey::BossRelic(value) => {
                format!("boss_relic:{}", value.diagnostic_string())
            }
            StableEngineKey::GameOver(value) => format!("game_over:{value}"),
        }
    }
}

impl StableTurnKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            concat!(
                "count:{}:phase:{:?}:energy:{}:draw_mod:{}:",
                "cards:{}:attacks:{}:damaged:{}:victory:{}:discover:{:?}:",
                "early_end:{}:escaping:{}:escape_reward:{}"
            ),
            self.turn_count,
            self.current_phase,
            self.energy,
            self.turn_start_draw_modifier,
            self.cards_played_this_turn,
            self.attacks_played_this_turn,
            self.times_damaged_this_combat,
            self.victory_triggered,
            self.discovery_cost_for_turn,
            self.early_end_turn_pending,
            self.player_escaping,
            self.escape_pending_reward,
        )
    }
}

impl StableCombatPlayerKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            "max_hp:{}:orbs:{}:max_orbs:{}:stance:{}:relics:{}:energy_master:{}",
            self.max_hp, self.orbs, self.max_orbs, self.stance, self.relics, self.energy_master,
        )
    }
}

impl StableZonesKey {
    pub(in crate::ai::combat_state_key) fn diagnostic_string(&self) -> String {
        format!(
            "draw:[{}];hand:[{}];disc:[{}];exhaust:[{}];limbo:[{}]",
            self.draw.join("|"),
            self.hand.join("|"),
            self.discard.join("|"),
            self.exhaust.join("|"),
            self.limbo.join("|"),
        )
    }
}
