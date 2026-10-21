use crate::content::cards::{get_card_definition, CardType};
use crate::runtime::combat::CombatCard;

/// Java WristBlade.atDamageModify:
/// attacks with `costForTurn == 0`, or `freeToPlayOnce && cost != -1`, gain +4.
pub fn modify_attack_damage_for_card(card: &CombatCard, damage: f32) -> f32 {
    let def = get_card_definition(card.id);
    if def.card_type != CardType::Attack {
        return damage;
    }

    let costs_zero_for_turn = card.get_cost() == 0;
    let is_non_x_free_once = card.free_to_play_once && def.cost != -1;
    if costs_zero_for_turn || is_non_x_free_once {
        damage + 4.0
    } else {
        damage
    }
}
