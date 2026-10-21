use super::*;
use crate::ai::planner_core::{
    PlannerDecisionContext, PlannerDecisionSite, PlannerMechanicsManifest, PlannerPlayerClass,
    PlannerPublicHistory, PlannerPublicMap, PlannerRunGoal, PlannerRunScalars,
};

#[derive(Clone, Copy, Debug)]
enum Behavior {
    Normal,
    MissingPolicy,
    BrokenTransition,
    PartialPlan,
}

// A four-boundary task-kernel fixture, not a simulator correctness test.
// Production mechanics and the reference policy are exercised by natural probes.
#[derive(Clone)]
struct World {
    stage: usize,
    behavior: Behavior,
}
impl World {
    fn decision(&self) -> TaskPublicDecisionV1 {
        decision(self.stage)
    }
}
impl TaskWorldV1 for World {
    type ScopeAnchor = usize;
    type Error = String;
    fn initial_scope(&self) -> Result<ReloadScopeV1<usize>, String> {
        Ok(ReloadScopeV1 {
            public_root: self.decision(),
            restore_anchor: self.stage,
            reload_permission: ReloadPermissionV1::ReloadToExactScopeRoot,
        })
    }
    fn restore_scope_anchor(&mut self, anchor: &usize) -> Result<TaskPublicDecisionV1, String> {
        self.stage = *anchor;
        Ok(self.decision())
    }
    fn explore_choice(&mut self, _: &TaskPublicChoiceV1) -> Result<TaskWorldExplorationV1, String> {
        match self.behavior {
            Behavior::BrokenTransition => return Err("fixture transition failed".to_string()),
            Behavior::PartialPlan if self.stage == 0 => {}
            _ => self.stage += 1,
        }
        Ok(TaskWorldExplorationV1::Decision(self.decision()))
    }
    fn commit_explored_scope(&mut self) -> Result<TaskWorldCommitV1<usize>, String> {
        Ok(TaskWorldCommitV1::NextScope(self.initial_scope()?))
    }
    fn commit_choice(
        &mut self,
        _: &TaskPublicChoiceV1,
    ) -> Result<TaskWorldCommitV1<usize>, String> {
        Err("fixture has only private exact scopes".to_string())
    }
}
impl TaskPrivilegedSupportWorldV1 for World {
    fn privileged_support(&mut self) -> Result<TaskPrivilegedSupportV1, String> {
        assert_eq!(
            self.stage, 1,
            "continuation policy must stop before next combat support"
        );
        Ok(TaskPrivilegedSupportV1::StrategicChoice {
            support: TaskPrivilegedStrategicSupportV1 {
                root_public_snapshot_id: self.decision().information.snapshot_id,
                provenance: provenance(),
                outcome: if matches!(self.behavior, Behavior::MissingPolicy) {
                    TaskPrivilegedStrategicSupportOutcomeV1::Unavailable {
                        reason: TaskPrivilegedStrategicSupportUnavailableV1::ExactOwnerUnavailable,
                    }
                } else {
                    TaskPrivilegedStrategicSupportOutcomeV1::Choice {
                        choice: TaskPublicChoiceV1::one(0),
                    }
                },
            },
        })
    }
}
fn decision(stage: usize) -> TaskPublicDecisionV1 {
    if stage.is_multiple_of(2) {
        let mut combat = crate::test_support::blank_test_combat();
        combat.turn.turn_count = stage as u32;
        combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
            crate::content::cards::CardId::Strike,
            1,
        )];
        combat.entities.monsters = vec![crate::test_support::test_monster(
            crate::content::monsters::EnemyId::JawWorm,
        )];
        let actions = crate::agent::information::action::project_public_combat_actions_v1(
            &crate::state::EngineState::CombatPlayerTurn,
            &combat,
        )
        .unwrap();
        return task_public_combat_decision_v1(
            crate::agent::information::run::PublicCombatRunContextV1::Unavailable {
                reason: crate::agent::information::run::PublicCombatRunContextGapV1::DetachedExactCombatPosition,
            }, crate::agent::information::state::public_combat_state_v1(&combat), &actions.public).unwrap();
    }
    TaskPublicDecisionV1::new(
        TaskDecisionSiteV1::Reward,
        TaskPublicObservationV1::Strategic {
            observation: TaskStrategicObservationV1 {
                mechanics: PlannerMechanicsManifest {
                    mechanics_id: "continuation-fixture".to_string(),
                    mechanics_version: 1,
                },
                run_goal: PlannerRunGoal::ActThreeVictory,
                decision_site: PlannerDecisionSite::Reward,
                run: PlannerRunScalars {
                    player_class: PlannerPlayerClass::Ironclad,
                    ascension_level: 0,
                    act: 1,
                    floor: stage as i32,
                    current_hp: 70,
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
                    current_y: stage as i32,
                    boss: None,
                    nodes: Vec::new(),
                },
                context: PlannerDecisionContext::Reward {
                    pending_rewards: Vec::new(),
                },
                public_history: PlannerPublicHistory {
                    shop_purge_count: 0,
                    recent_combat_hp_loss: None,
                    recent_combat_turns: None,
                    recent_combat_potions_used: None,
                },
            },
        },
        vec![TaskPublicCandidateV1::Strategic {
            action: TaskStrategicActionV1::Proceed {
                site: PlannerDecisionSite::Reward,
            },
        }],
    )
    .unwrap()
}
fn provenance() -> TaskPrivilegedSupportProvenanceV1 {
    TaskPrivilegedSupportProvenanceV1 {
        access: TaskPrivilegedSupportAccessV1::PrivilegedExactCombatWitness,
        producer_name: "fixture".to_string(),
        producer_revision: 1,
    }
}
fn attempt(root: &TaskPublicDecisionV1) -> TaskResearchPlanAttemptV1 {
    TaskResearchPlanAttemptV1 {
        lanes: vec![TaskResearchPlanLaneAttemptV1 {
            provenance: provenance(),
            work_unit: TaskResearchPlanWorkUnitV1::AtomicExpandedNodes,
            work_consumed: 2,
            exit: TaskResearchPlanAttemptExitV1::Witness,
        }],
        candidates: (0..2)
            .map(|i| TaskResearchPlanCandidateV1 {
                plan: TaskResearchPlanV1 {
                    root_public_snapshot_id: root.information.snapshot_id.clone(),
                    steps: vec![TaskResearchPlanStepV1 {
                        source_public_snapshot_id: root.information.snapshot_id.clone(),
                        choice: TaskPublicChoiceV1::one(i),
                    }],
                },
                outcome: TaskResearchPlanOutcomeV1 {
                    final_hp: 70,
                    final_gold: 99,
                    persistent: TaskResearchPlanPersistentOutcomeV1 {
                        max_hp: 80,
                        recoverable_gold_delta: 0,
                        unrecovered_stolen_gold: 0,
                        ritual_dagger_value: 0,
                        genetic_algorithm_value: 0,
                        external_burden_count: 0,
                    },
                    consumed_entry_potion_slots: Vec::new(),
                    total_potions_consumed: 0,
                    generated_potions: Vec::new(),
                    final_relics: Vec::new(),
                    meta_changes: Vec::new(),
                    pending_rewards: Vec::new(),
                    combat_mugged: false,
                },
                satisfies_requested_quality: false,
                supporting_provenances: vec![provenance()],
            })
            .collect(),
    }
}
fn fixture(
    behavior: Behavior,
) -> (
    NodeReloadTaskEnvironmentV1<World>,
    TaskPrivilegedCombatResearchSupportV1,
) {
    let mut task = super::super::ironclad_a0_exact_branching_task_spec_v1();
    task.information_access =
        TaskInformationAccessV1::PublicHistoryLegalReloadMemoryAndPrivilegedSupport;
    let env = NodeReloadTaskEnvironmentV1::new(
        World { stage: 0, behavior },
        task,
        TaskBudgetV1 {
            max_world_transitions: 100,
            max_reloads: 0,
        },
    )
    .unwrap();
    let root = current(&env).unwrap();
    let support = TaskPrivilegedCombatResearchSupportV1 {
        root_public_snapshot_id: root.information.snapshot_id.clone(),
        selection_provenance: provenance(),
        attempt: attempt(&root),
        non_dominated_candidate_ordinals: vec![0, 1],
        selection: TaskCombatResearchPlanSelectionOutcomeV1::Unavailable {
            reason: TaskCombatResearchPlanSelectionUnavailableV1::AmbiguousNonDominatedFrontier,
        },
    };
    (env, support)
}
fn options() -> CombatPlanContinuationOptionsV1 {
    CombatPlanContinuationOptionsV1 {
        max_task_commands: 100,
        max_combat_searches: 10,
        max_endpoints: 10,
        combat_search: Default::default(),
    }
}
fn evaluate(
    env: &NodeReloadTaskEnvironmentV1<World>,
    support: &TaskPrivilegedCombatResearchSupportV1,
    options: &CombatPlanContinuationOptionsV1,
) -> CombatPlanContinuationReportV1 {
    evaluate_with(
        env,
        support,
        options,
        |w| Ok(w.clone()),
        |e, _| Ok(attempt(&current(e).unwrap())),
    )
    .unwrap()
}

