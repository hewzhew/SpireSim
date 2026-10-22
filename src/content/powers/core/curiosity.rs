pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Curiosity,
        round_duration: false,
    };

use crate::content::powers::PowerId;
use crate::runtime::action::Action;
use crate::runtime::combat::CombatCard;
use crate::EntityId;

pub fn on_player_card_played(
    owner: EntityId,
    amount: i32,
    card: &CombatCard,
) -> smallvec::SmallVec<[Action; 2]> {
    let def = crate::content::cards::get_card_definition(card.id);
    if def.card_type == crate::content::cards::CardType::Power {
        smallvec::smallvec![Action::ApplyPower {
            source: owner,
            target: owner,
            power_id: PowerId::Strength,
            amount,
        }]
    } else {
        smallvec::smallvec![]
    }
}
