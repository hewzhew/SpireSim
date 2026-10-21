use super::*;

fn batch_budget() -> SearchBudget {
    SearchBudget {
        guidance: GuidanceBudget {
            max_calls: 100,
            max_input_tokens: 10_000,
            max_output_tokens: 10_000,
        },
        ..budget()
    }
}

fn reservation() -> TokenReservation {
    TokenReservation {
        input: 100,
        output: 100,
    }
}

fn branches(count: usize) -> Graph {
    let mut next = vec![(1..=count).collect::<Vec<_>>()];
    next.extend((1..=count).map(|i| vec![count + i]));
    next.extend((0..count).map(|_| vec![]));
    let mut terminal = vec![None; count + 1];
    terminal.extend((0..count).map(|i| Some(i % 2 == 0)));
    Graph::new(next, terminal)
}

fn session(graph: Graph, width: usize) -> SearchSession<Graph> {
    SearchSession::new(
        graph,
        0,
        SearchConfig {
            max_pending_guidance: width,
            ..Default::default()
        },
        batch_budget(),
    )
    .unwrap()
}

fn frontier(session: &mut SearchSession<Graph>) -> Vec<NodeRef> {
    session
        .submit_guidance(session.root(), Guidance::default())
        .unwrap();
    let work = session.gather_guidance().unwrap();
    assert_eq!(work.stopped, None);
    work.sources
}

fn neutral_replies(sources: &[NodeRef]) -> Vec<(NodeRef, Guidance)> {
    sources.iter().map(|s| (*s, Guidance::default())).collect()
}

#[test]
fn waiting_leaves_do_not_block_other_guided_work_or_acquire_fake_samples() {
    let graph = branches(3);
    let mut session = session(graph.clone(), 3);
    let initial = session.gather_guidance().unwrap();
    assert_eq!(initial.sources, [session.root()]);
    assert!(graph.calls.borrow().is_empty());
    let sources = frontier(&mut session);
    assert_eq!(sources.len(), 3);
    assert_eq!(*graph.calls.borrow(), [(0, 1), (0, 2), (0, 3)]);
    let before = session.stats();
    let ticket = session
        .begin_guidance_batch(&sources[..1], reservation())
        .unwrap();
    assert_eq!(
        session.submit_guidance(sources[0], Guidance::default()),
        Err(SearchError::GuidanceInFlight)
    );
    for _ in 0..3 {
        let waiting = session.gather_guidance().unwrap();
        assert_eq!(waiting.sources, sources[1..]);
        assert_eq!(waiting.stopped, None);
    }
    assert_eq!(session.stats(), before);
    assert_eq!(graph.calls.borrow().len(), 3);
    assert_eq!(session.view(session.root()).unwrap().evaluation_samples, 0);

    session
        .submit_guidance(sources[1], Guidance::default())
        .unwrap();
    assert!(matches!(
        session.step().unwrap(),
        StepStatus::Terminal { won: false, .. }
    ));
    assert_eq!(graph.calls.borrow().last(), Some(&(2, 5)));
    assert_eq!(
        session.guidance_receipt(ticket).unwrap().outcome,
        GuidanceOutcome::InFlight
    );
    let waiting = session.gather_guidance().unwrap();
    assert_eq!(waiting.sources, [sources[2]]);
    assert_eq!(waiting.stopped, None);
    let root = session.view(session.root()).unwrap();
    assert_eq!(root.visits, 4);
    assert_eq!(root.evaluation_samples, 1);
    assert_eq!(root.estimated_value, Some(0.0));

    let second = session
        .begin_guidance_batch(&sources[2..], reservation())
        .unwrap();
    assert!(session.gather_guidance().unwrap().sources.is_empty());
    session
        .complete_guidance_batch(second, neutral_replies(&sources[2..]), None)
        .unwrap();
    assert!(matches!(
        session.step().unwrap(),
        StepStatus::Terminal { won: true, .. }
    ));
    session
        .complete_guidance_batch(ticket, neutral_replies(&sources[..1]), None)
        .unwrap();
    assert!(matches!(
        session.step().unwrap(),
        StepStatus::Terminal { won: true, .. }
    ));
    assert_eq!(
        session.step().unwrap(),
        StepStatus::Stopped(StopReason::NoOpenWork)
    );
}

