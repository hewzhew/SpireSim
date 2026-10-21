use std::fmt::Display;

use serde::{Deserialize, Serialize};

use super::{
    outcome_for_game_terminal_v1, ArchivedNodeKnowledgeV1, CommittedPublicHistoryV1,
    CommittedPublicStepV1, CommittedScopeBoundaryV1, GameOutcomeV1, LegalKnowledgeLedgerV1,
    NodeAttemptStatusV1, NodeMemoryV1, ReloadPermissionV1, TaskBudgetStateV1, TaskBudgetV1,
    TaskCommittedScopeExitV1, TaskEpisodeFailureV1, TaskEpisodeOutcomeV1, TaskObservedBoundaryV1,
    TaskPublicChoiceV1, TaskPublicDecisionV1, TaskPublicStepV1, TaskScopeOrdinalV1, TaskSpecV1,
};

/// Private world boundary supplied to the task environment. `restore_anchor`
/// never appears in [`TaskActorViewV1`]. Strict native-save and research exact
/// branching worlds use different anchor types and public permissions.
#[derive(Clone, Debug)]
pub struct ReloadScopeV1<A> {
    pub public_root: TaskPublicDecisionV1,
    pub restore_anchor: A,
    pub reload_permission: ReloadPermissionV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskExploredScopeExitV1 {
    NextScope { decision: TaskPublicDecisionV1 },
    GameTerminal { game_outcome: GameOutcomeV1 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum TaskWorldExplorationV1 {
    Decision(TaskPublicDecisionV1),
    /// Strict native-save execution has detected an autosave replacement but
    /// deliberately withholds the successor generation until commit.
    CommitReady,
    /// Research exact branching deliberately exposes the branch exit while
    /// retaining authority to restore its exact scope root.
    ExploredScopeExit(TaskExploredScopeExitV1),
    /// A strict deployed world reached a terminal that was already committed
    /// and cannot be restored under its declared protocol.
    CommittedTerminal(GameOutcomeV1),
}

#[derive(Clone, Debug)]
pub enum TaskWorldCommitV1<A> {
    Decision(TaskPublicDecisionV1),
    NextScope(ReloadScopeV1<A>),
    Terminal(GameOutcomeV1),
}

/// Exact simulator adapters implement this private authority. Deployed actors
/// must be handed only [`TaskActorViewV1`] and task commands, never this trait
/// object, its anchor, or the owning environment.
pub trait TaskWorldV1 {
    type ScopeAnchor: Clone;
    type Error: Display;

    fn initial_scope(&self) -> Result<ReloadScopeV1<Self::ScopeAnchor>, Self::Error>;

    fn restore_scope_anchor(
        &mut self,
        anchor: &Self::ScopeAnchor,
    ) -> Result<TaskPublicDecisionV1, Self::Error>;

    /// Execute inside a reloadable scope without crossing its irreversible
    /// boundary. `CommitReady` means the adapter retained a private pending
    /// transition which only `commit_explored_scope` may cross.
    fn explore_choice(
        &mut self,
        choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldExplorationV1, Self::Error>;

    fn commit_explored_scope(
        &mut self,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error>;

    /// Execute one immediately committed choice in a `NoReload` scope.
    fn commit_choice(
        &mut self,
        choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error>;
}

/// Optional private evidence channel from an execution world back to its
/// evaluator. Evidence is collected outside actor views and task commands;
/// actors cannot inspect or influence it.
pub trait TaskExecutionEvidenceWorldV1: TaskWorldV1 {
    type Evidence;

    fn take_execution_evidence(&mut self) -> Self::Evidence;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TaskDecisionCapabilityV1(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskDecisionCommandV1 {
    capability: TaskDecisionCapabilityV1,
    pub choice: TaskPublicChoiceV1,
}

#[derive(Clone, Debug)]
pub struct TaskDecisionHandleV1 {
    view: TaskActorViewV1,
    capability: TaskDecisionCapabilityV1,
}

impl TaskDecisionHandleV1 {
    pub fn view(&self) -> &TaskActorViewV1 {
        &self.view
    }

    pub fn into_command(self, choice: TaskPublicChoiceV1) -> TaskDecisionCommandV1 {
        TaskDecisionCommandV1 {
            capability: self.capability,
            choice,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskCommandErrorV1 {
    EpisodeTerminal,
    OperationNotPermitted,
    InvalidBoundary,
    StaleDecisionCapability,
    InvalidChoice(String),
}

impl Display for TaskCommandErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for TaskCommandErrorV1 {}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskActorBoundaryV1 {
    Decision { decision: TaskPublicDecisionV1 },
    CommitReady,
    ExploredScopeExit { exit: TaskExploredScopeExitV1 },
    Terminal { outcome: TaskEpisodeOutcomeV1 },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskActorViewV1 {
    pub task: TaskSpecV1,
    pub budget: TaskBudgetStateV1,
    pub committed_history: CommittedPublicHistoryV1,
    pub legal_knowledge: LegalKnowledgeLedgerV1,
    pub node_memory: Option<NodeMemoryV1>,
    pub reload_permission: Option<ReloadPermissionV1>,
    pub boundary: TaskActorBoundaryV1,
}

pub struct NodeReloadTaskEnvironmentV1<W: TaskWorldV1> {
    world: Box<W>,
    task: TaskSpecV1,
    budget: TaskBudgetStateV1,
    committed_history: CommittedPublicHistoryV1,
    legal_knowledge: LegalKnowledgeLedgerV1,
    active: Option<Box<ActiveScopeV1<W::ScopeAnchor>>>,
    boundary: TaskActorBoundaryV1,
    current_decision_capability: Option<TaskDecisionCapabilityV1>,
    next_scope_ordinal: u64,
    next_decision_capability: u64,
}

#[derive(Clone)]
struct ActiveScopeV1<A> {
    ordinal: TaskScopeOrdinalV1,
    anchor: A,
    reload_permission: ReloadPermissionV1,
    memory: NodeMemoryV1,
}

/// Private, research-only restore point for exploring another complete-run
/// continuation from an earlier committed prefix. Serialization is available
/// only when the execution owner supplies a serializable private anchor; the
/// point is never projected into an actor view. Callers must still supply a
/// fresh explicit continuation budget on restore.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskResearchRunBranchPointV1<A> {
    task: TaskSpecV1,
    committed_history: CommittedPublicHistoryV1,
    legal_knowledge: LegalKnowledgeLedgerV1,
    scope_ordinal: TaskScopeOrdinalV1,
    anchor: A,
    reload_permission: ReloadPermissionV1,
    root: TaskPublicDecisionV1,
    next_scope_ordinal: u64,
}

impl<A> TaskResearchRunBranchPointV1<A> {
    pub fn task(&self) -> &TaskSpecV1 {
        &self.task
    }

    pub fn committed_history(&self) -> &CommittedPublicHistoryV1 {
        &self.committed_history
    }

    #[doc(hidden)]
    pub fn legal_knowledge(&self) -> &LegalKnowledgeLedgerV1 {
        &self.legal_knowledge
    }

    #[doc(hidden)]
    pub fn scope_ordinal(&self) -> TaskScopeOrdinalV1 {
        self.scope_ordinal
    }

    #[doc(hidden)]
    pub fn reload_permission(&self) -> &ReloadPermissionV1 {
        &self.reload_permission
    }

    #[doc(hidden)]
    pub fn next_scope_ordinal(&self) -> u64 {
        self.next_scope_ordinal
    }

    pub fn root(&self) -> &TaskPublicDecisionV1 {
        &self.root
    }

    #[doc(hidden)]
    pub fn private_anchor(&self) -> &A {
        &self.anchor
    }

    /// Replaces only the world-owned private anchor while preserving every
    /// task-kernel field. This lets an execution owner strip diagnostic-only
    /// state before durable storage without exposing the anchor to the actor.
    pub fn map_anchor<B>(self, map: impl FnOnce(A) -> B) -> TaskResearchRunBranchPointV1<B> {
        TaskResearchRunBranchPointV1 {
            task: self.task,
            committed_history: self.committed_history,
            legal_knowledge: self.legal_knowledge,
            scope_ordinal: self.scope_ordinal,
            anchor: map(self.anchor),
            reload_permission: self.reload_permission,
            root: self.root,
            next_scope_ordinal: self.next_scope_ordinal,
        }
    }

    /// Reconstructs a private research point from an owner-validated durable
    /// representation. This is intentionally hidden from actor-facing APIs;
    /// the artifact owner must still verify the exact anchor/public join.
    #[doc(hidden)]
    pub fn from_durable_parts_v1(
        task: TaskSpecV1,
        committed_history: CommittedPublicHistoryV1,
        legal_knowledge: LegalKnowledgeLedgerV1,
        scope_ordinal: TaskScopeOrdinalV1,
        anchor: A,
        reload_permission: ReloadPermissionV1,
        root: TaskPublicDecisionV1,
        next_scope_ordinal: u64,
    ) -> Result<Self, String> {
        task.validate()?;
        root.validate()?;
        if next_scope_ordinal <= scope_ordinal.0 {
            return Err(
                "run branch point next scope ordinal must follow its active scope".to_string(),
            );
        }
        Ok(Self {
            task,
            committed_history,
            legal_knowledge,
            scope_ordinal,
            anchor,
            reload_permission,
            root,
            next_scope_ordinal,
        })
    }
}

impl<W: TaskWorldV1> NodeReloadTaskEnvironmentV1<W> {
    pub fn new(world: W, task: TaskSpecV1, budget: TaskBudgetV1) -> Result<Self, String> {
        task.validate()?;
        budget.validate()?;
        let scope = world.initial_scope().map_err(|error| error.to_string())?;
        Self::validate_scope_for_task(&task, &scope)?;
        let ordinal = TaskScopeOrdinalV1(0);
        let boundary = TaskActorBoundaryV1::Decision {
            decision: scope.public_root.clone(),
        };
        Ok(Self {
            world: Box::new(world),
            task,
            budget: budget.into(),
            committed_history: CommittedPublicHistoryV1::default(),
            legal_knowledge: LegalKnowledgeLedgerV1::default(),
            active: Some(Box::new(ActiveScopeV1 {
                ordinal,
                anchor: scope.restore_anchor,
                reload_permission: scope.reload_permission,
                memory: NodeMemoryV1::new(ordinal, scope.public_root),
            })),
            boundary,
            current_decision_capability: Some(TaskDecisionCapabilityV1(0)),
            next_scope_ordinal: 1,
            next_decision_capability: 1,
        })
    }

    pub fn actor_view(&self) -> TaskActorViewV1 {
        TaskActorViewV1 {
            task: self.task.clone(),
            budget: self.budget,
            committed_history: self.committed_history.clone(),
            legal_knowledge: self.legal_knowledge.clone(),
            node_memory: self.active.as_ref().map(|scope| scope.memory.clone()),
            reload_permission: self
                .active
                .as_ref()
                .map(|scope| scope.reload_permission.clone()),
            boundary: self.boundary.clone(),
        }
    }

    /// Forks the complete private execution state for an owning research
    /// controller. This is deliberately not an actor operation: neither the
    /// clone nor any private world state enters [`TaskActorViewV1`]. The
    /// run-play workbench uses it to retain a small in-memory set of exact
    /// decision anchors so local navigation does not replay an entire run.
    #[doc(hidden)]
    pub fn fork_private_research_execution_v1(
        &self,
        fork_world: impl FnOnce(&W) -> Result<W, String>,
    ) -> Result<Self, String> {
        let world = fork_world(self.world.as_ref())?;
        let task = self.task.clone();
        let committed_history = self.committed_history.clone();
        let legal_knowledge = self.legal_knowledge.clone();
        let active = self.active.clone();
        let boundary = self.boundary.clone();
        Ok(Self {
            world: Box::new(world),
            task,
            budget: self.budget,
            committed_history,
            legal_knowledge,
            active,
            boundary,
            current_decision_capability: self.current_decision_capability,
            next_scope_ordinal: self.next_scope_ordinal,
            next_decision_capability: self.next_decision_capability,
        })
    }

    /// Lets an owning research controller project one bounded diagnostic from
    /// the private execution world without cloning it or adding the result to
    /// an actor view. This is the read-only counterpart to
    /// [`Self::fork_private_research_execution_v1`].
    #[doc(hidden)]
    pub fn inspect_private_research_execution_v1<R>(
        &self,
        inspect_world: impl FnOnce(&W) -> Result<R, String>,
    ) -> Result<R, String> {
        inspect_world(self.world.as_ref())
    }

    /// Captures an exact-scope root before it has acquired any provisional
    /// attempts. This is a privileged run-exploration operation, not an actor
    /// command and not a legal-reload claim. Restricting capture to a fresh
    /// root prevents knowledge from abandoned future branches leaking into a
    /// subsequently replayed committed trajectory.
    pub fn capture_research_run_branch_point_v1(
        &self,
    ) -> Result<TaskResearchRunBranchPointV1<W::ScopeAnchor>, String> {
        let active = self
            .active
            .as_ref()
            .ok_or_else(|| "run branch capture requires an active scope".to_string())?;
        if !matches!(
            active.reload_permission,
            ReloadPermissionV1::ReloadToExactScopeRoot
        ) {
            return Err("run branch capture requires an exact research scope".to_string());
        }
        let TaskActorBoundaryV1::Decision { decision } = &self.boundary else {
            return Err("run branch capture requires a public decision root".to_string());
        };
        let [attempt] = active.memory.attempts.as_slice() else {
            return Err(
                "run branch capture requires a fresh scope without prior attempts".to_string(),
            );
        };
        if !attempt.steps.is_empty() || attempt.root != *decision {
            return Err("run branch capture requires an unmodified exact scope root".to_string());
        }
        Ok(TaskResearchRunBranchPointV1 {
            task: self.task.clone(),
            committed_history: self.committed_history.clone(),
            legal_knowledge: self.legal_knowledge.clone(),
            scope_ordinal: active.ordinal,
            anchor: active.anchor.clone(),
            reload_permission: active.reload_permission.clone(),
            root: decision.clone(),
            next_scope_ordinal: self.next_scope_ordinal,
        })
    }

    /// Restores one earlier research root and installs a fresh, explicit
    /// continuation budget. Prior branch work is not presented to the actor as
    /// legal node memory; global exploration accounting remains the caller's
    /// responsibility and must be bound into the run-attempt manifest.
    pub fn restore_research_run_branch_point_v1(
        &mut self,
        point: &TaskResearchRunBranchPointV1<W::ScopeAnchor>,
        continuation_budget: TaskBudgetV1,
    ) -> Result<TaskActorViewV1, String> {
        continuation_budget.validate()?;
        if point.task != self.task {
            return Err("run branch restore belongs to a different task".to_string());
        }
        if !matches!(
            point.reload_permission,
            ReloadPermissionV1::ReloadToExactScopeRoot
        ) {
            return Err("run branch restore requires an exact research scope".to_string());
        }
        let restored = self
            .world
            .restore_scope_anchor(&point.anchor)
            .map_err(|error| error.to_string())?;
        restored.validate()?;
        if !point.root.replay_compatible_v2(&restored) {
            return Err("run branch restore changed its public scope root".to_string());
        }
        self.budget = continuation_budget.into();
        self.committed_history = point.committed_history.clone();
        self.legal_knowledge = point.legal_knowledge.clone();
        self.active = Some(Box::new(ActiveScopeV1 {
            ordinal: point.scope_ordinal,
            anchor: point.anchor.clone(),
            reload_permission: point.reload_permission.clone(),
            memory: NodeMemoryV1::new(point.scope_ordinal, restored.clone()),
        }));
        self.next_scope_ordinal = point.next_scope_ordinal;
        self.boundary = self.new_decision_boundary(restored);
        Ok(self.actor_view())
    }

    /// Execution-shell handle for the current decision. The opaque capability
    /// is deliberately outside the serializable/model-facing actor view.
    pub fn decision_handle(&self) -> Result<TaskDecisionHandleV1, TaskCommandErrorV1> {
        if !matches!(self.boundary, TaskActorBoundaryV1::Decision { .. }) {
            return Err(TaskCommandErrorV1::InvalidBoundary);
        }
        let capability = self
            .current_decision_capability
            .ok_or(TaskCommandErrorV1::InvalidBoundary)?;
        Ok(TaskDecisionHandleV1 {
            view: self.actor_view(),
            capability,
        })
    }

    pub fn explore_choice(
        &mut self,
        command: TaskDecisionCommandV1,
    ) -> Result<TaskActorViewV1, TaskCommandErrorV1> {
        if self.is_terminal() {
            return Err(TaskCommandErrorV1::EpisodeTerminal);
        }
        let Some(active) = self.active.as_ref() else {
            return Ok(self.fail_protocol("active task boundary has no active scope"));
        };
        if !active.reload_permission.permits_reload() {
            return Err(TaskCommandErrorV1::OperationNotPermitted);
        }
        let decision = match &self.boundary {
            TaskActorBoundaryV1::Decision { decision } => decision.clone(),
            _ => return Err(TaskCommandErrorV1::InvalidBoundary),
        };
        if Some(command.capability) != self.current_decision_capability {
            return Err(TaskCommandErrorV1::StaleDecisionCapability);
        }
        if let Err(message) = decision.validate_choice(&command.choice) {
            return Err(TaskCommandErrorV1::InvalidChoice(message));
        }
        if !self.consume_transition() {
            return Ok(self.actor_view());
        }
        let result = match self.world.explore_choice(&command.choice) {
            Ok(result) => result,
            Err(error) => return Ok(self.fail_protocol(error.to_string())),
        };
        let observed_boundary = match &result {
            TaskWorldExplorationV1::Decision(next) => {
                if let Err(message) = self.validate_same_scope_decision(next) {
                    return Ok(self.fail_protocol(message));
                }
                TaskObservedBoundaryV1::Decision {
                    decision: next.clone(),
                }
            }
            TaskWorldExplorationV1::CommitReady => TaskObservedBoundaryV1::CommitReady,
            TaskWorldExplorationV1::ExploredScopeExit(exit) => {
                TaskObservedBoundaryV1::ExploredScopeExit { exit: exit.clone() }
            }
            TaskWorldExplorationV1::CommittedTerminal(game_outcome) => {
                TaskObservedBoundaryV1::CommittedGameTerminal {
                    game_outcome: *game_outcome,
                }
            }
        };
        self.active_mut()
            .memory
            .current_mut()
            .steps
            .push(TaskPublicStepV1 {
                source: decision,
                choice: command.choice,
                observed_boundary,
            });
        self.boundary = match result {
            TaskWorldExplorationV1::Decision(decision) => self.new_decision_boundary(decision),
            TaskWorldExplorationV1::CommitReady => {
                self.active_mut().memory.current_mut().status = NodeAttemptStatusV1::AwaitingCommit;
                TaskActorBoundaryV1::CommitReady
            }
            TaskWorldExplorationV1::ExploredScopeExit(exit) => {
                self.active_mut().memory.current_mut().status = NodeAttemptStatusV1::AwaitingCommit;
                TaskActorBoundaryV1::ExploredScopeExit { exit }
            }
            TaskWorldExplorationV1::CommittedTerminal(game_outcome) => {
                self.active_mut().memory.current_mut().status =
                    NodeAttemptStatusV1::SelectedAndCommitted;
                self.append_current_attempt_to_committed_history();
                return Ok(self.finish_scope(TaskWorldCommitV1::Terminal(game_outcome)));
            }
        };
        Ok(self.actor_view())
    }

    pub fn reload(&mut self) -> Result<TaskActorViewV1, TaskCommandErrorV1> {
        if self.is_terminal() {
            return Err(TaskCommandErrorV1::EpisodeTerminal);
        }
        let Some(active) = self.active.as_ref() else {
            return Ok(self.fail_protocol("reload requested without an active scope"));
        };
        if !active.reload_permission.permits_reload() {
            return Err(TaskCommandErrorV1::OperationNotPermitted);
        }
        if self.budget.remaining_reloads == 0 {
            return Ok(self.fail_task(TaskEpisodeFailureV1::RetryBudgetExhausted, None));
        }
        self.budget.remaining_reloads -= 1;
        let anchor = active.anchor.clone();
        let root = match self.world.restore_scope_anchor(&anchor) {
            Ok(root) => root,
            Err(error) => return Ok(self.fail_protocol(error.to_string())),
        };
        if let Err(message) = self.validate_restored_root(&root) {
            return Ok(self.fail_protocol(message));
        }
        let active = self.active_mut();
        if matches!(
            active.memory.current().status,
            NodeAttemptStatusV1::InProgress | NodeAttemptStatusV1::AwaitingCommit
        ) {
            active.memory.current_mut().status = NodeAttemptStatusV1::AbandonedByReload;
        }
        active.memory.begin_reloaded_attempt(root.clone());
        self.boundary = self.new_decision_boundary(root);
        Ok(self.actor_view())
    }

    pub fn commit_current_attempt(&mut self) -> Result<TaskActorViewV1, TaskCommandErrorV1> {
        if self.is_terminal() {
            return Err(TaskCommandErrorV1::EpisodeTerminal);
        }
        let Some(active) = self.active.as_ref() else {
            return Ok(self.fail_protocol("commit requested without an active scope"));
        };
        if !active.reload_permission.permits_reload() {
            return Err(TaskCommandErrorV1::OperationNotPermitted);
        }
        let exact_decision_commit = matches!(
            (&self.boundary, &active.reload_permission),
            (
                TaskActorBoundaryV1::Decision { .. },
                ReloadPermissionV1::ReloadToExactScopeRoot
            )
        );
        if exact_decision_commit && active.memory.current().steps.is_empty() {
            return Err(TaskCommandErrorV1::InvalidBoundary);
        }
        if !exact_decision_commit
            && !matches!(
                self.boundary,
                TaskActorBoundaryV1::CommitReady | TaskActorBoundaryV1::ExploredScopeExit { .. }
            )
        {
            return Err(TaskCommandErrorV1::InvalidBoundary);
        }
        if !self.consume_transition() {
            return Ok(self.actor_view());
        }
        let result = match self.world.commit_explored_scope() {
            Ok(result) => result,
            Err(error) => return Ok(self.fail_protocol(error.to_string())),
        };
        match &result {
            TaskWorldCommitV1::Decision(_) => {
                return Ok(self.fail_protocol("scope commit returned an in-scope decision"));
            }
            TaskWorldCommitV1::NextScope(scope) => {
                if let Err(message) = self.validate_next_scope(scope) {
                    return Ok(self.fail_protocol(message));
                }
            }
            TaskWorldCommitV1::Terminal(_) => {}
        }
        self.active_mut().memory.current_mut().status = NodeAttemptStatusV1::SelectedAndCommitted;
        self.append_current_attempt_to_committed_history();
        Ok(self.finish_scope(result))
    }

    pub fn commit_choice(
        &mut self,
        command: TaskDecisionCommandV1,
    ) -> Result<TaskActorViewV1, TaskCommandErrorV1> {
        if self.is_terminal() {
            return Err(TaskCommandErrorV1::EpisodeTerminal);
        }
        let Some(active) = self.active.as_ref() else {
            return Ok(self.fail_protocol("committed choice requested without an active scope"));
        };
        if !matches!(
            active.reload_permission,
            ReloadPermissionV1::NoReload { .. }
        ) {
            return Err(TaskCommandErrorV1::OperationNotPermitted);
        }
        let decision = match &self.boundary {
            TaskActorBoundaryV1::Decision { decision } => decision.clone(),
            _ => return Err(TaskCommandErrorV1::InvalidBoundary),
        };
        if Some(command.capability) != self.current_decision_capability {
            return Err(TaskCommandErrorV1::StaleDecisionCapability);
        }
        if let Err(message) = decision.validate_choice(&command.choice) {
            return Err(TaskCommandErrorV1::InvalidChoice(message));
        }
        if !self.consume_transition() {
            return Ok(self.actor_view());
        }
        let result = match self.world.commit_choice(&command.choice) {
            Ok(result) => result,
            Err(error) => return Ok(self.fail_protocol(error.to_string())),
        };
        let observed_boundary = match &result {
            TaskWorldCommitV1::Decision(next) => {
                if let Err(message) = self.validate_same_scope_decision(next) {
                    return Ok(self.fail_protocol(message));
                }
                TaskObservedBoundaryV1::Decision {
                    decision: next.clone(),
                }
            }
            TaskWorldCommitV1::NextScope(scope) => {
                if let Err(message) = self.validate_next_scope(scope) {
                    return Ok(self.fail_protocol(message));
                }
                TaskObservedBoundaryV1::CommittedNextScope {
                    decision: scope.public_root.clone(),
                }
            }
            TaskWorldCommitV1::Terminal(game_outcome) => {
                TaskObservedBoundaryV1::CommittedGameTerminal {
                    game_outcome: *game_outcome,
                }
            }
        };
        let step = TaskPublicStepV1 {
            source: decision,
            choice: command.choice,
            observed_boundary,
        };
        let scope = self.active_ref().ordinal;
        let site = step.source.site;
        self.active_mut()
            .memory
            .current_mut()
            .steps
            .push(step.clone());
        self.committed_history
            .steps
            .push(CommittedPublicStepV1 { scope, site, step });
        match result {
            TaskWorldCommitV1::Decision(decision) => {
                self.boundary = self.new_decision_boundary(decision);
                Ok(self.actor_view())
            }
            result => {
                self.active_mut().memory.current_mut().status =
                    NodeAttemptStatusV1::SelectedAndCommitted;
                Ok(self.finish_scope(result))
            }
        }
    }

    /// Ends an episode when a public actor cannot handle the current typed
    /// decision surface. This is a task admission failure, not a simulator
    /// terminal and not a private-world error.
    pub fn terminate_unsupported_decision_surface(&mut self) -> TaskActorViewV1 {
        if self.is_terminal() {
            return self.actor_view();
        }
        self.fail_task(TaskEpisodeFailureV1::UnsupportedDecisionSurface, None)
    }

    /// Converts a private support/projection failure into durable task
    /// evidence. The public actor did not choose a game action, so this is a
    /// protocol terminal rather than a committed defeat or thrown runner
    /// error.
    pub fn terminate_mechanics_or_protocol_error(
        &mut self,
        message: impl Into<String>,
    ) -> TaskActorViewV1 {
        if self.is_terminal() {
            return self.actor_view();
        }
        self.fail_protocol(message)
    }

    fn finish_scope(&mut self, result: TaskWorldCommitV1<W::ScopeAnchor>) -> TaskActorViewV1 {
        debug_assert!(!matches!(result, TaskWorldCommitV1::Decision(_)));
        let exit = match &result {
            TaskWorldCommitV1::NextScope(scope) => TaskCommittedScopeExitV1::NextScope {
                decision: scope.public_root.clone(),
            },
            TaskWorldCommitV1::Terminal(game_outcome) => TaskCommittedScopeExitV1::GameTerminal {
                game_outcome: *game_outcome,
            },
            TaskWorldCommitV1::Decision(_) => unreachable!("guarded above"),
        };
        let completed = self.active.take().expect("active scope exists at commit");
        self.committed_history
            .completed_scopes
            .push(CommittedScopeBoundaryV1 {
                scope: completed.ordinal,
                entry_site: completed.memory.entry_site,
                exit,
            });
        self.legal_knowledge
            .completed_scopes
            .push(ArchivedNodeKnowledgeV1 {
                scope: completed.ordinal,
                entry_site: completed.memory.entry_site,
                attempts: completed.memory.attempts,
            });
        match result {
            TaskWorldCommitV1::Decision(_) => unreachable!("guarded above"),
            TaskWorldCommitV1::NextScope(scope) => self.open_next_scope(scope),
            TaskWorldCommitV1::Terminal(game_outcome) => {
                let outcome = outcome_for_game_terminal_v1(self.task.objective, game_outcome);
                self.boundary = TaskActorBoundaryV1::Terminal { outcome };
                self.actor_view()
            }
        }
    }

    fn open_next_scope(&mut self, scope: ReloadScopeV1<W::ScopeAnchor>) -> TaskActorViewV1 {
        self.validate_next_scope(&scope)
            .expect("next scope was validated before archiving the committed scope");
        let ordinal = TaskScopeOrdinalV1(self.next_scope_ordinal);
        self.next_scope_ordinal += 1;
        self.boundary = self.new_decision_boundary(scope.public_root.clone());
        self.active = Some(Box::new(ActiveScopeV1 {
            ordinal,
            anchor: scope.restore_anchor,
            reload_permission: scope.reload_permission,
            memory: NodeMemoryV1::new(ordinal, scope.public_root),
        }));
        self.actor_view()
    }

    fn append_current_attempt_to_committed_history(&mut self) {
        let active = self.active_ref();
        let scope = active.ordinal;
        let steps = active.memory.current().steps.clone();
        self.committed_history
            .steps
            .extend(steps.into_iter().map(|step| CommittedPublicStepV1 {
                scope,
                site: step.source.site,
                step,
            }));
    }

    fn validate_same_scope_decision(&self, decision: &TaskPublicDecisionV1) -> Result<(), String> {
        decision.validate()
    }

    fn validate_restored_root(&self, decision: &TaskPublicDecisionV1) -> Result<(), String> {
        decision.validate()?;
        let expected = &self.active_ref().memory.attempts[0].root;
        if decision != expected {
            return Err("native reload did not reconstruct the scope's public root".to_string());
        }
        Ok(())
    }

    fn consume_transition(&mut self) -> bool {
        if self.budget.remaining_world_transitions == 0 {
            self.fail_task(TaskEpisodeFailureV1::TransitionBudgetExhausted, None);
            return false;
        }
        self.budget.remaining_world_transitions -= 1;
        true
    }

    fn fail_protocol(&mut self, message: impl Into<String>) -> TaskActorViewV1 {
        self.fail_task(
            TaskEpisodeFailureV1::MechanicsOrProtocolError {
                message: message.into(),
            },
            None,
        )
    }

    fn fail_task(
        &mut self,
        reason: TaskEpisodeFailureV1,
        game_outcome: Option<GameOutcomeV1>,
    ) -> TaskActorViewV1 {
        self.current_decision_capability = None;
        self.boundary = TaskActorBoundaryV1::Terminal {
            outcome: TaskEpisodeOutcomeV1::Failure {
                reason,
                game_outcome,
            },
        };
        self.actor_view()
    }

    fn active_ref(&self) -> &ActiveScopeV1<W::ScopeAnchor> {
        self.active
            .as_ref()
            .expect("non-terminal task environment has an active scope")
    }

    fn active_mut(&mut self) -> &mut ActiveScopeV1<W::ScopeAnchor> {
        self.active
            .as_mut()
            .expect("non-terminal task environment has an active scope")
    }

    fn is_terminal(&self) -> bool {
        matches!(self.boundary, TaskActorBoundaryV1::Terminal { .. })
    }

    fn new_decision_boundary(&mut self, decision: TaskPublicDecisionV1) -> TaskActorBoundaryV1 {
        let capability = TaskDecisionCapabilityV1(self.next_decision_capability);
        self.next_decision_capability += 1;
        self.current_decision_capability = Some(capability);
        TaskActorBoundaryV1::Decision { decision }
    }

    fn validate_next_scope(&self, scope: &ReloadScopeV1<W::ScopeAnchor>) -> Result<(), String> {
        Self::validate_scope_for_task(&self.task, scope)
    }

    fn validate_scope_for_task(
        task: &TaskSpecV1,
        scope: &ReloadScopeV1<W::ScopeAnchor>,
    ) -> Result<(), String> {
        scope.public_root.validate()?;
        task.reload_protocol
            .validate_scope_permission(&scope.reload_permission)
    }
}

impl<W> NodeReloadTaskEnvironmentV1<W>
where
    W: TaskExecutionEvidenceWorldV1,
{
    pub fn take_execution_evidence(&mut self) -> W::Evidence {
        self.world.take_execution_evidence()
    }
}

impl<W> NodeReloadTaskEnvironmentV1<W>
where
    W: super::TaskPrivilegedSupportWorldV1,
{
    /// Private execution-shell hook used only by the explicitly privileged
    /// research-plan runner. The returned plan has already lost exact inputs
    /// and is revalidated against the current public root before delivery.
    pub fn prepare_privileged_support(&mut self) -> Result<super::TaskPrivilegedSupportV1, String> {
        if self.task.information_access
            != super::TaskInformationAccessV1::PublicHistoryLegalReloadMemoryAndPrivilegedSupport
        {
            return Err("task does not authorize privileged support".to_string());
        }
        if !matches!(&self.boundary, TaskActorBoundaryV1::Decision { .. }) {
            return Err("research plan requested outside a public decision".to_string());
        }
        let support = self
            .world
            .privileged_support()
            .map_err(|error| error.to_string())?;
        match &support {
            super::TaskPrivilegedSupportV1::CombatResearchPlan { support } => {
                support.validate(&self.actor_view())?;
            }
            super::TaskPrivilegedSupportV1::StrategicChoice { support } => {
                support.validate(&self.actor_view())?;
            }
        }
        Ok(support)
    }
}
