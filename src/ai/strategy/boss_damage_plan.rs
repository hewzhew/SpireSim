use serde::Serialize;

use crate::ai::block_plan_profile_v1::block_plan_profile_from_deck_v1;
use crate::ai::strategy::deck_role_inventory::DeckRoleInventory;
use crate::content::cards::CardId;
use crate::runtime::combat::CombatCard;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BossDamagePlanReadinessV1 {
    #[default]
    Missing,
    Fragment,
    Support,
    Engine,
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BossDamagePlanEngineReliabilityV1 {
    #[default]
    None,
    Fragile,
    Established,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BossDamagePlanKindV1 {
    RepeatableStrengthGrowth,
    PersistentStrength,
    StrengthMultiplication,
    BlockConversion,
    ExhaustCompression,
    RitualDagger,
    StrengthPayoff,
}

impl BossDamagePlanKindV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RepeatableStrengthGrowth => "repeatable_strength_growth",
            Self::PersistentStrength => "persistent_strength",
            Self::StrengthMultiplication => "strength_multiplication",
            Self::BlockConversion => "block_conversion",
            Self::ExhaustCompression => "exhaust_compression",
            Self::RitualDagger => "ritual_dagger",
            Self::StrengthPayoff => "strength_payoff",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct BossDamagePlanFactsV1 {
    pub readiness: BossDamagePlanReadinessV1,
    pub engine_reliability: BossDamagePlanEngineReliabilityV1,
    pub engines: Vec<BossDamagePlanKindV1>,
    pub supports: Vec<BossDamagePlanKindV1>,
    pub fragments: Vec<BossDamagePlanKindV1>,
}

impl BossDamagePlanFactsV1 {
    fn add_engine(
        &mut self,
        kind: BossDamagePlanKindV1,
        reliability: BossDamagePlanEngineReliabilityV1,
    ) {
        self.engines.push(kind);
        self.engine_reliability = self.engine_reliability.max(reliability);
    }

    pub fn active_plan_labels(&self) -> Vec<String> {
        let active = match self.readiness {
            BossDamagePlanReadinessV1::Engine => &self.engines,
            BossDamagePlanReadinessV1::Support => &self.supports,
            BossDamagePlanReadinessV1::Fragment | BossDamagePlanReadinessV1::Missing => {
                return Vec::new();
            }
        };
        active
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect()
    }
}

pub fn assess_boss_damage_plan_v1(deck: &[CombatCard]) -> BossDamagePlanFactsV1 {
    let inventory = DeckRoleInventory::from_deck(deck);
    let block_plan = block_plan_profile_from_deck_v1(deck);
    let has = |id| deck.iter().any(|card| card.id == id);
    let has_demon_form = has(CardId::DemonForm);
    let has_spot_weakness = has(CardId::SpotWeakness);
    let has_rupture = has(CardId::Rupture);
    let has_repeatable_rupture = has_rupture && inventory.repeatable_self_damage_supply;
    let has_repeatable_strength_growth =
        has_demon_form || has_spot_weakness || has_repeatable_rupture;
    let has_usable_strength_source = inventory.strength_source_units > 0 || has_spot_weakness;
    let has_strength_multiplier = inventory.strength_multiplier_units > 0;
    let has_block_conversion = inventory.block_payoff_units > 0;
    let has_exhaust_compression = has(CardId::DarkEmbrace);
    let has_exhaust_fuel = inventory.exhaust_stream_units > 0 || inventory.corruption_units > 0;

    let mut facts = BossDamagePlanFactsV1::default();

    if has_repeatable_strength_growth {
        facts.add_engine(
            BossDamagePlanKindV1::RepeatableStrengthGrowth,
            if has_demon_form {
                BossDamagePlanEngineReliabilityV1::Established
            } else {
                BossDamagePlanEngineReliabilityV1::Fragile
            },
        );
    } else if has_rupture {
        facts
            .fragments
            .push(BossDamagePlanKindV1::RepeatableStrengthGrowth);
    }

    if has_strength_multiplier {
        if has_usable_strength_source {
            facts.add_engine(
                BossDamagePlanKindV1::StrengthMultiplication,
                if inventory.strength_source_units >= 2 {
                    BossDamagePlanEngineReliabilityV1::Established
                } else {
                    BossDamagePlanEngineReliabilityV1::Fragile
                },
            );
        } else {
            facts
                .fragments
                .push(BossDamagePlanKindV1::StrengthMultiplication);
        }
    }

    if has_usable_strength_source && !has_repeatable_strength_growth && !has_strength_multiplier {
        facts
            .supports
            .push(BossDamagePlanKindV1::PersistentStrength);
    }

    if has_block_conversion {
        if block_plan.plain_block_cards >= 3 {
            facts.add_engine(
                BossDamagePlanKindV1::BlockConversion,
                BossDamagePlanEngineReliabilityV1::Established,
            );
        } else {
            facts.fragments.push(BossDamagePlanKindV1::BlockConversion);
        }
    }

    if has_exhaust_compression {
        if has_exhaust_fuel {
            facts
                .supports
                .push(BossDamagePlanKindV1::ExhaustCompression);
        } else {
            facts
                .fragments
                .push(BossDamagePlanKindV1::ExhaustCompression);
        }
    }

    if has(CardId::RitualDagger) {
        facts.supports.push(BossDamagePlanKindV1::RitualDagger);
    }

    if inventory.strength_payoff_units > 0 && !has_usable_strength_source {
        facts.fragments.push(BossDamagePlanKindV1::StrengthPayoff);
    }

    facts.readiness = if !facts.engines.is_empty() {
        BossDamagePlanReadinessV1::Engine
    } else if !facts.supports.is_empty() {
        BossDamagePlanReadinessV1::Support
    } else if !facts.fragments.is_empty() {
        BossDamagePlanReadinessV1::Fragment
    } else {
        BossDamagePlanReadinessV1::Missing
    };
    facts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(ids: &[CardId]) -> Vec<CombatCard> {
        ids.iter()
            .enumerate()
            .map(|(index, id)| CombatCard::for_scenario(*id, index as u32 + 1))
            .collect()
    }

    #[test]
    fn demon_form_is_a_repeatable_strength_engine() {
        let facts = assess_boss_damage_plan_v1(&deck(&[CardId::DemonForm, CardId::Strike]));

        assert_eq!(facts.readiness, BossDamagePlanReadinessV1::Engine);
        assert_eq!(
            facts.engine_reliability,
            BossDamagePlanEngineReliabilityV1::Established
        );
        assert_eq!(
            facts.engines,
            vec![BossDamagePlanKindV1::RepeatableStrengthGrowth]
        );
    }

    #[test]
    fn inflame_and_limit_break_form_a_strength_multiplication_engine() {
        let facts = assess_boss_damage_plan_v1(&deck(&[
            CardId::Inflame,
            CardId::LimitBreak,
            CardId::Strike,
        ]));

        assert_eq!(facts.readiness, BossDamagePlanReadinessV1::Engine);
        assert_eq!(
            facts.engine_reliability,
            BossDamagePlanEngineReliabilityV1::Fragile
        );
        assert!(facts
            .engines
            .contains(&BossDamagePlanKindV1::StrengthMultiplication));
    }

    #[test]
    fn body_slam_with_three_block_sources_is_a_block_conversion_engine() {
        let facts = assess_boss_damage_plan_v1(&deck(&[
            CardId::BodySlam,
            CardId::Defend,
            CardId::ShrugItOff,
            CardId::FlameBarrier,
        ]));

        assert_eq!(facts.readiness, BossDamagePlanReadinessV1::Engine);
        assert_eq!(
            facts.engine_reliability,
            BossDamagePlanEngineReliabilityV1::Established
        );
        assert!(facts
            .engines
            .contains(&BossDamagePlanKindV1::BlockConversion));
    }

    #[test]
    fn dark_embrace_and_true_grit_are_support_not_a_damage_engine() {
        let facts = assess_boss_damage_plan_v1(&deck(&[CardId::DarkEmbrace, CardId::TrueGrit]));

        assert_eq!(facts.readiness, BossDamagePlanReadinessV1::Support);
        assert_eq!(
            facts.supports,
            vec![BossDamagePlanKindV1::ExhaustCompression]
        );
        assert!(facts.engines.is_empty());
    }

    #[test]
    fn limit_break_and_pummel_without_strength_are_only_fragments() {
        let facts = assess_boss_damage_plan_v1(&deck(&[CardId::LimitBreak, CardId::Pummel]));

        assert_eq!(facts.readiness, BossDamagePlanReadinessV1::Fragment);
        assert!(facts
            .fragments
            .contains(&BossDamagePlanKindV1::StrengthMultiplication));
        assert!(facts
            .fragments
            .contains(&BossDamagePlanKindV1::StrengthPayoff));
    }

    #[test]
    fn spot_weakness_is_conditional_but_repeatable_strength_growth() {
        let facts = assess_boss_damage_plan_v1(&deck(&[CardId::SpotWeakness]));

        assert_eq!(facts.readiness, BossDamagePlanReadinessV1::Engine);
        assert_eq!(
            facts.engine_reliability,
            BossDamagePlanEngineReliabilityV1::Fragile
        );
        assert!(facts
            .engines
            .contains(&BossDamagePlanKindV1::RepeatableStrengthGrowth));
    }

    #[test]
    fn rupture_requires_a_repeatable_self_damage_stream() {
        let one_shot = assess_boss_damage_plan_v1(&deck(&[CardId::Rupture, CardId::Offering]));
        let repeatable =
            assess_boss_damage_plan_v1(&deck(&[CardId::Rupture, CardId::Bloodletting]));

        assert_eq!(one_shot.readiness, BossDamagePlanReadinessV1::Fragment);
        assert_eq!(repeatable.readiness, BossDamagePlanReadinessV1::Engine);
        assert_eq!(
            repeatable.engine_reliability,
            BossDamagePlanEngineReliabilityV1::Fragile
        );
    }

    #[test]
    fn second_stable_strength_source_establishes_multiplier_reliability() {
        let facts = assess_boss_damage_plan_v1(&deck(&[
            CardId::Inflame,
            CardId::Inflame,
            CardId::LimitBreak,
        ]));

        assert_eq!(facts.readiness, BossDamagePlanReadinessV1::Engine);
        assert_eq!(
            facts.engine_reliability,
            BossDamagePlanEngineReliabilityV1::Established
        );
    }
}
