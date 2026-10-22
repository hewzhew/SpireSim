use super::*;
use sts_oracle_learning::ai::planner_core::PlannerPublicMapEdge;
use sts_oracle_learning::content::monsters::factory::EncounterId;

#[test]
fn localized_maps_preserve_edges_ranges_and_preparation_paths() {
    use PlannerMapRoom::*;
    let mut burning = node((2, 2), Elite, &[(1, 3)]);
    burning.has_emerald_key = true;
    let mut map = map(vec![
        node((1, 0), Monster, &[(0, 1), (2, 1)]),
        node((0, 1), Rest, &[(0, 2)]),
        node((0, 2), Monster, &[(1, 3)]),
        node((2, 1), Shop, &[(2, 2)]),
        burning,
        node((1, 3), Boss, &[]),
    ]);
    map.current_x = 1;
    map.current_y = 0;
    map.boss = Some(EncounterId::SlimeBoss);
    let before = serde_json::to_value(&map).unwrap();
    let en = PublicRouteReading::new(&map, (1, 0), Locale::En).unwrap();
    let zh = PublicRouteReading::new(&map, (1, 0), Locale::ZhCn).unwrap();
    assert_eq!(en.reachable, zh.reachable);
    assert_eq!(en.segments, zh.segments);
    assert_eq!(
        en.preparation_paths
            .iter()
            .map(|path| (&path.nodes, path.endpoint))
            .collect::<Vec<_>>(),
        zh.preparation_paths
            .iter()
            .map(|path| (&path.nodes, path.endpoint))
            .collect::<Vec<_>>()
    );
    assert!(en.summary().contains("Elites=0..1"));
    assert!(zh.summary().contains("精英数=0..1"));
    assert!(zh.summary().contains("燃烧精英：部分路线有"));
    assert!(zh.summary().contains("商店"));
    assert_lossless_edges(&zh);
    assert_eq!(
        assert_connected_preparation_examples(&en),
        assert_connected_preparation_examples(&zh)
    );
    let roots = [(0, 1), (2, 1)];
    let route = route_from(&map, &roots, (1, 0), Locale::ZhCn).unwrap();
    assert!(route.contains("Boss：史莱姆老大"), "{route}");
    assert!(route.contains("必经绿钥匙精英"));
    assert!(route.contains("无绿钥匙精英"));
    let prepared = preparation_routes(&map, Locale::ZhCn).unwrap().unwrap();
    assert!(prepared.contains("当前房间 (1,0) 战斗之后"));
    assert_eq!(serde_json::to_value(&map).unwrap(), before);
}

#[test]
fn one_grid_owner_preserves_connections_bands_and_direction_in_both_languages() {
    use PlannerMapRoom::*;
    let mut map = map(vec![
        node((1, 0), Monster, &[(0, 8), (2, 8)]),
        node((0, 8), Treasure, &[(0, 14)]),
        node((2, 8), Treasure, &[(0, 14)]),
        node((0, 14), Rest, &[(0, 15)]),
        node((0, 15), Boss, &[]),
    ]);
    map.current_x = 1;
    map.current_y = 0;
    for locale in [Locale::En, Locale::ZhCn] {
        let up = map_grid(&map, &[(0, 8), (2, 8)], locale, false);
        let down = map_grid(&map, &[(0, 8), (2, 8)], locale, true);
        assert!(up.find("y15").unwrap() < up.find("y00").unwrap());
        assert!(down.find("y00").unwrap() < down.find("y15").unwrap());
        for grid in [&up, &down] {
            assert!(grid.contains("*M(1,0)->(0,8)/(2,8)"));
            assert!(grid.contains(">T(0,8)->(0,14)"));
            assert!(grid.contains(">T(2,8)->(0,14)"));
            assert!(grid.contains("R(0,14)->(0,15)"));
        }
        if locale == Locale::ZhCn {
            assert!(up.contains("宝箱层"));
            assert!(up.contains("Boss 前营火层"));
            assert!(down.contains("向下显示后续路线"));
        }
    }
}