#[test]
fn invalid_batches_are_atomic_and_still_account_the_attempt() {
    let mut session = session(branches(2), 2);
    let sources = frontier(&mut session);
    let foreign = super::SearchSession::new(branches(2), 0, SearchConfig::default(), budget())
        .unwrap()
        .root();
    let before = session.stats();
    let cases = [
        (
            vec![
                (
                    sources[0],
                    Guidance {
                        value: Some(1.0),
                        ..Default::default()
                    },
                ),
                (
                    sources[1],
                    Guidance {
                        weights: Some(vec![f64::NAN]),
                        value: None,
                    },
                ),
            ],
            SearchError::InvalidGuidance,
        ),
        (neutral_replies(&sources[..1]), SearchError::InvalidGuidance),
        (
            neutral_replies(&[sources[0], sources[0]]),
            SearchError::DuplicateGuidanceSource,
        ),
        (
            neutral_replies(&[sources[0], foreign]),
            SearchError::InvalidSource,
        ),
    ];
    for (index, (replies, expected)) in cases.into_iter().enumerate() {
        let ticket = session
            .begin_guidance_batch(&sources, reservation())
            .unwrap();
        let usage = Some(TokenUsage {
            input: 5,
            output: 2,
        });
        assert_eq!(
            session.complete_guidance_batch(ticket, replies, usage),
            Err(expected)
        );
        assert_eq!(
            session.guidance_receipt(ticket).unwrap().outcome,
            GuidanceOutcome::Rejected
        );
        assert_eq!(session.guidance_receipt(ticket).unwrap().usage, usage);
        assert_eq!(session.pending_guidance(), sources);
        assert_eq!(session.stats(), before);
        let costs = session.guidance_costs();
        assert_eq!(costs.calls, index + 1);
        assert_eq!(costs.rejected, index + 1);
        assert_eq!(costs.in_flight, 0);
        assert_eq!(costs.known_input_tokens, (index as u128 + 1) * 5);
        assert_eq!(
            session.complete_guidance_batch(ticket, neutral_replies(&sources), usage),
            Err(SearchError::GuidanceAlreadyFinished)
        );
        assert_eq!(session.guidance_costs(), costs);
    }
    let ticket = session
        .begin_guidance_batch(&sources, reservation())
        .unwrap();
    session
        .complete_guidance_batch(ticket, neutral_replies(&sources), None)
        .unwrap();
    assert!(session.pending_guidance().is_empty());
}

#[test]
fn response_order_does_not_rebind_nodes_or_double_count_a_ticket() {
    let mut a = session(branches(3), 3);
    let mut b = session(branches(3), 3);
    let sources = frontier(&mut a);
    let ticket = a.begin_guidance_batch(&sources, reservation()).unwrap();
    let foreign = b.begin_guidance_batch(&[b.root()], reservation()).unwrap();
    let before = a.guidance_costs();
    assert_eq!(
        a.complete_guidance_batch(foreign, neutral_replies(&sources), None),
        Err(SearchError::InvalidGuidanceTicket)
    );
    assert_eq!(a.guidance_costs(), before);
    let replies = sources
        .iter()
        .rev()
        .map(|s| {
            (
                *s,
                Guidance {
                    value: Some(*a.view(*s).unwrap().state as f64 / 10.0),
                    weights: None,
                },
            )
        })
        .collect();
    a.complete_guidance_batch(
        ticket,
        replies,
        Some(TokenUsage {
            input: 9,
            output: 4,
        }),
    )
    .unwrap();
    for source in &sources {
        let view = a.view(*source).unwrap();
        assert_eq!(view.estimated_value, Some(*view.state as f64 / 10.0));
        assert_eq!(view.evaluation_samples, 1);
    }
    let before = a.guidance_costs();
    assert_eq!(
        a.fail_guidance_batch(ticket, None),
        Err(SearchError::GuidanceAlreadyFinished)
    );
    assert_eq!(a.guidance_costs(), before);
    assert_eq!(a.guidance_receipt(ticket).unwrap().sources, sources);
}

