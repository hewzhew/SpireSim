use crate::content::cards::{get_card_definition, CardType};
use crate::runtime::action::{repeated_damage_matrix, Action, DamageType, NO_SOURCE};
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub fn on_card_drawn(
    state: &CombatState,
    _owner: crate::EntityId,
    card_id: crate::content::cards::CardId,
    amount: i32,
) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    let def = get_card_definition(card_id);
    if def.card_type == CardType::Status || def.card_type == CardType::Curse {
        actions.push(Action::DamageAllEnemies {
            source: NO_SOURCE,
            damages: repeated_damage_matrix(state.entities.monsters.len(), amount),
            damage_type: DamageType::Thorns,
            is_modified: false,
        });
    }
    actions
}
