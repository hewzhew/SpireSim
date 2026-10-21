use super::*;

#[test]
fn immediate_win_is_replayed_before_any_model_estimate_and_boundaries_are_not_recursive() {
    let mut s = search(root(&[CardId::Strike], 6))
        .with_immediate_probe(true)
        .unwrap();
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert_eq!(s.stats.evaluated_states, 0);
    assert_eq!(s.plans()[0].inputs.len(), 1);
    assert_eq!(s.stats.immediate_transitions, 2); // EndTurn and Strike.
    assert_eq!(s.immediate_checks()[0].lower, 1);
    assert_eq!(s.immediate_checks()[0].witnessed_remaining_inputs, Some(1));
    let children = s.nodes[0].children.clone();
    let boundary = s.nodes[0].probe_boundary.clone().unwrap();
    assert_eq!(boundary.len(), 1);
    assert!(s.nodes[boundary[0]].features.is_none());
    assert!(s.nodes[boundary[0]].choices.is_none());
    assert!(s.data().rows[0].complete && !s.nodes[0].released);
    let b = s.evaluations().unwrap();
    assert_eq!(b.nodes, vec![0]);
    s.accept_estimates(b.token, "test-model", &[9.]).unwrap();
    let before = s.stats.transitions;
    s.advance(budget(1)).unwrap();
    assert_eq!(
        s.stats.transitions, before,
        "opening an existing boundary must not replay its parent edge"
    );
    assert_eq!(s.nodes[0].children, children);
    assert_eq!(s.nodes[0].release_cursor, 1);
    assert!(s.nodes[boundary[0]].features.is_some());
    for _ in 0..1000 {
        if matches!(
            service(&mut s, 100, 9.),
            DistanceStop::NodeLimit | DistanceStop::DepthLimit | DistanceStop::NoOpenWork
        ) {
            break;
        }
    }
    assert_eq!(s.stats.transitions, s.nodes.len() - 1);
    assert_eq!(s.nodes[0].children, children);
}

#[test]
fn one_real_input_includes_virtual_assembly_but_not_the_following_card_play() {
    let mut r = root(&[CardId::Defend, CardId::Strike], 6);
    r.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        reason: HandSelectReason::Exhaust,
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        candidate_lineages: sts_core::test_support::scenario_lineages([1]),
    });
    let mut s = search(r).with_immediate_probe(true).unwrap();
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert_eq!(s.stats.real_inputs, 1);
    assert_eq!(s.stats.virtual_choices, 2);
    assert!(s.plans().is_empty());
    assert_eq!(s.immediate_checks()[0].lower, 2);
    assert_eq!(s.immediate_checks()[0].boundary_nodes, 1);
    assert_eq!(s.nodes.iter().map(|n| n.g).max(), Some(1));
    for _ in 0..1000 {
        service(&mut s, 100, 9.);
        if !s.plans().is_empty() {
            break;
        }
    }
    assert_eq!(s.plans()[0].inputs.len(), 2);
    assert_connected_reference(&s);
}

#[test]
fn immediate_interruption_preserves_partial_coverage_and_original_limits() {
    let mut s = search(root(&[CardId::Strike], 6))
        .with_immediate_probe(true)
        .unwrap();
    assert_eq!(s.advance(budget(1)).unwrap(), DistanceStop::WorkBudget);
    assert!(s.immediate_pending());
    assert_eq!(
        s.advance(DistanceBudget {
            max_nodes: 1,
            ..budget(100)
        })
        .unwrap(),
        DistanceStop::NodeLimit
    );
    assert_eq!(
        s.advance(DistanceBudget {
            max_depth: 0,
            ..budget(100)
        })
        .unwrap(),
        DistanceStop::DepthLimit
    );
    assert_eq!(s.stats.transitions, 0);
    let expired = DistanceBudget {
        step: CombatStepLimits {
            deadline: Some(Instant::now()),
            ..budget(1).step
        },
        ..budget(100)
    };
    assert_eq!(s.advance(expired).unwrap(), DistanceStop::Deadline);
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
    assert!(s.immediate_checks().is_empty() && s.immediate_pending());
    let row = &s.data().rows[0];
    assert!(!row.complete && row.lower == 1 && row.upper == 9);
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert!(!s.immediate_pending());
    assert_eq!(s.immediate_checks()[0].witnessed_remaining_inputs, Some(1));
}

