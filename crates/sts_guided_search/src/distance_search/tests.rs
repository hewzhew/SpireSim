use super::*;
mod immediate;
mod ordering;
mod outcomes;
use sts_core::{
    content::{cards::CardId, monsters::EnemyId},
    runtime::combat::CombatCard,
    state::core::{EngineState, HandSelectReason, PendingChoice},
};

fn root(hand: &[CardId], hp: i32) -> CombatPosition {
    let mut c = sts_core::test_support::blank_test_combat();
    let mut m = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
    m.current_hp = hp;
    m.max_hp = hp;
    c.entities.monsters = vec![m];
    c.zones.hand = hand
        .iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(*id, i as u32 + 1))
        .collect();
    CombatPosition::new(EngineState::CombatPlayerTurn, c)
}
fn budget(work: usize) -> DistanceBudget {
    DistanceBudget {
        work,
        max_nodes: 800,
        max_depth: 12,
        step: CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    }
}
fn search(root: CombatPosition) -> DistanceSearch {
    DistanceSearch::new(
        root,
        ResourceRequirement {
            min_final_hp: 1,
            preserve_starting_potion_slots: 0,
        },
        8,
        "test-model".into(),
        16,
    )
    .unwrap()
}
fn service(s: &mut DistanceSearch, work: usize, value: f64) -> DistanceStop {
    let stop = s.advance(budget(work)).unwrap();
    if stop == DistanceStop::NeedsEvaluation {
        let b = s.evaluations().unwrap();
        s.accept_estimates(b.token, "test-model", &vec![value; b.nodes.len()])
            .unwrap();
    }
    stop
}

