//! First public-only, prefix-scoped proposal-improvement vertical slice.
//!
//! It improves only the natural Neow root, where public mechanics establish
//! that trying one offer reaches another actor boundary before formal death.
//! It is not yet a reference actor or general combat planner.

use crate::agent::task::{
    compare_task_assessments_v1, TaskActorIdentityV1, TaskAssessmentComponentV1,
    TaskAssessmentDirectionV1, TaskAssessmentMetricV1, TaskAssessmentPreferenceV1,
    TaskCommitControllerV1, TaskContinuationAssessmentV1, TaskContinuationEvaluatorV1,
    TaskPrefixProposalImproverActorV1, TaskPrefixProposalV1,
    TaskProposalExplorationAuthorizationV1, TaskProposalGeneratorV1, TaskProposalProvenanceV1,
    TaskProposalTrialV1, TaskPublicCandidateV1, TaskPublicDecisionV1, TaskPublicObservationV1,
    TaskScopeProposalTraceV1,
};
use crate::ai::planner_core::{stable_planner_id, PlannerDecisionSite};

use super::{legal_task_choice_for_candidate_v1, FirstLegalTaskActorV1};

pub const PUBLIC_NEOW_SCOPE_IMPROVER_NAME_V1: &str = "public_neow_scope_improver_vertical_slice";
pub const PUBLIC_NEOW_SCOPE_IMPROVER_REVISION_V1: u32 = 1;

#[derive(Clone, Debug, Default)]
pub struct PublicNeowPrefixProposalGeneratorV1;

impl TaskProposalGeneratorV1 for PublicNeowPrefixProposalGeneratorV1 {
    fn proposals(
        &mut self,
        view: &crate::agent::task::TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskPrefixProposalV1>, String> {
        let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
            return Ok(Vec::new());
        };
        if observation.decision_site != PlannerDecisionSite::Neow {
            return Ok(Vec::new());
        }
        decision
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| matches!(candidate, TaskPublicCandidateV1::Strategic { .. }))
            .map(|(ordinal, _)| {
                TaskPrefixProposalV1::new_public(
                    view,
                    decision,
                    legal_task_choice_for_candidate_v1(decision, ordinal)?,
                    TaskProposalExplorationAuthorizationV1::ReversibleUntilNextActorBoundary,
                    TaskProposalProvenanceV1::public_candidate_alternative(
                        PUBLIC_NEOW_SCOPE_IMPROVER_NAME_V1,
                        PUBLIC_NEOW_SCOPE_IMPROVER_REVISION_V1,
                    ),
                )
            })
            .collect()
    }
}

#[derive(Clone, Debug, Default)]
pub struct PublicStrategicEnvelopeEvaluatorV1;

