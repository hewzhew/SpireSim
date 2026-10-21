use super::*;
use sts_oracle_learning::ai::planner_core::PlannerPublicMapEdge;

fn node(point: Point, room: PlannerMapRoom, edges: &[Point]) -> PlannerPublicMapNode {
    PlannerPublicMapNode {
        x: point.0,
        y: point.1,
        room,
        has_emerald_key: false,
        edges: edges
            .iter()
            .map(|p| PlannerPublicMapEdge {
                destination_x: p.0,
                destination_y: p.1,
            })
            .collect(),
    }
}

fn map(nodes: Vec<PlannerPublicMapNode>) -> PlannerPublicMap {
    PlannerPublicMap {
        current_x: -1,
        current_y: -1,
        boss: None,
        nodes,
    }
}

// Independent, exhaustive oracle used only on small graphs. Production must
// match its preparation/key classes and elite extremes, without enumerating.
fn enumerate(point: Point, nodes: &Nodes<'_>) -> Vec<Vec<Point>> {
    let edges = nodes.get(&point).map_or_else(Vec::new, |n| destinations(n));
    if edges.is_empty() {
        return vec![vec![point]];
    }
    edges
        .into_iter()
        .flat_map(|next| {
            enumerate(next, nodes).into_iter().map(move |mut path| {
                path.insert(0, point);
                path
            })
        })
        .collect()
}

fn classify(points: &[Point], nodes: &Nodes<'_>) -> (Profile, usize) {
    let first = points.iter().position(|p| {
        nodes
            .get(p)
            .is_some_and(|n| n.room == PlannerMapRoom::Elite)
    });
    let before = &points[..first.unwrap_or(points.len())];
    let prep = before.iter().fold(0, |mask, p| {
        mask | match nodes.get(p).map(|n| n.room) {
            Some(PlannerMapRoom::Rest) => REST,
            Some(PlannerMapRoom::Shop) => SHOP,
            _ => 0,
        }
    });
    let count = points
        .iter()
        .filter(|p| {
            nodes
                .get(p)
                .is_some_and(|n| n.room == PlannerMapRoom::Elite)
        })
        .count();
    (
        Profile {
            has_elite: first.is_some(),
            preparation: prep,
            key: points.iter().any(|p| {
                nodes
                    .get(p)
                    .is_some_and(|n| n.room == PlannerMapRoom::Elite && n.has_emerald_key)
            }),
            complete: nodes
                .get(points.last().unwrap())
                .is_some_and(|n| matches!(n.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory)),
        },
        count,
    )
}

#[test]
fn representatives_match_exhaustive_routes_and_preserve_preparation_order() {
    use PlannerMapRoom::*;
    let mut key = node((2, 3), Elite, &[(1, 4)]);
    key.has_emerald_key = true;
    let map = map(vec![
        node((1, 0), Monster, &[(0, 1), (1, 1), (2, 1)]),
        node((0, 1), Rest, &[(0, 2), (1, 2)]),
        node((1, 1), Shop, &[(1, 2), (2, 2)]),
        node((2, 1), Elite, &[(2, 2)]),
        node((0, 2), Shop, &[(0, 3)]),
        node((1, 2), Monster, &[(0, 3), (1, 3), (2, 3)]),
        node((2, 2), Rest, &[(2, 3), (3, 3)]),
        node((0, 3), Elite, &[(1, 4)]),
        node((1, 3), QuestionMark, &[(1, 4)]),
        key,
        // (3,3) is an unpublished endpoint.
        node((1, 4), Boss, &[]),
    ]);
    let reading = PublicRouteReading::new(&map, (1, 0)).unwrap();
    let paths = enumerate((1, 0), &reading.nodes);
    let mut expected = BTreeMap::<Profile, (usize, usize)>::new();
    for path in &paths {
        let (profile, count) = classify(path, &reading.nodes);
        expected
            .entry(profile)
            .and_modify(|range| {
                range.0 = range.0.min(count);
                range.1 = range.1.max(count);
            })
            .or_insert((count, count));
    }
    let actual = suffixes((1, 0), &reading.nodes, &mut BTreeMap::new());
    assert_eq!(actual.len(), expected.len());
    for (profile, bounds) in actual {
        assert_eq!((bounds.min.elites, bounds.max.elites), expected[&profile]);
        for path in [bounds.min, bounds.max] {
            assert!(
                paths.contains(&path.points),
                "a whole connected route, including its end"
            );
            assert_eq!(
                classify(&path.points, &reading.nodes),
                (profile, path.elites)
            );
        }
    }
    let view = route_examples(&map, &[(1, 0)]).unwrap();
    assert!(view.contains("R+S before first elite"));
    assert!(view.contains("no R/S or ")); // Rest after elite is not preparation.
    assert!(view.contains("later Unknown"));
    assert!(view.contains("U(3,3)"));
    let mut reordered = map.clone();
    reordered.nodes.reverse();
    for node in &mut reordered.nodes {
        node.edges.reverse();
    }
    assert_eq!(view, route_examples(&reordered, &[(1, 0), (1, 0)]).unwrap());
}

