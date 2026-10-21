use serde::{Deserialize, Serialize};

use crate::ai::card_semantics_v1::{potion_acquisition_traits_v1, PotionAcquisitionTraitV1};
use crate::ai::strategy::deck_role_inventory::DeckRoleInventory;
use crate::content::cards::get_card_definition;
use crate::content::potions::PotionId;
use crate::state::run::RunState;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PotionRetentionBandV1 {
    Unclassified,
    Flexible,
    Grounded,
    Critical,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PotionDeckSupportV1 {
    XCostEnergySink,
    CostlyCardEnergySink,
    ExhaustPayoff,
    BroadExhaustNeed,
    StrengthPayoff,
    IntrinsicTargetedAccess,
    IntrinsicActionAmplifier,
    DeathInsurance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionRetentionEvidenceV1 {
    pub potion: PotionId,
    pub traits: Vec<PotionAcquisitionTraitV1>,
    pub deck_support: Vec<PotionDeckSupportV1>,
    pub band: PotionRetentionBandV1,
}

pub fn assess_potion_retention_v1(
    run_state: &RunState,
    potion: PotionId,
) -> PotionRetentionEvidenceV1 {
    let traits = potion_acquisition_traits_v1(potion);
    let roles = DeckRoleInventory::from_deck(&run_state.master_deck);
    let costly_cards = run_state
        .master_deck
        .iter()
        .filter(|card| get_card_definition(card.id).cost >= 2)
        .count();
    let mut deck_support = Vec::new();
    if traits.contains(&PotionAcquisitionTraitV1::EnergyBurst) {
        if roles.x_cost_payoff_units > 0 {
            deck_support.push(PotionDeckSupportV1::XCostEnergySink);
        }
        if costly_cards >= 3 {
            deck_support.push(PotionDeckSupportV1::CostlyCardEnergySink);
        }
    }
    if traits.contains(&PotionAcquisitionTraitV1::ExhaustControl) {
        if roles.exhaust_payoff_units > 0 {
            deck_support.push(PotionDeckSupportV1::ExhaustPayoff);
        }
        if roles.broad_exhaust_units == 0 && roles.non_attack_units >= 4 {
            deck_support.push(PotionDeckSupportV1::BroadExhaustNeed);
        }
    }
    if traits.contains(&PotionAcquisitionTraitV1::StrengthGain) && roles.strength_payoff_units > 0 {
        deck_support.push(PotionDeckSupportV1::StrengthPayoff);
    }
    if traits.contains(&PotionAcquisitionTraitV1::CardAccess) {
        deck_support.push(PotionDeckSupportV1::IntrinsicTargetedAccess);
    }
    if traits.contains(&PotionAcquisitionTraitV1::ActionAmplifier) {
        deck_support.push(PotionDeckSupportV1::IntrinsicActionAmplifier);
    }
    if traits.contains(&PotionAcquisitionTraitV1::DeathInsurance) {
        deck_support.push(PotionDeckSupportV1::DeathInsurance);
    }

    deck_support.sort();
    deck_support.dedup();
    let band = if traits.contains(&PotionAcquisitionTraitV1::DeathInsurance)
        && run_state.current_hp * 2 <= run_state.max_hp
    {
        PotionRetentionBandV1::Critical
    } else if !deck_support.is_empty() {
        PotionRetentionBandV1::Grounded
    } else if !traits.is_empty() {
        PotionRetentionBandV1::Flexible
    } else {
        PotionRetentionBandV1::Unclassified
    };
    PotionRetentionEvidenceV1 {
        potion,
        traits,
        deck_support,
        band,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::runtime::combat::CombatCard;

    #[test]
    fn exhaust_energy_and_targeted_access_are_grounded_but_discovery_remains_flexible() {
        let mut run = RunState::new(0, 0, false, "Ironclad");
        run.master_deck = [
            CardId::DarkEmbrace,
            CardId::Impervious,
            CardId::FiendFire,
            CardId::Clothesline,
            CardId::Whirlwind,
        ]
        .into_iter()
        .enumerate()
        .map(|(uuid, card)| CombatCard::for_scenario(card, uuid as u32))
        .collect();

        assert_eq!(
            assess_potion_retention_v1(&run, PotionId::EnergyPotion).band,
            PotionRetentionBandV1::Grounded
        );
        assert_eq!(
            assess_potion_retention_v1(&run, PotionId::LiquidMemories).band,
            PotionRetentionBandV1::Grounded
        );
        assert_eq!(
            assess_potion_retention_v1(&run, PotionId::Elixir).band,
            PotionRetentionBandV1::Grounded
        );
        assert_eq!(
            assess_potion_retention_v1(&run, PotionId::ColorlessPotion).band,
            PotionRetentionBandV1::Flexible
        );
    }
}
