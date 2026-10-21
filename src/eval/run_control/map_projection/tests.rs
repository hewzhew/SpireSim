use super::*;
use crate::ai::planner_core::PlannerAction;
use crate::eval::run_control::{capture_planner_boundary_yield_v1, PlannerBoundaryYieldKindV1};
use crate::state::core::EngineState;
use crate::state::map::{MapEdge, MapRoomNode, MapState, RoomType};

fn standard_end() -> RunControlSession {
    let mut session = RunControlSession::new(Default::default());
    let mut graph: Vec<Vec<MapRoomNode>> = (0..15)
        .map(|y| (0..7).map(|x| MapRoomNode::new(x, y)).collect())
        .collect();
    graph[13][1].class = Some(RoomType::MonsterRoom);
    graph[13][1].edges.insert(MapEdge::new(1, 13, 2, 14));
    graph[14][2].class = Some(RoomType::RestRoom);
    graph[14][2].edges.insert(MapEdge::new(2, 14, 3, 16));
    // Java assigns rooms to unused matrix cells, but does not display them.
    graph[0][6].class = Some(RoomType::MonsterRoom);
    graph[14][6].class = Some(RoomType::RestRoom);
    session.run_state.map = MapState::new(graph);
    session.engine_state = EngineState::MapNavigation;
    session
}

fn at(map: &PlannerPublicMap, x: i32, y: i32) -> &PlannerPublicMapNode {
    map.nodes
        .iter()
        .find(|node| node.x == x && node.y == y)
        .unwrap()
}

#[test]
fn actor_navigation_connects_visible_final_rooms_to_the_canonical_boss_from_the_start() {
    let session = standard_end();
    let public = planner_public_map_v1(&session);
    assert_eq!((public.current_x, public.current_y), (-1, -1));
    assert_eq!(public.nodes.len(), 3);
    assert_eq!(at(&public, 0, 15).room, PlannerMapRoom::Boss);
    assert_eq!(
        at(&public, 2, 14).edges,
        vec![PlannerPublicMapEdge {
            destination_x: 0,
            destination_y: 15
        }]
    );
    assert!(!public.nodes.iter().any(|node| node.x == 6));
    assert!(public
        .nodes
        .iter()
        .flat_map(|node| &node.edges)
        .all(|edge| public
            .nodes
            .iter()
            .any(|node| node.x == edge.destination_x && node.y == edge.destination_y)));
}

#[test]
fn source_comparison_retains_unused_assigned_cells_and_the_original_anchor() {
    let session = standard_end();
    let before = serde_json::to_value((
        &session.run_state.map,
        &session.run_state.map_rng,
        &session.run_state.rng_pool,
    ))
    .unwrap();
    let source = run_map_comparison_topology_v1(&session);
    let public = planner_public_map_v1(&session);
    assert_eq!(source.nodes.len(), 4);
    assert!(source.nodes.iter().any(|node| (node.x, node.y) == (6, 14)));
    let final_room = source
        .nodes
        .iter()
        .find(|node| (node.x, node.y) == (2, 14))
        .unwrap();
    assert_eq!(
        final_room.edges,
        vec![PlannerPublicMapEdge {
            destination_x: 3,
            destination_y: 16
        }]
    );
    assert!(!source.nodes.iter().any(|node| (node.x, node.y) == (0, 15)));
    assert_ne!(source.nodes, public.nodes);
    // MapRoomNode's Eq only compares coordinates, so verify serialized fields,
    // including edges, room classes and RNG, rather than Map equality.
    assert_eq!(
        serde_json::to_value((
            &session.run_state.map,
            &session.run_state.map_rng,
            &session.run_state.rng_pool
        ))
        .unwrap(),
        before
    );
}

#[test]
fn current_boss_action_targets_one_published_node_and_comparison_stays_source_oriented() {
    let mut session = standard_end();
    session.run_state.map.current_x = 2;
    session.run_state.map.current_y = 14;
    let capture =
        capture_planner_boundary_yield_v1(&session, PlannerBoundaryYieldKindV1::CallbackStop)
            .unwrap()
            .visits
            .remove(0);
    assert!(capture
        .legal_candidate_set
        .candidates
        .iter()
        .any(|candidate| matches!(
            candidate.action,
            PlannerAction::ChooseRouteNode {
                x: 0,
                y: 15,
                flight: false,
                ..
            }
        )));
    assert_eq!(
        capture
            .observation
            .public_map
            .nodes
            .iter()
            .filter(|node| (node.x, node.y) == (0, 15))
            .count(),
        1
    );
    assert_eq!(
        at(&capture.observation.public_map, 2, 14).edges[0].destination_y,
        15
    );
    let source = run_map_comparison_topology_v1(&session);
    assert_eq!(
        source
            .nodes
            .iter()
            .find(|node| (node.x, node.y) == (2, 14))
            .unwrap()
            .edges[0]
            .destination_y,
        16
    );
    assert_eq!(
        source
            .nodes
            .iter()
            .filter(|node| (node.x, node.y) == (0, 15))
            .count(),
        1
    );
    session.run_state.map.current_x = 0;
    session.run_state.map.current_y = 15;
    assert_eq!(
        planner_public_map_v1(&session)
            .nodes
            .iter()
            .filter(|node| (node.x, node.y) == (0, 15))
            .count(),
        1
    );
}

