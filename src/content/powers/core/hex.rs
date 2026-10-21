use crate::runtime::action::Action;
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;

pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Dazed cards added per non-Attack card played",
    effect: "When the owner plays a non-Attack card, including a Skill or Power, add amount unupgraded Dazed to random positions in the owner's draw pile. Dazed is unplayable and Ethereal. Attacks do not trigger this effect.",
    timing: "Card play; persists until removed. Does not decrement each round. Artifact can block its application and debuff removal can remove it.",
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
