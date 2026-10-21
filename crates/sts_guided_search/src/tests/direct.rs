use super::*;

fn prefer_last(session: &mut SearchSession<Graph>, source: NodeRef) {
    let mut weights = vec![0.0; session.view(source).unwrap().actions.len()];
    *weights.last_mut().unwrap() = 1.0;
    session
        .submit_guidance(
            source,
            Guidance {
                weights: Some(weights),
                value: None,
            },
        )
        .unwrap();
}

#[test]
fn direct_uses_raw_policy_and_does_not_try_siblings_after_a_loss() {
    let graph = Graph::new(
        vec![vec![1, 2], vec![], vec![]],
        vec![None, Some(true), Some(false)],
    );
    let mut session = SearchSession::new(
        graph.clone(),
        0,
        SearchConfig {
            uniform_fraction: 1.0,
            ..Default::default()
        },
        budget(),
    )
    .unwrap();
    let root = session.root();
    assert_eq!(
        session.follow_guidance(root),
        Err(SearchError::InvalidGuidance)
    );
    prefer_last(&mut session, root);
    let outcome = session.follow_guidance(root).unwrap();
    assert!(matches!(outcome, StepStatus::Terminal { won: false, .. }));
    assert_eq!(*graph.calls.borrow(), [(0, 2)]);
    assert!(session.observed_wins().is_empty());
    let stats = session.stats();
    assert_eq!(session.follow_guidance(root).unwrap(), outcome);
    assert_eq!(session.stats(), stats); // Reading the same successor is not new evidence.
    assert_eq!(*graph.calls.borrow(), [(0, 2)]);
    assert_eq!(finish_graph(&mut session, None), StopReason::NoOpenWork);
    assert_eq!(session.observed_wins().len(), 1); // Alternatives were retained, not pruned.
}

#[test]
fn direct_reuses_pending_successors_and_rejects_foreign_sources() {
    let graph = Graph::new(
        vec![vec![1, 2], vec![3], vec![3], vec![]],
        vec![None, None, None, Some(true)],
    );
    let mut a = SearchSession::new(graph.clone(), 0, SearchConfig::default(), budget()).unwrap();
    let b = SearchSession::new(graph.clone(), 0, SearchConfig::default(), budget()).unwrap();
    let root = a.root();
    prefer_last(&mut a, root);
    assert_eq!(a.follow_guidance(b.root()), Err(SearchError::InvalidSource));
    let StepStatus::NeedsGuidance(next) = a.follow_guidance(root).unwrap() else {
        panic!()
    };
    assert_eq!(a.follow_guidance(next), Err(SearchError::InvalidGuidance));
    let stats = a.stats();
    assert_eq!(
        a.follow_guidance(root).unwrap(),
        StepStatus::NeedsGuidance(next)
    );
    assert_eq!(a.stats(), stats);
    prefer_last(&mut a, next);
    assert_eq!(a.follow_guidance(root), Ok(StepStatus::Guided(next)));
    assert!(matches!(
        a.follow_guidance(next).unwrap(),
        StepStatus::Terminal { won: true, .. }
    ));
    assert_eq!(*graph.calls.borrow(), [(0, 2), (2, 3)]);
}

#[test]
fn direct_cuts_and_budgets_do_not_become_terminal_samples() {
    for limits in [
        SearchBudget {
            max_depth: 0,
            ..budget()
        },
        SearchBudget {
            max_services: 0,
            ..budget()
        },
        SearchBudget {
            max_nodes: 1,
            ..budget()
        },
        SearchBudget {
            max_engine_steps: 1,
            ..budget()
        },
        SearchBudget {
            deadline: Some(Instant::now() - Duration::from_secs(1)),
            ..budget()
        },
    ] {
        let graph = Graph::new(vec![vec![1], vec![]], vec![None, Some(true)]);
        let mut session =
            SearchSession::new(graph.clone(), 0, SearchConfig::default(), limits).unwrap();
        let root = session.root();
        prefer_last(&mut session, root);
        assert!(matches!(
            session.follow_guidance(root).unwrap(),
            StepStatus::Progress | StepStatus::Stopped(_)
        ));
        assert!(graph.calls.borrow().is_empty());
        assert_eq!(session.stats().terminal_observations, 0);
    }
    for interrupted in [None, Some(TransitionInterruption::EngineStepLimit)] {
        let mut graph = Graph::new(vec![vec![0, 1], vec![]], vec![None, Some(true)]);
        graph.interrupted = interrupted;
        let mut session =
            SearchSession::new(graph.clone(), 0, SearchConfig::default(), budget()).unwrap();
        let root = session.root();
        session
            .submit_guidance(
                root,
                Guidance {
                    weights: Some(vec![1.0, 0.0]),
                    value: None,
                },
            )
            .unwrap();
        assert_eq!(session.follow_guidance(root).unwrap(), StepStatus::Progress);
        assert_eq!(session.stats().coverage_gaps, 1);
        assert_eq!(session.stats().terminal_observations, 0);
        assert_eq!(*graph.calls.borrow(), [(0, 0)]);
    }
}

