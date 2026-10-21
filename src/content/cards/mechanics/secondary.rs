//! Reviewed ordered and secondary requests. Missing coverage is Unknown.
//! These rules do not predict prevention, queue clearing, or strategy value.

use super::CardId;
use crate::content::powers::PowerId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardEnemyDebuffV1 {
    Weak,
    Vulnerable,
}

impl CardEnemyDebuffV1 {
    pub fn power_id(self) -> PowerId {
        match self {
            Self::Weak => PowerId::Weak,
            Self::Vulnerable => PowerId::Vulnerable,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardEnemyDebuffRequestV1 {
    pub debuff: CardEnemyDebuffV1,
    pub amount: i32,
}

/// Queue this sequence for each monster in list order, before the next monster.
/// Each request resolves independently (including Artifact and dead/escaped
/// filtering in ApplyPower). This is not an unordered aggregate of debuffs.
pub fn card_enemy_debuffs_in_monster_order_v1(
    card: CardId,
    effective_magic: i32,
) -> Option<Vec<CardEnemyDebuffRequestV1>> {
    (card == CardId::Shockwave).then(|| {
        [CardEnemyDebuffV1::Weak, CardEnemyDebuffV1::Vulnerable]
            .into_iter()
            .map(|debuff| CardEnemyDebuffRequestV1 {
                debuff,
                amount: effective_magic,
            })
            .collect()
    })
}

/// Install additive Rupture stacks. Each positive, Rupture-eligible HP-loss
/// event requests Strength equal to the current power amount, not HP lost.
/// Eligibility belongs to the damage/HP-loss producer; incoming enemy damage
/// is not eligible. No Strength is granted by playing this card itself.
pub fn card_hp_loss_strength_power_v1(card: CardId, effective_magic: i32) -> Option<i32> {
    (card == CardId::Rupture).then_some(effective_magic)
}

/// Playing requires a Skill in the draw pile. The queued effect rechecks at
/// resolution: zero matches does nothing, one auto-moves, multiple require
/// an uncancellable selection. Earlier hooks may change the pile in between.
/// No Draw does not gate this move; a full hand routes it to discard. The card
/// instance keeps its costs; this effect neither draws nor generates a card.
pub fn card_draw_pile_skill_to_hand_v1(card: CardId) -> Option<u8> {
    (card == CardId::SecretTechnique).then_some(1)
}

/// Queue combat-only stat-equivalent copies to discard after damage. Copies
/// retain upgrades and persistent instance changes, not rendered damage; the
/// generation owner allocates fresh lineage and applies Master Reality.
/// Combat-end queue clearing may prevent this queued request from resolving.
pub fn card_copy_to_discard_after_damage_v1(card: CardId) -> Option<u8> {
    (card == CardId::Anger).then_some(1)
}

/// One combat-only random Attack from the player's class pool is generated
/// for Hand (overflow goes to Discard). The cost override expires at turn end;
/// the generated card then follows its own ordinary cost and lifecycle rules.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardRandomAttackToHandV1 {
    pub count: u8,
    pub cost_for_turn: u8,
}

pub fn card_random_attack_to_hand_v1(card: CardId) -> Option<CardRandomAttackToHandV1> {
    (card == CardId::InfernalBlade)
        .then_some(super::super::ironclad::infernal_blade::RANDOM_ATTACK_TO_HAND)
}

#[cfg(test)]
#[path = "secondary_tests.rs"]
mod tests;
