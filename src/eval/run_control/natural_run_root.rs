//! Current-simulator roots for a natural run with an explicit character and goal.
//!
//! This owner deliberately constructs fresh exact state instead of round-
//! tripping through an older archive/checkpoint pair. Historical artifacts
//! remain an import format and retain their strict identity validation.

use crate::agent::task::{
    run_task_actor_episode_v1, TaskActorBoundaryV1, TaskActorIdentityV1, TaskActorIntentV1,
    TaskActorV1, TaskActorViewV1, TaskBudgetV1, TaskObjectiveV1, TaskPublicChoiceV1,
    TaskPublicObservationV1, TaskRunSolutionArchiveV1,
};
use crate::ai::planner_core::{PlannerDecisionContext, PlannerPlayerClass};
use crate::state::events::EventId;
use sts_run_checkpoint::TaskRunBranchCheckpointArtifactV1;

use super::{
    ironclad_exact_branching_task_spec_with_objective_v1, CombatResearchPlanOptionsV1,
    ExactBranchingTaskEvidenceRequestV1, ExactBranchingTaskWorldV1, RunControlConfig,
};

pub struct NaturalRunRootArtifactsV1 {
    pub archive: TaskRunSolutionArchiveV1,
    pub checkpoints: TaskRunBranchCheckpointArtifactV1,
}

#[derive(Default)]
struct NaturalRunRootCaptureActorV1 {
    entered_neow_choice_screen: bool,
    committed_neow_intro: bool,
}

impl TaskActorV1 for NaturalRunRootCaptureActorV1 {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only("natural-run-root-capture", 2)
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        let decision = match &view.boundary {
            TaskActorBoundaryV1::CommitReady | TaskActorBoundaryV1::ExploredScopeExit { .. } => {
                return TaskActorIntentV1::CommitScope;
            }
            TaskActorBoundaryV1::Decision { decision } => decision,
            _ => {
                return TaskActorIntentV1::UnsupportedDecisionSurface {
                    reason: "natural run root capture left a public decision boundary".to_string(),
                };
            }
        };
        let is_neow_intro = matches!(
            &decision.observation,
            TaskPublicObservationV1::Strategic { observation }
                if matches!(
                    &observation.context,
                    PlannerDecisionContext::Event {
                        event: EventId::Neow,
                        screen: 0,
                        ..
                    }
                )
        );
        if !self.entered_neow_choice_screen && is_neow_intro && decision.candidates.len() == 1 {
            self.entered_neow_choice_screen = true;
            return TaskActorIntentV1::Act {
                choice: TaskPublicChoiceV1::one(0),
            };
        }
        if self.entered_neow_choice_screen && !self.committed_neow_intro {
            self.committed_neow_intro = true;
            TaskActorIntentV1::CommitScope
        } else if self.committed_neow_intro {
            TaskActorIntentV1::YieldControl
        } else {
            TaskActorIntentV1::UnsupportedDecisionSurface {
                reason: "natural run root is not the unique Neow intro handshake".to_string(),
            }
        }
    }
}

