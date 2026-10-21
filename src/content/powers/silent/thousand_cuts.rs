use crate::runtime::action::{repeated_damage_matrix, Action};
use crate::runtime::combat::CombatState;
use crate::EntityId;

pub fn on_after_card_played(
    state: &CombatState,
    owner: EntityId,
    amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::SmallVec::new();
    if amount > 0 {
        // Java: ThousandCutsPower.onAfterCardPlayed queues DamageType.THORNS.
        actions.push(Action::DamageAllEnemies {
            source: owner,
            damages: repeated_damage_matrix(state.entities.monsters.len(), amount),
            damage_type: crate::runtime::action::DamageType::Thorns,
            is_modified: false,
        });
    }
    actions
}
