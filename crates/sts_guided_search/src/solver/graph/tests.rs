use super::*;
use sts_core::content::cards::CardId;
use sts_core::state::core::{EngineState, HandSelectReason, PendingChoice};

fn source() -> Node {
    Node::root(
        CombatSearchState::new(super::super::tests::root(
            &[CardId::Strike, CardId::Strike, CardId::Defend],
            12,
        ))
        .unwrap(),
    )
}

fn limits() -> CombatStepLimits {
    CombatStepLimits {
        max_engine_steps: 512,
        deadline: None,
    }
}

fn execute(graph: &mut Graph, node: &Node, branch: CombatBranch, stats: &mut SolverStats) -> Node {
    let choices = CombatSearchSpace::default()
        .prepare_choices(&node.state)
        .unwrap();
    let i = choices
        .branches()
        .iter()
        .position(|b| b == &branch)
        .unwrap();
    let input = choices.input(i).unwrap();
    let Transition::Settled { state, .. } = graph
        .transition(
            &Key::node(node),
            &choices,
            i,
            input.as_ref(),
            limits(),
            stats,
        )
        .unwrap()
    else {
        panic!("expected settled transition");
    };
    let mut next = Node {
        state,
        card_baseline: node.card_baseline.clone(),
        potion_budget: node.potion_budget.clone(),
        trace: node.trace.clone(),
        input_count: node.input_count,
    };
    if let Some(input) = input {
        let mut expenditures = next.expenditures();
        expenditures.record(&input);
        next.trace = Some(Arc::new(Trace {
            previous: next.trace,
            input,
            expenditures,
        }));
        next.input_count += 1;
    }
    next
}

#[test]
fn potion_work_separates_engine_calls_cache_hits_and_spent_prefixes() {
    use sts_core::content::potions::{Potion, PotionId};
    let mut root = source().state.position().clone();
    root.combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 1))];
    let source = Node::root(CombatSearchState::new(root).unwrap());
    let mut graph = Graph::new(128);
    let mut stats = SolverStats::default();
    let use_potion = CombatBranch::Input(ClientInput::UsePotion {
        potion_index: 0,
        target: None,
    });
    let used = execute(&mut graph, &source, use_potion.clone(), &mut stats);
    execute(&mut graph, &source, use_potion, &mut stats);
    assert_eq!(stats.potion_search.use_transitions, 2);
    assert_eq!(stats.potion_search.cached_use_transitions, 1);
    assert_eq!(stats.potion_search.after_expenditure_transitions, 0);
    let end = CombatBranch::Input(ClientInput::EndTurn);
    execute(&mut graph, &used, end.clone(), &mut stats);
    let steps = stats.potion_search.after_expenditure_engine_steps;
    assert!(steps > 0);
    execute(&mut graph, &used, end, &mut stats);
    assert_eq!(stats.potion_search.after_expenditure_transitions, 2);
    assert_eq!(stats.potion_search.after_expenditure_engine_steps, steps);
    let discard = CombatBranch::Input(ClientInput::DiscardPotion(0));
    execute(&mut graph, &source, discard.clone(), &mut stats);
    execute(&mut graph, &source, discard, &mut stats);
    assert_eq!(stats.potion_search.discard_transitions, 2);
    assert_eq!(stats.potion_search.cached_discard_transitions, 1);
}

fn kill() -> CombatBranch {
    CombatBranch::Input(ClientInput::PlayCard {
        card_index: 0,
        target: Some(1),
    })
}

fn verified(source: &Node, endpoint: &Node) -> CombatWitnessMaterializedOutcomeV1 {
    let mut replay = source.state.position().clone();
    for input in endpoint.inputs() {
        assert!(EngineCombatStepper.is_legal_action(&replay, &input));
        let step = EngineCombatStepper.apply_to_stable(&replay, input, limits());
        assert!(!step.timed_out && !step.truncated);
        replay = step.position;
    }
    assert_eq!(&replay, endpoint.state.position());
    materialize_replay_checked_combat_outcome_v1(
        source.state.position(),
        &endpoint.inputs(),
        &replay,
    )
    .unwrap()
}

fn drain(graph: &mut Graph, stats: &mut SolverStats) {
    let before = stats.result_backup_work;
    while graph.has_pending() {
        graph.backup_step(stats);
        assert!(
            stats.result_backup_work - before < 1000,
            "backup cycle did not stop"
        );
    }
}

