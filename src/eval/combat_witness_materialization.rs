use crate::agent::task::{
    TaskResearchPlanGeneratedPotionV1, TaskResearchPlanMetaChangeV1, TaskResearchPlanOutcomeV1,
    TaskResearchPlanPendingRewardV1, TaskResearchPlanPersistentOutcomeV1,
    TaskResearchPlanRewardCardV1,
};
use crate::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1;
use crate::ai::combat_witness_contract::{
    combat_witness_satisfies_materialized_quality_v1, CombatWitnessSatisfactionV1,
};
use crate::runtime::combat::CardLineageId;
use crate::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use crate::state::core::ClientInput;
use serde::Serialize;

/// Materialized combat outcome facts shared by witness producers and diagnostics.
/// Replay provenance belongs to the producing operation, not this public data
/// struct: an observed endpoint can be projected for screening before replay.
///
/// This type deliberately contains no continuation value. It can prove strict
/// resource dominance between complete lines, but it cannot price HP against a
/// consumed potion or any other incomparable run resource.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CombatWitnessMaterializedOutcomeV1 {
    pub final_hp: i32,
    pub final_gold: i32,
    pub persistent: CombatPersistentOutcomeV1,
    pub starting_potion_expenditure_slots: u64,
    pub total_potion_expenditures: u32,
    pub generated_potions: Vec<(usize, crate::content::potions::Potion)>,
    pub final_relics: Vec<crate::content::relics::RelicState>,
    pub meta_changes: Vec<crate::runtime::combat::MetaChange>,
    pub pending_rewards: Vec<crate::state::rewards::RewardItem>,
    pub combat_mugged: bool,
}

impl CombatWitnessMaterializedOutcomeV1 {
    pub fn satisfies(
        &self,
        root: &CombatPosition,
        satisfaction: CombatWitnessSatisfactionV1,
    ) -> bool {
        combat_witness_satisfies_materialized_quality_v1(
            satisfaction,
            root.combat.entities.player.current_hp,
            CombatPersistentOutcomeV1::from_combat(&root.combat),
            self.final_hp,
            self.persistent,
            self.total_potion_expenditures,
        )
    }

    pub fn strictly_dominates(&self, other: &Self) -> bool {
        self.strictly_dominates_with_relic_equivalence_v1(
            other,
            crate::content::relics::persistent_state::post_combat_persistent_relic_states_equal_v1(
                &self.final_relics,
                &other.final_relics,
            ),
            false,
        )
    }

    /// Exact resource equality after normalizing representational details that
    /// do not survive combat: generated potion UUIDs and whether claimable
    /// gold is already on the player or still in the reward surface.
    pub fn has_same_materialized_resources_v1(&self, other: &Self) -> bool {
        materialized_claimable_gold_v1(self) == materialized_claimable_gold_v1(other)
            && self.final_hp == other.final_hp
            && self.persistent == other.persistent
            && self.starting_potion_expenditure_slots
                == other.starting_potion_expenditure_slots
            && self.total_potion_expenditures == other.total_potion_expenditures
            && same_materialized_generated_potions_v1(
                &self.generated_potions,
                &other.generated_potions,
            )
            && crate::content::relics::persistent_state::post_combat_persistent_relic_states_equal_v1(
                &self.final_relics,
                &other.final_relics,
            )
            && self.meta_changes == other.meta_changes
            && same_materialized_non_gold_pending_rewards_v1(
                &self.pending_rewards,
                &other.pending_rewards,
            )
            && self.combat_mugged == other.combat_mugged
    }

    pub fn strictly_dominates_with_bounded_relic_progress_equivalence_v1(
        &self,
        other: &Self,
    ) -> bool {
        self.strictly_dominates_with_relic_equivalence_v1(
            other,
            crate::content::relics::persistent_state::persistent_relic_states_equivalent_for_resource_dominance_v1(
                &self.final_relics,
                &other.final_relics,
            ),
            crate::content::relics::persistent_state::persistent_relic_states_strictly_dominate_within_resource_equivalence_v1(
                &self.final_relics,
                &other.final_relics,
            ),
        )
    }

