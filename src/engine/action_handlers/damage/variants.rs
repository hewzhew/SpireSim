use super::core::{
    apply_damage_to_monster_via_pipeline, calculate_damage_action_output,
    clear_post_combat_actions_if_ready, damage_after_final_receive, handle_damage,
    should_cancel_java_damage_action,
};
use crate::content::powers::{store, PowerId};
use crate::engine::targeting::pick_random_target;
use crate::runtime::action::{Action, DamageType, NO_SOURCE};
use crate::runtime::combat::CombatState;
use crate::state::TargetValidation;

fn pummel_source_is_dying(state: &CombatState, source_id: usize) -> bool {
    if source_id == 0 {
        state.entities.player.current_hp <= 0
    } else if source_id == NO_SOURCE {
        false
    } else {
        state
            .entities
            .monsters
            .iter()
            .find(|m| m.id == source_id)
            .is_some_and(|m| m.is_dying)
    }
}

fn target_current_hp_is_positive(state: &CombatState, target_id: usize) -> bool {
    if target_id == 0 {
        state.entities.player.current_hp > 0
    } else {
        state
            .entities
            .monsters
            .iter()
            .find(|m| m.id == target_id)
            .is_some_and(|m| m.current_hp > 0)
    }
}

pub fn handle_pummel_damage(info: crate::runtime::action::DamageInfo, state: &mut CombatState) {
    if !target_current_hp_is_positive(state, info.target) {
        return;
    }
    if info.damage_type != DamageType::Thorns && pummel_source_is_dying(state, info.source) {
        return;
    }

    if info.target == 0 {
        handle_damage(info, state);
    } else {
        let final_damage = info.output.max(0);
        let _ = apply_damage_to_monster_via_pipeline(state, &info, final_damage);
        clear_post_combat_actions_if_ready(state);
    }
}

pub fn handle_bane_damage(info: crate::runtime::action::DamageInfo, state: &mut CombatState) {
    if !store::has_power(state, info.target, PowerId::Poison) {
        return;
    }
    if !target_current_hp_is_positive(state, info.target) {
        return;
    }
    if info.damage_type != DamageType::Thorns && pummel_source_is_dying(state, info.source) {
        return;
    }

    if info.target == 0 {
        handle_damage(info, state);
    } else {
        let final_damage = info.output.max(0);
        let _ = apply_damage_to_monster_via_pipeline(state, &info, final_damage);
        clear_post_combat_actions_if_ready(state);
    }
}

pub fn handle_wallop_damage(info: crate::runtime::action::DamageInfo, state: &mut CombatState) {
    if should_cancel_java_damage_action(state, &info) {
        return;
    }
    if info.target == 0 {
        handle_damage(info, state);
        return;
    }

    let (calculated_output, damage_already_includes_final_receive) =
        calculate_damage_action_output(state, &info);
    let mut final_damage = calculated_output;

    if !damage_already_includes_final_receive {
        final_damage =
            damage_after_final_receive(state, info.target, final_damage, info.damage_type);
    }

    let outcome = apply_damage_to_monster_via_pipeline(state, &info, final_damage);
    if outcome.hp_lost > 0 {
        state.queue_action_front(Action::GainBlock {
            target: info.source,
            amount: outcome.hp_lost,
        });
    }
    clear_post_combat_actions_if_ready(state);
}

pub fn handle_damage_per_attack_played(
    info: crate::runtime::action::DamageInfo,
    state: &mut CombatState,
) {
    if !target_current_hp_is_positive(state, info.target) {
        return;
    }

    let attack_count_before_finisher = state
        .turn
        .counters
        .attacks_played_this_turn
        .saturating_sub(1) as usize;
    for _ in 0..attack_count_before_finisher {
        state.queue_action_front(Action::Damage(info.clone()));
    }
}

pub fn handle_heel_hook(info: crate::runtime::action::DamageInfo, state: &mut CombatState) {
    if store::has_power(state, info.target, PowerId::Weak) {
        state.queue_action_front(Action::DrawCards(1));
        state.queue_action_front(Action::GainEnergy { amount: 1 });
    }
    state.queue_action_front(Action::Damage(info));
}

pub fn handle_flechettes(info: crate::runtime::action::DamageInfo, state: &mut CombatState) {
    let skill_count = state
        .zones
        .hand
        .iter()
        .filter(|card| {
            crate::content::cards::get_card_definition(card.id).card_type
                == crate::content::cards::CardType::Skill
        })
        .count();
    for _ in 0..skill_count {
        state.queue_action_front(Action::Damage(info.clone()));
    }
}