#[test]
fn localized_partial_maps_do_not_promote_unpublished_nodes_or_hidden_bosses() {
    use PlannerMapRoom::*;
    let mut map = map(vec![node((0, 0), Monster, &[(1, 1)])]);
    map.boss = None;
    let zh = PublicRouteReading::new(&map, (0, 0), Locale::ZhCn).unwrap();
    assert!(zh.summary().contains("后续未公开"));
    assert!(zh.detail().contains("尚未公开的终点"));
    let routes = route_examples(&map, &[(0, 0)], Locale::ZhCn).unwrap();
    assert!(routes.contains("Boss 尚未揭晓"));
    assert!(!routes.contains("史莱姆老大"));
    assert!(!routes.contains("B("));
}

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

fn assert_lossless_edges(reading: &PublicRouteReading<'_>) {
    let expected: BTreeSet<_> = reading
        .reachable
        .iter()
        .flat_map(|point| {
            reading.nodes.get(point).into_iter().flat_map(move |node| {
                destinations(node)
                    .into_iter()
                    .map(move |next| (*point, next))
            })
        })
        .collect();
    let edges: Vec<_> = reading
        .segments
        .iter()
        .flat_map(|segment| segment.windows(2).map(|pair| (pair[0], pair[1])))
        .collect();
    let observed: BTreeSet<_> = edges.iter().copied().collect();
    assert_eq!(observed, expected);
    assert_eq!(
        edges.len(),
        expected.len(),
        "no edge should be enumerated once per complete path"
    );
}

fn assert_connected_preparation_examples(reading: &PublicRouteReading<'_>) -> usize {
    let view = reading.preparation_windows();
    let mut examples = 0;
    for path in &reading.preparation_paths {
        let points = &path.nodes;
        let rendered = points
            .iter()
            .map(|point| reading.label(*point))
            .collect::<Vec<_>>()
            .join(" -> ");
        assert!(view.contains(&rendered));
        assert_eq!(points[0], reading.root);
        for pair in points.windows(2) {
            assert!(destinations(reading.nodes[&pair[0]]).contains(&pair[1]));
            assert!(!matches!(
                reading.nodes[&pair[0]].room,
                PlannerMapRoom::Elite | PlannerMapRoom::Boss | PlannerMapRoom::Victory
            ));
        }
        examples += 1;
    }
    examples
}

#[test]
fn preparation_examples_do_not_combine_mutually_exclusive_rest_and_shop() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((1, 0), Monster, &[(0, 1), (2, 1)]),
        node((0, 1), Rest, &[(1, 2)]),
        node((2, 1), Shop, &[(1, 2)]),
        node((1, 2), QuestionMark, &[(1, 3)]),
        node((1, 3), Elite, &[(1, 4)]),
        node((1, 4), Boss, &[]),
    ]);
    let reading = PublicRouteReading::new(&map, (1, 0), sts_presentation::Locale::En).unwrap();
    let view = reading.preparation_windows();
    assert_eq!(assert_connected_preparation_examples(&reading), 2);
    assert!(view.contains("approach: 1 Monster, 1 ?; Rest (0,1); Shop none"));
    assert!(view.contains("approach: 1 Monster, 1 ?; Rest none; Shop (2,1)"));
    assert!(!view.contains("Rest (0,1); Shop (2,1)"));
    assert!(!view.contains("(1,4)"));
    let queried = elite_routes(&map, &[(1, 0)], &[], sts_presentation::Locale::En).unwrap();
    assert!(queried.contains(&view.replace('\n', "\n    ")));
}