impl TaskContinuationEvaluatorV1 for PublicStrategicEnvelopeEvaluatorV1 {
    fn assess(
        &mut self,
        view: &crate::agent::task::TaskActorViewV1,
    ) -> Result<TaskContinuationAssessmentV1, String> {
        let crate::agent::task::TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return Err("strategic envelope evaluator requires a public decision".to_string());
        };
        let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
            return Ok(TaskContinuationAssessmentV1 {
                evaluator_name: "public_strategic_envelope".to_string(),
                evaluator_revision: 1,
                public_snapshot_id: decision.information.snapshot_id.clone(),
                comparison_context_id: None,
                components: Vec::new(),
                limitations: vec!["combat continuation context is unresolved".to_string()],
            });
        };
        let comparison_context_id = stable_planner_id(
            "public_strategic_envelope_context_v1",
            &(
                &observation.mechanics,
                observation.run_goal,
                observation.decision_site,
                observation.run.player_class,
                observation.run.ascension_level,
                observation.run.act,
                observation.run.floor,
                observation.run.keys,
                observation.run.potion_capacity,
                &observation.cards,
                &observation.relics,
                &observation.potions,
                &observation.public_map,
                &observation.context,
                &observation.public_history,
                &decision.candidates,
            ),
        )?;
        Ok(TaskContinuationAssessmentV1 {
            evaluator_name: "public_strategic_envelope".to_string(),
            evaluator_revision: 1,
            public_snapshot_id: decision.information.snapshot_id.clone(),
            comparison_context_id: Some(comparison_context_id),
            components: vec![
                TaskAssessmentComponentV1 {
                    metric: TaskAssessmentMetricV1::PlayerHp,
                    direction: TaskAssessmentDirectionV1::HigherIsBetter,
                    value: i64::from(observation.run.current_hp),
                },
                TaskAssessmentComponentV1 {
                    metric: TaskAssessmentMetricV1::PlayerMaxHp,
                    direction: TaskAssessmentDirectionV1::HigherIsBetter,
                    value: i64::from(observation.run.max_hp),
                },
                TaskAssessmentComponentV1 {
                    metric: TaskAssessmentMetricV1::Gold,
                    direction: TaskAssessmentDirectionV1::HigherIsBetter,
                    value: i64::from(observation.run.gold),
                },
                TaskAssessmentComponentV1 {
                    metric: TaskAssessmentMetricV1::PotionCount,
                    direction: TaskAssessmentDirectionV1::HigherIsBetter,
                    value: observation
                        .potions
                        .iter()
                        .filter(|slot| slot.potion.is_some())
                        .count() as i64,
                },
            ],
            limitations: vec![
                "comparison is valid only when all non-metric public run, deck, relic, potion, map, history, context, and candidate semantics match"
                    .to_string(),
                "no scalar tradeoff or long-run victory estimate is claimed".to_string(),
            ],
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ConservativePublicDominanceControllerV1;

impl TaskCommitControllerV1 for ConservativePublicDominanceControllerV1 {
    fn select_proposal(&mut self, trials: &[TaskProposalTrialV1]) -> Result<usize, String> {
        let mut incumbent = 0usize;
        for challenger in 1..trials.len() {
            let preference = match (
                trials[incumbent].assessment.as_ref(),
                trials[challenger].assessment.as_ref(),
            ) {
                (Some(left), Some(right)) => compare_task_assessments_v1(left, right),
                _ => TaskAssessmentPreferenceV1::UnresolvedContext,
            };
            if preference == TaskAssessmentPreferenceV1::RightDominates {
                incumbent = challenger;
            }
        }
        Ok(incumbent)
    }
}

pub type PublicNeowScopeImproverActorV1 = TaskPrefixProposalImproverActorV1<
    PublicNeowPrefixProposalGeneratorV1,
    PublicStrategicEnvelopeEvaluatorV1,
    ConservativePublicDominanceControllerV1,
    FirstLegalTaskActorV1,
>;

pub fn public_neow_scope_improver_actor_v1() -> PublicNeowScopeImproverActorV1 {
    TaskPrefixProposalImproverActorV1::new(
        TaskActorIdentityV1::task_actor_view_only(
            PUBLIC_NEOW_SCOPE_IMPROVER_NAME_V1,
            PUBLIC_NEOW_SCOPE_IMPROVER_REVISION_V1,
        ),
        PublicNeowPrefixProposalGeneratorV1,
        PublicStrategicEnvelopeEvaluatorV1,
        ConservativePublicDominanceControllerV1,
        FirstLegalTaskActorV1,
    )
}

pub fn public_neow_scope_improver_traces_v1(
    actor: &PublicNeowScopeImproverActorV1,
) -> &[TaskScopeProposalTraceV1] {
    actor.selection_traces()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::task::{run_task_actor_episode_v1, TaskBudgetV1, TaskEpisodeFailureV1};

    #[test]
    fn natural_a0_neow_scope_trials_public_alternatives_before_replaying_one_commit() {
        let mut environment = super::super::ironclad_a0_native_autosave_environment_v1(
            1,
            TaskBudgetV1 {
                max_world_transitions: 512,
                max_reloads: 4,
            },
        )
        .expect("natural A0 native-autosave environment");
        let mut actor = public_neow_scope_improver_actor_v1();

        let episode = run_task_actor_episode_v1(&mut environment, &mut actor)
            .expect("public Neow scope improver episode");

        let traces = public_neow_scope_improver_traces_v1(&actor);
        assert_eq!(traces.len(), 1);
        assert!(traces[0].trials.len() >= 2);
        assert_ne!(
            traces[0].scope_root_snapshot_id,
            traces[0].decision_snapshot_id
        );
        assert!(traces[0]
            .trials
            .iter()
            .all(|trial| !trial.proposal.replay_prefix.is_empty()));
        assert_eq!(episode.reload_intents as usize, traces[0].trials.len());
        assert!(traces[0]
            .trials
            .iter()
            .all(|trial| trial.proposal.provenance.information_class
                == crate::agent::task::TaskProposalInformationClassV1::PublicActorVisible));
        assert!(!matches!(
            episode.outcome,
            crate::agent::task::TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::MechanicsOrProtocolError { .. }
                    | TaskEpisodeFailureV1::UnsupportedDecisionSurface
                    | TaskEpisodeFailureV1::TransitionBudgetExhausted,
                ..
            }
        ));
    }
}
