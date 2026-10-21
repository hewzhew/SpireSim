use crate::runtime::action::ActionInfo;
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

/// Preserved Insect: Enemies in Elite rooms have 25% less HP.
pub fn at_battle_start(state: &mut CombatState) -> SmallVec<[ActionInfo; 4]> {
    if state.meta.is_elite_fight {
        for monster in &mut state.entities.monsters {
            let threshold = (monster.max_hp as f32 * 0.75) as i32;
            if monster.current_hp > threshold {
                monster.current_hp = threshold;
            }
        }
    }

    SmallVec::new()
}