#[test]
fn direct_ties_use_choice_order_and_in_flight_guidance_cannot_execute() {
    let graph = Graph::new(
        vec![vec![1, 2], vec![], vec![]],
        vec![None, Some(false), Some(true)],
    );
    let mut session = SearchSession::new(
        graph.clone(),
        0,
        SearchConfig::default(),
        SearchBudget {
            guidance: GuidanceBudget {
                max_calls: 1,
                max_input_tokens: 1,
                max_output_tokens: 1,
            },
            ..budget()
        },
    )
    .unwrap();
    let root = session.root();
    let ticket = session
        .begin_guidance_batch(
            &[root],
            TokenReservation {
                input: 1,
                output: 1,
            },
        )
        .unwrap();
    assert_eq!(
        session.follow_guidance(root),
        Err(SearchError::GuidanceInFlight)
    );
    session
        .complete_guidance_batch(
            ticket,
            vec![(
                root,
                Guidance {
                    weights: Some(vec![1.0, 1.0]),
                    value: None,
                },
            )],
            Some(TokenUsage {
                input: 1,
                output: 1,
            }),
        )
        .unwrap();
    assert!(matches!(
        session.follow_guidance(root).unwrap(),
        StepStatus::Terminal { won: false, .. }
    ));
    assert_eq!(*graph.calls.borrow(), [(0, 1)]);
    assert_eq!(session.guidance_costs().calls, 1);
}

#[test]
fn direct_cannot_overfill_the_shared_pending_frontier() {
    let graph = Graph::new(
        vec![vec![1, 2, 3], vec![4], vec![4], vec![4], vec![]],
        vec![None, None, None, None, Some(true)],
    );
    let mut session = SearchSession::new(
        graph.clone(),
        0,
        SearchConfig {
            max_pending_guidance: 2,
            ..Default::default()
        },
        budget(),
    )
    .unwrap();
    session
        .submit_guidance(session.root(), Guidance::default())
        .unwrap();
    let sources = session.gather_guidance().unwrap().sources;
    assert_eq!(sources.len(), 2);
    prefer_last(&mut session, sources[0]);
    assert_eq!(session.gather_guidance().unwrap().sources.len(), 2);
    let stats = session.stats();
    assert_eq!(
        session.follow_guidance(sources[0]),
        Err(SearchError::GuidanceCapacity)
    );
    assert_eq!(session.stats(), stats);
    assert_eq!(*graph.calls.borrow(), [(0, 1), (0, 2), (0, 3)]);
}

#[test]
fn reading_a_materialized_terminal_does_not_charge_an_exhausted_budget() {
    let graph = Graph::new(vec![vec![1], vec![]], vec![None, Some(true)]);
    let mut session = SearchSession::new(
        graph.clone(),
        0,
        SearchConfig::default(),
        SearchBudget {
            max_services: 1,
            ..budget()
        },
    )
    .unwrap();
    let root = session.root();
    prefer_last(&mut session, root);
    let outcome = session.follow_guidance(root).unwrap();
    assert!(matches!(outcome, StepStatus::Terminal { won: true, .. }));
    let stats = session.stats();
    assert_eq!(session.follow_guidance(root).unwrap(), outcome);
    assert_eq!(session.stats(), stats);
    assert_eq!(*graph.calls.borrow(), [(0, 1)]);
}
