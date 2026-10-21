use serde::{Deserialize, Serialize};

use crate::ai::card_semantics_v1::card_reward_facts_v1;
use crate::content::cards::CardId;
use crate::state::rewards::RewardCard;

/// One unconditional output axis supplied by the same card play.
///
/// This is a mechanics projection, not an acquisition verdict. In particular,
/// several small axes may still fail to justify spending a draw slot, while a
/// context owner may reject even a dense card because the deck does not need
/// its outputs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CardTacticalOutputAxisV1 {
    DirectDamage,
    Block,
    Weak,
    Vulnerable,
    EnemyStrengthDown,
    CardDraw,
    EnergyGain,
}

/// Context-free delivery facts for one additional draw slot.
///
/// The individual density fields preserve energy/output distinctions used by
/// threat coverage. `multi_axis_draw_slot_density` captures a different fact:
/// one play simultaneously carries at least three visible axes, including at
/// least one substantial axis. It must not be interpreted as a scalar card
/// score or as permission to acquire the card.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardTacticalDeliveryProfileV1 {
    pub card: CardId,
    pub upgrades: u8,
    pub energy_cost: i8,
    pub direct_damage_dense: bool,
    pub block_dense: bool,
    pub reliable_weak: bool,
    pub reliable_vulnerable: bool,
    pub simultaneous_output_axes: Vec<CardTacticalOutputAxisV1>,
    pub multi_axis_draw_slot_density: bool,
}

impl CardTacticalDeliveryProfileV1 {
    pub fn has_draw_slot_density(&self) -> bool {
        self.direct_damage_dense
            || self.block_dense
            || self.reliable_weak
            || self.reliable_vulnerable
            || self.multi_axis_draw_slot_density
            || self
                .simultaneous_output_axes
                .contains(&CardTacticalOutputAxisV1::EnemyStrengthDown)
    }
}

pub fn card_tactical_delivery_profile_v1(
    card: CardId,
    upgrades: u8,
) -> CardTacticalDeliveryProfileV1 {
    let facts = card_reward_facts_v1(&RewardCard::new(card, upgrades));
    let base_damage = facts.damage.unconditional_base_total().unwrap_or(0);
    let direct_damage_dense = base_damage_is_dense_v1(i32::from(facts.cost), base_damage);
    let unconditional_block = facts.unconditional_base_block_request_v1().unwrap_or(0);
    let block_dense = base_block_is_dense_v1(i32::from(facts.cost), unconditional_block);
    let reliable_weak = facts.weak > 0 && (facts.cost <= 1 || facts.weak >= 3);
    let reliable_vulnerable = facts.vulnerable > 0 && (facts.cost <= 1 || facts.vulnerable >= 2);

    let mut simultaneous_output_axes = Vec::new();
    if base_damage >= 10 {
        simultaneous_output_axes.push(CardTacticalOutputAxisV1::DirectDamage);
    }
    if unconditional_block >= 8 {
        simultaneous_output_axes.push(CardTacticalOutputAxisV1::Block);
    }
    if facts.weak > 0 {
        simultaneous_output_axes.push(CardTacticalOutputAxisV1::Weak);
    }
    if facts.vulnerable > 0 {
        simultaneous_output_axes.push(CardTacticalOutputAxisV1::Vulnerable);
    }
    if facts.enemy_strength_down > 0 {
        simultaneous_output_axes.push(CardTacticalOutputAxisV1::EnemyStrengthDown);
    }
    if facts.draw_cards > 0 {
        simultaneous_output_axes.push(CardTacticalOutputAxisV1::CardDraw);
    }
    if facts.energy_gain > 0 {
        simultaneous_output_axes.push(CardTacticalOutputAxisV1::EnergyGain);
    }

    let has_substantial_axis = base_damage >= 10
        || unconditional_block >= 8
        || reliable_weak
        || reliable_vulnerable
        || facts.enemy_strength_down > 0
        || facts.draw_cards >= 2
        || facts.energy_gain >= 2;
    let multi_axis_draw_slot_density = (0..=2).contains(&facts.cost)
        && simultaneous_output_axes.len() >= 3
        && has_substantial_axis
        && !facts.is_random_output
        && !facts.has_conditional_playability;

    CardTacticalDeliveryProfileV1 {
        card,
        upgrades,
        energy_cost: facts.cost,
        direct_damage_dense,
        block_dense,
        reliable_weak,
        reliable_vulnerable,
        simultaneous_output_axes,
        multi_axis_draw_slot_density,
    }
}

pub(crate) fn base_damage_is_dense_v1(cost: i32, base_damage: i32) -> bool {
    base_damage >= 20 || (cost <= 1 && base_damage >= 10) || (cost == 0 && base_damage > 0)
}

