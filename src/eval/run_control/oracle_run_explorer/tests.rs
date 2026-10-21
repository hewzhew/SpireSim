use super::decision_supply::apply_decision_policy;
use super::*;
use crate::content::potions::{Potion, PotionId};
use crate::eval::run_control::{
    build_decision_surface, expand_oracle_neow_candidates_v1, positive_ranked_run_policy_prior_v1,
    CardRewardFunctionV1, CardRewardObligationDeltaV1, CardRewardObligationSourceV1,
    CardRewardOwnerProvenanceV1, RunCombatResolutionKindV1, RunControlConfig, RunPolicyCandidateV1,
};
use crate::eval::run_control::{oracle_potion_rescue_tier_v1, OraclePotionRescueTierV1};
use crate::state::core::{ActiveCombat, ClientInput, CombatContext, RoomCombatContext};
use crate::state::map::node::RoomType;

fn test_branch(branch_id: usize, parent_branch_id: Option<usize>) -> OracleRunBranchV1 {
    OracleRunBranchV1 {
        branch_id,
        parent_branch_id,
        neow_root_candidate_id: "root".to_string(),
        neow_root_label: "root".to_string(),
        state_fingerprint: format!("state/{branch_id}"),
        boundary: OracleRunBoundaryV1::MapDecision,
        path_negative_log_policy: 0.0,
        path_discrepancy: 0,
        path_depth: 1,
        replay: Vec::new(),
        journal: RunProgressJournalV1::default(),
        session: RunControlSession::new(RunControlConfig::default()),
    }
}

fn test_decision(parent_branch_id: usize, candidate_id: &str) -> LazyOracleRunDecisionV1 {
    LazyOracleRunDecisionV1 {
        parent_branch_id,
        parent_state_fingerprint: format!("state/{parent_branch_id}"),
        neow_root_candidate_id: "root".to_string(),
        kind: OracleRunWorkKindV1::MapTravel,
        candidate_id: candidate_id.to_string(),
        label: candidate_id.to_string(),
        action: RunDecisionAction::Input(ClientInput::Proceed),
        stable_work_key: candidate_id.to_string(),
        path_negative_log_policy: 0.0,
        path_discrepancy: 0,
        path_depth: 2,
        parent_act: 0,
        parent_floor: 0,
        combat_edge_probe: None,
    }
}

fn test_explore_budget(wall_ms: Option<u64>) -> OracleRunExploreBudgetV1 {
    OracleRunExploreBudgetV1 {
        max_work_items: 1,
        wall_ms,
        combat: OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default()),
        combat_quantum_generation_work: 1,
        combat_quantum_ms: None,
        decision_prior: None,
        decision_annotation: None,
        combat_edge_order: None,
    }
}

fn take_next_scheduled_work_for_test(
    explorer: &mut OracleRunExplorerV1,
) -> PreparedScheduledOracleRunWorkV1 {
    let prepared = explorer
        .prepare_next_scheduled_work()
        .expect("scheduled work");
    match &prepared {
        PreparedScheduledOracleRunWorkV1::Decision { root, index, work } => {
            explorer.commit_scheduled_decision(root.clone(), *index, &work.stable_work_key);
        }
        PreparedScheduledOracleRunWorkV1::DeferredCombat {
            root,
            index,
            branch_id,
            stage,
        } => {
            explorer.commit_scheduled_deferred_combat(root.clone(), *index, *branch_id, *stage);
        }
    }
    prepared
}

#[test]
fn removed_inline_explorer_payload_fields_are_rejected() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut root = test_branch(0, None);
    root.state_fingerprint = run_session_fingerprint_v3(&root.session);
    explorer.accept_branch(root).expect("root branch");
    let checkpoint = explorer.analysis_checkpoint().expect("pooled checkpoint");
    let mut value = serde_json::to_value(checkpoint).expect("serialize checkpoint value");
    value["branches"][0]["replay"] = serde_json::json!([]);

    let error = serde_json::from_value::<OracleRunExplorerCheckpointV1>(value)
        .expect_err("removed inline replay field must fail closed");
    assert!(
        error.to_string().contains("unknown field `replay`"),
        "{error}"
    );
}

#[test]
fn pre_identity_cutover_fingerprint_algorithm_is_rejected() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut root = test_branch(0, None);
    root.state_fingerprint = run_session_fingerprint_v3(&root.session);
    explorer.accept_branch(root).expect("root branch");
    let mut checkpoint = explorer.analysis_checkpoint().expect("checkpoint");
    checkpoint.state_fingerprint_algorithm =
        Some("blake2b_256_canonical_run_checkpoint_v2".to_string());

    let error = match seed_oracle_run_explorer_from_checkpoint_v1(
        checkpoint,
        &OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default()),
    ) {
        Ok(_) => panic!("pre-cutover fingerprints must not be migrated implicitly"),
        Err(error) => error,
    };

    assert!(error.contains("stable gameplay identity cutover"));
}

#[test]
fn drive_reports_work_exhausted_without_consuming_service() {
    let result =
        drive_oracle_run_explorer_v1(OracleRunExplorerV1::empty(), test_explore_budget(None))
            .expect("empty explorer stops cleanly");

    assert_eq!(result.stop, OracleRunExploreStopV1::WorkExhausted);
    assert_eq!(result.work_items, 0);
    assert_eq!(result.combat_quanta, 0);
}

#[test]
fn drive_reports_wall_deadline_before_touching_live_work() {
    let mut explorer = OracleRunExplorerV1::empty();
    explorer
        .pending_decisions
        .push_back(test_decision(0, "still-live"));

    let result = drive_oracle_run_explorer_v1(explorer, test_explore_budget(Some(0)))
        .expect("expired explorer stops cleanly");

    assert_eq!(result.stop, OracleRunExploreStopV1::WallDeadlineReached);
    assert_eq!(result.work_items, 0);
    assert_eq!(result.combat_quanta, 0);
    assert_eq!(result.explorer.pending_decisions.len(), 1);
    assert_eq!(
        result.explorer.pending_decisions[0].candidate_id,
        "still-live"
    );
}

#[test]
fn scheduled_work_is_only_removed_when_its_preparation_commits() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut first = test_decision(0, "first");
    first.neow_root_candidate_id = "0".to_string();
    let mut second = test_decision(1, "second");
    second.neow_root_candidate_id = "1".to_string();
    explorer.pending_decisions = VecDeque::from([first, second]);

    let prepared = explorer
        .prepare_next_scheduled_work()
        .expect("prepared scheduled work");

    assert_eq!(explorer.pending_decisions.len(), 2);
    assert_eq!(explorer.last_served_neow_root, None);
    let PreparedScheduledOracleRunWorkV1::Decision { root, index, work } = prepared else {
        panic!("first scheduled work should be a decision");
    };
    explorer.commit_scheduled_decision(root, index, &work.stable_work_key);
    assert_eq!(explorer.pending_decisions.len(), 1);
    assert_eq!(explorer.pending_decisions[0].candidate_id, "second");
    assert_eq!(explorer.last_served_neow_root.as_deref(), Some("0"));
}

fn reject_branch_schedule_prior(
    _session: &RunControlSession,
    _legal: &[RunPolicyCandidateV1<'_>],
) -> Result<crate::eval::run_control::RunPolicyPriorV1, String> {
    Err("injected branch-schedule prior failure".to_string())
}

#[test]
fn branch_schedule_prior_failure_is_prepare_only() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut branch = test_branch(0, None);
    branch.state_fingerprint = run_session_fingerprint_v3(&branch.session);
    explorer.next_branch_id = 1;
    explorer.accept_branch(branch).expect("unique branch");
    let before_registered = explorer.registered_work_keys.clone();
    let before_decisions = explorer.pending_decisions.len();

    let error = explorer
        .prepare_branch_schedule(
            &explorer.branches[0],
            &OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default()),
            Some(reject_branch_schedule_prior),
        )
        .err()
        .expect("injected child scheduling must fail");

    assert!(
        error.contains("injected branch-schedule prior failure"),
        "unexpected scheduling error: {error}"
    );
    assert_eq!(explorer.next_branch_id, 1);
    assert_eq!(explorer.branches.len(), 1);
    assert_eq!(explorer.registered_work_keys, before_registered);
    assert_eq!(explorer.pending_decisions.len(), before_decisions);
    assert!(explorer.pending_combats.is_empty());
}

