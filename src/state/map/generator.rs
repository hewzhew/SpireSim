//! Map generation — bit-perfect replica of `MapGenerator` + `RoomTypeAssigner`.
//!
//! Generates the 15×7 map grid with path creation, edge-crossing elimination,
//! room type assignment, and emerald key placement. Supports Acts 1–4.
//!
//! Reference: `sts_map_oracle` by Rusty 0ne (structural algorithm),
//! verified against decompiled Java (`MapGenerator.java`, `RoomTypeAssigner.java`).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::Serialize;

use super::node::{Map, MapEdge, MapRoomNode, Point, RoomType};
use crate::runtime::rng::StsRng;
use RoomType::*;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Seed offsets per act, from each dungeon's constructor:
///   Exordium: `seed + actNum`      (actNum=1 → +1)
///   TheCity:  `seed + actNum*100`   (actNum=2 → +200)
///   TheBeyond:`seed + actNum*200`   (actNum=3 → +600)
const ACT_OFFSETS: [u64; 3] = [1, 200, 600];

const MAP_HEIGHT: i32 = 15;
const MAP_WIDTH: i32 = 7;
const PATH_DENSITY: i32 = 6;

// ─── Public API ──────────────────────────────────────────────────────────────

/// Generate maps for all 3 acts.
pub fn generate_maps(
    seed: u64,
    map_height: i32,
    map_width: i32,
    path_density: i32,
    is_ascension_zero: bool,
) -> Vec<Map> {
    ACT_OFFSETS
        .iter()
        .map(|offset| {
            let mut rng = StsRng::new_named(seed.wrapping_add(*offset), "run.map");
            generate_single_map(
                map_height,
                map_width,
                path_density,
                is_ascension_zero,
                &mut rng,
            )
        })
        .collect()
}

/// Generate a single map for a specific act (1, 2, or 3).
/// Returns (map, consumed_rng) — the RNG has been consumed by path generation
/// and room assignment. Pass it to `set_emerald_elite()` for Java parity.
pub fn generate_map_for_act(seed: u64, act: u8, is_ascension_zero: bool) -> (Map, StsRng) {
    let offset = ACT_OFFSETS[(act - 1).min(2) as usize];
    let mut rng = StsRng::new_named(seed.wrapping_add(offset), "run.map");
    let map = generate_single_map(
        MAP_HEIGHT,
        MAP_WIDTH,
        PATH_DENSITY,
        is_ascension_zero,
        &mut rng,
    );
    (map, rng)
}

/// Java: `AbstractDungeon.setEmeraldElite()`
/// Picks a random elite node on the map and marks it with `has_emerald_key = true`.
pub fn set_emerald_elite(map: &mut Map, rng: &mut StsRng) {
    let elite_nodes: Vec<(usize, usize)> = map
        .iter()
        .enumerate()
        .flat_map(|(y, row)| {
            row.iter()
                .enumerate()
                .filter(|(_, node)| node.class == Some(MonsterRoomElite))
                .map(move |(x, _)| (y, x))
        })
        .collect();

    if !elite_nodes.is_empty() {
        let idx = rng.random_range(0, (elite_nodes.len() - 1) as i32) as usize;
        let (y, x) = elite_nodes[idx];
        map[y][x].has_emerald_key = true;
    }
}

/// Java: `TheEnding.generateSpecialMap()`
/// Fixed 5-row linear map for Act 4: Rest → Shop → Elite → Boss → Victory.
pub const ENDING_ROOM_SEQUENCE: [RoomType; 5] = [
    RoomType::RestRoom,
    RoomType::ShopRoom,
    RoomType::MonsterRoomElite,
    RoomType::MonsterRoomBoss,
    RoomType::TrueVictoryRoom,
];

pub fn generate_ending_map() -> Map {
    let mut map: Map = (0..5)
        .map(|y| {
            (0..MAP_WIDTH)
                .map(|x| {
                    let mut node = MapRoomNode::new(x, y);
                    if x == 3 {
                        node.class = Some(ENDING_ROOM_SEQUENCE[y as usize]);
                    }
                    node
                })
                .collect()
        })
        .collect();

    // Linear path at x=3: row 0 → 1 → 2 → 3
    for y in 0..3 {
        let edge = MapEdge::new(3, y as i32, 3, (y + 1) as i32);
        map[y][3].edges.insert(edge);
        map[y + 1][3].parents.push(Point::new(3, y));
    }

    map
}

