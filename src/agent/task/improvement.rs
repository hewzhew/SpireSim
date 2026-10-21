use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ai::planner_core::stable_planner_id;

use super::{
    TaskActorBoundaryV1, TaskActorIdentityV1, TaskActorIntentV1, TaskActorV1, TaskScopeOrdinalV1,
};
use super::{
    TaskActorViewV1, TaskObservedBoundaryV1, TaskPublicChoiceV1, TaskPublicDecisionV1,
    TaskPublicStepV1,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskProposalInformationClassV1 {
    PublicActorVisible,
    PrivilegedResearch,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskProposalRoleV1 {
    PublicCandidateAlternative,
    StructuredPlan,
    LearnedPolicy,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskProposalExplorationAuthorizationV1 {
    /// The proposal owner asserts from public mechanics that executing this
    /// choice reaches another actor boundary before the native generation can
    /// be replaced or formal death can delete it. A false assertion is a
    /// proposal-owner defect; the runner does not inspect an exact future to
    /// infer reversibility.
    ReversibleUntilNextActorBoundary,
    /// The choice may be selected for the committed line, but must not be
    /// executed as a trial that expects the current generation to remain. A
    /// different controller may handle it without trial; this improver does
    /// not admit it to its tried-proposal set.
    CommitOnly,
    /// Research-only permission to follow this proposal through multiple
    /// public decisions, including a game terminal, while an explicitly
    /// declared exact scope root remains restoreable.
    ExactBranchUntilPlannerStop,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskProposalProvenanceV1 {
    pub producer_name: String,
    pub producer_revision: u32,
    pub information_class: TaskProposalInformationClassV1,
    pub role: TaskProposalRoleV1,
}

impl TaskProposalProvenanceV1 {
    pub fn public_candidate_alternative(
        producer_name: impl Into<String>,
        producer_revision: u32,
    ) -> Self {
        Self {
            producer_name: producer_name.into(),
            producer_revision,
            information_class: TaskProposalInformationClassV1::PublicActorVisible,
            role: TaskProposalRoleV1::PublicCandidateAlternative,
        }
    }

    pub fn public_structured_plan(
        producer_name: impl Into<String>,
        producer_revision: u32,
    ) -> Self {
        Self {
            producer_name: producer_name.into(),
            producer_revision,
            information_class: TaskProposalInformationClassV1::PublicActorVisible,
            role: TaskProposalRoleV1::StructuredPlan,
        }
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if self.producer_name.trim().is_empty() {
            return Err("task proposal provenance has an empty producer name".to_string());
        }
        if self.producer_revision == 0 {
            return Err("task proposal provenance revision must be positive".to_string());
        }
        Ok(())
    }
}

/// One proposal at a discovered public prefix inside the current native
/// scope. The replay prefix contains only actor-visible steps from the native
/// root; no private successor is embedded.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPrefixProposalV1 {
    pub proposal_id: String,
    pub scope_root_snapshot_id: String,
    pub decision_snapshot_id: String,
    pub replay_prefix: Vec<TaskPublicStepV1>,
    pub choice: TaskPublicChoiceV1,
    pub exploration_authorization: TaskProposalExplorationAuthorizationV1,
    pub provenance: TaskProposalProvenanceV1,
}

impl TaskPrefixProposalV1 {
    pub fn new_public(
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
        choice: TaskPublicChoiceV1,
        exploration_authorization: TaskProposalExplorationAuthorizationV1,
        provenance: TaskProposalProvenanceV1,
    ) -> Result<Self, String> {
        decision.validate()?;
        decision.validate_choice(&choice)?;
        provenance.validate()?;
        if provenance.information_class != TaskProposalInformationClassV1::PublicActorVisible {
            return Err(
                "deployable task proposal must use public-actor-visible provenance".to_string(),
            );
        }
        let memory = view
            .node_memory
            .as_ref()
            .ok_or_else(|| "task proposal requires current node memory".to_string())?;
        let attempt = memory
            .attempts
            .last()
            .ok_or_else(|| "task proposal node memory has no current attempt".to_string())?;
        let scope_root_snapshot_id = attempt.root.information.snapshot_id.clone();
        let decision_snapshot_id = decision.information.snapshot_id.clone();
        if let Some(last) = attempt.steps.last() {
            if !matches!(
                &last.observed_boundary,
                TaskObservedBoundaryV1::Decision { decision: observed } if observed == decision
            ) {
                return Err(
                    "task proposal decision is not the current public prefix boundary".to_string(),
                );
            }
        } else if attempt.root != *decision {
            return Err(
                "task proposal at an empty prefix does not match the native root".to_string(),
            );
        }
        if attempt.steps.iter().any(|step| {
            !matches!(
                step.observed_boundary,
                TaskObservedBoundaryV1::Decision { .. }
            )
        }) {
            return Err(
                "task proposal replay prefix crosses a commit or terminal boundary".to_string(),
            );
        }
        let replay_prefix = attempt.steps.clone();
        let proposal_id = stable_planner_id(
            "task_prefix_proposal_v1",
            &(
                scope_root_snapshot_id.as_str(),
                decision_snapshot_id.as_str(),
                &replay_prefix,
                &choice,
                exploration_authorization,
                &provenance,
            ),
        )?;
        Ok(Self {
            proposal_id,
            scope_root_snapshot_id,
            decision_snapshot_id,
            replay_prefix,
            choice,
            exploration_authorization,
            provenance,
        })
    }

    pub fn validate_for_current_prefix(
        &self,
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
    ) -> Result<(), String> {
        let canonical = Self::new_public(
            view,
            decision,
            self.choice.clone(),
            self.exploration_authorization,
            self.provenance.clone(),
        )?;
        if canonical != *self {
            return Err(
                "task prefix proposal identity or public-prefix binding is not canonical"
                    .to_string(),
            );
        }
        Ok(())
    }
}

pub trait TaskProposalGeneratorV1 {
    fn proposals(
        &mut self,
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskPrefixProposalV1>, String>;
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskAssessmentMetricV1 {
    PlayerHp,
    PlayerMaxHp,
    PlayerBlock,
    Energy,
    Gold,
    PotionCount,
    LivingEnemyHpWithBlock,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskAssessmentDirectionV1 {
    HigherIsBetter,
    LowerIsBetter,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskAssessmentComponentV1 {
    pub metric: TaskAssessmentMetricV1,
    pub direction: TaskAssessmentDirectionV1,
    pub value: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContinuationAssessmentV1 {
    pub evaluator_name: String,
    pub evaluator_revision: u32,
    pub public_snapshot_id: String,
    /// Assessments are comparable only when the evaluator declares the same
    /// public semantic context. Different cards, relics, intents, plans, or
    /// unresolved futures must produce a different or absent context id.
    pub comparison_context_id: Option<String>,
    pub components: Vec<TaskAssessmentComponentV1>,
    pub limitations: Vec<String>,
}

pub trait TaskContinuationEvaluatorV1 {
    fn assess(&mut self, view: &TaskActorViewV1) -> Result<TaskContinuationAssessmentV1, String>;
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskAssessmentPreferenceV1 {
    LeftDominates,
    RightDominates,
    Equivalent,
    Tradeoff,
    UnresolvedContext,
}

/// Conservative Pareto comparison over explicitly named public facts. It
/// refuses to compare assessments from different semantic contexts and never
/// collapses HP, resources, and threat into one scalar.
pub fn compare_task_assessments_v1(
    left: &TaskContinuationAssessmentV1,
    right: &TaskContinuationAssessmentV1,
) -> TaskAssessmentPreferenceV1 {
    let (Some(left_context), Some(right_context)) = (
        left.comparison_context_id.as_deref(),
        right.comparison_context_id.as_deref(),
    ) else {
        return TaskAssessmentPreferenceV1::UnresolvedContext;
    };
    if left.evaluator_name != right.evaluator_name
        || left.evaluator_revision != right.evaluator_revision
        || left_context != right_context
    {
        return TaskAssessmentPreferenceV1::UnresolvedContext;
    }
    let components = |assessment: &TaskContinuationAssessmentV1| {
        let components = assessment
            .components
            .iter()
            .map(|component| (component.metric, (component.direction, component.value)))
            .collect::<BTreeMap<_, _>>();
        (components.len() == assessment.components.len()).then_some(components)
    };
    let (Some(left), Some(right)) = (components(left), components(right)) else {
        return TaskAssessmentPreferenceV1::UnresolvedContext;
    };
    if left.is_empty() || left.len() != right.len() || left.keys().ne(right.keys()) {
        return TaskAssessmentPreferenceV1::UnresolvedContext;
    }
    let mut left_better = false;
    let mut right_better = false;
    for (metric, (left_direction, left_value)) in left {
        let Some((right_direction, right_value)) = right.get(&metric).copied() else {
            return TaskAssessmentPreferenceV1::UnresolvedContext;
        };
        if left_direction != right_direction {
            return TaskAssessmentPreferenceV1::UnresolvedContext;
        }
        let order = match left_direction {
            TaskAssessmentDirectionV1::HigherIsBetter => left_value.cmp(&right_value),
            TaskAssessmentDirectionV1::LowerIsBetter => right_value.cmp(&left_value),
        };
        left_better |= order.is_gt();
        right_better |= order.is_lt();
    }
    match (left_better, right_better) {
        (true, false) => TaskAssessmentPreferenceV1::LeftDominates,
        (false, true) => TaskAssessmentPreferenceV1::RightDominates,
        (false, false) => TaskAssessmentPreferenceV1::Equivalent,
        (true, true) => TaskAssessmentPreferenceV1::Tradeoff,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskProposalTrialBoundaryV1 {
    InScopeDecision,
    CommitReadyBeforeGenerationReplacement,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskProposalTrialV1 {
    pub proposal: TaskPrefixProposalV1,
    pub boundary: TaskProposalTrialBoundaryV1,
    pub assessment: Option<TaskContinuationAssessmentV1>,
}

pub trait TaskCommitControllerV1 {
    /// Selects which already tried proposal should be replayed and followed to
    /// commit. The return value is an index into `trials`; it does not grant
    /// permission to restore a superseded autosave generation.
    fn select_proposal(&mut self, trials: &[TaskProposalTrialV1]) -> Result<usize, String>;
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TaskPrefixProposalPhaseV1 {
    ReadyToTrial,
    TrialIssued,
    AwaitingReload,
    ReplayingPrefix { next_step: usize },
    FollowingSelected,
}

struct ActivePrefixProposalScopeV1 {
    scope: TaskScopeOrdinalV1,
    scope_root_snapshot_id: String,
    decision_snapshot_id: String,
    replay_prefix: Vec<TaskPublicStepV1>,
    proposals: Vec<TaskPrefixProposalV1>,
    next_trial: usize,
    trials: Vec<TaskProposalTrialV1>,
    phase: TaskPrefixProposalPhaseV1,
}

/// This trace records that the controller selected a tried proposal for
/// replay. It does not claim that replay or the later scope commit succeeded;
/// committed task history remains the authority for that fact.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskScopeProposalTraceV1 {
    pub scope: TaskScopeOrdinalV1,
    pub scope_root_snapshot_id: String,
    pub decision_snapshot_id: String,
    pub trials: Vec<TaskProposalTrialV1>,
    pub selected_proposal_id: String,
}

/// One-step prefix-alternative improver over a legal native reload scope.
///
/// Each proposal is tried for exactly one public action and is then reloaded
/// while the original autosave generation is still live. After every trial,
/// the controller selects one proposal; its public prefix and choice are
/// replayed from the native root before a continuation actor takes over. The
/// improver never restores after `CommitScope`, never examines a successor
/// privately, and cannot compare terminal exits from a superseded generation.
pub struct TaskPrefixProposalImproverActorV1<G, E, C, F> {
    identity: TaskActorIdentityV1,
    generator: G,
    evaluator: E,
    controller: C,
    continuation: F,
    active: Option<ActivePrefixProposalScopeV1>,
    bypass_scope: Option<TaskScopeOrdinalV1>,
    selection_traces: Vec<TaskScopeProposalTraceV1>,
}

impl<G, E, C, F> TaskPrefixProposalImproverActorV1<G, E, C, F> {
    pub fn new(
        identity: TaskActorIdentityV1,
        generator: G,
        evaluator: E,
        controller: C,
        continuation: F,
    ) -> Self {
        Self {
            identity,
            generator,
            evaluator,
            controller,
            continuation,
            active: None,
            bypass_scope: None,
            selection_traces: Vec::new(),
        }
    }

    pub fn selection_traces(&self) -> &[TaskScopeProposalTraceV1] {
        &self.selection_traces
    }

    fn unsupported(reason: impl Into<String>) -> TaskActorIntentV1 {
        TaskActorIntentV1::UnsupportedDecisionSurface {
            reason: reason.into(),
        }
    }
}

impl<G, E, C, F> TaskActorV1 for TaskPrefixProposalImproverActorV1<G, E, C, F>
where
    G: TaskProposalGeneratorV1,
    E: TaskContinuationEvaluatorV1,
    C: TaskCommitControllerV1,
    F: TaskActorV1,
{
    fn identity(&self) -> TaskActorIdentityV1 {
        self.identity.clone()
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        let Some(memory) = view.node_memory.as_ref() else {
            return Self::unsupported("proposal improver received no active node memory");
        };
        let scope = memory.scope;
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.scope != scope)
        {
            self.active = None;
        }
        if self.bypass_scope.is_some_and(|bypass| bypass != scope) {
            self.bypass_scope = None;
        }
        if self.bypass_scope == Some(scope) {
            return self.continuation.next_intent(view);
        }

        if self.active.is_none() {
            let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
                self.bypass_scope = Some(scope);
                return self.continuation.next_intent(view);
            };
            if view.budget.remaining_reloads < 2 {
                self.bypass_scope = Some(scope);
                return self.continuation.next_intent(view);
            }
            let mut proposals = match self.generator.proposals(view, decision) {
                Ok(proposals) => proposals,
                Err(error) => return Self::unsupported(error),
            };
            if let Some(error) = proposals
                .iter()
                .find_map(|proposal| proposal.validate_for_current_prefix(view, decision).err())
            {
                return Self::unsupported(error);
            }
            proposals.retain(|proposal| {
                proposal.exploration_authorization
                    == TaskProposalExplorationAuthorizationV1::ReversibleUntilNextActorBoundary
            });
            let max_trials = usize::try_from(view.budget.remaining_reloads)
                .unwrap_or(usize::MAX)
                .min(proposals.len());
            proposals.truncate(max_trials);
            if proposals.len() < 2 {
                return self.continuation.next_intent(view);
            }
            let scope_root_snapshot_id = proposals[0].scope_root_snapshot_id.clone();
            let decision_snapshot_id = decision.information.snapshot_id.clone();
            let replay_prefix = proposals[0].replay_prefix.clone();
            let mut unique_proposal_ids = std::collections::BTreeSet::new();
            let mut unique_choices = std::collections::BTreeSet::new();
            if proposals.iter().any(|proposal| {
                proposal.scope_root_snapshot_id != scope_root_snapshot_id
                    || proposal.decision_snapshot_id != decision_snapshot_id
                    || proposal.replay_prefix != replay_prefix
                    || !unique_proposal_ids.insert(proposal.proposal_id.as_str())
                    || !unique_choices.insert(proposal.choice.ordinal_path.as_slice())
            }) {
                return Self::unsupported(
                    "proposal generator returned a mismatched or duplicate prefix proposal or choice",
                );
            }
            self.active = Some(ActivePrefixProposalScopeV1 {
                scope,
                scope_root_snapshot_id,
                decision_snapshot_id,
                replay_prefix,
                proposals,
                next_trial: 0,
                trials: Vec::new(),
                phase: TaskPrefixProposalPhaseV1::ReadyToTrial,
            });
        }

        loop {
            let phase = self
                .active
                .as_ref()
                .expect("active proposal scope initialized")
                .phase;
            match phase {
                TaskPrefixProposalPhaseV1::ReadyToTrial => {
                    let active = self.active.as_mut().expect("active proposal scope");
                    if active.next_trial < active.proposals.len() {
                        let choice = active.proposals[active.next_trial].choice.clone();
                        active.phase = TaskPrefixProposalPhaseV1::TrialIssued;
                        return TaskActorIntentV1::Act { choice };
                    }
                    let selected = match self.controller.select_proposal(&active.trials) {
                        Ok(selected) if selected < active.trials.len() => selected,
                        Ok(_) => {
                            return Self::unsupported(
                                "commit controller selected outside the tried proposal set",
                            )
                        }
                        Err(error) => return Self::unsupported(error),
                    };
                    let proposal = active.trials[selected].proposal.clone();
                    self.selection_traces.push(TaskScopeProposalTraceV1 {
                        scope: active.scope,
                        scope_root_snapshot_id: active.scope_root_snapshot_id.clone(),
                        decision_snapshot_id: active.decision_snapshot_id.clone(),
                        trials: active.trials.clone(),
                        selected_proposal_id: proposal.proposal_id.clone(),
                    });
                    active.phase = TaskPrefixProposalPhaseV1::FollowingSelected;
                    return TaskActorIntentV1::Act {
                        choice: proposal.choice,
                    };
                }
                TaskPrefixProposalPhaseV1::TrialIssued => {
                    let (boundary, assessment) = match &view.boundary {
                        TaskActorBoundaryV1::Decision { .. } => {
                            let assessment = match self.evaluator.assess(view) {
                                Ok(assessment) => assessment,
                                Err(error) => return Self::unsupported(error),
                            };
                            let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
                                unreachable!("decision boundary matched above")
                            };
                            if assessment.public_snapshot_id != decision.information.snapshot_id {
                                return Self::unsupported(
                                    "continuation assessment belongs to another public snapshot",
                                );
                            }
                            (
                                TaskProposalTrialBoundaryV1::InScopeDecision,
                                Some(assessment),
                            )
                        }
                        TaskActorBoundaryV1::CommitReady => (
                            TaskProposalTrialBoundaryV1::CommitReadyBeforeGenerationReplacement,
                            None,
                        ),
                        TaskActorBoundaryV1::ExploredScopeExit { .. } => {
                            return Self::unsupported(
                                "prefix improver does not own exact branch-exit comparison",
                            )
                        }
                        TaskActorBoundaryV1::Terminal { .. } => {
                            unreachable!("task runner does not invoke actors after terminal")
                        }
                    };
                    let active = self.active.as_mut().expect("active proposal scope");
                    let proposal = active.proposals[active.next_trial].clone();
                    active.trials.push(TaskProposalTrialV1 {
                        proposal,
                        boundary,
                        assessment,
                    });
                    active.phase = TaskPrefixProposalPhaseV1::AwaitingReload;
                    return TaskActorIntentV1::Reload;
                }
                TaskPrefixProposalPhaseV1::AwaitingReload => {
                    let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
                        return Self::unsupported(
                            "native reload did not return a public decision root",
                        );
                    };
                    let active = self.active.as_mut().expect("active proposal scope");
                    if decision.information.snapshot_id != active.scope_root_snapshot_id
                        || memory
                            .attempts
                            .last()
                            .is_some_and(|attempt| !attempt.steps.is_empty())
                    {
                        return Self::unsupported(
                            "native reload did not restore the proposal root and empty prefix",
                        );
                    }
                    active.next_trial += 1;
                    active.phase = TaskPrefixProposalPhaseV1::ReplayingPrefix { next_step: 0 };
                }
                TaskPrefixProposalPhaseV1::ReplayingPrefix { next_step } => {
                    let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
                        return Self::unsupported(
                            "public prefix replay did not remain at a decision boundary",
                        );
                    };
                    let active = self.active.as_mut().expect("active proposal scope");
                    let attempt = memory
                        .attempts
                        .last()
                        .expect("node memory always has a current attempt");
                    if attempt.steps != active.replay_prefix[..next_step] {
                        return Self::unsupported(
                            "public prefix replay diverged from acquired node memory",
                        );
                    }
                    if let Some(step) = active.replay_prefix.get(next_step) {
                        if step.source != *decision {
                            return Self::unsupported(
                                "public prefix replay reached a different information state",
                            );
                        }
                        active.phase = TaskPrefixProposalPhaseV1::ReplayingPrefix {
                            next_step: next_step + 1,
                        };
                        return TaskActorIntentV1::Act {
                            choice: step.choice.clone(),
                        };
                    }
                    if decision.information.snapshot_id != active.decision_snapshot_id {
                        return Self::unsupported(
                            "public prefix replay did not reconstruct the proposal decision",
                        );
                    }
                    active.phase = TaskPrefixProposalPhaseV1::ReadyToTrial;
                }
                TaskPrefixProposalPhaseV1::FollowingSelected => {
                    return self.continuation.next_intent(view);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assessment(context: Option<&str>, hp: i64, gold: i64) -> TaskContinuationAssessmentV1 {
        TaskContinuationAssessmentV1 {
            evaluator_name: "public-envelope".to_string(),
            evaluator_revision: 1,
            public_snapshot_id: format!("snapshot-{hp}-{gold}"),
            comparison_context_id: context.map(str::to_string),
            components: vec![
                TaskAssessmentComponentV1 {
                    metric: TaskAssessmentMetricV1::PlayerHp,
                    direction: TaskAssessmentDirectionV1::HigherIsBetter,
                    value: hp,
                },
                TaskAssessmentComponentV1 {
                    metric: TaskAssessmentMetricV1::Gold,
                    direction: TaskAssessmentDirectionV1::HigherIsBetter,
                    value: gold,
                },
            ],
            limitations: Vec::new(),
        }
    }

    #[test]
    fn public_assessment_uses_dominance_without_inventing_a_scalar_tradeoff() {
        assert_eq!(
            compare_task_assessments_v1(
                &assessment(Some("same"), 50, 120),
                &assessment(Some("same"), 45, 100),
            ),
            TaskAssessmentPreferenceV1::LeftDominates
        );
        assert_eq!(
            compare_task_assessments_v1(
                &assessment(Some("same"), 50, 90),
                &assessment(Some("same"), 45, 100),
            ),
            TaskAssessmentPreferenceV1::Tradeoff
        );
    }

    #[test]
    fn public_assessment_refuses_cross_context_comparison() {
        assert_eq!(
            compare_task_assessments_v1(
                &assessment(Some("deck-a"), 50, 100),
                &assessment(Some("deck-b"), 40, 90),
            ),
            TaskAssessmentPreferenceV1::UnresolvedContext
        );
        assert_eq!(
            compare_task_assessments_v1(&assessment(None, 50, 100), &assessment(None, 40, 90),),
            TaskAssessmentPreferenceV1::UnresolvedContext
        );
        let mut duplicate_metric = assessment(Some("same"), 50, 100);
        duplicate_metric
            .components
            .push(duplicate_metric.components[0]);
        assert_eq!(
            compare_task_assessments_v1(&duplicate_metric, &assessment(Some("same"), 40, 90),),
            TaskAssessmentPreferenceV1::UnresolvedContext
        );
    }
}