#[test]
fn preparation_examples_keep_direct_and_prepared_approaches_to_the_same_elite() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((1, 0), Monster, &[(2, 1), (0, 1)]),
        node((0, 1), Rest, &[(1, 2)]),
        node((2, 1), Monster, &[(1, 2)]),
        node((1, 2), Elite, &[]),
    ]);
    let reading = PublicRouteReading::new(&map, (1, 0), sts_presentation::Locale::En).unwrap();
    let view = reading.preparation_windows();
    assert_eq!(assert_connected_preparation_examples(&reading), 2);
    assert_eq!(view.matches("First Elite: (1,2)").count(), 2);
    assert!(view.contains("approach: 2 Monster, 0 ?; Rest none; Shop none"));
    assert!(view.contains("approach: 1 Monster, 0 ?; Rest (0,1); Shop none"));

    let mut reordered = map.clone();
    reordered.nodes.reverse();
    for node in &mut reordered.nodes {
        node.edges.reverse();
    }
    assert_eq!(
        view,
        PublicRouteReading::new(&reordered, (1, 0), sts_presentation::Locale::En)
            .unwrap()
            .preparation_windows()
    );
}

#[test]
fn preparation_examples_distinguish_a_boss_bypass_from_an_unpublished_suffix() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((1, 0), Monster, &[(0, 1), (1, 1), (2, 1)]),
        node((0, 1), Shop, &[(0, 2)]),
        node((0, 2), Elite, &[]),
        node((1, 1), Rest, &[(1, 2)]),
        node((1, 2), Boss, &[(1, 3)]),
        node((1, 3), Shop, &[]),
        node((2, 1), QuestionMark, &[(2, 2)]), // Unpublished destination.
    ]);
    let reading = PublicRouteReading::new(&map, (1, 0), sts_presentation::Locale::En).unwrap();
    let view = reading.preparation_windows();
    assert_eq!(assert_connected_preparation_examples(&reading), 3);
    assert!(view.contains("First Elite: (0,2) Elite"));
    assert!(view.contains("Elite-free prefix to endpoint: (1,2) Boss"));
    assert!(view.contains("Elite-free prefix to endpoint: (2,2) Unknown [unpublished endpoint]"));
    assert!(!view.contains("(1,3)"));
}

#[test]
fn preparation_routes_exclude_current_and_unconnected_shops() {
    use PlannerMapRoom::*;
    let mut map = map(vec![
        node((5, 2), Shop, &[(4, 3)]),
        node((4, 3), Monster, &[(5, 10)]),
        node((5, 10), Shop, &[(0, 15)]),
        node((5, 3), Shop, &[(6, 4)]),
        node((6, 4), Boss, &[]),
        node((0, 15), Boss, &[]),
    ]);
    map.current_x = 5;
    map.current_y = 2;
    let view = preparation_routes(&map, sts_presentation::Locale::En)
        .unwrap()
        .unwrap();
    assert!(view.contains("current room (5,2) Shop"));
    assert!(view.contains("via (4,3) Monster: Elites=0; shop map y10 reachable"));
    assert!(!view.contains("shop map y2 reachable"));
    assert!(!view.contains("(5,3)"));
    assert!(!view.contains("shop map y3 reachable"));
    assert!(view.contains("Boss (0,15) reachable"));
    assert!(!view.contains("(6,4)"));
    assert!(view.contains("Rest none; Shop (5,10); path: (4,3) Monster -> (5,10) Shop"));
    assert!(!view.contains("Shop (5,2)"));

    map.current_x = 0;
    map.current_y = 15;
    assert!(preparation_routes(&map, sts_presentation::Locale::En)
        .unwrap()
        .is_none());
    map.current_x = 5;
    map.current_y = 9;
    assert!(preparation_routes(&map, sts_presentation::Locale::En)
        .unwrap()
        .is_none());
}

#[test]
fn preparation_routes_keep_branch_opportunities_separate() {
    use PlannerMapRoom::*;
    let mut map = map(vec![
        node((0, 0), Rest, &[(0, 1), (1, 1)]),
        node((0, 1), Shop, &[(0, 2)]),
        node((0, 2), Boss, &[]),
        node((1, 1), Elite, &[(1, 2)]),
    ]);
    map.current_x = 0;
    map.current_y = 0;
    let view = preparation_routes(&map, sts_presentation::Locale::En)
        .unwrap()
        .unwrap();
    assert!(view.contains("via (0,1) Shop: Elites=0; shop map y1 reachable"));
    assert!(view.contains("via (1,1) Elite: Elites>=1 [later Unknown]; shop Unknown"));
    assert!(view.contains("Boss (0,2) reachable"));
    assert!(view.contains("Boss Unknown beyond published map"));
    assert!(view.contains("After leaving this room, choose one next branch."));
    assert_eq!(
        view.lines()
            .filter(|line| line.starts_with("  via "))
            .count(),
        2
    );
}

