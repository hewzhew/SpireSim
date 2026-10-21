use serde::Serialize;

use crate::ai::card_semantics_v1::{card_mechanics_profile_v1, CombatExternalPayoffV1};
use crate::content::cards::{get_card_definition, CardId, CardType};
use crate::runtime::combat::{CombatCard, CombatState, MetaChange};
use crate::state::rewards::RewardItem;

/// Exact persistent combat outcomes that must survive tactical witness search.
///
/// This is a fact vector, not a run-level value function. Callers may retain a
/// Pareto frontier with it, then apply their own continuation context when one
/// exact witness must be selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CombatPersistentOutcomeV1 {
    pub max_hp: i32,
    pub recoverable_gold_delta: i32,
    pub unrecovered_stolen_gold: i32,
    pub ritual_dagger_value: i32,
    pub genetic_algorithm_value: i32,
    pub external_burden_count: i32,
}

impl CombatPersistentOutcomeV1 {
    pub fn from_combat(combat: &CombatState) -> Self {
        Self {
            max_hp: combat.entities.player.max_hp,
            recoverable_gold_delta: recoverable_gold_delta(combat),
            unrecovered_stolen_gold: unrecovered_stolen_gold(combat),
            ritual_dagger_value: persistent_card_value(combat, CardId::RitualDagger),
            genetic_algorithm_value: persistent_card_value(combat, CardId::GeneticAlgorithm),
            external_burden_count: external_burden_count(combat),
        }
    }

    pub fn dominates_or_equals(self, other: Self) -> bool {
        self.max_hp >= other.max_hp
            && self.recoverable_gold_delta >= other.recoverable_gold_delta
            && self.unrecovered_stolen_gold <= other.unrecovered_stolen_gold
            && self.ritual_dagger_value >= other.ritual_dagger_value
            && self.genetic_algorithm_value >= other.genetic_algorithm_value
            && self.external_burden_count <= other.external_burden_count
    }

    pub fn strictly_dominates(self, other: Self) -> bool {
        self.dominates_or_equals(other) && self != other
    }

    pub fn materialized_payoff_score(self) -> i32 {
        self.max_hp
            .saturating_add(self.recoverable_gold_delta.saturating_div(5))
            .saturating_add(self.ritual_dagger_value)
            .saturating_add(self.genetic_algorithm_value)
    }
}

/// Compatibility score over persistent effects already materialized inside a
/// combat state.
///
/// This is not a continuation value and does not predict the rest of the run.
/// It only preserves the historical comparison among max HP, recovered gold,
/// Ritual Dagger growth, and Genetic Algorithm growth while callers migrate to
/// the typed `CombatPersistentOutcomeV1` frontier.
pub fn materialized_persistent_payoff_score_v1(combat: &CombatState) -> i32 {
    CombatPersistentOutcomeV1::from_combat(combat).materialized_payoff_score()
}

pub fn has_external_payoff_opportunity(combat: &CombatState) -> bool {
    combat_cards(combat).any(|card| card_has_external_payoff_opportunity(card, combat))
}

pub fn has_persistent_or_reward_payoff_opportunity(combat: &CombatState) -> bool {
    combat_cards(combat).any(|card| {
        matches!(
            card_mechanics_profile_v1(card.id).combat_external_payoff,
            Some(CombatExternalPayoffV1::PersistentOrReward)
        )
    })
}

pub fn has_healing_payoff_opportunity(combat: &CombatState) -> bool {
    combat.entities.player.current_hp < combat.entities.player.max_hp
        && combat_cards(combat).any(|card| {
            matches!(
                card_mechanics_profile_v1(card.id).combat_external_payoff,
                Some(CombatExternalPayoffV1::HealingIfDamaged)
            )
        })
}

fn combat_cards(combat: &CombatState) -> impl Iterator<Item = &CombatCard> {
    combat
        .meta
        .master_deck_snapshot
        .iter()
        .chain(combat.zones.hand.iter())
        .chain(combat.zones.draw_pile.iter())
        .chain(combat.zones.discard_pile.iter())
        .chain(combat.zones.exhaust_pile.iter())
        .chain(combat.zones.limbo.iter())
        .chain(combat.zones.queued_cards.iter().map(|queued| &queued.card))
}

fn card_has_external_payoff_opportunity(card: &CombatCard, combat: &CombatState) -> bool {
    match card_mechanics_profile_v1(card.id).combat_external_payoff {
        Some(CombatExternalPayoffV1::PersistentOrReward) => true,
        Some(CombatExternalPayoffV1::HealingIfDamaged) => {
            combat.entities.player.current_hp < combat.entities.player.max_hp
        }
        None => false,
    }
}

