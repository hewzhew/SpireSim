use crate::ai::planner_core::{
    CandidateCompletenessBasis, CandidateSetCompleteness, LegalCandidate, LegalCandidateSet,
    PlannerAction, PlannerCardObservation, PlannerDecisionContext, PlannerDecisionSite,
    PlannerMechanicsManifest, PlannerPlayerClass, PlannerPotionObservation,
    PlannerPotionSlotObservation, PlannerPublicHistory, PlannerPublicMap, PlannerRunGoal,
    PlannerRunScalars,
};
use crate::content::cards::CardId;
use crate::content::potions::PotionId;
use crate::runtime::combat::CombatCard;

use std::collections::VecDeque;

use super::*;

#[derive(Clone, Debug)]
struct FakeAnchor {
    _private_world_marker: u64,
}

#[derive(Clone)]
struct FakeWorld {
    pending_commit: bool,
    reload_permission: ReloadPermissionV1,
}

impl Default for FakeWorld {
    fn default() -> Self {
        Self {
            pending_commit: false,
            reload_permission: ReloadPermissionV1::NoReload {
                reason: NoReloadReasonV1::UnverifiedSimulatorParity,
            },
        }
    }
}

impl FakeWorld {
    fn reloadable() -> Self {
        Self {
            pending_commit: false,
            reload_permission: ReloadPermissionV1::ReloadToNativeAnchor {
                native_anchor: NativeReloadAnchorV1::EnterRoomAutosave,
                irreversible_commit:
                    IrreversibleCommitBoundaryV1::AutosaveGenerationReplacedOrDeleted,
            },
        }
    }

    fn exact_branching() -> Self {
        Self {
            pending_commit: false,
            reload_permission: ReloadPermissionV1::ReloadToExactScopeRoot,
        }
    }
}

impl TaskWorldV1 for FakeWorld {
    type ScopeAnchor = FakeAnchor;
    type Error = String;

    fn initial_scope(&self) -> Result<ReloadScopeV1<Self::ScopeAnchor>, Self::Error> {
        Ok(ReloadScopeV1 {
            public_root: decision("map-root", TaskDecisionSiteV1::Map, 3),
            restore_anchor: FakeAnchor {
                _private_world_marker: 0xfeed_cafe,
            },
            reload_permission: self.reload_permission.clone(),
        })
    }

    fn restore_scope_anchor(
        &mut self,
        _anchor: &Self::ScopeAnchor,
    ) -> Result<TaskPublicDecisionV1, Self::Error> {
        self.pending_commit = false;
        Ok(decision("map-root", TaskDecisionSiteV1::Map, 3))
    }

    fn explore_choice(
        &mut self,
        choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldExplorationV1, Self::Error> {
        match choice.ordinal_path.as_slice() {
            [0] => Ok(TaskWorldExplorationV1::Decision(decision(
                "card-reward",
                TaskDecisionSiteV1::CardReward,
                1,
            ))),
            [1] => {
                self.pending_commit = true;
                Ok(TaskWorldExplorationV1::CommitReady)
            }
            [2] => Ok(TaskWorldExplorationV1::CommittedTerminal(
                GameOutcomeV1::Defeat,
            )),
            _ => Err("fake world received an unsupported choice".to_string()),
        }
    }

    fn commit_explored_scope(
        &mut self,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        if !self.pending_commit {
            return Err("fake world has no pending explored scope".to_string());
        }
        self.pending_commit = false;
        Ok(TaskWorldCommitV1::Terminal(GameOutcomeV1::ActThreeVictory))
    }

    fn commit_choice(
        &mut self,
        _choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        Ok(TaskWorldCommitV1::Terminal(GameOutcomeV1::ActThreeVictory))
    }
}

fn decision(label: &str, site: TaskDecisionSiteV1, candidate_count: usize) -> TaskPublicDecisionV1 {
    let (planner_site, context) = match site {
        TaskDecisionSiteV1::Map => (
            PlannerDecisionSite::Map,
            PlannerDecisionContext::Map { overlay: false },
        ),
        TaskDecisionSiteV1::CardReward => (
            PlannerDecisionSite::CardReward,
            PlannerDecisionContext::CardReward {
                reward_item_index: Some(0),
                pending_rewards: Vec::new(),
            },
        ),
        _ => panic!("fake decision helper does not implement {site:?}"),
    };
    TaskPublicDecisionV1::new(
        site,
        TaskPublicObservationV1::Strategic {
            observation: TaskStrategicObservationV1 {
                mechanics: PlannerMechanicsManifest {
                    mechanics_id: label.to_string(),
                    mechanics_version: 1,
                },
                run_goal: PlannerRunGoal::ActThreeVictory,
                decision_site: planner_site,
                run: PlannerRunScalars {
                    player_class: PlannerPlayerClass::Ironclad,
                    ascension_level: 0,
                    act: 1,
                    floor: 0,
                    current_hp: 80,
                    max_hp: 80,
                    gold: 99,
                    keys: [false; 3],
                    potion_capacity: 3,
                    pending_act_entry: None,
                },
                cards: Vec::new(),
                relics: Vec::new(),
                potions: Vec::new(),
                public_map: PlannerPublicMap {
                    current_x: 0,
                    current_y: 0,
                    boss: None,
                    nodes: Vec::new(),
                },
                context,
                public_history: PlannerPublicHistory {
                    shop_purge_count: 0,
                    recent_combat_hp_loss: None,
                    recent_combat_turns: None,
                    recent_combat_potions_used: None,
                },
            },
        },
        (0..candidate_count)
            .map(|_| TaskPublicCandidateV1::Strategic {
                action: TaskStrategicActionV1::Proceed { site: planner_site },
            })
            .collect(),
    )
    .expect("task public decision")
}

fn reloadable_map_task() -> TaskSpecV1 {
    let mut task = TaskSpecV1::ironclad_a0_act_three_fail_closed();
    task.reload_protocol.protocol_name = "test_native_autosave_reload_v1".to_string();
    task.reload_protocol.authorization = ReloadAuthorizationV1::NativeAutosaveScopes {
        native_save_model: NativeSaveModelV1::SourceAuditedAutosaveGenerations {
            game_build: "test build".to_string(),
            source_reference: "test fixture: original save behavior".to_string(),
        },
        simulator_contract_reference: "test fixture: simulator parity".to_string(),
    };
    task.validate().expect("reloadable map task");
    task
}

#[test]
fn combat_task_candidate_projects_effect_from_its_public_observation() {
    let mut combat = crate::test_support::blank_test_combat();
    combat.zones.hand = vec![CombatCard::for_scenario(CardId::Bash, 41)];
    let mut monster = crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
    monster.id = 7;
    monster.slot = 4;
    combat.entities.monsters = vec![monster];
    let observation = crate::agent::information::state::public_combat_state_v1(&combat);
    let actions = crate::agent::information::action::project_public_combat_actions_v1(
        &crate::state::EngineState::CombatPlayerTurn,
        &combat,
    )
    .expect("public actions");
    let decision = task_public_combat_decision_v1(
        crate::agent::information::run::PublicCombatRunContextV1::Unavailable {
            reason: crate::agent::information::run::PublicCombatRunContextGapV1::DetachedExactCombatPosition,
        },
        observation,
        &actions.public,
    )
    .expect("combat task decision");
    let bash_ordinal = decision
        .candidates
        .iter()
        .position(|candidate| {
            matches!(
                candidate,
                TaskPublicCandidateV1::CombatAtomic {
                    action:
                        crate::agent::information::action::PublicCombatAtomicActionV1::PlayCard {
                            hand_index: 0,
                            target_monster_index: Some(0),
                        }
                }
            )
        })
        .expect("Bash task candidate");

    let effect = decision
        .combat_candidate_effect_v1(bash_ordinal)
        .expect("public candidate effect");
    assert_eq!(effect.public_snapshot_id, decision.information.snapshot_id);
    assert_eq!(effect.candidate_ordinal, bash_ordinal);
    let crate::agent::information::effect::PublicCombatCandidateEffectV1::PlayCard { card } =
        effect.effect.effect
    else {
        panic!("expected card effect")
    };
    assert_eq!(card.card_id, CardId::Bash);
    assert_eq!(card.selected_target.expect("target").monster_slot, 4);

    let mut tampered = decision.clone();
    tampered.information.snapshot_id.push_str("-tampered");
    assert_eq!(
        tampered.combat_candidate_effect_v1(bash_ordinal),
        Err(crate::agent::information::effect::PublicCombatEffectProjectionErrorV1::NonCanonicalTaskDecision)
    );
}

struct ScriptedPublicActor {
    intents: VecDeque<TaskActorIntentV1>,
}

impl ScriptedPublicActor {
    fn new(intents: impl IntoIterator<Item = TaskActorIntentV1>) -> Self {
        Self {
            intents: intents.into_iter().collect(),
        }
    }
}

impl TaskActorV1 for ScriptedPublicActor {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only("scripted-public-test-actor", 1)
    }