#[test]
fn first_landmarks_preserve_the_difference_between_rest_before_and_after_elite() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((1, 0), Monster, &[(0, 1), (2, 1)]),
        node((0, 1), QuestionMark, &[(0, 2)]),
        node((0, 2), Rest, &[(0, 3)]),
        node((0, 3), Elite, &[(1, 4)]),
        node((2, 1), Monster, &[(2, 2)]),
        node((2, 2), Elite, &[(2, 3)]),
        node((2, 3), Rest, &[(1, 4)]),
        node((1, 4), Shop, &[]),
    ]);
    let reading = PublicRouteReading::new(&map, (1, 0), sts_presentation::Locale::En).unwrap();
    assert_eq!(reading.first_landmarks, vec![(0, 2), (2, 2)]);
    assert_eq!(
        reading.segments,
        vec![
            vec![(1, 0), (0, 1), (0, 2), (0, 3), (1, 4)],
            vec![(1, 0), (2, 1), (2, 2), (2, 3), (1, 4)],
        ]
    );
    assert_lossless_edges(&reading);
    let detail = reading.detail();
    assert!(detail.contains("(0,2) Rest → (0,3) Elite"));
    assert!(detail.contains("(2,2) Elite → (2,3) Rest"));
    assert!(detail.contains("(0,1) ?"));
    assert!(
        !reading.summary().contains("(1,4)"),
        "later shop is not a first landmark"
    );
    let preparation = reading.preparation_windows();
    assert_eq!(assert_connected_preparation_examples(&reading), 2);
    assert!(preparation.contains("Rest (0,2); Shop none"));
    assert!(!preparation.contains("(2,3)")); // Rest after the other first elite.
    assert!(!preparation.contains("(1,4)")); // Shop after either first elite.
}

#[test]
fn reconverging_branches_retain_identity_without_repeating_downstream_graphs() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((1, 0), Monster, &[(0, 1), (2, 1)]),
        node((0, 1), Shop, &[(1, 2)]),
        node((2, 1), Monster, &[(1, 2)]),
        node((1, 2), QuestionMark, &[(1, 3)]),
        node((1, 3), Rest, &[(0, 4), (2, 4)]),
        node((0, 4), Elite, &[]),
        node((2, 4), Monster, &[]),
        node((6, 0), Shop, &[]), // An unrelated map cell is not a branch of this target.
    ]);
    let reading = PublicRouteReading::new(&map, (1, 0), sts_presentation::Locale::En).unwrap();
    assert_eq!(
        reading
            .segments
            .iter()
            .filter(|s| s.windows(2).any(|p| p == [(1, 2), (1, 3)]))
            .count(),
        1
    );
    assert_eq!(reading.first_landmarks, vec![(0, 1), (1, 3)]);
    assert!(!reading.reachable.contains(&(6, 0)));
    assert_lossless_edges(&reading);
    let reordered = PlannerPublicMap {
        nodes: map.nodes.iter().cloned().rev().collect(),
        ..map.clone()
    };
    assert_eq!(
        reading.detail(),
        PublicRouteReading::new(&reordered, (1, 0), sts_presentation::Locale::En)
            .unwrap()
            .detail()
    );
}