#[test]
fn estimates_are_frozen_source_bound_atomic_and_cannot_close_branches() {
    let r = root(&[CardId::Strike], 6);
    let mut a = search(r.clone());
    let mut b = search(r);
    assert_eq!(
        a.advance(budget(10)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert_eq!(
        b.advance(budget(10)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    let x = a.evaluations().unwrap();
    let y = b.evaluations().unwrap();
    let before = a.stats().clone();
    assert!(a.accept_estimates(y.token, "test-model", &[9.]).is_err());
    assert!(a.accept_estimates(x.token, "new-model", &[9.]).is_err());
    assert!(a
        .accept_estimates(x.token, "test-model", &[f64::NAN])
        .is_err());
    assert_eq!(a.stats(), &before);
    assert_eq!(a.evaluations().unwrap().token, x.token);
    a.accept_estimates(x.token, "test-model", &[9.]).unwrap();
    assert!(a.accept_estimates(x.token, "test-model", &[9.]).is_err());
    for _ in 0..20 {
        service(&mut a, 20, 9.);
        if !a.plans().is_empty() {
            break;
        }
    }
    assert!(
        !a.plans().is_empty(),
        "range overflow is not proof of failure"
    );
    assert_eq!(a.plans()[0].inputs.len(), 1);
}

#[test]
fn pause_resume_preserves_exact_search_order_and_full_replay() {
    let r = root(&[CardId::Strike, CardId::Defend, CardId::Strike], 12);
    let mut a = search(r.clone());
    let mut b = search(r);
    for _ in 0..10000 {
        let stop = service(&mut a, 1, 0.);
        if matches!(
            stop,
            DistanceStop::NodeLimit | DistanceStop::DepthLimit | DistanceStop::NoOpenWork
        ) {
            break;
        }
    }
    for _ in 0..1000 {
        let stop = service(&mut b, 10000, 0.);
        if matches!(
            stop,
            DistanceStop::NodeLimit | DistanceStop::DepthLimit | DistanceStop::NoOpenWork
        ) {
            break;
        }
    }
    assert_eq!(a.stats(), b.stats());
    assert_eq!(
        serde_json::to_value(a.plans()).unwrap(),
        serde_json::to_value(b.plans()).unwrap()
    );
    assert!(!a.plans().is_empty());
    let data = a.data();
    assert_eq!(data.rows[0].witnessed_remaining_inputs, Some(2));
    assert_eq!(data.rows[0].upper, 2);
    assert!(data.rows.iter().any(|r| !r.complete && r.upper > r.lower));
}

#[test]
fn partial_successors_do_not_prove_failure_or_make_predictions_factual() {
    let mut s = search(root(&[CardId::Defend, CardId::Strike], 6));
    service(&mut s, 10, 9.);
    let before = s.data();
    assert_eq!((before.rows[0].lower, before.rows[0].upper), (0, 9));
    s.advance(budget(1)).unwrap();
    let after = s.data();
    assert!(!after.rows[0].complete);
    assert_eq!((after.rows[0].lower, after.rows[0].upper), (0, 9));
    assert_eq!(after.rows[0].witnessed_remaining_inputs, None);
}

#[test]
fn virtual_selection_does_not_increase_remaining_input_distance() {
    let mut r = root(&[CardId::Strike, CardId::Defend, CardId::Strike], 12);
    r.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        reason: HandSelectReason::Exhaust,
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        candidate_lineages: sts_core::test_support::scenario_lineages([1, 2, 3]),
    });
    let mut s = search(r);
    for _ in 0..1000 {
        service(&mut s, 1000, 0.);
        if !s.plans().is_empty() {
            break;
        }
    }
    assert!(!s.plans().is_empty());
    let data = s.data();
    assert!(data
        .rows
        .iter()
        .flat_map(|r| &r.children)
        .any(|(_, c)| *c == 0));
    assert_eq!(data.rows[0].witnessed_remaining_inputs, Some(3));
    assert_eq!(s.plans()[0].inputs.len(), 3);
    assert_connected_reference(&s);
}

fn assert_connected_reference(s: &DistanceSearch) {
    let plan = &s.plans()[0];
    let d = route_neighborhood(
        &s.root,
        &s.goal,
        &plan.inputs,
        &plan.successor,
        8,
        crate::path_search::routes::RouteLimits {
            max_decisions: 100,
            step: budget(1).step,
        },
    )
    .unwrap();
    let mut references: Vec<_> = d
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.route_decision.map(|decision| (decision, i)))
        .collect();
    references.sort_unstable();
    assert!(references.len() >= 2);
    assert_eq!(d.rows.iter().filter(|r| r.parent.is_none()).count(), 1);
    for pair in references.windows(2) {
        let parent = pair[0].1;
        let child = pair[1].1;
        assert_eq!(d.rows[child].parent, Some(parent));
        let (_, cost) = d.rows[parent]
            .children
            .iter()
            .find(|(j, _)| *j == child)
            .expect("checked reference continuation must be an actual edge");
        assert_eq!(
            d.rows[parent].witnessed_remaining_inputs.unwrap(),
            d.rows[child].witnessed_remaining_inputs.unwrap() + usize::from(*cost)
        );
        assert!(d.rows[child].complete);
    }
    for (i, row) in d.rows.iter().enumerate() {
        for (child, _) in &row.children {
            assert!(*child > i, "fact propagation needs a forward graph");
            assert_eq!(d.rows[*child].parent, Some(i));
        }
        if row.route_decision.is_none() && row.lower != row.upper {
            assert!(!row.complete && row.children.is_empty());
        }
    }
    let (_, last) = references.last().unwrap();
    assert!(d.rows[*last].children.iter().any(|(j, _)| {
        d.rows[*j].witnessed_remaining_inputs == Some(0) && d.rows[*j].upper == 0
    }));
}

#[test]
fn reference_neighbors_keep_the_checked_continuation_without_completing_alternatives() {
    let mut s = search(root(&[CardId::Strike, CardId::Strike], 12));
    for _ in 0..100 {
        service(&mut s, 100, 0.);
        if !s.plans().is_empty() {
            break;
        }
    }
    assert_connected_reference(&s);
}

#[test]
fn short_check_proves_a_distance_bound_without_discarding_longer_wins() {
    let mut s = search(root(&[CardId::Strike, CardId::Strike, CardId::Strike], 18))
        .with_short_lookahead(2)
        .unwrap();
    for _ in 0..1000 {
        service(&mut s, 100, 1.);
        if !s.short_checks().is_empty() {
            break;
        }
    }
    let check = &s.short_checks()[0];
    assert_eq!(check.node, 0);
    assert_eq!(check.lower, 3);
    assert_eq!(check.witnessed_remaining_inputs, None);
    let child = s.nodes[0]
        .children
        .iter()
        .copied()
        .find(|j| s.nodes[*j].terminal.is_none())
        .unwrap();
    assert!(s.required_lower(child) >= 2);
    assert!(s.priority(child, 0.) >= 3.);
    for _ in 0..1000 {
        service(&mut s, 100, 1.);
        if s.plans().iter().any(|p| p.inputs.len() == 3) {
            break;
        }
    }
    assert!(s.plans().iter().any(|p| p.inputs.len() == 3));
    assert_eq!(s.data().rows[0].upper, 3);
    assert!(s.stats.short_transitions > 0);
    assert!(s.stats.short_transitions <= s.stats.transitions);
}

#[test]
fn short_checks_resume_exactly_across_virtual_choices_and_budget_stops() {
    let mut r = root(&[CardId::Strike, CardId::Defend, CardId::Strike], 12);
    r.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        reason: HandSelectReason::Exhaust,
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        candidate_lineages: sts_core::test_support::scenario_lineages([1, 2, 3]),
    });
    let mut a = search(r.clone()).with_short_lookahead(2).unwrap();
    let mut b = search(r).with_short_lookahead(2).unwrap();
    for (s, work) in [(&mut a, 1), (&mut b, 10000)] {
        for _ in 0..10000 {
            if matches!(
                service(s, work, 1.),
                DistanceStop::NodeLimit | DistanceStop::DepthLimit | DistanceStop::NoOpenWork
            ) {
                break;
            }
        }
    }
    assert_eq!(a.stats(), b.stats());
    assert_eq!(
        serde_json::to_value(a.data()).unwrap(),
        serde_json::to_value(b.data()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(a.plans()).unwrap(),
        serde_json::to_value(b.plans()).unwrap()
    );
    assert_eq!(
        serde_json::to_value(a.short_checks()).unwrap(),
        serde_json::to_value(b.short_checks()).unwrap()
    );
    assert_eq!(a.short_checks()[0].lower, 3);
    assert_eq!(a.plans()[0].inputs.len(), 3);
    assert!(a.stats.virtual_choices > 0);
}

