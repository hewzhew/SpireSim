use crate::runtime::action::{repeated_damage_matrix, Action, DamageType, NO_SOURCE};
use crate::runtime::combat::CombatState;
use crate::EntityId;

pub fn on_death(
    state: &CombatState,
    owner: EntityId,
    amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    if state.are_monsters_basically_dead_java() {
        return smallvec::smallvec![];
    }

    let Some(monster) = state.entities.monsters.iter().find(|m| m.id == owner) else {
        return smallvec::smallvec![];
    };

    if monster.current_hp > 0 {
        return smallvec::smallvec![];
    }

    smallvec::smallvec![Action::DamageAllEnemies {
        source: NO_SOURCE,
        damages: repeated_damage_matrix(state.entities.monsters.len(), monster.max_hp * amount),
        damage_type: DamageType::Thorns,
        is_modified: false,
    }]
}