fn delayed_reward_prior(
    _session: &RunControlSession,
    legal: &[RunPolicyCandidateV1<'_>],
) -> Result<crate::eval::run_control::RunPolicyPriorV1, String> {
    std::thread::sleep(Duration::from_millis(15));
    positive_ranked_run_policy_prior_v1(legal, std::iter::empty::<String>())
}

#[test]
fn combat_service_time_includes_exact_commit_and_child_scheduling() {
    let mut session = RunControlSession::new(RunControlConfig::default());
    let mut combat = crate::test_support::blank_test_combat();
    let mut monster = crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
    let plan = crate::content::monsters::roll_monster_turn_plan(
        &mut combat.rng.ai_rng,
        &monster,
        combat.meta.ascension_level,
        99,
        std::slice::from_ref(&monster),
        &[],
    );
    monster.set_planned_move_id(plan.move_id);
    monster.set_planned_steps(plan.steps);
    monster.set_planned_visible_spec(plan.visible_spec);
    monster.current_hp = 6;
    monster.max_hp = 6;
    let target = monster.id;
    combat.entities.monsters = vec![monster];
    combat.zones.hand = vec![crate::runtime::combat::CombatCard::for_scenario(
        crate::content::cards::CardId::Strike,
        1,
    )];
    session.engine_state = EngineState::CombatPlayerTurn;
    session.active_combat = Some(ActiveCombat::new(
        EngineState::CombatPlayerTurn,
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));

    let mut explorer = OracleRunExplorerV1::empty();
    explorer.next_branch_id = 1;
    let branch_id = explorer
        .accept_branch(OracleRunBranchV1 {
            branch_id: 0,
            parent_branch_id: None,
            neow_root_candidate_id: "test_root".to_string(),
            neow_root_label: "test root".to_string(),
            state_fingerprint: run_session_fingerprint_v3(&session),
            boundary: OracleRunBoundaryV1::Combat,
            path_negative_log_policy: 0.0,
            path_discrepancy: 0,
            path_depth: 1,
            replay: Vec::new(),
            journal: RunProgressJournalV1::default(),
            session,
        })
        .expect("unique combat branch");
    let combat_budgets =
        OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default());
    explorer
        .schedule_branch(branch_id, &combat_budgets, None)
        .expect("combat schedule");
    explorer.pending_combats[0]
        .work
        .verify_and_restore_action_witness(&[ClientInput::PlayCard {
            card_index: 0,
            target: Some(target),
        }])
        .expect("exact one-Strike witness");

    let result = drive_oracle_run_explorer_v1(
        explorer,
        OracleRunExploreBudgetV1 {
            max_work_items: 2,
            wall_ms: None,
            combat: combat_budgets,
            combat_quantum_generation_work: 64,
            combat_quantum_ms: None,
            decision_prior: Some(delayed_reward_prior),
            decision_annotation: None,
            combat_edge_order: None,
        },
    )
    .expect("drive exact combat completion");

    assert!(
        result.combat_service_ms >= 10,
        "combat service must include the delayed child schedule, got {} ms with stop {:?}, {} branches, {} pending combats, and {} pending decisions",
        result.combat_service_ms,
        result.stop,
        result.explorer.branches.len(),
        result.explorer.pending_combat_count(),
        result.explorer.pending_decisions.len()
    );
}

#[test]
fn exact_duplicate_admission_is_first_wins_and_records_the_discarded_path() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut first = test_branch(7, Some(1));
    first.state_fingerprint = "shared-exact-state".to_string();
    first.path_negative_log_policy = 9.0;
    first.path_discrepancy = 12;
    assert_eq!(explorer.accept_branch(first), Some(7));

    let mut later_better_path = test_branch(9, Some(3));
    later_better_path.state_fingerprint = "shared-exact-state".to_string();
    later_better_path.neow_root_candidate_id = "alternate-root".to_string();
    later_better_path.path_negative_log_policy = 0.0;
    later_better_path.path_discrepancy = 0;
    later_better_path.replay.push(OracleRunReplayStepV1 {
        candidate_id: "discarded-choice".to_string(),
        label: "discarded choice".to_string(),
        action: RunDecisionAction::Input(ClientInput::Proceed),
    });
    assert_eq!(explorer.accept_branch(later_better_path), None);

    assert_eq!(explorer.branches.len(), 1);
    assert_eq!(explorer.branches[0].branch_id, 7);
    assert_eq!(explorer.branches[0].path_discrepancy, 12);
    assert_eq!(
        explorer.state_index.get("shared-exact-state"),
        Some(&7),
        "the first exact-state owner remains the survivor"
    );

    assert_eq!(explorer.retired_exact_duplicates.len(), 1);
    let duplicate = &explorer.retired_exact_duplicates[0];
    assert_eq!(duplicate.branch_id, 9);
    assert_eq!(duplicate.parent_branch_id, Some(3));
    assert_eq!(duplicate.survivor_branch_id, 7);
    assert_eq!(duplicate.neow_root_candidate_id, "alternate-root");
    assert_eq!(duplicate.state_fingerprint, "shared-exact-state");
    assert_eq!(duplicate.replay.len(), 1);
    assert_eq!(duplicate.replay[0].candidate_id, "discarded-choice");
}

#[test]
fn neow_root_policy_mass_is_normalized_after_exact_duplicate_retirement() {
    let session = RunControlSession::new(RunControlConfig {
        seed: 0,
        ascension_level: 20,
        final_act: false,
        ..RunControlConfig::default()
    });
    let expansion = expand_oracle_neow_candidates_v1(&session).expect("A20 Neow expansion");
    let expanded_count = expansion.completed.len();
    let explorer = seed_oracle_run_explorer_v1(expansion, None).expect("seed explorer");

    assert!(!explorer.retired_exact_duplicates.is_empty());
    assert_eq!(
        explorer.branches.len() + explorer.retired_exact_duplicates.len(),
        expanded_count
    );
    let expected = (explorer.branches.len() as f64).ln();
    assert!(explorer
        .branches
        .iter()
        .all(|branch| (branch.path_negative_log_policy - expected).abs() < f64::EPSILON));
    let total_mass = explorer
        .branches
        .iter()
        .map(|branch| (-branch.path_negative_log_policy).exp())
        .sum::<f64>();
    assert!((total_mass - 1.0).abs() < 1e-12, "mass={total_mass}");
}

#[test]
fn parameterized_run_selection_releases_one_exact_member_at_a_time() {
    let mut branch = test_branch(0, None);
    branch.boundary = OracleRunBoundaryV1::RunChoice;
    branch.session.run_state.master_deck = (0..3)
        .map(|uuid| {
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::Strike,
                uuid,
            )
        })
        .collect();
    branch.session.engine_state =
        EngineState::RunPendingChoice(crate::state::core::RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 2,
            max_choices: 2,
            reason: crate::state::core::RunPendingChoiceReason::PurgeNonBottled,
            source: crate::state::selection::DomainEventSource::Selection(
                crate::state::core::RunPendingChoiceReason::PurgeNonBottled.into(),
            ),
            return_state: Box::new(EngineState::MapNavigation),
        });

    let mut explorer = OracleRunExplorerV1::empty();
    explorer.accept_branch(branch);
    explorer
        .schedule_branch(
            0,
            &OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default()),
            None,
        )
        .unwrap();

    assert_eq!(explorer.pending_decisions.len(), 1);
    assert_eq!(explorer.pending_selection_families.len(), 1);
    assert_eq!(
        explorer.pending_selection_families[0].cursor.total_count(),
        3
    );
    let mut emitted = Vec::new();
    for expected_rank in 0..3 {
        let work = explorer.take_best_decision().unwrap();
        assert_eq!(work.candidate_id, "select");
        assert_eq!(work.path_discrepancy, expected_rank);
        assert!(((-work.path_negative_log_policy).exp() - 1.0 / 3.0).abs() < 1.0e-9);
        emitted.push(work.stable_work_key.clone());
        let release = explorer
            .prepare_selection_member_release(&work.stable_work_key)
            .unwrap();
        explorer.apply_selection_member_release(release);
        assert!(
            explorer.pending_decisions.len() <= 1,
            "the frontier must never contain the whole combination family"
        );
    }
    assert_eq!(emitted.into_iter().collect::<BTreeSet<_>>().len(), 3);
    assert!(explorer.pending_decisions.is_empty());
    assert!(explorer.pending_selection_families.is_empty());
}

