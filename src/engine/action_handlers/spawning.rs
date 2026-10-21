// action_handlers/spawning.rs - Monster lifecycle facade
//
// This module keeps the old public action-handler surface while the implementation
// is grouped around how the engine/search layer uses it: spawn construction,
// lifecycle mutations, move selection, runtime patches, and relic state updates.

mod initial_runtime;
mod lifecycle;
mod move_roll;
mod relics;
mod runtime_patch;
mod spawn;

pub use lifecycle::{
    handle_add_combat_reward, handle_escape, handle_revive_monster, handle_suicide,
};
pub use move_roll::{handle_roll_monster_move, handle_set_monster_move};
pub use relics::{
    handle_update_relic_amount, handle_update_relic_counter, handle_update_relic_used_up,
};
pub use runtime_patch::handle_update_monster_runtime;
pub use spawn::{
    handle_spawn_collector_torch, handle_spawn_gremlin_leader_minion, handle_spawn_monster,
    handle_spawn_monster_smart, handle_spawn_reptomancer_dagger,
};