#[test]
fn interrupted_short_check_is_not_complete_and_can_reach_a_verified_short_win() {
    let mut s = search(root(&[CardId::Strike, CardId::Strike], 12))
        .with_short_lookahead(2)
        .unwrap();
    service(&mut s, 100, 1.);
    assert_eq!(
        s.advance(DistanceBudget {
            max_nodes: 1,
            ..budget(100)
        })
        .unwrap(),
        DistanceStop::NodeLimit
    );
    assert!(s.short_check_pending());
    assert!(s.short_checks().is_empty());
    assert_eq!(s.data().rows[0].lower, 1);
    assert_eq!(s.data().rows[0].upper, 9);
    let interrupted = || DistanceBudget {
        step: CombatStepLimits {
            max_engine_steps: 1,
            deadline: None,
        },
        ..budget(100)
    };
    assert_eq!(
        s.advance(interrupted()).unwrap(),
        DistanceStop::EngineStepLimit
    );
    let before = s.stats().clone();
    assert_eq!(
        s.advance(interrupted()).unwrap(),
        DistanceStop::EngineStepLimit
    );
    assert_eq!(s.stats(), &before);
    assert!(s.short_check_pending() && s.short_checks().is_empty());
    for _ in 0..1000 {
        service(&mut s, 100, 1.);
        if !s.short_checks().is_empty() {
            break;
        }
    }
    assert_eq!(s.short_checks()[0].lower, 2);
    assert_eq!(s.short_checks()[0].witnessed_remaining_inputs, Some(2));
    assert_eq!(s.plans()[0].inputs.len(), 2);
}