pub fn natural_run_root_artifacts_v1(
    mut config: RunControlConfig,
    budget: TaskBudgetV1,
    plan_options: CombatResearchPlanOptionsV1,
    evidence_request: ExactBranchingTaskEvidenceRequestV1,
) -> Result<NaturalRunRootArtifactsV1, String> {
    budget.validate()?;
    plan_options.validate()?;
    config.player_class = super::canonical_player_class(config.player_class)?;
    let player_class = match config.player_class {
        "Ironclad" => PlannerPlayerClass::Ironclad,
        "Silent" => PlannerPlayerClass::Silent,
        "Defect" => PlannerPlayerClass::Defect,
        "Watcher" => PlannerPlayerClass::Watcher,
        _ => return Err("natural root has an unsupported character".into()),
    };
    let seed = config.seed;
    let objective = if config.final_act {
        TaskObjectiveV1::HeartVictory
    } else {
        TaskObjectiveV1::ActThreeVictory
    };
    let mut task =
        ironclad_exact_branching_task_spec_with_objective_v1(config.ascension_level, objective)?;
    task.player_class = player_class;
    task.information_access = crate::agent::task::TaskInformationAccessV1::PublicHistoryLegalReloadMemoryAndPrivilegedSupport;
    let mut environment = crate::agent::task::NodeReloadTaskEnvironmentV1::new(
        ExactBranchingTaskWorldV1::new_natural(config)?
            .with_combat_research_plans(plan_options)?
            .with_execution_evidence_request(evidence_request)?,
        task,
        budget,
    )?;
    let episode = run_task_actor_episode_v1(
        &mut environment,
        &mut NaturalRunRootCaptureActorV1::default(),
    )
    .map_err(|error| format!("failed to enter natural Neow choice root: {error}"))?;
    let TaskActorBoundaryV1::Decision { decision } = &episode.final_view.boundary else {
        return Err("natural run root did not stop at a public decision".to_string());
    };
    if !matches!(
        &decision.observation,
        TaskPublicObservationV1::Strategic { observation }
            if matches!(
                &observation.context,
                PlannerDecisionContext::Event {
                    event: EventId::Neow,
                    screen: 1,
                    ..
                }
            )
    ) {
        return Err("natural run root did not reach the Neow choice screen".to_string());
    }
    let root = environment
        .capture_research_run_branch_point_v1()?
        .map_anchor(|anchor| anchor.durable_checkpoint_v1());
    let mut archive = TaskRunSolutionArchiveV1::new(episode.final_view.task.clone())?;
    archive.record_episode(seed, "natural-run-root-v1", budget, &episode)?;
    let checkpoints =
        TaskRunBranchCheckpointArtifactV1::from_branch_points(&archive, seed, [root])?;
    Ok(NaturalRunRootArtifactsV1 {
        archive,
        checkpoints,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::planner_core::PlannerRunGoal;
    use crate::content::{cards::CardId, events::neow::NeowEligibilityV1, relics::RelicId};
    use crate::eval::run_control::ExactRunPlaySessionV1;

    #[test]
    fn natural_character_roots_reopen_with_their_own_deck_relic_and_heart_goal() {
        for (player_class, expected_class, card, relic) in [
            (
                "Ironclad",
                PlannerPlayerClass::Ironclad,
                CardId::Bash,
                RelicId::BurningBlood,
            ),
            (
                "Silent",
                PlannerPlayerClass::Silent,
                CardId::Survivor,
                RelicId::SnakeRing,
            ),
            (
                "Defect",
                PlannerPlayerClass::Defect,
                CardId::Zap,
                RelicId::CrackedCore,
            ),
            (
                "Watcher",
                PlannerPlayerClass::Watcher,
                CardId::Eruption,
                RelicId::PureWater,
            ),
        ] {
            let budget = TaskBudgetV1 {
                max_world_transitions: 8,
                max_reloads: 0,
            };
            let plan_options = CombatResearchPlanOptionsV1::default();
            let artifacts = natural_run_root_artifacts_v1(
                RunControlConfig {
                    seed: 17,
                    player_class,
                    ascension_level: 20,
                    final_act: true,
                    ..Default::default()
                },
                budget,
                plan_options.clone(),
                Default::default(),
            )
            .unwrap();
            assert_eq!(artifacts.archive.task.player_class, expected_class);
            assert_eq!(
                artifacts.archive.task.objective,
                TaskObjectiveV1::HeartVictory
            );
            let root_id = artifacts
                .checkpoints
                .identities()
                .next()
                .unwrap()
                .archive_node_id
                .clone();
            let reopened = ExactRunPlaySessionV1::open(
                artifacts.archive,
                artifacts.checkpoints,
                &root_id,
                budget,
                plan_options,
                Default::default(),
            )
            .unwrap();
            let TaskPublicObservationV1::Strategic { observation } =
                &reopened.current_public_decision().unwrap().observation
            else {
                panic!("Neow must be strategic")
            };
            assert_eq!(observation.run.player_class, expected_class);
            assert_eq!(observation.run.ascension_level, 20);
            assert_eq!(observation.run_goal, PlannerRunGoal::HeartVictory);
            assert!(observation.cards.iter().any(|c| c.card == card));
            assert!(observation.relics.iter().any(|r| r.relic == relic));
            assert!(observation
                .public_map
                .nodes
                .iter()
                .any(|n| n.has_emerald_key));
        }
    }

    #[test]
    fn natural_character_root_rejects_unknown_character_and_invalid_ascension() {
        for config in [
            RunControlConfig {
                player_class: "Unknown",
                ..Default::default()
            },
            RunControlConfig {
                player_class: "Silent",
                ascension_level: 21,
                ..Default::default()
            },
        ] {
            assert!(natural_run_root_artifacts_v1(
                config,
                TaskBudgetV1 {
                    max_world_transitions: 8,
                    max_reloads: 0
                },
                Default::default(),
                Default::default(),
            )
            .is_err());
        }
    }

    #[test]
    fn natural_root_is_immediately_restorable() {
        let budget = TaskBudgetV1 {
            max_world_transitions: 8,
            max_reloads: 0,
        };
        let plan_options = CombatResearchPlanOptionsV1::default();
        let evidence_request = ExactBranchingTaskEvidenceRequestV1::default();
        let artifacts = natural_run_root_artifacts_v1(
            crate::eval::run_control::RunControlConfig {
                seed: 0,
                ascension_level: 0,
                neow_eligibility: crate::content::events::neow::NeowEligibilityV1::Rich,
                final_act: false,
                ..Default::default()
            },
            budget,
            plan_options.clone(),
            evidence_request,
        )
        .expect("natural root");

        artifacts.archive.validate().expect("archive");
        artifacts
            .checkpoints
            .validate_for_archive(&artifacts.archive)
            .expect("checkpoint joins");
        assert_eq!(artifacts.checkpoints.seed(), 0);
        assert_eq!(artifacts.checkpoints.checkpoint_count(), 1);
        let root_id = artifacts
            .checkpoints
            .identities()
            .next()
            .expect("root identity")
            .archive_node_id
            .clone();
        let session = ExactRunPlaySessionV1::open(
            artifacts.archive,
            artifacts.checkpoints,
            &root_id,
            budget,
            plan_options,
            evidence_request,
        )
        .expect("natural root exact reopen");
        assert!(session.current_public_decision().is_some());
    }

    #[test]
    fn natural_a20_root_preserves_ascension_through_exact_reopen() {
        let budget = TaskBudgetV1 {
            max_world_transitions: 8,
            max_reloads: 0,
        };
        let plan_options = CombatResearchPlanOptionsV1::default();
        let evidence_request = ExactBranchingTaskEvidenceRequestV1::default();
        let artifacts = natural_run_root_artifacts_v1(
            crate::eval::run_control::RunControlConfig {
                seed: 17,
                ascension_level: 20,
                neow_eligibility: crate::content::events::neow::NeowEligibilityV1::Rich,
                final_act: false,
                ..Default::default()
            },
            budget,
            plan_options.clone(),
            evidence_request,
        )
        .expect("natural A20 root");
        assert_eq!(artifacts.archive.task.ascension_level, 20);
        let root_id = artifacts
            .checkpoints
            .identities()
            .next()
            .expect("A20 root identity")
            .archive_node_id
            .clone();
        let session = ExactRunPlaySessionV1::open(
            artifacts.archive,
            artifacts.checkpoints,
            &root_id,
            budget,
            plan_options,
            evidence_request,
        )
        .expect("A20 exact reopen");
        let decision = session.current_public_decision().expect("A20 decision");
        let crate::agent::task::TaskPublicObservationV1::Strategic { observation } =
            &decision.observation
        else {
            panic!("natural A20 root must be strategic");
        };
        assert_eq!(observation.run.ascension_level, 20);
        assert!(matches!(
            &observation.context,
            PlannerDecisionContext::Event {
                event: EventId::Neow,
                screen: 1,
                ..
            }
        ));
        assert!(decision.candidates.len() > 1);
    }

    #[test]
    fn heart_root_preserves_goal_and_exposes_an_emerald_key_elite() {
        let budget = TaskBudgetV1 {
            max_world_transitions: 8,
            max_reloads: 0,
        };
        let plan_options = CombatResearchPlanOptionsV1::default();
        let evidence_request = ExactBranchingTaskEvidenceRequestV1::default();
        let artifacts = natural_run_root_artifacts_v1(
            crate::eval::run_control::RunControlConfig {
                seed: 0,
                ascension_level: 20,
                neow_eligibility: NeowEligibilityV1::Rich,
                final_act: true,
                ..Default::default()
            },
            budget,
            plan_options.clone(),
            evidence_request,
        )
        .expect("natural A20H root");
        assert_eq!(
            artifacts.archive.task.objective,
            TaskObjectiveV1::HeartVictory
        );
        let root_id = artifacts
            .checkpoints
            .identities()
            .next()
            .expect("A20H root identity")
            .archive_node_id
            .clone();
        let session = ExactRunPlaySessionV1::open(
            artifacts.archive,
            artifacts.checkpoints,
            &root_id,
            budget,
            plan_options,
            evidence_request,
        )
        .expect("A20H exact reopen");
        let decision = session.current_public_decision().expect("A20H decision");
        let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
            panic!("natural A20H root must be strategic");
        };
        assert_eq!(observation.run_goal, PlannerRunGoal::HeartVictory);
        assert!(
            observation
                .public_map
                .nodes
                .iter()
                .any(|node| node.has_emerald_key),
            "Heart root must expose the burning elite on the public map"
        );
    }
}