/// ASCII-art visualization of a map.
pub fn format_map(nodes: &Map) -> String {
    let mut s = String::new();
    for row_num in (0..nodes.len()).rev() {
        s.push_str(&format!("\n{: <6}", ""));
        for node in nodes[row_num].iter() {
            let (mut left, mut mid, mut right) = (" ", " ", " ");
            for edge in node.edges.iter() {
                match edge.dst_x.cmp(&node.x) {
                    Ordering::Less => left = r"\",
                    Ordering::Equal => mid = "|",
                    Ordering::Greater => right = "/",
                };
            }
            s.push_str(&format!("{}{}{}", left, mid, right));
        }
        s.push_str(&format!("\n{: <6}", row_num));
        for node in nodes[row_num].iter() {
            let symbol = if row_num == nodes.len() - 1 {
                // Top row: only show if reachable from below
                if nodes[row_num - 1]
                    .iter()
                    .any(|lower| lower.edges.iter().any(|e| e.dst_x == node.x))
                {
                    room_symbol(&node.class)
                } else {
                    " "
                }
            } else if !node.edges.is_empty() {
                room_symbol(&node.class)
            } else {
                " "
            };
            s.push_str(&format!(" {} ", symbol));
        }
    }
    s
}

/// Serialize a map to JSON (edges + nodes with rooms).
pub fn dump_map(map: &Map) -> String {
    let mut edges = vec![];
    let mut nodes = vec![];
    for row in map.iter() {
        for node in row.iter() {
            if node.class.is_some() {
                if !node.parents.is_empty() || !node.edges.is_empty() {
                    nodes.push(node);
                }
                edges.extend(&node.edges);
            }
        }
    }
    serde_json::to_string(&DumpMap { edges, nodes }).unwrap()
}

// ─── Internal: Map Generation ────────────────────────────────────────────────

/// Shared logic for both `generate_maps` and `generate_map_for_act`.
fn generate_single_map(
    height: i32,
    width: i32,
    path_density: i32,
    is_ascension_zero: bool,
    rng: &mut StsRng,
) -> Map {
    let mut map = generate_dungeon(height, width, path_density, rng);

    // Fixed row assignments (Java: AbstractDungeon.generateMap)
    map[0].iter_mut().for_each(|n| n.class = Some(MonsterRoom));
    map[8].iter_mut().for_each(|n| n.class = Some(TreasureRoom));
    let last = map.len() - 1;
    map[last].iter_mut().for_each(|n| n.class = Some(RestRoom));

    // Count nodes eligible for room assignment (excludes fixed rows)
    let count = map
        .iter()
        .flat_map(|row| row.iter())
        .filter(|n| {
            (!n.edges.is_empty() || (n.y as usize == map.len() - 1 && !n.parents.is_empty()))
                && n.y as usize != map.len() - 2
        })
        .count();

    let room_chances: HashMap<RoomType, f64> = [
        (ShopRoom, 0.05),
        (RestRoom, 0.12),
        (EventRoom, 0.22),
        (
            MonsterRoomElite,
            0.08 * if is_ascension_zero { 1.0 } else { 1.6 },
        ),
    ]
    .iter()
    .cloned()
    .collect();

    let room_list = generate_room_type(&room_chances, count);
    distribute_rooms_across_map(map, room_list, rng)
}

fn generate_dungeon(height: i32, width: i32, path_density: i32, rng: &mut StsRng) -> Map {
    let map = create_nodes(height, width);
    let map = create_paths(map, path_density, rng);
    filter_redundant_edges(map)
}

fn create_nodes(height: i32, width: i32) -> Map {
    (0..height)
        .map(|y| (0..width).map(|x| MapRoomNode::new(x, y)).collect())
        .collect()
}

// ─── Internal: Path Creation ─────────────────────────────────────────────────

fn create_paths(mut nodes: Map, path_density: i32, rng: &mut StsRng) -> Map {
    assert!(!nodes.is_empty() && !nodes[0].is_empty());
    let row_size = (nodes[0].len() - 1) as i32;
    let mut first_starting_node = -1i32;

    for i in 0..path_density {
        let mut starting_node = rng.random(row_size);
        if i == 0 {
            first_starting_node = starting_node;
        }
        while starting_node == first_starting_node && i == 1 {
            starting_node = rng.random(row_size);
        }
        let edge = MapEdge::new(starting_node, -1, starting_node, 0);
        nodes = create_path_recursive(nodes, &edge, rng);
    }
    nodes
}