#[test]
fn compression_factors_real_merge_and_locates_ordinary_fork_exits() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((1, 0), Treasure, &[(0, 1), (2, 1)]),
        node((0, 1), Rest, &[(0, 2)]),
        node((0, 2), Monster, &[(1, 3)]),
        node((2, 1), Elite, &[(2, 2)]),
        node((2, 2), Rest, &[(1, 3)]),
        node((1, 3), Monster, &[(1, 4)]),
        node((1, 4), Boss, &[]),
    ]);
    let reading = PublicRouteReading::new(&map, (1, 0)).unwrap();
    let bounds = Extremes {
        min: Path {
            points: vec![(1, 0), (0, 1), (0, 2), (1, 3), (1, 4)],
            elites: 0,
        },
        max: Path {
            points: vec![(1, 0), (2, 1), (2, 2), (1, 3), (1, 4)],
            elites: 1,
        },
    };
    assert_eq!(
        paired(&bounds, &reading.nodes),
        vec!["T(1,0) [R(0,1) M | E(2,1) R(2,2)] M(1,3) B"]
    );
    let mut multi = map.clone();
    multi
        .nodes
        .iter_mut()
        .find(|n| (n.x, n.y) == (2, 2))
        .unwrap()
        .room = Elite;
    let multi = PublicRouteReading::new(&multi, (1, 0)).unwrap();
    assert_eq!(
        paired(&bounds, &multi.nodes).len(),
        2,
        "multi-elite alternatives remain separate even with a real merge"
    );
    // Same room letters/rows on two legs do not establish a real merge.
    let mut separate = map.clone();
    separate
        .nodes
        .iter_mut()
        .find(|n| (n.x, n.y) == (2, 2))
        .unwrap()
        .edges = vec![PlannerPublicMapEdge {
        destination_x: 2,
        destination_y: 3,
    }];
    separate.nodes.push(node((2, 3), Monster, &[(1, 4)]));
    let separate = PublicRouteReading::new(&separate, (1, 0)).unwrap();
    let mut changed = bounds.clone();
    changed.max.points[3] = (2, 3);
    let lines = paired(&changed, &separate.nodes);
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|line| !line.contains('|')));

    // The displayed branch must locate an ordinary fork as well as its exits
    // and merge. Its coordinate parentheses stay distinct from the alternatives.
    let mut ordinary = map.clone();
    ordinary
        .nodes
        .iter_mut()
        .find(|n| (n.x, n.y) == (1, 0))
        .unwrap()
        .room = Monster;
    let ordinary = PublicRouteReading::new(&ordinary, (1, 0)).unwrap();
    let line = &paired(&bounds, &ordinary.nodes)[0];
    assert_eq!(line, "M(1,0) [R(0,1) M | E(2,1) R(2,2)] M(1,3) B");
}