#[test]
fn explicit_target_works_at_entrance_or_after_flight_without_inventing_future_jumps() {
    use PlannerMapRoom::*;
    let mut map = map(vec![
        node((0, 0), Monster, &[(0, 1)]),
        node((0, 1), Rest, &[]),
        node((5, 1), Elite, &[]),
    ]);
    assert_eq!(
        PublicRouteReading::new(&map, (0, 0), sts_presentation::Locale::En)
            .unwrap()
            .first_landmarks,
        vec![(0, 1)]
    );
    map.current_x = 6;
    map.current_y = 0; // The explicit candidate can land away from the ordinary path.
    let reading = PublicRouteReading::new(&map, (5, 1), sts_presentation::Locale::En).unwrap();
    assert_eq!(reading.first_landmarks, vec![(5, 1)]);
    assert_eq!(reading.reachable, BTreeSet::from([(5, 1)]));
    assert_eq!(reading.segments, vec![vec![(5, 1)]]);
    assert_lossless_edges(&reading);
}

#[test]
fn unknown_destinations_and_recorded_leaves_are_not_promoted_to_a_boss_or_victory() {
    use PlannerMapRoom::*;
    let mut map = map(vec![node((2, 14), Rest, &[(3, 16)])]);
    map.boss = Some(EncounterId::SlimeBoss);
    let reading = PublicRouteReading::new(&map, (2, 14), sts_presentation::Locale::En).unwrap();
    assert!(reading
        .detail()
        .contains("(3,16) Unknown [unpublished endpoint]"));
    assert_eq!(reading.first_landmarks, vec![(2, 14)]);
    assert_lossless_edges(&reading);
    assert!(!reading.single_path().unwrap().contains("SlimeBoss"));
    map.nodes[0].edges.clear();
    assert!(
        PublicRouteReading::new(&map, (2, 14), sts_presentation::Locale::En)
            .unwrap()
            .detail()
            .contains("Rest [recorded endpoint]")
    );
    map.nodes.push(node((0, 15), Boss, &[]));
    assert!(
        PublicRouteReading::new(&map, (0, 15), sts_presentation::Locale::En)
            .unwrap()
            .detail()
            .contains("Boss [recorded endpoint]")
    );
    let absent = PublicRouteReading::new(&map, (9, 9), sts_presentation::Locale::En).unwrap();
    assert_eq!(absent.first_landmarks, vec![(9, 9)]);
    assert!(absent.summary().contains("Unknown"));
}

#[test]
fn key_is_kept_on_its_exact_node_and_can_be_a_first_landmark() {
    let mut key = node((0, 1), PlannerMapRoom::Monster, &[(0, 2)]);
    key.has_emerald_key = true; // Even unexpected constructed room kinds retain public flags.
    let map = map(vec![
        node((0, 0), PlannerMapRoom::Monster, &[(0, 1)]),
        key,
        node((0, 2), PlannerMapRoom::Rest, &[]),
    ]);
    let reading = PublicRouteReading::new(&map, (0, 0), sts_presentation::Locale::En).unwrap();
    assert_eq!(reading.first_landmarks, vec![(0, 1)]);
    assert!(reading.detail().contains("(0,1) Monster +Emerald key"));
}

#[test]
fn cyclic_or_ambiguous_topology_fails_instead_of_dropping_branches() {
    use PlannerMapRoom::*;
    for nodes in [
        vec![node((0, 0), Monster, &[(0, 0)])],
        vec![
            node((0, 0), Monster, &[(0, 1)]),
            node((0, 1), Rest, &[(0, 0)]),
        ],
        vec![node((0, 0), Monster, &[(0, 1), (0, 1)])],
        vec![node((0, 0), Monster, &[]), node((0, 0), Rest, &[])],
    ] {
        assert!(
            PublicRouteReading::new(&map(nodes), (0, 0), sts_presentation::Locale::En).is_err()
        );
    }
}