    fn next_intent(&mut self, _view: &TaskActorViewV1) -> TaskActorIntentV1 {
        self.intents.pop_front().expect("scripted actor intent")
    }
}

struct MixedAuthorizationPrefixProposals;

impl TaskProposalGeneratorV1 for MixedAuthorizationPrefixProposals {
    fn proposals(
        &mut self,
        _view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskPrefixProposalV1>, String> {
        [
            (0, TaskProposalExplorationAuthorizationV1::CommitOnly),
            (
                0,
                TaskProposalExplorationAuthorizationV1::ReversibleUntilNextActorBoundary,
            ),
            (
                1,
                TaskProposalExplorationAuthorizationV1::ReversibleUntilNextActorBoundary,
            ),
        ]
        .into_iter()
        .map(|(ordinal, authorization)| {
            TaskPrefixProposalV1::new_public(
                _view,
                decision,
                TaskPublicChoiceV1::one(ordinal),
                authorization,
                TaskProposalProvenanceV1::public_candidate_alternative(
                    "mixed-authorization-prefix-test-proposals",
                    1,
                ),
            )
        })
        .collect()
    }
}

struct PublicTestEvaluator;

impl TaskContinuationEvaluatorV1 for PublicTestEvaluator {
    fn assess(&mut self, view: &TaskActorViewV1) -> Result<TaskContinuationAssessmentV1, String> {
        let TaskActorBoundaryV1::Decision { decision } = &view.boundary else {
            return Err("test evaluator requires a public decision".to_string());
        };
        Ok(TaskContinuationAssessmentV1 {
            evaluator_name: "public-test-evaluator".to_string(),
            evaluator_revision: 1,
            public_snapshot_id: decision.information.snapshot_id.clone(),
            comparison_context_id: Some("same-test-root".to_string()),
            components: Vec::new(),
            limitations: vec!["test-only empty assessment".to_string()],
        })
    }
}

struct SelectCommitReadyTrial;

impl TaskCommitControllerV1 for SelectCommitReadyTrial {
    fn select_proposal(&mut self, trials: &[TaskProposalTrialV1]) -> Result<usize, String> {
        trials
            .iter()
            .position(|trial| {
                trial.boundary
                    == TaskProposalTrialBoundaryV1::CommitReadyBeforeGenerationReplacement
            })
            .ok_or_else(|| "no commit-ready proposal trial".to_string())
    }
}

#[test]
fn proposal_improver_trials_only_before_commit_then_replays_the_selected_prefix() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::reloadable(),
        reloadable_map_task(),
        TaskBudgetV1 {
            max_world_transitions: 6,
            max_reloads: 2,
        },
    )
    .expect("task environment");
    let continuation = ScriptedPublicActor::new([TaskActorIntentV1::CommitScope]);
    let mut actor = TaskPrefixProposalImproverActorV1::new(
        TaskActorIdentityV1::task_actor_view_only("prefix-proposal-improver-test", 1),
        MixedAuthorizationPrefixProposals,
        PublicTestEvaluator,
        SelectCommitReadyTrial,
        continuation,
    );

    let episode = run_task_actor_episode_v1(&mut env, &mut actor).expect("improved episode");

    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Success { .. }
    ));
    assert_eq!(episode.reload_intents, 2);
    assert_eq!(episode.commit_intents, 1);
    assert_eq!(actor.selection_traces().len(), 1);
    let trace = &actor.selection_traces()[0];
    assert_eq!(trace.trials.len(), 2);
    assert!(trace.trials.iter().all(|trial| {
        trial.proposal.exploration_authorization
            == TaskProposalExplorationAuthorizationV1::ReversibleUntilNextActorBoundary
    }));
    assert_eq!(
        trace.trials[1].boundary,
        TaskProposalTrialBoundaryV1::CommitReadyBeforeGenerationReplacement
    );
    assert_eq!(
        trace.selected_proposal_id,
        trace.trials[1].proposal.proposal_id
    );
    assert_eq!(
        episode.final_view.committed_history.steps[0].step.choice,
        TaskPublicChoiceV1::one(1)
    );
}

#[test]
fn actor_runner_keeps_world_private_while_driving_reload_and_commit() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::reloadable(),
        reloadable_map_task(),
        TaskBudgetV1 {
            max_world_transitions: 4,
            max_reloads: 1,
        },
    )
    .expect("task environment");
    let mut actor = ScriptedPublicActor::new([
        TaskActorIntentV1::Act {
            choice: TaskPublicChoiceV1::one(0),
        },
        TaskActorIntentV1::Reload,
        TaskActorIntentV1::Act {
            choice: TaskPublicChoiceV1::one(1),
        },
        TaskActorIntentV1::CommitScope,
    ]);

    let episode = run_task_actor_episode_v1(&mut env, &mut actor).expect("actor episode");

    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::ActThreeVictory
        }
    ));
    assert_eq!(episode.actor_turns, 4);
    assert_eq!(episode.act_intents, 2);
    assert_eq!(episode.reload_intents, 1);
    assert_eq!(episode.commit_intents, 1);
    assert_eq!(episode.final_view.committed_history.steps.len(), 1);
    assert_eq!(
        episode.final_view.legal_knowledge.completed_scopes[0]
            .attempts
            .len(),
        2
    );
}

#[test]
fn actor_runner_commits_actions_immediately_when_reload_is_not_authorized() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::default(),
        TaskSpecV1::ironclad_a0_act_three_fail_closed(),
        TaskBudgetV1 {
            max_world_transitions: 1,
            max_reloads: 0,
        },
    )
    .expect("task environment");
    let mut actor = ScriptedPublicActor::new([TaskActorIntentV1::Act {
        choice: TaskPublicChoiceV1::one(1),
    }]);

    let episode = run_task_actor_episode_v1(&mut env, &mut actor).expect("actor episode");

    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Success { .. }
    ));
    assert_eq!(episode.actor_turns, 1);
    assert_eq!(episode.act_intents, 1);
    assert_eq!(episode.reload_intents, 0);
    assert_eq!(episode.commit_intents, 0);
    assert_eq!(episode.final_view.committed_history.steps.len(), 1);
}

#[test]
fn actor_can_fail_closed_on_an_unsupported_public_surface() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::reloadable(),
        reloadable_map_task(),
        TaskBudgetV1 {
            max_world_transitions: 4,
            max_reloads: 1,
        },
    )
    .expect("task environment");
    let mut actor = ScriptedPublicActor::new([TaskActorIntentV1::UnsupportedDecisionSurface {
        reason: "reference actor has no typed proposal for this public surface".to_string(),
    }]);

    let episode = run_task_actor_episode_v1(&mut env, &mut actor).expect("actor episode");

    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Failure {
            reason: TaskEpisodeFailureV1::UnsupportedDecisionSurface,
            game_outcome: None,
        }
    ));
    assert_eq!(
        episode.unsupported_reason.as_deref(),
        Some("reference actor has no typed proposal for this public surface")
    );
    assert!(episode.final_view.committed_history.steps.is_empty());
}

#[test]
fn actor_can_yield_a_replayable_public_decision_without_calling_it_failure() {
    let budget = TaskBudgetV1 {
        max_world_transitions: 4,
        max_reloads: 1,
    };
    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::exact_branching(),
        exact_branching_test_task(),
        budget,
    )
    .expect("task environment");
    let mut actor = ScriptedPublicActor::new([TaskActorIntentV1::YieldControl]);

    let episode = run_task_actor_episode_v1(&mut env, &mut actor).expect("paused episode");

    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Paused {
            reason: TaskEpisodePauseV1::PublicDecisionControlYield {
                site: TaskDecisionSiteV1::Map,
            }
        }
    ));
    assert_eq!(episode.actor_turns, 1);
    assert!(episode.final_view.committed_history.steps.is_empty());

    let mut archive =
        TaskRunSolutionArchiveV1::new(exact_branching_test_task()).expect("run archive");
    archive
        .record_episode(17, "manual-control-yield", budget, &episode)
        .expect("record paused episode");
    assert!(matches!(
        archive.nodes[0].proof,
        TaskRunArchiveNodeProofV1::ReachableUnknown
    ));
    archive.validate().expect("paused archive remains valid");
}