#[test]
fn short_check_depth_and_truncated_prediction_range_remain_independent() {
    let mut s = DistanceSearch::new(
        root(&[CardId::Strike, CardId::Strike, CardId::Strike], 18),
        ResourceRequirement {
            min_final_hp: 1,
            preserve_starting_potion_slots: 0,
        },
        1,
        "test-model".into(),
        16,
    )
    .unwrap()
    .with_short_lookahead(2)
    .unwrap();
    for _ in 0..1000 {
        service(&mut s, 100, 1.);
        if s.plans().iter().any(|p| p.inputs.len() == 3) {
            break;
        }
    }
    assert!(s.plans().iter().any(|p| p.inputs.len() == 3));
    assert_eq!(s.short_checks()[0].inputs, 2);
    assert_eq!(s.short_checks()[0].lower, 2);
    assert_eq!(s.data().rows[0].upper, 2);
}

#[test]
fn a_low_intermediate_hp_can_recover_and_reference_length_is_only_an_upper_bound() {
    let mut r = root(&[CardId::Reaper, CardId::Strike], 4);
    r.combat.entities.player.current_hp = 10;
    let goal = ResourceRequirement {
        min_final_hp: 14,
        preserve_starting_potion_slots: 0,
    };
    let mut s = DistanceSearch::new(r, goal, 8, "test-model".into(), 16).unwrap();
    for _ in 0..30 {
        service(&mut s, 100, 0.);
        if !s.plans().is_empty() {
            break;
        }
    }
    assert_eq!(s.plans()[0].outcome.final_hp, 14);
    let p = &s.plans()[0];
    let route = crate::path_search::routes::reconstruct_route(
        &s.root,
        &s.goal,
        &p.inputs,
        &p.successor,
        crate::path_search::routes::RouteLimits {
            max_decisions: 100,
            step: budget(1).step,
        },
    )
    .unwrap();
    let d = route_neighborhood(
        &route.root,
        &route.goal,
        &route.inputs,
        &route.successor,
        8,
        crate::path_search::routes::RouteLimits {
            max_decisions: 100,
            step: budget(1).step,
        },
    )
    .unwrap();
    assert_eq!((d.rows[0].lower, d.rows[0].upper), (1, 1));
    assert!(d.rows.iter().any(|r| r.lower == 9 && r.upper == 9));
}

#[test]
fn range_does_not_cap_search_and_interrupted_inputs_resume_once_limit_is_raised() {
    let mut s = DistanceSearch::new(
        root(&[CardId::Strike, CardId::Strike, CardId::Strike], 18),
        ResourceRequirement {
            min_final_hp: 1,
            preserve_starting_potion_slots: 0,
        },
        1,
        "test-model".into(),
        16,
    )
    .unwrap();
    service(&mut s, 10, 2.);
    let short = || DistanceBudget {
        step: CombatStepLimits {
            max_engine_steps: 1,
            deadline: None,
        },
        ..budget(100)
    };
    assert_eq!(s.advance(short()).unwrap(), DistanceStop::EngineStepLimit);
    let before = s.stats().clone();
    assert_eq!(s.advance(short()).unwrap(), DistanceStop::EngineStepLimit);
    assert_eq!(s.stats(), &before);
    for _ in 0..100 {
        service(&mut s, 100, 2.);
        if !s.plans().is_empty() {
            break;
        }
    }
    assert_eq!(s.plans()[0].inputs.len(), 3);
    assert_eq!(s.data().rows[0].witnessed_remaining_inputs, Some(3));
    assert_eq!(s.data().rows[0].upper, 2);
}

