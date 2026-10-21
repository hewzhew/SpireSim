use super::task_world_support::{
    game_outcome_for_learning_terminal_v1,
    natural_ironclad_learning_env_with_neow_and_objective_v1, next_autosave_generation_v1,
    prepare_task_choice_v1, task_public_decision_v1, AutosaveBoundaryFactsV1,
};
use super::{LearningBoundaryV1, LearningEnvCheckpointV3, LearningEnvV3};
use crate::agent::task::{
    IrreversibleCommitBoundaryV1, NativeReloadAnchorV1, NodeReloadTaskEnvironmentV1,
    ReloadAuthorizationV1, ReloadPermissionV1, ReloadScopeV1, TaskBudgetV1, TaskObjectiveV1,
    TaskPublicChoiceV1, TaskPublicDecisionV1, TaskSpecV1, TaskWorldCommitV1,
    TaskWorldExplorationV1, TaskWorldV1,
};
use crate::content::events::neow::NeowEligibilityV1;

/// Simulator-owned reconstruction of one source-audited original-game
/// autosave generation. The exact checkpoint remains private to the task
/// environment and is never part of the actor view.
#[derive(Clone, Debug, PartialEq)]
pub struct SimulatorNativeAutosaveAnchorV1 {
    generation: u64,
    checkpoint: LearningEnvCheckpointV3,
}

#[derive(Clone, Debug)]
struct LiveGenerationV1 {
    generation: u64,
    kind: NativeReloadAnchorV1,
    checkpoint: LearningEnvCheckpointV3,
    public_root: TaskPublicDecisionV1,
}

#[derive(Clone, Debug)]
struct PendingGenerationV1 {
    env: LearningEnvV3,
    kind: NativeReloadAnchorV1,
}

/// Thin native-autosave owner over exact run execution.
///
/// Production construction starts only at a natural new-game Neow entrance.
/// There is deliberately no public constructor from an arbitrary exact
/// checkpoint: such a checkpoint is research capability, not reload
/// permission.
#[derive(Clone, Debug)]
pub struct NativeAutosaveTaskWorldV1 {
    env: LearningEnvV3,
    live: LiveGenerationV1,
    live_deleted: bool,
    pending: Option<PendingGenerationV1>,
    next_generation: u64,
}

impl NativeAutosaveTaskWorldV1 {
    pub fn new_ironclad_a0(seed: u64) -> Result<Self, String> {
        Self::new_ironclad(seed, 0, TaskObjectiveV1::ActThreeVictory)
    }

    pub fn new_ironclad(
        seed: u64,
        ascension_level: u8,
        objective: TaskObjectiveV1,
    ) -> Result<Self, String> {
        let env = natural_ironclad_learning_env_with_neow_and_objective_v1(
            seed,
            ascension_level,
            NeowEligibilityV1::Rich,
            objective,
        )?;
        Self::from_initial_env(env, NativeReloadAnchorV1::EnterRoomAutosave)
    }

    fn from_initial_env(env: LearningEnvV3, kind: NativeReloadAnchorV1) -> Result<Self, String> {
        let public_root = task_public_decision_v1(&env)?;
        let checkpoint = env.checkpoint();
        Ok(Self {
            env,
            live: LiveGenerationV1 {
                generation: 0,
                kind,
                checkpoint,
                public_root,
            },
            live_deleted: false,
            pending: None,
            next_generation: 1,
        })
    }

    #[cfg(test)]
    fn from_native_test_session(
        session: super::RunControlSession,
        kind: NativeReloadAnchorV1,
    ) -> Result<Self, String> {
        Self::from_initial_env(LearningEnvV3::from_session(session), kind)
    }

    fn reload_scope(&self) -> ReloadScopeV1<SimulatorNativeAutosaveAnchorV1> {
        ReloadScopeV1 {
            public_root: self.live.public_root.clone(),
            restore_anchor: SimulatorNativeAutosaveAnchorV1 {
                generation: self.live.generation,
                checkpoint: self.live.checkpoint.clone(),
            },
            reload_permission: ReloadPermissionV1::ReloadToNativeAnchor {
                native_anchor: self.live.kind,
                irreversible_commit:
                    IrreversibleCommitBoundaryV1::AutosaveGenerationReplacedOrDeleted,
            },
        }
    }