#[test]
fn exponentially_many_complete_paths_are_read_as_a_linear_size_graph() {
    use PlannerMapRoom::*;
    // Two nodes per layer with full cross-links: 2^80 complete paths, but
    // only 161 published nodes. No timing threshold or truncated search budget.
    let mut nodes = vec![node((0, 0), Monster, &[(0, 1), (1, 1)])];
    for y in 1..=80 {
        for x in 0..=1 {
            let edges = if y == 80 {
                vec![]
            } else {
                vec![(0, y + 1), (1, y + 1)]
            };
            nodes.push(node((x, y), if y == 80 { Rest } else { Monster }, &edges));
        }
    }
    let map = map(nodes);
    let reading = PublicRouteReading::new(&map, (0, 0), sts_presentation::Locale::En).unwrap();
    assert_eq!(reading.reachable.len(), 161);
    assert_eq!(reading.first_landmarks, vec![(0, 80), (1, 80)]);
    assert_eq!(reading.segments.len(), 318);
    assert_lossless_edges(&reading);
    assert_eq!(assert_connected_preparation_examples(&reading), 2);
    assert_eq!(reading.preparation_windows().lines().count(), 3);
}

#[test]
fn map_queries_count_elites_on_one_path_and_keep_branch_evidence() {
    use PlannerMapRoom::*;
    let mut key = node((2, 2), Elite, &[(1, 3)]);
    key.has_emerald_key = true;
    let map = map(vec![
        node((1, 0), Monster, &[(0, 1), (2, 1)]),
        node((0, 1), Elite, &[(0, 2)]),
        node((0, 2), Rest, &[(1, 3)]),
        node((2, 1), Elite, &[(2, 2)]),
        key,
        node((1, 3), Boss, &[]),
    ]);
    let (from, roots, _) = next_nodes(&map, &[(1, 0)], None).unwrap();
    assert_eq!(from, (-1, -1));
    assert_eq!(roots, vec![(1, 0)]);
    assert_eq!(next_nodes(&map, &[], None).unwrap().1, vec![(1, 0)]);
    let (_, fork, _) = next_nodes(&map, &[], Some((1, 0))).unwrap();
    assert_eq!(fork, vec![(0, 1), (2, 1)]);
    let routes = elite_routes(&map, &fork, &[(1, 0)], sts_presentation::Locale::En).unwrap();
    assert!(routes
        .contains("2,1E: Elites=2; Burning elites=1; burning elite unavoidable on this branch\n    Most-elite route: first elite map y1"));
    assert!(routes.contains("via 2,1E → 2,2K → 1,3B"));
    assert!(routes
        .contains("0,1E: Elites=1; Burning elites=0\n    Most-elite route: first elite map y1"));
    assert!(routes.contains("via 0,1E → 0,2R → 1,3B"));
    assert_eq!(routes.matches("1,3B").count(), 2); // shared boss is on each distinct path
    let grid = map_grid(&map, &[(1, 0)], sts_presentation::Locale::En, false);
    assert!(grid.contains("y00 >M(1,0)->(0,1)/(2,1)"));
    assert!(grid.contains("y02 R(0,2)->(1,3) K(2,2)->(1,3)"));
    assert!(reachable_from_current(&map, &[(1, 0)], (2, 2)).unwrap());
    assert!(!reachable_from_current(&map, &[(1, 0)], (5, 5)).unwrap());
}

#[test]
fn minimum_elite_witness_exposes_a_bypass_before_the_first_rest() {
    use PlannerMapRoom::*;
    let mut key = node((6, 9), Elite, &[(6, 10)]);
    key.has_emerald_key = true;
    let map = map(vec![
        node((3, 3), QuestionMark, &[(2, 4), (4, 4)]),
        node((2, 4), QuestionMark, &[(3, 5)]),
        node((3, 5), QuestionMark, &[(4, 6)]),
        node((4, 4), Monster, &[(4, 5)]),
        node((4, 5), Elite, &[(4, 6)]),
        node((4, 6), Rest, &[(5, 7)]),
        node((5, 7), Treasure, &[(5, 8)]),
        node((5, 8), Monster, &[(5, 9), (6, 9)]),
        node((5, 9), Monster, &[(6, 10)]),
        key,
        node((6, 10), Boss, &[]),
    ]);
    let routes = elite_routes(&map, &[(3, 3)], &[], sts_presentation::Locale::En).unwrap();
    assert!(routes.contains(
        "3,3?: Elites=0..2; Burning elites=0..1; burning elite avoidable on this branch"
    ));
    assert!(routes.contains("via 3,3? → 4,4M → 4,5E → 4,6R → 5,7T → 5,8M → 6,9K → 6,10B"));
    assert!(routes.contains("Fewest-elite route: Elites=0; Burning elites=0; first elite: none on this path; via 3,3? → 2,4? → 3,5? → 4,6R → 5,7T → 5,8M → 5,9M → 6,10B"));
    let reordered = PlannerPublicMap {
        nodes: map.nodes.iter().cloned().rev().collect(),
        ..map.clone()
    };
    assert_eq!(
        routes,
        elite_routes(&reordered, &[(3, 3)], &[], sts_presentation::Locale::En).unwrap()
    );
}

