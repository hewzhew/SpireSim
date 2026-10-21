//! Static output of an owned card, retaining its persistent instance numbers.
//! This is not acquisition text or a prediction for a particular combat state.

use super::CardRewardDamageFactsV1;
use crate::content::cards::mechanics::{
    card_base_block_delivery_v1, card_persistent_base_numbers_v1, CardAttackAmountV1,
    CardAttackHitsV1, CardBaseBlockDeliveryV1, CardBaseDamageDeliveryV1,
};
use crate::content::cards::{get_card_definition, upgrade_card_once_java};
use crate::runtime::combat::CombatCard;

#[derive(Clone, Debug, PartialEq)]
pub struct CardOwnedBaseOutputV1 {
    pub damage: CardRewardDamageFactsV1,
    /// Definition/instance parameter, qualified by base_block_delivery.
    pub base_block: i32,
    pub base_block_delivery: Option<CardBaseBlockDeliveryV1>,
    /// Card-specific parameter, not a universally additive effect.
    pub base_magic: i32,
    /// None includes unreviewed, conditional and variable delivery.
    pub unconditional_block: Option<i32>,
    /// Persistent cost, excluding cost-for-turn and temporary free play.
    pub energy_cost: i32,
}

pub fn card_owned_base_output_v1(card: &CombatCard) -> CardOwnedBaseOutputV1 {
    let numbers = card_persistent_base_numbers_v1(card);
    let definition = get_card_definition(card.id);
    let base_block_delivery = card_base_block_delivery_v1(card.id);
    CardOwnedBaseOutputV1 {
        damage: CardRewardDamageFactsV1::from_base_value(card.id, card.upgrades, numbers.damage),
        base_block: numbers.block,
        base_block_delivery,
        base_magic: definition.base_magic + i32::from(card.upgrades > 0) * definition.upgrade_magic,
        unconditional_block: base_block_delivery
            .filter(|rule| rule.has_unconditional_base_request())
            .map(|_| numbers.block.max(0)),
        energy_cost: card.combat_cost_without_turn_override_java(),
    }
}

/// A private, one-step preview through the same upgrade operation as the run
/// and combat. It owns no card identity or action permission and never mutates
/// the caller. Before/after rules preserve variable counts and choice effects.
#[derive(Clone, Debug, PartialEq)]
pub struct CardOwnedUpgradeOutputV1 {
    pub before: CardOwnedBaseOutputV1,
    pub after: CardOwnedBaseOutputV1,
}

pub fn card_owned_upgrade_output_v1(card: &CombatCard) -> Option<CardOwnedUpgradeOutputV1> {
    card.upgrades.checked_add(1)?;
    let mut upgraded = card.clone();
    if !upgrade_card_once_java(&mut upgraded) {
        return None;
    }
    Some(CardOwnedUpgradeOutputV1 {
        before: card_owned_base_output_v1(card),
        after: card_owned_base_output_v1(&upgraded),
    })
}

impl CardOwnedUpgradeOutputV1 {
    /// Delta of each card's unconditional base contribution, not complete
    /// resolved damage or focused damage for random-target requests.
    pub fn unconditional_damage_delta(&self) -> Option<i32> {
        Some(
            self.after
                .damage
                .unconditional_base_total()?
                .saturating_sub(self.before.damage.unconditional_base_total()?),
        )
    }

    pub fn unconditional_block_delta(&self) -> Option<i32> {
        Some(
            self.after
                .unconditional_block?
                .saturating_sub(self.before.unconditional_block?),
        )
    }

