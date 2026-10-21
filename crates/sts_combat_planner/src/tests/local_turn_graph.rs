use super::*;
use crate::types::CompleteTurnOptionSource;

struct QueryMergeStepper;

impl CombatStepper for QueryMergeStepper {
    fn atomic_actions(&self, position: &CombatPosition) -> Vec<ClientInput> {
        match &position.engine {
            EngineState::PendingChoice(PendingChoice::DiscoverySelect(choice)) => {
                (0..choice.cards.len())
                    .map(ClientInput::SubmitDiscoverChoice)
                    .collect()
            }
            EngineState::CombatPlayerTurn => vec![ClientInput::EndTurn],
            _ => Vec::new(),
        }
    }

    fn apply_to_stable(
        &self,
        position: &CombatPosition,
        input: ClientInput,
        _limits: CombatStepLimits,
    ) -> CombatStepResult {
        let mut next = position.clone();
        match input {
            ClientInput::SubmitDiscoverChoice(_) => {
                // Both choices deliberately reach the same simulator state.
                // Only the query product state may distinguish them.
                next.engine = EngineState::CombatPlayerTurn;
            }
            ClientInput::EndTurn if next.combat.turn.turn_count == 0 => {
                next.combat.turn.turn_count = 1;
            }
            ClientInput::EndTurn => {
                next.engine = EngineState::GameOver(sts_core::state::core::RunResult::Victory);
            }
            _ => panic!("query merge stepper received unsupported input"),
        }
        CombatStepResult {
            terminal: self.terminal(&next),
            position: next,
            alive: true,
            truncated: false,
            timed_out: false,
            engine_steps: 1,
        }
    }

    fn terminal(&self, position: &CombatPosition) -> CombatTerminal {
        if matches!(
            position.engine,
            EngineState::GameOver(sts_core::state::core::RunResult::Victory)
        ) {
            CombatTerminal::Win
        } else {
            CombatTerminal::Unresolved
        }
    }
}

#[test]
fn local_turn_graph_keeps_query_progress_across_equal_turn_boundaries() {
    let mut combat = sts_core::test_support::blank_test_combat();
    combat.entities.monsters = vec![sts_core::test_support::test_monster(EnemyId::JawWorm)];
    combat.turn.turn_count = 0;
    let root_position = CombatPosition::new(
        EngineState::PendingChoice(PendingChoice::DiscoverySelect(
            sts_core::state::core::DiscoveryChoiceState {
                cards: vec![CardId::Panacea, CardId::MasterOfStrategy],
                colorless: true,
                card_type: None,
                amount: 1,
                can_skip: false,
            },
        )),
        combat,
    );
    let root = CombatDecisionRoot::new(root_position.clone())
        .expect("pending Discovery is a stable combat root");
    let query =
        CombatTrajectoryQueryV1::eventually(CombatTrajectoryAtomV1::GeneratedCardSelected {
            card_id: CardId::MasterOfStrategy,
        });
    let mut session = LocalTurnGraphWitnessSession::with_policy_and_trajectory_query(
        root,
        LocalTurnGraphWitnessConfig {
            generator: config(),
            initial_expansion_work: 32,
            root_initial_expansion_work: 32,
            max_turn_depth: 2,
            satisfaction: OracleCombatWitnessSatisfaction::FirstWitness,
            ..LocalTurnGraphWitnessConfig::default()
        },
        Arc::new(PreferEndTurnPolicy),
        &query,
    )
    .expect("supported query");
    let report = session.advance(
        LocalTurnGraphWitnessQuantum {
            additional_selections: 128,
            additional_generation_work: 512,
            additional_engine_steps: 2_048,
            deadline: None,
        },
        &QueryMergeStepper,
    );

    assert_eq!(report.status, LocalTurnGraphWitnessStatus::WitnessFound);
    assert!(report.counters.exact_nodes >= 3);
    assert!(report.counters.trajectory_query_rejected_terminal_wins >= 1);
    let witness = report.witness.expect("query-satisfying exact witness");
    let events =
        replay_combat_trajectory_events_v1(&root_position, &witness.actions, &QueryMergeStepper)
            .expect("witness event replay");
    assert!(
        evaluate_combat_trajectory_query_v1(&query, &events)
            .expect("query evaluation")
            .satisfied
    );
}

