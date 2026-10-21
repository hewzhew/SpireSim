use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub struct GremlinHorn;

impl GremlinHorn {
    pub fn on_monster_death(
        state: &crate::runtime::combat::CombatState,
        target: crate::EntityId,
    ) -> SmallVec<[ActionInfo; 4]> {
        let mut actions = SmallVec::new();
        let target_is_dead = state
            .entities
            .monsters
            .iter()
            .find(|monster| monster.id == target)
            .is_some_and(|monster| monster.current_hp == 0 && !monster.is_escaped);
        if !target_is_dead {
            return actions;
        }

        let any_other_monster_alive = state.entities.monsters.iter().any(|monster| {
            monster.id != target
                && monster.current_hp > 0
                && !monster.is_dying
                && !monster.is_escaped
                && !monster.half_dead
        });
        if !any_other_monster_alive {
            return actions;
        }

        actions.push(ActionInfo {
            action: Action::GainEnergy { amount: 1 },
            insertion_mode: AddTo::Bottom, // AddTo::Top if it needs to resolve immediately, but Bottom matches the native cadence
        });
        actions.push(ActionInfo {
            action: Action::DrawCards(1),
            insertion_mode: AddTo::Bottom, // Bottom resolves draw after energy cleanly
        });
        actions
    }
}
