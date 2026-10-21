use crate::runtime::action::Action;
use smallvec::SmallVec;

pub fn on_use_card(card: &crate::runtime::combat::CombatCard) -> SmallVec<[Action; 4]> {
    let mut actions = SmallVec::new();
    let def = crate::content::cards::get_card_definition(card.id);
    if def.card_type == crate::content::cards::CardType::Attack {
        actions.push(Action::RemovePower {
            target: 0,
            power_id: crate::content::powers::PowerId::PenNibPower,
        });
    }
    actions
}

pub fn on_calculate_damage_to_enemy(damage: f32) -> f32 {
    damage * 2.0
}