#[test]
fn analysis_selection_widens_without_mutating_parent_choices() {
    let mut branch = test_branch(0, None);
    branch.boundary = OracleRunBoundaryV1::RunChoice;
    branch.session.run_state.master_deck = (0..3)
        .map(|uuid| {
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::Strike,
                uuid,
            )
        })
        .collect();
    branch.session.engine_state =
        EngineState::RunPendingChoice(crate::state::core::RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 2,
            max_choices: 2,
            reason: crate::state::core::RunPendingChoiceReason::PurgeNonBottled,
            source: crate::state::selection::DomainEventSource::Selection(
                crate::state::core::RunPendingChoiceReason::PurgeNonBottled.into(),
            ),
            return_state: Box::new(EngineState::MapNavigation),
        });

    let mut explorer = OracleRunExplorerV1::empty();
    explorer.accept_branch(branch);
    explorer
        .schedule_branch(
            0,
            &OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default()),
            None,
        )
        .unwrap();
    let first = explorer.pending_decisions[0].stable_work_key.clone();

    let release = explorer.prepare_selection_member_release(&first).unwrap();
    explorer.apply_selection_member_release(release);
    assert_eq!(
        explorer.pending_decisions.len(),
        2,
        "trying a variation must preserve the exact parent choice while exposing one sibling"
    );
    assert!(explorer
        .pending_decisions
        .iter()
        .any(|decision| decision.stable_work_key == first));

    let release = explorer.prepare_selection_member_release(&first).unwrap();
    explorer.apply_selection_member_release(release);
    assert_eq!(
        explorer.pending_decisions.len(),
        2,
        "retrying an old member must not widen the family twice"
    );
}

#[test]
fn parameterized_run_selection_cursor_survives_frontier_checkpoint() {
    let mut branch = test_branch(0, None);
    branch.boundary = OracleRunBoundaryV1::RunChoice;
    branch.session.run_state.master_deck = (0..20)
        .map(|uuid| {
            crate::runtime::combat::CombatCard::for_scenario(
                crate::content::cards::CardId::Strike,
                uuid,
            )
        })
        .collect();
    branch.session.engine_state =
        EngineState::RunPendingChoice(crate::state::core::RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 3,
            max_choices: 3,
            reason: crate::state::core::RunPendingChoiceReason::TransformUpgraded,
            source: crate::state::selection::DomainEventSource::Relic(
                crate::content::relics::RelicId::Astrolabe,
            ),
            return_state: Box::new(EngineState::MapNavigation),
        });
    branch.state_fingerprint = run_session_fingerprint_v3(&branch.session);

    let mut explorer = OracleRunExplorerV1::empty();
    explorer.accept_branch(branch);
    explorer
        .schedule_branch(
            0,
            &OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default()),
            None,
        )
        .unwrap();
    let checkpoint = explorer.frontier_checkpoint().unwrap().unwrap();

    assert_eq!(checkpoint.pending_decisions.len(), 1);
    assert_eq!(checkpoint.pending_selection_families.len(), 1);
    let encoded = serde_json::to_vec(&checkpoint).unwrap();
    assert!(
        encoded.len() < 50_000,
        "checkpoint must not contain all 1,140 Astrolabe combinations"
    );
    let decoded = serde_json::from_slice(&encoded).unwrap();
    let mut restored = seed_oracle_run_explorer_from_checkpoint_v1(
        decoded,
        &OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default()),
    )
    .unwrap();
    assert_eq!(restored.pending_decisions.len(), 1);
    assert_eq!(restored.pending_selection_families.len(), 1);

    let first = restored.take_best_decision().unwrap();
    let release = restored
        .prepare_selection_member_release(&first.stable_work_key)
        .unwrap();
    restored.apply_selection_member_release(release);
    assert_eq!(restored.pending_decisions.len(), 1);
    assert_eq!(
        restored.pending_selection_families[0]
            .cursor
            .emitted_count(),
        2
    );
}

#[test]
fn explicit_smoke_bomb_escape_materializes_a_typed_run_successor() {
    let mut session = RunControlSession::new(RunControlConfig::default());
    let mut combat = crate::test_support::blank_test_combat();
    combat.meta.is_boss_fight = false;
    combat.entities.player.current_hp = 37;
    combat.entities.player.max_hp = 80;
    let mut jaw_worm =
        crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
    let plan = crate::content::monsters::roll_monster_turn_plan(
        &mut combat.rng.ai_rng,
        &jaw_worm,
        combat.meta.ascension_level,
        99,
        std::slice::from_ref(&jaw_worm),
        &[],
    );
    jaw_worm.set_planned_move_id(plan.move_id);
    jaw_worm.set_planned_steps(plan.steps);
    jaw_worm.set_planned_visible_spec(plan.visible_spec);
    combat.entities.monsters = vec![jaw_worm];
    combat.entities.potions = vec![Some(Potion::new(PotionId::SmokeBomb, 41))];
    session.engine_state = EngineState::CombatPlayerTurn;
    session.active_combat = Some(ActiveCombat::new(
        EngineState::CombatPlayerTurn,
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));

    let mut explorer = OracleRunExplorerV1::empty();
    explorer.next_branch_id = 1;
    explorer
        .accept_branch(OracleRunBranchV1 {
            branch_id: 0,
            parent_branch_id: None,
            neow_root_candidate_id: "test_root".to_string(),
            neow_root_label: "test root".to_string(),
            state_fingerprint: run_session_fingerprint_v3(&session),
            boundary: OracleRunBoundaryV1::Combat,
            path_negative_log_policy: 0.0,
            path_discrepancy: 0,
            path_depth: 1,
            replay: Vec::new(),
            journal: RunProgressJournalV1::default(),
            session,
        })
        .expect("unique combat branch");

    let child_id = explorer
        .materialize_explicit_smoke_bomb_escape(0)
        .expect("exact escape")
        .expect("escape child");
    let child = explorer
        .branches
        .iter()
        .find(|branch| branch.branch_id == child_id)
        .expect("materialized child");

    assert_eq!(child.boundary, OracleRunBoundaryV1::Reward);
    assert_eq!(child.session.run_state.current_hp, 37);
    let [RunProgressStepV1::CombatResolution(resolution)] = child.journal.entries() else {
        panic!("escape should append exactly one typed combat resolution");
    };
    assert_eq!(resolution.kind, RunCombatResolutionKindV1::SmokeBombEscape);
}

fn shadow_key(enemy_hp_delta: i32, survival_margin: i32) -> StrategicProbeShadowOrderKeyV1 {
    StrategicProbeShadowOrderKeyV1 {
        terminal_win_seen: false,
        non_loss_endpoint_seen: true,
        living_enemy_delta: 0,
        total_enemy_hp_delta: enemy_hp_delta,
        survival_margin,
        pollution_avoidance: 0,
        depth_turns: 1,
    }
}

fn empty_combat_work_checkpoint() -> OracleCombatWitnessCheckpointV1 {
    OracleCombatWitnessCheckpointV1 {
        consumed_generation_work: 10,
        remaining_generation_work: 0,
        remaining_engine_steps: 0,
        remaining_wall_ms: Some(0),
        quantum_count: 1,
        restart_count: 0,
        incumbent_revision: 0,
        quanta_since_incumbent_improvement: 1,
        max_potions_used: None,
        allowed_potion_slots: None,
        potion_spend_requires_satisfaction: false,
        incumbent: None,
    }
}

