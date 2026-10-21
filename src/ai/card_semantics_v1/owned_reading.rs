//! Detached persistent card instances, without allocating runtime identity.
use serde::{Deserialize, Serialize};

use super::{card_reward_facts_v1, reward_reading::CardDefinitionReadingV1};
use crate::content::cards::{
    mechanics::card_blueprint_persistent_base_numbers_v1, upgrade_card_blueprint_once_java,
};
use crate::runtime::combat::CardBlueprint;
use crate::state::rewards::RewardCard;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardOwnedReadingV1 {
    pub current: CardOwnedStateReadingV1,
    pub next_upgrade: Option<CardOwnedStateReadingV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardOwnedStateReadingV1 {
    pub definition: CardDefinitionReadingV1,
    /// Before combat conditions, powers, temporary free play or turn cost.
    pub persistent_cost: i32,
    pub persistent_damage: i32,
    pub persistent_block: i32,
    /// Card-specific record. Not a generic damage, block or hit-count field.
    pub misc_value: i32,
}

fn read(card: &CardBlueprint) -> CardOwnedStateReadingV1 {
    let numbers = card_blueprint_persistent_base_numbers_v1(card);
    CardOwnedStateReadingV1 {
        definition: card_reward_facts_v1(&RewardCard::new(card.id, card.upgrades)).into(),
        persistent_cost: card.combat_cost_without_turn_override_java(),
        persistent_damage: numbers.damage,
        persistent_block: numbers.block,
        misc_value: card.misc_value,
    }
}

/// Preview only the card's own upgrade, not room/relic effects or legality at
/// the current decision. Uses the same blueprint upgrade as runtime generation.
pub fn card_owned_reading_v1(card: &CardBlueprint) -> CardOwnedReadingV1 {
    let mut upgraded = card.clone();
    let next_upgrade = card
        .upgrades
        .checked_add(1)
        .filter(|_| upgrade_card_blueprint_once_java(&mut upgraded))
        .map(|_| read(&upgraded));
    CardOwnedReadingV1 {
        current: read(card),
        next_upgrade,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::{upgrade_card_once_java, CardId};
    use crate::runtime::combat::CombatCard;

    #[test]
    fn detached_upgrade_matches_materialized_runtime_and_keeps_instance_values() {
        for (id, damage, block, misc, cost, upgrades) in [
            (CardId::Strike, Some(20), None, 0, -1, 0),
            (CardId::Defend, None, Some(17), 0, 0, 0),
            (CardId::Corruption, None, None, 0, -3, 0),
            (CardId::BloodForBlood, None, None, 0, -2, 0),
            (CardId::RitualDagger, None, None, 42, 0, 0),
            (CardId::GeneticAlgorithm, None, None, 31, 0, 0),
            (CardId::SearingBlow, None, None, 0, 0, 5),
        ] {
            let mut card = CombatCard::for_scenario(id, 1);
            card.base_damage_override = damage;
            card.base_block_override = block;
            card.misc_value = misc;
            card.cost_modifier = cost;
            card.upgrades = upgrades;
            card.base_damage_mut = 999;
            card.cost_for_turn = Some(0);
            let blueprint = CardBlueprint::from_stat_equivalent(&card);
            let original = blueprint.clone();
            let reading = card_owned_reading_v1(&blueprint);
            assert_eq!(blueprint, original);
            let expected_before = super::super::card_owned_base_output_v1(&card);
            assert_eq!(
                reading.current.persistent_damage,
                expected_before.damage.damage_per_hit
            );
            assert_eq!(reading.current.persistent_block, expected_before.base_block);
            assert_eq!(reading.current.persistent_cost, expected_before.energy_cost);
            assert!(upgrade_card_once_java(&mut card));
            let expected = super::super::card_owned_base_output_v1(&card);
            let next = reading.next_upgrade.unwrap();
            assert_eq!(
                next.persistent_damage, expected.damage.damage_per_hit,
                "{id:?}"
            );
            assert_eq!(next.persistent_block, expected.base_block, "{id:?}");
            assert_eq!(next.persistent_cost, expected.energy_cost, "{id:?}");
            assert_eq!(next.misc_value, misc);
        }
        let mut corruption = CardBlueprint::fresh(CardId::Corruption);
        corruption.cost_modifier = -3;
        let reading = card_owned_reading_v1(&corruption);
        assert_eq!(reading.current.persistent_cost, 0);
        assert_eq!(reading.next_upgrade.unwrap().persistent_cost, 2);
        let mut strike = CardBlueprint::fresh(CardId::Strike);
        strike.base_damage_override = Some(20);
        assert_eq!(
            card_owned_reading_v1(&strike)
                .next_upgrade
                .unwrap()
                .persistent_damage,
            23
        );
    }

    #[test]
    fn upgrade_eligibility_and_representable_count_do_not_wrap() {
        for id in [CardId::Wound, CardId::Pain] {
            assert!(card_owned_reading_v1(&CardBlueprint::fresh(id))
                .next_upgrade
                .is_none());
        }
        for id in [CardId::Strike, CardId::SearingBlow] {
            let mut card = CardBlueprint::fresh(id);
            card.upgrades = u8::MAX;
            assert!(card_owned_reading_v1(&card).next_upgrade.is_none());
        }
    }
}
