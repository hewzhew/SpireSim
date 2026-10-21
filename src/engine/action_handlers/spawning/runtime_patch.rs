mod beyond;
mod city;
mod ending;
mod exordium;

use crate::runtime::action::MonsterRuntimePatch;
use crate::runtime::combat::CombatState;

pub fn handle_update_monster_runtime(
    monster_id: usize,
    patch: MonsterRuntimePatch,
    state: &mut CombatState,
) {
    let patch = match exordium::try_handle_patch(monster_id, patch, state) {
        Ok(()) => return,
        Err(patch) => patch,
    };
    let patch = match city::try_handle_patch(monster_id, patch, state) {
        Ok(()) => return,
        Err(patch) => patch,
    };
    let patch = match beyond::try_handle_patch(monster_id, patch, state) {
        Ok(()) => return,
        Err(patch) => patch,
    };
    let patch = match ending::try_handle_patch(monster_id, patch, state) {
        Ok(()) => return,
        Err(patch) => patch,
    };

    #[cfg(debug_assertions)]
    eprintln!(
        "[runtime_patch] Unhandled monster runtime patch: {:?}",
        patch
    );
    let _ = patch;
}
