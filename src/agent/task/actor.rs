use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use super::{
    NodeReloadTaskEnvironmentV1, ReloadPermissionV1, TaskActorBoundaryV1, TaskActorViewV1,
    TaskCommandErrorV1, TaskEpisodeOutcomeV1, TaskPublicChoiceV1, TaskResearchRunBranchPointV1,
    TaskWorldV1,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskActorInformationAccessV1 {
    TaskActorViewOnly,
    TaskActorViewAndPrivilegedSupport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskActorIdentityV1 {
    pub actor_name: String,
    pub actor_revision: u32,
    pub information_access: TaskActorInformationAccessV1,
}

impl TaskActorIdentityV1 {
    pub fn task_actor_view_only(actor_name: impl Into<String>, actor_revision: u32) -> Self {
        Self {
            actor_name: actor_name.into(),
            actor_revision,
            information_access: TaskActorInformationAccessV1::TaskActorViewOnly,
        }
    }

    pub fn privileged_support_assisted(actor_name: impl Into<String>, actor_revision: u32) -> Self {
        Self {
            actor_name: actor_name.into(),
            actor_revision,
            information_access: TaskActorInformationAccessV1::TaskActorViewAndPrivilegedSupport,
        }
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if self.actor_name.trim().is_empty() {
            return Err("task actor identity has an empty name".to_string());
        }
        if self.actor_revision == 0 {
            return Err("task actor identity revision must be positive".to_string());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "intent", rename_all = "snake_case")]
pub enum TaskActorIntentV1 {
    Act { choice: TaskPublicChoiceV1 },
    Reload,
    CommitScope,
    YieldControl,
    UnsupportedDecisionSurface { reason: String },
}

impl TaskActorIntentV1 {
    fn kind(&self) -> TaskActorIntentKindV1 {
        match self {
            Self::Act { .. } => TaskActorIntentKindV1::Act,
            Self::Reload => TaskActorIntentKindV1::Reload,
            Self::CommitScope => TaskActorIntentKindV1::CommitScope,
            Self::YieldControl => TaskActorIntentKindV1::YieldControl,
            Self::UnsupportedDecisionSurface { .. } => {
                TaskActorIntentKindV1::UnsupportedDecisionSurface
            }
        }
    }
}

/// A deployed task actor receives only the serializable public task view.
/// Native anchors, exact checkpoints, and private action resolution remain in
/// the execution shell.
pub trait TaskActorV1 {
    fn identity(&self) -> TaskActorIdentityV1;

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1;
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskActorControlBoundaryV1 {
    Decision,
    CommitReady,
    ExploredScopeExit,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskActorIntentKindV1 {
    Act,
    Reload,
    CommitScope,
    YieldControl,
    UnsupportedDecisionSurface,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskActorRunErrorV1 {
    InvalidActorIdentity(String),
    InvalidIntent {
        boundary: TaskActorControlBoundaryV1,
        intent: TaskActorIntentKindV1,
    },
    Environment(TaskCommandErrorV1),
    PrivilegedSupport(String),
    RunBranchCapture(String),
}

impl Display for TaskActorRunErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for TaskActorRunErrorV1 {}

impl From<TaskCommandErrorV1> for TaskActorRunErrorV1 {
    fn from(error: TaskCommandErrorV1) -> Self {
        Self::Environment(error)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskActorEpisodeV1 {
    pub actor: TaskActorIdentityV1,
    pub actor_turns: u64,
    pub act_intents: u64,
    pub reload_intents: u32,
    pub commit_intents: u64,
    pub unsupported_reason: Option<String>,
    pub outcome: TaskEpisodeOutcomeV1,
    pub final_view: TaskActorViewV1,
}

/// Drives one complete task episode without exposing the private world to the
/// actor. The environment, rather than the actor, decides whether an `Act`
/// executes provisionally inside a reloadable scope or immediately on a
/// fail-closed committed line.
pub fn run_task_actor_episode_v1<W, A>(
    environment: &mut NodeReloadTaskEnvironmentV1<W>,
    actor: &mut A,
) -> Result<TaskActorEpisodeV1, TaskActorRunErrorV1>
where
    W: TaskWorldV1,
    A: TaskActorV1,
{
    run_task_actor_episode_with_pre_turn_v1(
        environment,
        actor,
        TaskActorInformationAccessV1::TaskActorViewOnly,
        |_, _, _| Ok(()),
    )
}

/// Runs an explicitly privileged-support actor. Support construction stays
/// inside the private world; the actor receives only public ordinal choices
/// or replay-verified public combat plans with explicit provenance.
pub fn run_task_privileged_support_actor_episode_v1<W, A>(
    environment: &mut NodeReloadTaskEnvironmentV1<W>,
    actor: &mut A,
) -> Result<TaskActorEpisodeV1, TaskActorRunErrorV1>
where
    W: super::TaskPrivilegedSupportWorldV1,
    A: super::TaskPrivilegedSupportActorV1,
{
    run_task_actor_episode_with_pre_turn_v1(
        environment,
        actor,
        TaskActorInformationAccessV1::TaskActorViewAndPrivilegedSupport,
        deliver_privileged_support_or_fail_v1,
    )
}

/// Runs a privileged-support episode while privately capturing every fresh
/// exact scope root. The caller-provided mapper may immediately strip
/// diagnostic-only anchor state before the point is retained. Neither the
/// branch point nor its mapped representation is exposed to the actor.
pub fn run_task_privileged_support_actor_episode_capturing_fresh_scopes_v1<W, A, C, F>(
    environment: &mut NodeReloadTaskEnvironmentV1<W>,
    actor: &mut A,
    mut map_point: F,
) -> Result<(TaskActorEpisodeV1, Vec<C>), TaskActorRunErrorV1>
where
    W: super::TaskPrivilegedSupportWorldV1,
    A: super::TaskPrivilegedSupportActorV1,
    F: FnMut(TaskResearchRunBranchPointV1<W::ScopeAnchor>) -> Result<C, String>,
{
    let mut captured = Vec::new();
    let episode = run_task_actor_episode_with_pre_turn_v1(
        environment,
        actor,
        TaskActorInformationAccessV1::TaskActorViewAndPrivilegedSupport,
        |environment, actor, view| {
            if is_fresh_exact_scope_root(view) {
                let point = environment
                    .capture_research_run_branch_point_v1()
                    .map_err(TaskActorRunErrorV1::RunBranchCapture)?;
                captured.push(map_point(point).map_err(TaskActorRunErrorV1::RunBranchCapture)?);
            }
            deliver_privileged_support_or_fail_v1(environment, actor, view)
        },
    )?;
    Ok((episode, captured))
}

fn deliver_privileged_support_or_fail_v1<W, A>(
    environment: &mut NodeReloadTaskEnvironmentV1<W>,
    actor: &mut A,
    view: &TaskActorViewV1,
) -> Result<(), TaskActorRunErrorV1>
where
    W: super::TaskPrivilegedSupportWorldV1,
    A: super::TaskPrivilegedSupportActorV1,
{
    if !actor.needs_privileged_support(view) {
        return Ok(());
    }
    let support = match environment.prepare_privileged_support() {
        Ok(support) => support,
        Err(error) => {
            environment.terminate_mechanics_or_protocol_error(format!(
                "privileged support construction failed: {error}"
            ));
            return Ok(());
        }
    };
    if let Err(error) = actor.receive_privileged_support(view, support) {
        environment.terminate_mechanics_or_protocol_error(format!(
            "privileged support delivery failed: {error}"
        ));
    }
    Ok(())
}

fn is_fresh_exact_scope_root(view: &TaskActorViewV1) -> bool {
    if !matches!(
        view.reload_permission,
        Some(ReloadPermissionV1::ReloadToExactScopeRoot)
    ) {
        return false;
    }
    let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
        return false;
    };
    let Some(memory) = view.node_memory.as_ref() else {
        return false;
    };
    let [attempt] = memory.attempts.as_slice() else {
        return false;
    };
    attempt.steps.is_empty() && attempt.root == *decision
}

fn run_task_actor_episode_with_pre_turn_v1<W, A, H>(
    environment: &mut NodeReloadTaskEnvironmentV1<W>,
    actor: &mut A,
    required_information_access: TaskActorInformationAccessV1,
    mut before_turn: H,
) -> Result<TaskActorEpisodeV1, TaskActorRunErrorV1>
where
    W: TaskWorldV1,
    A: TaskActorV1,
    H: FnMut(
        &mut NodeReloadTaskEnvironmentV1<W>,
        &mut A,
        &TaskActorViewV1,
    ) -> Result<(), TaskActorRunErrorV1>,
{
    let identity = actor.identity();
    identity
        .validate()
        .map_err(TaskActorRunErrorV1::InvalidActorIdentity)?;
    if identity.information_access != required_information_access {
        return Err(TaskActorRunErrorV1::InvalidActorIdentity(format!(
            "actor information access {:?} does not match this runner",
            identity.information_access
        )));
    }
    let mut actor_turns = 0u64;
    let mut act_intents = 0u64;
    let mut reload_intents = 0u32;
    let mut commit_intents = 0u64;
    let mut unsupported_reason = None;

    loop {
        let view = environment.actor_view();
        let TaskActorBoundaryV1::Terminal { outcome } = &view.boundary else {
            before_turn(environment, actor, &view)?;
            // A pre-turn capability may fail the task without issuing a game
            // action. Refresh so the actor never acts against the stale
            // decision observed before that terminal transition.
            let view = environment.actor_view();
            if matches!(view.boundary, TaskActorBoundaryV1::Terminal { .. }) {
                continue;
            }
            let intent = actor.next_intent(&view);
            actor_turns = actor_turns.saturating_add(1);
            match (&view.boundary, intent) {
                (TaskActorBoundaryV1::Decision { .. }, TaskActorIntentV1::Act { choice }) => {
                    act_intents = act_intents.saturating_add(1);
                    let command = environment.decision_handle()?.into_command(choice);
                    if view
                        .reload_permission
                        .as_ref()
                        .is_some_and(|permission| permission.permits_reload())
                    {
                        environment.explore_choice(command)?;
                    } else {
                        environment.commit_choice(command)?;
                    }
                }
                (TaskActorBoundaryV1::Decision { decision }, TaskActorIntentV1::YieldControl) => {
                    return Ok(TaskActorEpisodeV1 {
                        actor: identity,
                        actor_turns,
                        act_intents,
                        reload_intents,
                        commit_intents,
                        unsupported_reason,
                        outcome: TaskEpisodeOutcomeV1::Paused {
                            reason: super::TaskEpisodePauseV1::PublicDecisionControlYield {
                                site: decision.site,
                            },
                        },
                        final_view: view,
                    });
                }
                (
                    TaskActorBoundaryV1::Decision { .. }
                    | TaskActorBoundaryV1::CommitReady
                    | TaskActorBoundaryV1::ExploredScopeExit { .. },
                    TaskActorIntentV1::Reload,
                ) => {
                    reload_intents = reload_intents.saturating_add(1);
                    environment.reload()?;
                }
                (
                    TaskActorBoundaryV1::Decision { .. }
                    | TaskActorBoundaryV1::CommitReady
                    | TaskActorBoundaryV1::ExploredScopeExit { .. },
                    TaskActorIntentV1::CommitScope,
                ) if view.reload_permission.as_ref().is_some_and(|permission| {
                    matches!(
                        permission,
                        super::ReloadPermissionV1::ReloadToExactScopeRoot
                    )
                }) || !matches!(view.boundary, TaskActorBoundaryV1::Decision { .. }) =>
                {
                    commit_intents = commit_intents.saturating_add(1);
                    environment.commit_current_attempt()?;
                }
                (
                    TaskActorBoundaryV1::Decision { .. }
                    | TaskActorBoundaryV1::CommitReady
                    | TaskActorBoundaryV1::ExploredScopeExit { .. },
                    TaskActorIntentV1::UnsupportedDecisionSurface { reason },
                ) => {
                    unsupported_reason = Some(reason);
                    environment.terminate_unsupported_decision_surface();
                }
                (boundary, intent) => {
                    return Err(TaskActorRunErrorV1::InvalidIntent {
                        boundary: match boundary {
                            TaskActorBoundaryV1::Decision { .. } => {
                                TaskActorControlBoundaryV1::Decision
                            }
                            TaskActorBoundaryV1::CommitReady => {
                                TaskActorControlBoundaryV1::CommitReady
                            }
                            TaskActorBoundaryV1::ExploredScopeExit { .. } => {
                                TaskActorControlBoundaryV1::ExploredScopeExit
                            }
                            TaskActorBoundaryV1::Terminal { .. } => {
                                unreachable!("terminal handled before actor invocation")
                            }
                        },
                        intent: intent.kind(),
                    });
                }
            }
            continue;
        };

        return Ok(TaskActorEpisodeV1 {
            actor: identity,
            actor_turns,
            act_intents,
            reload_intents,
            commit_intents,
            unsupported_reason,
            outcome: outcome.clone(),
            final_view: view,
        });
    }
}