fn create_path_recursive(mut nodes: Map, edge: &MapEdge, rng: &mut StsRng) -> Map {
    let current_node = &nodes[edge.dst_y as usize][edge.dst_x as usize];
    if edge.dst_y + 1 >= nodes.len() as i32 {
        let boss_edge = MapEdge::new(edge.dst_x, edge.dst_y, 3, edge.dst_y + 2);
        nodes[edge.dst_y as usize][edge.dst_x as usize]
            .edges
            .insert(boss_edge);
        return nodes;
    }

    let row_end = nodes[edge.dst_y as usize].len() as i32 - 1;
    let (min, max) = match edge.dst_x {
        0 => (0, 1),
        x if x == row_end => (-1, 0),
        _ => (-1, 1),
    };

    let mut new_x = edge.dst_x + map_rand_range(rng, min, max);
    let new_y = edge.dst_y + 1;
    let target_candidate = &nodes[new_y as usize][new_x as usize];
    let mut target_coord = Point::new(new_x as usize, new_y as usize);
    let current_coord = Point::new(current_node.x as usize, current_node.y as usize);

    // Ancestor gap enforcement
    const MIN_ANCESTOR_GAP: i32 = 3;
    const MAX_ANCESTOR_GAP: i32 = 5;

    for parent in target_candidate.parents.iter() {
        if &current_coord != parent {
            if let Some(ancestor) =
                get_common_ancestor(&nodes, parent, &current_coord, MAX_ANCESTOR_GAP)
            {
                let gap = new_y - ancestor.y as i32;
                if gap < MIN_ANCESTOR_GAP {
                    match target_coord.x.cmp(&(current_node.x as usize)) {
                        Ordering::Greater => {
                            new_x = edge.dst_x + map_rand_range(rng, -1, 0);
                            if new_x < 0 {
                                new_x = edge.dst_x;
                            }
                        }
                        Ordering::Equal => {
                            new_x = edge.dst_x + map_rand_range(rng, -1, 1);
                            if new_x > row_end {
                                new_x = edge.dst_x - 1;
                            } else if new_x < 0 {
                                new_x = edge.dst_x + 1;
                            }
                        }
                        Ordering::Less => {
                            new_x = edge.dst_x + map_rand_range(rng, 0, 1);
                            if new_x > row_end {
                                new_x = edge.dst_x;
                            }
                        }
                    }
                    target_coord = Point::new(new_x as usize, new_y as usize);
                    continue;
                }
            }
        }
    }

    // Eliminate edge crossings
    if edge.dst_x != 0 {
        let left_node = &nodes[edge.dst_y as usize][(edge.dst_x - 1) as usize];
        if let Some(right_edge) = left_node.edges.iter().last() {
            if right_edge.dst_x > new_x {
                new_x = right_edge.dst_x;
            }
        }
    }
    if edge.dst_x < row_end {
        let right_node = &nodes[edge.dst_y as usize][(edge.dst_x + 1) as usize];
        if let Some(left_edge) = right_node.edges.iter().next() {
            if left_edge.dst_x < new_x {
                new_x = left_edge.dst_x;
            }
        }
    }

    // Insert edge and parent link
    let new_edge = MapEdge::new(edge.dst_x, edge.dst_y, new_x, new_y);
    let next_edge = new_edge.clone();
    nodes[edge.dst_y as usize][edge.dst_x as usize]
        .edges
        .insert(new_edge);
    nodes[new_y as usize][new_x as usize]
        .parents
        .push(Point::new(edge.dst_x as usize, edge.dst_y as usize));

    create_path_recursive(nodes, &next_edge, rng)
}

/// Trim edges with common destination on the first floor.
fn filter_redundant_edges(mut map: Map) -> Map {
    let mut seen: HashSet<Point> = HashSet::new();
    let mut to_remove: Vec<(usize, MapEdge)> = vec![];

    for (i, node) in map[0].iter().enumerate() {
        for edge in node.edges.iter() {
            let dst = Point::new(edge.dst_x as usize, edge.dst_y as usize);
            if seen.contains(&dst) {
                to_remove.push((i, edge.clone()));
            }
            seen.insert(dst);
        }
    }
    for (idx, edge) in &to_remove {
        map[0][*idx].edges.remove(edge);
    }
    map
}

fn get_common_ancestor<'a>(
    map: &'a Map,
    node1: &'a Point,
    node2: &'a Point,
    max_depth: i32,
) -> Option<&'a Point> {
    assert!(node1.y == node2.y);
    assert!(node1.x != node2.x);

    // Bug from game's original codebase: compares node1.x < node2.y (not node2.x).
    // Retained for bit-perfect parity.
    let (mut l_node, mut r_node) = if node1.x < node2.y {
        (node1, node2)
    } else {
        (node2, node1)
    };

    let mut current_y = node1.y as i32;
    while current_y >= 0 && current_y >= node1.y as i32 - max_depth {
        if l_node.parents(map).is_empty() || r_node.parents(map).is_empty() {
            return None;
        }
        l_node = l_node.parents(map).iter().max_by_key(|p| p.x).unwrap();
        r_node = r_node.parents(map).iter().min_by_key(|p| p.x).unwrap();
        if l_node == r_node {
            return Some(l_node);
        }
        current_y -= 1;
    }
    None
}