    fn trial_choice(&self, choice: &TaskPublicChoiceV1) -> Result<TrialChoiceV1, String> {
        if self.pending.is_some() {
            return Err("native autosave task already has a pending generation".to_string());
        }
        let before = AutosaveBoundaryFactsV1::capture(&self.env);
        let prepared = prepare_task_choice_v1(&self.env, choice)?;
        let mut trial = self.env.clone();
        let step = trial.step_prepared(prepared)?;
        match step.boundary {
            LearningBoundaryV1::Terminal { outcome } => Ok(TrialChoiceV1::Terminal {
                outcome: game_outcome_for_learning_terminal_v1(&trial, outcome.result),
                env: trial,
            }),
            LearningBoundaryV1::Unsupported => {
                Err("native autosave task reached an unsupported boundary".to_string())
            }
            LearningBoundaryV1::Strategic { .. } | LearningBoundaryV1::Combat { .. } => {
                let after = AutosaveBoundaryFactsV1::capture(&trial);
                if let Some(kind) = next_autosave_generation_v1(self.live.kind, before, after) {
                    Ok(TrialChoiceV1::NextGeneration { env: trial, kind })
                } else {
                    let decision = task_public_decision_v1(&trial)?;
                    Ok(TrialChoiceV1::InScope {
                        env: trial,
                        decision,
                    })
                }
            }
        }
    }

    fn install_generation(
        &mut self,
        env: LearningEnvV3,
        kind: NativeReloadAnchorV1,
    ) -> Result<ReloadScopeV1<SimulatorNativeAutosaveAnchorV1>, String> {
        let public_root = task_public_decision_v1(&env)?;
        let checkpoint = env.checkpoint();
        let generation = self.next_generation;
        self.next_generation = self.next_generation.saturating_add(1);
        self.env = env;
        self.live = LiveGenerationV1 {
            generation,
            kind,
            checkpoint,
            public_root,
        };
        self.live_deleted = false;
        self.pending = None;
        Ok(self.reload_scope())
    }
}

enum TrialChoiceV1 {
    InScope {
        env: LearningEnvV3,
        decision: TaskPublicDecisionV1,
    },
    NextGeneration {
        env: LearningEnvV3,
        kind: NativeReloadAnchorV1,
    },
    Terminal {
        env: LearningEnvV3,
        outcome: crate::agent::task::GameOutcomeV1,
    },
}

impl TaskWorldV1 for NativeAutosaveTaskWorldV1 {
    type ScopeAnchor = SimulatorNativeAutosaveAnchorV1;
    type Error = String;

    fn initial_scope(&self) -> Result<ReloadScopeV1<Self::ScopeAnchor>, Self::Error> {
        Ok(self.reload_scope())
    }

    fn restore_scope_anchor(
        &mut self,
        anchor: &Self::ScopeAnchor,
    ) -> Result<TaskPublicDecisionV1, Self::Error> {
        if self.live_deleted || anchor.generation != self.live.generation {
            return Err("native autosave generation has been replaced or deleted".to_string());
        }
        self.pending = None;
        self.env = LearningEnvV3::from_checkpoint(anchor.checkpoint.clone())?;
        let restored = task_public_decision_v1(&self.env)?;
        if restored != self.live.public_root {
            return Err("native autosave restore changed its public root".to_string());
        }
        Ok(restored)
    }