#[test]
fn exact_cache_reuses_engine_results_but_not_rng_order_or_expenditure_changes() {
    let mut graph = Graph::with_capacity(32);
    let mut stats = SolverStats::default();
    let source = source();
    let first = execute(&mut graph, &source, kill(), &mut stats);
    let repeated = execute(&mut graph, &source, kill(), &mut stats);
    assert_eq!(first.state, repeated.state);
    assert_eq!(stats.transition_cache_hits, 1);
    let mut changed = source.state.position().clone();
    changed.combat.rng.shuffle_rng.random_long();
    // Different full positions sharing the same partial bucket remain separate.
    let changed = Node::root(CombatSearchState::new(changed).unwrap());
    assert_eq!(Key::node(&source).hash, Key::node(&changed).hash);
    execute(&mut graph, &changed, kill(), &mut stats);
    let mut reordered = source.state.position().clone();
    reordered.combat.zones.hand.swap(0, 1);
    execute(
        &mut graph,
        &Node::root(CombatSearchState::new(reordered).unwrap()),
        kill(),
        &mut stats,
    );
    let mut spent = source.clone();
    let mut expenditures = CombatInputExpendituresV1::default();
    expenditures.record(&ClientInput::DiscardPotion(0));
    spent.trace = Some(Arc::new(Trace {
        previous: None,
        input: ClientInput::DiscardPotion(0),
        expenditures,
    }));
    execute(&mut graph, &spent, kill(), &mut stats);
    assert_eq!(stats.transition_cache_hits, 1);
    assert_eq!(stats.graph_edges_recorded, 4);
    assert!(graph.returns(&Key::node(&spent)).is_empty());
}

#[test]
fn interrupted_edges_are_not_cached_and_cached_hits_respect_deadlines() {
    let mut graph = Graph::with_capacity(8);
    let mut stats = SolverStats::default();
    let source = source();
    let choices = CombatSearchSpace::default()
        .prepare_choices(&source.state)
        .unwrap();
    let i = choices
        .branches()
        .iter()
        .position(|b| b == &kill())
        .unwrap();
    let input = choices.input(i).unwrap();
    for limit in [
        CombatStepLimits {
            max_engine_steps: 1,
            deadline: None,
        },
        CombatStepLimits {
            deadline: Some(Instant::now()),
            ..limits()
        },
    ] {
        assert!(matches!(
            graph
                .transition(
                    &Key::node(&source),
                    &choices,
                    i,
                    input.as_ref(),
                    limit,
                    &mut stats
                )
                .unwrap(),
            Transition::Interrupted { .. }
        ));
        assert!(graph.edges.is_empty());
    }
    execute(&mut graph, &source, kill(), &mut stats);
    assert!(matches!(
        graph
            .transition(
                &Key::node(&source),
                &choices,
                i,
                input.as_ref(),
                CombatStepLimits {
                    deadline: Some(Instant::now()),
                    ..limits()
                },
                &mut stats
            )
            .unwrap(),
        Transition::Interrupted {
            cause: crate::TransitionInterruption::Deadline,
            engine_steps: 0
        }
    ));
    assert_eq!(stats.transition_cache_hits, 0);
}

#[test]
fn verified_returns_cross_virtual_selection_edges_and_change_upstream_order() {
    let mut position =
        super::super::tests::root(&[CardId::Strike, CardId::Defend, CardId::Defend], 6);
    position.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages: sts_core::test_support::scenario_lineages([1, 2, 3]),
        min_cards: 1,
        max_cards: 2,
        can_cancel: false,
        reason: HandSelectReason::Exhaust,
    });
    let source = Node::root(CombatSearchState::new(position).unwrap());
    let mut graph = Graph::with_capacity(32);
    let mut stats = SolverStats::default();
    let begun = execute(
        &mut graph,
        &source,
        CombatBranch::BeginSelection(0),
        &mut stats,
    );
    let one = execute(
        &mut graph,
        &begun,
        CombatBranch::SelectMember(1),
        &mut stats,
    );
    let selected = execute(&mut graph, &one, CombatBranch::SelectMember(2), &mut stats);
    let reverse_one = execute(
        &mut graph,
        &begun,
        CombatBranch::SelectMember(2),
        &mut stats,
    );
    let reverse = execute(
        &mut graph,
        &reverse_one,
        CombatBranch::SelectMember(1),
        &mut stats,
    );
    assert_ne!(selected.state, reverse.state);
    let completed = execute(
        &mut graph,
        &selected,
        CombatBranch::FinishSelection,
        &mut stats,
    );
    let endpoint = execute(&mut graph, &completed, kill(), &mut stats);
    // Mere observed wins do not attach a return to any source.
    assert!(graph.returns(&Key::node(&source)).is_empty());
    assert_eq!(stats.result_backup_work, 0);
    graph.verified_win(
        vec![Key::node(&endpoint)],
        verified(&source, &endpoint),
        endpoint.expenditures(),
        &mut stats,
    );
    drain(&mut graph, &mut stats);
    assert_eq!(graph.returns(&Key::node(&source)).len(), 1);
    assert_eq!(graph.returns(&Key::node(&selected)).len(), 1);
    assert!(graph.returns(&Key::node(&reverse)).is_empty());
    let mut pass = Pass::new(begun.clone(), 4, true);
    let mut expansion = pass
        .prepare_node(
            begun,
            &CombatSearchSpace::default(),
            &SolverConfig::default(),
            &mut stats,
        )
        .unwrap();
    let desired = expansion
        .choices
        .branches()
        .iter()
        .position(|b| *b == CombatBranch::SelectMember(1))
        .unwrap();
    assert_ne!(expansion.branches[0], desired);
    graph.reorder(&mut expansion, true, Some(0), &mut stats);
    assert_eq!(expansion.branches[0], desired);
    assert_eq!(stats.backed_branch_reorders, 1);
    assert_eq!(stats.work(), stats.result_backup_work);
}

