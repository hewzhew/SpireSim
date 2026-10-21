use crate::engine::action_handlers::{powers, stances};

use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

pub(super) fn try_execute(action: Action, state: &mut CombatState) -> Result<(), Action> {
    match action {
        // === Power domain ===
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => powers::handle_apply_power(source, target, power_id, amount, state),
        Action::ApplyPowerDetailed {
            source,
            target,
            power_id,
            amount,
            instance_id,
            extra_data,
        } => powers::handle_apply_power_detailed(
            source,
            target,
            power_id,
            amount,
            instance_id,
            extra_data,
            state,
        ),
        Action::ApplyPowerWithPayload {
            source,
            target,
            power_id,
            amount,
            instance_id,
            extra_data,
            payload,
        } => powers::handle_apply_power_with_payload(
            source,
            target,
            power_id,
            amount,
            instance_id,
            extra_data,
            payload,
            state,
        ),
        Action::ReducePower {
            target,
            power_id,
            amount,
        } => powers::handle_reduce_power(target, power_id, amount, state),
        Action::ReducePowerInstance {
            target,
            power_id,
            instance_id,
            amount,
        } => powers::handle_reduce_power_instance(target, power_id, instance_id, amount, state),
        Action::BouncingFlask {
            target,
            amount,
            num_times,
        } => powers::handle_bouncing_flask(target, amount, num_times, state),
        Action::RemovePower { target, power_id } => {
            powers::handle_remove_power(target, power_id, state)
        }
        Action::RemovePowerInstance {
            target,
            power_id,
            instance_id,
        } => powers::handle_remove_power_instance(target, power_id, instance_id, state),
        Action::RemoveAllDebuffs { target } => powers::handle_remove_all_debuffs(target, state),
        Action::ApplyStasis { target_id } => powers::handle_apply_stasis(target_id, state),
        Action::UpdatePowerExtraData {
            target,
            power_id,
            value,
        } => powers::handle_update_power_extra_data(target, power_id, value, state),
        Action::UpdatePowerExtraDataInstance {
            target,
            power_id,
            instance_id,
            value,
        } => powers::handle_update_power_extra_data_instance(
            target,
            power_id,
            instance_id,
            value,
            state,
        ),
        Action::TriggerTimeWarpEndTurn { owner } => {
            powers::handle_trigger_time_warp_end_turn(owner, state)
        }
        Action::GainEnergy { amount } => powers::handle_gain_energy(amount, state),
        Action::DoubleEnergy => powers::handle_double_energy(state),
        Action::GainEnergyIfDiscardedThisTurn { amount } => {
            if state.turn.counters.cards_discarded_this_turn > 0 {
                powers::handle_gain_energy(amount, state);
            }
        }
        Action::FollowUp => stances::handle_follow_up(state),
        Action::Sanctity { draw_amount } => stances::handle_sanctity(draw_amount, state),
        Action::CrushJoints { target, amount } => {
            stances::handle_crush_joints(target, amount, state)
        }
        Action::SashWhip { target, amount } => stances::handle_sash_whip(target, amount, state),
        Action::GainMaxHp { amount } => powers::handle_gain_max_hp(amount, state),
        Action::IncreaseMonsterMaxHpPercent { target, percent } => {
            powers::handle_increase_monster_max_hp_percent(target, percent, state)
        }
        Action::LoseMaxHp { target, amount } => powers::handle_lose_max_hp(target, amount, state),

        other => return Err(other),
    }
    Ok(())
}