#[test]
fn shared_approaches_and_continuations_use_one_exact_room_and_keep_each_entrance_outlook() {
    use PlannerMapRoom::*;
    let mut key = node((0, 6), Elite, &[(1, 7)]);
    key.has_emerald_key = true;
    let mut map = map(vec![
        node((0, 0), Monster, &[(0, 1)]),
        node((2, 0), Monster, &[(2, 1)]),
        node((0, 1), Rest, &[(1, 2)]),
        node((2, 1), Shop, &[(1, 2)]),
        node((1, 2), Treasure, &[(1, 3)]),
        node((1, 3), Monster, &[(1, 4)]),
        node((1, 4), Monster, &[(1, 5)]),
        node((1, 5), Monster, &[(0, 6), (2, 6)]),
        key,
        node((2, 6), Monster, &[(1, 7)]),
        node((1, 7), Monster, &[(1, 8)]),
        node((1, 8), Rest, &[(1, 9)]),
        node((1, 9), Monster, &[(1, 10)]),
        node((1, 10), Rest, &[(1, 11)]),
        node((1, 11), Boss, &[]),
    ]);
    map.boss = Some(sts_oracle_learning::content::monsters::factory::EncounterId::TimeEater);
    let reading = PublicRouteReading::new(&map, (0, 0)).unwrap();
    let roots = [(0, 0), (2, 0)];
    let all: BTreeSet<_> = roots
        .iter()
        .flat_map(|root| enumerate(*root, &reading.nodes))
        .collect();
    let mut memo = BTreeMap::new();
    for root in roots {
        suffixes(root, &reading.nodes, &mut memo);
    }
    let view = shared_routes(&map, &roots, &reading.nodes, &memo, &all).unwrap();
    assert!(view.starts_with("Boss: Time Eater\nLegend:"));
    assert!(view.contains("From (0,0)  0-1 elites, key optional, no shop, R before first elite"));
    assert!(
        view.contains("From (2,0)  0-1 elites, key optional, shop reachable, S before first elite")
    );
    let anchor = view
        .lines()
        .find_map(|line| line.strip_prefix("Approaches to "))
        .unwrap();
    let coordinates = anchor.split_once('(').unwrap().1.trim_end_matches("):");
    let (x, y) = coordinates.split_once(',').unwrap();
    let pivot = (x.parse().unwrap(), y.parse().unwrap());
    assert!(!matches!(reading.nodes[&pivot].room, Boss | Victory));
    assert!(view.contains(&format!(
        "After {} (each approach can use these):",
        located(pivot, &reading.nodes)
    )));
    // Independently splice every approach and tail at the displayed point.
    // They must remain real full routes, including every key/rest/shop node.
    let mut prefixes = BTreeSet::new();
    let mut tails = BTreeSet::new();
    for path in &all {
        if let Some(i) = path.iter().position(|p| *p == pivot) {
            prefixes.insert(path[..i].to_vec());
            tails.insert(path[i..].to_vec());
        }
    }
    for prefix in prefixes {
        for tail in &tails {
            let mut path = prefix.clone();
            path.extend(tail);
            assert!(
                all.contains(&path),
                "shared endings must not invent a connection"
            );
        }
    }
    let public = route_examples(&map, &roots).unwrap();
    assert!(public.contains("Approaches to"));
    let mut reordered = map.clone();
    reordered.nodes.reverse();
    assert_eq!(
        public,
        route_examples(&reordered, &[(2, 0), (0, 0)]).unwrap()
    );
}

#[test]
fn equal_room_letters_on_disjoint_legs_do_not_create_a_shared_ending() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((0, 0), Monster, &[(0, 1)]),
        node((2, 0), Monster, &[(2, 1)]),
        node((0, 1), Rest, &[(0, 2)]),
        node((2, 1), Rest, &[(2, 2)]),
        node((0, 2), Elite, &[(0, 3)]),
        node((2, 2), Elite, &[(2, 3)]),
        node((0, 3), Monster, &[(1, 4)]),
        node((2, 3), Monster, &[(1, 4)]),
        node((1, 4), Boss, &[]),
    ]);
    let view = route_examples(&map, &[(0, 0), (2, 0)]).unwrap();
    assert!(!view.contains("Approaches to"));
    assert!(view.contains("R(0,1) E(0,2)"));
    assert!(view.contains("R(2,1) E(2,2)"));
}

#[test]
fn future_roots_exclude_current_room_and_preserve_wing_boots_choices() {
    use PlannerMapRoom::*;
    let mut map = map(vec![
        node((0, 0), Shop, &[(0, 1)]),
        node((0, 1), Monster, &[(0, 2)]),
        node((0, 2), Boss, &[]),
        node((2, 1), Elite, &[(0, 2)]),
    ]);
    map.current_x = 0;
    map.current_y = 0;
    assert_eq!(planning_roots(&map, &[]), vec![(0, 1)]);
    let view = route_examples(&map, &planning_roots(&map, &[])).unwrap();
    assert!(!view.contains("S(0,0)"));
    assert_eq!(
        planning_roots(&map, &[(2, 1), (0, 1)]),
        vec![(0, 1), (2, 1)]
    );
    map.current_y = 2;
    assert!(planning_roots(&map, &[]).is_empty());
}