#[test]
fn bounded_eviction_cannot_lend_a_stale_edge_or_loop_after_return_eviction() {
    let source = source();
    let mut graph = Graph::with_capacity(2);
    let mut stats = SolverStats::default();
    let first = execute(&mut graph, &source, kill(), &mut stats);
    let endpoint = execute(&mut graph, &first, kill(), &mut stats);
    let result = Return {
        outcome: Arc::new(verified(&source, &endpoint)),
        final_expenditures: endpoint.expenditures(),
    };
    let stale = *graph.edge_order.front().unwrap();
    graph.enqueue(Backup::Edge(stale, result.clone()), &mut stats);
    execute(
        &mut graph,
        &source,
        CombatBranch::Input(ClientInput::EndTurn),
        &mut stats,
    );
    graph.backup_step(&mut stats);
    assert!(graph.returns(&Key::node(&source)).is_empty());
    assert_eq!(stats.graph_edges_evicted, 1);
    assert!(graph.edge(&Key::node(&source), &kill()).is_none());
    assert_eq!(graph.edges.len(), 2);
    // Adversarial bookkeeping cycle: even with fewer return slots than vertices,
    // each retained edge remembers that it already forwarded this exact result.
    graph = Graph::with_capacity(2);
    graph.record_edge(Key::node(&source), kill(), Key::node(&first), &mut stats);
    graph.record_edge(Key::node(&first), kill(), Key::node(&source), &mut stats);
    graph.enqueue(
        Backup::Seed(Key::node(&endpoint), result.clone()),
        &mut stats,
    );
    graph.enqueue(Backup::Seed(Key::node(&source), result), &mut stats);
    drain(&mut graph, &mut stats);
    assert_eq!(graph.records.len(), 2);
    assert!(stats.result_backup_work < 20);
}

#[test]
fn potion_suffixes_do_not_guide_card_only_search_and_resource_fronts_stay_whole() {
    let source = source();
    let mut graph = Graph::with_capacity(8);
    let mut stats = SolverStats::default();
    let first = execute(&mut graph, &source, kill(), &mut stats);
    let endpoint = execute(&mut graph, &first, kill(), &mut stats);
    let outcome = verified(&source, &endpoint);
    let mut with_potion = outcome.clone();
    with_potion.final_hp += 5;
    with_potion.starting_potion_expenditure_slots = 1;
    with_potion.total_potion_expenditures = 1;
    let mut spent = CombatInputExpendituresV1::default();
    spent.record(&ClientInput::UsePotion {
        potion_index: 0,
        target: None,
    });
    let no_more = Return {
        outcome: Arc::new(outcome),
        final_expenditures: Default::default(),
    };
    let potion = Return {
        outcome: Arc::new(with_potion),
        final_expenditures: spent,
    };
    assert!(!potion.permitted(&Key::node(&source), true, None));
    assert!(potion.permitted(&Key::node(&source), false, Some(1)));
    assert!(!potion.permitted(&Key::node(&source), false, Some(0)));
    graph.enqueue(Backup::Seed(Key::node(&first), no_more), &mut stats);
    graph.enqueue(Backup::Seed(Key::node(&first), potion), &mut stats);
    drain(&mut graph, &mut stats);
    let resources = graph.returns(&Key::node(&first));
    assert_eq!(resources.len(), 2);
    assert!(resources
        .iter()
        .any(|r| r.outcome.total_potion_expenditures == 0));
    assert!(resources
        .iter()
        .any(|r| r.outcome.total_potion_expenditures == 1));
    let choices = CombatSearchSpace::default()
        .prepare_choices(&source.state)
        .unwrap();
    let branches = (0..choices.branches().len()).collect::<Vec<_>>();
    let mut expansion = Expansion {
        graph_key: Key::node(&source),
        node: source.clone(),
        choices,
        branches,
        index: 0,
        tactical_plan: None,
        backed_revision: 0,
    };
    graph.reorder(&mut expansion, true, None, &mut stats);
    assert_eq!(expansion.choices.branches()[expansion.branches[0]], kill());
    // Explicit history is part of the key even at an identical simulator state.
    assert!(graph.returns(&Key::new(source.state, spent)).is_empty());
}