#[test]
fn private_research_execution_forks_advance_independently() {
    let mut original = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::exact_branching(),
        exact_branching_test_task(),
        TaskBudgetV1 {
            max_world_transitions: 4,
            max_reloads: 0,
        },
    )
    .expect("exact task environment");
    let mut fork = original
        .fork_private_research_execution_v1(|world| Ok(world.clone()))
        .expect("fork exact task environment");

    let original_handle = original.decision_handle().expect("original handle");
    original
        .explore_choice(original_handle.into_command(TaskPublicChoiceV1::one(0)))
        .expect("advance original");
    assert!(matches!(
        original.actor_view().boundary,
        TaskActorBoundaryV1::Decision { ref decision }
            if decision.site == TaskDecisionSiteV1::CardReward
    ));
    assert!(matches!(
        fork.actor_view().boundary,
        TaskActorBoundaryV1::Decision { ref decision }
            if decision.site == TaskDecisionSiteV1::Map
    ));

    let fork_handle = fork.decision_handle().expect("fork handle");
    fork.explore_choice(fork_handle.into_command(TaskPublicChoiceV1::one(1)))
        .expect("advance fork");
    assert!(matches!(
        fork.actor_view().boundary,
        TaskActorBoundaryV1::CommitReady
    ));
    assert!(matches!(
        original.actor_view().boundary,
        TaskActorBoundaryV1::Decision { .. }
    ));
}

#[test]
fn fail_closed_protocol_still_executes_one_committed_world_line() {
    let task = TaskSpecV1::ironclad_a0_act_three_fail_closed();
    assert!(matches!(
        task.reload_protocol.authorization,
        ReloadAuthorizationV1::NoReload {
            reason: NoReloadReasonV1::UnverifiedSimulatorParity
        }
    ));

    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::default(),
        task,
        TaskBudgetV1 {
            max_world_transitions: 1,
            max_reloads: 0,
        },
    )
    .expect("task environment");
    let command = env
        .decision_handle()
        .expect("current decision handle")
        .into_command(TaskPublicChoiceV1::one(1));
    let view = env.commit_choice(command).expect("committed choice");

    assert!(matches!(
        view.boundary,
        TaskActorBoundaryV1::Terminal {
            outcome: TaskEpisodeOutcomeV1::Success {
                game_outcome: GameOutcomeV1::ActThreeVictory
            }
        }
    ));
    assert_eq!(view.committed_history.steps.len(), 1);
    assert_eq!(view.committed_history.completed_scopes.len(), 1);
    assert_eq!(view.legal_knowledge.completed_scopes.len(), 1);
}

#[test]
fn source_audit_alone_does_not_authorize_a_simulator_reload_scope() {
    let error = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::reloadable(),
        TaskSpecV1::ironclad_a0_act_three_fail_closed(),
        TaskBudgetV1 {
            max_world_transitions: 1,
            max_reloads: 1,
        },
    )
    .err()
    .expect("unverified simulator parity must reject reload authority");

    assert_eq!(
        error,
        "task protocol does not authorize native autosave reload"
    );
}

#[test]
fn reload_preserves_exploration_but_commit_selects_only_one_attempt() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::reloadable(),
        reloadable_map_task(),
        TaskBudgetV1 {
            max_world_transitions: 4,
            max_reloads: 1,
        },
    )
    .expect("task environment");

    let failed_command = env
        .decision_handle()
        .expect("failed attempt handle")
        .into_command(TaskPublicChoiceV1::one(0));
    let failed = env
        .explore_choice(failed_command)
        .expect("explore failed attempt");
    assert!(matches!(
        failed.boundary,
        TaskActorBoundaryV1::Decision { ref decision }
            if decision.site == TaskDecisionSiteV1::CardReward
    ));
    assert!(failed.committed_history.steps.is_empty());

    let reloaded = env.reload().expect("reload");
    let memory = reloaded.node_memory.as_ref().expect("current node memory");
    assert_eq!(memory.attempts.len(), 2);
    assert_eq!(
        memory.attempts[0].status,
        NodeAttemptStatusV1::AbandonedByReload
    );
    assert_eq!(memory.attempts[0].steps.len(), 1);
    assert_eq!(memory.attempts[1].status, NodeAttemptStatusV1::InProgress);
    assert!(reloaded.committed_history.steps.is_empty());

    let ready_command = env
        .decision_handle()
        .expect("winning attempt handle")
        .into_command(TaskPublicChoiceV1::one(1));
    let ready = env
        .explore_choice(ready_command)
        .expect("explore winning attempt");
    assert!(matches!(ready.boundary, TaskActorBoundaryV1::CommitReady));
    let committed = env.commit_current_attempt().expect("commit attempt");

    assert!(matches!(
        committed.boundary,
        TaskActorBoundaryV1::Terminal {
            outcome: TaskEpisodeOutcomeV1::Success { .. }
        }
    ));
    assert!(committed.node_memory.is_none());
    assert_eq!(committed.committed_history.steps.len(), 1);
    assert_eq!(
        committed.committed_history.steps[0].step.choice,
        TaskPublicChoiceV1::one(1)
    );
    let archived = &committed.legal_knowledge.completed_scopes[0];
    assert_eq!(archived.attempts.len(), 2);
    assert_eq!(
        archived.attempts[0].status,
        NodeAttemptStatusV1::AbandonedByReload
    );
    assert_eq!(
        archived.attempts[1].status,
        NodeAttemptStatusV1::SelectedAndCommitted
    );

    assert_eq!(env.reload(), Err(TaskCommandErrorV1::EpisodeTerminal));
}

#[test]
fn transition_budget_exhaustion_is_not_a_game_defeat() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::reloadable(),
        reloadable_map_task(),
        TaskBudgetV1 {
            max_world_transitions: 1,
            max_reloads: 0,
        },
    )
    .expect("task environment");

    let command = env
        .decision_handle()
        .expect("attempt handle")
        .into_command(TaskPublicChoiceV1::one(1));
    let ready = env.explore_choice(command).expect("explore attempt");
    assert!(matches!(ready.boundary, TaskActorBoundaryV1::CommitReady));
    let exhausted = env
        .commit_current_attempt()
        .expect("budget exhaustion is a task outcome");

    assert!(matches!(
        exhausted.boundary,
        TaskActorBoundaryV1::Terminal {
            outcome: TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::TransitionBudgetExhausted,
                game_outcome: None,
            }
        }
    ));
    assert!(exhausted.committed_history.steps.is_empty());
    assert_eq!(
        exhausted
            .node_memory
            .as_ref()
            .expect("failed task retains its exploration evidence")
            .current()
            .status,
        NodeAttemptStatusV1::AwaitingCommit
    );
}

struct ExactTerminalWorld {
    pending: Option<GameOutcomeV1>,
}

impl TaskWorldV1 for ExactTerminalWorld {
    type ScopeAnchor = FakeAnchor;
    type Error = String;

    fn initial_scope(&self) -> Result<ReloadScopeV1<Self::ScopeAnchor>, Self::Error> {
        Ok(ReloadScopeV1 {
            public_root: decision("exact-root", TaskDecisionSiteV1::Map, 2),
            restore_anchor: FakeAnchor {
                _private_world_marker: 0x1234,
            },
            reload_permission: ReloadPermissionV1::ReloadToExactScopeRoot,
        })
    }

    fn restore_scope_anchor(
        &mut self,
        _anchor: &Self::ScopeAnchor,
    ) -> Result<TaskPublicDecisionV1, Self::Error> {
        self.pending = None;
        Ok(decision("exact-root", TaskDecisionSiteV1::Map, 2))
    }

