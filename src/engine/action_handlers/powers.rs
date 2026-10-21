// action_handlers/powers.rs - Power action facade
//
// Power actions are grouped by engine use: application, mutation/removal,
// resource-side effects, and card/monster-specific special actions.

mod apply;
mod mutation;
mod resources;
mod specials;

fn refresh_player_focus(
    target: usize,
    power_id: crate::content::powers::PowerId,
    state: &mut crate::runtime::combat::CombatState,
) {
    if target == 0 && power_id == crate::content::powers::PowerId::Focus {
        crate::content::orbs::hooks::refresh_orb_focus_values(state);
    }
}

pub use apply::{handle_apply_power, handle_apply_power_detailed, handle_apply_power_with_payload};
pub use mutation::{
    handle_reduce_power, handle_reduce_power_instance, handle_remove_all_debuffs,
    handle_remove_power, handle_remove_power_instance, handle_update_power_extra_data,
    handle_update_power_extra_data_instance,
};
pub use resources::{
    apply_player_turn_energy_recharge_hooks, handle_double_energy, handle_gain_energy,
    handle_gain_max_hp, handle_increase_monster_max_hp_percent, handle_lose_max_hp,
};
pub use specials::{
    handle_apply_stasis, handle_apply_weak_if_target_attacking, handle_bouncing_flask,
    handle_collect, handle_doppelganger, handle_malaise, handle_spot_weakness,
    handle_trigger_time_warp_end_turn, spot_weakness_strength_request,
};