#[test]
fn solver_replays_before_backing_up_and_charges_incremental_backups() {
    let mut solver = CombatSolver::new(
        source().state.position().clone(),
        SolverConfig {
            initial_width: 2,
            max_width: 4,
            refine_card_order: false,
            refine_potion_plans: false,
            ..Default::default()
        },
    )
    .unwrap();
    while solver.stats.observed_wins == 0 {
        solver.advance(1, Duration::from_secs(5)).unwrap();
    }
    assert_eq!(solver.stats.replayed_wins, 0);
    assert!(solver.graph.records.values().all(|r| r.returns.is_empty()));
    assert!(solver
        .graph
        .pending
        .iter()
        .all(|b| matches!(b, Backup::BoundarySeed(..) | Backup::BoundaryEdge(..))));
    while solver.stats.replayed_wins == 0 {
        solver.advance(1, Duration::from_secs(5)).unwrap();
    }
    assert!(solver.graph.has_pending());
    let before = solver.stats.work();
    let backups_before = solver.stats.result_backup_work;
    solver.advance(1, Duration::from_secs(5)).unwrap();
    assert_eq!(solver.stats.work(), before + 1);
    assert_eq!(solver.stats.result_backup_work, backups_before + 1);
    solver.advance(1000, Duration::from_secs(5)).unwrap();
    assert!(solver.stats.transition_cache_hits > 0);
    assert!(solver.stats.backed_return_updates > 0);
    assert!(solver.stats.engine_steps > 0);
    assert!(!solver.plans.is_empty());
}

#[test]
fn failed_final_replay_cannot_publish_or_seed_resource_returns() {
    let mut solver = CombatSolver::new(
        super::super::tests::root(&[CardId::Strike], 6),
        SolverConfig::default(),
    )
    .unwrap();
    while solver.replay.is_none() {
        solver.advance(1, Duration::from_secs(5)).unwrap();
    }
    solver
        .replay
        .as_mut()
        .unwrap()
        .expected
        .combat
        .entities
        .player
        .gold += 1;
    assert!(solver
        .advance(1, Duration::from_secs(5))
        .unwrap_err()
        .contains("differs from searched"));
    assert!(solver.plans.is_empty());
    assert!(solver.graph.records.values().all(|r| r.returns.is_empty()));
    assert!(solver
        .graph
        .pending
        .iter()
        .all(|b| matches!(b, Backup::BoundarySeed(..) | Backup::BoundaryEdge(..))));
}

#[test]
fn completed_turn_consequences_revalue_preparation_before_any_verified_win() {
    let position = super::super::tests::root(&[CardId::Strike, CardId::Defend], 100);
    let source = Node::root(CombatSearchState::new(position).unwrap());
    let mut graph = Graph::with_capacity(32);
    let mut stats = SolverStats::default();
    let attack = execute(&mut graph, &source, kill(), &mut stats);
    let prepare = execute(
        &mut graph,
        &source,
        CombatBranch::Input(ClientInput::PlayCard {
            card_index: 1,
            target: None,
        }),
        &mut stats,
    );
    assert!(retention::scores(&attack)[0] > retention::scores(&prepare)[0]);
    let attack_end = execute(
        &mut graph,
        &attack,
        CombatBranch::Input(ClientInput::EndTurn),
        &mut stats,
    );
    let prepare_end = execute(
        &mut graph,
        &prepare,
        CombatBranch::Input(ClientInput::EndTurn),
        &mut stats,
    );
    assert_eq!(
        prepare_end
            .state
            .position()
            .combat
            .entities
            .player
            .current_hp,
        attack_end
            .state
            .position()
            .combat
            .entities
            .player
            .current_hp
            + 5
    );
    for endpoint in [&attack_end, &prepare_end] {
        let replay =
            crate::shuffle_order::replay(source.state.position(), &endpoint.inputs()).unwrap();
        assert_eq!(replay.last(), Some(endpoint.state.position()));
        graph.observe_boundary(endpoint, 0, None, &mut stats);
    }
    drain(&mut graph, &mut stats);
    let nodes = vec![attack.clone(), prepare.clone()];
    let keys = nodes.iter().map(Node::index_key).collect::<Vec<_>>();
    let mut order = vec![0, 1];
    graph
        .retention_guide(true, Some(0))
        .order_by_boundaries(&nodes, &keys, &mut order, &mut stats);
    assert_eq!(
        order,
        [1, 0],
        "actual protected HP repays the initially lower damage score"
    );
    assert!(
        graph.records.values().all(|r| r.returns.is_empty()),
        "no terminal witness was invented"
    );
    assert_eq!(stats.work(), stats.boundary_backup_work);
    assert!(stats.boundary_retention_reorders > 0);
    // A new incoming exact edge reopens already observed boundary evidence.
    let mut late_graph = Graph::with_capacity(32);
    late_graph.observe_boundary(&prepare_end, 0, None, &mut stats);
    drain(&mut late_graph, &mut stats);
    execute(
        &mut late_graph,
        &prepare,
        CombatBranch::Input(ClientInput::EndTurn),
        &mut stats,
    );
    drain(&mut late_graph, &mut stats);
    assert!(!late_graph.boundaries(&Key::node(&prepare)).is_empty());
}

