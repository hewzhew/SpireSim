use super::*;
use sts_combat_outcome::combat_witness_materialization::starting_potion_slots_targeted_by_input_v1;
use sts_core::content::{
    cards::CardId,
    monsters::EnemyId,
    potions::{Potion, PotionId},
};
use sts_core::runtime::combat::CombatCard;
use sts_core::state::core::{EngineState, HandSelectReason, PendingChoice};

mod inspection_checks {
    use super::*;
    use crate::path_search::inspection::ReferenceProgress;
    use crate::path_search::routes::{reconstruct_route, CompleteRoute, RouteLimits};

    fn reference(
        r: &CombatPosition,
        g: &ResourceRequirement,
        inputs: Vec<ClientInput>,
    ) -> CompleteRoute {
        let step = CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        };
        let mut p = r.clone();
        for input in &inputs {
            let next = EngineCombatStepper.apply_to_stable(&p, input.clone(), step);
            assert!(!next.truncated && !next.timed_out);
            p = next.position;
        }
        reconstruct_route(
            r,
            g,
            &inputs,
            &p,
            RouteLimits {
                max_decisions: 128,
                step,
            },
        )
        .unwrap()
    }
    fn play(index: usize) -> ClientInput {
        ClientInput::PlayCard {
            card_index: index,
            target: Some(1),
        }
    }

    #[test]
    fn witnessed_queue_keeps_exact_edges_shared_ancestors_and_runtime_costs() {
        use super::linear::{FrozenLinearPrior, LinearWeights, FEATURE_COUNT, SCHEMA};
        let r = root(&[CardId::Strike, CardId::Strike, CardId::Strike], 18);
        let g = goal(1, 0);
        let route = reference(&r, &g, vec![play(2), play(0), play(0)]);
        let make = || {
            PathSearch::with_order(
                r.clone(),
                g.clone(),
                GroupedNeutralPrior,
                PathOrder::RelativePrior,
            )
            .unwrap()
        };
        let mut checked = make();
        let mut control = make();
        let zero = FrozenLinearPrior::new(LinearWeights {
            schema: SCHEMA,
            group: vec![0.0; FEATURE_COUNT],
            member: vec![0.0; FEATURE_COUNT],
        })
        .unwrap();
        let mut samples = 0;
        for _ in 0..150 {
            if let Some(data) = checked
                .witnessed_queue(std::slice::from_ref(&route), 1)
                .unwrap()
            {
                samples += 1;
                assert!(data.edges.len() <= 2 && !data.edges.is_empty());
                assert_eq!(
                    data.edges
                        .iter()
                        .filter(|e| !e.witnesses.is_empty())
                        .count(),
                    1
                );
                let review = checked.inspect_route(&route).unwrap();
                let target = data.edges.iter().find(|e| !e.witnesses.is_empty()).unwrap();
                assert_eq!(
                    target.choice,
                    route.decisions[review.reached_decisions].selected
                );
                let logs = data
                    .nodes
                    .iter()
                    .map(|n| zero.score(&n.features).unwrap())
                    .collect::<Vec<_>>();
                let mut costs = vec![0.0; data.nodes.len()];
                for (n, node) in data.nodes.iter().enumerate() {
                    if let Some(p) = node.parent {
                        assert!(p < n);
                        costs[n] = costs[p]
                            + (logs[p].iter().copied().reduce(f64::max).unwrap()
                                - logs[p][node.incoming_choice.unwrap()]);
                    } else {
                        assert_eq!(n, 0);
                    }
                }
                for edge in data.edges {
                    let cost = costs[edge.parent]
                        + (logs[edge.parent].iter().copied().reduce(f64::max).unwrap()
                            - logs[edge.parent][edge.choice]);
                    assert!((cost - edge.source_cost).abs() < 1e-12);
                }
            }
            assert_eq!(
                checked.advance(budget(1)).unwrap(),
                control.advance(budget(1)).unwrap()
            );
            assert_eq!(checked.stats(), control.stats());
            assert_eq!(
                serde_json::to_value(checked.plans()).unwrap(),
                serde_json::to_value(control.plans()).unwrap()
            );
        }
        assert!(samples > 0);
        let mut other = route.clone();
        other.root.combat.entities.player.current_hp -= 1;
        assert!(checked.witnessed_queue(&[other], 1).is_err());
        assert!(checked.witnessed_queue(&[route], 0).is_err());
    }

    #[test]
    fn witnessed_queue_filters_dominated_routes_and_skips_completed_evidence() {
        let mut r = root(&[CardId::Strike, CardId::Reaper], 4);
        r.combat.entities.player.current_hp = 10;
        let g = goal(1, 0);
        let weaker = reference(&r, &g, vec![play(0)]);
        let better = reference(
            &r,
            &g,
            vec![ClientInput::PlayCard {
                card_index: 1,
                target: None,
            }],
        );
        assert!(better.outcome.strictly_dominates(&weaker.outcome));
        let mut s =
            PathSearch::with_order(r, g, GroupedNeutralPrior, PathOrder::RelativePrior).unwrap();
        assert!(s
            .witnessed_queue(&[weaker.clone(), better.clone()], 1)
            .unwrap()
            .is_none());
        let mut observed = false;
        for _ in 0..100 {
            s.advance(budget(1)).unwrap();
            if let Some(data) = s
                .witnessed_queue(&[weaker.clone(), better.clone()], 1)
                .unwrap()
            {
                observed = true;
                assert!(data
                    .edges
                    .iter()
                    .flat_map(|e| &e.witnesses)
                    .all(|&i| i == 1));
            }
        }
        assert!(observed);
        assert!(s.witnessed_queue(&[weaker, better], 1).unwrap().is_none());
    }

    #[test]
    fn inspection_is_read_only_at_preparation_queue_and_replay_boundaries() {
        let r = root(&[CardId::Strike, CardId::Strike, CardId::Defend], 12);
        let g = goal(1, 0);
        let route = reference(&r, &g, vec![play(0), play(0)]);
        let make = || {
            PathSearch::with_order(
                r.clone(),
                g.clone(),
                GroupedNeutralPrior,
                PathOrder::RelativePrior,
            )
            .unwrap()
        };
        let mut checked = make();
        let mut control = make();
        assert_eq!(
            checked.inspect_route(&route).unwrap().progress,
            ReferenceProgress::AwaitingPreparation
        );
        checked.advance(budget(1)).unwrap();
        control.advance(budget(1)).unwrap();
        assert_eq!(
            checked.inspect_route(&route).unwrap().progress,
            ReferenceProgress::AwaitingEnqueue
        );
        for _ in 0..100 {
            let before = checked.stats().clone();
            let inspection = checked.inspect_route(&route).unwrap();
            assert_eq!(inspection, checked.inspect_route(&route).unwrap());
            assert_eq!(checked.stats(), &before);
            assert_eq!(
                inspection
                    .divergences
                    .iter()
                    .map(|d| d.retained_nodes)
                    .sum::<usize>()
                    + inspection.reached_decisions
                    + 1,
                checked.nodes.len()
            );
            if let ReferenceProgress::Queued {
                ahead,
                strictly_lower_cost,
                equal_cost,
            } = inspection.progress
            {
                assert!(strictly_lower_cost <= ahead && equal_cost >= 1);
                assert_eq!(
                    ahead,
                    inspection
                        .divergences
                        .iter()
                        .map(|d| d.queued_ahead)
                        .sum::<usize>()
                );
            }
            assert_eq!(
                checked.advance(budget(1)).unwrap(),
                control.advance(budget(1)).unwrap()
            );
            assert_eq!(checked.stats(), control.stats());
            assert_eq!(
                serde_json::to_value(checked.plans()).unwrap(),
                serde_json::to_value(control.plans()).unwrap()
            );
        }
    }

    #[test]
    fn queued_reference_uses_exact_priority_depth_and_insertion_order() {
        let r = root(&[CardId::Strike, CardId::Strike, CardId::Strike], 12);
        let g = goal(1, 0);
        let route = reference(&r, &g, vec![play(2), play(0)]);
        let mut search =
            PathSearch::with_order(r, g, GroupedNeutralPrior, PathOrder::RelativePrior).unwrap();
        search.advance(budget(2)).unwrap();
        let review = search.inspect_route(&route).unwrap();
        assert_eq!(review.reached_decisions, 0); // A different duplicate is not this prefix.
        let choice = route.decisions[0].selected;
        let target = search
            .frontier
            .iter()
            .find(|e| e.parent == 0 && e.choice == choice)
            .unwrap();
        let mut sorted = search.frontier.clone().into_sorted_vec();
        sorted.reverse();
        let rank = sorted
            .iter()
            .position(|e| e.sequence == target.sequence)
            .unwrap();
        let ReferenceProgress::Queued { ahead, .. } = review.progress else {
            panic!("expected queued edge")
        };
        assert_eq!(ahead, rank);
        assert!(ahead > 0);
    }

    #[test]
    fn reference_depth_deferral_and_complete_reach_are_distinct() {
        let r = root(&[CardId::Strike, CardId::Strike], 12);
        let g = goal(1, 0);
        let route = reference(&r, &g, vec![play(0), play(0)]);
        let mut search =
            PathSearch::with_order(r, g, GroupedNeutralPrior, PathOrder::RelativePrior).unwrap();
        search
            .advance(PathBudget {
                max_depth: 1,
                ..budget(500)
            })
            .unwrap();
        let review = search.inspect_route(&route).unwrap();
        assert_eq!(review.reached_decisions, 1);
        assert_eq!(review.progress, ReferenceProgress::DepthDeferred);
        search
            .advance(PathBudget {
                max_depth: 2,
                ..budget(500)
            })
            .unwrap();
        let review = search.inspect_route(&route).unwrap();
        assert_eq!(review.reached_decisions, route.decisions.len());
        assert!(matches!(
            review.progress,
            ReferenceProgress::Reached {
                replay_pending: false,
                ..
            }
        ));
        let mut other = route.clone();
        other.goal.min_final_hp += 1;
        assert!(search.inspect_route(&other).is_err());
        other = route.clone();
        other.root.combat.entities.player.gold += 1;
        assert!(search.inspect_route(&other).is_err());
        other = route.clone();
        other.decisions[0].state = other.decisions[1].state.clone();
        assert!(search.inspect_route(&other).is_err());
    }

    #[test]
    fn reached_reference_still_distinguishes_pending_replay_from_a_published_plan() {
        let r = root(&[CardId::Strike], 6);
        let g = goal(1, 0);
        let route = reference(&r, &g, vec![play(0)]);
        let mut search =
            PathSearch::with_order(r, g, GroupedNeutralPrior, PathOrder::RelativePrior).unwrap();
        let pending = ReferenceProgress::Reached {
            replay_pending: true,
            retained_plan: false,
        };
        for _ in 0..100 {
            if search.inspect_route(&route).unwrap().progress == pending {
                break;
            }
            search
                .advance(PathBudget {
                    max_depth: 2,
                    ..budget(1)
                })
                .unwrap();
        }
        assert_eq!(search.inspect_route(&route).unwrap().progress, pending);
        search.advance(budget(1)).unwrap();
        assert_eq!(
            search.inspect_route(&route).unwrap().progress,
            ReferenceProgress::Reached {
                replay_pending: false,
                retained_plan: true
            }
        );
        let mut changed = route;
        changed.successor.combat.entities.player.current_hp -= 1;
        assert!(search.inspect_route(&changed).is_err());
    }

    #[test]
    fn positive_global_cost_scaling_preserves_queue_order_including_ties() {
        let edges = vec![(0., 1, 0), (2., 1, 1), (2., 2, 2), (0.5, 3, 3), (2., 2, 4)];
        let ordered = |scale: f64| {
            let mut heap = BinaryHeap::new();
            for &(cost, depth, sequence) in &edges {
                heap.push(Edge {
                    parent: 0,
                    choice: 0,
                    log_probability: 0.,
                    priority: cost * scale,
                    tie_depth: depth,
                    sequence,
                });
            }
            std::iter::from_fn(|| heap.pop())
                .map(|e| e.sequence)
                .collect::<Vec<_>>()
        };
        assert_eq!(ordered(1.), vec![0, 3, 2, 4, 1]);
        assert_eq!(ordered(1.), ordered(0.125));
        assert_eq!(ordered(1.), ordered(8.));
    }
}

