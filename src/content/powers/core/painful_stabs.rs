pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::PainfulStabs,
        round_duration: false,
    };

use crate::content::cards::CardId;
use crate::runtime::action::Action;

pub fn on_inflict_damage(
    damage: i32,
    damage_type: crate::runtime::action::DamageType,
) -> smallvec::SmallVec<[Action; 2]> {
    let mut actions = smallvec::SmallVec::new();
    if damage > 0 && damage_type != crate::runtime::action::DamageType::Thorns {
        actions.push(Action::MakeTempCardInDiscard {
            card_id: CardId::Wound,
            amount: 1,
            upgraded: false,
        });
    }
    actions
}
