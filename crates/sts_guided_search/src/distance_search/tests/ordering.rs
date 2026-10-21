use super::*;

#[test]
fn remaining_distance_can_prefer_a_longer_prefix_without_changing_evidence() {
    let mut s = search(root(&[CardId::Strike, CardId::Strike, CardId::Strike], 18));
    for _ in 0..20 {
        service(&mut s, 100, 1.);
        if s.nodes.iter().any(|n| n.g >= 3 && n.features.is_some()) {
            break;
        }
    }
    let shallow = s
        .nodes
        .iter()
        .position(|n| n.g == 1 && n.features.is_some())
        .unwrap();
    let deep = s
        .nodes
        .iter()
        .position(|n| n.g == 3 && n.features.is_some())
        .unwrap();
    let before = serde_json::to_value(s.data()).unwrap();
    // Synthetic predictions isolate ordering; they are not gameplay labels.
    let mut first = vec![];
    for ordering in [
        DistanceOrdering::TotalInputs,
        DistanceOrdering::RemainingInputs,
    ] {
        s.ordering = ordering;
        s.frontier.clear();
        s.pending = vec![shallow, deep];
        let b = s.evaluations().unwrap();
        s.accept_estimates(b.token, "test-model", &[3., 2.])
            .unwrap();
        first.push(s.frontier.pop().unwrap().node);
    }
    assert_eq!(first, vec![shallow, deep]);
    assert_eq!(serde_json::to_value(s.data()).unwrap(), before);
    assert!(s.with_ordering(DistanceOrdering::TotalInputs).is_err());
}

#[test]
fn remaining_distance_keeps_real_input_ties_and_factual_floors() {
    let mut q = BinaryHeap::from([
        Entry {
            node: 0,
            f: 2.,
            tie_g: 8,
        },
        Entry {
            node: 1,
            f: 2.,
            tie_g: 3,
        },
        Entry {
            node: 2,
            f: 1.9,
            tie_g: 20,
        },
    ]);
    assert_eq!(
        (
            q.pop().unwrap().node,
            q.pop().unwrap().node,
            q.pop().unwrap().node
        ),
        (2, 1, 0)
    );
    let mut s = search(root(&[CardId::Strike], 6))
        .with_ordering(DistanceOrdering::RemainingInputs)
        .unwrap()
        .with_immediate_probe(true)
        .unwrap();
    s.advance(budget(100)).unwrap();
    assert_eq!(s.priority(0, 0.), 1.);
    assert_eq!(s.data().rows[0].upper, 1);
    assert_eq!(s.plans()[0].inputs.len(), 1);
}

#[test]
fn remaining_order_resumes_through_selection_and_each_probe_mode() {
    for (immediate, short) in [(false, 0), (true, 0), (false, 2)] {
        let mut r = root(&[CardId::Defend, CardId::Strike, CardId::Strike], 12);
        r.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
            reason: HandSelectReason::Exhaust,
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            candidate_lineages: sts_core::test_support::scenario_lineages([1, 2, 3]),
        });
        let make = || {
            search(r.clone())
                .with_ordering(DistanceOrdering::RemainingInputs)
                .unwrap()
                .with_forced_advance(true)
                .unwrap()
                .with_short_lookahead(short)
                .unwrap()
                .with_immediate_probe(immediate)
                .unwrap()
        };
        let mut a = make();
        let mut b = make();
        for (s, work) in [(&mut a, 1), (&mut b, 10000)] {
            for _ in 0..20000 {
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
        assert!(!a.plans().is_empty());
        assert!(a.stats.virtual_choices > 0);
        assert_connected_reference(&a);
    }
}