fn root(hand: &[CardId], enemy_hp: i32) -> CombatPosition {
    let mut c = sts_core::test_support::blank_test_combat();
    let mut m = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    m.current_hp = enemy_hp;
    m.max_hp = enemy_hp;
    c.entities.monsters = vec![m];
    c.zones.hand = hand
        .iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(*id, i as u32 + 1))
        .collect();
    CombatPosition::new(EngineState::CombatPlayerTurn, c)
}
fn goal(hp: i32, slots: u64) -> ResourceRequirement {
    ResourceRequirement {
        min_final_hp: hp,
        preserve_starting_potion_slots: slots,
    }
}
fn budget(work: usize) -> PathBudget {
    PathBudget {
        work,
        max_nodes: 10000,
        max_pending_edges: 100000,
        max_depth: 100,
        max_engine_steps_per_input: 512,
        deadline: None,
    }
}

#[test]
fn log_prior_preserves_tiny_probabilities_and_rejects_invalid_mass() {
    let logs = log_softmax(&[1000.0, 0.0]).unwrap();
    assert_eq!(logs, vec![0.0, -1000.0]);
    prior::validate(&logs, 2).unwrap();
    assert!(prior::validate(&[0.0, f64::NEG_INFINITY], 2).is_err());
    assert!(prior::validate(&[-1.0, -1.0], 2).is_err());
    assert!(log_softmax(&[f64::MAX, -f64::MAX]).is_err());
}