#[test]
fn boundary_ranking_keeps_unobserved_slots_and_never_compares_different_turns() {
    let source = source();
    let mut graph = Graph::with_capacity(32);
    let mut stats = SolverStats::default();
    let strike = execute(&mut graph, &source, kill(), &mut stats);
    let defend = execute(
        &mut graph,
        &source,
        CombatBranch::Input(ClientInput::PlayCard {
            card_index: 2,
            target: None,
        }),
        &mut stats,
    );
    let nodes = vec![strike.clone(), source.clone(), defend.clone()];
    let keys = nodes.iter().map(Node::index_key).collect::<Vec<_>>();
    let strike_end = execute(
        &mut graph,
        &strike,
        CombatBranch::Input(ClientInput::EndTurn),
        &mut stats,
    );
    let defend_end = execute(
        &mut graph,
        &defend,
        CombatBranch::Input(ClientInput::EndTurn),
        &mut stats,
    );
    for endpoint in [&strike_end, &defend_end] {
        graph.observe_boundary(endpoint, 0, None, &mut stats);
    }
    drain(&mut graph, &mut stats);
    // The common parent also has evidence; use an exact RNG-distinct state as
    // the unobserved competitor, never a hand/name based state alias.
    let mut unseen = source.state.position().clone();
    unseen.combat.rng.shuffle_rng.random_long();
    let unseen = Node::root(CombatSearchState::new(unseen).unwrap());
    let nodes = vec![nodes[0].clone(), unseen.clone(), nodes[2].clone()];
    let keys = [keys[0], unseen.index_key(), keys[2]];
    let mut order = vec![0, 1, 2];
    graph
        .retention_guide(true, Some(0))
        .order_by_boundaries(&nodes, &keys, &mut order, &mut stats);
    assert_eq!(order, [2, 1, 0]);
    // Remove the common horizon from one candidate. Its deeper evidence cannot
    // outrank an earlier endpoint, even if its numerical value is much higher.
    let id = graph.record_id(&Key::node(&defend)).unwrap();
    let boundary = &mut graph.records.get_mut(&id).unwrap().boundaries[0];
    let mut later = boundary.endpoint.state.position().clone();
    later.combat.turn.turn_count += 1;
    boundary.endpoint = Key::new(CombatSearchState::new(later).unwrap(), Default::default());
    boundary.ranks[4].0 += 10000;
    let mut order = vec![0, 1, 2];
    graph
        .retention_guide(true, Some(0))
        .order_by_boundaries(&nodes, &keys, &mut order, &mut stats);
    assert_eq!(order, [0, 1, 2]);
}

#[test]
fn boundary_evidence_is_bounded_and_potion_constraints_survive_compression() {
    let source = source();
    let key = Key::node(&source);
    let mut pool = vec![];
    for value in 0..40 {
        let mut endpoint = source.state.position().clone();
        endpoint.combat.turn.turn_count += 1;
        endpoint.combat.entities.player.gold = value;
        let mut spent = CombatInputExpendituresV1::default();
        if value > 0 {
            spent.record(&ClientInput::DiscardPotion(0));
        }
        Graph::retain_boundaries(
            &mut pool,
            &key,
            Boundary {
                endpoint: Key::new(CombatSearchState::new(endpoint).unwrap(), spent),
                trace: None,
                input_count: 0,
                ranks: [(i64::from(value), 0, 0); retention::SCORE_LANES],
                total_potion_expenditures: usize::from(value > 0),
            },
        );
    }
    assert!(pool.len() <= 2 * retention::SCORE_LANES);
    assert_eq!(
        pool.iter()
            .filter(|b| b.permitted(&key, true, Some(0)))
            .count(),
        1
    );
    assert!(pool.iter().any(|b| b.ranks[0].0 == 39));
    // Passive loss is compatible with card-only, but not a no-expenditure cap.
    let mut passive = pool
        .iter()
        .find(|b| b.permitted(&key, true, Some(0)))
        .unwrap()
        .clone();
    passive.total_potion_expenditures = 1;
    assert!(passive.permitted(&key, true, None));
    assert!(!passive.permitted(&key, true, Some(0)));
    let mut graph = Graph::with_capacity(2);
    let mut stats = SolverStats::default();
    graph.record_edge(key.clone(), kill(), key.clone(), &mut stats);
    graph.enqueue(
        Backup::BoundarySeed(key.clone(), passive.clone()),
        &mut stats,
    );
    drain(&mut graph, &mut stats);
    assert!(
        stats.boundary_backup_work < 10,
        "self-loops stop after evidence repeats"
    );
    let mut far = passive.clone();
    let mut position = far.endpoint.state.position().clone();
    position.combat.turn.turn_count = source.state.position().combat.turn.turn_count + 3;
    far.endpoint = Key::new(
        CombatSearchState::new(position).unwrap(),
        Default::default(),
    );
    graph.backup_boundary(key.clone(), far, &mut stats);
    assert!(graph
        .boundaries(&key)
        .iter()
        .all(|b| b.turn() <= source.state.position().combat.turn.turn_count + 2));
}

