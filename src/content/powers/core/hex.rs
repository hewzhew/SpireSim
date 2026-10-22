use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Hex,
        round_duration: false,
    };

pub fn on_card_played(
    _state: &CombatState,
    _owner: EntityId,
    card: &CombatCard,
    power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::smallvec![];

    let def = crate::content::cards::get_card_definition(card.id);
    if def.card_type != crate::content::cards::CardType::Attack {
        actions.push(Action::MakeTempCardInDrawPile {
            card_id: crate::content::cards::CardId::Dazed,
            amount: power_amount as u8,
            random_spot: true,
            to_bottom: false,
            upgraded: false,
        });
    }

    actions
}
