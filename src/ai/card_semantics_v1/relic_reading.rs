//! Reviewed relic effects for public reading, without policy roles or scores.
use super::{
    relic_acquisition_mechanics_facts_v1, DrawnCardCostRandomizationFactsV1,
    RelicCombatVictoryHealingFactsV1, RelicPotionAcquisitionRuleFactsV1,
};
use crate::content::relics::{
    mechanics::{
        relic_campfire_restriction_v1, relic_end_turn_discard_rule_v1, RelicBattleStartPowerV1,
        RelicCampfireRestrictionV1, RelicEndTurnDiscardRuleV1, RelicGoldGainRuleV1,
        RelicShuffleEnergyV1,
    },
    RelicId,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelicAcquisitionReadingV1 {
    pub relic: RelicId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy_per_turn_delta: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub campfire_restriction: Option<RelicCampfireRestrictionV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub potion_acquisition_rule: Option<RelicPotionAcquisitionRuleFactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_play_limit_per_turn: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_cards_drawn_each_turn: Option<i32>,
    /// Pre-battle application is a request. Randomization requires Confusion
    /// to have been successfully applied and still be present at draw time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_battle_confusion: Option<DrawnCardCostRandomizationFactsV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_turn_discard_rule: Option<RelicEndTurnDiscardRuleV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gold_gain_rule: Option<RelicGoldGainRuleV1>,
    /// Immediate base healing per actual positive gold gain, before caps/modifiers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gold_gain_healing: Option<i32>,
    /// Changes choices within generated colored/colorless reward rows, not
    /// arbitrary card selection screens or the number of reward rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_reward_choice_delta: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_normal_monster_card_reward_rows: Option<u8>,
    pub battle_start_power: Option<RelicBattleStartPowerV1>,
    pub shuffle_energy: Option<RelicShuffleEnergyV1>,
    pub combat_victory_healing: Option<RelicCombatVictoryHealingFactsV1>,
}

pub fn relic_acquisition_reading_v1(relic: RelicId) -> RelicAcquisitionReadingV1 {
    let facts = relic_acquisition_mechanics_facts_v1(relic);
    RelicAcquisitionReadingV1 {
        relic,
        energy_per_turn_delta: facts.energy_per_turn_delta,
        campfire_restriction: relic_campfire_restriction_v1(relic),
        potion_acquisition_rule: facts.potion_acquisition_rule,
        card_play_limit_per_turn: (relic == RelicId::VelvetChoker)
            .then_some(crate::content::relics::velvet_choker::CARD_PLAY_LIMIT),
        additional_cards_drawn_each_turn: facts.additional_cards_drawn_each_turn,
        pre_battle_confusion: facts.drawn_card_cost_randomization,
        end_turn_discard_rule: relic_end_turn_discard_rule_v1(relic),
        gold_gain_rule: facts.gold_gain_rule,
        gold_gain_healing: crate::content::relics::mechanics::relic_gold_gain_healing_v1(relic),
        card_reward_choice_delta:
            crate::content::relics::mechanics::relic_card_reward_choice_delta_v1(relic),
        additional_normal_monster_card_reward_rows:
            crate::content::relics::mechanics::relic_additional_normal_monster_card_reward_rows_v1(
                relic,
            ),
        battle_start_power: facts.battle_start_power,
        shuffle_energy: facts.shuffle_energy,
        combat_victory_healing: facts.combat_victory_healing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relic_reading_does_not_convert_conditional_or_delayed_effects_to_immediate_value() {
        let bronze = relic_acquisition_reading_v1(RelicId::BronzeScales);
        assert_eq!(bronze.battle_start_power.unwrap().amount, 3);
        assert!(bronze.combat_victory_healing.is_none());
        let sundial = relic_acquisition_reading_v1(RelicId::Sundial);
        assert_eq!(sundial.shuffle_energy.unwrap().energy, 2);
        assert!(sundial.battle_start_power.is_none());
        let blood = relic_acquisition_reading_v1(RelicId::BurningBlood);
        assert_eq!(blood.combat_victory_healing.unwrap().base_hp, 6);
        let unknown = relic_acquisition_reading_v1(RelicId::Anchor);
        assert!(
            unknown.battle_start_power.is_none()
                && unknown.shuffle_energy.is_none()
                && unknown.combat_victory_healing.is_none()
        );
    }
}
