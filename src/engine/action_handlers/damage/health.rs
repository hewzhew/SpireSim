use super::core::{
    apply_damage_to_monster_via_pipeline, clear_post_combat_actions_if_ready,
    damage_after_final_receive, queue_on_block_gained_hooks, queue_player_hp_loss_hooks,
    queue_red_skull_threshold_actions, resolve_player_damage, update_player_cards_on_damage,
};
use crate::content::powers::{store, PowerId};
use crate::runtime::action::{DamageInfo, DamageType, NO_SOURCE};
use crate::runtime::combat::CombatState;

const MAX_BLOCK: i32 = 999;

pub fn block_after_gain(current_block: i32, amount: i32) -> i32 {
    current_block.saturating_add(amount).min(MAX_BLOCK)
}

pub fn handle_lose_hp(target: usize, amount: i32, triggers_rupture: bool, state: &mut CombatState) {
    if target == 0 {
        // LoseHPAction calls AbstractPlayer.damage without DamageAction's
        // cancellation guard. Share creature prevention (including Buffer),
        // while retaining this action's post-combat cleanup and timing.
        let source = triggers_rupture.then_some(0);
        let info = DamageInfo {
            source: source.unwrap_or(NO_SOURCE),
            target,
            base: amount,
            output: amount,
            damage_type: DamageType::HpLoss,
            is_modified: false,
        };
        let final_amount = resolve_player_damage(&info, state).hp_loss;
        let previous_hp = state.entities.player.current_hp;
        state.entities.player.current_hp = (state.entities.player.current_hp - final_amount).max(0);
        if final_amount > 0 {
            update_player_cards_on_damage(state);
            state.turn.increment_times_damaged_this_combat();
            queue_red_skull_threshold_actions(state, previous_hp, state.entities.player.current_hp);
            queue_player_hp_loss_hooks(state, final_amount, source, DamageType::HpLoss);
        }
        if state.entities.player.current_hp <= 0 {
            super::super::try_revive(state);
        }
        clear_post_combat_actions_if_ready(state);
    } else {
        let final_amount =
            damage_after_final_receive(state, target, amount.max(0), DamageType::HpLoss);
        let info = DamageInfo {
            source: NO_SOURCE,
            target,
            base: final_amount,
            output: final_amount,
            damage_type: DamageType::HpLoss,
            is_modified: false,
        };
        let _ = apply_damage_to_monster_via_pipeline(state, &info, final_amount);
    }
}

pub fn handle_poison_lose_hp(source: usize, target: usize, amount: i32, state: &mut CombatState) {
    if target == 0 {
        if state.entities.player.current_hp > 0 {
            let final_amount = crate::content::relics::hooks::on_lose_hp_last(state, amount.max(0));
            let previous_hp = state.entities.player.current_hp;
            state.entities.player.current_hp =
                (state.entities.player.current_hp - final_amount).max(0);
            if final_amount > 0 {
                update_player_cards_on_damage(state);
                state.turn.increment_times_damaged_this_combat();
                queue_red_skull_threshold_actions(
                    state,
                    previous_hp,
                    state.entities.player.current_hp,
                );
                queue_player_hp_loss_hooks(
                    state,
                    final_amount,
                    (source != NO_SOURCE).then_some(source),
                    DamageType::HpLoss,
                );
            }
            if state.entities.player.current_hp <= 0 {
                super::super::try_revive(state);
            }
        }
    } else if state
        .entities
        .monsters
        .iter()
        .any(|m| m.id == target && m.current_hp > 0)
    {
        let final_amount =
            damage_after_final_receive(state, target, amount.max(0), DamageType::HpLoss);
        let info = DamageInfo {
            source,
            target,
            base: final_amount,
            output: final_amount,
            damage_type: DamageType::HpLoss,
            is_modified: false,
        };
        let _ = apply_damage_to_monster_via_pipeline(state, &info, final_amount);
    }

    let should_remove_poison = store::with_power_mut(state, target, PowerId::Poison, |power| {
        power.amount -= 1;
        power.amount == 0
    })
    .unwrap_or(false);
    if should_remove_poison {
        store::remove_power_type(state, target, PowerId::Poison);
    }

    clear_post_combat_actions_if_ready(state);
}

