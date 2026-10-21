//! Fixed combat resource queries shared by offline search experiments.
use serde::{Deserialize, Serialize};
use sts_combat_outcome::combat_witness_materialization::{
    project_observed_combat_outcome_with_expenditures_v1, CombatInputExpendituresV1,
    CombatWitnessMaterializedOutcomeV1,
};
use sts_core::sim::combat::CombatPosition;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceRequirement {
    pub min_final_hp: i32,
    /// Original combat-start slots whose potions must not be spent or discarded.
    /// A newly generated potion in the same slot cannot satisfy this condition.
    pub preserve_starting_potion_slots: u64,
}

impl ResourceRequirement {
    pub fn validate(&self, root: &CombatPosition) -> Result<(), String> {
        if self.min_final_hp < 1 {
            return Err("a victory requirement needs positive final HP".into());
        }
        let occupied = root
            .combat
            .entities
            .potions
            .iter()
            .enumerate()
            .filter(|(_, potion)| potion.is_some())
            .try_fold(0_u64, |mask, (slot, _)| {
                u32::try_from(slot)
                    .ok()
                    .and_then(|slot| 1_u64.checked_shl(slot))
                    .map(|bit| mask | bit)
            })
            .ok_or("starting potion slots exceed the resource mask")?;
        if self.preserve_starting_potion_slots & !occupied != 0 {
            return Err("cannot require an unoccupied starting potion slot".into());
        }
        Ok(())
    }

    /// Only for an already established exact victory, not an intermediate HP test.
    pub fn accepts(&self, outcome: &CombatWitnessMaterializedOutcomeV1) -> bool {
        outcome.final_hp >= self.min_final_hp
            && outcome.starting_potion_expenditure_slots & self.preserve_starting_potion_slots == 0
    }

    /// Prefix facts, not proof of replay. Publication still replays from the root.
    /// The shared projection includes explicit discards and implicit Fairy use.
    pub(crate) fn spent_originals(
        &self,
        root: &CombatPosition,
        expenditures: CombatInputExpendituresV1,
        position: &CombatPosition,
    ) -> u64 {
        project_observed_combat_outcome_with_expenditures_v1(root, expenditures, position)
            .starting_potion_expenditure_slots
    }
}