pub(crate) fn base_block_is_dense_v1(cost: i32, block: i32) -> bool {
    block >= 20 || (cost <= 1 && block >= 8) || (cost == 0 && block > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_density_requires_base_requests_and_preserves_real_multi_hits() {
        for card in [
            CardId::Wish,
            CardId::Whirlwind,
            CardId::Skewer,
            CardId::FiendFire,
            CardId::Barrage,
            CardId::Flechettes,
            CardId::Finisher,
            CardId::ThunderStrike,
            CardId::Expunger,
            CardId::Clash,
            CardId::GrandFinale,
            CardId::SignatureMove,
        ] {
            for upgrades in [0, 1] {
                let profile = card_tactical_delivery_profile_v1(card, upgrades);
                assert!(!profile.direct_damage_dense, "{card:?}+{upgrades}");
                assert!(!profile
                    .simultaneous_output_axes
                    .contains(&CardTacticalOutputAxisV1::DirectDamage));
            }
        }
        assert!(card_tactical_delivery_profile_v1(CardId::Pummel, 1).direct_damage_dense);
        assert!(card_tactical_delivery_profile_v1(CardId::GlassKnife, 0).direct_damage_dense);
        assert!(card_tactical_delivery_profile_v1(CardId::Eviscerate, 0).direct_damage_dense);
    }

    #[test]
    fn conditional_repeated_and_choice_block_are_not_unconditional_delivery() {
        for card in [
            CardId::EscapePlan,
            CardId::AutoShields,
            CardId::ReinforcedBody,
            CardId::Wish,
        ] {
            for upgrades in [0, 1] {
                let delivery = card_tactical_delivery_profile_v1(card, upgrades);
                assert!(
                    !delivery.block_dense,
                    "{card:?}+{upgrades} is not unconditional block"
                );
                assert!(
                    !delivery
                        .simultaneous_output_axes
                        .contains(&CardTacticalOutputAxisV1::Block),
                    "{card:?}+{upgrades}"
                );
            }
        }
    }

    #[test]
    fn simultaneous_delivery_distinguishes_uppercut_from_two_weak_halves() {
        let uppercut = card_tactical_delivery_profile_v1(CardId::Uppercut, 0);
        assert_eq!(
            uppercut.simultaneous_output_axes,
            [
                CardTacticalOutputAxisV1::DirectDamage,
                CardTacticalOutputAxisV1::Weak,
                CardTacticalOutputAxisV1::Vulnerable,
            ]
        );
        assert!(uppercut.multi_axis_draw_slot_density);
        assert!(uppercut.has_draw_slot_density());
        assert!(!uppercut.direct_damage_dense);
        assert!(!uppercut.reliable_weak);
        assert!(!uppercut.reliable_vulnerable);

        let iron_wave = card_tactical_delivery_profile_v1(CardId::IronWave, 1);
        assert!(iron_wave.simultaneous_output_axes.is_empty());
        assert!(!iron_wave.multi_axis_draw_slot_density);
        assert!(!iron_wave.has_draw_slot_density());

        let clothesline = card_tactical_delivery_profile_v1(CardId::Clothesline, 0);
        assert_eq!(
            clothesline.simultaneous_output_axes,
            [
                CardTacticalOutputAxisV1::DirectDamage,
                CardTacticalOutputAxisV1::Weak,
            ]
        );
        assert!(!clothesline.multi_axis_draw_slot_density);
        assert!(!clothesline.has_draw_slot_density());
    }

    #[test]
    fn single_axis_density_remains_separate_from_multi_axis_delivery() {
        let carnage = card_tactical_delivery_profile_v1(CardId::Carnage, 0);
        assert!(carnage.direct_damage_dense);
        assert!(!carnage.multi_axis_draw_slot_density);

        let shrug = card_tactical_delivery_profile_v1(CardId::ShrugItOff, 0);
        assert!(shrug.block_dense);
        assert!(!shrug.multi_axis_draw_slot_density);

        let shockwave = card_tactical_delivery_profile_v1(CardId::Shockwave, 0);
        assert!(shockwave.reliable_weak);
        assert!(shockwave.reliable_vulnerable);
        assert!(!shockwave.multi_axis_draw_slot_density);

        let upgraded_clothesline = card_tactical_delivery_profile_v1(CardId::Clothesline, 1);
        assert!(upgraded_clothesline.reliable_weak);
        let upgraded_uppercut = card_tactical_delivery_profile_v1(CardId::Uppercut, 1);
        assert!(upgraded_uppercut.reliable_vulnerable);

        let sentinel = card_tactical_delivery_profile_v1(CardId::Sentinel, 0);
        assert!(!sentinel.block_dense);
    }
}
