use std::time::Instant;

use crate::sim::combat::CombatStepper;

use super::oracle_action_policy::OracleCombatRolloutContractV1;
use super::rollout_probe::{choose_by_one_step_probe, OneStepProbeSelection};
use super::rollout_profile::RolloutPerformanceCounters;
use super::*;

pub(super) const ROLLOUT_ACTION_REASON_CONSERVATIVE_ORDERING_FIRST: &str =
    "conservative_policy_selected_first_semantic_ordered_no_potion_action";
pub(super) const ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_PROBE: &str =
    "conservative_policy_selected_bounded_one_step_probe_no_potion_action";
pub(super) const ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_SURVIVAL_VALUE: &str =
    "conservative_policy_selected_bounded_one_step_survival_value_no_potion_action";
pub(super) const ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_PHASE_VALUE: &str =
    "conservative_policy_selected_bounded_one_step_phase_value_no_potion_action";
pub(super) const ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_ACTION_FACTS_VALUE: &str =
    "conservative_policy_selected_bounded_one_step_action_facts_value_no_potion_action";

pub(super) const CONSERVATIVE_ROLLOUT_PROBE_ACTION_LIMIT: usize = 6;

#[derive(Clone, Debug)]
pub(super) struct RolloutPolicySelection {
    pub(super) choice: IndexedActionChoice,
    pub(super) reason: &'static str,
    pub(super) cached_step: Option<crate::sim::combat::CombatStepResult>,
}

pub(super) fn filtered_rollout_legal_actions(
    plugin: CombatSearchRolloutPluginId,
    legal: Vec<CombatActionChoice>,
    combat: &CombatState,
) -> Vec<CombatActionChoice> {
    match plugin {
        CombatSearchRolloutPluginId::Disabled => Vec::new(),
        CombatSearchRolloutPluginId::EnemyMechanicsAdaptiveNoPotion
        | CombatSearchRolloutPluginId::ConservativeNoPotion
        | CombatSearchRolloutPluginId::PhaseAwareNoPotion
        | CombatSearchRolloutPluginId::TurnBeamNoPotion => {
            filtered_legal_actions(legal, CombatSearchV2PotionPolicy::Never, None, None, combat)
        }
    }
}

pub(super) fn choose_rollout_action(
    plugin: CombatSearchRolloutPluginId,
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    engine: &EngineState,
    combat: &CombatState,
    legal: Vec<CombatActionChoice>,
    performance: &mut RolloutPerformanceCounters,
    contract: OracleCombatRolloutContractV1,
) -> Option<RolloutPolicySelection> {
    match plugin {
        CombatSearchRolloutPluginId::Disabled => None,
        CombatSearchRolloutPluginId::EnemyMechanicsAdaptiveNoPotion
        | CombatSearchRolloutPluginId::ConservativeNoPotion => {
            choose_conservative_no_potion_action(
                false,
                node,
                stepper,
                config,
                deadline,
                engine,
                combat,
                legal,
                performance,
                contract,
            )
        }
        CombatSearchRolloutPluginId::PhaseAwareNoPotion => choose_conservative_no_potion_action(
            true,
            node,
            stepper,
            config,
            deadline,
            engine,
            combat,
            legal,
            performance,
            contract,
        ),
        CombatSearchRolloutPluginId::TurnBeamNoPotion => choose_conservative_no_potion_action(
            true,
            node,
            stepper,
            config,
            deadline,
            engine,
            combat,
            legal,
            performance,
            contract,
        ),
    }
}

fn choose_conservative_no_potion_action(
    allow_nonterminal_probe_upgrade: bool,
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    engine: &EngineState,
    combat: &CombatState,
    legal: Vec<CombatActionChoice>,
    performance: &mut RolloutPerformanceCounters,
    contract: OracleCombatRolloutContractV1,
) -> Option<RolloutPolicySelection> {
    let choices = legal
        .into_iter()
        .enumerate()
        .map(|(original_action_id, choice)| IndexedActionChoice {
            original_action_id,
            choice,
        })
        .collect();
    let ordering_started = Instant::now();
    let ordered = order_indexed_action_choices(engine, combat, choices);
    performance.no_potion_choose_ordering_elapsed_us = performance
        .no_potion_choose_ordering_elapsed_us
        .saturating_add(ordering_started.elapsed().as_micros());
    let fallback = ordered.choices.first().cloned()?;
    if ordered.choices.len() == 1 {
        return Some(RolloutPolicySelection {
            choice: fallback,
            reason: ROLLOUT_ACTION_REASON_CONSERVATIVE_ORDERING_FIRST,
            cached_step: None,
        });
    }
    let probe_started = Instant::now();
    let probed = choose_by_one_step_probe(
        node,
        stepper,
        config,
        deadline,
        &ordered.choices,
        allow_nonterminal_probe_upgrade,
        performance,
        contract,
    );
    performance.no_potion_probe_elapsed_us = performance
        .no_potion_probe_elapsed_us
        .saturating_add(probe_started.elapsed().as_micros());
    Some(match probed {
        OneStepProbeSelection::Upgrade {
            choice,
            reason,
            step,
        } => RolloutPolicySelection {
            choice,
            reason,
            cached_step: Some(step),
        },
        OneStepProbeSelection::Fallback { step } => RolloutPolicySelection {
            choice: fallback,
            reason: ROLLOUT_ACTION_REASON_CONSERVATIVE_ORDERING_FIRST,
            cached_step: Some(step),
        },
        OneStepProbeSelection::NoUsableProbe => RolloutPolicySelection {
            choice: fallback,
            reason: ROLLOUT_ACTION_REASON_CONSERVATIVE_ORDERING_FIRST,
            cached_step: None,
        },
    })
}

#[cfg(test)]
mod tests;