#[test]
fn local_turn_graph_retires_finished_generator_search_storage() {
    let mut session = LocalTurnGraphWitnessSession::with_policy(
        root(),
        LocalTurnGraphWitnessConfig {
            generator: config(),
            initial_expansion_work: 16,
            root_initial_expansion_work: 16,
            max_turn_depth: 1,
            satisfaction: OracleCombatWitnessSatisfaction::BudgetOrExhaustion,
            ..LocalTurnGraphWitnessConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );
    session.advance(
        LocalTurnGraphWitnessQuantum {
            additional_selections: 64,
            additional_generation_work: 256,
            additional_engine_steps: 1_024,
            deadline: None,
        },
        &TinyTurnStepper::plain(),
    );

    let storage = session.storage_snapshot();
    assert!(storage.finished_generators > 0);
    assert_eq!(storage.finished_generator_work_capacity, 0);
    assert_eq!(storage.finished_generator_seen_capacity, 0);
    assert_eq!(storage.finished_generator_anchor_capacity, 0);
    assert_eq!(storage.finished_generator_guide_capacity, 0);
    assert_eq!(storage.finished_generator_scheduled_round_capacity, 0);
}

#[test]
fn local_turn_graph_absolute_final_hp_satisfaction_uses_terminal_hp() {
    let decision_root = root();
    let terminal_hp = decision_root.position().combat.entities.player.current_hp;
    let config_for = |minimum| LocalTurnGraphWitnessConfig {
        generator: config(),
        initial_expansion_work: 16,
        root_initial_expansion_work: 16,
        max_turn_depth: 1,
        satisfaction: OracleCombatWitnessSatisfaction::FinalHpAtLeast(minimum),
        ..LocalTurnGraphWitnessConfig::default()
    };
    let quantum = LocalTurnGraphWitnessQuantum {
        additional_selections: 64,
        additional_generation_work: 256,
        additional_engine_steps: 1_024,
        deadline: None,
    };

    let mut reached = LocalTurnGraphWitnessSession::with_policy(
        decision_root.clone(),
        config_for(terminal_hp),
        Arc::new(PreferPlayPolicy),
    );
    let reached_report = reached.advance(quantum, &TinyTurnStepper::lethal());
    assert_eq!(
        reached_report.status,
        LocalTurnGraphWitnessStatus::WitnessFound
    );

    let mut missed = LocalTurnGraphWitnessSession::with_policy(
        decision_root,
        config_for(terminal_hp.saturating_add(1)),
        Arc::new(PreferPlayPolicy),
    );
    let missed_report = missed.advance(quantum, &TinyTurnStepper::lethal());
    assert_ne!(
        missed_report.status,
        LocalTurnGraphWitnessStatus::WitnessFound
    );
    assert_eq!(
        missed
            .witness()
            .expect("the lower-HP exact witness remains diagnostic evidence")
            .final_position
            .combat
            .entities
            .player
            .current_hp,
        terminal_hp
    );
}

#[test]
fn local_turn_graph_policy_line_defers_reserved_conversion_at_a_safe_phase_boundary() {
    let stepper = TinyTurnStepper::activating_finite_skill_conversion();
    let mut session = LocalTurnGraphWitnessSession::with_policy(
        awakened_conversion_root(),
        LocalTurnGraphWitnessConfig {
            generator: config(),
            ..LocalTurnGraphWitnessConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );

    let report = session
        .offer_plan_compatible_policy_line(1, 4, &stepper)
        .expect("plan-compatible line");

    assert_eq!(report.proposed_turns, 1);
    assert_eq!(report.deferred_actions, 1);
    assert_eq!(report.rejected_preview_transitions, 1);
    assert_eq!(stepper.call_count(&PLAY), 1);
    assert_eq!(stepper.call_count(&ClientInput::EndTurn), 1);
    let families = session.root_action_families();
    assert_eq!(families.len(), 1);
    assert_eq!(families[0].first_action, ClientInput::EndTurn);
}

#[test]
fn local_turn_graph_policy_line_does_not_end_before_an_emergency_conversion() {
    let stepper = TinyTurnStepper::activating_finite_skill_conversion();
    let mut session = LocalTurnGraphWitnessSession::with_policy(
        threatened_awakened_conversion_root(),
        LocalTurnGraphWitnessConfig {
            generator: config(),
            ..LocalTurnGraphWitnessConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );

    let report = session
        .offer_plan_compatible_policy_line(1, 1, &stepper)
        .expect("plan-compatible line");

    assert_eq!(report.deferred_actions, 0);
    assert_eq!(stepper.call_count(&PLAY), 1);
    assert_eq!(stepper.call_count(&ClientInput::EndTurn), 0);
}

#[test]
fn local_turn_graph_policy_line_does_not_defer_a_phase_committing_action() {
    let stepper = TinyTurnStepper {
        activates_finite_skill_conversion: true,
        opens_awakened_transition_window: true,
        ..TinyTurnStepper::plain()
    };
    let mut session = LocalTurnGraphWitnessSession::with_policy(
        awakened_conversion_root(),
        LocalTurnGraphWitnessConfig {
            generator: config(),
            ..LocalTurnGraphWitnessConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );

    let report = session
        .offer_plan_compatible_policy_line(1, 1, &stepper)
        .expect("plan-compatible line");

    assert_eq!(report.deferred_actions, 0);
    assert_eq!(stepper.call_count(&PLAY), 1);
    assert_eq!(stepper.call_count(&ClientInput::EndTurn), 0);
}

#[test]
fn local_turn_graph_policy_line_deploys_conversion_in_the_untaxed_window() {
    let stepper = TinyTurnStepper::activating_finite_skill_conversion();
    let mut session = LocalTurnGraphWitnessSession::with_policy(
        transition_window_conversion_root(),
        LocalTurnGraphWitnessConfig {
            generator: config(),
            ..LocalTurnGraphWitnessConfig::default()
        },
        Arc::new(PreferEndTurnPolicy),
    );

    let report = session
        .offer_plan_compatible_policy_line(1, 4, &stepper)
        .expect("plan-compatible line");

    assert_eq!(report.proposed_turns, 1);
    assert_eq!(report.deferred_actions, 0);
    let families = session.root_action_families();
    assert_eq!(families.len(), 1);
    assert_eq!(families[0].first_action, PLAY);
}

#[test]
fn local_turn_graph_policy_line_crosses_a_ranked_single_selection_transaction() {
    let stepper = TinyTurnStepper::with_winning_single_selection();
    let mut session = LocalTurnGraphWitnessSession::with_policy(
        awakened_root(),
        LocalTurnGraphWitnessConfig {
            generator: config(),
            ..LocalTurnGraphWitnessConfig::default()
        },
        Arc::new(PreferSelection22Policy),
    );

    let report = session
        .offer_plan_compatible_policy_line(1, 4, &stepper)
        .expect("plan-compatible line");

    assert!(report.reached_terminal_win);
    assert_eq!(report.chosen_action_transitions, 2);
    assert_eq!(
        stepper.call_count(&PLAY),
        2,
        "the proposed play and the authoritative witness replay each execute once"
    );
    assert!(session.witness().is_some());
}

#[test]
fn local_turn_graph_plan_annotations_are_opt_in_and_read_only() {
    let root = awakened_root();
    let parent_hash = root.exact_state_hash().to_owned();
    let successor_stepper = TinyTurnStepper::plain();
    let direct_end_turn = successor_stepper.apply_to_stable(
        root.position(),
        ClientInput::EndTurn,
        CombatStepLimits {
            max_engine_steps: 4,
            deadline: None,
        },
    );
    let successor_hash = exact_hash(&direct_end_turn.position);
    let search_config = LocalTurnGraphWitnessConfig {
        generator: config(),
        generation_quantum_work: 4,
        backed_generation_quantum_work: 4,
        initial_expansion_work: 16,
        root_initial_expansion_work: 16,
        max_turn_depth: 1,
        satisfaction: OracleCombatWitnessSatisfaction::BudgetOrExhaustion,
        ..LocalTurnGraphWitnessConfig::default()
    };
    let quantum = LocalTurnGraphWitnessQuantum {
        additional_selections: 64,
        additional_generation_work: 256,
        additional_engine_steps: 1_024,
        deadline: None,
    };

    let mut plain = LocalTurnGraphWitnessSession::with_policy(
        root.clone(),
        search_config,
        Arc::new(PreferPlayPolicy),
    );
    let plain_report = plain.advance(quantum, &TinyTurnStepper::plain());
    let plain_edge = plain
        .edge_snapshot_by_exact_hashes(&parent_hash, &successor_hash)
        .expect("the direct end-turn edge must be materialized");
    assert_eq!(plain_edge.plan_transition_annotation, None);
    assert_eq!(plain.counters().annotated_exact_edges, 0);

    let mut annotated =
        LocalTurnGraphWitnessSession::with_policy(root, search_config, Arc::new(PreferPlayPolicy));
    annotated
        .enable_plan_transition_annotations()
        .expect("annotation collection must be enabled before graph construction");
    let annotated_report = annotated.advance(quantum, &TinyTurnStepper::plain());
    let annotated_edge = annotated
        .edge_snapshot_by_exact_hashes(&parent_hash, &successor_hash)
        .expect("the annotated direct end-turn edge must be materialized");
    assert!(
        annotated_edge.plan_transition_annotation.is_some(),
        "an encounter-owned typed plan must annotate its exact edge"
    );
    assert!(annotated.counters().annotated_exact_edges > 0);
    let plan_edges = annotated.plan_transition_edge_snapshots();
    assert_eq!(plan_edges.len(), annotated.counters().annotated_exact_edges);
    assert!(plan_edges.iter().any(|edge| {
        edge.parent_exact_state_hash == parent_hash
            && edge.successor_exact_state_hash == successor_hash
            && edge.action_count == annotated_edge.actions.len()
    }));
    let plan_paths = annotated.plan_transition_path_snapshots();
    let direct_path = plan_paths
        .iter()
        .find(|path| path.successor_exact_state_hash == successor_hash)
        .expect("the annotated successor must expose a replayable root path");
    assert_eq!(direct_path.parent_action_count, 0);
    assert_eq!(direct_path.root_actions, annotated_edge.actions);
    assert_eq!(
        direct_path.plan_transition_annotation,
        annotated_edge
            .plan_transition_annotation
            .clone()
            .expect("annotated edge")
    );
    assert_eq!(
        annotated.enable_plan_transition_annotations(),
        Err(LocalTurnGraphPlanAnnotationEnableError::EdgesAlreadyMaterialized)
    );

    let mut plain_counters = plain.counters();
    let mut annotated_counters = annotated.counters();
    annotated_counters.annotated_exact_edges = 0;
    plain_counters.annotated_exact_edges = 0;
    assert_eq!(annotated_report.status, plain_report.status);
    assert_eq!(annotated_counters, plain_counters);
    assert_eq!(annotated_edge.actions, plain_edge.actions);
    assert_eq!(
        annotated_edge.negative_log_policy,
        plain_edge.negative_log_policy
    );
}

#[test]
fn state_service_index_attributes_plan_prefix_work_to_its_exact_boundary() {
    let root = double_thief_bridge_root();
    let root_hash = root.exact_state_hash().to_owned();
    let mut session = LocalTurnGraphWitnessSession::with_policy(
        root,
        LocalTurnGraphWitnessConfig {
            generator: TurnOptionGeneratorConfig {
                max_engine_steps_per_transition: 250,
                ..TurnOptionGeneratorConfig::default()
            },
            generation_quantum_work: 4,
            backed_generation_quantum_work: 4,
            initial_expansion_work: 16,
            root_initial_expansion_work: 16,
            max_turn_depth: 1,
            satisfaction: OracleCombatWitnessSatisfaction::BudgetOrExhaustion,
            ..LocalTurnGraphWitnessConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );
    assert_eq!(session.counters().plan_prefix_root_enqueues, 1);

    session.advance(
        LocalTurnGraphWitnessQuantum {
            additional_selections: 64,
            additional_generation_work: 256,
            additional_engine_steps: 8_192,
            deadline: None,
        },
        &EngineCombatStepper,
    );

    let state = session
        .state_service_index()
        .into_iter()
        .find(|state| state.exact_state_hash == root_hash)
        .expect("root service attribution");
    assert!(state.plan_prefix_applicable);
    assert!(state.plan_prefix_step_count.is_some_and(|count| count > 0));
    assert_eq!(state.plan_prefix_attempts, 1);
    assert_eq!(state.plan_prefix_completed, 1);
    assert_eq!(state.plan_prefix_rejections, 0);
    assert_eq!(state.plan_prefix_successor_exact_state_hashes.len(), 1);
    assert!(state.generation_anchor_services > 0);
}

#[test]
fn plan_prefix_completes_when_combat_ends_before_a_trailing_end_turn() {
    let mut generator = TurnOptionGeneratorSession::new(
        escaping_thief_lethal_root(),
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 250,
            ..TurnOptionGeneratorConfig::default()
        },
    );

    let report = generator.advance_with_continuation_proposal(
        &EngineCombatStepper,
        CombatPlanningQuantum {
            additional_generation_work: 16,
            additional_engine_steps: 4_000,
            deadline: None,
        },
    );
    assert_eq!(
        report.status,
        TurnOptionGenerationStatus::PlanPrefixServiced
    );

    let diagnostics = generator.diagnostics();
    let options = generator.take_completed_options();
    let prefix = options
        .iter()
        .find(|option| option.source() == CompleteTurnOptionSource::EncounterPlanPrefix)
        .expect("terminal prefix should become an exact option");
    assert_eq!(prefix.actions().len(), 1);
    assert_eq!(prefix.boundary(), CompleteTurnOptionBoundary::TerminalWin);
    assert_eq!(diagnostics.plan_prefix_attempts, 1);
    assert_eq!(diagnostics.plan_prefix_completed, 1);
    assert_eq!(diagnostics.plan_prefix_rejections, 0);
}

#[test]
fn continuation_only_prefix_waits_for_continuation_service() {
    let mut generator = TurnOptionGeneratorSession::new(
        escaping_thief_lethal_root(),
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 250,
            ..TurnOptionGeneratorConfig::default()
        },
    );

    generator.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum {
            additional_generation_work: 1,
            additional_engine_steps: 250,
            deadline: None,
        },
    );
    assert_eq!(generator.diagnostics().plan_prefix_attempts, 0);

    let report = generator.advance_with_continuation_proposal(
        &EngineCombatStepper,
        CombatPlanningQuantum {
            additional_generation_work: 16,
            additional_engine_steps: 4_000,
            deadline: None,
        },
    );
    assert_eq!(
        report.status,
        TurnOptionGenerationStatus::PlanPrefixServiced
    );
    assert_eq!(generator.diagnostics().plan_prefix_attempts, 1);
    assert_eq!(generator.diagnostics().plan_prefix_completed, 1);
}

#[test]
fn root_eligible_prefix_waits_for_proposal_root_service() {
    let mut generator = TurnOptionGeneratorSession::new(
        double_thief_bridge_root(),
        TurnOptionGeneratorConfig {
            max_engine_steps_per_transition: 250,
            ..TurnOptionGeneratorConfig::default()
        },
    );

    generator.advance(
        &EngineCombatStepper,
        CombatPlanningQuantum {
            additional_generation_work: 1,
            additional_engine_steps: 250,
            deadline: None,
        },
    );
    assert_eq!(generator.diagnostics().plan_prefix_attempts, 0);

    let report = generator.advance_with_root_proposal(
        &EngineCombatStepper,
        CombatPlanningQuantum {
            additional_generation_work: 16,
            additional_engine_steps: 4_000,
            deadline: None,
        },
    );
    assert_eq!(
        report.status,
        TurnOptionGenerationStatus::PlanPrefixServiced
    );
    assert_eq!(generator.diagnostics().plan_prefix_attempts, 1);
    assert_eq!(generator.diagnostics().plan_prefix_completed, 1);
}

#[test]
fn local_turn_graph_plan_annotations_leave_unowned_encounters_empty() {
    let mut session = LocalTurnGraphWitnessSession::with_policy(
        root(),
        LocalTurnGraphWitnessConfig {
            generator: config(),
            initial_expansion_work: 16,
            root_initial_expansion_work: 16,
            max_turn_depth: 1,
            satisfaction: OracleCombatWitnessSatisfaction::BudgetOrExhaustion,
            ..LocalTurnGraphWitnessConfig::default()
        },
        Arc::new(PreferPlayPolicy),
    );
    session
        .enable_plan_transition_annotations()
        .expect("an empty graph can enable annotations");
    session.advance(
        LocalTurnGraphWitnessQuantum {
            additional_selections: 64,
            additional_generation_work: 256,
            additional_engine_steps: 1_024,
            deadline: None,
        },
        &TinyTurnStepper::plain(),
    );

    assert!(session.counters().exact_edges > 0);
    assert_eq!(session.counters().annotated_exact_edges, 0);
    assert!(session.plan_transition_edge_snapshots().is_empty());
}