    fn explore_choice(
        &mut self,
        choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldExplorationV1, Self::Error> {
        let outcome = match choice.ordinal_path.as_slice() {
            [0] => GameOutcomeV1::Defeat,
            [1] => GameOutcomeV1::ActThreeVictory,
            _ => return Err("unsupported exact terminal choice".to_string()),
        };
        self.pending = Some(outcome);
        Ok(TaskWorldExplorationV1::ExploredScopeExit(
            TaskExploredScopeExitV1::GameTerminal {
                game_outcome: outcome,
            },
        ))
    }

    fn commit_explored_scope(
        &mut self,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        Ok(TaskWorldCommitV1::Terminal(
            self.pending
                .take()
                .ok_or_else(|| "missing exact terminal".to_string())?,
        ))
    }

    fn commit_choice(
        &mut self,
        _choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        Err("exact terminal world requires exploration".to_string())
    }
}

#[derive(Clone, Debug)]
struct HistoricalExactAnchor {
    state: u8,
}

struct HistoricalExactWorld {
    state: u8,
    pending: Option<GameOutcomeV1>,
}

impl HistoricalExactWorld {
    fn public_root(&self) -> TaskPublicDecisionV1 {
        match self.state {
            0 => decision("historical-root", TaskDecisionSiteV1::Map, 2),
            1 => decision("historical-losing-child", TaskDecisionSiteV1::CardReward, 1),
            2 => decision(
                "historical-winning-child",
                TaskDecisionSiteV1::CardReward,
                1,
            ),
            _ => panic!("unsupported historical test state"),
        }
    }

    fn reload_scope(&self) -> ReloadScopeV1<HistoricalExactAnchor> {
        ReloadScopeV1 {
            public_root: self.public_root(),
            restore_anchor: HistoricalExactAnchor { state: self.state },
            reload_permission: ReloadPermissionV1::ReloadToExactScopeRoot,
        }
    }
}

impl TaskWorldV1 for HistoricalExactWorld {
    type ScopeAnchor = HistoricalExactAnchor;
    type Error = String;

    fn initial_scope(&self) -> Result<ReloadScopeV1<Self::ScopeAnchor>, Self::Error> {
        Ok(self.reload_scope())
    }

    fn restore_scope_anchor(
        &mut self,
        anchor: &Self::ScopeAnchor,
    ) -> Result<TaskPublicDecisionV1, Self::Error> {
        self.state = anchor.state;
        self.pending = None;
        Ok(self.public_root())
    }

    fn explore_choice(
        &mut self,
        choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldExplorationV1, Self::Error> {
        match (self.state, choice.ordinal_path.as_slice()) {
            (0, [0]) => {
                self.state = 1;
                Ok(TaskWorldExplorationV1::Decision(self.public_root()))
            }
            (0, [1]) => {
                self.state = 2;
                Ok(TaskWorldExplorationV1::Decision(self.public_root()))
            }
            (1, [0]) => {
                self.pending = Some(GameOutcomeV1::Defeat);
                Ok(TaskWorldExplorationV1::ExploredScopeExit(
                    TaskExploredScopeExitV1::GameTerminal {
                        game_outcome: GameOutcomeV1::Defeat,
                    },
                ))
            }
            (2, [0]) => {
                self.pending = Some(GameOutcomeV1::ActThreeVictory);
                Ok(TaskWorldExplorationV1::ExploredScopeExit(
                    TaskExploredScopeExitV1::GameTerminal {
                        game_outcome: GameOutcomeV1::ActThreeVictory,
                    },
                ))
            }
            _ => Err("unsupported historical exact choice".to_string()),
        }
    }

    fn commit_explored_scope(
        &mut self,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        if let Some(game_outcome) = self.pending.take() {
            return Ok(TaskWorldCommitV1::Terminal(game_outcome));
        }
        Ok(TaskWorldCommitV1::NextScope(self.reload_scope()))
    }

    fn commit_choice(
        &mut self,
        _choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        Err("historical exact world requires exploration".to_string())
    }
}

impl TaskPrivilegedSupportWorldV1 for HistoricalExactWorld {
    fn privileged_support(&mut self) -> Result<TaskPrivilegedSupportV1, Self::Error> {
        Err("capture fixture never requests privileged support".to_string())
    }
}

struct CaptureOnlyPrivilegedActor {
    intents: VecDeque<TaskActorIntentV1>,
}

impl TaskActorV1 for CaptureOnlyPrivilegedActor {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::privileged_support_assisted("capture-only-fixture", 1)
    }

    fn next_intent(&mut self, _view: &TaskActorViewV1) -> TaskActorIntentV1 {
        self.intents.pop_front().expect("scripted capture intent")
    }
}

impl TaskPrivilegedSupportActorV1 for CaptureOnlyPrivilegedActor {
    fn needs_privileged_support(&mut self, _view: &TaskActorViewV1) -> bool {
        false
    }

    fn receive_privileged_support(
        &mut self,
        _view: &TaskActorViewV1,
        _support: TaskPrivilegedSupportV1,
    ) -> Result<(), String> {
        Err("capture fixture must not receive privileged support".to_string())
    }
}

struct SupportFailureActor;

impl TaskActorV1 for SupportFailureActor {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::privileged_support_assisted("support-failure-fixture", 1)
    }

    fn next_intent(&mut self, _view: &TaskActorViewV1) -> TaskActorIntentV1 {
        panic!("support failure must terminate before actor intent")
    }
}

impl TaskPrivilegedSupportActorV1 for SupportFailureActor {
    fn needs_privileged_support(&mut self, _view: &TaskActorViewV1) -> bool {
        true
    }

    fn receive_privileged_support(
        &mut self,
        _view: &TaskActorViewV1,
        _support: TaskPrivilegedSupportV1,
    ) -> Result<(), String> {
        panic!("fixture world fails before support delivery")
    }
}

#[test]
fn privileged_support_failure_retains_typed_episode_and_fresh_root_capture() {
    let mut task = exact_branching_test_task();
    task.information_access =
        TaskInformationAccessV1::PublicHistoryLegalReloadMemoryAndPrivilegedSupport;
    let mut environment = NodeReloadTaskEnvironmentV1::new(
        HistoricalExactWorld {
            state: 0,
            pending: None,
        },
        task,
        TaskBudgetV1 {
            max_world_transitions: 1,
            max_reloads: 0,
        },
    )
    .expect("historical exact environment");
    let mut actor = SupportFailureActor;

    let (episode, captured_depths) =
        run_task_privileged_support_actor_episode_capturing_fresh_scopes_v1(
            &mut environment,
            &mut actor,
            |point| Ok(point.committed_history().steps.len()),
        )
        .expect("support failure is durable episode evidence");

    assert_eq!(captured_depths, vec![0]);
    let TaskEpisodeOutcomeV1::Failure {
        reason: TaskEpisodeFailureV1::MechanicsOrProtocolError { message },
        game_outcome: None,
    } = &episode.outcome
    else {
        panic!("expected typed protocol failure, got {:?}", episode.outcome);
    };
    assert!(
        message.contains("capture fixture never requests privileged support"),
        "unexpected support failure: {message}"
    );
}

#[test]
fn privileged_runner_captures_initial_and_rebased_fresh_scope_roots() {
    let budget = TaskBudgetV1 {
        max_world_transitions: 4,
        max_reloads: 0,
    };
    let mut environment = NodeReloadTaskEnvironmentV1::new(
        HistoricalExactWorld {
            state: 0,
            pending: None,
        },
        exact_branching_test_task(),
        budget,
    )
    .expect("historical exact environment");
    let mut actor = CaptureOnlyPrivilegedActor {
        intents: VecDeque::from([
            TaskActorIntentV1::Act {
                choice: TaskPublicChoiceV1::one(1),
            },
            TaskActorIntentV1::CommitScope,
            TaskActorIntentV1::Act {
                choice: TaskPublicChoiceV1::one(0),
            },
            TaskActorIntentV1::CommitScope,
        ]),
    };

    let (episode, captured_depths) =
        run_task_privileged_support_actor_episode_capturing_fresh_scopes_v1(
            &mut environment,
            &mut actor,
            |point| Ok(point.committed_history().steps.len()),
        )
        .expect("capture every fresh exact scope");

    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Success { .. }
    ));
    assert_eq!(captured_depths, vec![0, 1]);
}

struct ExactRootAlternatives;

impl TaskClosedLoopActorCandidateGeneratorV1<ExactRootPolicy> for ExactRootAlternatives {
    fn candidates(
        &mut self,
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskClosedLoopActorCandidateV1<ExactRootPolicy>>, String> {
        (0..2)
            .map(|ordinal| {
                TaskClosedLoopActorCandidateV1::new_public(
                    view,
                    decision,
                    format!("root-choice-{ordinal}"),
                    TaskProposalProvenanceV1::public_candidate_alternative(
                        "exact-root-alternatives",
                        1,
                    ),
                    ExactRootPolicy { ordinal },
                )
            })
            .collect()
    }
}

#[derive(Clone)]
struct ExactRootPolicy {
    ordinal: usize,
}

impl TaskActorV1 for ExactRootPolicy {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only(format!("exact-root-policy-{}", self.ordinal), 1)
    }

