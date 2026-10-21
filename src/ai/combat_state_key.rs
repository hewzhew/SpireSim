//! State-key boundaries for combat search.
//!
//! Exact keys are for duplicate runtime states; dominance keys intentionally
//! remove player hp/block and compare them through a resource vector; stable
//! outcome keys are only for stable frontiers and may ignore runtime noise.
//! Do not use one key family in place of another.

mod combat;
mod identity;
mod monster;
mod pending_choice;
mod postcombat;
mod stable;
#[cfg(test)]
mod tests;
mod types;

use crate::runtime::combat::CombatState;
use crate::state::EngineState;

use combat::{combat_dominance_bucket_key, combat_exact_runtime_key};
use identity::combat_exact_identity_v2;
use stable::build_stable_outcome_key;
pub use types::{CombatDominanceKey, CombatExactStateKey, StableOutcomeKey};

/// Sampled wall-clock decomposition of one exact combat-key build.
///
/// Search semantics and budgets never read these values. The profiled builder
/// exists so performance work can target the expensive semantic section
/// instead of guessing from collection sizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct CombatExactKeyBuildTimingV1 {
    pub engine_elapsed_ns: u64,
    pub turn_elapsed_ns: u64,
    pub meta_elapsed_ns: u64,
    pub zones_elapsed_ns: u64,
    pub monsters_elapsed_ns: u64,
    pub powers_elapsed_ns: u64,
    pub potions_elapsed_ns: u64,
    pub queue_elapsed_ns: u64,
    pub runtime_elapsed_ns: u64,
    pub rng_elapsed_ns: u64,
    pub player_elapsed_ns: u64,
}

/// Stable diagnostic hashes for the semantic sections of a dominance key.
/// Search diagnostics use this view without depending on the key's private
/// representation.
pub struct CombatDominanceDiagnosticPartsV1 {
    pub engine_key: String,
    pub turn_key: String,
    pub meta_key: String,
    pub zones_key: String,
    pub monsters_key: String,
    pub powers_key: String,
    pub potions_key: String,
    pub queue_key: String,
    pub runtime_key: String,
    pub rng_key: String,
    pub player_key: String,
}

/// Exact in-combat key for Combat Search V2 transposition. It keeps player
/// hp/block and runtime details that affect future combat transitions.
pub fn combat_exact_state_key(engine: &EngineState, combat: &CombatState) -> CombatExactStateKey {
    combat_exact_runtime_key(engine, combat)
}

/// Builds the same exact key while timing each semantic section.
///
/// Callers should sample this function; its clocks are intentionally absent
/// from the ordinary production builder.
pub fn combat_exact_state_key_profiled_v1(
    engine: &EngineState,
    combat: &CombatState,
) -> (CombatExactStateKey, CombatExactKeyBuildTimingV1) {
    combat::combat_exact_runtime_key_profiled_v1(engine, combat)
}

/// Durable V2 identity for an exact combat state.
///
/// V2 deliberately removed the historical empty runtime-card-queue slot from
/// the typed key. Persisted V1 hashes are therefore not interchangeable with
/// this identity and must be regenerated from their source position.
pub fn combat_exact_state_hash_v2(engine: &EngineState, combat: &CombatState) -> String {
    combat_exact_state_key_hash_v2(&combat_exact_state_key(engine, combat))
}

/// Stable diagnostic hash for an already-materialized exact combat key.
///
/// Search hot paths frequently need both the typed key for transposition and
/// its durable string identity for replay evidence. Reusing the same key
/// avoids rebuilding the full combat key twice without changing hash bytes.
pub fn combat_exact_state_key_hash_v2(key: &CombatExactStateKey) -> String {
    combat_exact_state_key_identity_v2(key).1
}

/// Returns the binary digest and its durable lowercase-hex representation for
/// one already-materialized exact key.
///
/// The binary digest lets exact transposition tables choose a bucket without
/// hashing the large typed key a second time. Callers must still compare the
/// typed key inside equal-digest buckets; the digest alone is not equality.
pub fn combat_exact_state_key_identity_v2(key: &CombatExactStateKey) -> ([u8; 32], String) {
    let digest = combat_exact_identity_v2(key);
    (digest, hex_lower(&digest))
}

