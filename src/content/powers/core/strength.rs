pub const MECHANICS: super::super::mechanics::PowerMechanics =
    super::super::mechanics::PowerMechanics {
        text_id: crate::content::powers::PowerId::Strength,
        round_duration: false,
    };

use crate::content::cards::CardId;

/// Signed Strength contribution per requested attack hit. The card's magic
/// parameter is already upgrade/instance evaluated by the caller.
pub fn card_strength_multiplier_v1(card_id: CardId, base_magic_num: i32) -> i32 {
    if card_id == CardId::HeavyBlade {
        base_magic_num
    } else {
        1
    }
}

pub fn on_calculate_damage_to_enemy(
    card_id: CardId,
    base_magic_num: i32,
    mut damage: f32,
    amount: i32,
) -> f32 {
    let strength_multiplier = card_strength_multiplier_v1(card_id, base_magic_num);
    damage += (amount * strength_multiplier) as f32;
    damage
}

pub fn on_attack_to_change_damage(current_damage: i32, amount: i32) -> i32 {
    current_damage + amount
}