#[test]
fn retains_each_frontier_and_source_bound_continuation_without_changing_root() {
    let (env, support) = fixture(Behavior::Normal);
    let before = env.actor_view();
    let report = evaluate(&env, &support, &options());
    assert_eq!(report.branches.len(), 2);
    assert_eq!(
        report.work,
        CombatPlanContinuationWorkV1 {
            task_commands_issued: 14,
            combat_searches_started: 2,
            endpoints_replayed: 4
        }
    );
    assert!(report.interruption.is_none());
    assert!(report.unstarted_root_candidate_ordinals.is_empty());
    for branch in &report.branches {
        assert_eq!(
            branch.stop,
            CombatPlanContinuationStopV1::SampledFrontierReplayed
        );
        assert_eq!(branch.next_frontier, vec![0, 1]);
        assert_eq!(branch.between_combat_steps.len(), 1);
        assert_eq!(
            branch.between_combat_steps[0].source_public_snapshot_id,
            branch
                .after_root_combat
                .as_ref()
                .unwrap()
                .information
                .snapshot_id
        );
        assert_eq!(branch.endpoints.len(), 2);
        assert!(branch.endpoints.iter().all(|e| e.decision == decision(3)));
    }
    assert_eq!(env.actor_view(), before);
    assert_eq!(
        env.inspect_private_research_execution_v1(|w| Ok(w.stage))
            .unwrap(),
        0
    );
    let restored: CombatPlanContinuationReportV1 =
        serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
    assert_eq!(restored, report);
}