#[test]
fn invalid_admission_never_charges_or_reserves_a_node() {
    let mut a = session(branches(2), 2);
    let b = session(branches(2), 2);
    let sources = frontier(&mut a);
    for (attempt, expected) in [
        (vec![], SearchError::InvalidGuidance),
        (
            vec![sources[0], sources[0]],
            SearchError::DuplicateGuidanceSource,
        ),
        (vec![sources[0], b.root()], SearchError::InvalidSource),
        (vec![sources[0], a.root()], SearchError::InvalidSource),
    ] {
        assert_eq!(
            a.begin_guidance_batch(&attempt, reservation()),
            Err(expected)
        );
        assert_eq!(a.guidance_costs(), GuidanceCosts::default());
        assert_eq!(a.pending_guidance(), sources);
    }
    let ticket = a
        .begin_guidance_batch(&sources[..1], reservation())
        .unwrap();
    let before = a.guidance_costs();
    assert_eq!(
        a.begin_guidance_batch(&sources, reservation()),
        Err(SearchError::GuidanceInFlight)
    );
    assert_eq!(a.guidance_costs(), before);
    assert_eq!(a.pending_guidance(), sources[1..]);
    a.fail_guidance_batch(ticket, None).unwrap();
    assert_eq!(a.pending_guidance(), sources);
}

#[test]
fn token_reservations_cover_in_flight_and_unknown_usage_and_release_known_surplus() {
    let mut session = SearchSession::new(
        branches(3),
        0,
        SearchConfig {
            max_pending_guidance: 3,
            ..Default::default()
        },
        SearchBudget {
            guidance: GuidanceBudget {
                max_calls: 3,
                max_input_tokens: 10,
                max_output_tokens: 6,
            },
            ..budget()
        },
    )
    .unwrap();
    let sources = frontier(&mut session);
    let first = session
        .begin_guidance_batch(
            &sources[..1],
            TokenReservation {
                input: 8,
                output: 4,
            },
        )
        .unwrap();
    assert_eq!(session.guidance_costs().charged_input_tokens, 8);
    for reserve in [
        TokenReservation {
            input: 3,
            output: 0,
        },
        TokenReservation {
            input: 0,
            output: 3,
        },
    ] {
        assert_eq!(
            session.begin_guidance_batch(&sources[1..2], reserve),
            Err(SearchError::GuidanceBudgetExceeded)
        );
    }
    assert_eq!(session.guidance_costs().calls, 1);
    session
        .fail_guidance_batch(
            first,
            Some(TokenUsage {
                input: 2,
                output: 1,
            }),
        )
        .unwrap();
    let retry = session
        .begin_guidance_batch(
            &sources[..1],
            TokenReservation {
                input: 7,
                output: 4,
            },
        )
        .unwrap();
    session.fail_guidance_batch(retry, None).unwrap();
    assert_eq!(session.guidance_receipt(retry).unwrap().usage, None);
    let costs = session.guidance_costs();
    assert_eq!(
        (costs.known_input_tokens, costs.known_output_tokens),
        (2, 1)
    );
    assert_eq!(
        (costs.charged_input_tokens, costs.charged_output_tokens),
        (9, 5)
    );
    assert_eq!(costs.unknown_usage_calls, 1);
    assert_eq!(costs.failed, 2);
    assert_eq!(session.pending_guidance(), sources);
    assert_eq!(
        session.begin_guidance_batch(
            &sources[..1],
            TokenReservation {
                input: 2,
                output: 1
            }
        ),
        Err(SearchError::GuidanceBudgetExceeded)
    );
    let last = session
        .begin_guidance_batch(
            &sources[..1],
            TokenReservation {
                input: 1,
                output: 1,
            },
        )
        .unwrap();
    session
        .complete_guidance_batch(
            last,
            neutral_replies(&sources[..1]),
            Some(TokenUsage {
                input: 0,
                output: 0,
            }),
        )
        .unwrap();
    assert_eq!(
        session.begin_guidance_batch(&sources[1..2], TokenReservation::default()),
        Err(SearchError::GuidanceBudgetExceeded)
    );
    assert_eq!(session.guidance_costs().calls, 3);
}