struct PreferEnd;
impl PathPrior for PreferEnd {
    fn revision(&self) -> &str {
        "test-prefer-end"
    }
    fn log_probabilities(
        &self,
        _: &CombatSearchState,
        _: PathContext<'_>,
        branches: &[CombatBranch],
    ) -> Result<Vec<f64>, String> {
        log_softmax(
            &branches
                .iter()
                .map(|b| {
                    if matches!(b, CombatBranch::Input(ClientInput::EndTurn)) {
                        10.0
                    } else {
                        0.0
                    }
                })
                .collect::<Vec<_>>(),
        )
    }
}

#[test]
fn relative_prior_follows_equal_best_choices_without_probability_decay() {
    struct EqualPlays;
    impl PathPrior for EqualPlays {
        fn revision(&self) -> &str {
            "test-equal-plays"
        }
        fn log_probabilities(
            &self,
            _: &CombatSearchState,
            _: PathContext<'_>,
            b: &[CombatBranch],
        ) -> Result<Vec<f64>, String> {
            log_softmax(
                &b.iter()
                    .map(|branch| {
                        if matches!(branch, CombatBranch::Input(ClientInput::PlayCard { .. })) {
                            0.1
                        } else {
                            0.0
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        }
    }
    let r = root(
        &[
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
        ],
        18,
    );
    let mut search =
        PathSearch::with_order(r, goal(1, 0), EqualPlays, PathOrder::RelativePrior).unwrap();
    search.advance(budget(9)).unwrap();
    assert_eq!(search.stats().first_verified_win_work, Some(9));
    assert_eq!(search.plans()[0].inputs.len(), 3);
    assert!(search.plans()[0]
        .steps
        .iter()
        .all(|step| step.log_probability < -1.0));
    assert_eq!(search.stats().prepared_states, 3);
    assert_eq!(search.stats().transition_attempts, 3);
    assert_eq!(search.stats().replayed_inputs, 3);
}

#[test]
fn relative_prior_backtracks_after_a_confident_losing_choice_and_keeps_reserve() {
    let mut r = root(&[CardId::Strike], 6);
    r.combat.entities.player.current_hp = 1;
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 22))];
    let mut search =
        PathSearch::with_order(r, goal(1, 1), PreferEnd, PathOrder::RelativePrior).unwrap();
    search.advance(budget(50)).unwrap();
    assert!(search.stats().losses_or_escapes > 0);
    assert!(search.stats().explicit_constraint_rejections >= 2);
    assert!(!search.plans().is_empty());
    assert_eq!(
        search.plans()[0].outcome.starting_potion_expenditure_slots,
        0
    );
    assert_eq!(search.plans()[0].inputs.len(), 1);
}

#[test]
fn relative_depth_limit_defers_edges_and_still_searches_other_choices() {
    let r = root(&[CardId::Strike], 6);
    let hp = r.combat.entities.player.current_hp;
    let mut search =
        PathSearch::with_order(r, goal(hp, 0), PreferEnd, PathOrder::RelativePrior).unwrap();
    let limited = |work| PathBudget {
        max_depth: 2,
        ..budget(work)
    };
    assert_eq!(search.advance(limited(200)).unwrap(), PathStop::DepthLimit);
    assert!(search.stats().goal_misses > 0);
    assert!(search.plans().iter().any(|p| p.outcome.final_hp == hp));
    assert!(search.depth_deferred_edges() > 0);
    assert!(search.frontier.is_empty());
    assert_eq!(search.pending_edges(), search.depth_deferred_edges());
    let stats = search.stats().clone();
    let count = search.pending_edges();
    assert_eq!(search.advance(limited(200)).unwrap(), PathStop::DepthLimit);
    assert_eq!(search.stats(), &stats);
    assert_eq!(search.pending_edges(), count);
    assert_eq!(
        search
            .advance(PathBudget {
                max_pending_edges: count - 1,
                ..limited(200)
            })
            .unwrap(),
        PathStop::EdgeLimit
    );
    search
        .advance(PathBudget {
            max_depth: 3,
            ..budget(200)
        })
        .unwrap();
    assert!(search.stats().settled_inputs > stats.settled_inputs);
    assert!(search.depth_deferred.keys().all(|depth| *depth == 3));
    assert!(search.plans().iter().any(|p| p.outcome.final_hp == hp));
}

#[test]
fn relative_order_and_depth_deferrals_survive_work_and_memory_pauses() {
    let r = root(&[CardId::Strike, CardId::Defend, CardId::Strike], 12);
    let make = || {
        PathSearch::with_order(r.clone(), goal(1, 0), PreferEnd, PathOrder::RelativePrior).unwrap()
    };
    let limits = |work| PathBudget {
        max_depth: 4,
        ..budget(work)
    };
    let mut whole = make();
    let mut sliced = make();
    let mut paused = make();
    whole.advance(limits(300)).unwrap();
    for _ in 0..300 {
        sliced.advance(limits(1)).unwrap();
    }
    assert_eq!(
        paused
            .advance(PathBudget {
                max_pending_edges: 1,
                ..limits(300)
            })
            .unwrap(),
        PathStop::EdgeLimit
    );
    paused.advance(limits(300 - paused.stats().work())).unwrap();
    for search in [&sliced, &paused] {
        assert_eq!(whole.stats(), search.stats());
        assert_eq!(whole.pending_edges(), search.pending_edges());
        assert_eq!(whole.depth_deferred_edges(), search.depth_deferred_edges());
        assert_eq!(
            serde_json::to_value(whole.plans()).unwrap(),
            serde_json::to_value(search.plans()).unwrap()
        );
        let edges = |s: &PathSearch<PreferEnd>| {
            let mut rows: Vec<_> = s
                .frontier
                .iter()
                .chain(s.depth_deferred.values().flatten())
                .map(|e| {
                    (
                        e.sequence,
                        e.parent,
                        e.choice,
                        e.priority.to_bits(),
                        e.tie_depth,
                    )
                })
                .collect();
            rows.sort();
            rows
        };
        assert_eq!(edges(&whole), edges(search));
    }
}

#[test]
fn one_call_and_single_work_slices_preserve_search_and_replay() {
    let r = root(&[CardId::Strike, CardId::Defend, CardId::Strike], 12);
    let mut whole = PathSearch::new(r.clone(), goal(1, 0), GroupedNeutralPrior).unwrap();
    let mut slices = PathSearch::new(r, goal(1, 0), GroupedNeutralPrior).unwrap();
    whole.advance(budget(300)).unwrap();
    for _ in 0..300 {
        slices.advance(budget(1)).unwrap();
    }
    assert_eq!(whole.stats(), slices.stats());
    assert_eq!(whole.retained_nodes(), slices.retained_nodes());
    assert_eq!(whole.pending_edges(), slices.pending_edges());
    assert_eq!(
        serde_json::to_value(whole.plans()).unwrap(),
        serde_json::to_value(slices.plans()).unwrap()
    );
    assert!(!whole.plans().is_empty());
}

#[test]
fn reserved_potion_choices_are_excluded_before_prior_and_search() {
    for order in [PathOrder::PathProbability, PathOrder::RelativePrior] {
        let mut r = root(&[CardId::Strike], 6);
        r.combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 77))];
        let mut search = PathSearch::with_order(r, goal(1, 1), GroupedNeutralPrior, order).unwrap();
        search.advance(budget(120)).unwrap();
        assert!(search.stats.explicit_constraint_rejections >= 2);
        assert!(!search.plans().is_empty());
        for plan in search.plans() {
            assert_eq!(plan.outcome.starting_potion_expenditure_slots, 0);
            assert!(!plan.inputs.iter().any(|i| matches!(
                i,
                ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
            )));
        }
    }
}