    fn strictly_dominates_with_relic_equivalence_v1(
        &self,
        other: &Self,
        final_relics_equivalent: bool,
        relic_progress_strictly_better: bool,
    ) -> bool {
        let potion_expenditure_is_subset =
            self.starting_potion_expenditure_slots & !other.starting_potion_expenditure_slots == 0;
        let claimable_gold = materialized_claimable_gold_v1(self);
        let other_claimable_gold = materialized_claimable_gold_v1(other);
        let no_unmodeled_persistent_difference = same_materialized_generated_potions_v1(
            &self.generated_potions,
            &other.generated_potions,
        ) && final_relics_equivalent
            && self.meta_changes == other.meta_changes
            && same_materialized_non_gold_pending_rewards_v1(
                &self.pending_rewards,
                &other.pending_rewards,
            )
            && self.combat_mugged == other.combat_mugged;
        self.final_hp >= other.final_hp
            && claimable_gold >= other_claimable_gold
            && self.persistent.dominates_or_equals(other.persistent)
            && potion_expenditure_is_subset
            && self.total_potion_expenditures <= other.total_potion_expenditures
            && no_unmodeled_persistent_difference
            && (self.final_hp > other.final_hp
                || claimable_gold > other_claimable_gold
                || self.persistent != other.persistent
                || self.starting_potion_expenditure_slots
                    != other.starting_potion_expenditure_slots
                || self.total_potion_expenditures < other.total_potion_expenditures
                || relic_progress_strictly_better)
    }

    pub fn to_public_outcome_v1(
        &self,
        root: &CombatPosition,
    ) -> Result<TaskResearchPlanOutcomeV1, String> {
        let consumed_entry_potion_slots = (0..u64::BITS)
            .filter(|slot| self.starting_potion_expenditure_slots & (1_u64 << slot) != 0)
            .map(|slot| slot as usize)
            .collect();
        let generated_potions = self
            .generated_potions
            .iter()
            .map(|(slot, potion)| TaskResearchPlanGeneratedPotionV1 {
                slot: *slot,
                potion_id: potion.id,
            })
            .collect();
        let meta_changes = self
            .meta_changes
            .iter()
            .map(|change| public_meta_change_v1(root, change))
            .collect::<Result<Vec<_>, _>>()?;
        let pending_rewards = self
            .pending_rewards
            .iter()
            .map(public_pending_reward_v1)
            .collect();
        Ok(TaskResearchPlanOutcomeV1 {
            final_hp: self.final_hp,
            final_gold: self.final_gold,
            persistent: TaskResearchPlanPersistentOutcomeV1 {
                max_hp: self.persistent.max_hp,
                recoverable_gold_delta: self.persistent.recoverable_gold_delta,
                unrecovered_stolen_gold: self.persistent.unrecovered_stolen_gold,
                ritual_dagger_value: self.persistent.ritual_dagger_value,
                genetic_algorithm_value: self.persistent.genetic_algorithm_value,
                external_burden_count: self.persistent.external_burden_count,
            },
            consumed_entry_potion_slots,
            total_potions_consumed: self.total_potion_expenditures,
            generated_potions,
            final_relics: self.final_relics.clone(),
            meta_changes,
            pending_rewards,
            combat_mugged: self.combat_mugged,
        })
    }
}

fn same_materialized_generated_potions_v1(
    left: &[(usize, crate::content::potions::Potion)],
    right: &[(usize, crate::content::potions::Potion)],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|((left_slot, left), (right_slot, right))| {
                left_slot == right_slot && left.id == right.id
            })
}

/// Held gold plus pending gold rewards, without valuing other resources.
pub fn materialized_claimable_gold_v1(outcome: &CombatWitnessMaterializedOutcomeV1) -> i64 {
    i64::from(outcome.final_gold)
        + outcome
            .pending_rewards
            .iter()
            .map(|reward| match reward {
                crate::state::rewards::RewardItem::Gold { amount }
                | crate::state::rewards::RewardItem::StolenGold { amount } => i64::from(*amount),
                _ => 0,
            })
            .sum::<i64>()
}

fn same_materialized_non_gold_pending_rewards_v1(
    left: &[crate::state::rewards::RewardItem],
    right: &[crate::state::rewards::RewardItem],
) -> bool {
    left.iter()
        .filter(|reward| {
            !matches!(
                reward,
                crate::state::rewards::RewardItem::Gold { .. }
                    | crate::state::rewards::RewardItem::StolenGold { .. }
            )
        })
        .eq(right.iter().filter(|reward| {
            !matches!(
                reward,
                crate::state::rewards::RewardItem::Gold { .. }
                    | crate::state::rewards::RewardItem::StolenGold { .. }
            )
        }))
}