#[test]
fn branch_ranges_distinguish_required_key_from_optional_elite_after_merge() {
    use PlannerMapRoom::*;
    let mut key = node((0, 1), Elite, &[(0, 2), (2, 2)]);
    key.has_emerald_key = true;
    let map = map(vec![
        node((0, 0), Rest, &[(0, 1)]),
        key,
        node((0, 2), Monster, &[(1, 3)]),
        node((2, 2), Elite, &[(1, 3)]),
        node((1, 3), Rest, &[(1, 4)]),
        node((1, 4), Boss, &[]),
    ]);
    let routes = elite_routes(&map, &[(0, 1)], &[(0, 0)], sts_presentation::Locale::En).unwrap();
    assert!(routes
        .contains("0,1K: Elites=1..2; Burning elites=1; burning elite unavoidable on this branch"));
    assert!(routes.contains("first elite map y1; before M0 ?0, last rest map y0; next rest map y3"));
    assert!(routes.contains("via 0,1K → 2,2E → 1,3R → 1,4B"));
}

#[test]
fn equal_first_treasures_expose_different_shop_and_elite_outlooks() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((1, 0), Monster, &[(0, 1), (2, 1)]),
        node((0, 1), Treasure, &[(0, 2)]),
        node((0, 2), Rest, &[(0, 3)]),
        node((0, 3), Elite, &[(1, 4)]),
        node((2, 1), Treasure, &[(2, 2)]),
        node((2, 2), Monster, &[(2, 3)]),
        node((2, 3), Shop, &[(1, 4)]),
        node((1, 4), Boss, &[]),
    ]);
    let left = PublicRouteReading::new(&map, (0, 1), sts_presentation::Locale::En)
        .unwrap()
        .summary();
    let right = PublicRouteReading::new(&map, (2, 1), sts_presentation::Locale::En)
        .unwrap()
        .summary();
    assert!(left.contains("Elites=1; shop none; First landmarks: (0,1) Treasure"));
    assert!(right.contains("Elites=0; shop map y3 reachable; First landmarks: (2,1) Treasure"));
}

#[test]
fn unpublished_suffix_keeps_shop_and_elite_outlook_unknown() {
    let map = map(vec![node((2, 0), PlannerMapRoom::Treasure, &[(2, 1)])]);
    let summary = PublicRouteReading::new(&map, (2, 0), sts_presentation::Locale::En)
        .unwrap()
        .summary();
    assert!(summary.contains("Elites>=0 [later Unknown]; shop Unknown"));
}

#[test]
fn incomplete_map_does_not_claim_an_elite_or_key_is_avoidable() {
    use PlannerMapRoom::*;
    let mut key = node((0, 2), Elite, &[(0, 3)]);
    key.has_emerald_key = true;
    let map = map(vec![
        node((0, 0), Monster, &[(0, 1), (2, 1)]),
        node((0, 1), QuestionMark, &[(0, 2)]),
        key,
        node((0, 3), Boss, &[]),
        node((2, 1), Monster, &[(2, 2)]),
        // (2,2) is unpublished; it may lead to more elite/key rooms.
    ]);
    let routes = elite_routes(
        &map,
        &[(0, 1), (2, 1)],
        &[(0, 0)],
        sts_presentation::Locale::En,
    )
    .unwrap();
    assert!(routes
        .contains("0,1?: Elites=1; Burning elites=1; burning elite unavoidable on this branch"));
    assert!(routes.contains("2,1M: Elites>=0; Burning elites>=0 [Unknown beyond published map]"));
    assert!(!routes.contains("burning elite avoidable"));
}