#[test]
fn memory_pause_keeps_partly_enqueued_choices() {
    let r = root(&[CardId::Strike, CardId::Strike, CardId::Defend], 12);
    let mut a = PathSearch::new(r.clone(), goal(1, 0), GroupedNeutralPrior).unwrap();
    assert_eq!(
        a.advance(PathBudget {
            max_pending_edges: 1,
            ..budget(50)
        })
        .unwrap(),
        PathStop::EdgeLimit
    );
    let spent = a.stats.work();
    a.advance(budget(300 - spent)).unwrap();
    let mut b = PathSearch::new(r, goal(1, 0), GroupedNeutralPrior).unwrap();
    b.advance(budget(300)).unwrap();
    assert_eq!(a.stats(), b.stats());
    assert_eq!(
        serde_json::to_value(a.plans()).unwrap(),
        serde_json::to_value(b.plans()).unwrap()
    );
}

#[test]
fn engine_limit_is_not_retried_until_raised() {
    for order in [PathOrder::PathProbability, PathOrder::RelativePrior] {
        let r = root(&[CardId::Strike], 6);
        let mut a = PathSearch::with_order(r, goal(1, 0), GroupedNeutralPrior, order).unwrap();
        assert_eq!(
            a.advance(PathBudget {
                max_engine_steps_per_input: 1,
                ..budget(100)
            })
            .unwrap(),
            PathStop::EngineStepLimit
        );
        let before = a.stats().clone();
        assert_eq!(
            a.advance(PathBudget {
                max_engine_steps_per_input: 1,
                ..budget(100)
            })
            .unwrap(),
            PathStop::EngineStepLimit
        );
        assert_eq!(a.stats(), &before);
        a.advance(budget(100)).unwrap();
        assert!(!a.plans().is_empty());
    }
}