/// In-combat bucket for Combat Search V2 resource dominance. It keeps runtime
/// details that affect future combat transitions, while leaving current
/// hp/block to the searched resource vector.
pub fn combat_dominance_key(engine: &EngineState, combat: &CombatState) -> CombatDominanceKey {
    combat_dominance_bucket_key(engine, combat)
}

pub fn combat_dominance_diagnostic_parts_v1(
    key: &CombatDominanceKey,
) -> CombatDominanceDiagnosticPartsV1 {
    CombatDominanceDiagnosticPartsV1 {
        engine_key: diagnostic_hash(&key.common.engine),
        turn_key: diagnostic_hash(&key.common.turn),
        meta_key: diagnostic_hash(&key.common.meta),
        zones_key: diagnostic_hash(&key.common.zones),
        monsters_key: diagnostic_hash(&key.common.monsters),
        powers_key: diagnostic_hash(&key.common.powers),
        potions_key: diagnostic_hash(&key.common.potions),
        queue_key: diagnostic_hash(&key.common.queue),
        runtime_key: diagnostic_hash(&key.common.runtime),
        rng_key: diagnostic_hash(&key.common.rng),
        player_key: diagnostic_hash(&key.player),
    }
}

/// Stable frontier key for comparing outcomes after the engine reaches a
/// player decision boundary. This intentionally abstracts display/runtime noise
/// that should not affect future decisions from that boundary.
#[cfg_attr(not(test), allow(dead_code))]
pub fn stable_outcome_key(engine: &EngineState, combat: &CombatState) -> StableOutcomeKey {
    debug_assert_ne!(
        stable_frontier_scope(engine, combat),
        StableFrontierScope::Unstable,
        "stable_outcome_key should only be requested for stable frontiers"
    );
    diagnostic_outcome_key(engine, combat)
}

/// Stable dominance bucket only exists at stable frontiers. Unstable engine
/// processing states must not be merged under this abstraction.
pub fn stable_dominance_bucket_key(
    engine: &EngineState,
    combat: &CombatState,
) -> Option<StableOutcomeKey> {
    match stable_frontier_scope(engine, combat) {
        StableFrontierScope::Unstable => None,
        _ => Some(diagnostic_outcome_key(engine, combat)),
    }
}

fn diagnostic_outcome_key(engine: &EngineState, combat: &CombatState) -> StableOutcomeKey {
    build_stable_outcome_key(engine, combat)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
enum StableFrontierScope {
    CombatReady,
    PendingChoice,
    Unstable,
    PostCombat,
    GameOver,
}

fn stable_frontier_scope(engine: &EngineState, _combat: &CombatState) -> StableFrontierScope {
    match engine {
        EngineState::CombatPlayerTurn => StableFrontierScope::CombatReady,
        EngineState::PendingChoice(_) => StableFrontierScope::PendingChoice,
        EngineState::CombatProcessing | EngineState::CombatStart(_) => {
            StableFrontierScope::Unstable
        }
        EngineState::RewardScreen(_)
        | EngineState::RewardOverlay { .. }
        | EngineState::TreasureRoom(_)
        | EngineState::Campfire
        | EngineState::Shop(_)
        | EngineState::MapNavigation
        | EngineState::MapOverlay { .. }
        | EngineState::EventRoom
        | EngineState::RunPendingChoice(_)
        | EngineState::BossRelicSelect(_)
        | EngineState::DoubleBossComplete
        | EngineState::Act3VictoryComplete
        | EngineState::TrueVictoryComplete => StableFrontierScope::PostCombat,
        EngineState::GameOver(_) => StableFrontierScope::GameOver,
    }
}

fn diagnostic_hash<T: std::fmt::Debug>(value: &T) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in format!("{value:?}").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