#[test]
fn immediate_probes_keep_pause_resume_order_and_forced_prefixes() {
    for forced in [false, true] {
        let r = root(&[CardId::Strike, CardId::Defend, CardId::Strike], 12);
        let mut a = search(r.clone())
            .with_forced_advance(forced)
            .unwrap()
            .with_immediate_probe(true)
            .unwrap();
        let mut b = search(r)
            .with_forced_advance(forced)
            .unwrap()
            .with_immediate_probe(true)
            .unwrap();
        for (s, work) in [(&mut a, 1), (&mut b, 10000)] {
            for _ in 0..20000 {
                if matches!(
                    service(s, work, 9.),
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
            serde_json::to_value(a.immediate_checks()).unwrap(),
            serde_json::to_value(b.immediate_checks()).unwrap()
        );
        assert!(!a.plans().is_empty());
        assert_eq!(a.stats.transitions, a.nodes.len() - 1);
        assert!(a.stats.immediate_transitions <= a.stats.transitions);
        assert_eq!(
            a.stats.immediate_checks_started,
            a.stats.immediate_checks_completed + usize::from(a.immediate_pending())
        );
    }
    let mut r = root(&[], 6);
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    let mut s = search(r)
        .with_forced_advance(true)
        .unwrap()
        .with_immediate_probe(true)
        .unwrap();
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert_eq!(s.stats.evaluated_states, 0);
    assert_eq!(s.plans()[0].inputs.len(), 2);
    assert_eq!(s.plans()[0].inputs[0], ClientInput::EndTurn);
}

#[test]
fn immediate_kills_obey_full_resource_query_and_do_not_make_low_hp_a_failure() {
    let mut r = root(&[CardId::Strike, CardId::Reaper], 4);
    r.combat.entities.player.current_hp = 10;
    let mut s = DistanceSearch::new(
        r,
        ResourceRequirement {
            min_final_hp: 14,
            preserve_starting_potion_slots: 0,
        },
        8,
        "test-model".into(),
        16,
    )
    .unwrap()
    .with_immediate_probe(true)
    .unwrap();
    assert_eq!(s.advance(budget(100)).unwrap(), DistanceStop::NoOpenWork);
    assert!(s.nodes[1..].iter().all(|n| n.terminal.is_some()));
    assert_eq!(s.stats.evaluated_states, 0);
    assert_eq!(s.plans().len(), 1);
    assert_eq!(s.plans()[0].outcome.final_hp, 14);
    assert!(matches!(
        s.plans()[0].inputs[0],
        ClientInput::PlayCard { card_index: 1, .. }
    ));
    assert!(s.data().rows.iter().any(|r| r.lower == 9 && r.upper == 9));

    use sts_core::content::potions::{Potion, PotionId};
    let mut r = root(&[], 6);
    r.combat.entities.player.current_hp = 1;
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 1))];
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
    .with_immediate_probe(true)
    .unwrap();
    assert_eq!(s.advance(budget(100)).unwrap(), DistanceStop::NoOpenWork);
    assert!(s.plans().is_empty());
    assert_eq!(s.stats.evaluated_states, 0);
    assert_eq!(s.nodes[1].spent, 1);
    assert_eq!(s.immediate_checks()[0].lower, 9);
}

#[test]
fn predicted_checks_and_immediate_probes_cannot_claim_the_same_work() {
    assert!(search(root(&[CardId::Strike], 6))
        .with_short_lookahead(2)
        .unwrap()
        .with_immediate_probe(true)
        .is_err());
    assert!(search(root(&[CardId::Strike], 6))
        .with_immediate_probe(true)
        .unwrap()
        .with_short_lookahead(2)
        .is_err());
}

fn forced_boundary_root() -> CombatPosition {
    let mut r = root(&[CardId::Strike], 12);
    r.combat.turn.energy = 1;
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    r
}

