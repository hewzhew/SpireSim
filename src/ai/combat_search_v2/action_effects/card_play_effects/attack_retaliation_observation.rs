use crate::runtime::action::{Action, DamageInfo, DamageType};
use crate::runtime::combat::CombatState;

use super::observation::CardPlayEffectAccumulator;

pub(super) fn observe_attack_retaliation_action(
    combat: &CombatState,
    accumulator: &mut CardPlayEffectAccumulator,
    action: &Action,
) {
    match action {
        Action::Damage(info)
        | Action::PummelDamage(info)
        | Action::BaneDamage(info)
        | Action::WallopDamage(info)
        | Action::DamagePerAttackPlayed(info)
        | Action::HeelHook(info)
        | Action::Flechettes(info)
        | Action::DropkickDamageAndEffect {
            damage_info: info, ..
        }
        | Action::Ftl {
            damage_info: info, ..
        }
        | Action::Skewer {
            damage_info: info, ..
        }
        | Action::Sunder {
            damage_info: info, ..
        }
        | Action::FearNoEvil {
            damage_info: info, ..
        }
        | Action::FiendFire {
            damage_info: info, ..
        }
        | Action::Feed {
            damage_info: info, ..
        }
        | Action::LessonLearned {
            damage_info: info, ..
        }
        | Action::HandOfGreed {
            damage_info: info, ..
        }
        | Action::RitualDagger {
            damage_info: info, ..
        }
        | Action::VampireDamage(info)
        | Action::Barrage { damage: info } => observe_damage_info(combat, accumulator, info),
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => observe_damage_matrix(
            combat,
            accumulator,
            *source,
            damages,
            *damage_type,
            *is_modified,
        ),
        Action::VampireDamageAllEnemies {
            source,
            damages,
            damage_type,
        } => observe_damage_matrix(combat, accumulator, *source, damages, *damage_type, false),
        Action::Whirlwind {
            damages,
            damage_type,
            ..
        } => observe_damage_matrix(combat, accumulator, 0, damages, *damage_type, false),
        _ => {}
    }
}

fn observe_damage_matrix(
    combat: &CombatState,
    accumulator: &mut CardPlayEffectAccumulator,
    source: usize,
    damages: &[i32],
    damage_type: DamageType,
    is_modified: bool,
) {
    for (slot, &damage) in damages.iter().enumerate() {
        let Some(monster) = combat.entities.monsters.get(slot) else {
            continue;
        };
        observe_damage_info(
            combat,
            accumulator,
            &DamageInfo {
                source,
                target: monster.id,
                base: damage,
                output: damage,
                damage_type,
                is_modified,
            },
        );
    }
}

fn observe_damage_info(
    combat: &CombatState,
    accumulator: &mut CardPlayEffectAccumulator,
    info: &DamageInfo,
) {
    let projection = super::super::super::attack_retaliation::attack_retaliation_for_event(
        combat,
        &mut accumulator.retaliation_projection_state,
        info,
    );
    if projection.raw_player_damage <= 0 {
        return;
    }
    accumulator.reactive.attack_retaliation_trigger_count_hint = accumulator
        .reactive
        .attack_retaliation_trigger_count_hint
        .saturating_add(projection.trigger_count);
    accumulator
        .reactive
        .attack_retaliation_raw_player_damage_hint = accumulator
        .reactive
        .attack_retaliation_raw_player_damage_hint
        .saturating_add(projection.raw_player_damage);
    accumulator
        .reactive
        .attack_retaliation_player_block_loss_hint = accumulator
        .reactive
        .attack_retaliation_player_block_loss_hint
        .saturating_add(projection.player_block_loss);
    accumulator.reactive.attack_retaliation_player_hp_loss_hint = accumulator
        .reactive
        .attack_retaliation_player_hp_loss_hint
        .saturating_add(projection.player_hp_loss);
    accumulator.reactive.player_hp_loss = accumulator
        .reactive
        .player_hp_loss
        .saturating_add(projection.player_hp_loss);
}
