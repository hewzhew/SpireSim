pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Rage,
        round_duration: false,
    };

use crate::runtime::action::Action;
use crate::runtime::combat::CombatCard;
use crate::EntityId;

pub fn on_card_played(
    owner: EntityId,
    card: &CombatCard,
    power_amount: i32,
) -> smallvec::SmallVec<[Action; 2]> {
    let def = crate::content::cards::get_card_definition(card.id);
    if def.card_type == crate::content::cards::CardType::Attack {
        smallvec::smallvec![Action::GainBlock {
            target: owner,
            amount: power_amount.max(0),
        }]
    } else {
        smallvec::smallvec![]
    }
}