#[test]
fn reservation_overrun_records_actual_cost_rejects_guidance_and_stops_new_calls() {
    for usage in [
        TokenUsage {
            input: 101,
            output: 1,
        },
        TokenUsage {
            input: 1,
            output: 101,
        },
    ] {
        let mut session = session(branches(2), 2);
        let sources = frontier(&mut session);
        let before = session.stats();
        let ticket = session
            .begin_guidance_batch(&sources, reservation())
            .unwrap();
        assert_eq!(
            session.complete_guidance_batch(ticket, neutral_replies(&sources), Some(usage)),
            Err(SearchError::GuidanceUsageOverrun)
        );
        assert_eq!(
            session.guidance_receipt(ticket).unwrap().outcome,
            GuidanceOutcome::UsageExceededReservation
        );
        assert_eq!(session.stats(), before);
        assert_eq!(session.pending_guidance(), sources);
        let costs = session.guidance_costs();
        assert_eq!(costs.known_input_tokens, u128::from(usage.input));
        assert_eq!(costs.charged_output_tokens, u128::from(usage.output));
        assert_eq!(costs.usage_overruns, 1);
        assert_eq!(
            session.begin_guidance_batch(&sources, TokenReservation::default()),
            Err(SearchError::GuidanceBudgetExceeded)
        );
    }
}

#[test]
fn exhausted_search_or_guidance_budget_admits_no_new_call() {
    let graph = branches(1);
    for (limits, error) in [
        (
            SearchBudget {
                max_services: 0,
                ..batch_budget()
            },
            SearchError::SearchBudgetExhausted(StopReason::ServiceBudget),
        ),
        (
            SearchBudget {
                max_nodes: 1,
                ..batch_budget()
            },
            SearchError::SearchBudgetExhausted(StopReason::NodeBudget),
        ),
        (
            SearchBudget {
                max_engine_steps: 0,
                ..batch_budget()
            },
            SearchError::SearchBudgetExhausted(StopReason::EngineBudget),
        ),
        (
            SearchBudget {
                deadline: Some(Instant::now() - Duration::from_secs(1)),
                ..batch_budget()
            },
            SearchError::SearchBudgetExhausted(StopReason::Deadline),
        ),
        (budget(), SearchError::GuidanceBudgetExceeded),
    ] {
        let mut session =
            SearchSession::new(graph.clone(), 0, SearchConfig::default(), limits).unwrap();
        assert_eq!(
            session.begin_guidance_batch(&[session.root()], reservation()),
            Err(error)
        );
        assert_eq!(session.guidance_costs(), GuidanceCosts::default());
        assert_eq!(session.view(session.root()).unwrap().evaluation_samples, 0);
        assert!(graph.calls.borrow().is_empty());
    }
    let mut session = SearchSession::new(
        graph,
        0,
        SearchConfig::default(),
        SearchBudget {
            max_services: 1,
            ..batch_budget()
        },
    )
    .unwrap();
    session
        .submit_guidance(session.root(), Guidance::default())
        .unwrap();
    let work = session.gather_guidance().unwrap();
    assert_eq!(work.stopped, Some(StopReason::ServiceBudget));
    assert_eq!(work.sources.len(), 1);
    assert_eq!(
        session.begin_guidance_batch(&work.sources, reservation()),
        Err(SearchError::SearchBudgetExhausted(
            StopReason::ServiceBudget
        ))
    );
    assert_eq!(session.stats().terminal_observations, 0);
}