#[test]
fn staged_combat_budget_uses_a_cheap_first_pass_and_full_retry() {
    let options = OracleCombatWitnessOptionsV1 {
        max_generation_work: Some(101),
        wall_ms: Some(101),
        ..OracleCombatWitnessOptionsV1::default()
    };
    let budgets = OracleRunCombatWitnessBudgetsV1 {
        hallway: options.clone(),
        elite: options.clone(),
        boss: options,
        quality_policy: OracleRunCombatWitnessQualityPolicyV1::Configured,
        initial_divisor: 4,
        guidance_bundle: None,
    };
    let session = RunControlSession::new(RunControlConfig::default());

    let first = budgets.for_session_stage(&session, 0);
    assert_eq!(first.max_generation_work, Some(26));
    assert_eq!(first.wall_ms, Some(26));
    let retry = budgets.for_session_stage(&session, 1);
    assert_eq!(retry.max_generation_work, Some(101));
    assert_eq!(retry.wall_ms, Some(101));
}

#[test]
fn strategic_quality_policy_derives_a_nonboss_target_from_exact_run_state() {
    let options = OracleCombatWitnessOptionsV1 {
        max_generation_work: Some(101),
        wall_ms: Some(101),
        satisfaction: Some(CombatWitnessSatisfactionV1::ZeroLossOrBudget),
        ..OracleCombatWitnessOptionsV1::default()
    };
    let budgets = OracleRunCombatWitnessBudgetsV1 {
        hallway: options.clone(),
        elite: options.clone(),
        boss: options,
        quality_policy: OracleRunCombatWitnessQualityPolicyV1::StrategicRun,
        initial_divisor: 1,
        guidance_bundle: None,
    };
    let mut session = RunControlSession::new(RunControlConfig::default());
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.player.current_hp = 72;
    combat.entities.player.max_hp = 80;
    session.active_combat = Some(crate::state::core::ActiveCombat::new(
        crate::state::core::EngineState::CombatPlayerTurn,
        combat,
        crate::state::core::CombatContext::Room(crate::state::core::RoomCombatContext {
            room_type: crate::state::map::node::RoomType::MonsterRoom,
        }),
    ));

    let resolved = budgets.for_session(&session);

    assert_eq!(
        resolved.satisfaction,
        Some(CombatWitnessSatisfactionV1::HpLossAtMost(16))
    );

    let combat = &mut session.active_combat.as_mut().unwrap().combat_state;
    combat.meta.master_deck_snapshot = vec![crate::runtime::combat::CombatCard::for_scenario(
        crate::content::cards::CardId::Reaper,
        92,
    )]
    .into();
    assert_eq!(
        budgets.for_session(&session).satisfaction,
        Some(CombatWitnessSatisfactionV1::HpLossAtMost(0)),
        "an already-damaged run with combat healing should search for a net-zero line"
    );
}

#[test]
fn strategic_nonboss_search_conserves_potions_before_exact_rescue() {
    let options = OracleCombatWitnessOptionsV1 {
        max_generation_work: Some(101),
        wall_ms: Some(101),
        ..OracleCombatWitnessOptionsV1::default()
    };
    let budgets = OracleRunCombatWitnessBudgetsV1 {
        hallway: options.clone(),
        elite: options.clone(),
        boss: options,
        quality_policy: OracleRunCombatWitnessQualityPolicyV1::StrategicRun,
        initial_divisor: 1,
        guidance_bundle: None,
    };
    let mut session = RunControlSession::new(RunControlConfig::default());
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::BlockPotion,
        7,
    ))];
    session.active_combat = Some(crate::state::core::ActiveCombat::new(
        crate::state::core::EngineState::CombatPlayerTurn,
        combat,
        crate::state::core::CombatContext::Room(crate::state::core::RoomCombatContext {
            room_type: crate::state::map::node::RoomType::MonsterRoom,
        }),
    ));

    let primary = budgets.for_session_stage(&session, 0);
    assert_eq!(primary.max_potions_used, Some(0));
    assert_eq!(
        primary.potion_policy,
        Some(CombatWitnessPotionPolicyV1::Never)
    );
    assert!(budgets.has_later_stage(&session, 0));

    let rescue = budgets.for_session_stage(&session, 1);
    assert_ne!(rescue.max_potions_used, Some(0));
    assert_ne!(
        rescue.potion_policy,
        Some(CombatWitnessPotionPolicyV1::Never)
    );
    assert!(!budgets.has_later_stage(&session, 1));
}

#[test]
fn strategic_rescue_splits_allowance_across_concrete_potion_identities() {
    let options = OracleCombatWitnessOptionsV1 {
        max_generation_work: Some(101),
        wall_ms: Some(101),
        ..OracleCombatWitnessOptionsV1::default()
    };
    let budgets = OracleRunCombatWitnessBudgetsV1 {
        hallway: options.clone(),
        elite: options.clone(),
        boss: options,
        quality_policy: OracleRunCombatWitnessQualityPolicyV1::StrategicRun,
        initial_divisor: 1,
        guidance_bundle: None,
    };
    let mut session = RunControlSession::new(RunControlConfig::default());
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.potions = vec![
        Some(crate::content::potions::Potion::new(
            PotionId::SkillPotion,
            7,
        )),
        Some(crate::content::potions::Potion::new(
            PotionId::ExplosivePotion,
            8,
        )),
        Some(crate::content::potions::Potion::new(
            PotionId::FairyPotion,
            9,
        )),
    ];
    session.active_combat = Some(crate::state::core::ActiveCombat::new(
        crate::state::core::EngineState::CombatPlayerTurn,
        combat,
        crate::state::core::CombatContext::Room(crate::state::core::RoomCombatContext {
            room_type: crate::state::map::node::RoomType::MonsterRoom,
        }),
    ));
    let prior = empty_combat_work_checkpoint();

    let skill = budgets.for_session_stage_with_prior(&session, 1, &prior);
    let explosive = budgets.for_session_stage_with_prior(&session, 2, &prior);
    let no_potion = budgets.for_session_stage(&session, 0);

    assert_eq!(no_potion.max_generation_work, Some(51));
    assert_eq!(no_potion.wall_ms, Some(34));
    assert_eq!(skill.allowed_potion_slots, Some(0b001));
    assert_eq!(explosive.allowed_potion_slots, Some(0b010));
    assert_eq!(skill.max_generation_work, Some(51));
    assert_eq!(explosive.max_generation_work, Some(51));
    assert_eq!(skill.wall_ms, Some(34));
    assert_eq!(explosive.wall_ms, Some(34));
    assert!(budgets.has_later_stage(&session, 0));
    assert!(budgets.has_later_stage(&session, 1));
    assert!(!budgets.has_later_stage(&session, 2));
}

#[test]
fn strategic_boss_search_isolates_identities_before_multi_potion_fallback() {
    let options = OracleCombatWitnessOptionsV1 {
        max_generation_work: Some(101),
        wall_ms: Some(101),
        ..OracleCombatWitnessOptionsV1::default()
    };
    let budgets = OracleRunCombatWitnessBudgetsV1 {
        hallway: options.clone(),
        elite: options.clone(),
        boss: options,
        quality_policy: OracleRunCombatWitnessQualityPolicyV1::StrategicRun,
        initial_divisor: 1,
        guidance_bundle: None,
    };
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.run_state.act_num = 3;
    let mut combat = crate::test_support::blank_test_combat();
    combat.meta.is_boss_fight = true;
    combat.entities.potions = vec![
        Some(Potion::new(PotionId::Elixir, 7)),
        Some(Potion::new(PotionId::SwiftPotion, 8)),
        Some(Potion::new(PotionId::SpeedPotion, 9)),
    ];
    session.active_combat = Some(ActiveCombat::new(
        crate::state::core::EngineState::CombatPlayerTurn,
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoomBoss,
        }),
    ));
    let prior = empty_combat_work_checkpoint();

    let no_potion = budgets.for_session_stage(&session, 0);
    let elixir = budgets.for_session_stage_with_prior(&session, 1, &prior);
    let swift = budgets.for_session_stage_with_prior(&session, 2, &prior);
    let speed = budgets.for_session_stage_with_prior(&session, 3, &prior);
    let fallback = budgets.for_session_stage_with_prior(&session, 4, &prior);

    assert_eq!(no_potion.max_potions_used, Some(0));
    assert_eq!(no_potion.max_generation_work, Some(26));
    assert_eq!(no_potion.wall_ms, Some(21));
    assert_eq!(elixir.allowed_potion_slots, Some(0b001));
    assert_eq!(swift.allowed_potion_slots, Some(0b010));
    assert_eq!(speed.allowed_potion_slots, Some(0b100));
    for identity in [&elixir, &swift, &speed] {
        assert_eq!(identity.max_potions_used, Some(1));
        assert_eq!(identity.max_generation_work, Some(26));
        assert_eq!(identity.wall_ms, Some(21));
    }
    assert_eq!(
        fallback.potion_policy,
        Some(CombatWitnessPotionPolicyV1::SemanticBudgeted)
    );
    assert_eq!(fallback.max_potions_used, Some(2));
    assert_eq!(fallback.allowed_potion_slots, Some(0b111));
    assert_eq!(fallback.max_generation_work, Some(26));
    assert_eq!(fallback.wall_ms, Some(21));
    for stage in 0..4 {
        assert!(budgets.has_later_stage(&session, stage));
    }
    assert!(!budgets.has_later_stage(&session, 4));
}