#[test]
fn successor_estimates_order_actual_states_after_their_full_parent_expansion() {
    let mut s = search(root(&[CardId::Strike, CardId::Defend], 12));
    service(&mut s, 10, 0.);
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    let b = s.evaluations().unwrap();
    let wanted = b
        .nodes
        .iter()
        .copied()
        .find(|i| {
            matches!(
                s.nodes[*i].input,
                Some(ClientInput::PlayCard { card_index: 1, .. })
            )
        })
        .unwrap();
    let values = b
        .nodes
        .iter()
        .map(|i| if *i == wanted { 0. } else { 9. })
        .collect::<Vec<_>>();
    assert_eq!(
        s.nodes[wanted].state.position().combat.entities.monsters[0].current_hp,
        12
    );
    assert_eq!(
        s.nodes[0].cursor,
        s.nodes[0].choices.as_ref().unwrap().len()
    );
    s.accept_estimates(b.token, "test-model", &values).unwrap();
    s.advance(budget(1)).unwrap();
    assert_eq!(s.active, Some(wanted));
}

#[test]
fn original_fairy_is_checked_on_real_successor_and_its_consumption_is_a_query_fact() {
    use sts_core::content::potions::{Potion, PotionId};
    let mut r = root(&[], 6);
    r.combat.entities.player.current_hp = 1;
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 1))];
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    for preserve in [0, 1] {
        let mut s = DistanceSearch::new(
            r.clone(),
            ResourceRequirement {
                min_final_hp: 1,
                preserve_starting_potion_slots: preserve,
            },
            8,
            "test-model".into(),
            16,
        )
        .unwrap();
        for _ in 0..100 {
            let stop = service(&mut s, 100, 0.);
            if matches!(
                stop,
                DistanceStop::NoOpenWork | DistanceStop::NodeLimit | DistanceStop::DepthLimit
            ) {
                break;
            }
        }
        if preserve == 1 {
            assert!(s.plans().is_empty());
            assert!(s.stats().rejected_originals > 0);
            assert_eq!((s.data().rows[0].lower, s.data().rows[0].upper), (9, 9));
        } else {
            assert!(!s.plans().is_empty());
            assert!(s
                .plans()
                .iter()
                .all(|p| p.outcome.starting_potion_expenditure_slots == 1));
        }
    }
}

#[test]
fn forced_end_turn_reaches_a_real_choice_without_a_source_estimate() {
    let mut r = root(&[], 6);
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    let mut s = search(r).with_forced_advance(true).unwrap();
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    let b = s.evaluations().unwrap();
    assert_eq!(s.stats.evaluated_states, 0);
    assert_eq!(s.stats.forced_prepared, 1);
    assert_eq!(s.stats.forced_transitions, 1);
    assert_eq!(s.nodes[0].estimate, None);
    assert_eq!(s.nodes[0].cursor, 1);
    assert_eq!(b.nodes, vec![1]);
    assert_eq!(s.nodes[1].input, Some(ClientInput::EndTurn));
    assert_eq!(s.nodes[1].g, 1);
    assert_eq!(s.nodes[1].state.position().combat.zones.hand.len(), 1);
    s.accept_estimates(b.token, "test-model", &[9.]).unwrap();
    for _ in 0..100 {
        service(&mut s, 100, 9.);
        if !s.plans().is_empty() {
            break;
        }
    }
    assert_eq!(s.plans()[0].inputs.len(), 2);
    assert_eq!(s.plans()[0].inputs[0], ClientInput::EndTurn);
    assert_connected_reference(&s);
}