#[test]
fn selection_trace_retains_virtual_choices_but_replays_only_real_inputs() {
    // A control-flow fixture, not a quality claim for an untrained prior. With
    // uniform groups, EndTurn outranks each individual duplicate Strike.
    struct PlayBeforeEnd;
    impl PathPrior for PlayBeforeEnd {
        fn revision(&self) -> &str {
            "test-play-before-end"
        }
        fn log_probabilities(
            &self,
            _: &CombatSearchState,
            _: PathContext<'_>,
            branches: &[CombatBranch],
        ) -> Result<Vec<f64>, String> {
            log_softmax(
                &branches
                    .iter()
                    .map(|b| {
                        if matches!(b, CombatBranch::Input(ClientInput::EndTurn)) {
                            -1.0
                        } else {
                            0.0
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        }
    }
    for order in [PathOrder::PathProbability, PathOrder::RelativePrior] {
        let mut r = root(&[CardId::Defend, CardId::Strike, CardId::Strike], 12);
        r.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: sts_core::test_support::scenario_lineages([1, 2, 3]),
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: HandSelectReason::Exhaust,
        });
        let mut s = PathSearch::with_order(r, goal(1, 0), PlayBeforeEnd, order).unwrap();
        s.advance(budget(500)).unwrap();
        let p = s.plans().first().unwrap();
        assert!(p.steps.iter().any(|s| s.input.is_none()));
        assert!(matches!(p.inputs[0], ClientInput::SubmitSelection(_)));
        assert_eq!(
            p.steps
                .iter()
                .filter_map(|s| s.input.clone())
                .collect::<Vec<_>>(),
            p.inputs
        );
        assert!(p.steps.iter().all(|s| s.log_probability.is_finite()));
    }
}

#[test]
fn passive_fairy_closes_reserved_query_but_unreserved_search_can_win() {
    for order in [PathOrder::PathProbability, PathOrder::RelativePrior] {
        let mut r = root(&[], 6);
        r.combat.entities.player.current_hp = 1;
        r.combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 1))];
        r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
        let mut reserved =
            PathSearch::with_order(r.clone(), goal(1, 1), GroupedNeutralPrior, order).unwrap();
        reserved.advance(budget(300)).unwrap();
        assert!(reserved.plans().is_empty());
        assert!(reserved.stats.successor_constraint_rejections > 0);
        let mut unrestricted =
            PathSearch::with_order(r, goal(1, 0), GroupedNeutralPrior, order).unwrap();
        unrestricted.advance(budget(300)).unwrap();
        assert!(!unrestricted.plans().is_empty());
        assert!(unrestricted
            .plans()
            .iter()
            .all(|p| p.outcome.starting_potion_expenditure_slots == 1));
    }
}