#[test]
fn exponential_routes_have_bounded_classes_without_path_enumeration() {
    use PlannerMapRoom::*;
    let mut nodes = vec![node((0, 0), Monster, &[(0, 1), (1, 1)])];
    for y in 1..=80 {
        for x in 0..=1 {
            nodes.push(node(
                (x, y),
                if y == 80 { Boss } else { Monster },
                &if y == 80 {
                    vec![]
                } else {
                    vec![(0, y + 1), (1, y + 1)]
                },
            ));
        }
    }
    let map = map(nodes);
    let reading = PublicRouteReading::new(&map, (0, 0)).unwrap();
    let mut memo = BTreeMap::new();
    let classes = suffixes((0, 0), &reading.nodes, &mut memo);
    assert_eq!(memo.len(), 161);
    assert_eq!(classes.len(), 1);
    assert!(memo.values().all(|classes| classes.len() == 1));
    let path = &classes.values().next().unwrap().min.points;
    assert_eq!(path.len(), 81);
    for pair in path.windows(2) {
        assert!(destinations(reading.nodes[&pair[0]]).contains(&pair[1]));
    }
}

#[test]
fn a_single_next_boss_is_named_without_a_route_planning_legend() {
    use PlannerMapRoom::*;
    let mut map = map(vec![
        node((0, 0), Shop, &[(1, 1)]),
        node((1, 1), Elite, &[(5, 14)]),
        node((5, 14), Rest, &[(0, 15)]),
        node((0, 15), Boss, &[]),
    ]);
    map.current_x = 5;
    map.current_y = 14;
    map.boss = Some(sts_oracle_learning::content::monsters::factory::EncounterId::TimeEater);
    let roots = planning_roots(&map, &[]);
    assert_eq!(
        route_examples(&map, &roots).unwrap(),
        "Next room: Boss Time Eater at (0,15)\n"
    );
    map.boss = None;
    assert_eq!(
        route_examples(&map, &roots).unwrap(),
        "Next room: Boss (name not revealed) at (0,15)\n"
    );
    // Two distinct legal entrances still need their separate coordinates.
    map.nodes.push(node((2, 15), Boss, &[]));
    let alternatives = route_examples(&map, &[(0, 15), (2, 15)]).unwrap();
    assert!(alternatives.contains("From (0,15)/(2,15)"));
    assert!(!alternatives.starts_with("Next room:"));
}

#[test]
fn unknown_end_is_not_replaced_by_the_published_boss_name() {
    use PlannerMapRoom::*;
    let mut partial = map(vec![node((0, 0), Rest, &[])]);
    partial.boss = Some(sts_oracle_learning::content::monsters::factory::EncounterId::TimeEater);
    let view = route_examples(&partial, &[(0, 0)]).unwrap();
    assert!(view.starts_with("Boss: Time Eater\nLegend:"));
    assert!(view.contains("later Unknown"));
    assert!(view.contains("R(0,0)"));
    assert!(!view.lines().any(|line| line.trim() == "R(0,0) B"));
    partial.boss = None;
    let hidden = route_examples(&partial, &[(0, 0)]).unwrap();
    assert!(hidden.starts_with("Boss not revealed\nLegend:"));
    assert!(!hidden.contains("Time Eater"));
    let cycle = map(vec![node((0, 0), Monster, &[(0, 0)])]);
    assert!(route_examples(&cycle, &[(0, 0)]).is_err());
}

#[test]
fn local_routes_show_complete_examples_and_the_later_key_commitment() {
    use PlannerMapRoom::*;
    let mut key = node((4, 11), Elite, &[(5, 12)]);
    key.has_emerald_key = true;
    let map = map(vec![
        node((5, 8), Treasure, &[(4, 9), (5, 9)]),
        node((4, 9), Rest, &[(3, 10), (4, 10)]),
        node((5, 9), Elite, &[(5, 10)]),
        node((3, 10), Shop, &[(3, 11)]),
        node((4, 10), Monster, &[(4, 11)]),
        node((5, 10), Rest, &[(4, 11)]),
        node((3, 11), QuestionMark, &[(3, 12)]),
        key,
        node((3, 12), Monster, &[(2, 13)]),
        node((5, 12), Monster, &[(5, 13)]),
        node((2, 13), Monster, &[(2, 14)]),
        node((5, 13), Monster, &[(4, 14)]),
        node((2, 14), Rest, &[(0, 15)]),
        node((4, 14), Rest, &[(0, 15)]),
        node((0, 15), Boss, &[]),
    ]);
    let view = route_from(&map, &[], (5, 8)).unwrap();
    assert!(view.starts_with("Boss not revealed\nRoutes after T(5,8)"));
    assert!(view.contains("R(4,9)  0-1 elites, key optional, shop reachable"));
    assert!(view.contains("E(5,9)  2 elites, key required, no shop"));
    assert!(view.contains("R(4,9) S(3,10) ?MM R(2,14) B"));
    assert!(view.contains("R(4,9) M(4,10) K(4,11) MM R(4,14) B"));
    assert!(view.contains("E(5,9) R(5,10) K(4,11) MM R(4,14) B"));
    assert!(view.contains("R(4,9) -> S(3,10) 0 elites, no key, shop reachable | M(4,10) 1 elite, key required, no shop"));
    assert!(!view.contains("Outside"));
    // Once the rest is the anchor, it is no longer future preparation.
    let after_rest = route_from(&map, &[], (4, 9)).unwrap();
    assert_eq!(after_rest.matches("R(4,9)").count(), 1);
    assert!(!after_rest.contains("key optional"));
    let mut reordered = map.clone();
    reordered.nodes.reverse();
    for node in &mut reordered.nodes {
        node.edges.reverse();
    }
    assert_eq!(view, route_from(&reordered, &[], (5, 8)).unwrap());
}

