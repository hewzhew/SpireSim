//! Ectoplasm has no combat hook.
//!
//! Java semantics:
//! - `onEquip`/`onUnequip` adjust `energyMaster`.
//! - `canSpawn` allows this boss relic only while `AbstractDungeon.actNum <= 1`.
//! - gold gain is blocked by the gold-gain path, not by a combat action.
//!
//! Rust handles those in `energy_master_delta`, `RunState::relic_can_spawn_now`,
//! and `RunState::change_gold_with_source`.
