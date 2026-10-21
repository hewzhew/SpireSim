mod cards;
mod engine;
mod meta;
mod monster;
mod player;
mod potions;
mod powers;
mod queue;
mod rng;
mod runtime;
mod turn;

pub(crate) use self::cards::*;
pub(crate) use self::engine::*;
pub(crate) use self::meta::*;
pub(crate) use self::monster::*;
pub(crate) use self::player::*;
pub(crate) use self::potions::*;
pub(crate) use self::powers::*;
pub(crate) use self::queue::*;
pub(crate) use self::rng::*;
pub(crate) use self::runtime::*;
pub(crate) use self::turn::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub struct CombatExactStateKey {
    pub(crate) common: CombatRuntimeKey,
    pub(crate) player: CombatExactPlayerKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub struct CombatDominanceKey {
    pub(crate) common: CombatRuntimeKey,
    pub(crate) player: CombatDominancePlayerKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatRuntimeKey {
    pub(crate) engine: CombatEngineKey,
    pub(crate) turn: CombatTurnKey,
    pub(crate) meta: CombatMetaKey,
    pub(crate) zones: CombatZonesKey,
    pub(crate) monsters: Vec<CombatMonsterKey>,
    pub(crate) powers: Vec<CombatEntityPowersKey>,
    pub(crate) potions: Vec<CombatPotionSlotKey>,
    pub(crate) queue: Vec<CombatQueuedActionKey>,
    pub(crate) runtime: CombatRuntimeHintsKey,
    pub(crate) rng: CombatRngPoolKey,
}