#[test]
fn complete_fork_marks_key_optional_when_one_route_bypasses_it() {
    use PlannerMapRoom::*;
    let mut key = node((0, 1), Elite, &[(1, 2)]);
    key.has_emerald_key = true;
    let map = map(vec![
        node((1, 0), Monster, &[(0, 1), (2, 1)]),
        key,
        node((2, 1), Monster, &[(1, 2)]),
        node((1, 2), Boss, &[]),
    ]);
    let routes = elite_routes(&map, &[(1, 0)], &[], sts_presentation::Locale::En).unwrap();
    assert!(routes.contains(
        "1,0M: Elites=0..1; Burning elites=0..1; burning elite avoidable on this branch"
    ));
    assert!(routes.contains("via 1,0M → 0,1K → 1,2B"));
}

#[test]
fn route_query_skips_a_forced_corridor_to_the_next_fork() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((0, 0), Monster, &[(0, 1)]),
        node((0, 1), Shop, &[(1, 2)]),
        node((1, 2), Rest, &[(0, 3), (2, 3)]),
        node((0, 3), Elite, &[]),
        node((2, 3), Monster, &[]),
    ]);
    assert_eq!(
        next_nodes(&map, &[], Some((0, 0))).unwrap(),
        ((1, 2), vec![(0, 3), (2, 3)], vec![(0, 0), (0, 1), (1, 2)])
    );
    assert_eq!(
        next_nodes(&map, &[], Some((0, 1))).unwrap(),
        ((1, 2), vec![(0, 3), (2, 3)], vec![(0, 1), (1, 2)])
    );
    assert_eq!(
        format_corridor(&map, &[(0, 0), (0, 1), (1, 2)]),
        "0,0M → 0,1S → 1,2R"
    );
}

#[test]
fn recovery_alternatives_are_not_limited_to_the_maximum_elite_witness() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node((1, 0), Rest, &[(1, 1)]),
        node((1, 1), Elite, &[(1, 2)]),
        node((1, 2), Treasure, &[(0, 3), (2, 3)]),
        node((0, 3), Monster, &[(0, 4)]),
        node((0, 4), Elite, &[(1, 5)]),
        node((2, 3), Rest, &[(1, 5)]),
        node((1, 5), Rest, &[(1, 6)]),
        node((1, 6), Boss, &[]),
    ]);
    let routes = elite_routes(&map, &[(1, 1)], &[(1, 0)], sts_presentation::Locale::En).unwrap();
    assert!(routes.contains("1,1E: Elites=1..2; Burning elites=0"));
    assert!(routes.contains(
        "Most-elite route: first elite map y1; before M0 ?0, last rest map y0; next rest map y5"
    ));
    assert!(routes.contains("Rest map y3 (2,3) via 1,1E → 1,2T → 2,3R"));
    assert!(routes.contains("Elite map y4 (0,4) via 1,1E → 1,2T → 0,3M → 0,4E"));
    assert!(
        !routes.contains("Rest map y5 (1,5)"),
        "the frontier stops before a second landmark"
    );
    // Querying after the same elite puts it in the shared corridor. Its
    // contextual timing is not a claim that it remains on every branch.
    let (_, roots, corridor) = next_nodes(&map, &[], Some((1, 1))).unwrap();
    let routes = elite_routes(&map, &roots, &corridor, sts_presentation::Locale::En).unwrap();
    assert!(routes
        .contains("2,3R: Elites=0; Burning elites=0\n    Most-elite route: first elite map y1"));
    assert!(routes.contains("Route timing includes the query anchor and shared corridor."));
    let rest_branch = routes.split("2,3R: Elites=0").nth(1).unwrap();
    assert!(rest_branch.contains("across all continuations: Rest map y3 (2,3)"));
    assert!(!rest_branch.contains("Elite map y4 (0,4)"));
}
