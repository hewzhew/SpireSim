use super::core::{
    apply_damage_to_monster_via_pipeline, clear_post_combat_actions_if_ready,
    target_receives_java_unique_kill_reward,
};
use super::gold::handle_gain_gold;
use super::health::increase_player_max_hp_like_java;
use crate::content::powers::{store, PowerId};
use crate::runtime::action::{Action, DamageInfo, DamageType};
use crate::runtime::combat::CombatState;

pub fn handle_whirlwind(
    damages: smallvec::SmallVec<[i32; 5]>,
    damage_type: DamageType,
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
            state.queue_action_back(Action::DamageAllEnemies {
                source: 0,
                damages: damages.clone(),
                damage_type,
                is_modified: false,
            });
        }
        if !free_to_play_once {
            state.turn.spend_energy(state.turn.energy as i32);
        }
    }
}

pub fn handle_skewer(
    target: usize,
    damage_info: DamageInfo,
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
            let mut hit = damage_info.clone();
            hit.target = target;
            state.queue_action_back(Action::Damage(hit));
        }
        if !free_to_play_once {
            state.turn.spend_energy(state.turn.energy as i32);
        }
    }
}

pub fn handle_sunder(
    target: usize,
    mut damage_info: DamageInfo,
    energy_gain: i32,
    state: &mut CombatState,
) {
    damage_info.target = target;
    let final_damage = damage_info.output.max(0);
    let _ = apply_damage_to_monster_via_pipeline(state, &damage_info, final_damage);
    let killed_or_zero_hp = state
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target)
        .is_some_and(|monster| monster.is_dying || monster.current_hp <= 0);
    if killed_or_zero_hp && energy_gain > 0 {
        state.queue_action_back(Action::GainEnergy {
            amount: energy_gain,
        });
    }
    clear_post_combat_actions_if_ready(state);
}

pub fn handle_judgement(target: usize, cutoff: i32, state: &mut CombatState) {
    let should_kill = state
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target)
        .is_some_and(|monster| monster.current_hp <= cutoff);
    if should_kill {
        state.queue_action_front(Action::InstantKill { target });
    }
}

pub fn handle_instant_kill(target: usize, state: &mut CombatState) {
    if target == 0 {
        state.entities.player.current_hp = 0;
        super::super::try_revive(state);
        return;
    }
    if let Some(monster) = state.entities.monsters.iter_mut().find(|m| m.id == target) {
        monster.current_hp = 0;
    }
    super::super::check_and_trigger_monster_death(state, target);
    clear_post_combat_actions_if_ready(state);
}

pub fn handle_trigger_marks(card_id: crate::content::cards::CardId, state: &mut CombatState) {
    if card_id != crate::content::cards::CardId::PressurePoints {
        return;
    }
    let targets: Vec<(usize, i32)> = state
        .entities
        .monsters
        .iter()
        .filter_map(|monster| {
            let amount = crate::content::powers::store::power_amount(
                state,
                monster.id,
                crate::content::powers::PowerId::MarkPower,
            );
            (amount > 0).then_some((monster.id, amount))
        })
        .collect();
    for (target, amount) in targets {
        state.queue_action_back(Action::LoseHp {
            target,
            amount,
            triggers_rupture: false,
        });
    }
}

pub fn handle_dropkick(
    target: usize,
    damage_info: crate::runtime::action::DamageInfo,
    state: &mut CombatState,
) {
    let follow_up = crate::content::cards::mechanics::card_target_vulnerable_follow_up_v1(
        crate::content::cards::CardId::Dropkick,
    )
    .expect("Dropkick follow-up rule");
    let has_vulnerable = store::power_amount(state, target, PowerId::Vulnerable) > 0;
    if has_vulnerable {
        state.queue_action_front(Action::DrawCards(follow_up.draw_cards));
        state.queue_action_front(Action::GainEnergy {
            amount: follow_up.energy_gain,
        });
    }
    state.queue_action_front(Action::Damage(damage_info));
}

