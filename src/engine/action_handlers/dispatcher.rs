mod card_route;
mod damage_route;
mod orb_route;
mod power_route;
mod spawn_route;
mod unhandled;

use crate::engine::core::{
    CombatEnginePhaseProfiler, CombatEngineProfilePhaseV1, NoopCombatEnginePhaseProfiler,
};
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

/// Executes one queued action by delegating to the relevant domain handler.
pub fn execute_action(action: Action, state: &mut CombatState) {
    let mut profiler = NoopCombatEnginePhaseProfiler;
    execute_action_with_profiler(action, state, &mut profiler);
}

pub(crate) fn execute_action_with_profiler<P: CombatEnginePhaseProfiler>(
    action: Action,
    state: &mut CombatState,
    profiler: &mut P,
) {
    let marker = profiler.begin(CombatEngineProfilePhaseV1::MonsterActionDamageRoute);
    let result = damage_route::try_execute(action, state);
    profiler.end(CombatEngineProfilePhaseV1::MonsterActionDamageRoute, marker);
    let action = match result {
        Ok(()) => return,
        Err(action) => action,
    };
    let marker = profiler.begin(CombatEngineProfilePhaseV1::MonsterActionPowerRoute);
    let result = power_route::try_execute(action, state);
    profiler.end(CombatEngineProfilePhaseV1::MonsterActionPowerRoute, marker);
    let action = match result {
        Ok(()) => return,
        Err(action) => action,
    };
    let marker = profiler.begin(CombatEngineProfilePhaseV1::MonsterActionCardRoute);
    let result = card_route::try_execute(action, state);
    profiler.end(CombatEngineProfilePhaseV1::MonsterActionCardRoute, marker);
    let action = match result {
        Ok(()) => return,
        Err(action) => action,
    };
    let marker = profiler.begin(CombatEngineProfilePhaseV1::MonsterActionSpawnRoute);
    let result = spawn_route::try_execute(action, state);
    profiler.end(CombatEngineProfilePhaseV1::MonsterActionSpawnRoute, marker);
    let action = match result {
        Ok(()) => return,
        Err(action) => action,
    };
    let marker = profiler.begin(CombatEngineProfilePhaseV1::MonsterActionOrbRoute);
    let result = orb_route::try_execute(action, state);
    profiler.end(CombatEngineProfilePhaseV1::MonsterActionOrbRoute, marker);
    let action = match result {
        Ok(()) => return,
        Err(action) => action,
    };
    let marker = profiler.begin(CombatEngineProfilePhaseV1::MonsterActionUnhandledRoute);
    unhandled::execute(action, state);
    profiler.end(
        CombatEngineProfilePhaseV1::MonsterActionUnhandledRoute,
        marker,
    );
}
