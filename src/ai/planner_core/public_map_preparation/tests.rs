use super::*;
use crate::ai::planner_core::PlannerPublicMapEdge;

fn node(x: i32, y: i32, room: PlannerMapRoom, next: &[(i32, i32)]) -> PlannerPublicMapNode {
    PlannerPublicMapNode {
        x,
        y,
        room,
        has_emerald_key: false,
        edges: next
            .iter()
            .map(|&(destination_x, destination_y)| PlannerPublicMapEdge {
                destination_x,
                destination_y,
            })
            .collect(),
    }
}

fn map(nodes: Vec<PlannerPublicMapNode>) -> PlannerPublicMap {
    PlannerPublicMap {
        current_x: 1,
        current_y: 0,
        boss: None,
        nodes,
    }
}

#[test]
fn preparation_keeps_mutually_exclusive_rest_and_shop_on_separate_paths() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node(1, 0, Monster, &[(0, 1), (2, 1)]),
        node(0, 1, Rest, &[(1, 2)]),
        node(2, 1, Shop, &[(1, 2)]),
        node(1, 2, Elite, &[(1, 3)]),
        node(1, 3, Rest, &[]),
    ]);
    let paths = public_map_preparation_paths(&map, 1, 0).unwrap();
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|path| path.has_rest != path.has_shop));
    let summary = summarize_public_map_preparation(&map, &[(1, 0)], 0).unwrap();
    assert_eq!(
        summary.elite_steps_by_preparation,
        [None, Some(2), Some(2), None]
    );
    assert_eq!(summary.first_unresolved_end_steps, None);
}

#[test]
fn preparation_retains_direct_and_prepared_prefixes_to_one_elite() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node(1, 0, Monster, &[(0, 1), (2, 1)]),
        node(0, 1, Rest, &[(1, 2)]),
        node(2, 1, Monster, &[(1, 2)]),
        node(1, 2, Elite, &[]),
    ]);
    assert_eq!(
        summarize_public_map_preparation(&map, &[(1, 0)], 0)
            .unwrap()
            .elite_steps_by_preparation,
        [Some(2), Some(2), None, None]
    );
    let mut reordered = map.clone();
    reordered.nodes.reverse();
    for node in &mut reordered.nodes {
        node.edges.reverse();
    }
    assert_eq!(
        public_map_preparation_paths(&map, 1, 0),
        public_map_preparation_paths(&reordered, 1, 0)
    );
}

#[test]
fn preparation_distinguishes_known_bypass_from_unresolved_leaves() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node(1, 0, QuestionMark, &[(0, 1), (1, 1), (2, 1), (3, 1)]),
        node(0, 1, Boss, &[(0, 2)]),
        node(0, 2, Elite, &[]),
        node(1, 1, Monster, &[]),
        node(3, 1, Elite, &[]),
    ]);
    let paths = public_map_preparation_paths(&map, 1, 0).unwrap();
    assert_eq!(paths.len(), 4);
    assert!(paths.iter().all(|path| path.nodes.len() == 2));
    let summary = summarize_public_map_preparation(&map, &[(1, 0)], 0).unwrap();
    assert_eq!(
        summary.elite_steps_by_preparation,
        [Some(1), None, None, None]
    );
    assert_eq!(summary.first_known_elite_free_end_steps, Some(1));
    assert_eq!(summary.first_unresolved_end_steps, Some(1));
}

#[test]
fn preparation_excludes_current_room_and_unconnected_nodes() {
    use PlannerMapRoom::*;
    let map = map(vec![
        node(1, 0, Shop, &[(1, 1)]),
        node(1, 1, Elite, &[]),
        node(2, 1, Rest, &[(2, 2)]),
        node(2, 2, Boss, &[]),
    ]);
    let starts = public_map_preparation_starts_after_current(&map).unwrap();
    assert_eq!(starts, vec![(1, 1)]);
    assert_eq!(
        summarize_public_map_preparation(&map, &starts, 1)
            .unwrap()
            .elite_steps_by_preparation,
        [Some(1), None, None, None]
    );
    let mut absent = map.clone();
    absent.current_y = 8;
    assert_eq!(public_map_preparation_starts_after_current(&absent), None);
    absent.current_y = -1;
    assert_eq!(
        public_map_preparation_starts_after_current(&absent),
        Some(vec![(1, 0)])
    );
    absent.current_y = 1;
    assert_eq!(
        public_map_preparation_starts_after_current(&absent),
        Some(vec![])
    );
}

#[test]
fn preparation_rejects_duplicate_or_nonadvancing_topology() {
    use PlannerMapRoom::*;
    for nodes in [
        vec![node(1, 0, Monster, &[(1, 0)])],
        vec![node(1, 0, Monster, &[(1, 1), (1, 1)])],
        vec![node(1, 0, Monster, &[]), node(1, 0, Rest, &[])],
    ] {
        assert!(public_map_preparation_paths(&map(nodes), 1, 0).is_err());
    }
}

#[test]
fn preparation_does_not_enumerate_exponentially_many_complete_paths() {
    use PlannerMapRoom::*;
    let nodes = (0..30)
        .flat_map(|y| {
            (0..2).map(move |x| {
                let next = [(0, y + 1), (1, y + 1)];
                node(
                    x,
                    y,
                    if y == 29 {
                        Elite
                    } else if x == 0 && y == 1 {
                        Rest
                    } else {
                        Monster
                    },
                    if y == 29 { &[] } else { &next },
                )
            })
        })
        .collect();
    let map = map(nodes);
    let paths = public_map_preparation_paths(&map, 0, 0).unwrap();
    assert_eq!(paths.len(), 4);
    assert!(paths.iter().all(|path| path.nodes.len() == 30));
}