#[test]
fn forced_virtual_selection_preserves_real_input_cost_and_never_chooses_between_cards() {
    let mut r = root(&[CardId::Defend, CardId::Strike], 6);
    r.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        reason: HandSelectReason::Exhaust,
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        candidate_lineages: sts_core::test_support::scenario_lineages([1]),
    });
    let mut s = search(r).with_forced_advance(true).unwrap();
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    let b = s.evaluations().unwrap();
    assert_eq!(s.stats.forced_transitions, 3); // Begin, select, commit.
    assert_eq!(s.stats.virtual_choices, 2);
    assert_eq!(s.stats.real_inputs, 1);
    assert_eq!(b.nodes, vec![3]);
    assert_eq!(s.nodes[1].g, 0);
    assert_eq!(s.nodes[2].g, 0);
    assert_eq!(s.nodes[3].g, 1);
    assert!(s.nodes[3].choices.as_ref().unwrap().len() > 1);
    assert!(s.nodes[..3]
        .iter()
        .all(|n| n.estimate.is_none() && n.cursor == 1));
    assert_eq!(s.forced_pending(), 0);
}

#[test]
fn forced_work_keeps_depth_node_deadline_and_engine_interruptions_resumable() {
    let mut r = root(&[], 6);
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    let mut s = search(r).with_forced_advance(true).unwrap();
    assert_eq!(s.advance(budget(1)).unwrap(), DistanceStop::WorkBudget);
    assert_eq!(s.forced_pending(), 1);
    assert_eq!(s.stats.transitions, 0);
    assert_eq!(
        s.advance(DistanceBudget {
            max_depth: 0,
            ..budget(100)
        })
        .unwrap(),
        DistanceStop::DepthLimit
    );
    assert_eq!(
        s.advance(DistanceBudget {
            max_nodes: 1,
            ..budget(100)
        })
        .unwrap(),
        DistanceStop::NodeLimit
    );
    assert_eq!(s.nodes[0].cursor, 0);
    let before = s.stats().clone();
    assert_eq!(
        s.advance(DistanceBudget {
            step: CombatStepLimits {
                deadline: Some(Instant::now()),
                ..budget(1).step
            },
            ..budget(100)
        })
        .unwrap(),
        DistanceStop::Deadline
    );
    assert_eq!(s.stats(), &before);
    let short = || DistanceBudget {
        step: CombatStepLimits {
            max_engine_steps: 1,
            deadline: None,
        },
        ..budget(100)
    };
    assert_eq!(s.advance(short()).unwrap(), DistanceStop::EngineStepLimit);
    let before = s.stats().clone();
    assert_eq!(s.advance(short()).unwrap(), DistanceStop::EngineStepLimit);
    assert_eq!(s.stats(), &before);
    assert_eq!(s.nodes[0].cursor, 0);
    assert_eq!(s.forced_pending(), 1);
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert_eq!(s.nodes[0].cursor, 1);
    assert_eq!(s.stats.real_inputs, 1);
    assert_eq!(s.stats.forced_transitions, 2); // Interrupted attempt plus actual input.
}