    fn next_intent(&mut self, _view: &TaskActorViewV1) -> TaskActorIntentV1 {
        TaskActorIntentV1::Act {
            choice: TaskPublicChoiceV1::one(self.ordinal),
        }
    }
}

struct SelectWinningExactTrial;

impl TaskExactBranchSelectionControllerV1 for SelectWinningExactTrial {
    fn select_trial(&mut self, trials: &[TaskExactBranchTrialV1]) -> Result<usize, String> {
        trials
            .iter()
            .position(|trial| {
                matches!(
                    trial.end,
                    TaskExactBranchTrialEndV1::ExploredScopeExit {
                        exit: TaskExploredScopeExitV1::GameTerminal {
                            game_outcome: GameOutcomeV1::ActThreeVictory
                        }
                    }
                )
            })
            .ok_or_else(|| "no winning exact branch".to_string())
    }
}

fn exact_branching_test_task() -> TaskSpecV1 {
    let mut task = TaskSpecV1::ironclad_a0_act_three_fail_closed();
    task.reload_protocol.protocol_name = "test_exact_branching_v1".to_string();
    task.reload_protocol.authorization = ReloadAuthorizationV1::ExactBranchingScopes {
        simulator_contract_reference: "typed fake exact world".to_string(),
    };
    task
}

#[test]
fn exact_branch_planner_observes_defeat_then_replays_and_commits_the_selected_win() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        ExactTerminalWorld { pending: None },
        exact_branching_test_task(),
        TaskBudgetV1 {
            max_world_transitions: 8,
            max_reloads: 2,
        },
    )
    .expect("exact branching test environment");
    let mut actor = TaskExactBranchPlannerActorV1::new(
        TaskActorIdentityV1::task_actor_view_only("exact-branch-planner-test", 1),
        ExactRootAlternatives,
        ExactRootPolicy { ordinal: 0 },
        SelectWinningExactTrial,
        4,
        2,
    )
    .expect("exact branch planner");

    let episode = run_task_actor_episode_v1(&mut env, &mut actor).expect("planned episode");

    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::ActThreeVictory
        }
    ));
    assert_eq!(episode.reload_intents, 2);
    assert_eq!(episode.final_view.committed_history.steps.len(), 1);
    assert_eq!(
        episode.final_view.committed_history.steps[0].step.choice,
        TaskPublicChoiceV1::one(1)
    );
    let [trace] = actor.selection_traces() else {
        panic!("one exact branch selection trace")
    };
    assert_eq!(trace.trials.len(), 2);
    assert!(matches!(
        trace.trials[0].end,
        TaskExactBranchTrialEndV1::ExploredScopeExit {
            exit: TaskExploredScopeExitV1::GameTerminal {
                game_outcome: GameOutcomeV1::Defeat
            }
        }
    ));
    assert_eq!(trace.trials[0].candidate.candidate_key, "root-choice-0");
}

#[test]
fn research_run_branch_restore_rewinds_history_without_carrying_future_memory() {
    let budget = TaskBudgetV1 {
        max_world_transitions: 4,
        max_reloads: 0,
    };
    let mut env = NodeReloadTaskEnvironmentV1::new(
        HistoricalExactWorld {
            state: 0,
            pending: None,
        },
        exact_branching_test_task(),
        budget,
    )
    .expect("historical exact environment");
    let root = env
        .capture_research_run_branch_point_v1()
        .expect("fresh run branch root");

    let losing = env
        .decision_handle()
        .expect("losing root handle")
        .into_command(TaskPublicChoiceV1::one(0));
    env.explore_choice(losing).expect("explore losing child");
    env.commit_current_attempt().expect("commit losing prefix");
    let terminal = env
        .decision_handle()
        .expect("losing terminal handle")
        .into_command(TaskPublicChoiceV1::one(0));
    env.explore_choice(terminal).expect("observe defeat");
    let defeated = env.commit_current_attempt().expect("commit defeat");
    assert!(matches!(
        defeated.boundary,
        TaskActorBoundaryV1::Terminal {
            outcome: TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::CommittedDefeat,
                ..
            }
        }
    ));
    assert_eq!(defeated.committed_history.steps.len(), 2);

    let restored = env
        .restore_research_run_branch_point_v1(&root, budget)
        .expect("restore earlier run branch");
    assert!(restored.committed_history.steps.is_empty());
    assert!(restored.legal_knowledge.completed_scopes.is_empty());
    let memory = restored.node_memory.expect("restored root memory");
    assert_eq!(memory.attempts.len(), 1);
    assert!(memory.attempts[0].steps.is_empty());
    assert_eq!(restored.budget.remaining_world_transitions, 4);

    let winning = env
        .decision_handle()
        .expect("winning root handle")
        .into_command(TaskPublicChoiceV1::one(1));
    env.explore_choice(winning).expect("explore winning child");
    env.commit_current_attempt().expect("commit winning prefix");
    let terminal = env
        .decision_handle()
        .expect("winning terminal handle")
        .into_command(TaskPublicChoiceV1::one(0));
    env.explore_choice(terminal).expect("observe victory");
    let won = env.commit_current_attempt().expect("commit victory");
    assert!(matches!(
        won.boundary,
        TaskActorBoundaryV1::Terminal {
            outcome: TaskEpisodeOutcomeV1::Success { .. }
        }
    ));
    assert_eq!(won.committed_history.steps.len(), 2);
    assert_eq!(
        won.committed_history.steps[0].step.choice,
        TaskPublicChoiceV1::one(1)
    );
}

#[test]
fn run_solution_archive_marks_only_complete_victory_prefixes_as_proven() {
    let budget = TaskBudgetV1 {
        max_world_transitions: 2,
        max_reloads: 0,
    };
    let run_episode = |choice| {
        let mut env = NodeReloadTaskEnvironmentV1::new(
            ExactTerminalWorld { pending: None },
            exact_branching_test_task(),
            budget,
        )
        .expect("exact terminal environment");
        let mut actor = ScriptedPublicActor::new([
            TaskActorIntentV1::Act {
                choice: TaskPublicChoiceV1::one(choice),
            },
            TaskActorIntentV1::CommitScope,
        ]);
        run_task_actor_episode_v1(&mut env, &mut actor).expect("terminal episode")
    };
    let lost = run_episode(0);
    let won = run_episode(1);
    let mut archive =
        TaskRunSolutionArchiveV1::new(exact_branching_test_task()).expect("run archive");
    archive
        .record_episode(7, "losing-execution", budget, &lost)
        .expect("record losing attempt");
    let losing_path = archive.attempts[0].node_path.clone();
    let losing_root = lost.final_view.committed_history.steps[0]
        .step
        .source
        .clone();
    assert_eq!(
        archive
            .node_id_for_committed_prefix(7, &losing_root, &lost.final_view.committed_history,)
            .expect("resolve losing prefix"),
        *losing_path.last().expect("losing path terminal")
    );
    assert!(archive
        .node_id_for_committed_prefix(8, &losing_root, &lost.final_view.committed_history)
        .is_err());
    assert!(archive
        .nodes
        .iter()
        .all(|node| matches!(node.proof, TaskRunArchiveNodeProofV1::ReachableUnknown)));

    let proving_attempt = archive
        .record_episode(7, "winning-execution", budget, &won)
        .expect("record winning attempt");
    let winning_path = archive.attempts[1].node_path.clone();
    assert_eq!(losing_path[0], winning_path[0]);
    assert_ne!(losing_path[1], winning_path[1]);
    assert!(matches!(
        archive.node(&losing_path[1]).expect("losing child").proof,
        TaskRunArchiveNodeProofV1::ReachableUnknown
    ));
    for node_id in &winning_path {
        assert!(matches!(
            &archive.node(node_id).expect("winning path node").proof,
            TaskRunArchiveNodeProofV1::ProvenVictory {
                first_proving_attempt_id
            } if first_proving_attempt_id == &proving_attempt
        ));
    }
    archive.validate().expect("durable archive validates");

    assert!(archive
        .retained_recovery_path_to_node_v1(&winning_path[0])
        .is_err());
    let fork_archive = archive
        .retained_fork_archive_to_node_v1(&winning_path[0])
        .expect("victory-path fork preserves its complete proof archive");
    assert_eq!(fork_archive, archive);
    fork_archive.validate().expect("fork archive validates");

    let mut wrong_schema = archive.clone();
    wrong_schema.schema.push_str("-tampered");
    assert!(wrong_schema.validate().is_err());

    let mut wrong_attempt_identity = archive;
    wrong_attempt_identity.attempts[0]
        .execution_manifest_id
        .push_str("-tampered");
    assert!(wrong_attempt_identity.validate().is_err());
}

