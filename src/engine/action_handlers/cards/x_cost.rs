use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;

pub fn handle_aggregate_energy(divide_amount: i32, state: &mut CombatState) {
    if divide_amount <= 0 {
        return;
    }
    let amount = state.zones.draw_pile.len() as i32 / divide_amount;
    if amount > 0 {
        state.turn.adjust_energy(amount);
    }
}

pub fn handle_tempest(
    upgraded: bool,
    free_to_play_once: bool,
    energy_on_use: i32,
    state: &mut CombatState,
) {
    let base_effect = if energy_on_use != -1 {
        energy_on_use
    } else {
        state.turn.energy as i32
    };
    let mut effect = crate::content::relics::hooks::on_calculate_x_cost(state, base_effect);
    if upgraded {
        effect += 1;
    }

    if effect > 0 {
        for _ in 0..effect {
            state.queue_action_back(Action::ChannelOrb(crate::runtime::combat::OrbId::Lightning));
        }
        if !free_to_play_once {
            state.turn.spend_energy(state.turn.energy as i32);
        }
    }
}

pub fn handle_multicast(
    upgraded: bool,
    free_to_play_once: bool,
    energy_on_use: i32,
    state: &mut CombatState,
) {
    if !state
        .entities
        .player
        .orbs
        .first()
        .is_some_and(|orb| orb.id != crate::runtime::combat::OrbId::Empty)
    {
        return;
    }

    let base_effect = if energy_on_use != -1 {
        energy_on_use
    } else {
        state.turn.energy as i32
    };
    let mut effect = crate::content::relics::hooks::on_calculate_x_cost(state, base_effect);
    if upgraded {
        effect += 1;
    }

    if effect > 0 {
        for _ in 0..effect - 1 {
            state.queue_action_back(Action::EvokeOrbWithoutRemoving);
        }
        state.queue_action_back(Action::EvokeOrb);
        if !free_to_play_once {
            state.turn.spend_energy(state.turn.energy as i32);
        }
    }
}

pub fn handle_reinforced_body(
    block_amount: i32,
    free_to_play_once: bool,
    energy_on_use: i32,
    state: &mut CombatState,
) {
    let base_effect = if energy_on_use != -1 {
        energy_on_use
    } else {
        state.turn.energy as i32
    };
    let effect = crate::content::relics::hooks::on_calculate_x_cost(state, base_effect);

    if effect > 0 {
        for _ in 0..effect {
            state.queue_action_back(Action::GainBlock {
                target: 0,
                amount: block_amount,
            });
        }
        if !free_to_play_once {
            state.turn.spend_energy(state.turn.energy as i32);
        }
    }
}
