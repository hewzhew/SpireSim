use serde::{Deserialize, Serialize};

use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::runtime::combat::CardLineageId;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum RunApplyStatus {
    Running,
    Victory,
    Defeat,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::eval::run_control) struct TransitionAction {
    pub(in crate::eval::run_control) label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(in crate::eval::run_control) struct RunVisibleSnapshot {
    pub(in crate::eval::run_control) title: String,
    pub(in crate::eval::run_control) current_hp: i32,
    pub(in crate::eval::run_control) max_hp: i32,
    pub(in crate::eval::run_control) gold: i32,
    pub(in crate::eval::run_control) act: u8,
    pub(in crate::eval::run_control) floor: i32,
    pub(in crate::eval::run_control) keys: [bool; 3],
    pub(in crate::eval::run_control) relics: Vec<RelicSnapshot>,
    pub(in crate::eval::run_control) potions: Vec<Option<PotionSnapshot>>,
    pub(in crate::eval::run_control) deck: Vec<CardSnapshot>,
    pub(in crate::eval::run_control) combat: Option<CombatSnapshot>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(in crate::eval::run_control) struct RelicSnapshot {
    pub(in crate::eval::run_control) id: RelicId,
    pub(in crate::eval::run_control) counter: i32,
    pub(in crate::eval::run_control) used_up: bool,
    pub(in crate::eval::run_control) amount: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(in crate::eval::run_control) struct PotionSnapshot {
    pub(in crate::eval::run_control) id: PotionId,
    pub(in crate::eval::run_control) uuid: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CardSnapshot {
    pub id: CardId,
    pub lineage: CardLineageId,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(in crate::eval::run_control) struct CombatSnapshot {
    pub(in crate::eval::run_control) player_hp: i32,
    pub(in crate::eval::run_control) player_max_hp: i32,
    pub(in crate::eval::run_control) player_block: i32,
    pub(in crate::eval::run_control) energy: i32,
    pub(in crate::eval::run_control) monsters: Vec<MonsterSnapshot>,
    pub(in crate::eval::run_control) hand_count: usize,
    pub(in crate::eval::run_control) draw_count: usize,
    pub(in crate::eval::run_control) discard_count: usize,
    pub(in crate::eval::run_control) exhaust_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MonsterSnapshot {
    pub id: usize,
    pub label: String,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ActionResult {
    pub chosen_label: String,
    pub status: RunApplyStatus,
    pub changes: Vec<ActionResultChange>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionResultChange {
    HpChanged {
        before_current: i32,
        before_max: i32,
        after_current: i32,
        after_max: i32,
    },
    GoldChanged {
        before: i32,
        after: i32,
    },
    RelicGained {
        relic: RelicId,
    },
    RelicLost {
        relic: RelicId,
    },
    RelicChanged {
        relic: RelicId,
        counter: Option<ValueChange<i32>>,
        amount: Option<ValueChange<i32>>,
        used_up: Option<ValueChange<bool>>,
    },
    PotionGained {
        potion: PotionId,
        slot: usize,
    },
    PotionLost {
        potion: PotionId,
        slot: usize,
    },
    PotionChanged {
        slot: usize,
        before: PotionId,
        after: PotionId,
    },
    CardRemoved {
        card: CardSnapshot,
    },
    CardAdded {
        card: CardSnapshot,
    },
    CardTransformed {
        before: CardSnapshot,
        after: CardSnapshot,
    },
    CardUpgraded {
        before: CardSnapshot,
        after: CardSnapshot,
    },
    KeyChanged {
        key: RunKey,
        obtained: bool,
    },
    CombatStarted {
        player: CombatPlayerResult,
        monsters: Vec<MonsterSnapshot>,
    },
    CombatEnded,
    CombatCardDrawn {
        card: CardSnapshot,
    },
    CombatPlayerChanged {
        before: CombatPlayerResult,
        after: CombatPlayerResult,
    },
    CombatMonsterChanged {
        before: MonsterSnapshot,
        after: MonsterSnapshot,
    },
    PileCountsChanged {
        before: PileCounts,
        after: PileCounts,
    },
    LocationChanged {
        before_act: u8,
        before_floor: i32,
        after_act: u8,
        after_floor: i32,
    },
    AdvancedTo {
        title: String,
    },
    RunEnded {
        result: RunEndResult,
    },
    EngineStopped,
    NoVisibleStateChanges,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ValueChange<T> {
    pub before: T,
    pub after: T,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum RunKey {
    Ruby,
    Sapphire,
    Emerald,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum RunEndResult {
    Victory,
    Defeat,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CombatPlayerResult {
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub energy: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PileCounts {
    pub hand: usize,
    pub draw: usize,
    pub discard: usize,
    pub exhaust: usize,
}
