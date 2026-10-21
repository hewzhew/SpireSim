use crate::runtime::combat::CombatCard;

use super::super::types::CombatCardKey;

pub(super) fn card_key(card: &CombatCard) -> CombatCardKey {
    CombatCardKey {
        id: card.id,
        lineage: card.lineage,
        upgrades: card.upgrades,
        misc_value: card.misc_value,
        base_damage_override: card.base_damage_override,
        base_block_override: card.base_block_override,
        cost_modifier: card.cost_modifier,
        cost_for_turn: card.cost_for_turn,
        base_damage_mut: card.base_damage_mut,
        base_block_mut: card.base_block_mut,
        base_magic_num_mut: card.base_magic_num_mut,
        multi_damage: card.multi_damage.clone(),
        exhaust_override: card.exhaust_override,
        retain_override: card.retain_override,
        free_to_play_once: card.free_to_play_once,
        energy_on_use: card.energy_on_use,
    }
}
