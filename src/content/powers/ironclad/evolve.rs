use crate::content::cards::{get_card_definition, CardType};
use crate::content::powers::{store, PowerId};
use crate::runtime::action::Action;
use crate::runtime::combat::CombatState;
use smallvec::SmallVec;

pub fn on_card_drawn(
    state: &CombatState,
    owner: crate::EntityId,
    card_id: crate::content::cards::CardId,
    amount: i32,
) -> SmallVec<[Action; 2]> {
    let mut actions = SmallVec::new();
    let def = get_card_definition(card_id);
    if def.card_type == CardType::Status && !store::has_power(state, owner, PowerId::NoDraw) {
        actions.push(Action::DrawCards(amount as u32));
    }
    actions
}
