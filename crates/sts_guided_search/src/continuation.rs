//! Measured continuation experiments over explicitly sourced combat prefixes.
//! This adapter uses the existing solver as a named evidence producer. It does
//! not turn its ranking, a budget miss, or a reference witness into a label.

use crate::solver::{CombatSolver, SolverConfig, SolverStats, SolverStop};
pub use crate::ResourceRequirement;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use sts_combat_outcome::combat_witness_materialization::{
    materialize_replay_checked_combat_outcome_v1, CombatWitnessMaterializedOutcomeV1,
};
use sts_core::sim::combat::{
    CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
};
use sts_core::state::core::ClientInput;

/// Past-only execution input. Reference suffix, ending resources and distance
/// to a known win belong in a separate evaluation artifact, never this object.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationTaskSpec {
    pub source_producer: String,
    /// All slices of one source family inherit this split group.
    pub group: String,
    pub origin: CombatPosition,
    pub prefix: Vec<ClientInput>,
    pub requirement: ResourceRequirement,
}

pub struct ValidatedContinuationTask {
    spec: ContinuationTaskSpec,
    anchor: CombatPosition,
    id: String,
    validation_engine_steps: usize,
}

impl ValidatedContinuationTask {
    pub fn new(spec: ContinuationTaskSpec, limits: CombatStepLimits) -> Result<Self, String> {
        if limits.max_engine_steps == 0 {
            return Err("prefix validation requires a positive engine-step limit".into());
        }
        if spec.source_producer.trim().is_empty() || spec.group.trim().is_empty() {
            return Err("source producer and split group are required".into());
        }
        spec.requirement.validate(&spec.origin)?;
        let mut anchor = spec.origin.clone();
        let mut validation_engine_steps = 0;
        for input in &spec.prefix {
            let (next, steps) = checked_step(&anchor, input, limits)?;
            anchor = next;
            validation_engine_steps += steps;
        }
        if EngineCombatStepper.terminal(&anchor) != CombatTerminal::Unresolved {
            return Err("continuation anchor must be an unresolved combat".into());
        }
        crate::CombatSearchState::new(anchor.clone())?;
        let id = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&spec).map_err(|e| e.to_string())?)
        );
        Ok(Self {
            spec,
            anchor,
            id,
            validation_engine_steps,
        })
    }

    pub fn spec(&self) -> &ContinuationTaskSpec {
        &self.spec
    }
    pub fn anchor(&self) -> &CombatPosition {
        &self.anchor
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn validation_work(&self) -> usize {
        self.spec.prefix.len()
    }
    pub fn validation_engine_steps(&self) -> usize {
        self.validation_engine_steps
    }
}

fn checked_step(
    position: &CombatPosition,
    input: &ClientInput,
    limits: CombatStepLimits,
) -> Result<(CombatPosition, usize), String> {
    if !EngineCombatStepper.is_legal_action(position, input) {
        return Err("illegal input in replayed combat trajectory".into());
    }
    let step = EngineCombatStepper.apply_to_stable(position, input.clone(), limits);
    if step.truncated || step.timed_out {
        return Err("trajectory validation did not reach a stable boundary".into());
    }
    Ok((step.position, step.engine_steps))
}

