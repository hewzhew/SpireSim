use std::time::Instant;

use super::super::rollout_scheduler::DeferredRolloutAdmission;
use super::super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum RolloutEstimateSource {
    Root,
    Child,
    DeferredChild,
    TurnPlanSeed,
}

pub(super) fn timed_rollout_estimate(
    rollout_cache: &mut RolloutCache,
    node: &SearchNode,
    stepper: &impl CombatStepper,
    config: &CombatSearchV2Config,
    deadline: Option<Instant>,
    performance: &mut CombatSearchV2PerformanceReport,
    source: RolloutEstimateSource,
    nodes_generated_at_discovery: u64,
) -> RolloutNodeEstimate {
    let started = Instant::now();
    let estimate = rollout_cache.estimate(
        node,
        stepper,
        config,
        deadline,
        nodes_generated_at_discovery,
    );
    performance.rollout_estimate_calls = performance.rollout_estimate_calls.saturating_add(1);
    match source {
        RolloutEstimateSource::Root => {
            performance.root_rollout_estimate_calls =
                performance.root_rollout_estimate_calls.saturating_add(1);
        }
        RolloutEstimateSource::Child => {
            performance.child_rollout_estimate_calls =
                performance.child_rollout_estimate_calls.saturating_add(1);
        }
        RolloutEstimateSource::DeferredChild => {
            performance.deferred_child_rollout_estimate_calls = performance
                .deferred_child_rollout_estimate_calls
                .saturating_add(1);
        }
        RolloutEstimateSource::TurnPlanSeed => {
            performance.turn_plan_seed_rollout_estimate_calls = performance
                .turn_plan_seed_rollout_estimate_calls
                .saturating_add(1);
        }
    }
    performance.rollout_estimate_elapsed_us = performance
        .rollout_estimate_elapsed_us
        .saturating_add(started.elapsed().as_micros());
    estimate
}

pub(super) fn observe_deferred_rollout_admission(
    admission: DeferredRolloutAdmission,
    performance: &mut CombatSearchV2PerformanceReport,
) {
    match admission {
        DeferredRolloutAdmission::AdmitSignal => {
            performance.deferred_child_rollout_admitted_signal = performance
                .deferred_child_rollout_admitted_signal
                .saturating_add(1);
        }
        DeferredRolloutAdmission::AdmitPeriodic => {
            performance.deferred_child_rollout_admitted_periodic = performance
                .deferred_child_rollout_admitted_periodic
                .saturating_add(1);
        }
        DeferredRolloutAdmission::SkipLowSignal => {
            performance.deferred_child_rollout_skipped_low_signal = performance
                .deferred_child_rollout_skipped_low_signal
                .saturating_add(1);
        }
        DeferredRolloutAdmission::SkipBudgetShare => {
            performance.deferred_child_rollout_skipped_budget_share = performance
                .deferred_child_rollout_skipped_budget_share
                .saturating_add(1);
        }
    }
}
