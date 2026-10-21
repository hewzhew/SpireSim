pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "Block gained per Attack card played",
    effect: "When the player plays an Attack card, the owner gains Block equal to amount (at least 0). Triggers once per card, not once per hit; Skills and Powers do not trigger it.",
    timing: "Active while present; removed at the end of the player turn. Amount is not a duration.",
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