#[test]
fn run_solution_archive_keeps_a_root_only_budget_or_support_failure_unknown() {
    let budget = TaskBudgetV1 {
        max_world_transitions: 1,
        max_reloads: 0,
    };
    let mut env = NodeReloadTaskEnvironmentV1::new(
        ExactTerminalWorld { pending: None },
        exact_branching_test_task(),
        budget,
    )
    .expect("exact terminal environment");
    let mut actor = ScriptedPublicActor::new([TaskActorIntentV1::UnsupportedDecisionSurface {
        reason: "no complete continuation yet".to_string(),
    }]);
    let episode = run_task_actor_episode_v1(&mut env, &mut actor).expect("unsupported episode");
    let mut archive =
        TaskRunSolutionArchiveV1::new(exact_branching_test_task()).expect("run archive");
    archive
        .record_episode(11, "root-only-execution", budget, &episode)
        .expect("record root-only attempt");

    assert_eq!(archive.nodes.len(), 1);
    assert_eq!(archive.attempts[0].node_path.len(), 1);
    assert!(matches!(
        archive.nodes[0].proof,
        TaskRunArchiveNodeProofV1::ReachableUnknown
    ));
}

#[test]
fn run_solution_archive_retains_reachable_active_scope_frontier_without_committing_it() {
    let budget = TaskBudgetV1 {
        max_world_transitions: 2,
        max_reloads: 0,
    };
    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::exact_branching(),
        exact_branching_test_task(),
        budget,
    )
    .expect("exact frontier environment");
    let mut actor = ScriptedPublicActor::new([
        TaskActorIntentV1::Act {
            choice: TaskPublicChoiceV1::one(0),
        },
        TaskActorIntentV1::UnsupportedDecisionSurface {
            reason: "frontier decision unsupported".to_string(),
        },
    ]);
    let episode = run_task_actor_episode_v1(&mut env, &mut actor).expect("frontier episode");
    assert!(episode.final_view.committed_history.steps.is_empty());
    assert_eq!(
        episode
            .final_view
            .node_memory
            .as_ref()
            .expect("active scope")
            .attempts[0]
            .steps
            .len(),
        1
    );

    let mut archive =
        TaskRunSolutionArchiveV1::new(exact_branching_test_task()).expect("run archive");
    archive
        .record_episode(13, "frontier-execution", budget, &episode)
        .expect("record frontier");
    assert_eq!(archive.attempts[0].node_path.len(), 1);
    assert_eq!(archive.attempts[0].observed_frontier_node_ids.len(), 1);
    assert_eq!(archive.nodes.len(), 2);
    let frontier = archive
        .node(&archive.attempts[0].observed_frontier_node_ids[0])
        .expect("frontier node");
    assert_eq!(frontier.depth, 1);
    assert!(matches!(
        frontier.proof,
        TaskRunArchiveNodeProofV1::ReachableUnknown
    ));
    assert_eq!(
        frontier
            .incoming_step
            .as_ref()
            .expect("frontier incoming step")
            .step
            .choice,
        TaskPublicChoiceV1::one(0)
    );
    archive.validate().expect("frontier archive validates");
    let encoded = serde_json::to_vec(&archive).expect("encode frontier archive");
    let decoded: TaskRunSolutionArchiveV1 =
        serde_json::from_slice(&encoded).expect("decode frontier archive");
    decoded.validate().expect("decoded frontier validates");
}

#[derive(Clone)]
struct SameRootThenDivergePolicy {
    first: bool,
    second_choice: usize,
    expect_isolated_memory: bool,
}

impl TaskActorV1 for SameRootThenDivergePolicy {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only(
            format!("same-root-then-{}", self.second_choice),
            1,
        )
    }

    fn next_intent(&mut self, _view: &TaskActorViewV1) -> TaskActorIntentV1 {
        if self.expect_isolated_memory
            && (_view.node_memory.as_ref().is_some_and(|memory| {
                memory.attempts.len() != 1 || memory.attempts[0].attempt != TaskAttemptOrdinalV1(0)
            }) || _view.budget.remaining_reloads != 0
                || _view.budget.remaining_world_transitions != if self.first { 4 } else { 3 }
                || !matches!(
                    _view.reload_permission,
                    Some(ReloadPermissionV1::NoReload {
                        reason: NoReloadReasonV1::DisabledByTask
                    })
                ))
        {
            return TaskActorIntentV1::UnsupportedDecisionSurface {
                reason: "closed-loop candidate observed comparer-global control state".to_string(),
            };
        }
        let ordinal = if self.first {
            self.first = false;
            0
        } else {
            self.second_choice
        };
        TaskActorIntentV1::Act {
            choice: TaskPublicChoiceV1::one(ordinal),
        }
    }
}

struct SameRootThenDivergeCandidates;

impl TaskClosedLoopActorCandidateGeneratorV1<SameRootThenDivergePolicy>
    for SameRootThenDivergeCandidates
{
    fn candidates(
        &mut self,
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskClosedLoopActorCandidateV1<SameRootThenDivergePolicy>>, String> {
        [0usize, 1]
            .into_iter()
            .map(|second_choice| {
                TaskClosedLoopActorCandidateV1::new_public(
                    view,
                    decision,
                    format!("same-root-then-{second_choice}"),
                    TaskProposalProvenanceV1::public_structured_plan("same-root-then-diverge", 1),
                    SameRootThenDivergePolicy {
                        first: true,
                        second_choice,
                        expect_isolated_memory: true,
                    },
                )
            })
            .collect()
    }
}

struct SameRootDivergenceWorld {
    after_shared_root_action: bool,
    pending: Option<GameOutcomeV1>,
}

impl TaskWorldV1 for SameRootDivergenceWorld {
    type ScopeAnchor = FakeAnchor;
    type Error = String;

    fn initial_scope(&self) -> Result<ReloadScopeV1<Self::ScopeAnchor>, Self::Error> {
        Ok(ReloadScopeV1 {
            public_root: decision("shared-root", TaskDecisionSiteV1::Map, 1),
            restore_anchor: FakeAnchor {
                _private_world_marker: 0x5678,
            },
            reload_permission: ReloadPermissionV1::ReloadToExactScopeRoot,
        })
    }

    fn restore_scope_anchor(
        &mut self,
        _anchor: &Self::ScopeAnchor,
    ) -> Result<TaskPublicDecisionV1, Self::Error> {
        self.after_shared_root_action = false;
        self.pending = None;
        Ok(decision("shared-root", TaskDecisionSiteV1::Map, 1))
    }

    fn explore_choice(
        &mut self,
        choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldExplorationV1, Self::Error> {
        if !self.after_shared_root_action {
            if choice != &TaskPublicChoiceV1::one(0) {
                return Err("both policies must take the shared root action".to_string());
            }
            self.after_shared_root_action = true;
            return Ok(TaskWorldExplorationV1::Decision(decision(
                "divergence",
                TaskDecisionSiteV1::Map,
                2,
            )));
        }
        let game_outcome = match choice.ordinal_path.as_slice() {
            [0] => GameOutcomeV1::Defeat,
            [1] => GameOutcomeV1::ActThreeVictory,
            _ => return Err("unsupported divergence choice".to_string()),
        };
        self.pending = Some(game_outcome);
        Ok(TaskWorldExplorationV1::ExploredScopeExit(
            TaskExploredScopeExitV1::GameTerminal { game_outcome },
        ))
    }

    fn commit_explored_scope(
        &mut self,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        Ok(TaskWorldCommitV1::Terminal(
            self.pending
                .take()
                .ok_or_else(|| "missing divergence terminal".to_string())?,
        ))
    }

    fn commit_choice(
        &mut self,
        _choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        Err("same-root divergence world requires exploration".to_string())
    }
}

#[test]
fn exact_branch_compares_closed_loop_policies_that_share_the_same_root_action() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        SameRootDivergenceWorld {
            after_shared_root_action: false,
            pending: None,
        },
        exact_branching_test_task(),
        TaskBudgetV1 {
            max_world_transitions: 12,
            max_reloads: 2,
        },
    )
    .expect("same-root divergence environment");
    let mut actor = TaskExactBranchPlannerActorV1::new(
        TaskActorIdentityV1::task_actor_view_only("closed-loop-policy-comparer-test", 1),
        SameRootThenDivergeCandidates,
        SameRootThenDivergePolicy {
            first: true,
            second_choice: 0,
            expect_isolated_memory: false,
        },
        SelectWinningExactTrial,
        4,
        2,
    )
    .expect("closed-loop exact comparer");

    let episode = run_task_actor_episode_v1(&mut env, &mut actor).expect("planned episode");

    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::ActThreeVictory
        }
    ));
    let [trace] = actor.selection_traces() else {
        panic!("one exact branch selection trace")
    };
    assert_eq!(trace.trials.len(), 2);
    assert_eq!(trace.trials[0].steps.len(), 2);
    assert_eq!(trace.trials[1].steps.len(), 2);
    assert_eq!(
        trace.trials[0].steps[0].choice, trace.trials[1].steps[0].choice,
        "candidate identity must not be reduced to the root action"
    );
    assert_ne!(
        trace.trials[0].steps[1].choice,
        trace.trials[1].steps[1].choice
    );
    assert_eq!(
        trace.selected_candidate_id,
        trace.trials[1].candidate.candidate_id
    );
    assert_eq!(episode.final_view.committed_history.steps.len(), 2);
    assert_eq!(
        episode.final_view.committed_history.steps[1].step.choice,
        TaskPublicChoiceV1::one(1)
    );
}