#[test]
fn special_ending_map_keeps_its_own_boss_and_does_not_invent_a_map_edge_to_victory() {
    let mut session = RunControlSession::new(Default::default());
    session.run_state.act_num = 4;
    session.run_state.map = MapState::new(crate::state::map::generator::generate_ending_map());
    session.engine_state = EngineState::MapNavigation;
    let public = planner_public_map_v1(&session);
    assert_eq!(
        public
            .nodes
            .iter()
            .map(|node| (node.x, node.y))
            .collect::<Vec<_>>(),
        vec![(3, 0), (3, 1), (3, 2), (3, 3)]
    );
    assert_eq!(at(&public, 3, 3).room, PlannerMapRoom::Boss);
    assert!(at(&public, 3, 3).edges.is_empty());
    session.run_state.map.current_x = 3;
    session.run_state.map.current_y = 2;
    let capture =
        capture_planner_boundary_yield_v1(&session, PlannerBoundaryYieldKindV1::CallbackStop)
            .unwrap()
            .visits
            .remove(0);
    assert!(capture
        .legal_candidate_set
        .candidates
        .iter()
        .any(|candidate| matches!(
            candidate.action,
            PlannerAction::ChooseRouteNode {
                x: 3,
                y: 3,
                flight: false,
                ..
            }
        )));
    assert_eq!(
        at(&capture.observation.public_map, 3, 2).edges[0],
        PlannerPublicMapEdge {
            destination_x: 3,
            destination_y: 3
        }
    );
    session.run_state.map.current_y = 4;
    let terminal = planner_public_map_v1(&session);
    assert_eq!(at(&terminal, 3, 4).room, PlannerMapRoom::Victory);
    assert!(at(&terminal, 3, 3).edges.is_empty());
    assert!(!terminal.nodes.iter().any(|node| node.y == 15));
}

#[test]
fn nonstandard_or_unrelated_dangling_edges_are_not_guessed_into_boss_connections() {
    let mut session = standard_end();
    session.run_state.map.graph[13][1]
        .edges
        .insert(MapEdge::new(1, 13, 6, 22));
    assert!(at(&planner_public_map_v1(&session), 1, 13)
        .edges
        .contains(&PlannerPublicMapEdge {
            destination_x: 6,
            destination_y: 22
        }));
    session.run_state.map.graph.pop(); // Not a complete 15-row ordinary map.
    assert!(!planner_public_map_v1(&session)
        .nodes
        .iter()
        .any(|node| (node.x, node.y) == (0, 15)));
}

#[test]
fn generated_maps_have_closed_actor_graphs_without_changing_generation_or_rng() {
    for act in 1..=3 {
        for seed in [1, 17, 808] {
            let mut session = RunControlSession::new(Default::default());
            let (graph, _) = crate::state::map::generator::generate_map_for_act(seed, act, false);
            session.run_state.act_num = act;
            session.run_state.map = MapState::new(graph);
            let before = serde_json::to_value((
                &session.run_state.map,
                &session.run_state.map_rng,
                &session.run_state.rng_pool,
            ))
            .unwrap();
            let public = planner_public_map_v1(&session);
            let coordinates: BTreeSet<_> =
                public.nodes.iter().map(|node| (node.x, node.y)).collect();
            assert_eq!(coordinates.len(), public.nodes.len());
            assert!(coordinates.contains(&(0, 15)));
            assert!(!coordinates.contains(&(3, 16)));
            for node in &public.nodes {
                assert!(node
                    .edges
                    .iter()
                    .all(|edge| coordinates.contains(&(edge.destination_x, edge.destination_y))));
                if node.y == 14 {
                    assert_eq!(
                        node.edges,
                        vec![PlannerPublicMapEdge {
                            destination_x: 0,
                            destination_y: 15
                        }]
                    );
                }
            }
            assert_eq!(
                serde_json::to_value((
                    &session.run_state.map,
                    &session.run_state.map_rng,
                    &session.run_state.rng_pool
                ))
                .unwrap(),
                before
            );
        }
    }
}