#[derive(Clone, Debug, Serialize)]
pub struct ContinuationWitness {
    /// Includes the immutable prefix from the original combat root.
    pub inputs: Vec<ClientInput>,
    pub successor: CombatPosition,
    pub outcome: CombatWitnessMaterializedOutcomeV1,
    pub satisfies_requirement: bool,
    pub found_at_work: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ContinuationStop {
    WorkBudget,
    TimeBudget,
    ExecutorScheduleComplete,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContinuationReceipt {
    pub sequence: usize,
    pub requested_work: usize,
    pub used_work: usize,
    pub cumulative_work: usize,
    pub cumulative_executor_work: usize,
    pub cumulative_origin_replay_work: usize,
    pub cumulative_origin_replay_engine_steps: usize,
    pub executor_stats: SolverStats,
    pub elapsed_micros: u128,
    pub stop: ContinuationStop,
    pub verified_witnesses_added: usize,
    pub new_target_outcomes: usize,
    pub target_frontier: Vec<CombatWitnessMaterializedOutcomeV1>,
    /// False means only "no qualifying witness observed yet".
    pub target_witness_observed: bool,
    pub verification_pending: bool,
}

struct OriginReplay {
    inputs: Vec<ClientInput>,
    expected: CombatPosition,
    current: CombatPosition,
    index: usize,
}

/// One resident executor, resumed by every advance. Reference suffixes are not
/// accepted here. All search work and original-root replay spend the allowance.
pub struct ContinuationExperiment {
    task: ValidatedContinuationTask,
    solver: CombatSolver,
    config: SolverConfig,
    replay: Option<OriginReplay>,
    last_proposal_work: usize,
    replay_work: usize,
    replay_engine_steps: usize,
    witnesses: Vec<ContinuationWitness>,
    frontier: Vec<CombatWitnessMaterializedOutcomeV1>,
    receipts: Vec<ContinuationReceipt>,
    failed: bool,
}

impl ContinuationExperiment {
    /// The executor's potion cap is relative to the anchor. The task's original
    /// slot/HP requirement is a result filter, not a search constraint. Executor
    /// pruning may omit useful results; this adapter does not repair coverage.
    pub fn new(task: ValidatedContinuationTask, config: SolverConfig) -> Result<Self, String> {
        let solver = CombatSolver::new(task.anchor.clone(), config.clone())?;
        Ok(Self {
            task,
            solver,
            config,
            replay: None,
            last_proposal_work: 0,
            replay_work: 0,
            replay_engine_steps: 0,
            witnesses: vec![],
            frontier: vec![],
            receipts: vec![],
            failed: false,
        })
    }

    pub fn task(&self) -> &ValidatedContinuationTask {
        &self.task
    }
    pub fn config(&self) -> &SolverConfig {
        &self.config
    }
    pub fn witnesses(&self) -> &[ContinuationWitness] {
        &self.witnesses
    }
    pub fn receipts(&self) -> &[ContinuationReceipt] {
        &self.receipts
    }
    pub fn work(&self) -> usize {
        self.solver.stats().work() + self.replay_work
    }

    pub fn advance(
        &mut self,
        work: usize,
        duration: Duration,
    ) -> Result<&ContinuationReceipt, String> {
        if self.failed {
            return Err("continuation experiment stopped after an execution error".into());
        }
        // A replay or executor error invalidates this session for further work.
        // Previously verified witnesses remain readable, never a fresh receipt.
        if let Err(error) = self.advance_inner(work, duration) {
            self.failed = true;
            return Err(error);
        }
        Ok(self.receipts.last().unwrap())
    }

    fn advance_inner(&mut self, work: usize, duration: Duration) -> Result<(), String> {
        let start = Instant::now();
        let deadline = start.checked_add(duration).ok_or("invalid duration")?;
        let before_work = self.work();
        let before_witnesses = self.witnesses.len();
        let before_frontier = self.frontier.clone();
        let stop = loop {
            if self.work() - before_work >= work {
                break ContinuationStop::WorkBudget;
            }
            if Instant::now() >= deadline {
                break ContinuationStop::TimeBudget;
            }
            if let Some(replay) = &mut self.replay {
                let input = &replay.inputs[replay.index];
                if !EngineCombatStepper.is_legal_action(&replay.current, input) {
                    return Err("executor proposal failed original-root replay legality".into());
                }
                let step = EngineCombatStepper.apply_to_stable(
                    &replay.current,
                    input.clone(),
                    CombatStepLimits {
                        max_engine_steps: self.config.max_engine_steps_per_input,
                        deadline: Some(deadline),
                    },
                );
                self.replay_engine_steps += step.engine_steps;
                if step.timed_out {
                    break ContinuationStop::TimeBudget;
                }
                if step.truncated {
                    return Err("original-root replay exceeded engine-step limit".into());
                }
                replay.current = step.position;
                replay.index += 1;
                self.replay_work += 1;
                if replay.index == replay.inputs.len() {
                    self.finish_replay()?;
                }
            } else {
                let executor_stop = self
                    .solver
                    .advance(1, deadline.saturating_duration_since(Instant::now()))?;
                // At most one new witness can finish its independent executor
                // replay in one work unit. Do not pass it off as a task result
                // before replaying prefix + suffix from the original root.
                if let Some(plan) = self
                    .solver
                    .plans()
                    .iter()
                    .find(|p| p.found_at_work > self.last_proposal_work)
                {
                    self.last_proposal_work = plan.found_at_work;
                    let mut inputs = self.task.spec.prefix.clone();
                    inputs.extend(plan.inputs.iter().cloned());
                    self.replay = Some(OriginReplay {
                        inputs,
                        expected: plan.successor.clone(),
                        current: self.task.spec.origin.clone(),
                        index: 0,
                    });
                } else if matches!(executor_stop, SolverStop::ScheduleComplete) {
                    break ContinuationStop::ExecutorScheduleComplete;
                } else if matches!(executor_stop, SolverStop::TimeBudget) {
                    break ContinuationStop::TimeBudget;
                }
            }
        };
        let new_target_outcomes = self
            .frontier
            .iter()
            .filter(|new| {
                !before_frontier.iter().any(|old| {
                    old.has_same_materialized_resources_v1(new) || old.strictly_dominates(new)
                })
            })
            .count();
        self.receipts.push(ContinuationReceipt {
            sequence: self.receipts.len(),
            requested_work: work,
            used_work: self.work() - before_work,
            cumulative_work: self.work(),
            cumulative_executor_work: self.solver.stats().work(),
            cumulative_origin_replay_work: self.replay_work,
            cumulative_origin_replay_engine_steps: self.replay_engine_steps,
            executor_stats: self.solver.stats().clone(),
            elapsed_micros: start.elapsed().as_micros(),
            stop,
            verified_witnesses_added: self.witnesses.len() - before_witnesses,
            new_target_outcomes,
            target_frontier: self.frontier.clone(),
            target_witness_observed: !self.frontier.is_empty(),
            verification_pending: self.replay.is_some(),
        });
        Ok(())
    }

    fn finish_replay(&mut self) -> Result<(), String> {
        let replay = self.replay.take().unwrap();
        if replay.current != replay.expected
            || !EngineCombatStepper.is_exact_victory(&replay.current)
        {
            return Err("executor proposal differs in original-root replay".into());
        }
        let outcome = materialize_replay_checked_combat_outcome_v1(
            &self.task.spec.origin,
            &replay.inputs,
            &replay.current,
        )?;
        let satisfies_requirement = self.task.spec.requirement.accepts(&outcome);
        if satisfies_requirement
            && !self.frontier.iter().any(|old| {
                old.has_same_materialized_resources_v1(&outcome) || old.strictly_dominates(&outcome)
            })
        {
            self.frontier.retain(|old| !outcome.strictly_dominates(old));
            self.frontier.push(outcome.clone());
        }
        self.witnesses.push(ContinuationWitness {
            inputs: replay.inputs,
            successor: replay.current,
            outcome,
            satisfies_requirement,
            found_at_work: self.work(),
        });
        Ok(())
    }
}

/// Evaluation-only source validation. Returns exact states after each input;
/// callers choose anchors, then pass only the past prefix to the experiment.
pub fn validate_reference_trajectory(
    origin: &CombatPosition,
    inputs: &[ClientInput],
    expected: &CombatPosition,
    limits: CombatStepLimits,
) -> Result<(Vec<CombatPosition>, usize), String> {
    let mut states = vec![origin.clone()];
    let mut steps = 0;
    for input in inputs {
        let (next, used) = checked_step(states.last().unwrap(), input, limits)?;
        states.push(next);
        steps += used;
    }
    if states.last() != Some(expected) || !EngineCombatStepper.is_exact_victory(expected) {
        return Err("reference trajectory does not reproduce the supplied victory".into());
    }
    Ok((states, steps))
}

#[cfg(test)]
mod tests;
