mod cards;
mod engine;
mod meta;
mod monster;
mod pending_choice;
mod player;
mod potions;
mod powers;
mod queue;
mod rng;
mod runtime_hints;
mod turn;
mod zones;

pub(super) use rng::rng_pool_key;
pub(super) use turn::phase_key;

use std::time::Instant;

use crate::runtime::combat::CombatState;
use crate::state::core::EngineState;

use super::types::{
    CombatDominanceKey, CombatDominancePlayerKey, CombatExactStateKey, CombatRuntimeKey,
};
use super::CombatExactKeyBuildTimingV1;

/// Exact in-combat runtime key used by Combat Search V2 transposition pruning.
/// This is stricter than `stable_outcome_key`: player hp/block, card
/// instances, queue, monster runtime, powers, potions, and RNG remain in.
pub(crate) fn combat_exact_runtime_key(
    engine: &EngineState,
    combat: &CombatState,
) -> CombatExactStateKey {
    CombatExactStateKey {
        common: combat_runtime_key(engine, combat),
        player: player::player_exact_key(combat),
    }
}

pub(crate) fn combat_exact_runtime_key_profiled_v1(
    engine: &EngineState,
    combat: &CombatState,
) -> (CombatExactStateKey, CombatExactKeyBuildTimingV1) {
    let (engine_key, engine_elapsed_ns) = timed(|| engine::engine_key(engine));
    let (turn_key, turn_elapsed_ns) = timed(|| turn::turn_key(combat));
    let (meta_key, meta_elapsed_ns) = timed(|| meta::meta_key(combat));
    let (zones_key, zones_elapsed_ns) = timed(|| zones::zones_key(combat));
    let (monsters_key, monsters_elapsed_ns) = timed(|| monster::monsters_key(combat));
    let (powers_key, powers_elapsed_ns) = timed(|| powers::powers_key(combat));
    let (potions_key, potions_elapsed_ns) = timed(|| potions::potions_key(combat));
    let (queue_key, queue_elapsed_ns) = timed(|| queue::queue_key(combat));
    let (runtime_key, runtime_elapsed_ns) = timed(|| runtime_hints::runtime_key(combat));
    let (rng_key, rng_elapsed_ns) = timed(|| rng::rng_pool_key(&combat.rng.pool));
    let (player_key, player_elapsed_ns) = timed(|| player::player_exact_key(combat));

    (
        CombatExactStateKey {
            common: CombatRuntimeKey {
                engine: engine_key,
                turn: turn_key,
                meta: meta_key,
                zones: zones_key,
                monsters: monsters_key,
                powers: powers_key,
                potions: potions_key,
                queue: queue_key,
                runtime: runtime_key,
                rng: rng_key,
            },
            player: player_key,
        },
        CombatExactKeyBuildTimingV1 {
            engine_elapsed_ns,
            turn_elapsed_ns,
            meta_elapsed_ns,
            zones_elapsed_ns,
            monsters_elapsed_ns,
            powers_elapsed_ns,
            potions_elapsed_ns,
            queue_elapsed_ns,
            runtime_elapsed_ns,
            rng_elapsed_ns,
            player_elapsed_ns,
        },
    )
}

fn timed<T>(build: impl FnOnce() -> T) -> (T, u64) {
    let started = Instant::now();
    let value = build();
    let elapsed_ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    (value, elapsed_ns)
}

/// In-combat bucket used by Combat Search V2 resource dominance pruning. This
/// is not an exact transposition key: current HP/block are intentionally left
/// out because they are compared through `ResourceVector`, but card instances,
/// queue, monster runtime, powers, potions, and RNG remain in.
pub(crate) fn combat_dominance_bucket_key(
    engine: &EngineState,
    combat: &CombatState,
) -> CombatDominanceKey {
    CombatDominanceKey {
        common: combat_runtime_key(engine, combat),
        player: CombatDominancePlayerKey {
            future_relevant: player::player_future_key(combat),
        },
    }
}

fn combat_runtime_key(engine: &EngineState, combat: &CombatState) -> CombatRuntimeKey {
    CombatRuntimeKey {
        engine: engine::engine_key(engine),
        turn: turn::turn_key(combat),
        meta: meta::meta_key(combat),
        zones: zones::zones_key(combat),
        monsters: monster::monsters_key(combat),
        powers: powers::powers_key(combat),
        potions: potions::potions_key(combat),
        queue: queue::queue_key(combat),
        runtime: runtime_hints::runtime_key(combat),
        rng: rng::rng_pool_key(&combat.rng.pool),
    }
}
