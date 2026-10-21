use std::time::Instant;

use crate::sim::combat::CombatStepper;

use super::oracle_action_policy::OracleCombatRolloutContractV1;
use super::rollout_profile::RolloutPerformanceCounters;
use super::*;

mod score;
mod score_types;
mod upgrade;

use score::{probe_action, probe_terminal_action};
use score_types::{RolloutActionProbeScore, RolloutTerminalProbeScore};
use upgrade::probe_upgrade_reason;

#[derive(Clone, Debug)]
pub(super) enum OneStepProbeSelection {
    Upgrade {
        choice: IndexedActionChoice,
        reason: &'static str,
        step: crate::sim::combat::CombatStepResult,
    },
    Fallback {
        step: crate::sim::combat::CombatStepResult,
    },
    NoUsableProbe,
}

pub(super) fn choose_by_one_step_probe(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    ordered: &[IndexedActionChoice],
    allow_nonterminal_upgrade: bool,
    performance: &mut RolloutPerformanceCounters,
    contract: OracleCombatRolloutContractV1,
) -> OneStepProbeSelection {
    if !allow_nonterminal_upgrade {
        return choose_by_terminal_one_step_probe(
            node,
            stepper,
            config,
            deadline,
            ordered,
            performance,
            contract,
        );
    }
    let Some(fallback_choice) = ordered.first() else {
        return OneStepProbeSelection::NoUsableProbe;
    };
    let Some(fallback_probe) = probe_action(
        node,
        stepper,
        config,
        deadline,
        fallback_choice,
        0,
        performance,
        contract,
    ) else {
        return OneStepProbeSelection::NoUsableProbe;
    };
    let fallback_score = fallback_probe.score;
    let fallback_step = fallback_probe.step;
    let mut best: Option<(
        RolloutActionProbeScore,
        IndexedActionChoice,
        &'static str,
        crate::sim::combat::CombatStepResult,
    )> = None;
    for (ordered_index, choice) in ordered
        .iter()
        .take(super::rollout_action_selector::CONSERVATIVE_ROLLOUT_PROBE_ACTION_LIMIT)
        .skip(1)
        .enumerate()
    {
        let ordered_index = ordered_index + 1;
        let Some(probe) = probe_action(
            node,
            stepper,
            config,
            deadline,
            choice,
            ordered_index,
            performance,
            contract,
        ) else {
            continue;
        };
        let score = probe.score;
        let Some(reason) = probe_upgrade_reason(score, fallback_score, allow_nonterminal_upgrade)
        else {
            continue;
        };
        let replace = best
            .as_ref()
            .map(|(best_score, _, _, _)| score > *best_score)
            .unwrap_or(true);
        if replace {
            best = Some((score, choice.clone(), reason, probe.step));
        }
    }
    match best {
        Some((_, choice, reason, step)) => OneStepProbeSelection::Upgrade {
            choice,
            reason,
            step,
        },
        None => OneStepProbeSelection::Fallback {
            step: fallback_step,
        },
    }
}

fn choose_by_terminal_one_step_probe(
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    ordered: &[IndexedActionChoice],
    performance: &mut RolloutPerformanceCounters,
    contract: OracleCombatRolloutContractV1,
) -> OneStepProbeSelection {
    let Some(fallback_choice) = ordered.first() else {
        return OneStepProbeSelection::NoUsableProbe;
    };
    let Some(fallback_probe) = probe_terminal_action(
        node,
        stepper,
        config,
        deadline,
        fallback_choice,
        0,
        performance,
        contract,
    ) else {
        return OneStepProbeSelection::NoUsableProbe;
    };
    let fallback_score = fallback_probe.score;
    let fallback_step = fallback_probe.step;
    let mut best: Option<(
        RolloutTerminalProbeScore,
        IndexedActionChoice,
        crate::sim::combat::CombatStepResult,
    )> = None;
    for (ordered_index, choice) in ordered
        .iter()
        .take(super::rollout_action_selector::CONSERVATIVE_ROLLOUT_PROBE_ACTION_LIMIT)
        .skip(1)
        .enumerate()
    {
        let ordered_index = ordered_index + 1;
        let Some(probe) = probe_terminal_action(
            node,
            stepper,
            config,
            deadline,
            choice,
            ordered_index,
            performance,
            contract,
        ) else {
            continue;
        };
        if probe.score.terminal_rank <= fallback_score.terminal_rank {
            continue;
        }
        let replace = best
            .as_ref()
            .map(|(best_score, _, _)| probe.score > *best_score)
            .unwrap_or(true);
        if replace {
            best = Some((probe.score, choice.clone(), probe.step));
        }
    }
    match best {
        Some((_, choice, step)) => OneStepProbeSelection::Upgrade {
            choice,
            reason:
                super::rollout_action_selector::ROLLOUT_ACTION_REASON_CONSERVATIVE_ONE_STEP_PROBE,
            step,
        },
        None => OneStepProbeSelection::Fallback {
            step: fallback_step,
        },
    }
}

#[cfg(test)]
#[path = "../rollout_probe_tests.rs"]
mod tests;