#[test]
fn later_win_can_heal_and_improve_first_witness() {
    for order in [PathOrder::PathProbability, PathOrder::RelativePrior] {
        let mut r = root(&[CardId::Strike, CardId::Reaper], 6);
        r.combat.entities.player.current_hp = 5;
        let mut s =
            PathSearch::with_order(r.clone(), goal(1, 0), GroupedNeutralPrior, order).unwrap();
        for _ in 0..100 {
            s.advance(budget(1)).unwrap();
            if !s.plans().is_empty() {
                break;
            }
        }
        assert_eq!(s.plans()[0].outcome.final_hp, 5);
        let first_work = s.stats().first_verified_win_work;
        assert!(first_work.is_some());
        s.advance(budget(400)).unwrap();
        assert_eq!(s.stats().first_verified_win_work, first_work);
        assert!(s.plans().iter().any(|p| p.outcome.final_hp >= 9));
        let mut heal_required =
            PathSearch::with_order(r, goal(9, 0), GroupedNeutralPrior, order).unwrap();
        heal_required.advance(budget(400)).unwrap();
        assert!(!heal_required.plans().is_empty());
        assert!(heal_required
            .plans()
            .iter()
            .all(|p| p.outcome.final_hp >= 9));
        assert!(heal_required.stats.goal_misses > 0);
    }
}