pub fn materialize_combat_witness_outcome_v1(
    root: &CombatPosition,
    inputs: &[ClientInput],
    max_engine_steps_per_transition: usize,
) -> Result<CombatWitnessMaterializedOutcomeV1, String> {
    let mut position = root.clone();
    for (index, input) in inputs.iter().enumerate() {
        if EngineCombatStepper.terminal(&position) != CombatTerminal::Unresolved {
            return Err(format!(
                "combat witness continued after terminal action {index}"
            ));
        }
        if !EngineCombatStepper.is_legal_action(&position, input) {
            return Err(format!(
                "combat witness action {index} is not legal at its exact state"
            ));
        }
        let step = EngineCombatStepper.apply_to_stable(
            &position,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: max_engine_steps_per_transition,
                deadline: None,
            },
        );
        if step.truncated || step.timed_out {
            return Err(format!(
                "combat witness action {index} did not reach a stable boundary"
            ));
        }
        position = step.position;
    }
    materialize_replay_checked_combat_outcome_v1(root, inputs, &position)
}

/// Materialize a line whose endpoint was already reached through canonical
/// exact action application by its owning environment.
pub fn materialize_replay_checked_combat_outcome_v1(
    root: &CombatPosition,
    inputs: &[ClientInput],
    position: &CombatPosition,
) -> Result<CombatWitnessMaterializedOutcomeV1, String> {
    let terminal = EngineCombatStepper.terminal(position);
    if !EngineCombatStepper.is_exact_victory(position) {
        return Err(format!(
            "combat witness did not replay to an exact victory: terminal={terminal:?} combat_smoked={} actions={} {}",
            position.combat.runtime.combat_smoked,
            inputs.len(),
            bounded_combat_input_diagnostic_v1(inputs),
        ));
    }
    Ok(project_replay_checked_combat_outcome_v1(
        root, inputs, position,
    ))
}

/// Projects an endpoint already proven to be the exact replay of `inputs`.
///
/// Unlike [`materialize_replay_checked_combat_outcome_v1`], this boundary does
/// not repeat the terminal-victory check. Search owners that just performed
/// canonical replay can use it without inventing a second resource model.
pub fn project_replay_checked_combat_outcome_v1(
    root: &CombatPosition,
    inputs: &[ClientInput],
    position: &CombatPosition,
) -> CombatWitnessMaterializedOutcomeV1 {
    project_observed_combat_outcome_v1(root, inputs, position)
}

/// Commutative explicit input facts for observed-outcome screening. This is
/// not a replay receipt or state identity; implicit entry-potion loss is still
/// determined from root/endpoint UUIDs by the projection owner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CombatInputExpendituresV1 {
    explicit_count: u32,
    explicit_slot_mask: u64,
}

impl CombatInputExpendituresV1 {
    /// Monotone lower bound on total expenditures, including explicit discards.
    pub fn explicit_count(&self) -> u32 {
        self.explicit_count
    }

    /// Shared bottle-identity accounting for both partial search states and
    /// settled outcomes. Includes passive loss of entry bottles without
    /// counting an explicitly consumed bottle twice.
    pub fn potion_expenditures(
        &self,
        starting: &[Option<crate::content::potions::Potion>],
        remaining: &[Option<crate::content::potions::Potion>],
    ) -> (u64, u32) {
        let starting_slots = starting
            .iter()
            .enumerate()
            .filter_map(|(slot, potion)| {
                let potion = potion.as_ref()?;
                (!remaining.iter().flatten().any(|p| p.uuid == potion.uuid))
                    .then(|| u32::try_from(slot).ok())
                    .flatten()
                    .and_then(|slot| 1_u64.checked_shl(slot))
            })
            .fold(0_u64, |mask, slot| mask | slot);
        let implicit = starting_slots & !self.explicit_slot_mask;
        (
            starting_slots,
            self.explicit_count.saturating_add(implicit.count_ones()),
        )
    }

    /// Record one materialized input, including discards and repeated use of a
    /// refilled slot. Virtual selection assembly is not a game input.
    pub fn record(&mut self, input: &ClientInput) {
        let slot = match input {
            ClientInput::UsePotion { potion_index, .. } => *potion_index,
            ClientInput::DiscardPotion(slot) => *slot,
            _ => return,
        };
        self.explicit_count = self.explicit_count.saturating_add(1);
        if let Some(mask) = u32::try_from(slot)
            .ok()
            .and_then(|slot| 1_u64.checked_shl(slot))
        {
            self.explicit_slot_mask |= mask;
        }
    }