#[test]
fn batching_changes_synthetic_call_count_without_losing_finite_graph_outcomes() {
    let mut runs = Vec::new();
    for width in [1, 3] {
        let graph = branches(6);
        let mut session = session(graph.clone(), width);
        loop {
            let work = session.gather_guidance().unwrap();
            if let Some(reason) = work.stopped {
                assert_eq!(reason, StopReason::NoOpenWork);
                break;
            }
            assert!(!work.sources.is_empty());
            // Synthetic message costs, not measured provider usage.
            let usage = TokenUsage {
                input: 10 + 2 * work.sources.len() as u64,
                output: 3 * work.sources.len() as u64,
            };
            let ticket = session
                .begin_guidance_batch(&work.sources, reservation())
                .unwrap();
            session
                .complete_guidance_batch(ticket, neutral_replies(&work.sources), Some(usage))
                .unwrap();
        }
        let mut transitions = graph.calls.borrow().clone();
        transitions.sort_unstable();
        let mut wins = session
            .observed_wins()
            .iter()
            .map(|s| *session.view(*s).unwrap().state)
            .collect::<Vec<_>>();
        wins.sort_unstable();
        assert_eq!(session.stats().services, 12);
        assert_eq!(session.stats().engine_steps, 12);
        assert_eq!(session.stats().terminal_observations, 6);
        assert_eq!(session.stats().coverage_gaps, 0);
        assert_eq!(session.stats().guidance_replies, 7);
        assert_eq!(session.view(session.root()).unwrap().evaluation_samples, 6);
        runs.push((transitions, wins, session.guidance_costs()));
    }
    assert_eq!(runs[0].0, runs[1].0);
    assert_eq!(runs[0].1, runs[1].1);
    assert_eq!((runs[0].2.calls, runs[1].2.calls), (7, 3));
    assert_eq!(
        (runs[0].2.known_input_tokens, runs[1].2.known_input_tokens),
        (84, 44)
    );
    assert_eq!(
        (runs[0].2.known_output_tokens, runs[1].2.known_output_tokens),
        (21, 21)
    );
}

#[test]
fn real_selection_combat_accepts_batched_guidance_and_replays_the_win() {
    let mut root = combat_position(
        &[CardId::BurningPact, CardId::Defend, CardId::Wound],
        &[CardId::Strike],
        6,
    );
    root.combat.entities.player.current_hp = 1;
    let original = root.clone();
    let mut session = SearchSession::new(
        CombatSearchSpace::default(),
        CombatSearchState::new(root).unwrap(),
        SearchConfig {
            max_pending_guidance: 3,
            ..Default::default()
        },
        batch_budget(),
    )
    .unwrap();
    let win = loop {
        let work = session.gather_guidance().unwrap();
        // Batching may discover another valid win first. This sentinel needs
        // the selection path as well, not a particular first-discovery order.
        if let Some(win) = session.observed_wins().into_iter().find(|win| {
            session
                .path(*win)
                .unwrap()
                .iter()
                .any(|(_, action, _)| matches!(action, CombatBranch::FinishSelection))
        }) {
            break win;
        }
        assert_eq!(work.stopped, None);
        assert!(!work.sources.is_empty());
        let replies = work
            .sources
            .iter()
            .map(|source| {
                let view = session.view(*source).unwrap();
                (*source, fixture_guidance(view.state, &view.actions))
            })
            .collect();
        let ticket = session
            .begin_guidance_batch(&work.sources, reservation())
            .unwrap();
        session
            .complete_guidance_batch(
                ticket,
                replies,
                Some(TokenUsage {
                    input: 0,
                    output: 0,
                }),
            )
            .unwrap();
    };
    let replay = session.replay_win(win, 10_000, 256, None).unwrap();
    assert_eq!(replay.root(), &original);
    assert_eq!(
        session.view(session.root()).unwrap().state.position(),
        &original
    );
    assert!(replay
        .inputs()
        .iter()
        .any(|i| matches!(i, ClientInput::SubmitSelection(_))));
    assert!(EngineCombatStepper.is_exact_victory(replay.successor()));
    assert!(session.stats().guidance_replies > session.guidance_costs().calls);
    let timings = session.timings();
    assert!(timings.transitions <= timings.active_search);
    assert!(timings.active_search <= timings.elapsed);
    assert!(timings.blocked_on_guidance <= timings.elapsed);
}
