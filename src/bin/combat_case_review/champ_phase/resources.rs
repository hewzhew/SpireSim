use sts_oracle_runtime::content::cards::CardId;
use sts_oracle_runtime::content::potions::PotionId;
use sts_oracle_runtime::sim::combat::CombatPosition;
use sts_oracle_runtime::state::core::ClientInput;

use super::types::ChampResourceTiming;

pub(super) fn note_champ_resource_before_split(
    position: &CombatPosition,
    input: &ClientInput,
    step_index: usize,
    resources: &mut ChampResourceTiming,
) {
    match input {
        ClientInput::PlayCard { card_index, .. } => {
            if position
                .combat
                .zones
                .hand
                .get(*card_index)
                .is_some_and(|card| card.id == CardId::Disarm)
            {
                resources.disarm_used_before_split = true;
                resources.disarm_step.get_or_insert(step_index);
            }
        }
        ClientInput::UsePotion { potion_index, .. } => {
            resources.potions_used_before_split =
                resources.potions_used_before_split.saturating_add(1);
            match position
                .combat
                .entities
                .potions
                .get(*potion_index)
                .and_then(|potion| potion.as_ref())
                .map(|potion| potion.id)
            {
                Some(PotionId::FearPotion) => {
                    resources.fear_potion_used_before_split = true;
                    resources.fear_potion_step.get_or_insert(step_index);
                }
                Some(PotionId::StrengthPotion) => {
                    resources.strength_potion_used_before_split = true;
                    resources.strength_potion_step.get_or_insert(step_index);
                }
                Some(PotionId::SteroidPotion) => {
                    resources.steroid_potion_used_before_split = true;
                    resources.steroid_potion_step.get_or_insert(step_index);
                }
                Some(PotionId::BlessingOfTheForge) => {
                    resources.forge_potion_used_before_split = true;
                    resources.forge_potion_step.get_or_insert(step_index);
                }
                _ => {}
            }
        }
        _ => {}
    }
}