    pub fn from_inputs(inputs: &[ClientInput]) -> Self {
        let mut summary = Self::default();
        for input in inputs {
            summary.record(input);
        }
        summary
    }
}

/// Projects observed endpoint facts for candidate screening. This does not
/// establish that `inputs` replay to `position`, or that it is a victory.
/// A selected candidate must still pass canonical replay and terminal checks.
/// Keeping this projection shared prevents search from inventing a weaker
/// potion-count or HP-only notion of resource equivalence.
pub fn project_observed_combat_outcome_v1(
    root: &CombatPosition,
    inputs: &[ClientInput],
    position: &CombatPosition,
) -> CombatWitnessMaterializedOutcomeV1 {
    project_observed_combat_outcome_with_expenditures_v1(
        root,
        CombatInputExpendituresV1::from_inputs(inputs),
        position,
    )
}

/// Original bottles targeted by an explicit use/discard at this source.
/// This is an admission fact for an already legal input, not a replay receipt.
/// Matching uses potion identity, so a generated replacement in the same slot
/// is not mistaken for the original bottle.
pub fn starting_potion_slots_targeted_by_input_v1(
    root: &CombatPosition,
    source: &CombatPosition,
    input: &ClientInput,
) -> u64 {
    let slot = match input {
        ClientInput::UsePotion { potion_index, .. } => *potion_index,
        ClientInput::DiscardPotion(slot) => *slot,
        _ => return 0,
    };
    let Some(target) = source
        .combat
        .entities
        .potions
        .get(slot)
        .and_then(Option::as_ref)
    else {
        return 0;
    };
    root.combat
        .entities
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            (potion.as_ref()?.uuid == target.uuid)
                .then(|| {
                    u32::try_from(slot)
                        .ok()
                        .and_then(|slot| 1_u64.checked_shl(slot))
                })
                .flatten()
        })
        .fold(0, |mask, bit| mask | bit)
}

/// The same observed projection using an incremental summary of the actual
/// input prefix. This does not certify that the summary belongs to `position`;
/// publication still requires independent forward replay with the full inputs.
pub fn project_observed_combat_outcome_with_expenditures_v1(
    root: &CombatPosition,
    expenditures: CombatInputExpendituresV1,
    position: &CombatPosition,
) -> CombatWitnessMaterializedOutcomeV1 {
    let starting_potion_uuids = root
        .combat
        .entities
        .potions
        .iter()
        .flatten()
        .map(|potion| potion.uuid)
        .collect::<std::collections::BTreeSet<_>>();
    let generated_potions = position
        .combat
        .entities
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            let potion = potion.as_ref()?;
            (!starting_potion_uuids.contains(&potion.uuid)).then(|| (slot, potion.clone()))
        })
        .collect();
    let (starting_potion_expenditure_slots, total_potion_expenditures) = expenditures
        .potion_expenditures(
            &root.combat.entities.potions,
            &position.combat.entities.potions,
        );
    CombatWitnessMaterializedOutcomeV1 {
        final_hp: position.combat.entities.player.current_hp,
        final_gold: position.combat.entities.player.gold,
        persistent: CombatPersistentOutcomeV1::from_combat(&position.combat),
        starting_potion_expenditure_slots,
        total_potion_expenditures,
        generated_potions,
        final_relics: position
            .combat
            .entities
            .player
            .relics
            .iter()
            .map(crate::content::relics::persistent_state::post_combat_persistent_relic_state_v1)
            .collect(),
        meta_changes: position.combat.meta.meta_changes.clone(),
        pending_rewards: position.combat.runtime.pending_rewards.clone(),
        combat_mugged: position.combat.runtime.combat_mugged,
    }
}