fn damage_type(
    kind: crate::runtime::monster_move::DamageKind,
) -> crate::runtime::action::DamageType {
    match kind {
        crate::runtime::monster_move::DamageKind::Normal => {
            crate::runtime::action::DamageType::Normal
        }
        crate::runtime::monster_move::DamageKind::Thorns => {
            crate::runtime::action::DamageType::Thorns
        }
        crate::runtime::monster_move::DamageKind::HpLoss => {
            crate::runtime::action::DamageType::HpLoss
        }
        crate::runtime::monster_move::DamageKind::Unknown => panic!("monster attack kind unknown"),
    }
}

pub fn handle_monster_attack(
    source: usize,
    target: usize,
    base_damage: i32,
    damage_kind: crate::runtime::monster_move::DamageKind,
    state: &mut CombatState,
) {
    handle_damage(
        crate::runtime::action::DamageInfo {
            source,
            target,
            base: base_damage,
            output: base_damage,
            damage_type: damage_type(damage_kind),
            is_modified: !matches!(
                damage_kind,
                crate::runtime::monster_move::DamageKind::Normal
            ),
        },
        state,
    );
}

pub fn handle_damage_all_enemies(
    source: usize,
    damages: smallvec::SmallVec<[i32; 5]>,
    damage_type: DamageType,
    is_modified: bool,
    state: &mut CombatState,
) {
    let mut individual_damages: smallvec::SmallVec<[Action; 5]> = smallvec::SmallVec::new();
    for (i, &dmg) in damages.iter().enumerate() {
        if i >= state.entities.monsters.len() {
            break;
        }
        let m = &state.entities.monsters[i];
        if m.is_dead_or_escaped() {
            continue;
        }
        individual_damages.push(Action::Damage(crate::runtime::action::DamageInfo {
            source,
            target: m.id,
            base: dmg,
            output: dmg,
            damage_type,
            is_modified,
        }));
    }
    for action in individual_damages.into_iter().rev() {
        state.queue_action_front(action);
    }
}

fn orb_damage_amount_for_target(state: &CombatState, target: usize, base_damage: i32) -> i32 {
    if store::power_amount(state, target, PowerId::LockOn) > 0 {
        base_damage.saturating_mul(3) / 2
    } else {
        base_damage
    }
}

pub fn handle_orb_damage(source: usize, target: usize, base_damage: i32, state: &mut CombatState) {
    let output = orb_damage_amount_for_target(state, target, base_damage);
    handle_damage(
        crate::runtime::action::DamageInfo {
            source,
            target,
            base: base_damage,
            output,
            damage_type: DamageType::Thorns,
            is_modified: output != base_damage,
        },
        state,
    );
}

pub fn handle_orb_damage_random_enemy(source: usize, base_damage: i32, state: &mut CombatState) {
    let Some(target) = pick_random_target(state, TargetValidation::AnyEnemy) else {
        return;
    };
    let output = orb_damage_amount_for_target(state, target, base_damage);
    state.queue_action_front(Action::Damage(crate::runtime::action::DamageInfo {
        source,
        target,
        base: base_damage,
        output,
        damage_type: DamageType::Thorns,
        is_modified: output != base_damage,
    }));
}

pub fn handle_orb_damage_all_enemies(source: usize, base_damage: i32, state: &mut CombatState) {
    let mut individual_damages: smallvec::SmallVec<[Action; 5]> = smallvec::SmallVec::new();
    for monster in &state.entities.monsters {
        if monster.is_dead_or_escaped() {
            continue;
        }
        let output = orb_damage_amount_for_target(state, monster.id, base_damage);
        individual_damages.push(Action::Damage(crate::runtime::action::DamageInfo {
            source,
            target: monster.id,
            base: base_damage,
            output,
            damage_type: DamageType::Thorns,
            is_modified: output != base_damage,
        }));
    }
    for action in individual_damages.into_iter().rev() {
        state.queue_action_front(action);
    }
}

pub fn handle_damage_random_enemy(
    source: usize,
    base_damage: i32,
    damage_type: DamageType,
    state: &mut CombatState,
) {
    if let Some(target_id) = pick_random_target(state, TargetValidation::AnyEnemy) {
        state.queue_action_front(Action::Damage(crate::runtime::action::DamageInfo {
            source,
            target: target_id,
            base: base_damage,
            output: base_damage,
            damage_type,
            is_modified: false,
        }));
    }
}

pub fn handle_attack_damage_random_enemy_card(
    card: crate::runtime::combat::CombatCard,
    state: &mut CombatState,
) {
    if let Some(target_id) = pick_random_target(state, TargetValidation::AnyEnemy) {
        let evaluated =
            crate::content::cards::evaluate_card_for_play(&card, state, Some(target_id));
        state.queue_action_front(Action::Damage(crate::runtime::action::DamageInfo {
            source: 0,
            target: target_id,
            base: evaluated.base_damage_mut,
            output: evaluated.base_damage_mut,
            damage_type: DamageType::Normal,
            is_modified: false,
        }));
    }
}