fn forced_boundary_search(r: CombatPosition) -> DistanceSearch {
    search(r)
        .with_forced_advance(true)
        .unwrap()
        .with_immediate_probe(true)
        .unwrap()
        .with_probe_forced_boundaries(true)
        .unwrap()
}

#[test]
fn probe_boundary_advances_only_unique_choices_and_reuses_prepared_sources() {
    let mut s = forced_boundary_search(forced_boundary_root());
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert_eq!(s.stats.evaluated_states, 0);
    assert!(
        s.plans().is_empty(),
        "a next-turn card play is a new choice"
    );
    assert_eq!(s.stats.probe_forced_transitions, 1);
    let forced = s
        .nodes
        .iter()
        .position(|n| n.g == 1 && n.choices.as_ref().is_some_and(|c| c.len() == 1))
        .unwrap();
    let next = s.nodes[forced].children[0];
    assert_eq!(s.nodes[next].input, Some(ClientInput::EndTurn));
    assert_eq!(s.nodes[next].g, 2);
    assert!(s.nodes[forced].released);
    assert!(s.nodes[next].children.is_empty());
    assert!(s.nodes[0].probe_boundary.as_ref().unwrap().contains(&next));
    assert!(
        s.nodes[0]
            .probe_boundary
            .as_ref()
            .unwrap()
            .iter()
            .all(|i| s.nodes[*i].choices.as_ref().unwrap().len() > 1
                && s.nodes[*i].features.is_some())
    );
    let root_children = s.nodes[0].children.clone();
    let before_prepared = s.stats.prepared;
    let before_transitions = s.stats.transitions;
    let b = s.evaluations().unwrap();
    assert_eq!(b.nodes, vec![0]);
    s.accept_estimates(b.token, "test-model", &[9.]).unwrap();
    assert_eq!(s.advance(budget(1)).unwrap(), DistanceStop::WorkBudget);
    assert_eq!(
        s.stats.prepared, before_prepared,
        "opening a classified boundary cannot charge preparation twice"
    );
    assert_eq!(s.stats.transitions, before_transitions + 1);
    assert_eq!(s.nodes[0].children, root_children);
}

#[test]
fn forced_probe_tail_respects_depth_node_work_deadline_and_engine_limits() {
    for estimates in [false, true] {
        check_forced_probe_limits(estimates);
    }
}

fn check_forced_probe_limits(estimates: bool) {
    let mut s = forced_boundary_search(forced_boundary_root())
        .with_probe_boundary_estimates(estimates)
        .unwrap();
    assert_eq!(
        s.advance(DistanceBudget {
            max_depth: 1,
            ..budget(100)
        })
        .unwrap(),
        DistanceStop::DepthLimit
    );
    assert!(s.immediate_pending());
    assert_eq!(s.stats.probe_forced_transitions, 0);
    let nodes = s.nodes.len();
    assert_eq!(
        s.advance(DistanceBudget {
            max_nodes: nodes,
            ..budget(100)
        })
        .unwrap(),
        DistanceStop::NodeLimit
    );
    assert_eq!(
        s.advance(DistanceBudget {
            step: CombatStepLimits {
                deadline: Some(Instant::now()),
                ..budget(1).step
            },
            ..budget(1)
        })
        .unwrap(),
        DistanceStop::Deadline
    );
    let short = || DistanceBudget {
        step: CombatStepLimits {
            max_engine_steps: 1,
            deadline: None,
        },
        ..budget(100)
    };
    assert_eq!(s.advance(short()).unwrap(), DistanceStop::EngineStepLimit);
    let before = s.stats.clone();
    assert_eq!(s.advance(short()).unwrap(), DistanceStop::EngineStepLimit);
    assert_eq!(s.stats, before);
    assert_eq!(s.nodes.len(), nodes);
    assert_eq!(s.advance(budget(1)).unwrap(), DistanceStop::WorkBudget);
    assert_eq!(s.stats.work(), before.work() + 1);
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert!(!s.immediate_pending());
    assert!(s.stats.probe_forced_transitions <= s.stats.immediate_transitions);
}