fn bounded_combat_input_diagnostic_v1(inputs: &[ClientInput]) -> String {
    const HEAD_LIMIT: usize = 4;
    const TAIL_LIMIT: usize = 2;
    const POTION_LIMIT: usize = 8;

    let head = inputs
        .iter()
        .enumerate()
        .take(HEAD_LIMIT)
        .map(|(index, input)| format!("{index}:{input:?}"))
        .collect::<Vec<_>>();
    let tail_start = inputs
        .len()
        .saturating_sub(TAIL_LIMIT)
        .max(inputs.len().min(HEAD_LIMIT));
    let tail = inputs
        .iter()
        .enumerate()
        .skip(tail_start)
        .map(|(index, input)| format!("{index}:{input:?}"))
        .collect::<Vec<_>>();
    let potion_input_count = inputs
        .iter()
        .filter(|input| {
            matches!(
                input,
                ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
            )
        })
        .count();
    let potion_inputs = inputs
        .iter()
        .enumerate()
        .filter(|(_, input)| {
            matches!(
                input,
                ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
            )
        })
        .take(POTION_LIMIT)
        .map(|(index, input)| format!("{index}:{input:?}"))
        .collect::<Vec<_>>();

    format!(
        "head=[{}] tail=[{}] omitted_between={} potion_inputs=[{}] omitted_potion_inputs={}",
        head.join(", "),
        tail.join(", "),
        tail_start.saturating_sub(head.len()),
        potion_inputs.join(", "),
        potion_input_count.saturating_sub(potion_inputs.len()),
    )
}

fn public_meta_change_v1(
    root: &CombatPosition,
    change: &crate::runtime::combat::MetaChange,
) -> Result<TaskResearchPlanMetaChangeV1, String> {
    Ok(match change {
        crate::runtime::combat::MetaChange::AddCardToMasterDeck(card_id) => {
            TaskResearchPlanMetaChangeV1::AddCardToMasterDeck {
                card_id: card_id.card.id,
            }
        }
        crate::runtime::combat::MetaChange::ModifyCardMisc {
            card_lineage,
            amount,
        } => TaskResearchPlanMetaChangeV1::ModifyCardMisc {
            master_deck_ordinal: public_master_deck_ordinal_v1(root, *card_lineage)?,
            amount: *amount,
        },
        crate::runtime::combat::MetaChange::UpgradeMasterDeckCard { card_lineage } => {
            TaskResearchPlanMetaChangeV1::UpgradeMasterDeckCard {
                master_deck_ordinal: public_master_deck_ordinal_v1(root, *card_lineage)?,
            }
        }
    })
}

fn public_master_deck_ordinal_v1(
    root: &CombatPosition,
    card_lineage: CardLineageId,
) -> Result<usize, String> {
    crate::agent::information::state::public_master_deck_ordinal_for_lineage_v1(
        &root.combat,
        card_lineage,
    )
    .ok_or_else(|| "combat outcome references a card outside the public master deck".to_string())
}

