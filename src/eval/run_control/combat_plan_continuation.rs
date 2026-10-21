//! Bounded, private continuation evidence for a supplied combat frontier.
//! Selection remains with the strategic owner; this module never ranks roots.

use serde::{Deserialize, Serialize};

use super::{
    privileged_reference_task_actor_v1, CombatResearchPlanOptionsV1, ExactBranchingTaskWorldV1,
    PrivilegedReferenceTaskActorV1,
};
use crate::agent::task::*;

pub const COMBAT_PLAN_CONTINUATION_PRODUCER_V1: &str = "combat_plan_next_encounter_evidence";
pub const COMBAT_PLAN_CONTINUATION_REVISION_V1: u32 = 1;

/// A batch-wide envelope. Search internals have separate, typed work limits
/// in `combat_search`; they are not counted as task commands. Each started
/// search receives those same limits. This is not a hard wall-clock deadline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPlanContinuationOptionsV1 {
    pub max_task_commands: u64,
    pub max_combat_searches: usize,
    pub max_endpoints: usize,
    pub combat_search: CombatResearchPlanOptionsV1,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPlanContinuationWorkV1 {
    pub task_commands_issued: u64,
    pub combat_searches_started: usize,
    pub endpoints_replayed: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatPlanContinuationLimitV1 {
    TaskCommands,
    CombatSearches,
    Endpoints,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatPlanContinuationStopV1 {
    /// Every member of this *sampled* next-combat frontier was replayed.
    /// Search completeness is still described by the retained attempt lanes.
    SampledFrontierReplayed,
    BudgetExhausted {
        limit: CombatPlanContinuationLimitV1,
    },
    NoWitness {
        search_exit: TaskResearchPlanAttemptExitV1,
    },
    PolicyUnavailable {
        site: TaskDecisionSiteV1,
        reason: String,
    },
    ExecutionStopped {
        outcome: TaskEpisodeOutcomeV1,
    },
    ProtocolError {
        message: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPlanContinuationEndpointV1 {
    pub next_candidate_ordinal: usize,
    /// Actual replayed observation, including recent history. It is not an
    /// outcome score and does not claim equivalence of hidden future state.
    pub decision: TaskPublicDecisionV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPlanContinuationBranchV1 {
    pub root_candidate_ordinal: usize,
    pub after_root_combat: Option<TaskPublicDecisionV1>,
    pub between_combat_steps: Vec<TaskResearchPlanStepV1>,
    pub strategic_support: Vec<TaskPrivilegedStrategicSupportV1>,
    pub next_combat: Option<TaskPublicDecisionV1>,
    pub next_attempt: Option<TaskResearchPlanAttemptV1>,
    pub next_frontier: Vec<usize>,
    pub endpoints: Vec<CombatPlanContinuationEndpointV1>,
    /// Present when an interrupted private fork still has a decision boundary.
    pub stop_boundary: Option<TaskPublicDecisionV1>,
    pub stop: CombatPlanContinuationStopV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPlanContinuationReportV1 {
    pub producer: TaskPrivilegedSupportProvenanceV1,
    pub continuation_actor: TaskActorIdentityV1,
    pub root_execution_budget: TaskBudgetStateV1,
    pub root: TaskPublicDecisionV1,
    pub root_support: TaskPrivilegedCombatResearchSupportV1,
    pub options: CombatPlanContinuationOptionsV1,
    pub work: CombatPlanContinuationWorkV1,
    pub branches: Vec<CombatPlanContinuationBranchV1>,
    /// Every omitted root is explicit. Iteration order is scheduling only.
    pub unstarted_root_candidate_ordinals: Vec<usize>,
    pub interruption: Option<CombatPlanContinuationStopV1>,
}

/// Evaluates the supplied strict frontier through the next combat's reward
/// boundary. Between combats the existing reference policy supplies choices;
/// at the next combat all sampled frontier plans remain alternatives.
///
/// Only private forks change. No root is adopted, public resources do not
/// merge execution states, and no missing witness is treated as defeat.
pub fn evaluate_combat_plan_continuations_v1(
    root: &NodeReloadTaskEnvironmentV1<ExactBranchingTaskWorldV1>,
    support: &TaskPrivilegedCombatResearchSupportV1,
    options: &CombatPlanContinuationOptionsV1,
) -> Result<CombatPlanContinuationReportV1, String> {
    evaluate_with(
        root,
        support,
        options,
        |world| world.fork_for_hot_run_play_v1(),
        |env, options| {
            env.inspect_private_research_execution_v1(|world| {
                world
                    .combat_research_portfolio_for_run_play_v1(Some(options))
                    .map(|p| p.attempt)
            })
        },
    )
}

struct Ledger<'a> {
    options: &'a CombatPlanContinuationOptionsV1,
    work: CombatPlanContinuationWorkV1,
}
impl Ledger<'_> {
    fn limit(&self) -> Option<CombatPlanContinuationLimitV1> {
        if self.work.task_commands_issued >= self.options.max_task_commands {
            Some(CombatPlanContinuationLimitV1::TaskCommands)
        } else if self.work.combat_searches_started >= self.options.max_combat_searches {
            Some(CombatPlanContinuationLimitV1::CombatSearches)
        } else if self.work.endpoints_replayed >= self.options.max_endpoints {
            Some(CombatPlanContinuationLimitV1::Endpoints)
        } else {
            None
        }
    }
    fn command(&mut self) -> Result<(), CombatPlanContinuationStopV1> {
        if self.work.task_commands_issued >= self.options.max_task_commands {
            return Err(exhausted(CombatPlanContinuationLimitV1::TaskCommands));
        }
        self.work.task_commands_issued += 1;
        Ok(())
    }
}
fn exhausted(limit: CombatPlanContinuationLimitV1) -> CombatPlanContinuationStopV1 {
    CombatPlanContinuationStopV1::BudgetExhausted { limit }
}
fn protocol(message: impl ToString) -> CombatPlanContinuationStopV1 {
    CombatPlanContinuationStopV1::ProtocolError {
        message: message.to_string(),
    }
}
fn current<W: TaskWorldV1>(
    env: &NodeReloadTaskEnvironmentV1<W>,
) -> Result<TaskPublicDecisionV1, CombatPlanContinuationStopV1> {
    match env.actor_view().boundary {
        TaskActorBoundaryV1::Decision { decision } => Ok(decision),
        TaskActorBoundaryV1::Terminal { outcome } => {
            Err(CombatPlanContinuationStopV1::ExecutionStopped { outcome })
        }
        _ => Err(protocol("continuation expected a public decision")),
    }
}
fn batch_stop(stop: &CombatPlanContinuationStopV1) -> bool {
    matches!(
        stop,
        CombatPlanContinuationStopV1::BudgetExhausted { .. }
            | CombatPlanContinuationStopV1::ProtocolError { .. }
            | CombatPlanContinuationStopV1::ExecutionStopped {
                outcome: TaskEpisodeOutcomeV1::Failure {
                    reason: TaskEpisodeFailureV1::MechanicsOrProtocolError { .. },
                    ..
                }
            }
    )
}

fn evaluate_with<W, F, S>(
    root: &NodeReloadTaskEnvironmentV1<W>,
    support: &TaskPrivilegedCombatResearchSupportV1,
    options: &CombatPlanContinuationOptionsV1,
    fork_world: F,
    mut search: S,
) -> Result<CombatPlanContinuationReportV1, String>
where
    W: TaskPrivilegedSupportWorldV1,
    F: Fn(&W) -> Result<W, String>,
    S: FnMut(
        &NodeReloadTaskEnvironmentV1<W>,
        &CombatResearchPlanOptionsV1,
    ) -> Result<TaskResearchPlanAttemptV1, String>,
{
    options.combat_search.validate()?;
    support.validate(&root.actor_view())?;
    if root.actor_view().reload_permission != Some(ReloadPermissionV1::ReloadToExactScopeRoot) {
        return Err("combat continuation requires a private exact research scope".to_string());
    }
    let mut report = CombatPlanContinuationReportV1 {
        producer: TaskPrivilegedSupportProvenanceV1 {
            access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
            producer_name: COMBAT_PLAN_CONTINUATION_PRODUCER_V1.to_string(),
            producer_revision: COMBAT_PLAN_CONTINUATION_REVISION_V1,
        },
        continuation_actor: privileged_reference_task_actor_v1().identity(),
        root_execution_budget: root.actor_view().budget,
        root: current(root).map_err(|e| format!("invalid continuation root: {e:?}"))?,
        root_support: support.clone(),
        options: options.clone(),
        work: Default::default(),
        branches: Vec::new(),
        unstarted_root_candidate_ordinals: Vec::new(),
        interruption: None,
    };
    let mut ledger = Ledger {
        options,
        work: Default::default(),
    };
    for (index, ordinal) in support
        .non_dominated_candidate_ordinals
        .iter()
        .copied()
        .enumerate()
    {
        if let Some(limit) = ledger.limit() {
            report.interruption = Some(exhausted(limit));
            report.unstarted_root_candidate_ordinals =
                support.non_dominated_candidate_ordinals[index..].to_vec();
            break;
        }
        let mut branch = CombatPlanContinuationBranchV1 {
            root_candidate_ordinal: ordinal,
            after_root_combat: None,
            between_combat_steps: Vec::new(),
            strategic_support: Vec::new(),
            next_combat: None,
            next_attempt: None,
            next_frontier: Vec::new(),
            endpoints: Vec::new(),
            stop_boundary: None,
            stop: CombatPlanContinuationStopV1::SampledFrontierReplayed,
        };
        if let Err(stop) = evaluate_branch(
            root,
            &support.attempt.candidates[ordinal].plan,
            &fork_world,
            &mut search,
            &mut ledger,
            &mut branch,
        ) {
            branch.stop = stop;
        }
        let abort = batch_stop(&branch.stop);
        if abort {
            report.interruption = Some(branch.stop.clone());
            report.unstarted_root_candidate_ordinals =
                support.non_dominated_candidate_ordinals[index + 1..].to_vec();
        }
        report.branches.push(branch);
        if abort {
            break;
        }
    }
    report.work = ledger.work;
    Ok(report)
}

fn replay<W: TaskWorldV1>(
    env: &mut NodeReloadTaskEnvironmentV1<W>,
    plan: &TaskResearchPlanV1,
    ledger: &mut Ledger<'_>,
) -> Result<TaskPublicDecisionV1, CombatPlanContinuationStopV1> {
    plan.validate_for_root(&current(env)?).map_err(protocol)?;
    for step in &plan.steps {
        let decision = current(env)?;
        if decision.site != TaskDecisionSiteV1::Combat
            || decision.information.snapshot_id != step.source_public_snapshot_id
        {
            return Err(protocol(
                "combat continuation plan changed its source boundary",
            ));
        }
        let command = env
            .decision_handle()
            .map_err(protocol)?
            .into_command(step.choice.clone());
        ledger.command()?;
        env.explore_choice(command).map_err(protocol)?;
    }
    // Do not promote a legal but partial combat prefix into a full endpoint.
    if let TaskActorBoundaryV1::Decision { decision } = env.actor_view().boundary {
        if decision.site == TaskDecisionSiteV1::Combat {
            return Err(protocol(
                "combat continuation plan ended before combat completion",
            ));
        }
    }
    if matches!(
        env.actor_view().boundary,
        TaskActorBoundaryV1::Terminal { .. }
    ) {
        return current(env);
    }
    ledger.command()?;
    env.commit_current_attempt().map_err(protocol)?;
    current(env)
}

fn evaluate_branch<W, F, S>(
    root: &NodeReloadTaskEnvironmentV1<W>,
    plan: &TaskResearchPlanV1,
    fork_world: &F,
    search: &mut S,
    ledger: &mut Ledger<'_>,
    branch: &mut CombatPlanContinuationBranchV1,
) -> Result<(), CombatPlanContinuationStopV1>
where
    W: TaskPrivilegedSupportWorldV1,
    F: Fn(&W) -> Result<W, String>,
    S: FnMut(
        &NodeReloadTaskEnvironmentV1<W>,
        &CombatResearchPlanOptionsV1,
    ) -> Result<TaskResearchPlanAttemptV1, String>,
{
    let mut env = root
        .fork_private_research_execution_v1(fork_world)
        .map_err(protocol)?;
    branch.after_root_combat = Some(replay(&mut env, plan, ledger).inspect_err(|_| {
        branch.stop_boundary = current(&env).ok();
    })?);
    let mut actor = BetweenCombats {
        reference: privileged_reference_task_actor_v1(),
        ledger,
        steps: Vec::new(),
        stop: None,
    };
    let episode =
        run_task_privileged_support_actor_episode_v1(&mut env, &mut actor).map_err(protocol);
    branch.between_combat_steps = actor.steps;
    branch.strategic_support = actor.reference.strategic_support().to_vec();
    let stop = actor.stop;
    let episode = episode.inspect_err(|_| {
        branch.stop_boundary = current(&env).ok();
    })?;
    if let Some(stop) = stop {
        branch.stop_boundary = current(&env).ok();
        return Err(stop);
    }
    if !matches!(episode.outcome, TaskEpisodeOutcomeV1::Paused { .. }) {
        return Err(CombatPlanContinuationStopV1::ExecutionStopped {
            outcome: episode.outcome,
        });
    }
    let next = current(&env)?;
    if next.site != TaskDecisionSiteV1::Combat {
        return Err(protocol("continuation yielded before the next combat"));
    }
    branch.next_combat = Some(next.clone());
    if let Some(limit) = ledger.limit() {
        return Err(exhausted(limit));
    }
    ledger.work.combat_searches_started += 1;
    let attempt = search(&env, &ledger.options.combat_search).map_err(protocol)?;
    branch.next_attempt = Some(attempt.clone());
    attempt.validate().map_err(protocol)?;
    for candidate in &attempt.candidates {
        candidate.plan.validate_for_root(&next).map_err(protocol)?;
    }
    let frontier = task_research_plan_non_dominated_candidate_ordinals_v1(&attempt.candidates);
    branch.next_frontier = frontier.clone();
    if frontier.is_empty() {
        return Err(CombatPlanContinuationStopV1::NoWitness {
            search_exit: attempt.summary_exit(),
        });
    }
    for ordinal in frontier {
        if ledger.work.task_commands_issued >= ledger.options.max_task_commands {
            return Err(exhausted(CombatPlanContinuationLimitV1::TaskCommands));
        }
        if ledger.work.endpoints_replayed >= ledger.options.max_endpoints {
            return Err(exhausted(CombatPlanContinuationLimitV1::Endpoints));
        }
        let mut ending = env
            .fork_private_research_execution_v1(fork_world)
            .map_err(protocol)?;
        let decision =
            replay(&mut ending, &attempt.candidates[ordinal].plan, ledger).inspect_err(|_| {
                branch.stop_boundary = current(&ending).ok();
            })?;
        ledger.work.endpoints_replayed += 1;
        branch.endpoints.push(CombatPlanContinuationEndpointV1 {
            next_candidate_ordinal: ordinal,
            decision,
        });
    }
    Ok(())
}

struct BetweenCombats<'a, 'b> {
    reference: PrivilegedReferenceTaskActorV1,
    ledger: &'a mut Ledger<'b>,
    steps: Vec<TaskResearchPlanStepV1>,
    stop: Option<CombatPlanContinuationStopV1>,
}
impl BetweenCombats<'_, '_> {
    fn at_combat(view: &TaskActorViewV1) -> bool {
        matches!(&view.boundary, TaskActorBoundaryV1::Decision { decision } if decision.site == TaskDecisionSiteV1::Combat)
    }
}
impl TaskActorV1 for BetweenCombats<'_, '_> {
    fn identity(&self) -> TaskActorIdentityV1 {
        self.reference.identity()
    }
    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        if Self::at_combat(view) {
            return TaskActorIntentV1::YieldControl;
        }
        if self.ledger.work.task_commands_issued >= self.ledger.options.max_task_commands {
            self.stop = Some(exhausted(CombatPlanContinuationLimitV1::TaskCommands));
            return TaskActorIntentV1::YieldControl;
        }
        let intent = self.reference.next_intent(view);
        if let TaskActorIntentV1::UnsupportedDecisionSurface { reason } = intent {
            let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
                self.stop = Some(protocol(reason));
                return TaskActorIntentV1::YieldControl;
            };
            self.stop = Some(CombatPlanContinuationStopV1::PolicyUnavailable {
                site: decision.site,
                reason,
            });
            return TaskActorIntentV1::YieldControl;
        }
        if matches!(
            intent,
            TaskActorIntentV1::Act { .. } | TaskActorIntentV1::CommitScope
        ) {
            self.ledger.work.task_commands_issued += 1;
        }
        if let (TaskActorBoundaryV1::Decision { decision }, TaskActorIntentV1::Act { choice }) =
            (&view.boundary, &intent)
        {
            self.steps.push(TaskResearchPlanStepV1 {
                source_public_snapshot_id: decision.information.snapshot_id.clone(),
                choice: choice.clone(),
            });
        }
        intent
    }
}
impl TaskPrivilegedSupportActorV1 for BetweenCombats<'_, '_> {
    fn needs_privileged_support(&mut self, view: &TaskActorViewV1) -> bool {
        !Self::at_combat(view)
            && self.ledger.work.task_commands_issued < self.ledger.options.max_task_commands
            && self.reference.needs_privileged_support(view)
    }
    fn receive_privileged_support(
        &mut self,
        view: &TaskActorViewV1,
        support: TaskPrivilegedSupportV1,
    ) -> Result<(), String> {
        self.reference.receive_privileged_support(view, support)
    }
}

#[cfg(test)]
#[path = "combat_plan_continuation/tests.rs"]
mod tests;