#[test]
fn forced_probe_tails_preserve_pause_resume_and_full_input_distances() {
    for estimates in [false, true] {
        check_forced_probe_resume(estimates);
    }
}

fn check_forced_probe_resume(estimates: bool) {
    let r = forced_boundary_root();
    let mut a = forced_boundary_search(r.clone())
        .with_probe_boundary_estimates(estimates)
        .unwrap();
    let mut b = forced_boundary_search(r)
        .with_probe_boundary_estimates(estimates)
        .unwrap();
    for (s, work) in [(&mut a, 1), (&mut b, 10000)] {
        for _ in 0..20000 {
            if matches!(
                service(s, work, 9.),
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
        serde_json::to_value(a.immediate_checks()).unwrap(),
        serde_json::to_value(b.immediate_checks()).unwrap()
    );
    assert!(!a.plans().is_empty());
    assert_eq!(
        a.stats.prepared,
        a.nodes.iter().filter(|n| n.features.is_some()).count()
    );
    assert_eq!(a.stats.transitions, a.nodes.len() - 1);
    assert!(a.stats.probe_forced_transitions > 0);
    assert_connected_reference(&a);
}

#[test]
fn forced_probe_tail_keeps_passive_potion_expenditure_and_query_failure() {
    for estimates in [false, true] {
        check_forced_probe_potion(estimates);
    }
}

fn check_forced_probe_potion(estimates: bool) {
    use sts_core::content::potions::{Potion, PotionId};
    let mut r = forced_boundary_root();
    r.combat.entities.player.current_hp = 1;
    r.combat.entities.potions = vec![Some(Potion::new(PotionId::FairyPotion, 1))];
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
    .unwrap()
    .with_immediate_probe(true)
    .unwrap()
    .with_probe_forced_boundaries(true)
    .unwrap()
    .with_probe_boundary_estimates(estimates)
    .unwrap();
    assert_eq!(s.advance(budget(100)).unwrap(), DistanceStop::NoOpenWork);
    assert!(s.plans().is_empty());
    assert!(s
        .nodes
        .iter()
        .any(|n| n.g == 2 && n.spent == 1 && n.terminal == Some(false)));
    assert_eq!(s.stats.evaluated_states, 0);
}

#[test]
fn forced_probe_keeps_selection_input_before_forced_end_turn() {
    for estimates in [false, true] {
        check_forced_probe_selection(estimates);
    }
}

fn check_forced_probe_selection(estimates: bool) {
    let mut r = root(&[CardId::Defend, CardId::Strike], 12);
    r.combat.turn.energy = 0;
    r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Strike, 101)].into();
    r.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
        reason: HandSelectReason::Exhaust,
        min_cards: 1,
        max_cards: 1,
        can_cancel: false,
        candidate_lineages: sts_core::test_support::scenario_lineages([1, 2]),
    });
    let mut s = forced_boundary_search(r)
        .with_probe_boundary_estimates(estimates)
        .unwrap();
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert!(s.stats.virtual_choices >= 2);
    let begin = s.nodes[0].children[0];
    let select = s.nodes[begin].children[0];
    let commit = s.nodes[select].children[0];
    assert!(s.nodes[begin].input.is_none() && s.nodes[select].input.is_none());
    assert_eq!(
        (s.nodes[begin].g, s.nodes[select].g, s.nodes[commit].g),
        (0, 0, 1)
    );
    let end = s.nodes[commit].children[0];
    assert_eq!(s.nodes[end].input, Some(ClientInput::EndTurn));
    assert_eq!(s.nodes[end].g, 2);
    assert!(s.nodes[end].children.is_empty());
    assert!(s.nodes[end].choices.as_ref().unwrap().len() > 1);
    assert_eq!(s.stats.evaluated_states, 0);
}

