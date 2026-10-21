use crate::content::cards::CardId;
use crate::content::relics::RelicState;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::monster_move::{AttackSpec, DamageKind, MonsterMoveSpec, MonsterTurnPlan};
use crate::state::selection::{DomainEvent, EngineDiagnostic};
use crate::EntityId;
use rustc_hash::FxBuildHasher;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::{Deref, DerefMut};

pub use crate::content::powers::PowerId;
/// Small, trusted in-memory entity indexes on combat state.
///
/// Entity ids are simulator-owned integers, so randomized collision defense
/// only adds hashing cost while cloning and querying exact search states.
pub type CombatEntityMap<V> = HashMap<EntityId, V, FxBuildHasher>;

mod card;
mod card_birth;
mod card_lineage;
mod card_pile;
mod combat_methods;
mod draw_observation;
mod entities;
mod master_deck_snapshot;
mod monster_runtime;
mod oracle_shuffle_observation;
mod orbs;
mod power;
mod state;

pub use card::*;
pub use card_birth::*;
pub use card_lineage::*;
pub use card_pile::*;
pub use draw_observation::*;
pub use entities::*;
pub use master_deck_snapshot::*;
pub use monster_runtime::*;
pub use oracle_shuffle_observation::*;
pub use orbs::*;
pub use power::*;
pub use state::*;

#[cfg(test)]
mod tests;
