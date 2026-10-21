use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

fn target_is_dead_or_escaped(target: usize, state: &CombatState) -> bool {
    if target == 0 {
        return state.entities.player.current_hp <= 0;
    }
    state
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target)
        .is_some_and(|monster| monster.is_dead_or_escaped())
}

pub fn handle_remove_power(target: usize, power_id: PowerId, state: &mut CombatState) {
    // Original RemoveSpecificPowerAction is a no-op once its target is dead
    // or escaped. In particular, a later hit can kill a Louse while Curl Up's
    // queued removal is still waiting.
    if target_is_dead_or_escaped(target, state) {
        return;
    }
    let had_power = store::powers_for(state, target)
        .is_some_and(|powers| powers.iter().any(|p| p.power_type == power_id));
    if !had_power {
        return;
    }

    let on_remove_actions =
        crate::content::powers::resolve_power_on_remove(power_id, state, target);
    for action in on_remove_actions {
        state.queue_action_back(action);
    }

    store::remove_power_type(state, target, power_id);
    super::refresh_player_focus(target, power_id, state);
    if power_id == PowerId::Surrounded && target == 0 {
        crate::content::powers::core::surrounded::sync_back_attack_markers(state);
    }
}

pub fn handle_remove_power_instance(
    target: usize,
    power_id: PowerId,
    instance_id: u32,
    state: &mut CombatState,
) {
    if target_is_dead_or_escaped(target, state) {
        return;
    }
    let power_snapshot = store::powers_for(state, target).and_then(|powers| {
        powers
            .iter()
            .find(|p| p.power_type == power_id && p.instance_id == Some(instance_id))
            .cloned()
    });
    let Some(power_snapshot) = power_snapshot else {
        return;
    };

    let on_remove_actions =
        crate::content::powers::resolve_power_on_remove(power_snapshot.power_type, state, target);
    for action in on_remove_actions {
        state.queue_action_back(action);
    }

    store::remove_power_instance(state, target, power_id, instance_id);
    super::refresh_player_focus(target, power_id, state);
}

pub fn handle_reduce_power(target: usize, power_id: PowerId, amount: i32, state: &mut CombatState) {
    if amount <= 0 {
        return;
    }

    let Some(remaining) = store::with_power_mut(state, target, power_id, |power| {
        power.amount -= amount;
        power.amount
    }) else {
        return;
    };

    if !crate::content::powers::should_keep_power_instance(power_id, remaining) {
        handle_remove_power(target, power_id, state);
    }

    if target == 0 {
        state.recompute_turn_start_draw_modifier();
    }
    super::refresh_player_focus(target, power_id, state);
}

pub fn handle_reduce_power_instance(
    target: usize,
    power_id: PowerId,
    instance_id: u32,
    amount: i32,
    state: &mut CombatState,
) {
    if amount <= 0 {
        return;
    }

    let Some(remaining) =
        store::with_power_instance_mut(state, target, power_id, instance_id, |power| {
            power.amount -= amount;
            power.amount
        })
    else {
        return;
    };

    if !crate::content::powers::should_keep_power_instance(power_id, remaining) {
        handle_remove_power_instance(target, power_id, instance_id, state);
    }

    if target == 0 {
        state.recompute_turn_start_draw_modifier();
    }
    super::refresh_player_focus(target, power_id, state);
}

pub fn handle_remove_all_debuffs(target: usize, state: &mut CombatState) {
    let debuffs = store::powers_for(state, target)
        .map(|powers| {
            powers
                .iter()
                .filter(|p| crate::content::powers::is_debuff(p.power_type, p.amount))
                .map(|p| p.power_type)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for power_id in debuffs {
        state.queue_action_front(Action::RemovePower { target, power_id });
    }
}

pub fn handle_update_power_extra_data(
    target: usize,
    power_id: PowerId,
    value: i32,
    state: &mut CombatState,
) {
    let _ = store::with_power_mut(state, target, power_id, |power| {
        power.extra_data = value;
    });
}

pub fn handle_update_power_extra_data_instance(
    target: usize,
    power_id: PowerId,
    instance_id: u32,
    value: i32,
    state: &mut CombatState,
) {
    let _ = store::with_power_instance_mut(state, target, power_id, instance_id, |power| {
        power.extra_data = value;
    });
}
