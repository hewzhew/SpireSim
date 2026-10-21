use serde::Serialize;

use crate::ai::deck_mutation_compiler_v1::{
    deck_removal_target_snapshots_v1, DeckMutationTargetLossTierV1, DeckMutationTargetLossV1,
};
use crate::ai::strategy::deck_strategic_deficit::{
    assess_deck_strategic_deficit, DeckStrategicDeficit, StrategicDeficitLevel,
};
use crate::ai::strategy::run_strategic_facts::RunStrategicFacts;
use crate::content::cards::CardId;
use crate::content::relics::RelicId;
use crate::runtime::combat::CardLineageId;
use crate::state::run::RunState;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeckRepairFunctionV1 {
    Frontload,
    Aoe,
    Block,
    Scaling,
    Access,
    EnergyOrPlayability,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckRepairRemovalCandidateV1 {
    pub deck_index: usize,
    pub lineage: CardLineageId,
    pub card: CardId,
    pub target_loss: DeckMutationTargetLossV1,
    pub provided_functions: Vec<DeckRepairFunctionV1>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeckRepairUpgradePriorityV1 {
    NeededFunction,
    Reliability,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeckRepairUpgradeReasonV1 {
    RetainsTimeSensitiveDefense,
    LowersNeededFunctionCost,
    PaysImportantUpgradeDebt,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeckRepairUpgradeCandidateV1 {
    pub deck_index: usize,
    pub lineage: CardLineageId,
    pub card: CardId,
    pub priority: DeckRepairUpgradePriorityV1,
    pub reasons: Vec<DeckRepairUpgradeReasonV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckRepairProfileV1 {
    pub thin_or_missing_functions: Vec<DeckRepairFunctionV1>,
    pub low_loss_removals: Vec<DeckRepairRemovalCandidateV1>,
    pub reliability_upgrades: Vec<DeckRepairUpgradeCandidateV1>,
    pub source_tags: Vec<String>,
}

pub fn deck_repair_profile_v1(run_state: &RunState) -> DeckRepairProfileV1 {
    let upgrade_plan = crate::ai::upgrade_planner_v1::plan_upgrades_v1(run_state);
    deck_repair_profile_from_upgrade_plan_v1(run_state, &upgrade_plan)
}

pub fn deck_repair_profile_from_upgrade_plan_v1(
    run_state: &RunState,
    upgrade_plan: &crate::ai::upgrade_planner_v1::UpgradePlanV1,
) -> DeckRepairProfileV1 {
    let deficit = assess_deck_strategic_deficit(
        &run_state.master_deck,
        RunStrategicFacts::from_run_state(run_state),
    );
    let thin_or_missing_functions = thin_or_missing_functions(&deficit);
    let low_loss_removals = deck_removal_target_snapshots_v1(run_state)
        .into_iter()
        .filter(|snapshot| {
            snapshot.target_loss.tier == DeckMutationTargetLossTierV1::RedundantFunctional
        })
        .filter_map(|snapshot| {
            let provided_functions = functions_for_card(snapshot.card, snapshot.upgrades);
            if provided_functions
                .iter()
                .any(|function| thin_or_missing_functions.contains(function))
            {
                return None;
            }
            Some(DeckRepairRemovalCandidateV1 {
                deck_index: snapshot.deck_index,
                lineage: snapshot.lineage,
                card: snapshot.card,
                target_loss: snapshot.target_loss,
                provided_functions,
            })
        })
        .collect();
    let reliability_upgrades =
        repair_upgrade_candidates(run_state, &thin_or_missing_functions, upgrade_plan);
    let source_tags = if run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::PandorasBox)
    {
        vec!["pandoras_box".to_string()]
    } else {
        Vec::new()
    };

    DeckRepairProfileV1 {
        thin_or_missing_functions,
        low_loss_removals,
        reliability_upgrades,
        source_tags,
    }
}

fn repair_upgrade_candidates(
    run_state: &RunState,
    thin_or_missing_functions: &[DeckRepairFunctionV1],
    upgrade_plan: &crate::ai::upgrade_planner_v1::UpgradePlanV1,
) -> Vec<DeckRepairUpgradeCandidateV1> {
    use crate::ai::upgrade_planner_v1::{UpgradeDebtSeverityV1, UpgradeRoleV1};

    upgrade_plan
        .candidates
        .iter()
        .filter_map(|candidate| {
            let card = run_state.master_deck.get(candidate.deck_index)?;
            let retains_time_sensitive_defense = candidate.mechanical_delta.ethereal_removed_delta
                && candidate.roles.contains(&UpgradeRoleV1::DefensiveSurvival);
            let lowers_needed_function_cost = candidate.mechanical_delta.cost_delta > 0
                && functions_for_card(candidate.card, candidate.upgrades)
                    .iter()
                    .any(|function| thin_or_missing_functions.contains(function));
            let pays_important_upgrade_debt =
                candidate.urgency >= UpgradeDebtSeverityV1::ImportantBeforeBoss;

            let priority = if lowers_needed_function_cost || pays_important_upgrade_debt {
                DeckRepairUpgradePriorityV1::NeededFunction
            } else if retains_time_sensitive_defense {
                DeckRepairUpgradePriorityV1::Reliability
            } else {
                return None;
            };
            let mut reasons = Vec::new();
            if retains_time_sensitive_defense {
                reasons.push(DeckRepairUpgradeReasonV1::RetainsTimeSensitiveDefense);
            }
            if lowers_needed_function_cost {
                reasons.push(DeckRepairUpgradeReasonV1::LowersNeededFunctionCost);
            }
            if pays_important_upgrade_debt {
                reasons.push(DeckRepairUpgradeReasonV1::PaysImportantUpgradeDebt);
            }

            Some(DeckRepairUpgradeCandidateV1 {
                deck_index: candidate.deck_index,
                lineage: card.lineage,
                card: candidate.card,
                priority,
                reasons,
            })
        })
        .collect()
}

fn is_thin_or_missing(level: StrategicDeficitLevel) -> bool {
    matches!(
        level,
        StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
    )
}

fn thin_or_missing_functions(deficit: &DeckStrategicDeficit) -> Vec<DeckRepairFunctionV1> {
    let fields = [
        (DeckRepairFunctionV1::Frontload, deficit.frontload_damage),
        (DeckRepairFunctionV1::Aoe, deficit.aoe_or_minion_control),
        (DeckRepairFunctionV1::Block, deficit.block_or_mitigation),
        (DeckRepairFunctionV1::Scaling, deficit.boss_scaling_plan),
        (DeckRepairFunctionV1::Access, deficit.deck_access),
        (
            DeckRepairFunctionV1::EnergyOrPlayability,
            deficit.energy_or_playability,
        ),
    ];
    fields
        .into_iter()
        .filter_map(|(function, level)| is_thin_or_missing(level).then_some(function))
        .collect()
}

fn functions_for_card(card: CardId, upgrades: u8) -> Vec<DeckRepairFunctionV1> {
    use crate::ai::card_semantics_v1::{
        card_reward_semantic_profile_v1, CardRewardSemanticRoleV1 as Role,
    };
    use crate::state::rewards::RewardCard;

    let roles = card_reward_semantic_profile_v1(&RewardCard::new(card, upgrades)).roles;
    let mut functions = Vec::new();
    let mappings = [
        (DeckRepairFunctionV1::Frontload, vec![Role::FrontloadDamage]),
        (DeckRepairFunctionV1::Aoe, vec![Role::AoeDamage]),
        (
            DeckRepairFunctionV1::Block,
            vec![
                Role::Block,
                Role::BlockRetention,
                Role::BlockMultiplier,
                Role::Weak,
                Role::EnemyStrengthDown,
            ],
        ),
        (
            DeckRepairFunctionV1::Scaling,
            vec![
                Role::ScalingSource,
                Role::StrengthPayoff,
                Role::BlockPayoff,
                Role::CardInstanceDamageGrowth,
            ],
        ),
        (
            DeckRepairFunctionV1::Access,
            vec![
                Role::CardDraw,
                Role::CycleAccess,
                Role::DiscardPileTopdeckAccess,
                Role::HandTopdeckSelection,
                Role::ConditionalCardAccess,
                Role::HandCopy,
                Role::TopDeckPlay,
                Role::ExhaustReuse,
            ],
        ),
        (
            DeckRepairFunctionV1::EnergyOrPlayability,
            vec![Role::EnergySource, Role::HpLossCostReduction],
        ),
    ];
    for (function, accepted) in mappings {
        if roles.iter().any(|role| accepted.contains(role)) {
            functions.push(function);
        }
    }
    functions.sort();
    functions.dedup();
    functions
}

#[cfg(test)]
mod tests {
    use super::{deck_repair_profile_v1, DeckRepairUpgradePriorityV1, DeckRepairUpgradeReasonV1};
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::CombatCard;
    use crate::state::run::RunState;

    #[test]
    fn duplicate_low_marginal_function_can_be_a_repair_removal() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.master_deck = vec![
            CombatCard::for_scenario(CardId::Flex, 1),
            CombatCard::for_scenario(CardId::Flex, 2),
            CombatCard::for_scenario(CardId::Bash, 3),
            CombatCard::for_scenario(CardId::ShrugItOff, 4),
        ];

        let profile = deck_repair_profile_v1(&run);

        assert!(profile
            .low_loss_removals
            .iter()
            .any(|item| item.card == CardId::Flex));
    }

    #[test]
    fn singleton_core_function_and_pandora_tag_do_not_create_a_removal() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.master_deck = vec![CombatCard::for_scenario(CardId::Barricade, 1)];
        run.relics.push(RelicState::new(RelicId::PandorasBox));

        let profile = deck_repair_profile_v1(&run);

        assert!(profile.low_loss_removals.is_empty());
        assert_eq!(profile.source_tags, vec!["pandoras_box".to_string()]);
    }

    #[test]
    fn unsupported_card_semantics_do_not_create_repair_removal() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.master_deck = vec![
            CombatCard::for_scenario(CardId::Havoc, 1),
            CombatCard::for_scenario(CardId::Havoc, 2),
        ];

        let profile = deck_repair_profile_v1(&run);

        assert!(profile.low_loss_removals.is_empty());
    }

    #[test]
    fn unupgraded_apparitions_are_explicit_reliability_repairs() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.master_deck = (0..5)
            .map(|index| CombatCard::for_scenario(CardId::Apparition, 100 + index))
            .chain(std::iter::once(CombatCard::for_scenario(
                CardId::Cleave,
                200,
            )))
            .collect();

        let profile = deck_repair_profile_v1(&run);
        let apparition_repairs = profile
            .reliability_upgrades
            .iter()
            .filter(|candidate| candidate.card == CardId::Apparition)
            .collect::<Vec<_>>();

        assert_eq!(apparition_repairs.len(), 5);
        assert!(apparition_repairs.iter().all(|candidate| {
            candidate.priority == DeckRepairUpgradePriorityV1::Reliability
                && candidate
                    .reasons
                    .contains(&DeckRepairUpgradeReasonV1::RetainsTimeSensitiveDefense)
        }));
        assert!(!profile
            .reliability_upgrades
            .iter()
            .any(|candidate| candidate.card == CardId::Cleave));
    }
}