#[test]
fn observed_damage_health_tradeoffs_keep_the_original_objective_orders() {
    let position = super::super::tests::root(&[CardId::HeavyBlade, CardId::Defend], 100);
    let source = Node::root(CombatSearchState::new(position).unwrap());
    let mut graph = Graph::with_capacity(32);
    let mut stats = SolverStats::default();
    let attack = execute(&mut graph, &source, kill(), &mut stats);
    let defend = execute(
        &mut graph,
        &source,
        CombatBranch::Input(ClientInput::PlayCard {
            card_index: 1,
            target: None,
        }),
        &mut stats,
    );
    let mut endpoints = vec![];
    for node in [&attack, &defend] {
        let endpoint = execute(
            &mut graph,
            node,
            CombatBranch::Input(ClientInput::EndTurn),
            &mut stats,
        );
        assert_eq!(
            crate::shuffle_order::replay(source.state.position(), &endpoint.inputs())
                .unwrap()
                .last(),
            Some(endpoint.state.position())
        );
        graph.observe_boundary(&endpoint, 0, None, &mut stats);
        endpoints.push(endpoint);
    }
    assert!(
        retention::boundary_ranks(&endpoints[0], None)[0]
            > retention::boundary_ranks(&endpoints[1], None)[0]
    );
    assert!(
        retention::boundary_ranks(&endpoints[0], None)[4]
            < retention::boundary_ranks(&endpoints[1], None)[4]
    );
    drain(&mut graph, &mut stats);
    let nodes = [attack, defend];
    let keys = nodes.iter().map(Node::index_key).collect::<Vec<_>>();
    for original in [vec![0, 1], vec![1, 0]] {
        let mut order = original.clone();
        graph
            .retention_guide(true, Some(0))
            .order_by_boundaries(&nodes, &keys, &mut order, &mut stats);
        assert_eq!(
            order, original,
            "a witnessed HP/damage tradeoff is not an all-objective improvement"
        );
    }
    assert!(
        stats.boundary_backup_operations <= retention::SCORE_LANES * stats.boundary_backup_work
    );
}

#[test]
fn passive_potion_loss_is_not_an_explicit_suffix_but_still_obeys_resource_limit() {
    let source = source();
    let mut graph = Graph::with_capacity(8);
    let mut stats = SolverStats::default();
    let first = execute(&mut graph, &source, kill(), &mut stats);
    let endpoint = execute(&mut graph, &first, kill(), &mut stats);
    let mut outcome = verified(&source, &endpoint);
    outcome.total_potion_expenditures = 1;
    outcome.starting_potion_expenditure_slots = 1;
    let passive = Return {
        outcome: Arc::new(outcome),
        final_expenditures: Default::default(),
    };
    assert!(passive.permitted(&Key::node(&source), true, None));
    assert!(!passive.permitted(&Key::node(&source), true, Some(0)));
    let mut explicit = passive.clone();
    explicit
        .final_expenditures
        .record(&ClientInput::DiscardPotion(0));
    assert!(!explicit.covers(&passive));
    assert!(passive.covers(&explicit));
}