    /// Changed parameters/rules whose complete value cannot be represented by
    /// the two unconditional deltas. Includes unreviewed delivery, not just
    /// known conditions; callers must inspect the retained rules before use.
    pub fn has_unresolved_output_change(&self) -> bool {
        let fixed_base_attack = |output: &CardOwnedBaseOutputV1| {
            matches!(
                output.damage.delivery,
                Some(CardBaseDamageDeliveryV1::Attack {
                    amount: CardAttackAmountV1::Base,
                    hits: CardAttackHitsV1::Fixed { .. },
                    play_condition: None,
                    ..
                })
            )
        };
        let damage_changed = self.before.damage != self.after.damage
            && !(fixed_base_attack(&self.before) && fixed_base_attack(&self.after));
        let block_changed = (self.before.base_block != self.after.base_block
            || self.before.base_block_delivery != self.after.base_block_delivery
            || (self.before.base_magic != self.after.base_magic
                && self.after.base_block_delivery.is_some()))
            && !(self.before.base_block_delivery == Some(CardBaseBlockDeliveryV1::OnceOnPlay)
                && self.after.base_block_delivery == Some(CardBaseBlockDeliveryV1::OnceOnPlay));
        damage_changed || block_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;

    #[test]
    fn upgrade_preview_retains_instance_growth_and_does_not_mutate_input() {
        for (id, upgrades, override_damage, expected_delta) in [
            (CardId::Pummel, 0, Some(13), 13),
            (CardId::TwinStrike, 0, Some(13), 4),
            (CardId::SearingBlow, 5, None, 9),
        ] {
            let mut card = CombatCard::for_scenario(id, 1);
            card.upgrades = upgrades;
            card.base_damage_override = override_damage;
            card.base_damage_mut = 200;
            let original = card.clone();
            let preview = card_owned_upgrade_output_v1(&card).unwrap();
            assert_eq!(
                preview.unconditional_damage_delta(),
                Some(expected_delta),
                "{id:?}"
            );
            assert!(!preview.has_unresolved_output_change(), "{id:?}");
            assert_eq!(card, original);
        }
        let mut upgraded = CombatCard::for_scenario(CardId::Strike, 1);
        upgraded.upgrades = 1;
        assert!(card_owned_upgrade_output_v1(&upgraded).is_none());
        assert!(card_owned_upgrade_output_v1(&CombatCard::for_scenario(CardId::Burn, 1)).is_none());
    }

    #[test]
    fn upgrade_preview_preserves_variable_and_choice_parameters() {
        let preview = |id| card_owned_upgrade_output_v1(&CombatCard::for_scenario(id, 1)).unwrap();
        let whirlwind = preview(CardId::Whirlwind);
        assert_eq!(whirlwind.unconditional_damage_delta(), None);
        assert_eq!(
            (
                whirlwind.before.damage.damage_per_hit,
                whirlwind.after.damage.damage_per_hit
            ),
            (5, 8)
        );
        assert!(matches!(
            whirlwind.after.damage.delivery,
            Some(CardBaseDamageDeliveryV1::Attack {
                hits: CardAttackHitsV1::PerEffectiveX,
                ..
            })
        ));
        assert!(whirlwind.has_unresolved_output_change());
        let second_wind = preview(CardId::SecondWind);
        assert_eq!(second_wind.unconditional_block_delta(), None);
        assert_eq!(
            (second_wind.before.base_block, second_wind.after.base_block),
            (5, 7)
        );
        assert_eq!(
            second_wind.after.base_block_delivery,
            Some(CardBaseBlockDeliveryV1::PerNonAttackInHandAtResolution)
        );
        assert!(second_wind.has_unresolved_output_change());
        let wish = preview(CardId::Wish);
        assert_eq!(wish.unconditional_damage_delta(), None);
        assert_eq!(wish.unconditional_block_delta(), None);
        assert_eq!(
            wish.after.damage.delivery,
            Some(CardBaseDamageDeliveryV1::StrengthIfChosen)
        );
        assert_eq!((wish.before.base_block, wish.after.base_block), (6, 8));
        assert!(wish.has_unresolved_output_change());
    }

    #[test]
    fn upgrade_preview_preserves_additive_changes_and_current_cost() {
        let mut card = CombatCard::for_scenario(CardId::PerfectedStrike, 1);
        let preview = card_owned_upgrade_output_v1(&card).unwrap();
        assert_eq!(preview.unconditional_damage_delta(), Some(0));
        assert!(preview.has_unresolved_output_change());
        assert!(matches!(
            preview.after.damage.delivery,
            Some(CardBaseDamageDeliveryV1::Attack {
                amount: CardAttackAmountV1::BasePlusStrikeCards { per_card: 3 },
                ..
            })
        ));
        card.id = CardId::Corruption;
        card.cost_modifier = -3;
        card.cost_for_turn = Some(0);
        let preview = card_owned_upgrade_output_v1(&card).unwrap();
        assert_eq!(
            (preview.before.energy_cost, preview.after.energy_cost),
            (0, 2)
        );
        assert!(!preview.has_unresolved_output_change());
    }

    #[test]
    fn persistent_overrides_do_not_erase_request_conditions_or_use_turn_caches() {
        let mut conditional = CombatCard::for_scenario(CardId::EscapePlan, 1);
        conditional.base_block_override = Some(99);
        assert_eq!(
            card_owned_base_output_v1(&conditional).unconditional_block,
            None
        );
        let mut attack = CombatCard::for_scenario(CardId::TwinStrike, 2);
        attack.base_damage_override = Some(13);
        attack.cost_modifier = 2;
        attack.cost_for_turn = Some(0);
        attack.free_to_play_once = true;
        attack.base_damage_mut = 100;
        let output = card_owned_base_output_v1(&attack);
        assert_eq!(output.damage.unconditional_targeted_base_total(), Some(26));
        assert_eq!(output.energy_cost, 3);
        attack.id = CardId::Wish;
        let output = card_owned_base_output_v1(&attack);
        assert!(!output.damage.has_reviewed_attack());
        assert_eq!(output.damage.unconditional_base_total(), None);
        assert_eq!(output.unconditional_block, None);
    }
}
