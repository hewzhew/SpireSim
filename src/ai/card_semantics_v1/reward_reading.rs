//! A bounded, strategy-neutral reading of obtainable card definitions.
//!
//! This is deliberately not a complete card description or a combat prediction.
//! Base numbers may be conditional, replaced dynamically, or used repeatedly.
//! Only named reviewed effects assert a request or schedule; absence is unknown.

use serde::{Deserialize, Serialize};

use super::{
    card_acquisition_mechanics_facts_v1, CardPlayTargetV1, CardRewardFactsV1,
    CardRewardStatusInjectionV1,
};
use crate::content::cards::mechanics::{
    CardBaseBlockDeliveryV1, CardBaseDamageDeliveryV1, CardEndOfTurnBlockEachTurnV1,
    CardEndOfTurnHpLossAndDamageV1, CardEnergyGainV1, CardPlayerStrengthDeliveryV1,
    CardSelfHpLossOnPlayV1, CombatUpgradePersistenceV1, CombatUpgradeScopeV1,
    EnemyStrengthRestorationV1,
};
use crate::content::cards::{CardId, CardRarity, CardType};
use crate::state::rewards::RewardCard;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardAcquisitionReadingV1 {
    pub current: CardDefinitionReadingV1,
    /// The next legal upgrade within the simulator's representable count.
    /// These are card-definition facts, without acquisition/relic modifiers.
    pub next_upgrade: Option<CardDefinitionReadingV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardDefinitionReadingV1 {
    pub card: CardId,
    pub upgrades: u8,
    pub card_type: CardType,
    pub rarity: CardRarity,
    pub base_cost: i8,
    pub play_target: CardPlayTargetV1,
    pub exhausts: bool,
    pub ethereal: bool,
    pub innate: bool,
    /// Definition fields only, never unconditional damage/block or totals.
    pub definition_numbers: CardDefinitionNumbersV1,
    pub reviewed_effects: CardReviewedReadingEffectsV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardDefinitionNumbersV1 {
    pub damage: i32,
    pub block: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardReviewedReadingEffectsV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_upgrade_scope: Option<CombatUpgradeScopeV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_upgrade_persistence: Option<CombatUpgradePersistenceV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_damage_delivery: Option<CardBaseDamageDeliveryV1>,
    /// Missing in older readings or outside reviewed coverage: Unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_block_delivery: Option<CardBaseBlockDeliveryV1>,
    /// After successful play, before No Draw, hand caps, or follow-up discard.
    /// Missing does not rule out conditional or triggered draw.
    pub unconditional_draw_request: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaust_draw_power: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhaust_block_power: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand_topdeck_after_draw: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enemy_debuffs_in_monster_order:
        Option<Vec<crate::content::cards::mechanics::CardEnemyDebuffRequestV1>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hp_loss_strength_power: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw_pile_skill_to_hand: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_to_discard_after_damage: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_of_turn_hp_loss_and_damage: Option<CardEndOfTurnHpLossAndDamageV1>,
    /// Per affected enemy, before prevention and target-specific changes.
    pub enemy_strength_reduction: Option<CardEnemyStrengthReductionReadingV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_strength_gain: Option<CardPlayerStrengthGainReadingV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_gain: Option<CardEnergyGainV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_vulnerable_on_play: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status_injections_on_play: Vec<CardRewardStatusInjectionV1>,
    pub self_hp_loss_on_play: Option<CardSelfHpLossOnPlayV1>,
    pub end_of_turn_block_each_turn: Option<CardEndOfTurnBlockEachTurnV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardEnemyStrengthReductionReadingV1 {
    pub amount: i32,
    pub restoration: EnemyStrengthRestorationV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardPlayerStrengthGainReadingV1 {
    pub amount: i32,
    pub delivery: CardPlayerStrengthDeliveryV1,
}

impl From<CardRewardFactsV1> for CardDefinitionReadingV1 {
    fn from(facts: CardRewardFactsV1) -> Self {
        Self {
            card: facts.card,
            upgrades: facts.upgrades,
            card_type: facts.card_type,
            rarity: facts.rarity,
            base_cost: facts.cost,
            play_target: facts.play_target,
            exhausts: facts.exhausts,
            ethereal: facts.ethereal,
            innate: facts.innate,
            definition_numbers: CardDefinitionNumbersV1 {
                damage: facts.damage.damage_per_hit,
                block: facts.block,
            },
            reviewed_effects: CardReviewedReadingEffectsV1 {
                combat_upgrade_scope: facts.combat_upgrade_scope,
                combat_upgrade_persistence: facts.combat_upgrade_persistence,
                base_damage_delivery: facts.damage.delivery,
                base_block_delivery: facts.base_block_delivery,
                unconditional_draw_request: (facts.draw_cards > 0).then_some(facts.draw_cards),
                exhaust_draw_power: facts.exhaust_draw_power,
                exhaust_block_power: facts.exhaust_block_power,
                hand_topdeck_after_draw: facts.hand_topdeck_after_draw,
                enemy_debuffs_in_monster_order: facts.enemy_debuffs_in_monster_order,
                hp_loss_strength_power: facts.hp_loss_strength_power,
                draw_pile_skill_to_hand: facts.draw_pile_skill_to_hand,
                copy_to_discard_after_damage: facts.copy_to_discard_after_damage,
                end_of_turn_hp_loss_and_damage: facts.end_of_turn_hp_loss_and_damage,
                enemy_strength_reduction: facts.enemy_strength_restoration.map(|restoration| {
                    CardEnemyStrengthReductionReadingV1 {
                        amount: facts.enemy_strength_down,
                        restoration,
                    }
                }),
                self_hp_loss_on_play: facts.self_hp_loss_on_play,
                energy_gain: facts.energy_gain_rule,
                status_injections_on_play: facts.status_injections,
                player_vulnerable_on_play: (facts.player_vulnerable > 0)
                    .then_some(facts.player_vulnerable),
                player_strength_gain: facts.strength_gain_delivery.map(|delivery| {
                    CardPlayerStrengthGainReadingV1 {
                        amount: facts.strength_gain,
                        delivery,
                    }
                }),
                end_of_turn_block_each_turn: facts.end_of_turn_block_each_turn,
            },
        }
    }
}

pub fn card_acquisition_reading_v1(card: &RewardCard) -> CardAcquisitionReadingV1 {
    let facts = card_acquisition_mechanics_facts_v1(card);
    CardAcquisitionReadingV1 {
        current: facts.current.into(),
        next_upgrade: facts.upgraded.map(Into::into),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secondary_reading_preserves_upgrade_scope_and_does_not_invent_immediate_payoff() {
        use crate::content::cards::mechanics::{CardEnemyDebuffRequestV1, CardEnemyDebuffV1};
        let shockwave = card_acquisition_reading_v1(&RewardCard::new(CardId::Shockwave, 0));
        for (definition, amount) in [(shockwave.current, 3), (shockwave.next_upgrade.unwrap(), 5)] {
            assert_eq!(
                definition.reviewed_effects.enemy_debuffs_in_monster_order,
                Some(vec![
                    CardEnemyDebuffRequestV1 {
                        debuff: CardEnemyDebuffV1::Weak,
                        amount
                    },
                    CardEnemyDebuffRequestV1 {
                        debuff: CardEnemyDebuffV1::Vulnerable,
                        amount
                    }
                ])
            );
            assert_eq!(definition.play_target, CardPlayTargetV1::AllEnemies);
            assert!(definition.exhausts);
        }
        let rupture = card_acquisition_reading_v1(&RewardCard::new(CardId::Rupture, 0));
        for (definition, amount) in [(rupture.current, 1), (rupture.next_upgrade.unwrap(), 2)] {
            assert_eq!(
                definition.reviewed_effects.hp_loss_strength_power,
                Some(amount)
            );
            assert!(definition.reviewed_effects.player_strength_gain.is_none());
            assert!(definition.reviewed_effects.self_hp_loss_on_play.is_none());
        }
        let secret = card_acquisition_reading_v1(&RewardCard::new(CardId::SecretTechnique, 0));
        for (definition, exhaust) in [
            (secret.current, true),
            (secret.next_upgrade.unwrap(), false),
        ] {
            assert_eq!(definition.reviewed_effects.draw_pile_skill_to_hand, Some(1));
            assert_eq!(definition.exhausts, exhaust);
            assert!(definition
                .reviewed_effects
                .unconditional_draw_request
                .is_none());
        }
        let anger = card_acquisition_reading_v1(&RewardCard::new(CardId::Anger, 0));
        for (definition, damage) in [(anger.current, 6), (anger.next_upgrade.unwrap(), 8)] {
            assert_eq!(
                definition.reviewed_effects.copy_to_discard_after_damage,
                Some(1)
            );
            assert_eq!(definition.definition_numbers.damage, damage);
        }
        let other = card_acquisition_reading_v1(&RewardCard::new(CardId::Seek, 0))
            .current
            .reviewed_effects;
        assert!(other.draw_pile_skill_to_hand.is_none());
        assert!(other.enemy_debuffs_in_monster_order.is_none());
        assert!(other.hp_loss_strength_power.is_none());
        assert!(other.copy_to_discard_after_damage.is_none());
    }

    #[test]
    fn delayed_reward_comparison_keeps_costs_triggers_and_upgrade_effects() {
        let combust = card_acquisition_reading_v1(&RewardCard::new(CardId::Combust, 0));
        for (definition, damage) in [(combust.current, 5), (combust.next_upgrade.unwrap(), 7)] {
            assert_eq!(
                definition.reviewed_effects.end_of_turn_hp_loss_and_damage,
                Some(CardEndOfTurnHpLossAndDamageV1 {
                    damage_per_enemy_per_application: damage,
                    hp_loss_per_application: 1,
                })
            );
            assert_eq!(definition.definition_numbers.damage, 0);
            assert!(definition.reviewed_effects.self_hp_loss_on_play.is_none());
        }
        let embrace = card_acquisition_reading_v1(&RewardCard::new(CardId::DarkEmbrace, 0));
        assert_eq!(embrace.current.base_cost, 2);
        assert_eq!(embrace.current.reviewed_effects.exhaust_draw_power, Some(1));
        assert_eq!(
            embrace.current.reviewed_effects.unconditional_draw_request,
            None
        );
        let upgraded = embrace.next_upgrade.unwrap();
        assert_eq!(upgraded.base_cost, 1);
        assert_eq!(upgraded.reviewed_effects.exhaust_draw_power, Some(1));
        let pain = card_acquisition_reading_v1(&RewardCard::new(CardId::FeelNoPain, 0));
        for (definition, block) in [(pain.current, 3), (pain.next_upgrade.unwrap(), 4)] {
            assert_eq!(definition.base_cost, 1);
            assert_eq!(definition.definition_numbers.block, 0);
            assert_eq!(definition.reviewed_effects.exhaust_block_power, Some(block));
            assert!(definition.reviewed_effects.exhaust_draw_power.is_none());
        }
        assert!(upgraded.reviewed_effects.exhaust_block_power.is_none());
        let warcry = card_acquisition_reading_v1(&RewardCard::new(CardId::Warcry, 0));
        for (definition, draw) in [(warcry.current, 1), (warcry.next_upgrade.unwrap(), 2)] {
            assert_eq!(
                definition.reviewed_effects.unconditional_draw_request,
                Some(draw)
            );
            assert_eq!(definition.reviewed_effects.hand_topdeck_after_draw, Some(1));
            assert!(definition.exhausts);
        }
        let facts = super::super::card_reward_facts_v1(&RewardCard::new(CardId::Combust, 0));
        assert_eq!(facts.hp_loss_each_turn, 1);
        assert_eq!(facts.draw_cards, 0);
    }

    #[test]
    fn energy_reading_preserves_costs_schedules_and_upgrade_changes() {
        use crate::content::cards::mechanics::CardEnergyDeliveryV1::*;
        for (id, base, upgraded, delivery) in [
            (CardId::Offering, 2, 2, OnPlay),
            (CardId::SeeingRed, 2, 2, OnPlay),
            (CardId::Bloodletting, 2, 3, OnPlay),
            (CardId::Berserk, 1, 1, EachPlayerTurnStart),
            (CardId::Sentinel, 2, 3, OnExhaust),
            (CardId::Dropkick, 1, 1, IfTargetVulnerableAtResolution),
            (CardId::Adrenaline, 1, 2, OnPlay),
            (CardId::Outmaneuver, 2, 3, NextPlayerEnergyRecharge),
            (CardId::Turbo, 2, 3, OnPlay),
        ] {
            let reading = card_acquisition_reading_v1(&RewardCard::new(id, 0));
            for (definition, amount) in [
                (reading.current, base),
                (reading.next_upgrade.unwrap(), upgraded),
            ] {
                assert_eq!(
                    definition.reviewed_effects.energy_gain,
                    Some(CardEnergyGainV1 { amount, delivery })
                );
                let facts =
                    super::super::card_reward_facts_v1(&RewardCard::new(id, definition.upgrades));
                assert_eq!(
                    facts.energy_gain,
                    if delivery == OnPlay { amount } else { 0 },
                    "scheduled/conditional energy must not become immediate policy credit"
                );
            }
        }
        let red = card_acquisition_reading_v1(&RewardCard::new(CardId::SeeingRed, 0));
        assert_eq!(red.current.base_cost, 1);
        assert_eq!(red.next_upgrade.unwrap().base_cost, 0);
        let berserk = card_acquisition_reading_v1(&RewardCard::new(CardId::Berserk, 0));
        assert_eq!(
            berserk.current.reviewed_effects.player_vulnerable_on_play,
            Some(2)
        );
        assert_eq!(
            berserk
                .next_upgrade
                .unwrap()
                .reviewed_effects
                .player_vulnerable_on_play,
            Some(1)
        );
        let offering = card_acquisition_reading_v1(&RewardCard::new(CardId::Offering, 0));
        assert_eq!(
            offering
                .current
                .reviewed_effects
                .self_hp_loss_on_play
                .unwrap()
                .base_hp_loss,
            6
        );
        assert_eq!(
            offering.current.reviewed_effects.unconditional_draw_request,
            Some(3)
        );
        assert_eq!(
            offering
                .next_upgrade
                .unwrap()
                .reviewed_effects
                .unconditional_draw_request,
            Some(5)
        );
        for id in [CardId::DoubleEnergy, CardId::Fission, CardId::Concentrate] {
            assert!(card_acquisition_reading_v1(&RewardCard::new(id, 0))
                .current
                .reviewed_effects
                .energy_gain
                .is_none());
        }
    }

    #[test]
    fn strength_reading_preserves_delivery_and_upgrade_instead_of_flat_gain() {
        use CardPlayerStrengthDeliveryV1::*;
        for (id, base, upgraded, delivery) in [
            (
                CardId::SpotWeakness,
                3,
                4,
                IfTargetIntentBaseDamageNonnegativeAtResolution,
            ),
            (CardId::Inflame, 2, 3, OnPlay),
            (CardId::Flex, 2, 4, OnPlayWithSeparateLoseStrength),
            (CardId::DemonForm, 2, 3, EachPlayerTurnStartPostDraw),
        ] {
            let reading = card_acquisition_reading_v1(&RewardCard::new(id, 0));
            for (facts, amount) in [
                (reading.current, base),
                (reading.next_upgrade.unwrap(), upgraded),
            ] {
                assert_eq!(
                    facts.reviewed_effects.player_strength_gain,
                    Some(CardPlayerStrengthGainReadingV1 { amount, delivery })
                );
            }
        }
        // Multiplication and triggered Strength have not been expressed by
        // this delivery vocabulary; their absence must not become flat gain.
        for id in [CardId::LimitBreak, CardId::Rupture, CardId::Strike] {
            assert!(card_acquisition_reading_v1(&RewardCard::new(id, 0))
                .current
                .reviewed_effects
                .player_strength_gain
                .is_none());
        }
    }

    #[test]
    fn reading_retains_reviewed_timing_and_next_legal_upgrade() {
        let shackles = card_acquisition_reading_v1(&RewardCard::new(CardId::DarkShackles, 0));
        let strength = shackles
            .current
            .reviewed_effects
            .enemy_strength_reduction
            .unwrap();
        assert_eq!(strength.amount, 9);
        assert_eq!(
            strength.restoration,
            EnemyStrengthRestorationV1::ShackledAtOwnerTurnEnd
        );
        assert_eq!(
            shackles
                .next_upgrade
                .unwrap()
                .reviewed_effects
                .enemy_strength_reduction
                .unwrap()
                .amount,
            15
        );

        let breath = card_acquisition_reading_v1(&RewardCard::new(CardId::DeepBreath, 0));
        assert_eq!(
            breath.current.reviewed_effects.unconditional_draw_request,
            Some(1)
        );
        assert_eq!(
            breath
                .next_upgrade
                .unwrap()
                .reviewed_effects
                .unconditional_draw_request,
            Some(2)
        );

        let blow = card_acquisition_reading_v1(&RewardCard::new(CardId::SearingBlow, 5));
        assert_eq!(blow.current.definition_numbers.damage, 42);
        assert_eq!(blow.next_upgrade.unwrap().definition_numbers.damage, 51);
        assert!(
            card_acquisition_reading_v1(&RewardCard::new(CardId::Wound, 0))
                .next_upgrade
                .is_none()
        );
    }

    #[test]
    fn definition_numbers_and_missing_requests_do_not_claim_complete_effects() {
        let escape = card_acquisition_reading_v1(&RewardCard::new(CardId::EscapePlan, 0));
        // The block definition exists, but this projection does not say that
        // playing Escape Plan unconditionally grants Block.
        assert_eq!(escape.current.definition_numbers.block, 3);
        assert_eq!(
            escape.current.reviewed_effects.unconditional_draw_request,
            Some(1)
        );
        let slam = card_acquisition_reading_v1(&RewardCard::new(CardId::BodySlam, 0));
        assert_eq!(slam.current.definition_numbers.damage, 0);
        for id in [CardId::Dropkick, CardId::DarkEmbrace] {
            let reading = card_acquisition_reading_v1(&RewardCard::new(id, 0));
            assert_eq!(
                reading.current.reviewed_effects.unconditional_draw_request,
                None
            );
        }
    }
}