#[derive(Clone)]
struct StopAfterOneActionPolicy {
    choice: Option<usize>,
}

impl TaskActorV1 for StopAfterOneActionPolicy {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only("stop-after-one-action-policy", 1)
    }

    fn next_intent(&mut self, _view: &TaskActorViewV1) -> TaskActorIntentV1 {
        match self.choice.take() {
            Some(ordinal) => TaskActorIntentV1::Act {
                choice: TaskPublicChoiceV1::one(ordinal),
            },
            None => TaskActorIntentV1::CommitScope,
        }
    }
}

struct StopAfterOneActionCandidates;

impl TaskClosedLoopActorCandidateGeneratorV1<StopAfterOneActionPolicy>
    for StopAfterOneActionCandidates
{
    fn candidates(
        &mut self,
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskClosedLoopActorCandidateV1<StopAfterOneActionPolicy>>, String> {
        [0usize, 1]
            .into_iter()
            .map(|choice| {
                TaskClosedLoopActorCandidateV1::new_public(
                    view,
                    decision,
                    format!("stop-after-choice-{choice}"),
                    TaskProposalProvenanceV1::public_structured_plan("stop-after-one-action", 1),
                    StopAfterOneActionPolicy {
                        choice: Some(choice),
                    },
                )
            })
            .collect()
    }
}

struct SelectSecondStoppedTrial;

impl TaskExactBranchSelectionControllerV1 for SelectSecondStoppedTrial {
    fn select_trial(&mut self, trials: &[TaskExactBranchTrialV1]) -> Result<usize, String> {
        if trials.len() == 2
            && trials.iter().all(|trial| {
                matches!(
                    trial.end,
                    TaskExactBranchTrialEndV1::ControllerStopped { .. }
                )
            })
        {
            Ok(1)
        } else {
            Err("expected two controller-stopped trials".to_string())
        }
    }
}

#[derive(Clone)]
struct TwoActionPolicy {
    choices: std::collections::VecDeque<usize>,
}

impl TaskActorV1 for TwoActionPolicy {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only("two-action-policy", 1)
    }

    fn next_intent(&mut self, _view: &TaskActorViewV1) -> TaskActorIntentV1 {
        self.choices
            .pop_front()
            .map_or(TaskActorIntentV1::CommitScope, |ordinal| {
                TaskActorIntentV1::Act {
                    choice: TaskPublicChoiceV1::one(ordinal),
                }
            })
    }
}

fn replay_first_step_only(
    _actor: &TwoActionPolicy,
    _steps: &[TaskPublicStepV1],
) -> Result<usize, String> {
    Ok(1)
}

struct EvaluateTwoStepsCommitOneCandidates;

impl TaskClosedLoopActorCandidateGeneratorV1<TwoActionPolicy>
    for EvaluateTwoStepsCommitOneCandidates
{
    fn candidates(
        &mut self,
        view: &TaskActorViewV1,
        decision: &TaskPublicDecisionV1,
    ) -> Result<Vec<TaskClosedLoopActorCandidateV1<TwoActionPolicy>>, String> {
        [0usize, 1]
            .into_iter()
            .map(|choice| {
                TaskClosedLoopActorCandidateV1::new_public_with_replay_prefix(
                    view,
                    decision,
                    format!("evaluate-two-commit-one-{choice}"),
                    TaskProposalProvenanceV1::public_structured_plan("evaluate-two-commit-one", 1),
                    TwoActionPolicy {
                        choices: [choice, 0].into_iter().collect(),
                    },
                    replay_first_step_only,
                )
            })
            .collect()
    }
}

#[derive(Clone)]
struct FinishRebasedScope;

impl TaskActorV1 for FinishRebasedScope {
    fn identity(&self) -> TaskActorIdentityV1 {
        TaskActorIdentityV1::task_actor_view_only("finish-rebased-scope", 1)
    }

    fn next_intent(&mut self, view: &TaskActorViewV1) -> TaskActorIntentV1 {
        match view.boundary {
            TaskActorBoundaryV1::Decision { .. } => TaskActorIntentV1::Act {
                choice: TaskPublicChoiceV1::one(0),
            },
            TaskActorBoundaryV1::ExploredScopeExit { .. } => TaskActorIntentV1::CommitScope,
            _ => TaskActorIntentV1::UnsupportedDecisionSurface {
                reason: "unexpected rebased-scope boundary".to_string(),
            },
        }
    }
}

struct ControllerStopWorld {
    position: u8,
    selected_root_choice: Option<usize>,
    pending_terminal: bool,
}

impl TaskWorldV1 for ControllerStopWorld {
    type ScopeAnchor = FakeAnchor;
    type Error = String;

    fn initial_scope(&self) -> Result<ReloadScopeV1<Self::ScopeAnchor>, Self::Error> {
        Ok(ReloadScopeV1 {
            public_root: decision("controller-stop-root", TaskDecisionSiteV1::Map, 2),
            restore_anchor: FakeAnchor {
                _private_world_marker: 0,
            },
            reload_permission: ReloadPermissionV1::ReloadToExactScopeRoot,
        })
    }

    fn restore_scope_anchor(
        &mut self,
        anchor: &Self::ScopeAnchor,
    ) -> Result<TaskPublicDecisionV1, Self::Error> {
        self.position = anchor._private_world_marker as u8;
        self.selected_root_choice = None;
        self.pending_terminal = false;
        Ok(if self.position == 0 {
            decision("controller-stop-root", TaskDecisionSiteV1::Map, 2)
        } else {
            decision("rebased-after-stop", TaskDecisionSiteV1::Map, 1)
        })
    }