#[test]
fn original_potion_admission_uses_identity_not_slot_or_kind() {
    let mut r = root(&[], 6);
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 1)), None];
    let input = ClientInput::DiscardPotion(0);
    assert_eq!(
        starting_potion_slots_targeted_by_input_v1(&r, &r, &input),
        1
    );
    let mut replacement = r.clone();
    replacement.combat.entities.potions[0] = Some(Potion::new(PotionId::FirePotion, 2));
    assert_eq!(
        starting_potion_slots_targeted_by_input_v1(&r, &replacement, &input),
        0
    );
    let mut moved = r.clone();
    moved.combat.entities.potions.swap(0, 1);
    assert_eq!(
        starting_potion_slots_targeted_by_input_v1(&r, &moved, &ClientInput::DiscardPotion(1)),
        1
    );
}

#[test]
fn grouped_prior_does_not_multiply_mass_for_duplicate_cards() {
    let state = CombatSearchState::new(root(&[CardId::Strike, CardId::Strike, CardId::Defend], 20))
        .unwrap();
    let branches = vec![
        CombatBranch::Input(ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        }),
        CombatBranch::Input(ClientInput::PlayCard {
            card_index: 1,
            target: Some(1),
        }),
        CombatBranch::Input(ClientInput::PlayCard {
            card_index: 2,
            target: None,
        }),
    ];
    let logs = GroupedNeutralPrior
        .log_probabilities(
            &state,
            PathContext {
                root: state.position(),
                goal: &goal(1, 0),
                spent_originals: 0,
            },
            &branches,
        )
        .unwrap();
    prior::validate(&logs, 3).unwrap();
    assert!((logs[0].exp() - 0.25).abs() < 1e-12);
    assert!((logs[1].exp() - 0.25).abs() < 1e-12);
    assert!((logs[2].exp() - 0.5).abs() < 1e-12);
}

