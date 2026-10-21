pub const MECHANICS: super::super::mechanics::PowerMechanics = super::super::mechanics::PowerMechanics {
    amount_unit: "presence flag, not a numeric stack count",
    effect: "Whenever the owner inflicts positive HP damage other than THORNS damage, add 1 Wound to the player's discard pile. Fully blocked hits add none; multiple damaging hits each add a Wound.",
    timing: "On-inflict-damage trigger; persists until removed.",
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