#[test]
fn potion_rescue_tiers_distinguish_common_tactics_from_reserved_resources() {
    for potion in [
        PotionId::FirePotion,
        PotionId::ExplosivePotion,
        PotionId::PoisonPotion,
        PotionId::WeakenPotion,
        PotionId::FearPotion,
        PotionId::BlockPotion,
        PotionId::EnergyPotion,
        PotionId::StrengthPotion,
        PotionId::DexterityPotion,
        PotionId::SpeedPotion,
        PotionId::SteroidPotion,
        PotionId::SwiftPotion,
        PotionId::FocusPotion,
        PotionId::BottledMiracle,
        PotionId::BlessingOfTheForge,
    ] {
        assert_eq!(
            oracle_potion_rescue_tier_v1(potion),
            OraclePotionRescueTierV1::BoundedQuality,
            "{potion:?} should retain its combat-local audit classification"
        );
    }

    for potion in [
        PotionId::BloodPotion,
        PotionId::RegenPotion,
        PotionId::AttackPotion,
        PotionId::SkillPotion,
        PotionId::PowerPotion,
        PotionId::DuplicationPotion,
        PotionId::LiquidMemories,
        PotionId::GhostInAJar,
    ] {
        assert_eq!(
            oracle_potion_rescue_tier_v1(potion),
            OraclePotionRescueTierV1::FindAnyWin,
            "{potion:?} needs a quality-gated autonomous rescue or a no-win emergency"
        );
    }

    for potion in [PotionId::FairyPotion, PotionId::SmokeBomb] {
        assert_eq!(
            oracle_potion_rescue_tier_v1(potion),
            OraclePotionRescueTierV1::Excluded
        );
    }
}

#[test]
fn autonomous_quality_gated_rescue_can_inspect_continuation_sensitive_potions() {
    let mut session = RunControlSession::new(RunControlConfig::default());
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.potions = vec![
        Some(crate::content::potions::Potion::new(
            PotionId::BlockPotion,
            7,
        )),
        Some(crate::content::potions::Potion::new(
            PotionId::ColorlessPotion,
            8,
        )),
        Some(crate::content::potions::Potion::new(
            PotionId::FairyPotion,
            9,
        )),
    ];
    session.active_combat = Some(crate::state::core::ActiveCombat::new(
        crate::state::core::EngineState::CombatPlayerTurn,
        combat,
        crate::state::core::CombatContext::Room(crate::state::core::RoomCombatContext {
            room_type: crate::state::map::node::RoomType::MonsterRoom,
        }),
    ));

    assert_eq!(
        oracle_active_victory_potion_slot_mask_v1(&session),
        0b011,
        "quality-gated rescue inspects active identities but excludes passive death insurance"
    );
}

#[test]
fn strategic_staging_ignores_atomic_session_overrides_and_skips_escape_only_inventory() {
    let options = OracleCombatWitnessOptionsV1::default();
    let budgets = OracleRunCombatWitnessBudgetsV1 {
        hallway: options.clone(),
        elite: options.clone(),
        boss: options,
        quality_policy: OracleRunCombatWitnessQualityPolicyV1::StrategicRun,
        initial_divisor: 1,
        guidance_bundle: None,
    };
    let mut session = RunControlSession::new(RunControlConfig::default());
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::SmokeBomb,
        8,
    ))];
    session.active_combat = Some(crate::state::core::ActiveCombat::new(
        crate::state::core::EngineState::CombatPlayerTurn,
        combat,
        crate::state::core::CombatContext::Room(crate::state::core::RoomCombatContext {
            room_type: crate::state::map::node::RoomType::MonsterRoom,
        }),
    ));

    assert!(!budgets.has_later_stage(&session, 0));
    assert_eq!(
        budgets.for_session_stage(&session, 0).max_potions_used,
        Some(0)
    );
    session
        .active_combat
        .as_mut()
        .unwrap()
        .combat_state
        .entities
        .potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::FairyPotion,
        10,
    ))];
    assert!(!budgets.has_later_stage(&session, 0));
    assert_eq!(
        budgets.for_session_stage(&session, 0).max_potions_used,
        Some(0),
        "the conserving lane must count passive Fairy consumption even though Fairy is not an active rescue slot"
    );

    session.search_potion_policy = Some(CombatWitnessPotionPolicyV1::All);
    session
        .active_combat
        .as_mut()
        .unwrap()
        .combat_state
        .entities
        .potions = vec![Some(crate::content::potions::Potion::new(
        crate::content::potions::PotionId::BlockPotion,
        9,
    ))];
    assert!(budgets.has_later_stage(&session, 0));
    assert_eq!(
        budgets.for_session_stage(&session, 0).max_potions_used,
        Some(0),
        "resident witness staging must not inherit the atomic-v2 session override"
    );
}

#[test]
fn strategic_quality_stops_when_an_a0_intermediate_boss_materializes_payoff() {
    let options = OracleCombatWitnessOptionsV1 {
        satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
        ..OracleCombatWitnessOptionsV1::default()
    };
    let budgets = OracleRunCombatWitnessBudgetsV1 {
        hallway: options.clone(),
        elite: options.clone(),
        boss: options,
        quality_policy: OracleRunCombatWitnessQualityPolicyV1::StrategicRun,
        initial_divisor: 1,
        guidance_bundle: None,
    };
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.run_state.act_num = 1;
    let mut combat = crate::test_support::blank_test_combat();
    combat.meta.is_boss_fight = true;
    combat.meta.master_deck_snapshot = vec![crate::runtime::combat::CombatCard::for_scenario(
        crate::content::cards::CardId::HandOfGreed,
        91,
    )]
    .into();
    session.active_combat = Some(ActiveCombat::new(
        crate::state::core::EngineState::CombatPlayerTurn,
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoomBoss,
        }),
    ));

    assert_eq!(
            budgets.for_session(&session).satisfaction,
            Some(
                CombatWitnessSatisfactionV1::MaterializedPersistentPayoffGain
            ),
            "an A0 act heal removes combat HP pressure, but persistent payoff remains a finite exact target"
        );

    session.run_state.act_num = 3;
    assert_eq!(
        budgets.for_session(&session).satisfaction,
        Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
        "persistent payoff must not delay the requested terminal boss witness"
    );
}

#[test]
fn deferred_retry_stays_on_the_existing_deep_first_discrepancy_contour() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut branch = test_branch(0, None);
    branch.boundary = OracleRunBoundaryV1::Combat;
    branch.path_depth = 10;
    explorer.branches.push(branch);
    explorer.deferred_combats.push_back(DeferredOracleCombatV1 {
        branch_id: 0,
        stage: 1,
        prior_work: empty_combat_work_checkpoint(),
    });
    let mut shallow_same_contour = test_decision(0, "shallow-same-contour-decision");
    shallow_same_contour.path_discrepancy = 0;
    shallow_same_contour.path_depth = 2;
    shallow_same_contour.parent_act = explorer.branches[0].session.run_state.act_num;
    shallow_same_contour.parent_floor = explorer.branches[0].session.run_state.floor_num;
    explorer.pending_decisions.push_back(shallow_same_contour);

    assert!(matches!(
        take_next_scheduled_work_for_test(&mut explorer),
        PreparedScheduledOracleRunWorkV1::DeferredCombat { .. }
    ));

    explorer.deferred_combats.push_back(DeferredOracleCombatV1 {
        branch_id: 0,
        stage: 1,
        prior_work: empty_combat_work_checkpoint(),
    });
    let mut deeper_same_contour = test_decision(0, "deeper-same-contour-decision");
    deeper_same_contour.path_discrepancy = 0;
    deeper_same_contour.path_depth = 20;
    deeper_same_contour.parent_act = explorer.branches[0].session.run_state.act_num;
    deeper_same_contour.parent_floor = explorer.branches[0].session.run_state.floor_num;
    explorer.pending_decisions.push_back(deeper_same_contour);
    assert!(matches!(
        take_next_scheduled_work_for_test(&mut explorer),
        PreparedScheduledOracleRunWorkV1::Decision { .. }
    ));
}