// ─── Internal: Room Assignment ───────────────────────────────────────────────

fn generate_room_type(
    room_chances: &HashMap<RoomType, f64>,
    available_count: usize,
) -> Vec<RoomType> {
    let mut acc = vec![];
    for t in &[ShopRoom, RestRoom, MonsterRoomElite, EventRoom] {
        let rooms = (room_chances[t] * available_count as f64).round() as usize;
        for _ in 0..rooms {
            acc.push(*t);
        }
    }
    acc
}

fn distribute_rooms_across_map(
    mut map: Map,
    mut room_list: Vec<RoomType>,
    rng: &mut StsRng,
) -> Map {
    let node_count = count_connected_nodes(&map);
    room_list.resize(std::cmp::max(room_list.len(), node_count), MonsterRoom);
    shuffle(&mut room_list, rng);
    map = assign_rooms_to_nodes(map, &mut room_list);
    last_minute_node_checker(map)
}

fn count_connected_nodes(map: &Map) -> usize {
    map.iter()
        .flat_map(|row| row.iter())
        .filter(|node| !node.edges.is_empty() && node.class.is_none())
        .count()
}

fn assign_rooms_to_nodes(mut map: Map, room_list: &mut Vec<RoomType>) -> Map {
    let height = map.len();
    let width = map[0].len();
    for y in 0..height {
        for x in 0..width {
            let node = &map[y][x];
            if !node.edges.is_empty() && node.class.is_none() {
                if let Some(room) = get_next_room_type(&map, node, room_list) {
                    let pos = room_list.iter().position(|&r| r == room).unwrap();
                    room_list.remove(pos);
                    map[y][x].class = Some(room);
                }
            }
        }
    }
    map
}

fn get_next_room_type(map: &Map, n: &MapRoomNode, room_list: &[RoomType]) -> Option<RoomType> {
    let parents = &n.parents;
    let siblings = get_siblings(map, n);
    for room in room_list.iter() {
        if rule_assignable_to_row(n, room)
            && (!rule_parent_matches(map, parents, room) && !rule_sibling_matches(&siblings, room))
        {
            return Some(*room);
        }
        // Unreachable in practice, but matches Java
        if rule_assignable_to_row(n, room) && n.y == 0 {
            return Some(*room);
        }
    }
    None
}

fn last_minute_node_checker(mut map: Map) -> Map {
    for row in map.iter_mut() {
        for node in row.iter_mut() {
            if !node.edges.is_empty() && node.class.is_none() {
                node.class = Some(MonsterRoom);
            }
        }
    }
    map
}

// ─── Internal: Room Assignment Rules ─────────────────────────────────────────

fn rule_assignable_to_row(n: &MapRoomNode, room: &RoomType) -> bool {
    if n.y <= 4 && matches!(room, RestRoom | MonsterRoomElite) {
        return false;
    }
    if n.y >= 13 && room == &RestRoom {
        return false;
    }
    true
}

fn rule_parent_matches(map: &Map, parents: &[Point], room: &RoomType) -> bool {
    matches!(room, RestRoom | TreasureRoom | ShopRoom | MonsterRoomElite)
        && parents
            .iter()
            .flat_map(|p| p.node(map).class)
            .any(|c| c == *room)
}

fn rule_sibling_matches(sibs: &[&MapRoomNode], room: &RoomType) -> bool {
    matches!(
        room,
        RestRoom | TreasureRoom | ShopRoom | MonsterRoomElite | MonsterRoom | EventRoom
    ) && sibs.iter().flat_map(|s| s.class).any(|c| c == *room)
}

fn get_siblings<'a>(map: &'a Map, node: &'a MapRoomNode) -> Vec<&'a MapRoomNode> {
    node.parents
        .iter()
        .flat_map(|p| p.node(map).edges.iter())
        .map(|edge| &map[edge.dst_y as usize][edge.dst_x as usize])
        .filter(|sib| *sib != node)
        .collect()
}

// ─── Internal: Shuffle ───────────────────────────────────────────────────────