    fn explore_choice(
        &mut self,
        choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldExplorationV1, Self::Error> {
        match self.trial_choice(choice)? {
            TrialChoiceV1::InScope { env, decision } => {
                self.env = env;
                Ok(TaskWorldExplorationV1::Decision(decision))
            }
            TrialChoiceV1::NextGeneration { env, kind } => {
                self.pending = Some(PendingGenerationV1 { env, kind });
                Ok(TaskWorldExplorationV1::CommitReady)
            }
            TrialChoiceV1::Terminal { env, outcome } => {
                self.env = env;
                self.live_deleted = true;
                self.pending = None;
                Ok(TaskWorldExplorationV1::CommittedTerminal(outcome))
            }
        }
    }

    fn commit_explored_scope(
        &mut self,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        let pending = self
            .pending
            .take()
            .ok_or_else(|| "native autosave commit has no pending generation".to_string())?;
        let scope = self.install_generation(pending.env, pending.kind)?;
        Ok(TaskWorldCommitV1::NextScope(scope))
    }

    fn commit_choice(
        &mut self,
        choice: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldCommitV1<Self::ScopeAnchor>, Self::Error> {
        match self.trial_choice(choice)? {
            TrialChoiceV1::InScope { env, decision } => {
                self.env = env;
                Ok(TaskWorldCommitV1::Decision(decision))
            }
            TrialChoiceV1::NextGeneration { env, kind } => Ok(TaskWorldCommitV1::NextScope(
                self.install_generation(env, kind)?,
            )),
            TrialChoiceV1::Terminal { env, outcome } => {
                self.env = env;
                self.live_deleted = true;
                self.pending = None;
                Ok(TaskWorldCommitV1::Terminal(outcome))
            }
        }
    }
}

pub fn ironclad_a0_native_autosave_task_spec_v1() -> TaskSpecV1 {
    ironclad_native_autosave_task_spec_v1(0, TaskObjectiveV1::ActThreeVictory)
}

pub fn ironclad_native_autosave_task_spec_v1(
    ascension_level: u8,
    objective: TaskObjectiveV1,
) -> TaskSpecV1 {
    let mut task = TaskSpecV1::ironclad_a0_act_three_fail_closed();
    task.ascension_level = ascension_level;
    task.objective = objective;
    task.reload_protocol.protocol_name =
        "original_pc_v2_3_4_simulator_native_autosave_v1".to_string();
    task.reload_protocol.authorization = ReloadAuthorizationV1::NativeAutosaveScopes {
        native_save_model:
            crate::agent::task::NativeSaveModelV1::SourceAuditedAutosaveGenerations {
                game_build: "V2.3.4 / 2022-12-18".to_string(),
                source_reference: "docs/architecture/original-save-boundaries.md#source-audit"
                    .to_string(),
            },
        simulator_contract_reference:
            "src/eval/run_control/native_autosave_task_world.rs owning parity tests".to_string(),
    };
    task.validate()
        .expect("native autosave task specification must be valid");
    task
}

pub fn ironclad_a0_native_autosave_environment_v1(
    seed: u64,
    budget: TaskBudgetV1,
) -> Result<NodeReloadTaskEnvironmentV1<NativeAutosaveTaskWorldV1>, String> {
    ironclad_native_autosave_environment_v1(seed, 0, TaskObjectiveV1::ActThreeVictory, budget)
}

pub fn ironclad_native_autosave_environment_v1(
    seed: u64,
    ascension_level: u8,
    objective: TaskObjectiveV1,
    budget: TaskBudgetV1,
) -> Result<NodeReloadTaskEnvironmentV1<NativeAutosaveTaskWorldV1>, String> {
    NodeReloadTaskEnvironmentV1::new(
        NativeAutosaveTaskWorldV1::new_ironclad(seed, ascension_level, objective)?,
        ironclad_native_autosave_task_spec_v1(ascension_level, objective),
        budget,
    )
}

#[cfg(test)]
mod tests {
    use super::super::RunControlConfig;
    use super::*;
    use crate::agent::task::{
        PublicCombatPlanFollowerEventV1, TaskActorBoundaryV1, TaskActorIdentityV1,
        TaskActorIntentV1, TaskActorV1, TaskDecisionSiteV1, TaskPublicCandidateV1,
        TaskPublicCombatPlanFollowerActorV1, VulnerableSetupPayoffPlanGeneratorV1,
    };
    use crate::content::cards::CardId;
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ActiveCombat, CombatContext, EngineState, RoomCombatContext};
    use crate::state::events::EventId;
    use crate::state::map::node::{MapEdge, MapRoomNode, RoomType};
    use crate::state::map::state::MapState;

    fn budget() -> TaskBudgetV1 {
        TaskBudgetV1 {
            max_world_transitions: 128,
            max_reloads: 16,
        }
    }

    #[test]
    fn natural_a20_heart_entry_matches_public_task_contract() {
        let environment = ironclad_native_autosave_environment_v1(
            91_001,
            20,
            TaskObjectiveV1::HeartVictory,
            budget(),
        )
        .expect("natural A20 Heart entrance");
        let view = environment.actor_view();
        assert_eq!(view.task.ascension_level, 20);
        assert_eq!(view.task.objective, TaskObjectiveV1::HeartVictory);
        assert!(matches!(
            view.task.initial_condition,
            crate::agent::task::TaskInitialConditionV1::NaturalNewGameEntrance
        ));
        let TaskActorBoundaryV1::Decision { decision } = view.boundary else {
            panic!("expected Neow decision");
        };
        let crate::agent::task::TaskPublicObservationV1::Strategic { observation } =
            decision.observation
        else {
            panic!("expected strategic observation");
        };
        assert_eq!(observation.run.ascension_level, 20);
        assert_eq!(observation.run.floor, 0);
    }

    fn command(
        env: &NodeReloadTaskEnvironmentV1<NativeAutosaveTaskWorldV1>,
        ordinal: usize,
    ) -> crate::agent::task::TaskDecisionCommandV1 {
        env.decision_handle()
            .expect("decision handle")
            .into_command(TaskPublicChoiceV1::one(ordinal))
    }

    fn candidate_ordinal(
        env: &NodeReloadTaskEnvironmentV1<NativeAutosaveTaskWorldV1>,
        predicate: impl Fn(&TaskPublicCandidateV1) -> bool,
    ) -> usize {
        let TaskActorBoundaryV1::Decision { decision } = &env.actor_view().boundary else {
            panic!("expected decision boundary");
        };
        decision
            .candidates
            .iter()
            .position(predicate)
            .expect("matching public candidate")
    }

    fn commit_ready(
        env: &mut NodeReloadTaskEnvironmentV1<NativeAutosaveTaskWorldV1>,
        ordinal: usize,
    ) {
        let view = env
            .explore_choice(command(env, ordinal))
            .expect("explore generation transition");
        assert!(
            matches!(view.boundary, TaskActorBoundaryV1::CommitReady),
            "expected autosave-generation commit, got {:?}",
            view.boundary
        );
        env.commit_current_attempt().expect("commit generation");
    }

    fn forward_choice(decision: &TaskPublicDecisionV1) -> TaskPublicChoiceV1 {
        for (ordinal, candidate) in decision.candidates.iter().enumerate() {
            match candidate {
                TaskPublicCandidateV1::Strategic {
                    action:
                        crate::agent::task::TaskStrategicActionV1::BeginRunCardSelection {
                            min_choices,
                            selectable_card_ordinals,
                            ..
                        },
                } if selectable_card_ordinals.len() >= *min_choices => {
                    let mut path = vec![ordinal];
                    path.extend(0..*min_choices);
                    return TaskPublicChoiceV1 { ordinal_path: path };
                }
                TaskPublicCandidateV1::Strategic {
                    action:
                        crate::agent::task::TaskStrategicActionV1::Proceed { .. }
                        | crate::agent::task::TaskStrategicActionV1::SkipCardReward { .. },
                } => return TaskPublicChoiceV1::one(ordinal),
                _ => {}
            }
        }
        TaskPublicChoiceV1::one(0)
    }

    fn finish_neow_scope(env: &mut NodeReloadTaskEnvironmentV1<NativeAutosaveTaskWorldV1>) {
        for _ in 0..16 {
            let TaskActorBoundaryV1::Decision { decision } = &env.actor_view().boundary else {
                panic!("Neow closure expected a decision");
            };
            let view = env
                .explore_choice(
                    env.decision_handle()
                        .expect("Neow decision handle")
                        .into_command(forward_choice(decision)),
                )
                .expect("advance Neow closure");
            if matches!(view.boundary, TaskActorBoundaryV1::CommitReady) {
                return;
            }
        }
        panic!("Neow closure did not reach POST_NEOW in 16 decisions");
    }

    fn native_jaw_worm_combat_environment(
        hand: &[CardId],
        monster_hp: i32,
    ) -> NodeReloadTaskEnvironmentV1<NativeAutosaveTaskWorldV1> {
        let mut session = super::super::RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = hand
            .iter()
            .enumerate()
            .map(|(index, card)| CombatCard::for_scenario(*card, 51 + index as u32))
            .collect();
        combat.meta.master_deck_snapshot = hand
            .iter()
            .enumerate()
            .map(|(index, card)| CombatCard::for_scenario(*card, 91 + index as u32))
            .collect::<Vec<_>>()
            .into();
        combat.meta.master_deck_lineage_allocator =
            session.run_state.master_deck_lineage_allocator.clone();
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.current_hp = monster_hp;
        monster.max_hp = monster_hp;
        monster.id = 7;
        monster.set_planned_move_id(1);
        let plan =
            crate::content::monsters::exordium::jaw_worm::JawWorm::turn_plan(&combat, &monster);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters.push(monster);
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new_for_encounter(
            EngineState::CombatPlayerTurn,
            combat,
            crate::content::monsters::factory::EncounterId::JawWorm,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let world = NativeAutosaveTaskWorldV1::from_native_test_session(
            session,
            NativeReloadAnchorV1::EnterRoomAutosave,
        )
        .expect("test native combat scope");
        NodeReloadTaskEnvironmentV1::new(
            world,
            ironclad_a0_native_autosave_task_spec_v1(),
            budget(),
        )
        .expect("task environment")
    }

    #[test]
    fn natural_neow_completion_replaces_enter_room_with_post_neow() {
        let world = NativeAutosaveTaskWorldV1::new_ironclad_a0(1)
            .expect("natural Ironclad A0 autosave world");
        let session = world.env.session();
        assert_eq!(session.run_state.act_num, 1);
        assert_eq!(session.run_state.floor_num, 0);
        assert_eq!(session.run_state.ascension_level, 0);
        assert_eq!(session.run_state.player_class, "Ironclad");
        assert!(matches!(session.engine_state, EngineState::EventRoom));
        assert!(session
            .run_state
            .event_state
            .as_ref()
            .is_some_and(|event| event.id == EventId::Neow));
        let mut env = NodeReloadTaskEnvironmentV1::new(
            world,
            ironclad_a0_native_autosave_task_spec_v1(),
            budget(),
        )
        .expect("native autosave environment");
        assert!(matches!(
            env.actor_view().reload_permission,
            Some(ReloadPermissionV1::ReloadToNativeAnchor {
                native_anchor: NativeReloadAnchorV1::EnterRoomAutosave,
                ..
            })
        ));

        finish_neow_scope(&mut env);
        let next = env.commit_current_attempt().expect("commit post-Neow save");
        assert!(matches!(
            next.reload_permission,
            Some(ReloadPermissionV1::ReloadToNativeAnchor {
                native_anchor: NativeReloadAnchorV1::PostNeowAutosave,
                ..
            })
        ));
        assert!(matches!(
            next.boundary,
            TaskActorBoundaryV1::Decision { ref decision }
                if decision.site == TaskDecisionSiteV1::Map
        ));
    }

    #[test]
    fn route_commit_opens_one_enter_room_generation_for_a_noncombat_room() {
        let mut session = super::super::RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        session.engine_state = EngineState::MapNavigation;
        let mut rest = MapRoomNode::new(0, 0);
        rest.class = Some(RoomType::RestRoom);
        rest.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut next = MapRoomNode::new(0, 1);
        next.class = Some(RoomType::MonsterRoom);
        session.run_state.map = MapState::new(vec![vec![rest], vec![next]]);
        let world = NativeAutosaveTaskWorldV1::from_native_test_session(
            session,
            NativeReloadAnchorV1::PostNeowAutosave,
        )
        .expect("test native map scope");
        let mut env = NodeReloadTaskEnvironmentV1::new(
            world,
            ironclad_a0_native_autosave_task_spec_v1(),
            budget(),
        )
        .expect("task environment");

        commit_ready(&mut env, 0);
        let entered = env.actor_view();
        assert!(matches!(
            entered.reload_permission,
            Some(ReloadPermissionV1::ReloadToNativeAnchor {
                native_anchor: NativeReloadAnchorV1::EnterRoomAutosave,
                ..
            })
        ));
        assert!(matches!(
            entered.boundary,
            TaskActorBoundaryV1::Decision { ref decision }
                if decision.site == TaskDecisionSiteV1::Campfire
        ));

        let root = entered.node_memory.expect("node memory").attempts[0]
            .root
            .clone();
        let rest_ordinal = candidate_ordinal(&env, |candidate| {
            matches!(
                candidate,
                TaskPublicCandidateV1::Strategic {
                    action: crate::agent::task::TaskStrategicActionV1::Rest
                }
            )
        });
        env.explore_choice(command(&env, rest_ordinal))
            .expect("rest inside room scope");
        let restored = env.reload().expect("reload room entrance");
        assert_eq!(
            restored.node_memory.expect("node memory").attempts[1].root,
            root
        );
    }

    #[test]
    fn combat_victory_replaces_enter_room_with_post_combat_and_replays_rewards() {
        let mut session = super::super::RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 51)];
        combat.meta.master_deck_snapshot =
            vec![CombatCard::for_scenario(CardId::Strike, 91)].into();
        combat.meta.master_deck_lineage_allocator =
            session.run_state.master_deck_lineage_allocator.clone();
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.current_hp = 1;
        monster.max_hp = 1;
        monster.id = 7;
        monster.set_planned_move_id(1);
        let plan =
            crate::content::monsters::exordium::jaw_worm::JawWorm::turn_plan(&combat, &monster);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters.push(monster);
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new_for_encounter(
            EngineState::CombatPlayerTurn,
            combat,
            crate::content::monsters::factory::EncounterId::JawWorm,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let world = NativeAutosaveTaskWorldV1::from_native_test_session(
            session,
            NativeReloadAnchorV1::EnterRoomAutosave,
        )
        .expect("test native combat scope");
        let mut env = NodeReloadTaskEnvironmentV1::new(
            world,
            ironclad_a0_native_autosave_task_spec_v1(),
            budget(),
        )
        .expect("task environment");

        let strike = candidate_ordinal(&env, |candidate| {
            matches!(
                candidate,
                TaskPublicCandidateV1::CombatAtomic {
                    action:
                        crate::agent::information::action::PublicCombatAtomicActionV1::PlayCard {
                            hand_index: 0,
                            target_monster_index: Some(0)
                        }
                }
            )
        });
        commit_ready(&mut env, strike);
        let post_combat = env.actor_view();
        assert!(matches!(
            post_combat.reload_permission,
            Some(ReloadPermissionV1::ReloadToNativeAnchor {
                native_anchor: NativeReloadAnchorV1::PostCombatAutosave,
                ..
            })
        ));

        let reward_root = post_combat.node_memory.expect("node memory").attempts[0]
            .root
            .clone();
        env.explore_choice(command(&env, 0))
            .expect("take or reveal first reward");
        let restored = env.reload().expect("restore post-combat rewards");
        assert_eq!(
            restored.node_memory.expect("node memory").attempts[1].root,
            reward_root
        );
    }

    #[derive(Clone, Debug, Default)]
    struct UnsupportedCombatPlanFallback;

    impl TaskActorV1 for UnsupportedCombatPlanFallback {
        fn identity(&self) -> TaskActorIdentityV1 {
            TaskActorIdentityV1::task_actor_view_only("native-combat-plan-test-fallback", 1)
        }

        fn next_intent(
            &mut self,
            _view: &crate::agent::task::TaskActorViewV1,
        ) -> TaskActorIntentV1 {
            TaskActorIntentV1::UnsupportedDecisionSurface {
                reason: "native combat plan fixture reached fallback".to_string(),
            }
        }
    }

    fn actor_choice(intent: TaskActorIntentV1) -> TaskPublicChoiceV1 {
        let TaskActorIntentV1::Act { choice } = intent else {
            panic!("expected public combat plan action, got {intent:?}")
        };
        choice
    }

    #[test]
    fn structured_public_combat_plan_executes_bash_then_strike_in_the_native_world() {
        // This must cross the real task adapter and typed combat resolver; a
        // hand-built successor view would not protect that integration.
        let mut env = native_jaw_worm_combat_environment(&[CardId::Bash, CardId::Strike], 40);
        let mut actor = TaskPublicCombatPlanFollowerActorV1::new(
            TaskActorIdentityV1::task_actor_view_only("native-public-combat-plan-test", 1),
            VulnerableSetupPayoffPlanGeneratorV1,
            UnsupportedCombatPlanFallback,
        );

        let setup_choice = actor_choice(actor.next_intent(&env.actor_view()));
        let after_setup = env
            .explore_choice(
                env.decision_handle()
                    .expect("setup decision handle")
                    .into_command(setup_choice),
            )
            .expect("execute grounded Bash");
        let TaskActorBoundaryV1::Decision {
            decision: payoff_decision,
        } = &after_setup.boundary
        else {
            panic!("nonlethal Bash must reach another public combat decision")
        };
        let crate::agent::task::TaskPublicObservationV1::Combat { observation, .. } =
            &payoff_decision.observation
        else {
            panic!("Bash successor must remain in combat")
        };
        assert!(observation.monsters[0]
            .powers
            .iter()
            .any(|power| power.power == crate::content::powers::PowerId::Vulnerable));
        let hp_after_setup = observation.monsters[0].hp;

        let payoff_choice = actor_choice(actor.next_intent(&after_setup));
        let after_payoff = env
            .explore_choice(
                env.decision_handle()
                    .expect("payoff decision handle")
                    .into_command(payoff_choice),
            )
            .expect("execute grounded Strike");
        let TaskActorBoundaryV1::Decision {
            decision: after_payoff_decision,
        } = &after_payoff.boundary
        else {
            panic!("fixture Strike must remain inside the combat scope")
        };
        let crate::agent::task::TaskPublicObservationV1::Combat { observation, .. } =
            &after_payoff_decision.observation
        else {
            panic!("Strike successor must remain in combat")
        };
        assert!(observation.monsters[0].hp < hp_after_setup);
        assert!(actor.events().iter().any(|event| matches!(
            event,
            PublicCombatPlanFollowerEventV1::SuccessorGrounded { .. }
        )));
    }

    #[test]
    fn stale_generation_anchor_is_rejected_after_route_commit() {
        let mut world =
            NativeAutosaveTaskWorldV1::new_ironclad_a0(1).expect("native autosave world");
        let stale = world.reload_scope().restore_anchor;
        let mut reached_commit = false;
        for _ in 0..16 {
            let decision = task_public_decision_v1(&world.env).expect("current Neow decision");
            match world
                .explore_choice(&forward_choice(&decision))
                .expect("advance Neow closure")
            {
                TaskWorldExplorationV1::CommitReady => {
                    reached_commit = true;
                    break;
                }
                TaskWorldExplorationV1::Decision(_) => {}
                TaskWorldExplorationV1::CommittedTerminal(_) => {
                    panic!("Neow unexpectedly terminated run")
                }
                TaskWorldExplorationV1::ExploredScopeExit(_) => {
                    panic!("native world unexpectedly exposed a research branch exit")
                }
            }
        }
        assert!(reached_commit, "Neow closure did not reach POST_NEOW");
        world.commit_explored_scope().expect("install post-Neow");

        assert_eq!(
            world.restore_scope_anchor(&stale),
            Err("native autosave generation has been replaced or deleted".to_string())
        );
        assert_ne!(world.live.generation, stale.generation);
    }

    #[test]
    fn committed_defeat_deletes_the_live_autosave_generation() {
        let mut session = super::super::RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        session.run_state.current_hp = 1;
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.player.current_hp = 1;
        combat.entities.player.max_hp = 80;
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        monster.set_planned_move_id(1);
        let plan =
            crate::content::monsters::exordium::jaw_worm::JawWorm::turn_plan(&combat, &monster);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters.push(monster);
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new_for_encounter(
            EngineState::CombatPlayerTurn,
            combat,
            crate::content::monsters::factory::EncounterId::JawWorm,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let mut world = NativeAutosaveTaskWorldV1::from_native_test_session(
            session,
            NativeReloadAnchorV1::EnterRoomAutosave,
        )
        .expect("test native combat scope");
        let live = world.reload_scope().restore_anchor;
        let decision = task_public_decision_v1(&world.env).expect("combat decision");
        let end_turn = decision
            .candidates
            .iter()
            .position(|candidate| {
                matches!(
                    candidate,
                    TaskPublicCandidateV1::CombatAtomic {
                        action:
                            crate::agent::information::action::PublicCombatAtomicActionV1::EndTurn
                    }
                )
            })
            .expect("end turn candidate");

        assert!(matches!(
            world
                .commit_choice(&TaskPublicChoiceV1::one(end_turn))
                .expect("commit lethal end turn"),
            TaskWorldCommitV1::Terminal(crate::agent::task::GameOutcomeV1::Defeat)
        ));
        assert_eq!(
            world.restore_scope_anchor(&live),
            Err("native autosave generation has been replaced or deleted".to_string())
        );
    }
}