#[test]
fn deferred_combat_survives_frontier_checkpoint_without_a_live_search() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut branch = test_branch(0, None);
    branch.boundary = OracleRunBoundaryV1::Combat;
    explorer.branches.push(branch);
    explorer.next_branch_id = 1;
    explorer.deferred_combats.push_back(DeferredOracleCombatV1 {
        branch_id: 0,
        stage: 1,
        prior_work: empty_combat_work_checkpoint(),
    });

    let checkpoint = explorer
        .frontier_checkpoint()
        .expect("checkpoint")
        .expect("deferred combat is live work");
    assert_eq!(checkpoint.branches.len(), 1);
    assert_eq!(checkpoint.deferred_combats.len(), 1);
    assert_eq!(checkpoint.deferred_combats[0].branch_id, 0);
    assert_eq!(checkpoint.deferred_combats[0].stage, 1);
}

#[test]
fn unresolved_combat_evidence_survives_a_live_frontier_checkpoint() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut unresolved_branch = test_branch(0, None);
    unresolved_branch.boundary = OracleRunBoundaryV1::Combat;
    let live_branch = test_branch(1, None);
    explorer.branches = vec![unresolved_branch, live_branch];
    explorer.next_branch_id = 2;
    explorer
        .pending_decisions
        .push_back(test_decision(1, "live-work"));
    explorer
        .unresolved_combats
        .push(OracleRunUnresolvedCombatV1 {
            branch_id: 0,
            failure: OracleCombatWitnessFailureV1::NoVerifiedCompleteWin,
            evidence_kind: OracleRunCombatEvidenceKindV1::BudgetUnknown,
            last_status: Some("partial".to_string()),
            generation_work: 10,
            exact_states: 9,
            applied_action_transitions: 8,
            unique_successor_states: 7,
            duplicate_exact_successors: 6,
            completed_turn_options: 5,
            retained_state_work: 4,
            max_player_turn: 3,
            max_path_atomic_depth: 2,
            generation_gap_count: 1,
            incumbent_final_hp: None,
        });

    let checkpoint = explorer
        .frontier_checkpoint()
        .expect("checkpoint")
        .expect("live decision keeps a continuation");
    assert!(checkpoint
        .branches
        .iter()
        .any(|branch| branch.branch_id == 0));
    assert_eq!(checkpoint.unresolved_combats.len(), 1);
    let encoded = serde_json::to_string(&checkpoint).expect("serialize checkpoint");
    let decoded: OracleRunExplorerCheckpointV1 =
        serde_json::from_str(&encoded).expect("deserialize checkpoint");
    assert_eq!(decoded.unresolved_combats.len(), 1);
    assert_eq!(
        decoded.unresolved_combats[0].evidence_kind,
        OracleRunCombatEvidenceKindV1::BudgetUnknown
    );
    assert_eq!(decoded.unresolved_combats[0].generation_work, 10);
    let encoded_value: serde_json::Value =
        serde_json::from_str(&encoded).expect("deserialize checkpoint JSON");
    assert_eq!(
        encoded_value["unresolved_combats"][0]["evidence_kind"],
        "budget_unknown"
    );
    assert_eq!(
        encoded_value["unresolved_combats"][0]["generation_work"],
        10
    );
    assert!(
        encoded_value["unresolved_combats"][0]
            .get("nodes_expanded")
            .is_none(),
        "resident checkpoint must not serialize atomic node vocabulary"
    );
}

#[test]
fn unresolved_combat_evidence_classification_is_typed_and_conservative() {
    assert_eq!(
        combat_completion::classify_unresolved_combat_evidence(Some("frontier_exhausted"), 0),
        OracleRunCombatEvidenceKindV1::ExhaustiveRefutation
    );
    assert_eq!(
        combat_completion::classify_unresolved_combat_evidence(Some("frontier_exhausted"), 1),
        OracleRunCombatEvidenceKindV1::BudgetUnknown
    );
    for status in ["mechanics_gap", "replay_mismatch"] {
        assert_eq!(
            combat_completion::classify_unresolved_combat_evidence(Some(status), 0),
            OracleRunCombatEvidenceKindV1::SetupOrMechanicsError
        );
    }
    for status in [None, Some("partial"), Some("allowance_exhausted")] {
        assert_eq!(
            combat_completion::classify_unresolved_combat_evidence(status, 0),
            OracleRunCombatEvidenceKindV1::BudgetUnknown
        );
    }
}

fn assert_failed_decision_materialization_is_atomic(
    work: LazyOracleRunDecisionV1,
    expected_error_fragment: &str,
) {
    let mut explorer = OracleRunExplorerV1::empty();
    explorer.next_branch_id = 1;
    explorer
        .accept_branch(test_branch(0, None))
        .expect("unique parent branch");

    let error = explorer
        .prepare_explicit_decision(work, None)
        .err()
        .expect("invalid decision must fail");

    assert!(
        error.contains(expected_error_fragment),
        "unexpected materialization error: {error}"
    );
    assert_eq!(explorer.next_branch_id, 1);
    assert_eq!(explorer.branches.len(), 1);
    assert_eq!(explorer.branches[0].branch_id, 0);
    assert_eq!(explorer.branches[0].state_fingerprint, "state/0");
    assert_eq!(explorer.branches[0].session.decision_step, 0);
    assert!(explorer.branches[0].replay.is_empty());
    assert!(explorer.branches[0].journal.is_empty());
    assert_eq!(explorer.state_index.get("state/0"), Some(&0));
    assert!(explorer.retired_exact_duplicates.is_empty());
}

#[test]
fn failed_decision_materialization_never_commits_partial_explorer_state() {
    let mut missing_parent = test_decision(42, "missing-parent");
    missing_parent.parent_state_fingerprint = "state/42".to_string();
    assert_failed_decision_materialization_is_atomic(missing_parent, "missing parent branch 42");

    let mut stale_parent = test_decision(0, "stale-parent");
    stale_parent.parent_state_fingerprint = "stale-state".to_string();
    assert_failed_decision_materialization_is_atomic(
        stale_parent,
        "parent fingerprint changed for branch 0",
    );

    let illegal_action = test_decision(0, "not-a-public-candidate");
    assert_failed_decision_materialization_is_atomic(illegal_action, "candidate");
}

fn test_owner_annotation(
    _session: &RunControlSession,
    _candidate_id: &str,
) -> Option<RunControlTraceAnnotationV1> {
    Some(RunControlTraceAnnotationV1::CardRewardOwnerDecision {
        provenance: CardRewardOwnerProvenanceV1 {
            functions: vec![CardRewardFunctionV1::Access],
            obligations: vec![CardRewardObligationDeltaV1 {
                source: CardRewardObligationSourceV1::KnownBoss,
                subject: "test_boss".to_string(),
                deadline_nodes: Some(16),
                gaps_before: 1,
                gaps_after: 1,
            }],
            strengthened_capabilities: Vec::new(),
            hard_startup_liability: false,
            hard_duplicate_liability: false,
            component_debt_count: 0,
            access_saturated: false,
            stable_surface_index: 0,
            owner_rank: 1,
            tie_break_applied: false,
        },
    })
}

#[test]
fn oracle_settles_empty_campfire_as_typed_forced_progress() {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.engine_state = EngineState::Campfire;
    session
        .run_state
        .relics
        .push(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::CoffeeDripper,
        ));
    session
        .run_state
        .relics
        .push(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::FusionHammer,
        ));

    let steps = decision_materialization::settle_oracle_forced_transitions(&mut session)
        .expect("an optionless campfire should settle without owner input");

    assert!(matches!(session.engine_state, EngineState::MapNavigation));
    assert!(matches!(
        steps.as_slice(),
        [RunProgressStepV1::ForcedTransition(transition)]
            if transition.kind
                == crate::eval::run_control::RunForcedTransitionKindV1::EmptyCampfireExit
    ));
}

