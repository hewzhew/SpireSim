use super::core::{
    apply_damage_to_monster_via_pipeline, clear_post_combat_actions_if_ready, handle_damage,
};
use crate::runtime::action::{Action, DamageInfo, DamageType};
use crate::runtime::combat::CombatState;
pub fn handle_vampire_damage(info: DamageInfo, state: &mut CombatState) {
    let source = info.source;
    if info.target == 0 {
        let previous_hp = state.entities.player.current_hp;
        handle_damage(info, state);
        let hp_lost = (previous_hp - state.entities.player.current_hp).max(0);
        if hp_lost > 0 {
            queue_vampire_heal_source(state, source, hp_lost, AddTo::Top);
        }
    } else {
        let outcome = apply_damage_to_monster_via_pipeline(state, &info, info.output.max(0));
        if outcome.hp_lost > 0 {
            queue_vampire_heal_source(state, source, outcome.hp_lost, AddTo::Top);
        }
        clear_post_combat_actions_if_ready(state);
    }
}

pub fn handle_vampire_damage_all_enemies(
    source: usize,
    damages: smallvec::SmallVec<[i32; 5]>,
    damage_type: DamageType,
    state: &mut CombatState,
) {
    let mut total_hp_lost = 0;
    let target_damage_pairs: Vec<(usize, i32)> = state
        .entities
        .monsters
        .iter()
        .zip(damages.iter())
        .filter_map(|(m, &dmg)| {
            // Java VampireDamageAllEnemiesAction skips only isDying,
            // currentHealth <= 0, and isEscaping.  It does not consult
            // isDeadOrEscaped(), so `halfDead` is intentionally not a filter
            // here.
            if m.is_dying || m.current_hp <= 0 || m.is_escaped {
                None
            } else {
                Some((m.id, dmg))
            }
        })
        .collect();

    for (target_id, dmg) in target_damage_pairs {
        if dmg <= 0 {
            continue;
        }
        let outcome = apply_damage_to_monster_via_pipeline(
            state,
            &DamageInfo {
                source,
                target: target_id,
                base: dmg,
                output: dmg,
                damage_type,
                is_modified: true,
            },
            dmg.max(0),
        );
        total_hp_lost += outcome.hp_lost;
    }
    if total_hp_lost > 0 {
        queue_vampire_heal_source(state, source, total_hp_lost, AddTo::Bottom);
    }
    clear_post_combat_actions_if_ready(state);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddTo {
    Top,
    Bottom,
}

fn queue_vampire_heal_source(state: &mut CombatState, source: usize, amount: i32, add_to: AddTo) {
    if amount <= 0 {
        return;
    }

    let action = Action::Heal {
        target: source,
        amount,
    };
    match add_to {
        AddTo::Top => state.queue_action_front(action),
        AddTo::Bottom => state.queue_action_back(action),
    }
}
