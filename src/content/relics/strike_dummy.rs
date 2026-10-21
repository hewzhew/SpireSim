use crate::runtime::combat::CombatCard;

pub const MECHANICS: &str = "Attack cards with the actual Strike tag deal 3 additional damage per hit, before later damage modifiers. This is a card-specific damage bonus, not Strength; other attacks receive no bonus. It applies to tagged cards such as Strike, Pommel Strike and Perfected Strike.";

pub fn modify_attack_damage_for_card(card: &CombatCard, damage: f32) -> f32 {
    let def = crate::content::cards::get_card_definition(card.id);
    if def.card_type == crate::content::cards::CardType::Attack
        && def.tags.contains(&crate::content::cards::CardTag::Strike)
    {
        damage + 3.0
    } else {
        damage
    }
}