#[test]
fn batch_limits_do_not_reset_for_the_next_root_or_endpoint() {
    let (env, support) = fixture(Behavior::Normal);
    for limit in [
        CombatPlanContinuationLimitV1::TaskCommands,
        CombatPlanContinuationLimitV1::CombatSearches,
        CombatPlanContinuationLimitV1::Endpoints,
    ] {
        let mut options = options();
        match limit {
            CombatPlanContinuationLimitV1::TaskCommands => options.max_task_commands = 6,
            CombatPlanContinuationLimitV1::CombatSearches => options.max_combat_searches = 1,
            CombatPlanContinuationLimitV1::Endpoints => options.max_endpoints = 1,
        }
        let report = evaluate(&env, &support, &options);
        assert_eq!(report.interruption, Some(exhausted(limit)));
        assert_eq!(report.unstarted_root_candidate_ordinals, vec![1]);
        assert!(report.work.task_commands_issued <= options.max_task_commands);
        assert!(report.work.combat_searches_started <= options.max_combat_searches);
        assert!(report.work.endpoints_replayed <= options.max_endpoints);
        assert!(!report.branches[0].endpoints.is_empty());
    }
}

#[test]
fn no_command_allowance_starts_no_private_work() {
    let (env, support) = fixture(Behavior::Normal);
    let mut options = options();
    options.max_task_commands = 0;
    let report = evaluate_with(
        &env,
        &support,
        &options,
        |_| panic!("fork was not admitted"),
        |_, _| panic!("search was not admitted"),
    )
    .unwrap();
    assert_eq!(report.work, CombatPlanContinuationWorkV1::default());
    assert_eq!(report.unstarted_root_candidate_ordinals, vec![0, 1]);
}

#[test]
fn reaching_next_combat_with_no_commands_left_does_not_start_search() {
    let (env, support) = fixture(Behavior::Normal);
    let mut options = options();
    options.max_task_commands = 3;
    let report = evaluate_with(
        &env,
        &support,
        &options,
        |w| Ok(w.clone()),
        |_, _| panic!("replay budget is exhausted"),
    )
    .unwrap();
    assert_eq!(report.work.task_commands_issued, 3);
    assert_eq!(report.work.combat_searches_started, 0);
    assert!(report.branches[0].next_combat.is_some());
    assert_eq!(
        report.interruption,
        Some(exhausted(CombatPlanContinuationLimitV1::TaskCommands))
    );
}

#[test]
fn foreign_root_is_rejected_before_forking() {
    let (env, mut support) = fixture(Behavior::Normal);
    support.root_public_snapshot_id.push_str("-foreign");
    assert!(evaluate_with(
        &env,
        &support,
        &options(),
        |_| panic!("invalid input"),
        |_, _| panic!("invalid input")
    )
    .is_err());
}