#[test]
fn materialized_oracle_decision_commits_owner_provenance_to_the_journal() {
    let session = RunControlSession::new(RunControlConfig::default());
    let surface = build_decision_surface(&session);
    let candidate = surface
        .view
        .candidates
        .iter()
        .find_map(|candidate| {
            candidate
                .action
                .executable_input()
                .map(|input| (candidate, input.clone()))
        })
        .expect("one executable initial candidate");
    let fingerprint = run_session_fingerprint_v3(&session);
    let parent = OracleRunBranchV1 {
        branch_id: 0,
        parent_branch_id: None,
        neow_root_candidate_id: "root".to_string(),
        neow_root_label: "root".to_string(),
        state_fingerprint: fingerprint.clone(),
        boundary: classify_run_boundary(&session),
        path_negative_log_policy: 0.0,
        path_discrepancy: 0,
        path_depth: 0,
        replay: Vec::new(),
        journal: RunProgressJournalV1::default(),
        session,
    };
    let work = LazyOracleRunDecisionV1 {
        parent_branch_id: 0,
        parent_state_fingerprint: fingerprint,
        neow_root_candidate_id: "root".to_string(),
        kind: OracleRunWorkKindV1::EventOption,
        candidate_id: candidate.0.id.clone(),
        label: candidate.0.label.clone(),
        action: RunDecisionAction::Input(candidate.1),
        stable_work_key: "test-owner-provenance".to_string(),
        path_negative_log_policy: 0.0,
        path_discrepancy: 0,
        path_depth: 1,
        parent_act: parent.session.run_state.act_num,
        parent_floor: parent.session.run_state.floor_num,
        combat_edge_probe: None,
    };
    let mut explorer = OracleRunExplorerV1::empty();
    explorer.branches.push(parent);
    explorer.next_branch_id = 1;

    let prepared = explorer
        .prepare_explicit_decision(work, Some(test_owner_annotation))
        .expect("prepare decision");
    let child_id = explorer
        .commit_prepared_decision(prepared)
        .expect("unique child");
    let transaction = explorer
        .branches
        .iter()
        .find(|branch| branch.branch_id == child_id)
        .and_then(|branch| branch.journal.entries().first())
        .and_then(RunProgressStepV1::as_decision)
        .expect("journaled decision transaction");
    assert!(matches!(
        transaction.trace_annotations.as_slice(),
        [RunControlTraceAnnotationV1::CardRewardOwnerDecision { .. }]
    ));
}

#[test]
fn heuristic_probe_only_orders_two_immediate_combat_edges() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut owner_first = test_decision(0, "owner-first");
    owner_first.path_discrepancy = 0;
    owner_first.combat_edge_probe = Some(OracleRunCombatEdgeProbeV1::HeuristicEstimate {
        order_key: shadow_key(5, 5),
    });
    let mut owner_second = test_decision(0, "owner-second");
    owner_second.path_discrepancy = 1;
    owner_second.combat_edge_probe = Some(OracleRunCombatEdgeProbeV1::HeuristicEstimate {
        order_key: shadow_key(20, 40),
    });
    explorer.pending_decisions.push_back(owner_first);
    explorer.pending_decisions.push_back(owner_second);

    let selected = explorer.take_best_decision().expect("one edge selected");
    assert_eq!(selected.candidate_id, "owner-second");
    assert_eq!(selected.path_discrepancy, 1);
    assert_eq!(explorer.pending_decisions.len(), 1);
}

#[test]
fn edge_probe_never_promotes_a_noncombat_decision_over_owner_order() {
    let mut explorer = OracleRunExplorerV1::empty();
    let owner_first = test_decision(0, "owner-first");
    let mut unrelated_noncombat = test_decision(0, "noncombat");
    unrelated_noncombat.path_discrepancy = 1;
    unrelated_noncombat.combat_edge_probe = Some(OracleRunCombatEdgeProbeV1::NotImmediateCombat);
    explorer.pending_decisions.push_back(owner_first);
    explorer.pending_decisions.push_back(unrelated_noncombat);

    let selected = explorer
        .take_best_decision()
        .expect("one decision selected");
    assert_eq!(selected.candidate_id, "owner-first");
}

#[test]
fn seed006_registers_all_completed_neow_roots_without_selecting_one() {
    let session = RunControlSession::new(RunControlConfig {
        seed: 6,
        ascension_level: 0,
        ..RunControlConfig::default()
    });
    let expansion = expand_oracle_neow_candidates_v1(&session).expect("Neow expansion");
    let completed = expansion.completed.len();
    let explorer = seed_oracle_run_explorer_v1(expansion, None).expect("oracle run seed");
    assert_eq!(explorer.branches.len(), completed);
    assert!(!explorer.pending_decisions.is_empty());
    assert_eq!(explorer.pending_combat_count(), 0);
}

#[test]
fn changing_act_number_does_not_create_an_artificial_act_completion_boundary() {
    let mut session = RunControlSession::new(RunControlConfig::default());
    session.run_state.act_num = 2;
    session.engine_state = EngineState::MapNavigation;
    assert_eq!(
        classify_run_boundary(&session),
        OracleRunBoundaryV1::MapDecision
    );
}

#[test]
fn canonical_oracle_hash_ignores_hash_map_insertion_order() {
    let mut left = std::collections::HashMap::new();
    left.insert("z", 1);
    left.insert("a", 2);
    let mut right = std::collections::HashMap::new();
    right.insert("a", 2);
    right.insert("z", 1);

    assert_eq!(canonical_oracle_hash(&left), canonical_oracle_hash(&right));
}

#[test]
fn decision_policy_prefers_owner_order_and_keeps_every_fallback_positive() {
    fn prefer_second(
        _: &RunControlSession,
        legal: &[RunPolicyCandidateV1<'_>],
    ) -> Result<crate::eval::run_control::RunPolicyPriorV1, String> {
        positive_ranked_run_policy_prior_v1(legal, ["second".to_string()])
    }

    let branch = test_branch(7, None);
    let mut work = vec![test_decision(7, "first"), test_decision(7, "second")];
    apply_decision_policy(&branch, &mut work, Some(prefer_second))
        .expect("valid complete policy prior");

    let first = work
        .iter()
        .find(|work| work.candidate_id == "first")
        .unwrap();
    let second = work
        .iter()
        .find(|work| work.candidate_id == "second")
        .unwrap();
    assert!(second.path_negative_log_policy < first.path_negative_log_policy);
    assert!(first.path_negative_log_policy.is_finite());
    assert_eq!(second.path_discrepancy, 0);
    assert_eq!(first.path_discrepancy, 1);
    assert_eq!(first.path_depth, 2);
    assert_eq!(second.path_depth, 2);
}

#[test]
fn zero_discrepancy_mainline_continues_before_a_shallower_sibling() {
    let mut explorer = OracleRunExplorerV1::empty();
    let early_branch = test_branch(0, None);
    let mut deep_branch = test_branch(9, Some(0));
    deep_branch.session.run_state.floor_num = 10;
    explorer.branches = vec![early_branch, deep_branch];
    let mut deep = test_decision(9, "deep-policy-head");
    deep.path_depth = 20;
    deep.path_negative_log_policy = 8.0;
    let mut early = test_decision(0, "early-alternative");
    early.path_depth = 3;
    early.path_negative_log_policy = 2.0;
    explorer.pending_decisions = VecDeque::from([deep, early]);

    let selected = explorer.take_best_decision().expect("global work");
    assert_eq!(selected.candidate_id, "deep-policy-head");
    assert_eq!(explorer.pending_decisions.len(), 1);
}

#[test]
fn exact_run_progress_precedes_journal_depth_within_one_discrepancy_contour() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut early_but_busy = test_decision(0, "act-1-many-actions");
    early_but_busy.path_discrepancy = 1;
    early_but_busy.parent_act = 1;
    early_but_busy.parent_floor = 7;
    early_but_busy.path_depth = 200;
    let mut later_run_state = test_decision(1, "act-2-boss");
    later_run_state.path_discrepancy = 1;
    later_run_state.parent_act = 2;
    later_run_state.parent_floor = 32;
    later_run_state.path_depth = 20;
    explorer.pending_decisions = VecDeque::from([early_but_busy, later_run_state]);

    let selected = explorer.take_best_decision().expect("global work");
    assert_eq!(selected.candidate_id, "act-2-boss");
}