pub fn handle_ftl(
    target: usize,
    damage_info: crate::runtime::action::DamageInfo,
    card_play_count: i32,
    state: &mut CombatState,
) {
    state.queue_action_back(Action::Damage(crate::runtime::action::DamageInfo {
        target,
        ..damage_info
    }));
    if (state.turn.counters.cards_played_this_turn as i32).saturating_sub(1) < card_play_count {
        state.queue_action_front(Action::DrawCards(1));
    }
}

pub fn handle_fiend_fire(
    target: usize,
    damage_info: crate::runtime::action::DamageInfo,
    state: &mut CombatState,
) {
    let count = state.zones.hand.len();
    for _ in 0..count {
        let mut info = damage_info.clone();
        info.target = target;
        state.queue_action_front(Action::Damage(info));
    }
    for _ in 0..count {
        state.queue_action_front(Action::ExhaustRandomCard { amount: 1 });
    }
}

pub fn handle_feed(
    target: usize,
    damage_info: crate::runtime::action::DamageInfo,
    max_hp_amount: i32,
    state: &mut CombatState,
) {
    let mut info = damage_info;
    info.target = target;
    let _ = apply_damage_to_monster_via_pipeline(state, &info, info.output.max(0));
    if target_receives_java_unique_kill_reward(state, target) {
        increase_player_max_hp_like_java(max_hp_amount, state);
    }
    clear_post_combat_actions_if_ready(state);
}

pub fn handle_lesson_learned(
    target: usize,
    damage_info: crate::runtime::action::DamageInfo,
    state: &mut CombatState,
) {
    let mut info = damage_info;
    info.target = target;
    let _ = apply_damage_to_monster_via_pipeline(state, &info, info.output.max(0));
    if target_receives_java_unique_kill_reward(state, target) {
        let possible = state
            .meta
            .master_deck_snapshot
            .iter()
            .enumerate()
            .filter(|(_, card)| crate::content::cards::can_upgrade_card_once(card))
            .map(|(idx, _)| idx)
            .collect::<Vec<_>>();
        if !possible.is_empty() {
            let pick = state
                .rng
                .misc_rng
                .random_range(0, possible.len() as i32 - 1) as usize;
            let deck_idx = possible[pick];
            let card_lineage = state
                .meta
                .master_deck_snapshot
                .with_cards_mut(|master_deck| {
                    let card_lineage = master_deck[deck_idx].lineage;
                    master_deck[deck_idx].upgrades += 1;
                    card_lineage
                });
            state
                .meta
                .meta_changes
                .push(crate::runtime::combat::MetaChange::UpgradeMasterDeckCard { card_lineage });
        }
    }
    clear_post_combat_actions_if_ready(state);
}

pub fn handle_hand_of_greed(
    target: usize,
    damage_info: crate::runtime::action::DamageInfo,
    gold_amount: i32,
    state: &mut CombatState,
) {
    let mut info = damage_info;
    info.target = target;
    let _ = apply_damage_to_monster_via_pipeline(state, &info, info.output.max(0));
    if target_receives_java_unique_kill_reward(state, target) {
        handle_gain_gold(gold_amount, state);
    }
    clear_post_combat_actions_if_ready(state);
}

pub fn handle_ritual_dagger(
    target: usize,
    damage_info: crate::runtime::action::DamageInfo,
    misc_amount: i32,
    card_lineage: crate::runtime::combat::CardLineageId,
    state: &mut CombatState,
) {
    let mut info = damage_info;
    info.target = target;
    let _ = apply_damage_to_monster_via_pipeline(state, &info, info.output.max(0));
    if target_receives_java_unique_kill_reward(state, target) {
        crate::engine::action_handlers::cards::handle_modify_card_misc(
            card_lineage,
            misc_amount,
            state,
        );
    }
    clear_post_combat_actions_if_ready(state);
}