#[test]
fn later_replay_error_keeps_prior_endpoints_and_the_attempt() {
    let (env, support) = fixture(Behavior::Normal);
    let report = evaluate_with(
        &env,
        &support,
        &options(),
        |w| Ok(w.clone()),
        |e, _| {
            let mut attempt = attempt(&current(e).unwrap());
            attempt.candidates[1]
                .plan
                .steps
                .push(TaskResearchPlanStepV1 {
                    source_public_snapshot_id: "unexpected-future-frame".to_string(),
                    choice: TaskPublicChoiceV1::one(0),
                });
            Ok(attempt)
        },
    )
    .unwrap();
    assert_eq!(report.branches[0].endpoints.len(), 1);
    assert_eq!(
        report.branches[0]
            .next_attempt
            .as_ref()
            .unwrap()
            .candidates
            .len(),
        2
    );
    assert_eq!(report.unstarted_root_candidate_ordinals, vec![1]);
    assert!(matches!(
        report.interruption,
        Some(CombatPlanContinuationStopV1::ProtocolError { .. })
    ));
}

#[test]
fn search_miss_is_retained_without_a_defeat_or_policy_ranking() {
    let (env, support) = fixture(Behavior::Normal);
    let report = evaluate_with(
        &env,
        &support,
        &options(),
        |w| Ok(w.clone()),
        |_, _| {
            Ok(TaskResearchPlanAttemptV1 {
                candidates: Vec::new(),
                lanes: vec![TaskResearchPlanLaneAttemptV1 {
                    provenance: provenance(),
                    work_unit: TaskResearchPlanWorkUnitV1::AtomicExpandedNodes,
                    work_consumed: 2,
                    exit: TaskResearchPlanAttemptExitV1::ExpandedNodeBudgetExhausted,
                }],
            })
        },
    )
    .unwrap();
    assert_eq!(report.branches.len(), 2);
    assert!(report.interruption.is_none());
    for branch in report.branches {
        assert_eq!(
            branch.stop,
            CombatPlanContinuationStopV1::NoWitness {
                search_exit: TaskResearchPlanAttemptExitV1::ExpandedNodeBudgetExhausted
            }
        );
        assert!(branch.next_attempt.is_some());
        assert!(branch.endpoints.is_empty());
    }
}

#[test]
fn unavailable_strategic_policy_preserves_the_boundary_without_launching_search() {
    let (env, support) = fixture(Behavior::MissingPolicy);
    let report = evaluate_with(
        &env,
        &support,
        &options(),
        |w| Ok(w.clone()),
        |_, _| panic!("no next combat reached"),
    )
    .unwrap();
    assert_eq!(report.work.combat_searches_started, 0);
    assert_eq!(report.branches.len(), 2);
    assert!(report.branches.iter().all(|b| matches!(
        b.stop,
        CombatPlanContinuationStopV1::PolicyUnavailable {
            site: TaskDecisionSiteV1::Reward,
            ..
        }
    )));
}

#[test]
fn malformed_source_and_partial_plan_stop_the_batch() {
    for behavior in [Behavior::Normal, Behavior::PartialPlan] {
        let (env, mut support) = fixture(behavior);
        if matches!(behavior, Behavior::Normal) {
            let second = TaskResearchPlanStepV1 {
                source_public_snapshot_id: "wrong-frame".to_string(),
                choice: TaskPublicChoiceV1::one(0),
            };
            support.attempt.candidates[0].plan.steps.push(second);
        }
        let report = evaluate(&env, &support, &options());
        assert!(matches!(
            report.interruption,
            Some(CombatPlanContinuationStopV1::ProtocolError { .. })
        ));
        assert_eq!(report.work.combat_searches_started, 0);
        assert_eq!(report.unstarted_root_candidate_ordinals, vec![1]);
        assert_eq!(
            env.inspect_private_research_execution_v1(|w| Ok(w.stage))
                .unwrap(),
            0
        );
    }
}

#[test]
fn transition_failure_is_distinct_from_a_search_miss() {
    let (env, support) = fixture(Behavior::BrokenTransition);
    let report = evaluate(&env, &support, &options());
    assert!(matches!(
        report.interruption,
        Some(CombatPlanContinuationStopV1::ExecutionStopped {
            outcome: TaskEpisodeOutcomeV1::Failure {
                reason: TaskEpisodeFailureV1::MechanicsOrProtocolError { .. },
                ..
            }
        })
    ));
    assert_eq!(report.unstarted_root_candidate_ordinals, vec![1]);
    assert_eq!(report.work.combat_searches_started, 0);
}
