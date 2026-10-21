//! Public-only task actor used to exercise the complete task control shell.
//!
//! This actor is intentionally a legality smoke, not a capability baseline,
//! teacher, incumbent, or proposal-quality claim.

use crate::agent::task::{
    TaskActorBoundaryV1, TaskActorIdentityV1, TaskActorIntentV1, TaskActorV1, TaskActorViewV1,
    TaskPublicCandidateV1, TaskPublicChoiceV1, TaskPublicDecisionV1, TaskStrategicActionV1,
};

pub const FIRST_LEGAL_TASK_ACTOR_NAME_V1: &str = "first_legal_task_actor_smoke";
pub const FIRST_LEGAL_TASK_ACTOR_REVISION_V1: u32 = 1;

#[derive(Clone, Debug, Default)]
pub struct FirstLegalTaskActorV1;

impl TaskActorV1 for FirstLegalTaskActorV1 {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only(
            FIRST_LEGAL_TASK_ACTOR_NAME_V1,
            FIRST_LEGAL_TASK_ACTOR_REVISION_V1,
        )
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        match &view.boundary {
            TaskActorBoundaryV1::Decision { decision } => {
                match first_legal_task_choice_v1(decision) {
                    Ok(choice) => TaskActorIntentV1::Act { choice },
                    Err(reason) => TaskActorIntentV1::UnsupportedDecisionSurface { reason },
                }
            }
            TaskActorBoundaryV1::CommitReady | TaskActorBoundaryV1::ExploredScopeExit { .. } => {
                TaskActorIntentV1::CommitScope
            }
            TaskActorBoundaryV1::Terminal { .. } => TaskActorIntentV1::UnsupportedDecisionSurface {
                reason: "task runner invoked actor after episode terminal".to_string(),
            },
        }
    }
}

pub fn first_legal_task_choice_v1(
    decision: &TaskPublicDecisionV1,
) -> Result<TaskPublicChoiceV1, String> {
    legal_task_choice_for_candidate_v1(decision, 0)
}

pub fn legal_task_choice_for_candidate_v1(
    decision: &TaskPublicDecisionV1,
    candidate_ordinal: usize,
) -> Result<TaskPublicChoiceV1, String> {
    let candidate = decision
        .candidates
        .get(candidate_ordinal)
        .ok_or_else(|| "task candidate ordinal is outside the public surface".to_string())?;
    let mut ordinal_path = vec![candidate_ordinal];
    match candidate {
        TaskPublicCandidateV1::Strategic {
            action:
                TaskStrategicActionV1::BeginRunCardSelection {
                    min_choices,
                    selectable_card_ordinals,
                    ..
                },
        } => {
            if selectable_card_ordinals.len() < *min_choices {
                return Err(
                    "run selection exposes fewer public cards than its declared minimum"
                        .to_string(),
                );
            }
            ordinal_path.extend(0..*min_choices);
        }
        TaskPublicCandidateV1::CombatSelectionFamily { family } => {
            let required = usize::try_from(family.declared_min)
                .map_err(|_| "combat selection minimum does not fit this platform".to_string())?;
            if family.domain.len() < required {
                return Err(
                    "combat selection exposes fewer public candidates than its declared minimum"
                        .to_string(),
                );
            }
            ordinal_path.extend(0..required);
        }
        TaskPublicCandidateV1::Strategic { .. }
        | TaskPublicCandidateV1::CombatAtomic { .. }
        | TaskPublicCandidateV1::CombatIndexedChoice { .. } => {}
    }
    Ok(TaskPublicChoiceV1 { ordinal_path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::task::{
        run_task_actor_episode_v1, TaskBudgetV1, TaskEpisodeFailureV1, TaskEpisodeOutcomeV1,
    };

    #[test]
    fn natural_a0_task_reaches_a_typed_episode_outcome_without_private_policy_access() {
        let mut environment = super::super::ironclad_a0_native_autosave_environment_v1(
            1,
            TaskBudgetV1 {
                max_world_transitions: 512,
                max_reloads: 0,
            },
        )
        .expect("natural A0 native-autosave environment");
        let mut actor = FirstLegalTaskActorV1;

        let episode = run_task_actor_episode_v1(&mut environment, &mut actor)
            .expect("first-legal task actor episode");

        assert_eq!(episode.actor.actor_name, FIRST_LEGAL_TASK_ACTOR_NAME_V1);
        assert_eq!(episode.reload_intents, 0);
        assert!(episode.actor_turns > 0);
        assert!(episode.commit_intents > 0);
        assert!(!episode
            .final_view
            .committed_history
            .completed_scopes
            .is_empty());
        assert!(episode.final_view.committed_history.steps.len() <= episode.act_intents as usize);
        assert!(matches!(
            episode.outcome,
            TaskEpisodeOutcomeV1::Success { .. }
                | TaskEpisodeOutcomeV1::Failure {
                    reason: TaskEpisodeFailureV1::CommittedDefeat,
                    ..
                }
                | TaskEpisodeOutcomeV1::Failure {
                    reason: TaskEpisodeFailureV1::CommittedTerminalWithoutObjective,
                    ..
                }
        ));
        assert!(!matches!(
            episode.outcome,
            TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::MechanicsOrProtocolError { .. }
                    | TaskEpisodeFailureV1::UnsupportedDecisionSurface,
                ..
            }
        ));
    }
}
