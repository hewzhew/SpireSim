use serde::{Deserialize, Serialize};

use super::{
    ReloadPermissionV1, TaskActorBoundaryV1, TaskActorIdentityV1, TaskActorInformationAccessV1,
    TaskActorIntentV1, TaskActorV1, TaskActorViewV1, TaskExploredScopeExitV1,
    TaskProposalInformationClassV1, TaskProposalProvenanceV1, TaskPublicDecisionV1,
    TaskPublicStepV1, TaskScopeOrdinalV1,
};
use crate::ai::planner_core::stable_planner_id;

/// Public identity of one closed-loop policy candidate. The candidate is not
/// a root action: two candidates may deliberately choose the same first
/// action and diverge only after observing later public boundaries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskClosedLoopActorCandidateDescriptorV1 {
    pub candidate_id: String,
    pub candidate_key: String,
    pub root_public_snapshot_id: String,
    pub provenance: TaskProposalProvenanceV1,
    pub actor: TaskActorIdentityV1,
}

/// One independently stateful actor candidate. Its private actor state follows
/// only this trial and is never serialized into task evidence.
pub struct TaskClosedLoopActorCandidateV1<A> {
    descriptor: TaskClosedLoopActorCandidateDescriptorV1,
    actor: A,
    replay_prefix_steps: fn(&A, &[TaskPublicStepV1]) -> Result<usize, String>,
}

fn replay_complete_trial_v1<A>(_actor: &A, steps: &[TaskPublicStepV1]) -> Result<usize, String> {
    Ok(steps.len())
}

impl<A> TaskClosedLoopActorCandidateV1<A>
where
    A: TaskActorV1,
{
    pub fn new_public(
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
        candidate_key: impl Into<String>,
        provenance: TaskProposalProvenanceV1,
        actor: A,
    ) -> Result<Self, String> {
        Self::new_public_with_replay_prefix(
            view,
            decision,
            candidate_key,
            provenance,
            actor,
            replay_complete_trial_v1::<A>,
        )
    }

    /// Builds an anytime/receding-horizon candidate. Its actor may evaluate a
    /// continuation beyond the prefix that will actually be replayed and
    /// committed. The resolver sees only the candidate's own actor state and
    /// public steps; it cannot inspect the exact world or hidden successors.
    pub fn new_public_with_replay_prefix(
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
        candidate_key: impl Into<String>,
        provenance: TaskProposalProvenanceV1,
        actor: A,
        replay_prefix_steps: fn(&A, &[TaskPublicStepV1]) -> Result<usize, String>,
    ) -> Result<Self, String> {
        decision.validate()?;
        provenance.validate()?;
        if provenance.information_class != TaskProposalInformationClassV1::PublicActorVisible {
            return Err("closed-loop actor candidate must be public-actor-visible".to_string());
        }
        let actor_identity = actor.identity();
        actor_identity.validate()?;
        if actor_identity.information_access != TaskActorInformationAccessV1::TaskActorViewOnly {
            return Err(
                "closed-loop actor candidate must consume only TaskActorViewV1".to_string(),
            );
        }
        let candidate_key = candidate_key.into();
        if candidate_key.trim().is_empty() {
            return Err("closed-loop actor candidate key must not be empty".to_string());
        }
        let memory = view.node_memory.as_ref().ok_or_else(|| {
            "closed-loop actor candidate requires current scope memory".to_string()
        })?;
        let attempt = memory.attempts.last().ok_or_else(|| {
            "closed-loop actor candidate scope has no current attempt".to_string()
        })?;
        if !attempt.steps.is_empty() || attempt.root != *decision {
            return Err(
                "closed-loop actor candidates must bind a fresh exact scope root".to_string(),
            );
        }
        let root_public_snapshot_id = decision.information.snapshot_id.clone();
        let candidate_id = stable_planner_id(
            "task_closed_loop_actor_candidate_v1",
            &(
                root_public_snapshot_id.as_str(),
                candidate_key.as_str(),
                &provenance,
                &actor_identity,
            ),
        )?;
        Ok(Self {
            descriptor: TaskClosedLoopActorCandidateDescriptorV1 {
                candidate_id,
                candidate_key,
                root_public_snapshot_id,
                provenance,
                actor: actor_identity,
            },
            actor,
            replay_prefix_steps,
        })
    }

    pub fn descriptor(&self) -> &TaskClosedLoopActorCandidateDescriptorV1 {
        &self.descriptor
    }

    fn actor_mut(&mut self) -> &mut A {
        &mut self.actor
    }

    fn replay_prefix_steps(&self, steps: &[TaskPublicStepV1]) -> Result<usize, String> {
        let prefix = (self.replay_prefix_steps)(&self.actor, steps)?;
        if prefix == 0 || prefix > steps.len() {
            return Err(format!(
                "closed-loop candidate requested replay prefix {prefix} outside 1..={}",
                steps.len()
            ));
        }
        Ok(prefix)
    }
}

