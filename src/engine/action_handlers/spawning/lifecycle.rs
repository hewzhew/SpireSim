use crate::runtime::combat::CombatState;
pub fn handle_suicide(target: usize, trigger_relics: bool, state: &mut CombatState) {
    if let Some(m) = state.entities.monsters.iter_mut().find(|m| m.id == target) {
        m.current_hp = 0;
        if !trigger_relics {
            m.is_dying = true;
        }
    }

    if trigger_relics {
        crate::engine::action_handlers::check_and_trigger_monster_death(state, target);
    }
}

pub fn handle_escape(target: usize, state: &mut CombatState) {
    if let Some(m) = state.entities.monsters.iter_mut().find(|m| m.id == target) {
        m.is_escaped = true;
        if matches!(
            m.enemy_id,
            crate::content::monsters::EnemyId::Looter | crate::content::monsters::EnemyId::Mugger
        ) {
            state.runtime.combat_mugged = true;
        }
    }
}

pub fn handle_add_combat_reward(item: crate::state::rewards::RewardItem, state: &mut CombatState) {
    state.runtime.pending_rewards.push(item);
}

pub fn handle_revive_monster(target: usize, state: &mut CombatState) {
    if let Some(monster) = state.entities.monsters.iter_mut().find(|m| m.id == target) {
        monster.is_dying = false;
        monster.half_dead = false;
    }
}
