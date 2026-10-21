use crate::runtime::action::{Action, DamageInfo};

use super::projection::PhaseProjection;

pub(super) fn observe_actions_damage(
    projection: &mut PhaseProjection,
    actions: impl IntoIterator<Item = Action>,
) {
    for action in actions {
        observe_action_damage(projection, action);
    }
}

fn observe_action_damage(projection: &mut PhaseProjection, action: Action) {
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
        | Action::Barrage { damage: info } => observe_damage_info(projection, &info),
        Action::DamageAllEnemies { damages, .. }
        | Action::VampireDamageAllEnemies { damages, .. } => {
            for (slot, damage) in damages.iter().copied().enumerate() {
                projection.apply_damage_to_slot(slot, damage);
            }
        }
        Action::Whirlwind { damages, .. } => {
            for (slot, damage) in damages.iter().copied().enumerate() {
                projection.apply_damage_to_slot(slot, damage);
            }
        }
        _ => {}
    }
}

fn observe_damage_info(projection: &mut PhaseProjection, info: &DamageInfo) {
    projection.apply_damage_to_entity(info.target, info.output);
}
