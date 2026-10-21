use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PotionPlanningContext {
    pub(super) visible_incoming_damage: i32,
    pub(super) player_block: i32,
    pub(super) player_current_hp: i32,
    pub(super) player_max_hp: i32,
    pub(super) hp_after_visible_attack: i32,
    pub(super) visible_hp_loss: bool,
    pub(super) visible_uncovered_damage_after_hand_block: i32,
    pub(super) visible_attack_is_lethal: bool,
    pub(super) hand_damage_upper_bound: i32,
    pub(super) hand_block_upper_bound: i32,
    pub(super) has_visible_lethal: bool,
    pub(super) living_enemy_count: usize,
    pub(super) lowest_enemy_hp_with_block: Option<i32>,
    pub(super) total_enemy_hp_with_block: i32,
    pub(super) high_stakes_combat: bool,
}

impl PotionPlanningContext {
    pub(super) fn from_combat(combat: &CombatState) -> Self {
        let visible_incoming_damage = visible_incoming_damage(combat);
        let player_block = combat.entities.player.block;
        let player_current_hp = combat.entities.player.current_hp;
        let player_max_hp = combat.entities.player.max_hp;
        let visible_hp_loss = visible_incoming_damage > player_block;
        let hp_after_visible_attack =
            player_current_hp - (visible_incoming_damage - player_block).max(0);
        let hand_damage_upper_bound = playable_hand_damage(combat);
        let hand_block_upper_bound = playable_hand_block(combat);
        let visible_uncovered_damage_after_hand_block =
            (visible_incoming_damage - player_block - hand_block_upper_bound).max(0);
        let visible_attack_is_lethal = hp_after_visible_attack <= 0;
        let living_enemy_hp = living_enemy_hp_with_block(combat);
        let total_enemy_hp_with_block = living_enemy_hp.iter().sum();
        let lowest_enemy_hp_with_block = living_enemy_hp.iter().copied().min();
        let living_enemy_count = living_enemy_hp.len();
        let high_stakes_combat = combat.meta.is_elite_fight || combat.meta.is_boss_fight;

        Self {
            visible_incoming_damage,
            player_block,
            player_current_hp,
            player_max_hp,
            hp_after_visible_attack,
            visible_hp_loss,
            visible_uncovered_damage_after_hand_block,
            visible_attack_is_lethal,
            hand_damage_upper_bound,
            hand_block_upper_bound,
            has_visible_lethal: hand_damage_upper_bound >= total_enemy_hp_with_block
                && living_enemy_count > 0,
            living_enemy_count,
            lowest_enemy_hp_with_block,
            total_enemy_hp_with_block,
            high_stakes_combat,
        }
    }

    pub(super) fn has_living_enemy(self) -> bool {
        self.living_enemy_count > 0
    }

    pub(super) fn player_is_wounded(self) -> bool {
        self.player_current_hp < self.player_max_hp
    }

    pub(super) fn lacks_visible_lethal(self) -> bool {
        !self.has_visible_lethal
    }

    pub(super) fn has_uncovered_visible_hp_loss(self) -> bool {
        self.visible_uncovered_damage_after_hand_block > 0
    }
}

fn living_enemy_hp_with_block(combat: &CombatState) -> Vec<i32> {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| monster.current_hp + monster.block)
        .collect()
}

fn playable_hand_damage(combat: &CombatState) -> i32 {
    combat
        .zones
        .hand
        .iter()
        .filter(|card| crate::content::cards::can_play_card(card, combat).is_ok())
        .map(|card| {
            let def = crate::content::cards::get_card_definition(card.id);
            card.base_damage_override
                .unwrap_or(def.base_damage + def.upgrade_damage * card.upgrades as i32)
                .max(0)
        })
        .sum()
}

fn playable_hand_block(combat: &CombatState) -> i32 {
    combat
        .zones
        .hand
        .iter()
        .filter(|card| crate::content::cards::can_play_card(card, combat).is_ok())
        .map(|card| {
            let def = crate::content::cards::get_card_definition(card.id);
            card.base_block_override
                .unwrap_or(def.base_block + def.upgrade_block * card.upgrades as i32)
                .max(0)
        })
        .sum()
}
