//! Engine-neutral combat-search vocabulary.
//!
//! The repository has two intentionally different exact combat solvers:
//! atomic best-first search and the production complete-turn portfolio.
//! Shared request concepts and durable engine identity belong here; each
//! engine still validates its own supported satisfaction modes. Rollout,
//! turn-plan, frontier, and plugin controls remain owned by the atomic engine.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CombatWitnessSatisfactionV1 {
    BudgetOrExhaustion,
    #[default]
    ZeroLossOrBudget,
    FirstCompleteWin,
    HpLossAtMost(u32),
    /// Requires a strict non-regressing improvement in the typed persistent
    /// outcome vector. This is not run value and does not exchange one
    /// persistent resource for another.
    #[serde(alias = "persistent_run_value_gain")]
    MaterializedPersistentPayoffGain,
    FirstCompleteWinWithoutNewExternalBurden,
    HpLossAtMostWithoutNewExternalBurden(u32),
    PotionFreeHpLossAtMostWithoutNewExternalBurden(u32),
}

impl CombatWitnessSatisfactionV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::BudgetOrExhaustion => "budget_or_exhaustion",
            Self::ZeroLossOrBudget => "zero_loss_or_budget",
            Self::FirstCompleteWin => "first_complete_win",
            Self::HpLossAtMost(_) => "hp_loss_at_most",
            Self::MaterializedPersistentPayoffGain => "materialized_persistent_payoff_gain",
            Self::FirstCompleteWinWithoutNewExternalBurden => {
                "first_complete_win_without_new_external_burden"
            }
            Self::HpLossAtMostWithoutNewExternalBurden(_) => {
                "hp_loss_at_most_without_new_external_burden"
            }
            Self::PotionFreeHpLossAtMostWithoutNewExternalBurden(_) => {
                "potion_free_hp_loss_at_most_without_new_external_burden"
            }
        }
    }

    pub fn hp_loss_limit(self) -> Option<u32> {
        match self {
            Self::HpLossAtMost(limit)
            | Self::HpLossAtMostWithoutNewExternalBurden(limit)
            | Self::PotionFreeHpLossAtMostWithoutNewExternalBurden(limit) => Some(limit),
            Self::BudgetOrExhaustion
            | Self::ZeroLossOrBudget
            | Self::FirstCompleteWin
            | Self::MaterializedPersistentPayoffGain
            | Self::FirstCompleteWinWithoutNewExternalBurden => None,
        }
    }
}

/// Tests a replayed terminal victory against the materialized strategic
/// quality requested by run control. This is deliberately narrower than an
/// engine's stopping rule: a search may keep refining a satisfying line while
/// Feed, theft recovery, or another concrete payoff remains available.
pub fn combat_witness_satisfies_materialized_quality_v1(
    satisfaction: CombatWitnessSatisfactionV1,
    root_hp: i32,
    root_persistent: crate::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1,
    final_hp: i32,
    final_persistent: crate::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1,
    total_potion_expenditures: u32,
) -> bool {
    let hp_loss = root_hp.saturating_sub(final_hp).max(0) as u32;
    let no_unrecovered_theft = final_persistent.unrecovered_stolen_gold == 0;
    let no_new_external_burden =
        final_persistent.external_burden_count <= root_persistent.external_burden_count;
    let requested_quality = match satisfaction {
        CombatWitnessSatisfactionV1::BudgetOrExhaustion => false,
        CombatWitnessSatisfactionV1::ZeroLossOrBudget => hp_loss == 0,
        CombatWitnessSatisfactionV1::FirstCompleteWin => true,
        CombatWitnessSatisfactionV1::HpLossAtMost(limit) => hp_loss <= limit,
        CombatWitnessSatisfactionV1::MaterializedPersistentPayoffGain => {
            final_persistent.strictly_dominates(root_persistent)
        }
        CombatWitnessSatisfactionV1::FirstCompleteWinWithoutNewExternalBurden => {
            no_new_external_burden
        }
        CombatWitnessSatisfactionV1::HpLossAtMostWithoutNewExternalBurden(limit) => {
            hp_loss <= limit && no_new_external_burden
        }
        CombatWitnessSatisfactionV1::PotionFreeHpLossAtMostWithoutNewExternalBurden(limit) => {
            hp_loss <= limit && no_new_external_burden && total_potion_expenditures == 0
        }
    };
    requested_quality && no_unrecovered_theft
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatWitnessPotionPolicyV1 {
    Never,
    #[serde(alias = "all_legal_potion_actions")]
    All,
    #[serde(alias = "semantic_budgeted_potion_actions")]
    SemanticBudgeted,
}

impl CombatWitnessPotionPolicyV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::All => "all_legal_potion_actions",
            Self::SemanticBudgeted => "semantic_budgeted_potion_actions",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatWitnessEngineV1 {
    /// `src/ai/combat_search_v2`: atomic-action best-first fixed-root search.
    AtomicExactV2,
    /// Production resident portfolio over complete-turn local graph and
    /// policy-discrepancy search.
    TurnGraphPortfolioV1,
}

/// Shared owner default for the explicitly opened high-stakes semantic potion
/// lane. This is admission policy, not a property of either search engine.
pub fn high_stakes_semantic_witness_potion_budget_v1(
    combat: &crate::runtime::combat::CombatState,
) -> Option<u32> {
    const BOSS_MAX_POTIONS_USED: u32 = 2;
    const ELITE_MAX_POTIONS_USED: u32 = 1;

    if combat.meta.is_boss_fight {
        Some(BOSS_MAX_POTIONS_USED)
    } else if combat.meta.is_elite_fight {
        Some(ELITE_MAX_POTIONS_USED)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::combat_persistent_outcome_v1::CombatPersistentOutcomeV1;

    fn persistent(ritual_dagger_value: i32) -> CombatPersistentOutcomeV1 {
        CombatPersistentOutcomeV1 {
            max_hp: 80,
            recoverable_gold_delta: 0,
            unrecovered_stolen_gold: 0,
            ritual_dagger_value,
            genetic_algorithm_value: 0,
            external_burden_count: 0,
        }
    }

    #[test]
    fn persistent_payoff_satisfaction_accepts_non_regressing_dagger_growth() {
        assert!(combat_witness_satisfies_materialized_quality_v1(
            CombatWitnessSatisfactionV1::MaterializedPersistentPayoffGain,
            60,
            persistent(15),
            60,
            persistent(18),
            0,
        ));
    }

    #[test]
    fn persistent_payoff_satisfaction_rejects_cross_resource_exchange() {
        let root = persistent(15);
        let mut grown_with_new_burden = persistent(18);
        grown_with_new_burden.external_burden_count = 1;

        assert!(!combat_witness_satisfies_materialized_quality_v1(
            CombatWitnessSatisfactionV1::MaterializedPersistentPayoffGain,
            60,
            root,
            60,
            grown_with_new_burden,
            0,
        ));
    }
}