#[test]
fn replayed_returns_nominate_distinct_fronts_without_merging_exact_candidates() {
    let mut position =
        super::super::tests::root(&[CardId::Strike, CardId::Strike, CardId::Bash], 17);
    position.combat.zones.draw_pile = vec![sts_core::runtime::combat::CombatCard::for_scenario(
        CardId::Strike,
        101,
    )]
    .into();
    let source = Node::root(CombatSearchState::new(position).unwrap());
    let mut graph = Graph::with_capacity(32);
    let mut stats = SolverStats::default();
    let bad_first = execute(&mut graph, &source, kill(), &mut stats);
    let damage_leader = execute(&mut graph, &bad_first, kill(), &mut stats);
    let good = execute(
        &mut graph,
        &source,
        CombatBranch::Input(ClientInput::PlayCard {
            card_index: 2,
            target: Some(1),
        }),
        &mut stats,
    );
    let endpoint = execute(&mut graph, &good, kill(), &mut stats);
    let unbacked = execute(
        &mut graph,
        &source,
        CombatBranch::Input(ClientInput::PlayCard {
            card_index: 1,
            target: Some(1),
        }),
        &mut stats,
    );
    assert!(retention::scores(&damage_leader)[0] > retention::scores(&good)[0]);
    let pool = vec![damage_leader.clone(), good.clone()];
    let baseline = retention::partition(pool.clone(), 1, &mut stats, true, false, None, None);
    assert_eq!(baseline.0[0].state, damage_leader.state);
    let before_verification = retention::partition(
        pool.clone(),
        1,
        &mut stats,
        true,
        false,
        None,
        Some(graph.retention_guide(true, Some(0))),
    );
    assert_eq!(before_verification.0[0].state, baseline.0[0].state);
    graph.verified_win(
        vec![Key::node(&endpoint)],
        verified(&source, &endpoint),
        endpoint.expenditures(),
        &mut stats,
    );
    drain(&mut graph, &mut stats);
    // A verified return can still challenge the immediate score.
    let single = retention::partition(
        pool.clone(),
        1,
        &mut stats,
        true,
        false,
        None,
        Some(graph.retention_guide(true, Some(0))),
    );
    assert_eq!(single.0[0].state, good.state);
    assert_eq!(stats.backed_retention_selected, 1);
    let after_response = execute(
        &mut graph,
        &damage_leader,
        CombatBranch::Input(ClientInput::EndTurn),
        &mut stats,
    );
    let slow_endpoint = execute(&mut graph, &after_response, kill(), &mut stats);
    let slow_outcome = verified(&source, &slow_endpoint);
    assert!(verified(&source, &endpoint).strictly_dominates(&slow_outcome));
    graph.verified_win(
        vec![Key::node(&slow_endpoint)],
        slow_outcome,
        slow_endpoint.expenditures(),
        &mut stats,
    );
    drain(&mut graph, &mut stats);
    let result = retention::partition(
        pool,
        1,
        &mut stats,
        true,
        false,
        None,
        Some(graph.retention_guide(true, Some(0))),
    );
    assert_eq!(result.0[0].state, good.state);
    assert_eq!(result.1[0].state, damage_leader.state);
    // The root and Bash prefix have the same verified resource front. They get
    // one nomination; the covered slow return gets no extra nomination. None
    // of these exact states are merged or removed from ordinary score lanes.
    let candidates = [
        damage_leader.clone(),
        unbacked,
        good.clone(),
        source.clone(),
    ];
    let keys = candidates.iter().map(Node::index_key).collect::<Vec<_>>();
    let suppressed = stats.backed_retention_suppressed_nominations;
    assert_eq!(
        graph
            .retention_guide(true, Some(0))
            .nominations(&candidates, &keys, &mut stats),
        vec![2]
    );
    assert_eq!(
        stats.backed_retention_suppressed_nominations - suppressed,
        2
    );
    let (kept, omitted) = retention::partition(
        candidates.to_vec(),
        2,
        &mut stats,
        true,
        false,
        None,
        Some(graph.retention_guide(true, Some(0))),
    );
    assert_eq!(kept.len() + omitted.len(), candidates.len());
    for node in &candidates {
        assert!(kept.iter().chain(&omitted).any(|n| n.state == node.state));
    }
    // Every candidate remains represented in kept or omitted, and the actual
    // winning line still replays from the unchanged original combat root.
    assert_eq!(result.0.len() + result.1.len(), 2);
    assert_eq!(
        verified(&source, &endpoint).final_hp,
        source.state.position().combat.entities.player.current_hp
    );
}

#[test]
fn retention_evidence_obeys_the_same_potion_and_exact_history_boundaries() {
    use sts_core::content::potions::{Potion, PotionId};
    let mut position =
        super::super::tests::root(&[CardId::Strike, CardId::Strike, CardId::Bash], 17);
    position.combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 1))];
    let source = Node::root(CombatSearchState::new(position).unwrap());
    let mut graph = Graph::with_capacity(32);
    let mut stats = SolverStats::default();
    let first = execute(&mut graph, &source, kill(), &mut stats);
    let damage_leader = execute(&mut graph, &first, kill(), &mut stats);
    let good = execute(
        &mut graph,
        &source,
        CombatBranch::Input(ClientInput::PlayCard {
            card_index: 2,
            target: Some(1),
        }),
        &mut stats,
    );
    let endpoint = execute(
        &mut graph,
        &good,
        CombatBranch::Input(ClientInput::UsePotion {
            potion_index: 0,
            target: Some(1),
        }),
        &mut stats,
    );
    graph.verified_win(
        vec![Key::node(&endpoint)],
        verified(&source, &endpoint),
        endpoint.expenditures(),
        &mut stats,
    );
    drain(&mut graph, &mut stats);
    let after_response = execute(
        &mut graph,
        &damage_leader,
        CombatBranch::Input(ClientInput::EndTurn),
        &mut stats,
    );
    let reference_endpoint = execute(
        &mut graph,
        &after_response,
        CombatBranch::Input(ClientInput::UsePotion {
            potion_index: 0,
            target: Some(1),
        }),
        &mut stats,
    );
    let poor = verified(&source, &reference_endpoint);
    assert!(verified(&source, &endpoint).strictly_dominates(&poor));
    graph.verified_win(
        vec![Key::node(&reference_endpoint)],
        poor,
        reference_endpoint.expenditures(),
        &mut stats,
    );
    drain(&mut graph, &mut stats);
    let pool = vec![damage_leader.clone(), good.clone()];
    let pure = retention::partition(
        pool.clone(),
        1,
        &mut stats,
        true,
        false,
        None,
        Some(graph.retention_guide(true, None)),
    );
    assert_eq!(pure.0[0].state, damage_leader.state);
    let limited = retention::partition(
        pool.clone(),
        1,
        &mut stats,
        true,
        false,
        None,
        Some(graph.retention_guide(false, Some(0))),
    );
    assert_eq!(limited.0[0].state, damage_leader.state);
    let allowed = retention::partition(
        pool,
        1,
        &mut stats,
        true,
        false,
        None,
        Some(graph.retention_guide(false, Some(1))),
    );
    assert_eq!(allowed.0[0].state, good.state);
    // The omission diagnostic isolates guidance from earlier pool changes.
    // Watching the reference still cannot insert it into this new solver.
    let mut watched =
        CombatSolver::new(source.state.position().clone(), SolverConfig::default()).unwrap();
    watched
        .audit_reference(
            &reference_endpoint.inputs(),
            reference_endpoint.state.position(),
        )
        .unwrap();
    watched.beams[0].passes[0]
        .audit
        .as_mut()
        .unwrap()
        .partition(
            &[damage_leader.clone(), good.clone()],
            &allowed.0,
            1,
            "next_action",
            stats.work(),
            None,
            true,
            false,
            Some(graph.retention_guide(false, Some(1))),
        );
    let audit = watched.audit().unwrap();
    let omission = audit.passes[0].steps[2].first_omission.as_ref().unwrap();
    assert!(omission.kept_without_backed_guidance);
    assert_eq!(omission.backed_returns.len(), 1);
    assert_eq!(omission.backed_kept.len(), 1);
    assert_eq!(omission.backed_kept[0].inputs, good.inputs());
    assert_eq!(omission.backed_kept[0].backed_returns.len(), 1);
    assert_eq!(watched.stats.work(), 0);
    assert!(watched.plans.is_empty());
    let mut changed = good.clone();
    let mut expenditures = changed.expenditures();
    expenditures.record(&ClientInput::DiscardPotion(0));
    changed.trace = Some(Arc::new(Trace {
        previous: changed.trace,
        input: ClientInput::DiscardPotion(0),
        expenditures,
    }));
    let candidates = [damage_leader, changed];
    let keys = candidates.iter().map(Node::index_key).collect::<Vec<_>>();
    assert_eq!(
        graph
            .retention_guide(false, None)
            .nominations(&candidates, &keys, &mut stats),
        vec![0],
        "the changed explicit history cannot borrow the better return"
    );
}