/// Map-specific shuffle matching `Collections.shuffle(list, rng.random)`.
///
/// In Java, `Collections.shuffle` calls `RandomXS128.nextInt(int)` directly
/// (which delegates to `nextLong(n)` — xorshift128+ rejection sampling),
/// bypassing the `com.megacrit.cardcrawl.random.Random` counter wrapper.
///
/// These direct XS128 calls change the state but neither the Java wrapper
/// counter nor its wrapper-call ledger.
fn shuffle<T>(list: &mut [T], rng: &mut StsRng) {
    for i in (2..=list.len()).rev() {
        let j = rng.direct_xs128_next_int(i as i32) as usize;
        list.swap(j, i - 1);
    }
}

// MapGenerator.randRange uses random(max - min) + min, not random(min, max).
fn map_rand_range(rng: &mut StsRng, min: i32, max: i32) -> i32 {
    rng.random(max - min) + min
}

// ─── Internal: Serialization ─────────────────────────────────────────────────

fn room_symbol(t: &Option<RoomType>) -> &str {
    match t {
        None => "*",
        Some(r) => match r {
            RestRoom => "R",
            ShopRoom => "$",
            MonsterRoom => "M",
            EventRoom => "?",
            MonsterRoomElite => "E",
            MonsterRoomBoss => "B",
            TreasureRoom => "T",
            TrueVictoryRoom => "V",
        },
    }
}

#[derive(Serialize, Debug)]
struct DumpMap<'a> {
    edges: Vec<&'a MapEdge>,
    nodes: Vec<&'a MapRoomNode>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn act2_map_rng_matches_the_original_java_transition_without_a_run_replay() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/java_act2_rng.json")).unwrap();
        let input = &oracle["input"];
        let trace = begin_sts_rng_trace().unwrap();
        let (mut map, mut rng) = generate_map_for_act(
            input["seed"].as_str().unwrap().parse().unwrap(),
            input["act"].as_u64().unwrap() as u8,
            input["ascension_zero"].as_bool().unwrap(),
        );
        if input["place_emerald"].as_bool().unwrap() {
            set_emerald_elite(&mut map, &mut rng);
        }
        let actual = trace.finish().unwrap();
        let expected = oracle["calls"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(actual.owner, expected["owner"].as_str());
            assert_eq!(expected["owner_ordinal"], index);
            let operation = match actual.call {
                StsRngCall::RandomInt { range, result } => serde_json::json!({
                    "method": "random_int", "range": range, "result": result,
                }),
                StsRngCall::RandomIntRange { start, end, result } => serde_json::json!({
                    "method": "random_int_range", "start": start, "end": end, "result": result,
                }),
                _ => panic!("unexpected map wrapper call at {index}"),
            };
            assert_eq!(operation, expected["operation"], "map call {index}");
        }
        let terminal = &oracle["terminal_rng"];
        assert_eq!(rng.counter, terminal["counter"].as_u64().unwrap() as u32);
        assert_eq!(format!("{:016X}", rng.seed0), terminal["state_hex"][0]);
        assert_eq!(format!("{:016X}", rng.seed1), terminal["state_hex"][1]);
    }

    #[test]
    fn map_path_range_records_the_original_single_bound_call() {
        use crate::runtime::rng::{begin_sts_rng_trace, StsRngCall};
        let mut rng = StsRng::new(7);
        let trace = begin_sts_rng_trace().unwrap();
        let result = map_rand_range(&mut rng, -1, 1);
        let calls = trace.finish().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].call,
            StsRngCall::RandomInt {
                range: 2,
                result: result + 1
            }
        );
    }

    #[test]
    fn map_shuffle_advances_xs128_without_sts_wrapper_calls() {
        use crate::runtime::rng::begin_sts_rng_trace;
        let mut direct = StsRng::new(7);
        let mut wrapped = direct.clone();
        let mut actual = [0, 1, 2, 3, 4];
        let mut expected = actual;
        for i in (2..=expected.len()).rev() {
            let j = wrapped.random((i - 1) as i32) as usize;
            expected.swap(j, i - 1);
        }
        let trace = begin_sts_rng_trace().unwrap();
        shuffle(&mut actual, &mut direct);
        assert!(trace.finish().unwrap().is_empty());
        assert_eq!(actual, expected);
        assert_eq!(direct.counter, 0);
        assert_eq!(direct.seed0, wrapped.seed0);
        assert_eq!(direct.seed1, wrapped.seed1);
    }

    #[test]
    fn final_path_node_keeps_the_original_game_boss_anchor_edge() {
        let row = (0..7).map(|x| MapRoomNode::new(x, 0)).collect::<Vec<_>>();
        let entry = MapEdge::new(2, -1, 2, 0);
        let map = create_path_recursive(vec![row], &entry, &mut StsRng::new(7));

        assert!(map[0][2].edges.contains(&MapEdge::new(2, 0, 3, 2)));
    }
}