#[test]
fn local_route_keeps_the_single_exit_corridor_and_excludes_the_anchor_elite() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((0, 0), Elite, &[(0, 1)]),
        node((0, 1), Monster, &[(0, 2)]),
        node((0, 2), Rest, &[(0, 3)]),
        node((0, 3), Elite, &[(0, 4)]),
        node((0, 4), Boss, &[]),
    ]);
    let view = route_from(&map, &[], (0, 0)).unwrap();
    assert!(view.contains("M(0,1)  1 elite, no key, no shop"));
    assert!(view.contains("M R(0,2) E(0,3) B"));
    assert_eq!(view.matches("E(0,0)").count(), 1);
    assert!(!view.contains("Later choices"));
    let end = route_from(&map, &[], (0, 4)).unwrap();
    assert!(end.contains("No next room."));
    assert!(route_from(&map, &[], (9, 9)).is_err());
}

#[test]
fn local_route_unknown_and_disconnected_queries_do_not_invent_choices() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((0, 0), Monster, &[(0, 1)]),
        node((0, 1), Boss, &[]),
        node((2, 0), Rest, &[(2, 1)]),
        node((2, 1), Elite, &[(2, 2)]), // Unpublished end.
    ]);
    let view = route_from(&map, &[(0, 0)], (2, 0)).unwrap();
    assert!(view.contains("Outside the current ordinary reachable routes."));
    assert!(view.contains("at least 1 elite, later Unknown, key Unknown, shop Unknown"));
    assert!(view.contains("E(2,1) U(2,2)"));
    assert!(!view.contains("key optional"));
    assert!(!view.contains("no key"));
    assert!(!view.contains("no shop"));
    assert!(route_from(&map, &[], (2, 1)).unwrap().contains("U(2,2)"));
}

#[test]
fn local_fork_retains_shop_access_after_the_first_elite() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((1, 0), Treasure, &[(1, 1)]),
        node((1, 1), Monster, &[(0, 2), (2, 2)]),
        node((0, 2), Monster, &[(0, 3)]),
        node((2, 2), Monster, &[(2, 3)]),
        node((0, 3), Elite, &[(0, 4)]),
        node((2, 3), Elite, &[(2, 4)]),
        node((0, 4), Shop, &[(1, 5)]),
        node((2, 4), Monster, &[(1, 5)]),
        node((1, 5), Boss, &[]),
    ]);
    let reading = PublicRouteReading::new(&map, (1, 1)).unwrap();
    let mut memo = BTreeMap::new();
    let a = suffixes((0, 2), &reading.nodes, &mut memo);
    let b = suffixes((2, 2), &reading.nodes, &mut memo);
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    let view = route_from(&map, &[], (1, 0)).unwrap();
    assert!(view.contains(
        "M(1,1) -> M(0,2) 1 elite, no key, shop reachable | M(2,2) 1 elite, no key, no shop"
    ));
}

#[test]
fn local_fork_explains_preparation_when_the_counts_are_equal() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((1, 0), Treasure, &[(1, 1)]),
        node((1, 1), Monster, &[(0, 2), (2, 2)]),
        node((0, 2), Monster, &[(0, 3)]),
        node((2, 2), Monster, &[(2, 3)]),
        node((0, 3), Rest, &[(1, 4)]),
        node((2, 3), Monster, &[(1, 4)]),
        node((1, 4), Elite, &[(1, 5)]),
        node((1, 5), Boss, &[]),
    ]);
    let view = route_from(&map, &[], (1, 0)).unwrap();
    assert!(view.contains("M(1,1) -> M(0,2) 1 elite, no key, no shop, R before first elite | M(2,2) 1 elite, no key, no shop, no R/S before first elite"));
}