#[test]
fn forced_probe_boundary_configuration_is_explicit_and_frozen() {
    assert!(search(forced_boundary_root())
        .with_probe_forced_boundaries(true)
        .is_err());
    assert!(search(forced_boundary_root())
        .with_immediate_probe(true)
        .unwrap()
        .with_probe_forced_boundaries(true)
        .is_err());
    assert!(forced_boundary_search(forced_boundary_root())
        .with_forced_advance(false)
        .is_err());
    assert!(forced_boundary_search(forced_boundary_root())
        .with_immediate_probe(false)
        .is_err());
    let mut s = forced_boundary_search(forced_boundary_root());
    s.advance(budget(1)).unwrap();
    assert!(s.with_probe_forced_boundaries(false).is_err());
}

#[test]
fn prepared_boundaries_compete_without_parent_estimates_or_recursive_admission() {
    let mut s = forced_boundary_search(forced_boundary_root())
        .with_probe_boundary_estimates(true)
        .unwrap();
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    let b = s.evaluations().unwrap();
    assert!(s.nodes[0].released && s.nodes[0].estimate.is_none());
    assert_eq!(s.stats.immediate_checks_started, 1);
    assert!(b.nodes.len() > 1 && !b.nodes.contains(&0));
    assert!(b.nodes.iter().all(|i| {
        let n = &s.nodes[*i];
        n.features.is_some()
            && n.choices.as_ref().unwrap().len() > 1
            && n.children.is_empty()
            && !n.released
    }));
    // Prefer the later boundary. Neither the parent's value nor sibling order
    // can force the earlier boundary to be probed first.
    let selected = *b.nodes.last().unwrap();
    let mut values = vec![9.; b.nodes.len()];
    *values.last_mut().unwrap() = 0.;
    let before_data = serde_json::to_value(s.data()).unwrap();
    let before = s.stats.clone();
    s.accept_estimates(b.token, "test-model", &values).unwrap();
    assert_eq!(serde_json::to_value(s.data()).unwrap(), before_data);
    assert_eq!(s.advance(budget(1)).unwrap(), DistanceStop::WorkBudget);
    assert_eq!(s.stats.immediate_checks_started, 2);
    assert_eq!(s.stats.prepared, before.prepared);
    assert_eq!(s.stats.transitions, before.transitions + 1);
    assert_eq!(s.nodes[selected].children.len(), 1);
    assert!(b.nodes[..b.nodes.len() - 1]
        .iter()
        .all(|i| s.nodes[*i].children.is_empty()));
}

#[test]
fn boundary_estimates_preserve_direct_wins_before_inference_and_depth_deferral() {
    let mut s = forced_boundary_search(root(&[CardId::Strike], 6))
        .with_probe_boundary_estimates(true)
        .unwrap();
    assert_eq!(
        s.advance(budget(100)).unwrap(),
        DistanceStop::NeedsEvaluation
    );
    assert_eq!(s.plans()[0].inputs.len(), 1);
    assert_eq!(s.stats.evaluated_states, 0);
    let b = s.evaluations().unwrap();
    s.accept_estimates(b.token, "test-model", &vec![9.; b.nodes.len()])
        .unwrap();
    let before = s.stats.clone();
    assert_eq!(
        s.advance(DistanceBudget {
            max_depth: 1,
            ..budget(100)
        })
        .unwrap(),
        DistanceStop::DepthLimit
    );
    assert_eq!(s.stats, before);
    assert_eq!(
        s.deferred.values().map(Vec::len).sum::<usize>(),
        b.nodes.len()
    );
    s.advance(budget(1)).unwrap();
    assert_eq!(s.stats.immediate_checks_started, 2);
    assert_eq!(s.stats.transitions, before.transitions + 1);
    assert_eq!(s.plans()[0].inputs.len(), 1);
}

#[test]
fn boundary_estimation_configuration_cannot_disable_its_prerequisites() {
    assert!(search(forced_boundary_root())
        .with_probe_boundary_estimates(true)
        .is_err());
    assert!(forced_boundary_search(forced_boundary_root())
        .with_probe_boundary_estimates(true)
        .unwrap()
        .with_probe_forced_boundaries(false)
        .is_err());
    let mut s = forced_boundary_search(forced_boundary_root());
    s.advance(budget(1)).unwrap();
    assert!(s.with_probe_boundary_estimates(true).is_err());
}