pub fn recoverable_stolen_gold(combat: &CombatState) -> i32 {
    combat
        .runtime
        .pending_rewards
        .iter()
        .filter_map(|reward| match reward {
            RewardItem::StolenGold { amount } => Some(*amount),
            _ => None,
        })
        .fold(0i32, i32::saturating_add)
}

pub fn recoverable_gold_delta(combat: &CombatState) -> i32 {
    combat
        .entities
        .player
        .gold_delta_this_combat
        .saturating_add(recoverable_stolen_gold(combat))
}

/// Gold still held by a living thief when a combat line ends. The player's
/// combat gold delta already values this loss; the separate typed fact lets
/// run control distinguish an acceptable low-HP-loss victory from a terminal
/// line that merely allowed the thieves to leave with persistent resources.
pub fn unrecovered_stolen_gold(combat: &CombatState) -> i32 {
    combat
        .entities
        .monsters
        .iter()
        .filter(|monster| {
            matches!(
                monster.enemy_id,
                crate::content::monsters::EnemyId::Looter
                    | crate::content::monsters::EnemyId::Mugger
            ) && monster.current_hp > 0
                && !monster.is_dying
                && !monster.half_dead
        })
        .map(|monster| monster.thief.stolen_gold.max(0))
        .fold(0i32, i32::saturating_add)
}

pub fn persistent_card_value(combat: &CombatState, card_id: CardId) -> i32 {
    // The combat kernel already updates this snapshot. Meta changes only
    // notify the run owner; adding them here would count the growth twice.
    combat
        .meta
        .master_deck_snapshot
        .iter()
        .filter(|card| card.id == card_id)
        .map(|card| card.misc_value.max(0))
        .sum()
}

pub fn external_burden_count(combat: &CombatState) -> i32 {
    let curse_additions = combat
        .meta
        .meta_changes
        .iter()
        .filter(|change| {
            matches!(
                change,
                MetaChange::AddCardToMasterDeck(card_id)
                    if get_card_definition(card_id.card.id).card_type == CardType::Curse
            )
        })
        .count() as i32;
    curse_additions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;

    #[test]
    fn growth_outcome_reads_the_updated_master_snapshot_without_reapplying_its_run_notification() {
        for card_id in [CardId::GeneticAlgorithm, CardId::RitualDagger] {
            let mut combat = crate::test_support::blank_test_combat();
            let mut card = CombatCard::for_scenario(card_id, 901);
            card.misc_value = 17;
            combat.meta.master_deck_snapshot = vec![card.clone()].into();
            combat.zones.hand = vec![card.clone()];
            crate::engine::action_handlers::cards::handle_modify_card_misc(
                card.lineage,
                2,
                &mut combat,
            );
            assert_eq!(combat.meta.master_deck_snapshot[0].misc_value, 19);
            assert_eq!(persistent_card_value(&combat, card_id), 19);
            combat.meta.meta_changes.clear();
            assert_eq!(persistent_card_value(&combat, card_id), 19);
        }
    }

    #[test]
    fn unrecovered_stolen_gold_counts_only_living_thief_holdings() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut escaped = crate::test_support::test_monster(EnemyId::Looter);
        escaped.id = 1;
        escaped.is_escaped = true;
        escaped.thief.stolen_gold = 30;
        let mut killed = crate::test_support::test_monster(EnemyId::Mugger);
        killed.id = 2;
        killed.current_hp = 0;
        killed.is_dying = true;
        killed.thief.stolen_gold = 30;
        combat.entities.monsters = vec![escaped, killed];
        combat
            .runtime
            .pending_rewards
            .push(RewardItem::StolenGold { amount: 30 });

        let outcome = CombatPersistentOutcomeV1::from_combat(&combat);
        assert_eq!(outcome.unrecovered_stolen_gold, 30);
        assert_eq!(recoverable_stolen_gold(&combat), 30);
    }

    #[test]
    fn materialized_payoff_score_counts_recovered_stolen_gold_without_predicting_future_value() {
        let mut escaped = crate::test_support::blank_test_combat();
        escaped.entities.player.max_hp = 85;
        escaped.entities.player.gold_delta_this_combat = -75;
        let mut killed = escaped.clone();
        killed
            .runtime
            .pending_rewards
            .push(RewardItem::StolenGold { amount: 75 });

        assert_eq!(materialized_persistent_payoff_score_v1(&escaped), 70);
        assert_eq!(materialized_persistent_payoff_score_v1(&killed), 85);
    }
}
