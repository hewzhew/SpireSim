//! Partial potion use facts for acquisition and retention decisions. Values use
//! base potency, not a forecast of relics, targets or HP at a future use boundary.

use super::{potion_acquisition_mechanics_facts_v1, PotionCardDiscoveryFactsV1};
use crate::content::potions::{mechanics::ReviewedPotionUseV1, PotionId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionAcquisitionReadingV1 {
    pub potion: PotionId,
    pub requires_target: bool,
    /// Basic out-of-combat rule; We Meet Again and current owner legality still apply.
    pub permits_out_of_combat_use: bool,
    pub reviewed_use: Option<ReviewedPotionUseV1>,
    pub card_discovery: Option<PotionCardDiscoveryFactsV1>,
}

pub fn potion_acquisition_reading_v1(potion: PotionId) -> PotionAcquisitionReadingV1 {
    let facts = potion_acquisition_mechanics_facts_v1(potion);
    PotionAcquisitionReadingV1 {
        potion,
        requires_target: facts.target_required,
        permits_out_of_combat_use: crate::content::potions::potion_can_use_out_of_combat(
            potion, false,
        ),
        reviewed_use: facts.reviewed_use,
        card_discovery: facts.card_discovery,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_keeps_use_conditions_and_does_not_treat_zero_potency_as_no_effect() {
        let blood = potion_acquisition_reading_v1(PotionId::BloodPotion);
        assert_eq!(
            blood.reviewed_use,
            Some(ReviewedPotionUseV1::HealMaxHpPercent { percent: 20 })
        );
        assert!(blood.permits_out_of_combat_use);
        assert!(!blood.requires_target);
        let weak = potion_acquisition_reading_v1(PotionId::WeakenPotion);
        assert!(weak.requires_target && !weak.permits_out_of_combat_use);
        assert_eq!(
            weak.reviewed_use,
            Some(ReviewedPotionUseV1::ApplyEnemyWeak { amount: 3 })
        );
        let elixir = potion_acquisition_reading_v1(PotionId::Elixir);
        assert_eq!(
            elixir.reviewed_use,
            Some(ReviewedPotionUseV1::ChooseHandExhaust)
        );
        let power = potion_acquisition_reading_v1(PotionId::PowerPotion);
        assert_eq!(power.card_discovery.unwrap().offered_choices, 3);
        assert!(power.reviewed_use.is_none());
        let fairy = potion_acquisition_reading_v1(PotionId::FairyPotion);
        assert!(fairy.reviewed_use.is_none() && fairy.card_discovery.is_none());
        assert!(!fairy.permits_out_of_combat_use);
    }
}
