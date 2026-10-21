use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

/// PhilosopherStone: At the start of combat, ALL enemies gain 1 Strength.
/// Also grants +1 Energy (passive, handled by base_energy in combat init).
pub fn at_battle_start(state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    for monster in &state.entities.monsters {
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: monster.id,
                target: monster.id,
                power_id: crate::content::powers::PowerId::Strength,
                amount: 1,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