    fn explore_choice(
        &mut self,
        choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldExplorationV1, Self::Error> {
        if self.position == 0 {
            let [ordinal] = choice.ordinal_path.as_slice() else {
                return Err("controller-stop root requires one ordinal".to_string());
            };
            self.selected_root_choice = Some(*ordinal);
            self.position = 1;
            return Ok(TaskWorldExplorationV1::Decision(decision(
                &format!("after-root-choice-{ordinal}"),
                TaskDecisionSiteV1::Map,
                1,
            )));
        }
        if choice != &TaskPublicChoiceV1::one(0) {
            return Err("rebased scope requires its only legal choice".to_string());
        }
        self.pending_terminal = true;
        Ok(TaskWorldExplorationV1::ExploredScopeExit(
            TaskExploredScopeExitV1::GameTerminal {
                game_outcome: GameOutcomeV1::ActThreeVictory,
            },
        ))
    }

    fn commit_explored_scope(
        &mut self,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        if self.pending_terminal {
            self.pending_terminal = false;
            return Ok(TaskWorldCommitV1::Terminal(GameOutcomeV1::ActThreeVictory));
        }
        if self.position != 1 || self.selected_root_choice.is_none() {
            return Err("controller stop requires a non-empty root prefix".to_string());
        }
        Ok(TaskWorldCommitV1::NextScope(ReloadScopeV1 {
            public_root: decision("rebased-after-stop", TaskDecisionSiteV1::Map, 1),
            restore_anchor: FakeAnchor {
                _private_world_marker: 1,
            },
            reload_permission: ReloadPermissionV1::ReloadToExactScopeRoot,
        }))
    }

    fn commit_choice(
        &mut self,
        _choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        Err("controller-stop world requires provisional exploration".to_string())
    }
}

#[test]
fn exact_branch_controller_can_stop_replay_and_rebase_a_non_empty_prefix() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        ControllerStopWorld {
            position: 0,
            selected_root_choice: None,
            pending_terminal: false,
        },
        exact_branching_test_task(),
        TaskBudgetV1 {
            max_world_transitions: 8,
            max_reloads: 2,
        },
    )
    .expect("controller-stop environment");
    let mut actor = TaskExactBranchPlannerActorV1::new(
        TaskActorIdentityV1::task_actor_view_only("controller-stop-comparer-test", 1),
        StopAfterOneActionCandidates,
        FinishRebasedScope,
        SelectSecondStoppedTrial,
        4,
        2,
    )
    .expect("controller-stop comparer");

    let episode = run_task_actor_episode_v1(&mut env, &mut actor).expect("controller-stop episode");

    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::ActThreeVictory
        }
    ));
    let [trace] = actor.selection_traces() else {
        panic!("one controller-stop selection trace")
    };
    assert!(trace.trials.iter().all(|trial| {
        trial.steps.len() == 1
            && matches!(
                trial.end,
                TaskExactBranchTrialEndV1::ControllerStopped { .. }
            )
    }));
    assert_eq!(
        episode.final_view.committed_history.completed_scopes.len(),
        2
    );
    assert_eq!(episode.final_view.committed_history.steps.len(), 2);
    assert_eq!(
        episode.final_view.committed_history.steps[0].step.choice,
        TaskPublicChoiceV1::one(1),
        "only the selected stopped prefix is committed before replanning"
    );
}

#[test]
fn exact_branch_can_evaluate_a_longer_trial_than_it_commits() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        ControllerStopWorld {
            position: 0,
            selected_root_choice: None,
            pending_terminal: false,
        },
        exact_branching_test_task(),
        TaskBudgetV1 {
            max_world_transitions: 8,
            max_reloads: 2,
        },
    )
    .expect("receding-horizon environment");
    let mut actor = TaskExactBranchPlannerActorV1::new(
        TaskActorIdentityV1::task_actor_view_only("receding-horizon-comparer-test", 1),
        EvaluateTwoStepsCommitOneCandidates,
        FinishRebasedScope,
        SelectWinningExactTrial,
        4,
        2,
    )
    .expect("receding-horizon comparer");

    let episode =
        run_task_actor_episode_v1(&mut env, &mut actor).expect("receding-horizon episode");

    assert!(matches!(
        episode.outcome,
        TaskEpisodeOutcomeV1::Success {
            game_outcome: GameOutcomeV1::ActThreeVictory
        }
    ));
    let [trace] = actor.selection_traces() else {
        panic!("one receding-horizon selection trace")
    };
    assert!(trace.trials.iter().all(|trial| {
        trial.steps.len() == 2
            && trial.replay_prefix_steps == 1
            && matches!(
                trial.end,
                TaskExactBranchTrialEndV1::ExploredScopeExit {
                    exit: TaskExploredScopeExitV1::GameTerminal {
                        game_outcome: GameOutcomeV1::ActThreeVictory
                    }
                }
            )
    }));
    assert_eq!(
        episode.final_view.committed_history.steps[0].step.choice,
        TaskPublicChoiceV1::one(0)
    );
}

#[test]
fn stale_decision_capability_cannot_advance_or_spend_budget() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::reloadable(),
        reloadable_map_task(),
        TaskBudgetV1 {
            max_world_transitions: 4,
            max_reloads: 1,
        },
    )
    .expect("task environment");
    let stale = env
        .decision_handle()
        .expect("root decision handle")
        .into_command(TaskPublicChoiceV1::one(1));
    let reloaded = env.reload().expect("reload without taking a branch");

    assert_eq!(
        env.explore_choice(stale),
        Err(TaskCommandErrorV1::StaleDecisionCapability)
    );
    assert_eq!(env.actor_view(), reloaded);
}

#[test]
fn a_formal_game_defeat_deletes_the_reloadable_scope() {
    let mut env = NodeReloadTaskEnvironmentV1::new(
        FakeWorld::reloadable(),
        reloadable_map_task(),
        TaskBudgetV1 {
            max_world_transitions: 1,
            max_reloads: 1,
        },
    )
    .expect("task environment");
    let command = env
        .decision_handle()
        .expect("root decision handle")
        .into_command(TaskPublicChoiceV1::one(2));
    let terminal = env
        .explore_choice(command)
        .expect("formal defeat is a committed game terminal");

    assert!(matches!(
        terminal.boundary,
        TaskActorBoundaryV1::Terminal {
            outcome: TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::CommittedDefeat,
                game_outcome: Some(GameOutcomeV1::Defeat),
            }
        }
    ));
    assert_eq!(env.reload(), Err(TaskCommandErrorV1::EpisodeTerminal));
}

#[test]
fn strategic_task_projection_uses_public_ordinals_not_private_identities() {
    fn project(card_slot: u32, potion_uuid: u32) -> TaskPublicDecisionV1 {
        let card_lineage = crate::test_support::scenario_lineage(card_slot);
        let mechanics = PlannerMechanicsManifest {
            mechanics_id: "test-mechanics".to_string(),
            mechanics_version: 1,
        };
        let observation = crate::ai::planner_core::PlannerObservation {
            schema_name: "PlannerObservation".to_string(),
            schema_version: 1,
            observation_id: "public-observation".to_string(),
            mechanics: mechanics.clone(),
            run_goal: PlannerRunGoal::ActThreeVictory,
            decision_site: PlannerDecisionSite::Campfire,
            run: PlannerRunScalars {
                player_class: PlannerPlayerClass::Ironclad,
                ascension_level: 0,
                act: 1,
                floor: 4,
                current_hp: 60,
                max_hp: 80,
                gold: 99,
                keys: [false; 3],
                potion_capacity: 3,
                pending_act_entry: None,
            },
            cards: vec![PlannerCardObservation {
                card_lineage,
                card: CardId::Strike,
                upgrades: 0,
                misc_value: 0,
                base_damage_override: None,
                base_block_override: None,
                cost_modifier: 0,
            }],
            relics: Vec::new(),
            potions: vec![PlannerPotionSlotObservation {
                slot: 0,
                potion: Some(PlannerPotionObservation {
                    potion: PotionId::DexterityPotion,
                    potion_uuid,
                    can_use: false,
                    can_discard: true,
                    requires_target: false,
                }),
            }],
            public_map: PlannerPublicMap {
                current_x: 0,
                current_y: 4,
                boss: None,
                nodes: Vec::new(),
            },
            context: PlannerDecisionContext::Campfire,
            public_history: PlannerPublicHistory {
                shop_purge_count: 0,
                recent_combat_hp_loss: Some(7),
                recent_combat_turns: Some(3),
                recent_combat_potions_used: Some(0),
            },
        };
        let candidates = LegalCandidateSet {
            schema_name: "LegalCandidateSet".to_string(),
            schema_version: 2,
            candidate_set_id: "private-candidate-set".to_string(),
            decision_id: "private-decision".to_string(),
            observation_id: observation.observation_id.clone(),
            site: PlannerDecisionSite::Campfire,
            candidates: vec![LegalCandidate {
                candidate_id: "private-candidate".to_string(),
                action: PlannerAction::Smith {
                    card_lineage,
                    card: CardId::Strike,
                    upgrades: 0,
                },
                mechanics,
            }],
            completeness: CandidateSetCompleteness::Complete {
                basis: CandidateCompletenessBasis::RunControlBoundaryEnumerator,
            },
        };
        task_public_strategic_decision_v1(&observation, &candidates)
            .expect("public strategic decision")
    }

    let first = project(101, 201);
    let same_public_state = project(909, 707);

    assert_eq!(first, same_public_state);
    assert!(matches!(
        &first.candidates[0],
        TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Smith {
                card_ordinal: 0,
                card: CardId::Strike,
                ..
            }
        }
    ));
}