#[test]
fn node_depth_and_deadline_stops_do_not_discard_the_next_edge() {
    let r = root(&[CardId::Strike], 6);
    let mut s = PathSearch::new(r, goal(1, 0), GroupedNeutralPrior).unwrap();
    assert_eq!(
        s.advance(PathBudget {
            deadline: Some(Instant::now()),
            ..budget(100)
        })
        .unwrap(),
        PathStop::Deadline
    );
    assert_eq!(s.stats.work(), 0);
    assert_eq!(
        s.advance(PathBudget {
            max_nodes: 1,
            ..budget(100)
        })
        .unwrap(),
        PathStop::NodeLimit
    );
    let pending = s.pending_edges();
    assert_eq!(
        s.advance(PathBudget {
            max_pending_edges: 0,
            ..budget(100)
        })
        .unwrap(),
        PathStop::EdgeLimit
    );
    assert_eq!(s.pending_edges(), pending);
    assert_eq!(
        s.advance(PathBudget {
            max_depth: 0,
            ..budget(100)
        })
        .unwrap(),
        PathStop::DepthLimit
    );
    assert_eq!(s.pending_edges(), pending);
    s.advance(budget(100)).unwrap();
    assert!(!s.plans().is_empty());
}

#[test]
fn invalid_prior_stops_without_executing_or_silently_removing_actions() {
    for order in [PathOrder::PathProbability, PathOrder::RelativePrior] {
        struct Broken;
        impl PathPrior for Broken {
            fn revision(&self) -> &str {
                "invalid-test-prior"
            }
            fn log_probabilities(
                &self,
                _: &CombatSearchState,
                _: PathContext<'_>,
                _: &[CombatBranch],
            ) -> Result<Vec<f64>, String> {
                Ok(vec![0.0])
            }
        }
        let mut s =
            PathSearch::with_order(root(&[CardId::Strike], 6), goal(1, 0), Broken, order).unwrap();
        assert!(s.advance(budget(100)).is_err());
        assert_eq!(s.stats.transition_attempts, 0);
        let before = s.stats().clone();
        assert!(s.advance(budget(100)).is_err());
        assert_eq!(s.stats(), &before);
    }
}
