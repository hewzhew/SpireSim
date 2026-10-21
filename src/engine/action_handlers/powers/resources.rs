use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
pub fn apply_player_turn_energy_recharge_hooks(state: &mut CombatState) {
    // Java PlayerTurnEffect first recharges base energy, then calls
    // relic/power onEnergyRecharge hooks before ordinary start-of-turn hooks.
    for power in store::powers_snapshot_for(state, 0) {
        match power.power_type {
            PowerId::Energized | PowerId::EnergizedBlue => {
                state.turn.adjust_energy(power.amount);
                state.queue_action_back(Action::RemovePower {
                    target: 0,
                    power_id: power.power_type,
                });
            }
            PowerId::DevaForm => {
                let energy_gain = power.extra_data.max(1);
                state.turn.adjust_energy(energy_gain);
                let _ = store::with_power_mut(state, 0, PowerId::DevaForm, |deva| {
                    deva.extra_data += deva.amount;
                });
            }
            PowerId::CollectPower => {
                state.queue_action_back(
                    crate::content::cards::make_constructed_temp_card_in_hand_action(
                        crate::content::cards::CardId::Miracle,
                        1,
                        true,
                        state,
                    ),
                );
                if power.amount <= 1 {
                    state.queue_action_back(Action::RemovePower {
                        target: 0,
                        power_id: PowerId::CollectPower,
                    });
                } else {
                    state.queue_action_back(Action::ReducePower {
                        target: 0,
                        power_id: PowerId::CollectPower,
                        amount: 1,
                    });
                }
            }
            _ => {}
        }
    }
}

pub fn handle_gain_energy(amount: i32, state: &mut CombatState) {
    state.turn.adjust_energy(amount);
}

pub fn handle_double_energy(state: &mut CombatState) {
    let current_energy = state.turn.energy as i32;
    if current_energy > 0 {
        state.turn.adjust_energy(current_energy);
    }
}

pub fn handle_gain_max_hp(amount: i32, state: &mut CombatState) {
    crate::engine::action_handlers::damage::increase_player_max_hp_like_java(amount, state);
}

pub fn handle_increase_monster_max_hp_percent(target: usize, percent: u8, state: &mut CombatState) {
    let Some(monster) = state
        .entities
        .monsters
        .iter_mut()
        .find(|monster| monster.id == target)
    else {
        return;
    };
    let amount = (monster.max_hp as f32 * (f32::from(percent) / 100.0)).round() as i32;
    monster.max_hp += amount;
    monster.current_hp = (monster.current_hp + amount).min(monster.max_hp);
}

pub fn handle_lose_max_hp(target: usize, amount: i32, state: &mut CombatState) {
    if target == 0 {
        state.entities.player.max_hp = (state.entities.player.max_hp - amount).max(1);
        state.entities.player.current_hp = state
            .entities
            .player
            .current_hp
            .min(state.entities.player.max_hp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::action_handlers::{execute_action, powers::handle_apply_power};

    #[test]
    fn energized_colors_stack_independently_then_recharge_and_remove_together() {
        let mut state = crate::test_support::blank_test_combat();
        state.turn.energy = 3;
        for (power, amount) in [
            (PowerId::Energized, 2),
            (PowerId::EnergizedBlue, 1),
            (PowerId::EnergizedBlue, 1),
        ] {
            handle_apply_power(0, 0, power, amount, &mut state);
        }
        assert_eq!(store::power_amount(&state, 0, PowerId::Energized), 2);
        assert_eq!(store::power_amount(&state, 0, PowerId::EnergizedBlue), 2);
        assert_eq!(state.turn.energy, 3);

        apply_player_turn_energy_recharge_hooks(&mut state);
        assert_eq!(state.turn.energy, 7);
        while let Some(action) = state.pop_next_action() {
            execute_action(action, &mut state);
        }
        for power in [PowerId::Energized, PowerId::EnergizedBlue] {
            assert!(!store::has_power(&state, 0, power));
        }
        state.turn.energy = 3;
        apply_player_turn_energy_recharge_hooks(&mut state);
        assert_eq!(state.turn.energy, 3);
    }

    #[test]
    fn energized_constructor_and_stack_caps_follow_each_original_power() {
        let mut state = crate::test_support::blank_test_combat();
        for power in [PowerId::Energized, PowerId::EnergizedBlue] {
            handle_apply_power(0, 0, power, 1_000, &mut state);
            assert_eq!(store::power_amount(&state, 0, power), 1_000);
            handle_apply_power(0, 0, power, 1, &mut state);
            assert_eq!(store::power_amount(&state, 0, power), 999);
        }
    }
}