fn public_pending_reward_v1(
    reward: &crate::state::rewards::RewardItem,
) -> TaskResearchPlanPendingRewardV1 {
    match reward {
        crate::state::rewards::RewardItem::Gold { amount } => {
            TaskResearchPlanPendingRewardV1::Gold { amount: *amount }
        }
        crate::state::rewards::RewardItem::StolenGold { amount } => {
            TaskResearchPlanPendingRewardV1::StolenGold { amount: *amount }
        }
        crate::state::rewards::RewardItem::Card { cards } => {
            TaskResearchPlanPendingRewardV1::Card {
                cards: cards
                    .iter()
                    .map(|card| TaskResearchPlanRewardCardV1 {
                        card_id: card.id,
                        upgrades: card.upgrades,
                    })
                    .collect(),
            }
        }
        crate::state::rewards::RewardItem::Relic { relic_id } => {
            TaskResearchPlanPendingRewardV1::Relic {
                relic_id: *relic_id,
            }
        }
        crate::state::rewards::RewardItem::Potion { potion_id } => {
            TaskResearchPlanPendingRewardV1::Potion {
                potion_id: *potion_id,
            }
        }
        crate::state::rewards::RewardItem::EmeraldKey => {
            TaskResearchPlanPendingRewardV1::EmeraldKey
        }
        crate::state::rewards::RewardItem::SapphireKey => {
            TaskResearchPlanPendingRewardV1::SapphireKey
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_expenditures_preserve_refills_discards_and_passive_consumption() {
        use crate::content::potions::{Potion, PotionId};
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.entities.potions = vec![
            Some(Potion::new(PotionId::FirePotion, 1)),
            Some(Potion::new(PotionId::WeakenPotion, 2)),
            Some(Potion::new(PotionId::FairyPotion, 3)),
            Some(Potion::new(PotionId::DexterityPotion, 4)),
        ];
        let root = CombatPosition::new(crate::state::core::EngineState::CombatPlayerTurn, combat);
        // Explicit endpoint intervention tests projection facts, not legality or
        // victory: slot 0 was refilled twice, slot 2 was consumed passively.
        let mut endpoint = root.clone();
        let generated = Potion::new(PotionId::ColorlessPotion, 5);
        endpoint.combat.entities.potions[0] = Some(generated.clone());
        endpoint.combat.entities.potions[1] = None;
        endpoint.combat.entities.potions[2] = None;
        let inputs = [
            ClientInput::DiscardPotion(0),
            ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            },
            ClientInput::EndTurn,
            ClientInput::UsePotion {
                potion_index: 1,
                target: None,
            },
        ];
        let mut summary = CombatInputExpendituresV1::default();
        for (i, input) in inputs.iter().enumerate() {
            summary.record(input);
            assert_eq!(
                summary,
                CombatInputExpendituresV1::from_inputs(&inputs[..=i])
            );
        }
        let result =
            project_observed_combat_outcome_with_expenditures_v1(&root, summary, &endpoint);
        assert_eq!(
            result,
            project_observed_combat_outcome_v1(&root, &inputs, &endpoint)
        );
        assert_eq!(result.starting_potion_expenditure_slots, 0b111);
        assert_eq!(result.total_potion_expenditures, 4);
        assert_eq!(result.generated_potions, vec![(0, generated)]);
        // A branched history inherits the prefix without mutating its sibling.
        let mut sibling = summary;
        sibling.record(&ClientInput::DiscardPotion(3));
        assert_eq!(summary.explicit_count, 3);
        assert_eq!(sibling.explicit_count, 4);
    }

    #[test]
    fn expenditure_count_saturates_without_wrapping_or_invalid_shifts() {
        let mut summary = CombatInputExpendituresV1 {
            explicit_count: u32::MAX - 1,
            explicit_slot_mask: 0,
        };
        summary.record(&ClientInput::DiscardPotion(63));
        summary.record(&ClientInput::DiscardPotion(64));
        summary.record(&ClientInput::DiscardPotion(usize::MAX));
        assert_eq!(summary.explicit_count, u32::MAX);
        assert_eq!(summary.explicit_slot_mask, 1_u64 << 63);
    }

    fn outcome(
        final_hp: i32,
        final_gold: i32,
        pending_stolen_gold: i32,
    ) -> CombatWitnessMaterializedOutcomeV1 {
        CombatWitnessMaterializedOutcomeV1 {
            final_hp,
            final_gold,
            persistent: CombatPersistentOutcomeV1 {
                max_hp: 77,
                recoverable_gold_delta: 0,
                unrecovered_stolen_gold: 0,
                ritual_dagger_value: 0,
                genetic_algorithm_value: 0,
                external_burden_count: 0,
            },
            starting_potion_expenditure_slots: 0,
            total_potion_expenditures: 0,
            generated_potions: Vec::new(),
            final_relics: Vec::new(),
            meta_changes: Vec::new(),
            pending_rewards: vec![crate::state::rewards::RewardItem::StolenGold {
                amount: pending_stolen_gold,
            }],
            combat_mugged: false,
        }
    }

    #[test]
    fn materialized_dominance_normalizes_claimable_stolen_gold() {
        let healthier = outcome(69, 42, 30);
        let lower_hp = outcome(61, 57, 15);

        assert!(healthier.strictly_dominates(&lower_hp));
        assert!(!lower_hp.strictly_dominates(&healthier));
    }

    #[test]
    fn materialized_dominance_preserves_real_hp_gold_tradeoff() {
        let healthier = outcome(69, 42, 15);
        let richer = outcome(61, 57, 15);

        assert!(!healthier.strictly_dominates(&richer));
        assert!(!richer.strictly_dominates(&healthier));
    }

    #[test]
    fn failed_witness_diagnostic_bounds_actions_and_potion_inputs() {
        let mut inputs = vec![ClientInput::EndTurn; 12];
        for potion_index in 0..9 {
            inputs.insert(
                4 + potion_index,
                ClientInput::UsePotion {
                    potion_index,
                    target: None,
                },
            );
        }

        let diagnostic = bounded_combat_input_diagnostic_v1(&inputs);

        assert!(diagnostic.contains("head=[0:EndTurn"));
        assert!(diagnostic.contains("tail=[19:EndTurn, 20:EndTurn]"));
        assert!(diagnostic.contains("omitted_between=15"));
        assert!(diagnostic.contains("omitted_potion_inputs=1"));
    }
}