/// Generates a small set of complete policy/controller candidates at one
/// exact root. Every candidate owns its own actor state across the full trial.
pub trait TaskClosedLoopActorCandidateGeneratorV1<A>
where
    A: TaskActorV1,
{
    fn candidates(
        &mut self,
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskClosedLoopActorCandidateV1<A>>, String>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskExactBranchTrialEndV1 {
    /// The comparer stopped an otherwise live trial at its configured action
    /// horizon. This is not a statement made by the candidate controller.
    Decision {
        decision: TaskPublicDecisionV1,
    },
    /// The candidate deliberately ended a non-empty public prefix and asked
    /// its caller to commit that prefix as a new planning scope.
    ControllerStopped {
        decision: TaskPublicDecisionV1,
    },
    ExploredScopeExit {
        exit: TaskExploredScopeExitV1,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExactBranchTrialV1 {
    pub candidate: TaskClosedLoopActorCandidateDescriptorV1,
    pub steps: Vec<TaskPublicStepV1>,
    /// Number of evaluated steps that are replayed before the selected exact
    /// position is committed as the next planning scope.
    pub replay_prefix_steps: usize,
    pub end: TaskExactBranchTrialEndV1,
}

pub trait TaskExactBranchSelectionControllerV1 {
    fn select_trial(&mut self, trials: &[TaskExactBranchTrialV1]) -> Result<usize, String>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskExactBranchSelectionTraceV1 {
    pub scope: TaskScopeOrdinalV1,
    pub trials: Vec<TaskExactBranchTrialV1>,
    pub selected_candidate_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExactBranchPlannerPhaseV1 {
    ReadyToTrial,
    FollowingTrial,
    AwaitingReload,
    ReplayingSelected { next_step: usize },
}

struct ActiveExactBranchScopeV1<F> {
    scope: TaskScopeOrdinalV1,
    root_snapshot_id: String,
    candidates: Vec<TaskClosedLoopActorCandidateV1<F>>,
    next_trial: usize,
    trials: Vec<TaskExactBranchTrialV1>,
    selected: Option<usize>,
    phase: ExactBranchPlannerPhaseV1,
}

/// Bounded comparison of independently stateful public policy candidates over
/// an explicitly exact-reload task scope. The planner records only
/// actor-visible steps, restores the common exact root after every trial, then
/// replays and commits exactly one selected trajectory.
pub struct TaskExactBranchPlannerActorV1<G, A, F, C> {
    identity: TaskActorIdentityV1,
    generator: G,
    fallback: F,
    controller: C,
    max_actions_per_trial: usize,
    max_candidates: usize,
    active: Option<ActiveExactBranchScopeV1<A>>,
    selection_traces: Vec<TaskExactBranchSelectionTraceV1>,
}

impl<G, A, F, C> TaskExactBranchPlannerActorV1<G, A, F, C> {
    pub fn new(
        identity: TaskActorIdentityV1,
        generator: G,
        fallback: F,
        controller: C,
        max_actions_per_trial: usize,
        max_candidates: usize,
    ) -> Result<Self, String> {
        if max_actions_per_trial == 0 {
            return Err("exact branch planner requires a positive action horizon".to_string());
        }
        if max_candidates < 2 {
            return Err(
                "exact branch planner requires at least two controller candidates".to_string(),
            );
        }
        Ok(Self {
            identity,
            generator,
            fallback,
            controller,
            max_actions_per_trial,
            max_candidates,
            active: None,
            selection_traces: Vec::new(),
        })
    }

    pub fn selection_traces(&self) -> &[TaskExactBranchSelectionTraceV1] {
        &self.selection_traces
    }

    fn unsupported(reason: impl Into<String>) -> TaskActorIntentV1 {
        TaskActorIntentV1::UnsupportedDecisionSurface {
            reason: reason.into(),
        }
    }

    fn current_steps(view: &TaskActorViewV1) -> Result<&[TaskPublicStepV1], String> {
        Ok(&view
            .node_memory
            .as_ref()
            .ok_or_else(|| "exact branch planner received no scope memory".to_string())?
            .attempts
            .last()
            .ok_or_else(|| "exact branch planner scope has no current attempt".to_string())?
            .steps)
    }

    /// A policy candidate receives its own public prefix memory, not abandoned
    /// attempts produced earlier by another candidate in the same comparison.
    fn isolated_candidate_view(
        view: &TaskActorViewV1,
        max_actions_per_trial: usize,
    ) -> TaskActorViewV1 {
        let mut isolated = view.clone();
        if let Some(memory) = isolated.node_memory.as_mut() {
            if let Some(current) = memory.attempts.last().cloned() {
                let mut current = current;
                current.attempt = super::TaskAttemptOrdinalV1(0);
                let actions_used = current.steps.len();
                memory.attempts = vec![current];
                isolated.budget.remaining_world_transitions = max_actions_per_trial
                    .saturating_sub(actions_used)
                    .try_into()
                    .unwrap_or(u64::MAX);
            }
        }
        isolated.budget.remaining_reloads = 0;
        isolated.reload_permission = Some(ReloadPermissionV1::NoReload {
            reason: super::NoReloadReasonV1::DisabledByTask,
        });
        isolated
    }

    fn trial_end(view: &TaskActorViewV1) -> Result<TaskExactBranchTrialEndV1, String> {
        match &view.boundary {
            TaskActorBoundaryV1::Decision { decision } => Ok(TaskExactBranchTrialEndV1::Decision {
                decision: decision.clone(),
            }),
            TaskActorBoundaryV1::ExploredScopeExit { exit } => {
                Ok(TaskExactBranchTrialEndV1::ExploredScopeExit { exit: exit.clone() })
            }
            TaskActorBoundaryV1::CommitReady => {
                Err("exact branch planner reached a native commit-ready boundary".to_string())
            }
            TaskActorBoundaryV1::Terminal { .. } => {
                Err("task runner invoked exact branch planner after terminal".to_string())
            }
        }
    }

    fn boundary_matches_step(boundary: &TaskActorBoundaryV1, step: &TaskPublicStepV1) -> bool {
        match (&step.observed_boundary, boundary) {
            (
                super::TaskObservedBoundaryV1::Decision { decision: expected },
                TaskActorBoundaryV1::Decision { decision },
            ) => expected == decision,
            (
                super::TaskObservedBoundaryV1::ExploredScopeExit { exit: expected },
                TaskActorBoundaryV1::ExploredScopeExit { exit },
            ) => expected == exit,
            _ => false,
        }
    }
}

impl<G, A, F, C> TaskActorV1 for TaskExactBranchPlannerActorV1<G, A, F, C>
where
    G: TaskClosedLoopActorCandidateGeneratorV1<A>,
    A: TaskActorV1,
    F: TaskActorV1,
    C: TaskExactBranchSelectionControllerV1,
{
    fn identity(&self) -> TaskActorIdentityV1 {
        self.identity.clone()
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        let Some(memory) = view.node_memory.as_ref() else {
            return Self::unsupported("exact branch planner received no active scope");
        };
        if !matches!(
            view.reload_permission,
            Some(ReloadPermissionV1::ReloadToExactScopeRoot)
        ) {
            return self.fallback.next_intent(view);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.scope != memory.scope)
        {
            self.active = None;
        }

        if view.budget.remaining_reloads < 2 && self.active.is_none() {
            return self.fallback.next_intent(view);
        }

        if self.active.is_none() {
            let current = memory
                .attempts
                .last()
                .expect("scope memory always has a current attempt");
            if !current.steps.is_empty() {
                return TaskActorIntentV1::CommitScope;
            }
            let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
                return Self::unsupported("empty exact branch scope is not at a public decision");
            };
            if current.root != *decision {
                return Self::unsupported("exact branch planner requires a freshly rebased root");
            }
            let mut candidates = match self.generator.candidates(view, decision) {
                Ok(candidates) => candidates,
                Err(error) => return Self::unsupported(error),
            };
            let max_trials = usize::try_from(view.budget.remaining_reloads)
                .unwrap_or(usize::MAX)
                .min(candidates.len())
                .min(self.max_candidates);
            candidates.truncate(max_trials);
            let mut ids = std::collections::BTreeSet::new();
            if candidates.len() < 2
                || candidates.iter().any(|candidate| {
                    candidate.descriptor().root_public_snapshot_id
                        != decision.information.snapshot_id
                        || candidate.descriptor().actor != candidate.actor.identity()
                        || !ids.insert(candidate.descriptor().candidate_id.as_str())
                })
            {
                return self.fallback.next_intent(view);
            }
            self.active = Some(ActiveExactBranchScopeV1 {
                scope: memory.scope,
                root_snapshot_id: decision.information.snapshot_id.clone(),
                candidates,
                next_trial: 0,
                trials: Vec::new(),
                selected: None,
                phase: ExactBranchPlannerPhaseV1::ReadyToTrial,
            });
        }

        loop {
            let phase = self.active.as_ref().expect("active exact branch").phase;
            match phase {
                ExactBranchPlannerPhaseV1::ReadyToTrial => {
                    let active = self.active.as_mut().expect("active exact branch");
                    if active.next_trial < active.candidates.len() {
                        active.phase = ExactBranchPlannerPhaseV1::FollowingTrial;
                        continue;
                    }
                    let selected = match self.controller.select_trial(&active.trials) {
                        Ok(selected) if selected < active.trials.len() => selected,
                        Ok(_) => {
                            return Self::unsupported(
                                "exact branch controller selected outside the trial set",
                            )
                        }
                        Err(error) => return Self::unsupported(error),
                    };
                    active.selected = Some(selected);
                    active.phase = ExactBranchPlannerPhaseV1::ReplayingSelected { next_step: 0 };
                    self.selection_traces.push(TaskExactBranchSelectionTraceV1 {
                        scope: active.scope,
                        trials: active.trials.clone(),
                        selected_candidate_id: active.trials[selected]
                            .candidate
                            .candidate_id
                            .clone(),
                    });
                }
                ExactBranchPlannerPhaseV1::FollowingTrial => {
                    let steps = match Self::current_steps(view) {
                        Ok(steps) => steps,
                        Err(error) => return Self::unsupported(error),
                    };
                    let stopped =
                        matches!(view.boundary, TaskActorBoundaryV1::ExploredScopeExit { .. })
                            || steps.len() >= self.max_actions_per_trial;
                    if stopped {
                        let end = match Self::trial_end(view) {
                            Ok(end) => end,
                            Err(error) => return Self::unsupported(error),
                        };
                        let active = self.active.as_mut().expect("active exact branch");
                        let candidate = active.candidates[active.next_trial].descriptor().clone();
                        let replay_prefix_steps =
                            match active.candidates[active.next_trial].replay_prefix_steps(steps) {
                                Ok(prefix) => prefix,
                                Err(error) => return Self::unsupported(error),
                            };
                        active.trials.push(TaskExactBranchTrialV1 {
                            candidate,
                            steps: steps.to_vec(),
                            replay_prefix_steps,
                            end,
                        });
                        active.phase = ExactBranchPlannerPhaseV1::AwaitingReload;
                        return TaskActorIntentV1::Reload;
                    }
                    let active = self.active.as_mut().expect("active exact branch");
                    let candidate_view =
                        Self::isolated_candidate_view(view, self.max_actions_per_trial);
                    match active.candidates[active.next_trial]
                        .actor_mut()
                        .next_intent(&candidate_view)
                    {
                        TaskActorIntentV1::Act { choice } => {
                            return TaskActorIntentV1::Act { choice }
                        }
                        TaskActorIntentV1::CommitScope => {
                            if steps.is_empty() {
                                return Self::unsupported(
                                    "closed-loop candidate cannot stop before taking an action",
                                );
                            }
                            let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
                                return Self::unsupported(
                                    "closed-loop candidate can stop only at a public decision",
                                );
                            };
                            let candidate =
                                active.candidates[active.next_trial].descriptor().clone();
                            let replay_prefix_steps = match active.candidates[active.next_trial]
                                .replay_prefix_steps(steps)
                            {
                                Ok(prefix) => prefix,
                                Err(error) => return Self::unsupported(error),
                            };
                            active.trials.push(TaskExactBranchTrialV1 {
                                candidate,
                                steps: steps.to_vec(),
                                replay_prefix_steps,
                                end: TaskExactBranchTrialEndV1::ControllerStopped {
                                    decision: decision.clone(),
                                },
                            });
                            active.phase = ExactBranchPlannerPhaseV1::AwaitingReload;
                            return TaskActorIntentV1::Reload;
                        }
                        TaskActorIntentV1::UnsupportedDecisionSurface { reason } => {
                            return Self::unsupported(reason)
                        }
                        _ => {
                            return Self::unsupported(
                                "exact branch rollout actor requested control instead of an action",
                            )
                        }
                    }
                }
                ExactBranchPlannerPhaseV1::AwaitingReload => {
                    let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
                        return Self::unsupported("exact branch reload did not restore a decision");
                    };
                    let steps = match Self::current_steps(view) {
                        Ok(steps) => steps,
                        Err(error) => return Self::unsupported(error),
                    };
                    let active = self.active.as_mut().expect("active exact branch");
                    if decision.information.snapshot_id != active.root_snapshot_id
                        || !steps.is_empty()
                    {
                        return Self::unsupported(
                            "exact branch reload did not restore the common empty root",
                        );
                    }
                    active.next_trial += 1;
                    active.phase = ExactBranchPlannerPhaseV1::ReadyToTrial;
                }
                ExactBranchPlannerPhaseV1::ReplayingSelected { next_step } => {
                    let active = self.active.as_mut().expect("active exact branch");
                    let selected = active.selected.expect("selected exact branch trial");
                    let trial = &active.trials[selected];
                    if next_step == trial.replay_prefix_steps {
                        if trial.replay_prefix_steps < trial.steps.len() {
                            return TaskActorIntentV1::CommitScope;
                        }
                        let end_matches = match (&trial.end, &view.boundary) {
                            (
                                TaskExactBranchTrialEndV1::Decision { decision: expected },
                                TaskActorBoundaryV1::Decision { decision },
                            ) => expected == decision,
                            (
                                TaskExactBranchTrialEndV1::ControllerStopped { decision: expected },
                                TaskActorBoundaryV1::Decision { decision },
                            ) => expected == decision,
                            (
                                TaskExactBranchTrialEndV1::ExploredScopeExit { exit: expected },
                                TaskActorBoundaryV1::ExploredScopeExit { exit },
                            ) => expected == exit,
                            _ => false,
                        };
                        if !end_matches {
                            return Self::unsupported(
                                "selected exact branch replay ended at a different boundary",
                            );
                        }
                        return TaskActorIntentV1::CommitScope;
                    }
                    if next_step > 0
                        && !Self::boundary_matches_step(&view.boundary, &trial.steps[next_step - 1])
                    {
                        return Self::unsupported(
                            "selected exact branch replay diverged from its public trajectory",
                        );
                    }
                    let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
                        return Self::unsupported(
                            "selected exact branch replay expected a public decision",
                        );
                    };
                    let step = &trial.steps[next_step];
                    if step.source != *decision {
                        return Self::unsupported(
                            "selected exact branch replay source decision changed",
                        );
                    }
                    active.phase = ExactBranchPlannerPhaseV1::ReplayingSelected {
                        next_step: next_step + 1,
                    };
                    return TaskActorIntentV1::Act {
                        choice: step.choice.clone(),
                    };
                }
            }
        }
    }
}