#[test]
fn nomination_portfolios_keep_actual_hp_potion_tradeoffs_instead_of_synthesizing_a_win() {
    use sts_core::content::potions::{Potion, PotionId};
    use sts_core::runtime::combat::CombatCard;
    let mut position =
        super::super::tests::root(&[CardId::Strike, CardId::Strike, CardId::Bash], 17);
    position.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    position.combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 1))];
    let source = Node::root(CombatSearchState::new(position).unwrap());
    let mut graph = Graph::with_capacity(32);
    let mut stats = SolverStats::default();
    let first = execute(&mut graph, &source, kill(), &mut stats);
    let reserve = execute(&mut graph, &first, kill(), &mut stats);
    let response = execute(
        &mut graph,
        &reserve,
        CombatBranch::Input(ClientInput::EndTurn),
        &mut stats,
    );
    let reserve_endpoint = execute(&mut graph, &response, kill(), &mut stats);
    let health = execute(
        &mut graph,
        &source,
        CombatBranch::Input(ClientInput::PlayCard {
            card_index: 2,
            target: Some(1),
        }),
        &mut stats,
    );
    let health_endpoint = execute(
        &mut graph,
        &health,
        CombatBranch::Input(ClientInput::UsePotion {
            potion_index: 0,
            target: Some(1),
        }),
        &mut stats,
    );
    let reserve_outcome = verified(&source, &reserve_endpoint);
    let health_outcome = verified(&source, &health_endpoint);
    assert!(health_outcome.final_hp > reserve_outcome.final_hp);
    assert_eq!(reserve_outcome.total_potion_expenditures, 0);
    assert_eq!(health_outcome.total_potion_expenditures, 1);
    assert!(!health_outcome.strictly_dominates(&reserve_outcome));
    assert!(!reserve_outcome.strictly_dominates(&health_outcome));
    for (endpoint, outcome) in [
        (&reserve_endpoint, reserve_outcome.clone()),
        (&health_endpoint, health_outcome.clone()),
    ] {
        graph.verified_win(
            vec![Key::node(endpoint)],
            outcome,
            endpoint.expenditures(),
            &mut stats,
        );
    }
    drain(&mut graph, &mut stats);
    let candidates = [reserve, health, source];
    let keys = candidates.iter().map(Node::index_key).collect::<Vec<_>>();
    assert_eq!(
        graph
            .retention_guide(false, None)
            .nominations(&candidates[..2], &keys[..2], &mut stats),
        vec![0, 1],
        "both incomparable resource promises receive a nomination"
    );
    assert_eq!(
        graph
            .retention_guide(false, None)
            .nominations(&candidates, &keys, &mut stats),
        vec![2],
        "one actual portfolio covers both known promises"
    );
    let outcomes = graph.retention_guide(false, None).outcomes(&candidates[2]);
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.contains(&reserve_outcome));
    assert!(outcomes.contains(&health_outcome));
    assert!(!outcomes
        .iter()
        .any(|o| { o.final_hp == health_outcome.final_hp && o.total_potion_expenditures == 0 }));
}
