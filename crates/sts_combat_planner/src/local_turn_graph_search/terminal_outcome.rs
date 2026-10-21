use serde::Serialize;
use sts_combat_outcome::combat_witness_materialization::CombatWitnessMaterializedOutcomeV1;
use sts_core::sim::combat::CombatPosition;
use sts_core::state::core::ClientInput;

/// Compact typed projection of one retained exact terminal witness.
///
/// Reports expose this fact vector instead of duplicating full action lines.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalTurnGraphTerminalOutcomeSnapshotV1 {
    pub selected_by_local_hp_view: bool,
    pub final_hp: i32,
    pub final_max_hp: i32,
    pub recoverable_gold_delta: i32,
    pub recoverable_stolen_gold: i32,
    pub unrecovered_stolen_gold: i32,
    pub ritual_dagger_value: i32,
    pub genetic_algorithm_value: i32,
    pub external_burden_count: i32,
    pub potion_expenditures: u32,
    /// Complete replay-materialized resources used by frontier dominance.
    /// The flattened fields above remain compact report columns only.
    pub materialized_outcome: CombatWitnessMaterializedOutcomeV1,
    pub action_count: usize,
    pub negative_log_policy: f64,
}

impl LocalTurnGraphTerminalOutcomeSnapshotV1 {
    /// Projects a terminal position whose exact action line was already
    /// replay-verified by the caller. Resource identity is measured from the
    /// supplied root, so fixed prefixes must be included in `inputs`.
    pub fn from_replay_checked_position_v1(
        root: &CombatPosition,
        inputs: &[ClientInput],
        final_position: &CombatPosition,
        action_count: usize,
        negative_log_policy: f64,
        selected_by_local_hp_view: bool,
    ) -> Self {
        let materialized = sts_combat_outcome::combat_witness_materialization::project_replay_checked_combat_outcome_v1(
            root,
            inputs,
            final_position,
        );
        Self::from_materialized_outcome_v1(
            materialized,
            action_count,
            negative_log_policy,
            selected_by_local_hp_view,
        )
    }

    /// Builds the resource projection for an already materialized and replay-
    /// verified combat terminal. No search or continuation value is inferred.
    pub fn from_materialized_outcome_v1(
        materialized_outcome: CombatWitnessMaterializedOutcomeV1,
        action_count: usize,
        negative_log_policy: f64,
        selected_by_local_hp_view: bool,
    ) -> Self {
        let persistent = materialized_outcome.persistent;
        let recoverable_stolen_gold = materialized_outcome
            .pending_rewards
            .iter()
            .filter_map(|reward| match reward {
                sts_core::state::rewards::RewardItem::StolenGold { amount } => Some(*amount),
                _ => None,
            })
            .fold(0i32, i32::saturating_add);
        Self {
            selected_by_local_hp_view,
            final_hp: materialized_outcome.final_hp,
            final_max_hp: persistent.max_hp,
            recoverable_gold_delta: persistent.recoverable_gold_delta,
            recoverable_stolen_gold,
            unrecovered_stolen_gold: persistent.unrecovered_stolen_gold,
            ritual_dagger_value: persistent.ritual_dagger_value,
            genetic_algorithm_value: persistent.genetic_algorithm_value,
            external_burden_count: persistent.external_burden_count,
            potion_expenditures: materialized_outcome.total_potion_expenditures,
            materialized_outcome,
            action_count,
            negative_log_policy,
        }
    }

    /// Whether both snapshots carry the same materialized terminal resources.
    /// Search-policy likelihood and the compatibility selection marker are not
    /// resources and therefore do not participate in this relation.
    pub fn has_same_resources_as(&self, other: &Self) -> bool {
        self.materialized_outcome
            .has_same_materialized_resources_v1(&other.materialized_outcome)
    }

    /// Strict dominance on resources already materialized at combat terminal.
    /// This deliberately says nothing about unobserved outcomes from a bounded
    /// search and is suitable only for retaining an observed witness frontier.
    pub fn strictly_dominates_resources(&self, other: &Self) -> bool {
        self.materialized_outcome
            .strictly_dominates_with_bounded_relic_progress_equivalence_v1(
                &other.materialized_outcome,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_combat_evidence::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1;

    fn outcome(final_hp: i32, potion_expenditures: u32) -> LocalTurnGraphTerminalOutcomeSnapshotV1 {
        LocalTurnGraphTerminalOutcomeSnapshotV1::from_materialized_outcome_v1(
            CombatWitnessMaterializedOutcomeV1 {
                final_hp,
                final_gold: 0,
                persistent: CombatPersistentOutcomeV1 {
                    max_hp: 80,
                    recoverable_gold_delta: 0,
                    unrecovered_stolen_gold: 0,
                    ritual_dagger_value: 0,
                    genetic_algorithm_value: 0,
                    external_burden_count: 0,
                },
                starting_potion_expenditure_slots: if potion_expenditures == 0 { 0 } else { 1 },
                total_potion_expenditures: potion_expenditures,
                generated_potions: Vec::new(),
                final_relics: Vec::new(),
                meta_changes: Vec::new(),
                pending_rewards: Vec::new(),
                combat_mugged: false,
            },
            12,
            0.0,
            false,
        )
    }

    #[test]
    fn higher_hp_strictly_dominates_equal_resources() {
        assert!(outcome(50, 0).strictly_dominates_resources(&outcome(14, 0)));
    }

    #[test]
    fn hp_and_potion_tradeoff_remains_incomparable() {
        assert!(!outcome(50, 1).strictly_dominates_resources(&outcome(14, 0)));
        assert!(!outcome(14, 0).strictly_dominates_resources(&outcome(50, 1)));
    }

    #[test]
    fn action_count_is_not_a_materialized_resource() {
        let short = outcome(50, 0);
        let mut long = short.clone();
        long.action_count += 40;

        assert!(short.has_same_resources_as(&long));
        assert!(!short.strictly_dominates_resources(&long));
        assert!(!long.strictly_dominates_resources(&short));
    }
}