#[test]
fn another_root_mainline_precedes_a_deviation_from_the_first_root() {
    let mut explorer = OracleRunExplorerV1::empty();
    explorer.branches = vec![test_branch(0, None), test_branch(1, None)];
    let mut first_root_deviation = test_decision(0, "root-0-rank-1");
    first_root_deviation.path_discrepancy = 1;
    first_root_deviation.path_depth = 20;
    let mut second_root_mainline = test_decision(1, "root-1-rank-0");
    second_root_mainline.path_discrepancy = 0;
    second_root_mainline.path_depth = 2;
    explorer.pending_decisions = VecDeque::from([first_root_deviation, second_root_mainline]);

    let selected = explorer.take_best_decision().expect("strategic work");
    assert_eq!(selected.candidate_id, "root-1-rank-0");
}

#[test]
fn a_wide_neow_root_cannot_monopolize_strategic_service() {
    let mut explorer = OracleRunExplorerV1::empty();
    let mut root_zero_first = test_decision(0, "root-0-first");
    root_zero_first.neow_root_candidate_id = "0".to_string();
    let mut root_zero_second = test_decision(1, "root-0-second");
    root_zero_second.neow_root_candidate_id = "0".to_string();
    root_zero_second.path_depth = 100;
    let mut root_one = test_decision(2, "root-1");
    root_one.neow_root_candidate_id = "1".to_string();
    root_one.path_discrepancy = 20;
    explorer.pending_decisions = VecDeque::from([root_zero_first, root_zero_second, root_one]);

    let first = take_next_scheduled_work_for_test(&mut explorer);
    assert!(matches!(
        first,
        PreparedScheduledOracleRunWorkV1::Decision { ref work, .. }
            if work.neow_root_candidate_id == "0"
    ));
    let second = take_next_scheduled_work_for_test(&mut explorer);
    assert!(matches!(
        second,
        PreparedScheduledOracleRunWorkV1::Decision { ref work, .. }
            if work.neow_root_candidate_id == "1"
    ));
    let third = take_next_scheduled_work_for_test(&mut explorer);
    assert!(matches!(
        third,
        PreparedScheduledOracleRunWorkV1::Decision { ref work, .. }
            if work.neow_root_candidate_id == "0"
    ));
}

#[test]
fn a_single_combat_remains_exactly_resumable_across_quanta() {
    let mut session = RunControlSession::new(RunControlConfig::default());
    let mut combat = crate::test_support::blank_test_combat();
    combat.entities.monsters = vec![crate::test_support::planned_monster(
        crate::content::monsters::EnemyId::JawWorm,
        1,
    )];
    session.active_combat = Some(ActiveCombat::new(
        EngineState::CombatPlayerTurn,
        combat,
        CombatContext::Room(RoomCombatContext {
            room_type: RoomType::MonsterRoom,
        }),
    ));

    let mut explorer = OracleRunExplorerV1::empty();
    let branch = OracleRunBranchV1 {
        branch_id: 0,
        parent_branch_id: None,
        neow_root_candidate_id: "test_root".to_string(),
        neow_root_label: "test root".to_string(),
        state_fingerprint: run_session_fingerprint_v3(&session),
        boundary: OracleRunBoundaryV1::Combat,
        path_negative_log_policy: 0.0,
        path_discrepancy: 0,
        path_depth: 1,
        replay: Vec::new(),
        journal: RunProgressJournalV1::default(),
        session,
    };
    explorer.next_branch_id = 1;
    let branch_id = explorer.accept_branch(branch).expect("unique branch");
    let combat_budgets = OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1 {
        max_generation_work: Some(8),
        wall_ms: None,
        satisfaction: Some(CombatWitnessSatisfactionV1::BudgetOrExhaustion),
        ..OracleCombatWitnessOptionsV1::default()
    });
    explorer
        .schedule_branch(branch_id, &combat_budgets, None)
        .expect("combat work should schedule");
    let mut sibling = test_decision(0, "strategic-sibling");
    sibling.parent_state_fingerprint = explorer.branches[0].state_fingerprint.clone();
    explorer.pending_decisions.push_back(sibling);

    let result = drive_oracle_run_explorer_v1(
        explorer,
        OracleRunExploreBudgetV1 {
            max_work_items: 2,
            wall_ms: None,
            combat: combat_budgets.clone(),
            combat_quantum_generation_work: 1,
            combat_quantum_ms: None,
            decision_prior: None,
            decision_annotation: None,
            combat_edge_order: None,
        },
    )
    .expect("one explorer quantum");

    assert_eq!(result.stop, OracleRunExploreStopV1::WorkBudgetExhausted);
    assert_eq!(result.combat_quanta, 2);
    assert_eq!(result.explorer.pending_combat_count(), 1);
    assert_eq!(result.explorer.pending_decisions.len(), 1);
    assert_eq!(
        result.explorer.pending_decisions[0].candidate_id,
        "strategic-sibling"
    );
    assert!(result.explorer.unresolved_combats.is_empty());
    let pending = result
        .explorer
        .pending_combat_summaries()
        .expect("pending combat summary");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].branch_id, 0);
    assert_eq!(pending[0].enemies.len(), 1);
    assert_eq!(pending[0].enemies[0].name, "Jaw Worm");
    assert_eq!(pending[0].quantum_count, 2);
    assert!(pending[0].last_quantum_generation_work <= 1);
    assert!(pending[0].exact_states >= 1);
    assert!(pending[0].retained_state_work >= 1);
    assert_eq!(pending[0].quanta_since_incumbent_improvement, 2);
    assert_eq!(pending[0].incumbent_revision, 0);
    assert_eq!(
        pending[0].resume_kind,
        OracleCombatSearchResumeKindV1::Fresh
    );
    assert_eq!(pending[0].restart_count, 0);
    let pending_json = serde_json::to_value(&pending[0]).expect("serialize pending combat");
    assert_eq!(pending_json["generation_work"], pending[0].generation_work);
    assert!(
        pending_json.get("nodes_expanded").is_none(),
        "resident pending evidence must not serialize atomic node vocabulary"
    );
    let consumed_before_restart = pending[0].generation_work;
    let remaining_before_restart = pending[0].remaining_generation_work;

    let checkpoint = result
        .explorer
        .frontier_checkpoint()
        .expect("frontier checkpoint")
        .expect("live frontier");
    let encoded = serde_json::to_vec(&checkpoint).expect("serialize frontier");
    let decoded: OracleRunExplorerCheckpointV1 =
        serde_json::from_slice(&encoded).expect("deserialize frontier");
    assert!(decoded.active_combat.is_some());
    let restored = seed_oracle_run_explorer_from_checkpoint_v1(decoded, &combat_budgets)
        .expect("restore frontier");

    assert_eq!(restored.pending_combat_count(), 1);
    assert_eq!(restored.combat_search_restarts, 1);
    assert_eq!(restored.pending_decisions.len(), 1);
    assert_eq!(
        restored.pending_decisions[0].candidate_id,
        "strategic-sibling"
    );
    let restored_pending = restored
        .pending_combat_summaries()
        .expect("restored pending combat summary");
    assert_eq!(restored_pending[0].generation_work, consumed_before_restart);
    assert_eq!(
        restored_pending[0].remaining_generation_work,
        remaining_before_restart
    );
    assert_eq!(restored_pending[0].quantum_count, 2);
    assert_eq!(
        restored_pending[0].incumbent_revision,
        pending[0].incumbent_revision
    );
    assert_eq!(
        restored_pending[0].quanta_since_incumbent_improvement,
        pending[0].quanta_since_incumbent_improvement
    );
    assert_eq!(restored_pending[0].restart_count, 1);
    assert_eq!(
        restored_pending[0].resume_kind,
        OracleCombatSearchResumeKindV1::StateReplayExactSearchRestarted
    );
}