#[test]
fn forced_siblings_and_short_checks_resume_without_losing_work_or_expanding_past_the_check() {
    for k in [0, 1, 2] {
        let r = root(&[CardId::Strike, CardId::Defend, CardId::Strike], 12);
        let mut a = search(r.clone())
            .with_short_lookahead(k)
            .unwrap()
            .with_forced_advance(true)
            .unwrap();
        let mut b = search(r)
            .with_short_lookahead(k)
            .unwrap()
            .with_forced_advance(true)
            .unwrap();
        for (s, work) in [(&mut a, 1), (&mut b, 10000)] {
            for _ in 0..10000 {
                if matches!(
                    service(s, work, 1.),
                    DistanceStop::NodeLimit | DistanceStop::DepthLimit | DistanceStop::NoOpenWork
                ) {
                    break;
                }
            }
        }
        assert_eq!(a.stats(), b.stats());
        assert_eq!(
            serde_json::to_value(a.data()).unwrap(),
            serde_json::to_value(b.data()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(a.plans()).unwrap(),
            serde_json::to_value(b.plans()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(a.short_checks()).unwrap(),
            serde_json::to_value(b.short_checks()).unwrap()
        );
        assert!(!a.plans().is_empty());
        assert_eq!(
            a.nodes[0].cursor,
            a.nodes[0].choices.as_ref().unwrap().len()
        );
        assert!(
            a.stats.forced_transitions > 0 && a.stats.forced_transitions <= a.stats.transitions
        );
        for n in &a.nodes {
            if n.choices.as_ref().is_some_and(|c| c.len() == 1) {
                assert!(n.estimate.is_none());
            }
        }
    }
    let mut s = search(root(&[CardId::Strike], 12))
        .with_short_lookahead(1)
        .unwrap()
        .with_forced_advance(true)
        .unwrap();
    for _ in 0..1000 {
        service(&mut s, 1, 1.);
        if !s.short_checks().is_empty() {
            break;
        }
    }
    assert_eq!(s.short_checks()[0].inputs, 1);
    assert_eq!(s.short_checks()[0].lower, 2);
    assert_eq!(s.stats.short_transitions, 2); // Only the root's EndTurn and Strike.
}

#[test]
fn forced_end_turn_cannot_hide_a_preserved_fairys_passive_consumption() {
    use sts_core::content::potions::{Potion, PotionId};
    let mut r = root(&[], 6);
    r.combat.entities.player.current_hp = 1;
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 1))];
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    let mut s = DistanceSearch::new(
        r,
        ResourceRequirement {
            min_final_hp: 1,
            preserve_starting_potion_slots: 1,
        },
        8,
        "test-model".into(),
        16,
    )
    .unwrap()
    .with_forced_advance(true)
    .unwrap();
    assert_eq!(s.advance(budget(100)).unwrap(), DistanceStop::NoOpenWork);
    assert!(s.plans().is_empty());
    assert_eq!(s.stats.evaluated_states, 0);
    assert_eq!(s.stats.forced_transitions, 1);
    assert_eq!(s.nodes[1].spent, 1);
    assert_eq!((s.data().rows[0].lower, s.data().rows[0].upper), (9, 9));
}

#[test]
fn factual_bounds_configuration_requires_an_untouched_search() {
    let mut s = search(root(&[CardId::Strike], 6));
    assert_eq!(
        s.advance(budget(10)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert!(s.with_factual_bounds(true).is_err());
}

#[test]
fn factual_bounds_propagate_lower_bounds_without_probe_modes() {
    fn drain(s: &mut DistanceSearch) -> DistanceStop {
        let mut last = DistanceStop::NeedsEvaluation;
        for _ in 0..64 {
            last = service(s, 200, 8.);
            if !matches!(
                last,
                DistanceStop::NeedsEvaluation | DistanceStop::WorkBudget
            ) {
                break;
            }
        }
        last
    }
    // Low player HP bounds the EndTurn branches so both schedulers exhaust
    // the whole graph instead of stopping on an exploration limit.
    let mut r = root(&[CardId::Strike, CardId::Defend], 6);
    r.combat.entities.player.current_hp = 12;
    let mut plain = search(r.clone());
    let mut factual = search(r).with_factual_bounds(true).unwrap();
    assert_eq!(drain(&mut plain), DistanceStop::NoOpenWork);
    assert_eq!(drain(&mut factual), DistanceStop::NoOpenWork);
    // Without probe modes the default scheduler never touches factual bounds;
    // the opt-in flag makes complete-successor bounds propagate unconditionally.
    assert_eq!(plain.stats().lower_bound_updates, 0);
    assert!(factual.stats().lower_bound_updates > 0);
    // Ordering changes only; both schedulers verify the same one-input win.
    assert_eq!(plain.plans().len(), factual.plans().len());
    assert_eq!(factual.plans()[0].inputs.len(), 1);
}