pub fn handle_set_current_hp(target: usize, hp: i32, state: &mut CombatState) {
    let clamped_hp = hp.max(0);
    if target == 0 {
        state.entities.player.current_hp = clamped_hp.min(state.entities.player.max_hp);
        if state.entities.player.current_hp <= 0 {
            super::super::try_revive(state);
        }
        return;
    }

    if let Some(monster) = state.entities.monsters.iter_mut().find(|m| m.id == target) {
        monster.current_hp = clamped_hp.min(monster.max_hp);
    }
    super::super::check_and_trigger_monster_death(state, target);
    clear_post_combat_actions_if_ready(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::combat::{Power, PowerPayload};

    #[test]
    fn direct_hp_loss_obeys_buffer_before_tungsten_and_rupture() {
        use crate::content::relics::{RelicId, RelicState};
        use crate::engine::action_handlers::execute_action;
        for amount in [0, 1, 3] {
            for buffer in [false, true] {
                for tungsten in [false, true] {
                    for self_source in [false, true] {
                        let mut state = crate::test_support::blank_test_combat();
                        state
                            .entities
                            .monsters
                            .push(crate::test_support::test_monster(
                                crate::content::monsters::EnemyId::JawWorm,
                            ));
                        state.entities.player.block = 5;
                        let power = |power_type, amount| Power {
                            power_type,
                            amount,
                            instance_id: None,
                            extra_data: 0,
                            payload: PowerPayload::None,
                            just_applied: false,
                        };
                        let mut powers = vec![power(PowerId::Rupture, 2)];
                        if buffer {
                            powers.push(power(PowerId::Buffer, 1));
                        }
                        state.entities.power_db.insert(0, powers);
                        if tungsten {
                            state
                                .entities
                                .player
                                .add_relic(RelicState::new(RelicId::TungstenRod));
                        }
                        handle_lose_hp(0, amount, self_source, &mut state);
                        while let Some(action) = state.pop_next_action() {
                            execute_action(action, &mut state);
                        }
                        let lost = if buffer {
                            0
                        } else {
                            (amount - i32::from(tungsten)).max(0)
                        };
                        assert_eq!(
                            state.entities.player.current_hp,
                            80 - lost,
                            "{amount} {buffer} {tungsten} {self_source}"
                        );
                        assert_eq!(state.entities.player.block, 5);
                        assert_eq!(
                            state.get_power(0, PowerId::Buffer),
                            i32::from(buffer && amount == 0)
                        );
                        assert_eq!(
                            state.get_power(0, PowerId::Strength),
                            if self_source && lost > 0 { 2 } else { 0 }
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn player_direct_hp_loss_is_capped_by_intangible_like_java() {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.current_hp = 20;
        state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::IntangiblePlayer,
                instance_id: None,
                amount: 3,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        handle_lose_hp(0, 6, true, &mut state);

        assert_eq!(state.entities.player.current_hp, 19);
    }

    #[test]
    fn monster_hp_loss_and_poison_respect_intangible_without_spending_block() {
        for poison in [false, true] {
            for amount in [0, 1, 9] {
                let mut state = crate::test_support::blank_test_combat();
                let mut monster =
                    crate::test_support::test_monster(crate::content::monsters::EnemyId::Nemesis);
                monster.id = 1;
                monster.current_hp = 100;
                monster.max_hp = 100;
                monster.block = 20;
                state.entities.monsters.push(monster);
                state.entities.power_db.insert(
                    1,
                    vec![Power {
                        power_type: PowerId::Intangible,
                        instance_id: None,
                        amount: 1,
                        extra_data: 0,
                        payload: PowerPayload::None,
                        just_applied: false,
                    }],
                );
                if poison {
                    handle_poison_lose_hp(NO_SOURCE, 1, amount, &mut state);
                } else {
                    handle_lose_hp(1, amount, false, &mut state);
                }
                assert_eq!(
                    state.entities.monsters[0].current_hp,
                    100 - amount.min(1),
                    "poison={poison} amount={amount}"
                );
                assert_eq!(state.entities.monsters[0].block, 20);
            }
        }
    }

    #[test]
    fn gained_block_stops_at_the_java_creature_cap() {
        let mut state = crate::test_support::blank_test_combat();
        state.entities.player.current_hp = 20;
        state.entities.player.block = 990;

        handle_gain_block(0, 20, &mut state);
        assert_eq!(state.entities.player.block, 999);

        handle_double_block(0, &mut state);
        assert_eq!(state.entities.player.block, 999);
    }
}

pub fn handle_gain_block(target: usize, amount: i32, state: &mut CombatState) {
    if target == 0 {
        if state.entities.player.current_hp > 0 {
            state.entities.player.block = block_after_gain(state.entities.player.block, amount);
            queue_on_block_gained_hooks(state, 0, amount);
        }
    } else if let Some(m) = state.entities.monsters.iter_mut().find(|m| m.id == target) {
        if m.current_hp > 0 {
            m.block = block_after_gain(m.block, amount);
        }
    }
}

pub fn handle_double_block(target: usize, state: &mut CombatState) {
    let current_block = if target == 0 {
        state.entities.player.block
    } else {
        state
            .entities
            .monsters
            .iter()
            .find(|m| m.id == target)
            .map_or(0, |m| m.block)
    };
    if current_block > 0 {
        handle_gain_block(target, current_block, state);
    }
}

pub fn handle_gain_block_random_monster(source: usize, amount: i32, state: &mut CombatState) {
    let alive: Vec<usize> = state
        .entities
        .monsters
        .iter()
        .filter(|m| {
            m.id != source
                && !matches!(
                    crate::content::monsters::resolve_monster_turn_plan(state, m).summary_spec(),
                    crate::runtime::monster_move::MonsterMoveSpec::Escape
                )
                && !m.is_dying
        })
        .map(|m| m.id)
        .collect();
    let target_id = if !alive.is_empty() {
        let idx = state.rng.ai_rng.random(alive.len() as i32 - 1) as usize;
        alive[idx]
    } else {
        source
    };
    if let Some(m) = state
        .entities
        .monsters
        .iter_mut()
        .find(|m| m.id == target_id)
    {
        m.block = block_after_gain(m.block, amount);
    }
}

pub fn handle_lose_block(target: usize, amount: i32, state: &mut CombatState) {
    if target == 0 {
        state.entities.player.block = (state.entities.player.block - amount).max(0);
    } else if let Some(m) = state.entities.monsters.iter_mut().find(|m| m.id == target) {
        m.block = (m.block - amount).max(0);
    }
}

pub fn handle_remove_all_block(target: usize, state: &mut CombatState) {
    if target == 0 {
        state.entities.player.block = 0;
    } else if let Some(m) = state.entities.monsters.iter_mut().find(|m| m.id == target) {
        m.block = 0;
    }
}

pub fn handle_heal(target: usize, mut amount: i32, state: &mut CombatState) {
    if amount < 0 {
        let pct = (-amount) as f32 / 100.0;
        if target == 0 {
            amount = std::cmp::max(1, (state.entities.player.max_hp as f32 * pct) as i32);
        } else if let Some(m) = state.entities.monsters.iter().find(|m| m.id == target) {
            amount = std::cmp::max(1, (m.max_hp as f32 * pct) as i32);
        }
    }
    if target == 0 {
        amount = crate::content::relics::hooks::on_calculate_heal(state, amount);
        let previous_hp = state.entities.player.current_hp;
        state.entities.player.current_hp =
            (state.entities.player.current_hp + amount).min(state.entities.player.max_hp);
        queue_red_skull_threshold_actions(state, previous_hp, state.entities.player.current_hp);
    } else if let Some(m) = state.entities.monsters.iter_mut().find(|m| m.id == target) {
        if m.is_dying {
            return;
        }
        m.current_hp = (m.current_hp + amount).min(m.max_hp);
    }
}

pub fn increase_player_max_hp_like_java(amount: i32, state: &mut CombatState) {
    state.entities.player.max_hp += amount;
    handle_heal(0, amount, state);
}
